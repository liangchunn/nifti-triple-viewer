build-web:
  rustup target add wasm32-unknown-unknown
  cargo build --release --target wasm32-unknown-unknown
  cargo install --version 0.2.108 wasm-bindgen-cli
  wasm-bindgen --out-dir web --target web target/wasm32-unknown-unknown/release/nifti_triple_viewer.wasm

build-web-ci:
  cargo build --release --target wasm32-unknown-unknown
  wasm-bindgen --out-dir web --target web target/wasm32-unknown-unknown/release/nifti_triple_viewer.wasm
  wasm-opt -Oz --strip-debug --strip-dwarf --metrics -o web/nifti_triple_viewer_bg.wasm web/nifti_triple_viewer_bg.wasm