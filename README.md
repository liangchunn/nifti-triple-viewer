# nifti-triple-viewer

Renders MRI images in [NIfTI format](https://nifti.nimh.nih.gov) in your browser or desktop via [egui](https://github.com/emilk/egui)

Note: Files opened in the tool are processed locally and are NEVER uploaded

> ⚠️ **Disclaimer** ⚠️
>
> This tool (in the current state) is primarily vibe-coded and manually crossed-checked with [3D Slicer](https://www.slicer.org). Use at your own risk!


## Development

- [rustup](https://rustup.rs)
- [just](https://github.com/casey/just)

```sh
# run on your desktop
cargo run --release

# run on the web
just build-web
# serve the static assets
npx serve -s web
```
