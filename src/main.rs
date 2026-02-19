use anyhow::Result;
use eframe::egui;
use ndarray::{s, Array3};
use nifti::{InMemNiftiVolume, IntoNdArray, NiftiHeader, NiftiObject, ReaderOptions};
use std::io::Read;

#[cfg(target_arch = "wasm32")]
use flate2::read::GzDecoder;
#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::io::Cursor;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{Event, FileReader, HtmlCanvasElement, HtmlInputElement};

/// Build the 3x3 direction part of the affine from sform, qform, or pixdims.
fn get_affine_3x3(hdr: &NiftiHeader) -> [[f32; 3]; 3] {
    if hdr.sform_code > 0 {
        [
            [hdr.srow_x[0], hdr.srow_x[1], hdr.srow_x[2]],
            [hdr.srow_y[0], hdr.srow_y[1], hdr.srow_y[2]],
            [hdr.srow_z[0], hdr.srow_z[1], hdr.srow_z[2]],
        ]
    } else if hdr.qform_code > 0 {
        let b = hdr.quatern_b as f64;
        let c = hdr.quatern_c as f64;
        let d = hdr.quatern_d as f64;
        let a = (1.0 - b * b - c * c - d * d).max(0.0).sqrt();
        let r = [
            [
                (a * a + b * b - c * c - d * d) as f32,
                (2.0 * (b * c - a * d)) as f32,
                (2.0 * (b * d + a * c)) as f32,
            ],
            [
                (2.0 * (b * c + a * d)) as f32,
                (a * a + c * c - b * b - d * d) as f32,
                (2.0 * (c * d - a * b)) as f32,
            ],
            [
                (2.0 * (b * d - a * c)) as f32,
                (2.0 * (c * d + a * b)) as f32,
                (a * a + d * d - b * b - c * c) as f32,
            ],
        ];
        let qfac: f32 = if hdr.pixdim[0] < 0.0 { -1.0 } else { 1.0 };
        let (px, py, pz) = (hdr.pixdim[1], hdr.pixdim[2], hdr.pixdim[3] * qfac);
        [
            [r[0][0] * px, r[0][1] * py, r[0][2] * pz],
            [r[1][0] * px, r[1][1] * py, r[1][2] * pz],
            [r[2][0] * px, r[2][1] * py, r[2][2] * pz],
        ]
    } else {
        // No orientation info – assume identity with voxel sizes
        [
            [hdr.pixdim[1], 0.0, 0.0],
            [0.0, hdr.pixdim[2], 0.0],
            [0.0, 0.0, hdr.pixdim[3]],
        ]
    }
}

/// Extract the translation (origin at voxel 0,0,0) from the header affine.
fn get_translation(hdr: &NiftiHeader) -> [f32; 3] {
    if hdr.sform_code > 0 {
        [hdr.srow_x[3], hdr.srow_y[3], hdr.srow_z[3]]
    } else if hdr.qform_code > 0 {
        [hdr.quatern_x, hdr.quatern_y, hdr.quatern_z]
    } else {
        [0.0, 0.0, 0.0]
    }
}

