# Third-party notices

Mosamem is licensed under the GNU GPL v3.0 or later (see [LICENSE](LICENSE)). It ships with, or downloads,
the following components, each under its own license.

## Bundled with the app

| Component | Use | License |
|---|---|---|
| [IBM Plex Sans Arabic](https://github.com/IBM/plex) | Interface font and text tool | SIL Open Font License 1.1 (`assets/fonts/ibm-plex-sans-arabic/OFL.txt`) |
| [Cairo](https://github.com/Gue3bara/Cairo) | Text tool | SIL Open Font License 1.1 (`assets/fonts/cairo/OFL.txt`) |
| [Tajawal](https://github.com/googlefonts/tajawal) | Text tool | SIL Open Font License 1.1 (`assets/fonts/tajawal/OFL.txt`) |
| [Amiri](https://github.com/aliftype/amiri) | Text tool | SIL Open Font License 1.1 (`assets/fonts/amiri/OFL.txt`) |
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) (prebuilt by [pyke](https://github.com/pykeio/ort)) | Runs the AI models | MIT |
| [Dawn](https://dawn.googlesource.com/dawn) (`libwebgpu_dawn`) | ONNX Runtime's WebGPU provider | BSD 3-Clause |
| [ort](https://github.com/pykeio/ort) | Rust bindings for ONNX Runtime | MIT or Apache-2.0 |
| [HarfRust](https://github.com/harfbuzz/harfrust), [Skrifa](https://github.com/googlefonts/fontations) | Arabic shaping and font outlines | MIT or Apache-2.0 |
| [tiny-skia](https://github.com/linebender/tiny-skia) | Glyph rasterization | BSD 3-Clause |
| [unicode-bidi](https://github.com/servo/unicode-bidi), [ICU4X](https://github.com/unicode-org/icu4x) | Bidirectional text, line breaking | MIT or Apache-2.0; Unicode License v3 |
| [ag-psd](https://github.com/Agamnentzar/ag-psd) | PSD reading and writing | MIT |
| [Tauri](https://tauri.app), [Svelte](https://svelte.dev) | App shell and interface | MIT or Apache-2.0; MIT |
| [image](https://github.com/image-rs/image), [libwebp](https://chromium.googlesource.com/webm/libwebp), [rusqlite](https://github.com/rusqlite/rusqlite) / SQLite | Image files, `.mosamem` documents | MIT or Apache-2.0; BSD 3-Clause; MIT / public domain |

The full dependency lists are in `Cargo.lock` and `package-lock.json`. Each crate and package carries its
license in its own source.

## Downloaded on first use (not bundled)

The AI models are fetched from their authors' repositories, pinned to a commit and checked by SHA-256
(`src-tauri/src/ai/models.rs`). The downloaded files keep their licenses:

| Model | Feature | License | Source |
|---|---|---|---|
| BiRefNet lite (ONNX export by onnx-community) | Select Subject, Remove Background | MIT | https://huggingface.co/onnx-community/BiRefNet_lite-ONNX |
| SAM 2.1 hiera-tiny (ONNX export by onnx-community) | Object Selection | Apache-2.0 | https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX |
| MI-GAN (Picsart AI Research) | Smart Fill | MIT (code and weights) | https://github.com/Picsart-AI-Research/MI-GAN |

## Not used

Mosamem contains no code, icons, images or names from Adobe Photoshop. Behavior was learned from public
help pages, Adobe's public PSD file format specification, and public tutorials (see `docs/research.md`).
