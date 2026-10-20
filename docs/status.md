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

## Milestone 2: layers, blend modes, history, autosave ✔︎ (2026-10-20)

### What works, verified

| Area | Verified by |
|---|---|
| Layer tree: unlimited layers and groups. New, duplicate (⌘J), delete, group (⌘G), ungroup (⇧⌘G), merge down (⌘E), merge visible (⇧⌘E), flatten (onto white). Rename by double-click. Show/hide, Alt-click to show one layer alone. Lock states. Multi-select with Shift/⌘. | Rust tests `doc::ops::*`; harness runs; screenshots `m2-layers-*` |
| Drag to reorder, including into and out of groups (a group can't be dropped into itself) | Rust test; synthetic-pointer drag test in the real panel, then undo |
| Groups: **pass-through** (children blend with the backdrop; group opacity fades their effect) and isolated (any other mode). **Clipping masks** (⌥⌘G): clipped layers draw only inside the base and use the base's opacity and mode. **Fill vs opacity.** | Rust tests `doc::composite::*` with exact expected pixel values |
| **All 27 blend modes**: the 16 requested plus Dissolve, Linear Burn/Dodge, Darker/Lighter Color, Vivid/Linear/Pin Light, Hard Mix, Subtract, Divide | CPU: each formula checked against an independent implementation over a value grid, plus known values, non-separable invariants, alpha compositing and PSD keys (`crates/lawha-core/tests/blend_formulas.rs`). **GPU vs CPU: 27 modes × 2 opacities, every pixel within ±1/255** (`docs/gpu-blend-m2.json`) |
| **Live GPU preview**: dragging opacity or fill and changing the blend mode recomposites on the GPU from below / layer / above caches, at 60 fps (p50 16.7 ms, p95 19.4 ms). The engine's composite replaces it when ready, with no flash. It falls back to engine updates when the layer's position can't be previewed exactly. | Rust tests: live parts reproduce the composite within ±1; fallback for non-associative stacks |
| History: every edit is a step, with undo (⌘Z) and redo (⇧⌘Z), labels in the UI language in the Edit menu and History panel, and click-to-jump. Steps share unchanged pixels. Limits: 200 steps and a 2 GB memory budget. | Rust tests (undo/redo, transient drags don't add steps, step limit) |
| `.lawha` native format (SQLite): layer tree, properties, tiles stored once per distinct content (BLAKE3 + LZ4). Saving over the same file is incremental and transactional. | Rust tests (exact round-trip, dedupe, rejecting non-Lawha files). Harness: photo document round-trips with an identical pixel hash |
| **Autosave + crash recovery**: changes are saved about 2 s after you pause, or at least every 20 s, on a background thread. After a crash, a recovery dialog offers the documents on next launch. A clean quit removes the copies. | **`tools/crash-test.sh`: edit → `kill -9` → relaunch → restore → identical pixel hash (PASS)**; screenshot `m2-recovery` |
| Closing a document or the window with unsaved changes asks Save / Don't Save / Cancel | Screenshot `m2-unsaved-ar` |
| Mixed-direction text: names inserted into Arabic sentences are bidi-isolated | i18n test |

### Measurements (6000×4000, 30 full layers, debug app)

| Operation | Time |
|---|---|
| Save `.lawha` (84 MB file) | 1.1 s |
| Re-save after a small edit | 75 ms (incremental) |
| Open | 0.32 s |
| Undo | 1 ms |
| Property edit | 8 ms |

### Not done or not verified in M2

- Locks are stored and shown, but there are no pixel tools yet to enforce them. Enforcement comes with painting (M4) and moving (M6).
- Opacity/fill drags on a layer inside a non-pass-through group, a clipping unit, or under non-Normal layers use the engine path, which is slower on huge documents. They're correct, just not instant.
- The special "fill-sensitive" behavior of 8 modes (docs/research.md §16 #1) isn't implemented yet; it waits on PSD test files (M9).

## Milestone 3: selections and masks ✔︎ (2026-10-20)

### What works, verified

| Area | Verified by |
|---|---|
| **Rectangle and ellipse select**: Shift-drag keeps it square or circular, Alt draws from the center, fixed ratio, fixed size, feather, anti-alias. A click deselects. | Rust tests (exact area and fractional edges; ellipse area within 0.2% of πab); real pointer-event tests; screenshot `m3-ellipse-ar` |
| **Lasso / polygon lasso**: non-zero winding, 4× sub-scanline anti-aliasing. Polygon: click points, close on the first point or by double-click or Enter. Shift snaps to 45°. Backspace removes a point, Esc cancels. | Rust tests (triangle area, self-crossing star); pointer-event test; screenshot `m3-polygon-en` |
| **Magic wand**: tolerance, contiguous (scanline flood fill) or global, sample the active layer or all layers, anti-alias | Rust test (contiguous vs global vs zero tolerance) |
| **Quick select** (classic, no AI): learns the stroke's colors and grows through connected similar pixels with an adaptive threshold, then snaps the edge with the guided filter. Add, or Alt to subtract. | Rust test (IoU > 0.95 on a noisy disc); real photo: the sky selected along the horizon in 0.3 s (`m3-quick-sky`) |
| Combine modes (new, add, subtract, intersect) from the options bar or Shift/Alt. Select All ⌘A, Deselect ⌘D, Reselect ⇧⌘D, Inverse ⇧⌘I. Drag inside the selection to move its outline. | Rust tests; pointer-event test (Move Selection step recorded) |
| **Modify**: Feather (Gaussian via 3 box passes), Expand and Contract (exact Euclidean distance transform; the canvas border is not an edge), Border, Smooth | Rust tests (distance transform exact against brute force; edges move by the exact amount) |
| **Refine Edges**: a floating panel with live preview and views (overlay, on black, on white, black & white, on layers, marching ants). Edge band radius, smooth, feather, contrast, shift edge, decontaminate colors. Output to selection, layer mask, new layer, or new layer with mask. | Rust test (refinement cuts the edge error vs the image edge); screenshots `m3-refine-overlay-*`; sky cut-out (`m3-mask-ar`) |
| **Marching ants**: drawn by the GPU straight from the selection mask (no outline geometry), animated, correct at any zoom and rotation | Screenshots |
| **Layer masks**: Reveal or hide all, reveal or hide selection, with the panel button (Alt for the hiding variant). Density, render-time feather, invert, disable (Shift-click), load as selection (⌘-click), apply, delete. Masks on groups, including pass-through. Saved in `.lawha`. Live GPU previews include the mask. | Rust tests (hide/show, density, disable, feather ramp, apply bakes pixels, save/load round-trip with mask); screenshot `m3-mask-ar` |
| Everything is in history (undo/redo) | Rust tests |

### Measurements (6000×4000, 30 full layers, debug app)

| Operation | Time |
|---|---|
| Rectangle / ellipse / 200-point lasso | 13 / 9 / 11 ms |
| Feather 20 px | 133 ms |
| Expand 10 px | 42 ms |
| Invert | 4 ms |
| Magic wand sampling all 30 layers | 436 ms |
| Refine preview | 0.78 s |
| Mask from selection | 8 ms |
| Undo | 1 ms |

### Fixed during this milestone

- A deadlock between tile serving and multithreaded edits (tile requests now run off the compute pool).
- The blend-mode dropdown losing its value after a language switch.
- The refine view staying on after the panel closed.

### Not done or not verified in M3

- Painting on a layer mask needs the brush (M4).
- No "view mask alone" (Alt-click) mode yet.
- Quick select is the classic algorithm; the AI-assisted subject tools come in M10.
- Selections are not saved inside `.lawha` files (same as other editors, which don't save them unless stored as a channel).