/// Reorient a volume to RAS (Right–Anterior–Superior) using the header affine.
/// Returns the reoriented volume, voxel spacings in RAS order, and the RAS
/// coordinate (in mm) of voxel (0,0,0) in the reoriented volume.
fn reorient_to_ras(volume: Array3<f32>, hdr: &NiftiHeader) -> (Array3<f32>, [f32; 3], [f32; 3]) {
    let affine = get_affine_3x3(hdr);
    let translation = get_translation(hdr);
    let orig_shape = [volume.shape()[0], volume.shape()[1], volume.shape()[2]];

    // For each voxel axis (column), find which world axis (row) dominates.
    let mut voxel_to_world = [0usize; 3];
    let mut voxel_flip = [false; 3];
    for col in 0..3 {
        let mut best_row = 0;
        let mut best_val = 0.0f32;
        for row in 0..3 {
            let v = affine[row][col].abs();
            if v > best_val {
                best_val = v;
                best_row = row;
            }
        }
        voxel_to_world[col] = best_row;
        voxel_flip[col] = affine[best_row][col] < 0.0;
    }

    // Invert the mapping: world_to_voxel[world_axis] = voxel_axis
    let mut world_to_voxel = [0usize; 3];
    for col in 0..3 {
        world_to_voxel[voxel_to_world[col]] = col;
    }

    // Voxel spacings reordered to RAS
    let orig_spacing = [hdr.pixdim[1], hdr.pixdim[2], hdr.pixdim[3]];
    let ras_spacing = [
        orig_spacing[world_to_voxel[0]],
        orig_spacing[world_to_voxel[1]],
        orig_spacing[world_to_voxel[2]],
    ];

    // Permute so output axis i = world axis i (R, A, S)
    let vol = volume.permuted_axes(world_to_voxel).to_owned();

    // Flip axes that run in the negative direction to make them positive (RAS)
    let needs_flip = [
        voxel_flip[world_to_voxel[0]],
        voxel_flip[world_to_voxel[1]],
        voxel_flip[world_to_voxel[2]],
    ];
    let vol = if needs_flip[0] {
        vol.slice(s![..;-1, .., ..]).to_owned()
    } else {
        vol
    };
    let vol = if needs_flip[1] {
        vol.slice(s![.., ..;-1, ..]).to_owned()
    } else {
        vol
    };
    let vol = if needs_flip[2] {
        vol.slice(s![.., .., ..;-1]).to_owned()
    } else {
        vol
    };
    // Compute the RAS coordinate at reoriented voxel (0,0,0).
    // After permutation + flip, new voxel 0 along axis a came from
    // original axis world_to_voxel[a] at index (shape-1 if flipped, 0 otherwise).
    let mut orig_ijk = [0.0f32; 3];
    for a in 0..3 {
        let v = world_to_voxel[a];
        orig_ijk[v] = if needs_flip[a] {
            (orig_shape[v] - 1) as f32
        } else {
            0.0
        };
    }
    let ras_origin = [
        affine[0][0] * orig_ijk[0]
            + affine[0][1] * orig_ijk[1]
            + affine[0][2] * orig_ijk[2]
            + translation[0],
        affine[1][0] * orig_ijk[0]
            + affine[1][1] * orig_ijk[1]
            + affine[1][2] * orig_ijk[2]
            + translation[1],
        affine[2][0] * orig_ijk[0]
            + affine[2][1] * orig_ijk[1]
            + affine[2][2] * orig_ijk[2]
            + translation[2],
    ];

    (vol, ras_spacing, ras_origin)
}

struct NiftiViewer {
    /// Volume in RAS orientation: axis 0 = L→R, axis 1 = P→A, axis 2 = I→S
    volume: Option<Array3<f32>>,
    /// Voxel spacings in mm for RAS axes [R, A, S]
    voxdim: [f32; 3],
    /// RAS coordinate (mm) at reoriented voxel (0,0,0)
    ras_origin: [f32; 3],
    slice_x: usize,
    slice_y: usize,
    slice_z: usize,
    scroll_accum: [f32; 3],
    error_msg: Option<String>,
}

impl NiftiViewer {
    fn new() -> Self {
        Self {
            volume: None,
            voxdim: [1.0; 3],
            ras_origin: [0.0; 3],
            slice_x: 0,
            slice_y: 0,
            slice_z: 0,
            scroll_accum: [0.0; 3],
            error_msg: None,
        }
    }

    fn load_from_path(&mut self, path: &str) {
        match load_nifti(path) {
            Ok((volume, voxdim, ras_origin)) => {
                self.slice_x = volume.shape()[0] / 2;
                self.slice_y = volume.shape()[1] / 2;
                self.slice_z = volume.shape()[2] / 2;
                self.volume = Some(volume);
                self.voxdim = voxdim;
                self.ras_origin = ras_origin;
                self.scroll_accum = [0.0; 3];
                self.error_msg = None;
            }
            Err(e) => {
                self.error_msg = Some(format!("Failed to load: {e}"));
            }
        }
    }

