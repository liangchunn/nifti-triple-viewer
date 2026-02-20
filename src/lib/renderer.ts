import type { RefObject } from "react";
import type { NiftiData, ViewPlane } from "./types";

// Cached off-screen resources to avoid per-frame allocations
let _tmpCanvas: HTMLCanvasElement | null = null;
let _tmpCtx: CanvasRenderingContext2D | null = null;
let _imageData: ImageData | null = null;
let _cachedCols = 0;
let _cachedRows = 0;

// Cached LUT to avoid rebuilding when contrast/brightness haven't changed
let _lut: Uint8ClampedArray | null = null;
let _cachedContrast = NaN;
let _cachedBrightness = NaN;

// Cache main canvas context (getContext is cheap but returns same object anyway)
const _ctxCache = new WeakMap<HTMLCanvasElement, CanvasRenderingContext2D>();

function getTmpCanvas(cols: number, rows: number) {
  if (_tmpCanvas && _cachedCols === cols && _cachedRows === rows) {
    return { tmpCanvas: _tmpCanvas, tmpCtx: _tmpCtx!, imageData: _imageData! };
  }
  if (!_tmpCanvas) {
    _tmpCanvas = document.createElement("canvas");
    _tmpCtx = _tmpCanvas.getContext("2d")!;
  }
  _tmpCanvas.width = cols;
  _tmpCanvas.height = rows;
  _imageData = _tmpCtx!.createImageData(cols, rows);
  _cachedCols = cols;
  _cachedRows = rows;
  return { tmpCanvas: _tmpCanvas, tmpCtx: _tmpCtx!, imageData: _imageData };
}

function getLut(contrast: number, brightness: number) {
  if (
    _lut &&
    _cachedContrast === contrast &&
    _cachedBrightness === brightness
  ) {
    return _lut;
  }
  if (!_lut) _lut = new Uint8ClampedArray(256);
  for (let i = 0; i < 256; i++) {
    _lut[i] = (i - 128) * contrast + 128 + brightness;
  }
  _cachedContrast = contrast;
  _cachedBrightness = brightness;
  return _lut;
}

