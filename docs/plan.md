# Lawha (لوحة): plan for version 1

Status: draft for review. No code is written until this plan is approved.

## 1. Principles

- **Clean room.** Behavior comes from public sources only: Adobe's public help pages, the public
  Photoshop File Formats Specification, W3C/ISO specs, academic papers, and public tutorials.
  We never open, disassemble or instrument Adobe software. GIMP and Krita are open source, so we
  read their public docs and design notes to learn techniques. We never copy their code. Every
  feature's source is listed in `docs/research.md`.
- **Our own identity.** We make our own name, icons, colors and wording. We avoid Adobe trademarked
  feature names (for example we say "Smart Fill", not "Content-Aware Fill"). Keyboard shortcuts
  follow common convention so Photoshop users feel at home.
- **Arabic first.** The UI is designed RTL first, and LTR/English is the second mode. Arabic text
  on the canvas is a core feature, not an add-on.
- **Never lose work.** Autosave is incremental and transactional, and crash recovery is tested by
  killing the process.
- **Measured quality.** Every quality claim has an automated test or a recorded measurement.

## 2. Architecture

```
┌──────────────────────────── Tauri 2 window (WKWebView / WebView2) ────────────────────────────┐
│ UI (Svelte 5 + TypeScript)                                                                     │
│   panels · our own docking · tools · options bar · i18n (ar/en, RTL/LTR) · shortcuts          │
│ Display compositor (WebGL2)                                                                    │
│   GPU tile atlas + per-mip page tables · coarse-level fallback while tiles load               │
│   live layers for interaction: below-cache · active layer · above-cache, with blend shaders   │
└──────────── tiles over the lawha:// scheme (~1 GB/s) · commands over raw-bytes IPC ───────────┘
┌──────────────────────────────── Rust engine (Tauri backend) ──────────────────────────────────┐
│ Documents: layers, groups, masks, adjustments, styles, text · sparse 256² tiles, Arc + CoW     │
│ Compositing on the CPU (rayon), cached per tile and per mip level                              │
│ Painting, selections, filters, transforms, text layout · history (tile before-images)         │
│ Codecs: PNG, JPEG, WebP, TIFF; HEIC through macOS ImageIO; ICC → sRGB (moxcms)                 │
│ .lawha format + autosave journal (SQLite) · crash detection                                    │
│ Local AI: ONNX Runtime (WebGPU provider, CPU fallback; Core ML measured and dropped in M10)    │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
lawha-core (Rust crate) = the pixel math in one place: used by the engine and by the tests, and the
reference the GPU shaders are checked against.
```

**Changed after milestone 0 (measured, see docs/spikes.md):** the plan first kept pixels in the
webview. The spikes showed three things:
- Moving data between the webview and Rust is fast on Mac: about 4 GB/s through `invoke` and about
  1 GB/s for tiles over the custom scheme.
- WebKit throttles and can kill hidden pages.
- The GPU atlas can't hold 30 full layers' visible tiles.

So **the Rust engine owns all pixels and does the CPU work**, and the webview is the UI plus a display
compositor. This removes WASM threads, the 4 GB wasm32 limit and the page-kill risk. Windows transfer
speed gets measured in the VM. If it is too slow there, WebView2's shared-memory buffers are the planned
fix.

Key decisions, with reasons:

| Decision | Why |
|---|---|
| Pixel data lives in the Rust engine; the webview displays tiles | Measured in M0: fast enough transfer, no webview memory limits, works when the window is hidden. Live interaction (opacity, moving, painting) is drawn by the GPU from the active layer plus cached composites, so it never waits on a full recomposite. |
| One Rust crate for pixel math | A single source of truth. `cargo test` checks the formulas against the specs, then a GPU-vs-CPU test checks each shader against the crate (±1/255). |
| WebGL2 first, WebGPU later behind the same renderer interface | WebGL2 is solid in both WKWebView and WebView2 today. WebGPU depends on the OS version, so it becomes an optional faster path after v1. |
| 8-bit RGBA tiles, compositing in half-float | Same default as mainstream editors. It avoids banding when many adjustments stack. 16-bit documents come after v1. |
| PSD via **ag-psd** (MIT, TypeScript) behind our own `PsdCodec` interface | The most complete open read/write library: layers, groups, masks, blend modes, adjustments, text and effects. It runs in a worker and hands layer pixels to the engine, and it can be replaced behind our interface. |
| Text: harfrust (the HarfBuzz project's official Rust port) + skrifa + unicode-bidi + ICU4X line breaking in lawha-core; glyph outlines rasterized by us. Parley is evaluated in M0 as a ready-made layout layer. | Identical shaping and layout on Mac and Windows, so the same file looks the same everywhere and the layout can be tested headless. It also supports kashida justification. (rustybuzz and ttf-parser were archived and flagged unmaintained by RustSec in 2026.) |
| Native project format `.lawha` = SQLite | Transactional, incremental saves of dirty tiles only, survives crashes. The autosave journal is the same format. PSD stays the interchange format. |
| Local AI through ONNX Runtime (`ort`) on the native side, with license-checked models: BiRefNet-lite (MIT, about 115 MB fp16) for background removal and subject selection, SAM 2.1 tiny (Apache-2.0, about 70 MB) for click-to-select, MI-GAN (MIT code and weights, 28 MB) for Smart Fill | Uses the GPU through ONNX Runtime's WebGPU provider (M10 measured Core ML as failing or slower for these models), with the CPU as fallback. No cloud and no account. Models with non-commercial terms (BRIA RMBG, EdgeSAM) or unclear weight licenses (ISNet, LaMa's training data) are excluded. |
| Classic algorithms where a public paper already solves the problem | Healing = Poisson blending (Pérez 2003 / Georgiev 2004). Smart Fill = PatchMatch (Barnes 2009), plus an optional LaMa model. |
| Svelte 5 UI with our own docking system | Small and fast. None of the docking libraries supports RTL (dockview, golden-layout and FlexLayout all have open or closed-as-won't-fix RTL issues), so we build a small one: panels mirror, the canvas never does. |
| GPL-3.0 license | Matches Adawat and keeps forks open. Every dependency (MIT, Apache-2.0, OFL fonts) is compatible. |

### The main risk, and how milestone 0 handles it

A 6000×4000 document with 30 full layers is about 2.9 GB of raw pixels. WebKit's source shows that a
WKWebView page is killed above about 4 GB when it is **hidden or occluded** (and above ~15 GB when
visible on this Mac). wasm32 is capped at 4 GB, and Safari doesn't ship memory64. **Design rule: the
webview keeps at most ~2 GB of pixel data.** The Rust side owns the full tile store; the webview holds
only hot tiles. The design answers this with sparse tiles,
LZ4-compressed cold tiles, the native tile store (memory plus disk), and keeping only visible tiles
on the GPU. Tile transfer uses a custom URI scheme or channels; Tauri's IPC is reported as
much slower on Windows than on Mac, so this is benchmarked too. If a page is ever killed, the native
side still holds the document and simply reloads the UI. Milestone 0 measures this first. **Go/no-go gate:** if the webview can't meet the
performance and memory targets, we switch the renderer to a native Rust/wgpu surface under the
webview UI. We decide this before building features on top of it.

## 3. Milestones and what "done" means

Every milestone ends the same way: tests pass → build and launch the app → scripted screenshots in
Arabic and English → I check them and fix what's wrong → commit with a clear message → a short
report to you.

| # | Milestone | Done means |
|---|---|---|
| **0** | Research and spikes | `docs/research.md` lists a public source for every feature. Toolchain installed (Rust, wasm target, Tauri CLI). Spike numbers recorded for: 6000×4000×30-layer compositing fps, webview memory ceiling, tile transfer throughput on Mac (Windows measured later in the VM), pen pressure (WKWebView reports Wacom pens as a mouse, so we test a native NSEvent pressure bridge), cross-origin isolation for wasm threads, ag-psd on a large PSD. Architecture go/no-go decided and written down. |
| **1** | App shell and canvas | Tauri app opens on Mac (core code stays cross-platform; Windows builds come later, see M11). Dark theme, ar/en switch with full RTL/LTR mirroring, tools bar, options bar, dockable/collapsible/rearrangeable panels, rulers, guides, grid. Tile store and WebGL2 compositor with pan/zoom/rotate view. New document presets (Instagram post/story, YouTube thumbnail, A4 at 300 dpi, custom). Open/save PNG, JPEG, WebP, TIFF, and HEIC on Mac. **Perf gate:** pan/zoom ≥ 60 fps (p95 frame < 16.7 ms) on 6000×4000×30 layers on the M4 Pro. |
| **2** | Layers, blend modes, history, autosave | Unlimited layers, groups (incl. Pass Through), opacity, fill, lock, hide, drag reorder, merge, duplicate. All requested blend modes plus the rest of the PSD set (Linear Burn/Dodge, Vivid/Linear/Pin Light, Hard Mix, Darker/Lighter Color, Subtract, Divide). Tests compare every mode to the published formula on CPU and on GPU. History panel with a memory-budgeted undo stack. `.lawha` save and open. Autosave plus crash recovery, tested by `kill -9` mid-edit and checking that nothing is lost after the last autosave. |
| **3** | Selections and masks | Rectangle, ellipse (fixed ratio/size), lasso, polygonal lasso, magic wand, quick selection (classic), select all/inverse/deselect, feather, expand, contract, refine edge (guided-filter edge refinement), marching ants. Layer masks (paint, feather, density, invert, disable) and clipping masks. Tests check selection masks against known shapes. |
| **4** | Painting | Brush engine: size, hardness, opacity versus flow (stroke buffer model), spacing, smoothing, pen pressure (Mac and Windows). Eraser, clone stamp (aligned or not), healing brush and spot healing, gradient (linear, radial, angle, reflected, diamond), paint bucket, eyedropper (sample sizes). Brushes and Color panels. **Perf gate:** paint latency p95 < 16 ms with a 500 px brush on the 6000×4000×30 test document. |
| **5** | Adjustments and filters | Adjustment layers with a Properties panel: Levels, Curves (live histogram), Hue/Saturation, Brightness/Contrast, Exposure, Vibrance, Color Balance, Black & White, Gradient Map, Invert, Threshold. Filters with live preview: Gaussian blur, motion blur, unsharp mask, noise, pixelate. Tests: known-input to known-output vectors for each, plus GPU-vs-CPU agreement. |
| **6** | Transform and resize | Move and free transform (scale, rotate, skew, distort, perspective, flip, reference point), crop with ratios, straighten and delete-or-keep cropped pixels, canvas size with anchor, image size with nearest/bilinear/bicubic/Lanczos. Tests check resampling against reference kernels. |
| **7** | Arabic-first text | Text tool: RTL/LTR, shaping and ligatures, mixed Arabic/English, diacritics, Arabic/Hindi digits, kashida justification. Bundled IBM Plex Sans Arabic, Cairo, Tajawal and Amiri. Size, weight, color, line and letter spacing, alignment. Text stays editable. Tests: shaping golden tests in Rust plus screenshots of an Arabic test sheet checked by eye. |
| **8** | Layer styles | Drop shadow, inner shadow, outer glow, stroke (inside/center/outside), color overlay, gradient overlay. All non-destructive, respecting fill versus opacity. |
| **9** | PSD | Read and write layers, groups, masks, clipping, blend modes, opacity/fill, adjustment layers, layer styles, and text (editable when the PSD carries text data, otherwise rasterized with a warning). **Round-trip report:** a set of real PSDs is opened, rendered, re-saved and re-opened. Our render is compared with the composite stored in the file, and the per-file pixel difference is published in `docs/psd-report.md`. |
| **10** | Local AI | Remove background and Select Subject (matting/segmentation model), click-to-select (segment-anything-style model), Smart Fill (PatchMatch, plus LaMa if it fits the size budget). Runs offline. A network-capture test shows the app makes no network requests. |
| **11** | Export, polish, release | Export As (PNG/JPEG/WebP with quality and size preview), export selection or layer, web size presets. Photoshop-familiar shortcuts with a reference sheet. Our own icon set and logo. Arabic and English README. Mac `.dmg` (Apple silicon + Intel) and Windows installer (x64 + arm64) built by GitHub Actions. An honest `docs/status.md` listing what works and what isn't done. |

## 4. How it is tested

- **Rust (`cargo test`)**: blend and adjustment formulas, resampling, selections, flood fill,
  brush dab math, text shaping goldens, PSD structure, `.lawha` and autosave journal.
- **GPU vs CPU**: every shader is rendered in a headless WebGL2 context and compared with lawha-core
  output (±1/255).
- **TypeScript (Vitest)**: document model, history, layer operations, i18n completeness (no missing
  Arabic strings).
- **End-to-end**: WebdriverIO with Tauri's embedded WebDriver plugin, which works on macOS, unlike
  tauri-driver. It drives scripted scenarios such as opening a file, painting a stroke or running
  a filter.
- **In-app perf harness** (debug builds only): records frame times and paint latency, and takes
  window screenshots on Mac (`screencapture -l`) and on the Windows VM.
- **Crash recovery**: script → edit → `kill -9` → relaunch → assert the recovered pixels match.
- **No cloud**: REA's process/network capture is used on **our own** built app to show it makes no
  outbound connections.
- **Windows**: built by CI and smoke-tested on the Windows 11 ARM VM. VM GPU performance is not
  representative, so Windows performance numbers need a real Windows PC.

## 5. Decisions (2026-10-20)

- Plan approved.
- **Mac first.** Build and test on Mac until the core milestones are solid. The code stays
  cross-platform from day one: nothing Mac-only in the core, and platform code sits behind interfaces.
  The Windows build is produced and tested only after the main features work well on Mac, in the
  Windows 11 ARM VM (UTM), with a report of what works.
- **AI models** download on first use from our GitHub Releases, checksum-verified. After that
  everything runs offline.
- **Test PSDs:** public MIT-licensed fixtures, plus PSDs the user makes in their own Photoshop from a
  checklist we provide.
- **No pen tablet, no real Windows PC, no Apple Developer ID yet.**
  - Ship unsigned, like Adawat. The README explains macOS "Open Anyway" (System Settings → Privacy &
    Security) and Windows SmartScreen "More info → Run anyway".
  - Pen pressure is tested with simulated input: synthetic pointer events with pressure ramps, and
    simulated samples fed into the native pressure bridge.
  - Real-pen pressure and real-PC Windows performance are reported as **not verified**.