    fn load_from_bytes(&mut self, bytes: &[u8]) {
        match load_nifti_bytes(bytes) {
            Ok((volume, voxdim, ras_origin)) => {
                self.slice_x = volume.shape()[0] / 2;
                self.slice_y = volume.shape()[1] / 2;
                self.slice_z = volume.shape()[2] / 2;
                self.volume = Some(volume);
                self.voxdim = voxdim;
                self.ras_origin = ras_origin;
                self.scroll_accum = [0.0; 3];
                self.error_msg = None;
            }
            Err(e) => {
                self.error_msg = Some(format!("Failed to load: {e}"));
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn open_web_file_dialog(&mut self, ctx: &egui::Context) {
        let window = web_sys::window().expect("window not available");
        let document = window.document().expect("document not available");
        let input: HtmlInputElement = document
            .create_element("input")
            .expect("create input")
            .dyn_into()
            .expect("input element");
        input.set_type("file");
        input.set_accept(".nii,.nii.gz");

        let input_clone = input.clone();
        let ctx_clone = ctx.clone();
        let onload = Closure::wrap(Box::new(move |event: Event| {
            let target = event.target().expect("no event target");
            let reader: FileReader = target.dyn_into().expect("file reader");
            if let Ok(result) = reader.result() {
                let array = Uint8Array::new(&result);
                let mut bytes = vec![0u8; array.length() as usize];
                array.copy_to(&mut bytes);
                set_pending_bytes(bytes);
                ctx_clone.request_repaint();
            }
        }) as Box<dyn FnMut(_)>);

        let reader = FileReader::new().expect("file reader");
        reader.set_onloadend(Some(onload.as_ref().unchecked_ref()));
        onload.forget();

        let reader_clone = reader.clone();
        let onchange = Closure::wrap(Box::new(move |_event: Event| {
            if let Some(files) = input_clone.files() {
                if let Some(file) = files.get(0) {
                    let _ = reader_clone.read_as_array_buffer(&file);
                }
            }
        }) as Box<dyn FnMut(_)>);
        input.set_onchange(Some(onchange.as_ref().unchecked_ref()));
        onchange.forget();

        input.click();
    }

    /// Convert a voxel index to display mm along the given axis.
    /// Axes 0 (R) and 1 (A) are negated to match LPS display convention
    /// used by 3D Slicer (L = −R, P = −A, S = S).
    fn voxel_to_mm(&self, axis: usize, idx: usize) -> f32 {
        let ras = self.ras_origin[axis] + idx as f32 * self.voxdim[axis];
        if axis < 2 {
            -ras
        } else {
            ras
        }
    }

    /// Convert a display mm value back to the nearest voxel index.
    fn mm_to_voxel(&self, axis: usize, mm: f32) -> usize {
        let ras_mm = if axis < 2 { -mm } else { mm };
        let n = self.volume.as_ref().unwrap().shape()[axis];
        let idx = ((ras_mm - self.ras_origin[axis]) / self.voxdim[axis]).round() as isize;
        idx.clamp(0, (n - 1) as isize) as usize
    }

    fn get_slices(
        &self,
    ) -> Option<(
        ndarray::Array2<f32>,
        ndarray::Array2<f32>,
        ndarray::Array2<f32>,
    )> {
        let vol = self.volume.as_ref()?;
        // Volume is RAS: axis 0 = L→R, axis 1 = P→A, axis 2 = I→S
        let sagittal = vol.slice(s![self.slice_x, .., ..]).to_owned(); // (P→A, I→S)
        let coronal = vol.slice(s![.., self.slice_y, ..]).to_owned(); // (L→R, I→S)
        let axial = vol.slice(s![.., .., self.slice_z]).to_owned(); // (L→R, P→A)
        Some((sagittal, coronal, axial))
    }

    /// Prepare a 2D RAS slice for radiological display.
    ///
    /// All three standard views (after RAS reorientation) need the same
    /// transform: transpose then reverse both axes.  This puts the
    /// superior / anterior direction at the top of the image and uses
    /// radiological left–right convention.
    fn array2_to_color_image(slice: &ndarray::Array2<f32>) -> egui::ColorImage {
        let slice = slice.t();
        let slice = slice.slice(s![..;-1, ..;-1]);

        let (h, w) = slice.dim();
        let mut pixels = Vec::with_capacity(h * w);

        let min = slice.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = slice.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        for y in 0..h {
            for x in 0..w {
                let mut val = slice[[y, x]];
                val = ((val - min) / (max - min)).clamp(0.0, 1.0);
                let gray = (val * 255.0) as u8;
                pixels.push(egui::Color32::from_gray(gray));
            }
        }

        egui::ColorImage {
            size: [w, h],
            pixels,
            source_size: egui::Vec2::new(w as f32, h as f32),
        }
    }
    /// Return the physical display size for a slice, preserving aspect ratio
    /// while fitting within the given bounding box. Uses voxel counts × voxel
    /// spacing to compute the true physical aspect ratio.
    fn fit_size(
        nvox_w: usize,
        nvox_h: usize,
        vox_w: f32,
        vox_h: f32,
        max_w: f32,
        max_h: f32,
    ) -> egui::Vec2 {
        let phys_w = nvox_w as f32 * vox_w;
        let phys_h = nvox_h as f32 * vox_h;
        let scale = (max_w / phys_w).min(max_h / phys_h);
        egui::vec2(phys_w * scale, phys_h * scale)
    }
}

impl eframe::App for NiftiViewer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Load NIfTI…").clicked() {
                        ui.close();
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("NIfTI", &["nii", "gz"])
                                .pick_file()
                            {
                                self.load_from_path(&path.to_string_lossy());
                            }
                        }
                        #[cfg(target_arch = "wasm32")]
                        {
                            self.open_web_file_dialog(ctx);
                        }
                    }
                });
            });
            if let Some(ref msg) = self.error_msg {
                ui.colored_label(egui::Color32::RED, msg);
            }
        });

        #[cfg(target_arch = "wasm32")]
        if let Some(bytes) = take_pending_bytes() {
            self.load_from_bytes(&bytes);
        }

        let frame = egui::Frame::new()
            .fill(egui::Color32::BLACK)
            .inner_margin(0.0);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            let Some((sagittal, coronal, axial)) = self.get_slices() else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "No volume loaded.\nUse File > Load NIfTI… to open a file.",
                        )
                        .color(egui::Color32::GRAY)
                        .size(20.0),
                    );
                });
                return;
            };

            let img_s = Self::array2_to_color_image(&sagittal);
            let img_c = Self::array2_to_color_image(&coronal);
            let img_a = Self::array2_to_color_image(&axial);

            let vd = self.voxdim; // [R, A, S]

            // Save pixel sizes before textures consume the images
            let s_px = img_s.size;
            let c_px = img_c.size;
            let a_px = img_a.size;

            let tex_s = ui
                .ctx()
                .load_texture("sagittal", img_s, egui::TextureOptions::LINEAR);
            let tex_c = ui
                .ctx()
                .load_texture("coronal", img_c, egui::TextureOptions::LINEAR);
            let tex_a = ui
                .ctx()
                .load_texture("axial", img_a, egui::TextureOptions::LINEAR);

            let avail = ui.available_size();
            let spacing = ui.spacing().item_spacing;
            let cell_w = (avail.x - spacing.x) / 2.0;
            let cell_h = (avail.y - spacing.y) / 2.0;

            let border_width = 0.0;
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

            // Images scale to fill the full quadrant
            let size_s = Self::fit_size(s_px[0], s_px[1], vd[1], vd[2], cell_w, cell_h);
            let size_c = Self::fit_size(c_px[0], c_px[1], vd[0], vd[2], cell_w, cell_h);
            let size_a = Self::fit_size(a_px[0], a_px[1], vd[0], vd[1], cell_w, cell_h);

            let overlay_bg = egui::Color32::from_black_alpha(160);
            let label_font = egui::FontId::proportional(14.0);
            let strip_h = 22.0;
            let slider_strip_h = 28.0;
            let pad = 4.0;

            // ── Top row ──────────────────────────────────────────────
            ui.horizontal(|ui| {
                // Upper-left: Axial (Yellow)
                ui.allocate_ui(egui::vec2(cell_w, cell_h), |ui| {
                    let (cell_rect, _) =
                        ui.allocate_exact_size(egui::vec2(cell_w, cell_h), egui::Sense::hover());
                    let offset = egui::vec2((cell_w - size_a.x) / 2.0, (cell_h - size_a.y) / 2.0);
                    let img_rect = egui::Rect::from_min_size(cell_rect.min + offset, size_a);
                    ui.painter()
                        .image(tex_a.id(), img_rect, uv, egui::Color32::WHITE);
                    ui.painter().rect_stroke(
                        img_rect,
                        0.0,
                        egui::Stroke::new(border_width, egui::Color32::YELLOW),
                        egui::StrokeKind::Outside,
                    );
                    let label_strip = egui::Rect::from_min_size(
                        cell_rect.min,
                        egui::vec2(cell_rect.width(), strip_h),
                    );
                    ui.painter().rect_filled(label_strip, 0.0, overlay_bg);
                    ui.painter().text(
                        label_strip.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("Axial  Z = {:.1} mm", self.voxel_to_mm(2, self.slice_z)),
                        label_font.clone(),
                        egui::Color32::YELLOW,
                    );
                    let slider_strip = egui::Rect::from_min_size(
                        egui::pos2(cell_rect.min.x, cell_rect.max.y - slider_strip_h),
                        egui::vec2(cell_rect.width(), slider_strip_h),
                    );
                    ui.painter().rect_filled(slider_strip, 0.0, overlay_bg);
                    let mm_a = self.voxel_to_mm(2, 0);
                    let mm_b = self.voxel_to_mm(2, self.volume.as_ref().unwrap().shape()[2] - 1);
                    let mm_min_z = mm_a.min(mm_b);
                    let mm_max_z = mm_a.max(mm_b);
                    let mut mm_z = self.voxel_to_mm(2, self.slice_z);
                    let resp = ui.put(
                        slider_strip.shrink(pad),
                        egui::Slider::new(&mut mm_z, mm_min_z..=mm_max_z)
                            .suffix(" mm")
                            .step_by(self.voxdim[2] as f64),
                    );
                    if resp.changed() {
                        self.slice_z = self.mm_to_voxel(2, mm_z);
                    }
                    if ui.rect_contains_pointer(cell_rect) {
                        self.scroll_accum[2] += ui.input(|i| i.raw_scroll_delta.y);
                        let step = 30.0_f32;
                        while self.scroll_accum[2] >= step {
                            self.scroll_accum[2] -= step;
                            if self.slice_z < self.volume.as_ref().unwrap().shape()[2] - 1 {
                                self.slice_z += 1;
                            }
                        }
                        while self.scroll_accum[2] <= -step {
                            self.scroll_accum[2] += step;
                            self.slice_z = self.slice_z.saturating_sub(1);
                        }
                    }
                });

                // Upper-right: empty quadrant
                ui.allocate_ui(egui::vec2(cell_w, cell_h), |_ui| {});
            });

            // ── Bottom row ───────────────────────────────────────────
            ui.horizontal(|ui| {
                // Lower-left: Coronal (Green)
                ui.allocate_ui(egui::vec2(cell_w, cell_h), |ui| {
                    let (cell_rect, _) =
                        ui.allocate_exact_size(egui::vec2(cell_w, cell_h), egui::Sense::hover());
                    let offset = egui::vec2((cell_w - size_c.x) / 2.0, (cell_h - size_c.y) / 2.0);
                    let img_rect = egui::Rect::from_min_size(cell_rect.min + offset, size_c);
                    ui.painter()
                        .image(tex_c.id(), img_rect, uv, egui::Color32::WHITE);
                    ui.painter().rect_stroke(
                        img_rect,
                        0.0,
                        egui::Stroke::new(border_width, egui::Color32::GREEN),
                        egui::StrokeKind::Outside,
                    );
                    let label_strip = egui::Rect::from_min_size(
                        cell_rect.min,
                        egui::vec2(cell_rect.width(), strip_h),
                    );
                    ui.painter().rect_filled(label_strip, 0.0, overlay_bg);
                    ui.painter().text(
                        label_strip.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("Coronal  Y = {:.1} mm", self.voxel_to_mm(1, self.slice_y)),
                        label_font.clone(),
                        egui::Color32::GREEN,
                    );
                    let slider_strip = egui::Rect::from_min_size(
                        egui::pos2(cell_rect.min.x, cell_rect.max.y - slider_strip_h),
                        egui::vec2(cell_rect.width(), slider_strip_h),
                    );
                    ui.painter().rect_filled(slider_strip, 0.0, overlay_bg);
                    let mm_a = self.voxel_to_mm(1, 0);
                    let mm_b = self.voxel_to_mm(1, self.volume.as_ref().unwrap().shape()[1] - 1);
                    let mm_min_y = mm_a.min(mm_b);
                    let mm_max_y = mm_a.max(mm_b);
                    let mut mm_y = self.voxel_to_mm(1, self.slice_y);
                    let resp = ui.put(
                        slider_strip.shrink(pad),
                        egui::Slider::new(&mut mm_y, mm_min_y..=mm_max_y)
                            .suffix(" mm")
                            .step_by(self.voxdim[1] as f64),
                    );
                    if resp.changed() {
                        self.slice_y = self.mm_to_voxel(1, mm_y);
                    }
                    if ui.rect_contains_pointer(cell_rect) {
                        self.scroll_accum[1] += ui.input(|i| i.raw_scroll_delta.y);
                        let step = 30.0_f32;
                        while self.scroll_accum[1] >= step {
                            self.scroll_accum[1] -= step;
                            if self.slice_y < self.volume.as_ref().unwrap().shape()[1] - 1 {
                                self.slice_y += 1;
                            }
                        }
                        while self.scroll_accum[1] <= -step {
                            self.scroll_accum[1] += step;
                            self.slice_y = self.slice_y.saturating_sub(1);
                        }
                    }
                });

                // Lower-right: Sagittal (Red)
                ui.allocate_ui(egui::vec2(cell_w, cell_h), |ui| {
                    let (cell_rect, _) =
                        ui.allocate_exact_size(egui::vec2(cell_w, cell_h), egui::Sense::hover());
                    let offset = egui::vec2((cell_w - size_s.x) / 2.0, (cell_h - size_s.y) / 2.0);
                    let img_rect = egui::Rect::from_min_size(cell_rect.min + offset, size_s);
                    ui.painter()
                        .image(tex_s.id(), img_rect, uv, egui::Color32::WHITE);
                    ui.painter().rect_stroke(
                        img_rect,
                        0.0,
                        egui::Stroke::new(border_width, egui::Color32::RED),
                        egui::StrokeKind::Outside,
                    );
                    let label_strip = egui::Rect::from_min_size(
                        cell_rect.min,
                        egui::vec2(cell_rect.width(), strip_h),
                    );
                    ui.painter().rect_filled(label_strip, 0.0, overlay_bg);
                    ui.painter().text(
                        label_strip.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("Sagittal  X = {:.1} mm", self.voxel_to_mm(0, self.slice_x)),
                        label_font.clone(),
                        egui::Color32::RED,
                    );
                    let slider_strip = egui::Rect::from_min_size(
                        egui::pos2(cell_rect.min.x, cell_rect.max.y - slider_strip_h),
                        egui::vec2(cell_rect.width(), slider_strip_h),
                    );
                    ui.painter().rect_filled(slider_strip, 0.0, overlay_bg);
                    let mm_a = self.voxel_to_mm(0, 0);
                    let mm_b = self.voxel_to_mm(0, self.volume.as_ref().unwrap().shape()[0] - 1);
                    let mm_min_x = mm_a.min(mm_b);
                    let mm_max_x = mm_a.max(mm_b);
                    let mut mm_x = self.voxel_to_mm(0, self.slice_x);
                    let resp = ui.put(
                        slider_strip.shrink(pad),
                        egui::Slider::new(&mut mm_x, mm_min_x..=mm_max_x)
                            .suffix(" mm")
                            .step_by(self.voxdim[0] as f64),
                    );
                    if resp.changed() {
                        self.slice_x = self.mm_to_voxel(0, mm_x);
                    }
                    if ui.rect_contains_pointer(cell_rect) {
                        self.scroll_accum[0] += ui.input(|i| i.raw_scroll_delta.y);
                        let step = 30.0_f32;
                        while self.scroll_accum[0] >= step {
                            self.scroll_accum[0] -= step;
                            if self.slice_x < self.volume.as_ref().unwrap().shape()[0] - 1 {
                                self.slice_x += 1;
                            }
                        }
                        while self.scroll_accum[0] <= -step {
                            self.scroll_accum[0] += step;
                            self.slice_x = self.slice_x.saturating_sub(1);
                        }
                    }
                });
            });
        });

        ctx.request_repaint(); // keeps the UI responsive
    }
}

