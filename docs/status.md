# Lawha: status by milestone

This file is honest by design: each milestone lists what was verified and how, and what is not
verified.

## Milestone 0: research and spikes ✔︎ (2026-10-20)

- Research: see `docs/research.md` and `docs/spikes.md`.
- Decision: **go**, with the Rust engine owning all pixels (see `docs/plan.md` §2).

## Milestone 1: app shell and canvas ✔︎ (2026-10-20)

### What works, verified

| Area | Verified by |
|---|---|
| Dark theme. Arabic UI by default; one click (or Window menu) switches to English, mirroring the whole UI (tools, dock, rulers, dialogs) while the canvas never mirrors | Screenshots `docs/shots/m1-*` in both languages; i18n tests (same keys, same placeholders, `dir` flips) |
| Native menus in both languages (File, Edit, View, Window), with standard shortcuts (⌘N, ⌘O, ⌘W, ⌘=, ⌘−, ⌘0, ⌘1, ⌘R, ⌘', ⌘;) | Built from the same translation tables, rebuilt on language change |
| Toolbar with Photoshop-familiar single-key shortcuts by **physical key** (they work with an Arabic keyboard layout), tool groups with flyouts, Shift+key to cycle | Manual and harness checks |
| Docking: two columns plus an icon strip with flyouts; tabbed stacks; collapse; drag tabs between stacks, between stacks, or into a new column; resize columns; layout saved per user; "Reset workspace" | Unit tests (`src/tests/dock.test.ts`); screenshot `m1-dock-rotate-en` |
| Canvas: GPU tile compositor, coarse-level fallback while loading, crisp pixels above 100%, pixel grid at 800%+, pan (hand, space, trackpad), zoom (tool, ⌘ keys, pinch, ⌘/Alt + wheel, about the cursor), rotate view (tool, angle field, Shift snaps 15°) | Unit tests for the view math; screenshots |
| Rulers (px), guides (drag from rulers, move with the Move tool, drop on a ruler to delete), grid (spacing plus subdivisions), document edge | Screenshot `m1-guides-grid-ar` |
| New Document: Instagram post 4:5, square, story; YouTube thumbnail; Full HD; A4 at 300 ppi; custom with px/mm/cm/in, resolution, orientation and background (white, black, transparent, color) | Screenshots in both languages |
| Open PNG, JPEG, WebP, TIFF, HEIC (ImageIO). Embedded ICC profiles converted to sRGB. Drag and drop files onto the window | Rust tests; a 6016×6016 HEIC opened in 0.84 s |
| Export PNG (sRGB chunk + ppi), JPEG (quality, ppi), WebP (lossy and lossless), TIFF (Deflate), HEIC (quality). Atomic writes (temporary file, then rename) | Rust round-trip tests (lossless formats exact; JPEG PSNR > 38 dB, WebP > 36 dB, HEIC > 34 dB); 6016² exports checked by reading them back with macOS `sips` |

### Performance gate: met on the M4 Pro

Test document: 6000×4000 with 30 layers, **every layer covering the whole canvas**. That is the worst
case, about 3 GB of pixels held by the engine. The cache was cold. The run uses the debug app with
optimized pixel code. Raw numbers are in `docs/perf-m1.json`.

| Scenario | fps | Frame interval p95 | Dropped frames | GPU draw p95 |
|---|---|---|---|---|
| Pan at fit | 60 | 18.1 ms | 0 | 0.28 ms |
| Pan at 100% | 60 | 17.7 ms | 0 | 0.74 ms |
| Zoom sweep fit → 400% | 60 | 18.0 ms | 0 | 0.40 ms |
| Rotate 360° | 60 | 18.0 ms | 0 | 0.28 ms |

- The webview's `requestAnimationFrame` runs at 60 Hz, so 60 fps is the ceiling.
- Full detail at fit view appears **0.64 s** after creating the document.
- While new tiles load, the view shows a coarser level for at most about 6 frames (100 ms). It never
  shows holes.
- CPU compositing costs 4.5 ms per 256² tile for 30 full layers on one thread, and runs in parallel
  across cores.

- **Release build** (`tauri build`, 10 MB `.app`): launched, opened a HEIC, tiles served over
  `lawha://`, rendered correctly (`docs/shots/m1-release.jpg`).

### Not done or not verified in M1 (by plan)

- Layers can't be edited yet (visibility, order, opacity and so on): that's M2. The Layers panel shows
  the stack read-only, with placeholder thumbnails.
- The History, Adjustments and Brushes panels show which milestone completes them.
- Rulers show unrotated document coordinates when the view is rotated.
- Grid spacing and ruler units can't be changed from the UI yet (preferences come in M11).
- Windows: not built or tested yet (decided: after the core milestones, in the Windows 11 ARM VM).