export function render(
  data: NiftiData | null,
  slice: number,
  canvas: RefObject<HTMLCanvasElement | null>,
  viewPlane: ViewPlane,
  contrast: number,
  brightness: number,
) {
  if (data && canvas.current) {
    const c = canvas.current;
    const { header, typedData, orientation, min, max } = data;
    const dimI = header.dims[1];
    const dimJ = header.dims[2];
    const { perm, flip, rasSize, voxdim } = orientation;
    const [rSize, aSize, sSize] = rasSize;

    // RAS display conventions:
    //   Axial:    cols = R (L→R), rows = A (ant top → post bottom), slice = S
    //   Coronal:  cols = R (L→R), rows = S (sup top → inf bottom), slice = A
    //   Sagittal: cols = A (post left → ant right), rows = S (sup top → inf bottom), slice = R
    let cols: number, rows: number;
    let voxW: number, voxH: number;
    if (viewPlane === "axial") {
      cols = rSize;
      rows = aSize;
      voxW = voxdim[0];
      voxH = voxdim[1];
    } else if (viewPlane === "coronal") {
      cols = rSize;
      rows = sSize;
      voxW = voxdim[0];
      voxH = voxdim[2];
    } else {
      cols = aSize;
      rows = sSize;
      voxW = voxdim[1];
      voxH = voxdim[2];
    }

    // Physical aspect ratio accounting for voxel spacing
    const physAspect = (cols * voxW) / (rows * voxH);

    // Size canvas backing store to match the actual CSS display size × DPR
    // so the browser doesn't upscale a low-res buffer on HiDPI screens.
    const dpr = window.devicePixelRatio || 1;
    const cssW = c.clientWidth;
    const cssH = c.clientHeight;
    let canvasW: number, canvasH: number;
    if (cssW > 0 && cssH > 0) {
      // Fit within the CSS box while preserving voxel aspect ratio
      if (cssW / cssH > physAspect) {
        canvasH = Math.round(cssH * dpr);
        canvasW = Math.round(canvasH * physAspect);
      } else {
        canvasW = Math.round(cssW * dpr);
        canvasH = Math.round(canvasW / physAspect);
      }
    } else {
      // Fallback before layout
      canvasW = Math.round(cols * voxW);
      canvasH = Math.round(rows * voxH);
    }

    // Only reset canvas dimensions when they change (setting clears the canvas)
    if (c.width !== canvasW || c.height !== canvasH) {
      c.width = canvasW;
      c.height = canvasH;
    }

    // Reuse cached main canvas context
    let ctx = _ctxCache.get(c);
    if (!ctx) {
      ctx = c.getContext("2d")!;
      _ctxCache.set(c, ctx);
    }

    // Reuse cached off-screen canvas and ImageData
    const { tmpCanvas, tmpCtx, imageData } = getTmpCanvas(cols, rows);

    const range = max - min || 1;
    const invRange = 255 / range;

    // Reuse cached LUT when contrast/brightness haven't changed
    const lut = getLut(contrast, brightness);

    // Use Uint32Array view for 4x fewer memory writes (ABGR on little-endian)
    const buf32 = new Uint32Array(imageData.data.buffer);

    // Pre-extract perm/flip to avoid repeated property lookups in hot loop
    const p0 = perm[0],
      p1 = perm[1];
    const f0 = flip[0],
      f1 = flip[1],
      f2 = flip[2];
    const colsM1 = cols - 1;
    const rowsM1 = rows - 1;
    const rSizeM1 = rSize - 1;
    const aSizeM1 = aSize - 1;
    const sSizeM1 = sSize - 1;
    const dimIJ = dimI * dimJ;

    // Hoist viewPlane branch out of the hot loop
    if (viewPlane === "axial") {
      for (let row = 0; row < rows; row++) {
        const a = rowsM1 - row;
        const rowOff = row * cols;
        for (let col = 0; col < cols; col++) {
          const r = colsM1 - col;
          const voxel0 = f0 ? rSizeM1 - r : r;
          const voxel1 = f1 ? aSizeM1 - a : a;
          const voxel2 = f2 ? sSizeM1 - slice : slice;
          // Inline RAS→voxel mapping using pre-extracted perm
          const vi = p0 === 0 ? voxel0 : p1 === 0 ? voxel1 : voxel2;
          const vj = p0 === 1 ? voxel0 : p1 === 1 ? voxel1 : voxel2;
          const vk = p0 === 2 ? voxel0 : p1 === 2 ? voxel1 : voxel2;
          const value = typedData[vk * dimIJ + vj * dimI + vi];
          const adjusted = lut[((value - min) * invRange) | 0];
          buf32[rowOff + col] =
            0xff000000 | (adjusted << 16) | (adjusted << 8) | adjusted;
        }
      }
    } else if (viewPlane === "coronal") {
      for (let row = 0; row < rows; row++) {
        const s = rowsM1 - row;
        const rowOff = row * cols;
        for (let col = 0; col < cols; col++) {
          const r = colsM1 - col;
          const voxel0 = f0 ? rSizeM1 - r : r;
          const voxel1 = f1 ? aSizeM1 - slice : slice;
          const voxel2 = f2 ? sSizeM1 - s : s;
          const vi = p0 === 0 ? voxel0 : p1 === 0 ? voxel1 : voxel2;
          const vj = p0 === 1 ? voxel0 : p1 === 1 ? voxel1 : voxel2;
          const vk = p0 === 2 ? voxel0 : p1 === 2 ? voxel1 : voxel2;
          const value = typedData[vk * dimIJ + vj * dimI + vi];
          const adjusted = lut[((value - min) * invRange) | 0];
          buf32[rowOff + col] =
            0xff000000 | (adjusted << 16) | (adjusted << 8) | adjusted;
        }
      }
    } else {
      for (let row = 0; row < rows; row++) {
        const s = rowsM1 - row;
        const rowOff = row * cols;
        for (let col = 0; col < cols; col++) {
          const a = colsM1 - col;
          const voxel0 = f0 ? rSizeM1 - slice : slice;
          const voxel1 = f1 ? aSizeM1 - a : a;
          const voxel2 = f2 ? sSizeM1 - s : s;
          const vi = p0 === 0 ? voxel0 : p1 === 0 ? voxel1 : voxel2;
          const vj = p0 === 1 ? voxel0 : p1 === 1 ? voxel1 : voxel2;
          const vk = p0 === 2 ? voxel0 : p1 === 2 ? voxel1 : voxel2;
          const value = typedData[vk * dimIJ + vj * dimI + vi];
          const adjusted = lut[((value - min) * invRange) | 0];
          buf32[rowOff + col] =
            0xff000000 | (adjusted << 16) | (adjusted << 8) | adjusted;
        }
      }
    }

    tmpCtx.putImageData(imageData, 0, 0);

    // Clear & scale voxel-resolution image to display-resolution canvas
    ctx.clearRect(0, 0, canvasW, canvasH);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(tmpCanvas, 0, 0, canvasW, canvasH);
  }
}