fn load_nifti(path: &str) -> Result<(Array3<f32>, [f32; 3], [f32; 3])> {
    let obj = ReaderOptions::new().read_file(path)?;
    let header = obj.header().clone();
    let volume = obj.into_volume().into_ndarray::<f32>()?;
    let volume = volume.into_dimensionality::<ndarray::Ix3>()?;
    let (volume, voxdim, ras_origin) = reorient_to_ras(volume, &header);
    Ok((volume, voxdim, ras_origin))
}

#[cfg(target_arch = "wasm32")]
fn load_nifti_bytes(bytes: &[u8]) -> Result<(Array3<f32>, [f32; 3], [f32; 3])> {
    let is_gz = bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b;
    let cursor = Cursor::new(bytes);
    if is_gz {
        load_nifti_reader(GzDecoder::new(cursor))
    } else {
        load_nifti_reader(cursor)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_nifti_bytes(_bytes: &[u8]) -> Result<(Array3<f32>, [f32; 3], [f32; 3])> {
    anyhow::bail!("in-memory loading is only available on wasm")
}

fn load_nifti_reader<R: Read>(mut reader: R) -> Result<(Array3<f32>, [f32; 3], [f32; 3])> {
    let header = NiftiHeader::from_reader(&mut reader)?;
    let vox_offset = header.vox_offset.max(348.0) as usize;
    let skip = vox_offset.saturating_sub(348);
    if skip > 0 {
        let mut discard = vec![0u8; skip];
        reader.read_exact(&mut discard)?;
    }
    let volume = InMemNiftiVolume::from_reader(reader, &header)?;
    let volume = volume.into_ndarray::<f32>()?;
    let volume = volume.into_dimensionality::<ndarray::Ix3>()?;
    let (volume, voxdim, ras_origin) = reorient_to_ras(volume, &header);
    Ok((volume, voxdim, ras_origin))
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static PENDING_BYTES: RefCell<Option<Vec<u8>>> = RefCell::new(None);
}

#[cfg(target_arch = "wasm32")]
fn set_pending_bytes(bytes: Vec<u8>) {
    PENDING_BYTES.with(|cell| {
        *cell.borrow_mut() = Some(bytes);
    });
}

#[cfg(target_arch = "wasm32")]
fn take_pending_bytes() -> Option<Vec<u8>> {
    PENDING_BYTES.with(|cell| cell.borrow_mut().take())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    let app = NiftiViewer::new();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rust NIfTI Triple Axis Viewer",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // Set up panic hook for better error messages
    console_error_panic_hook::set_once();
    wasm_logger::init(wasm_logger::Config::default());
    let app = NiftiViewer::new();
    let web_options = eframe::WebOptions::default();
    let window = web_sys::window().expect("window not available");
    let document = window.document().expect("document not available");
    let canvas: HtmlCanvasElement = document
        .get_element_by_id("canvas_render")
        .expect("canvas not found")
        .dyn_into()
        .expect("canvas element");
    wasm_bindgen_futures::spawn_local(async move {
        eframe::WebRunner::new()
            .start(canvas, web_options, Box::new(|_cc| Ok(Box::new(app))))
            .await
            .expect("failed to start eframe web app");
    });
}
