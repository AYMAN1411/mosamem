# Mosamem: status by milestone

This file is honest by design: each milestone lists what was verified and how, and what is not
verified.

**Name.** The app was called Lawha (لوحة) through M10. On 2026-10-21 it was renamed **Mosamem
(مصمم)**:
- the app and window name, the bundle id (`app.mosamem.editor`) and the crates (`mosamem-core`);
- the document extension (`.mosamem`, format `mosamem/1`), the `mosamem://` protocol and the
  `MOSAMEM_*` variables;
- a new mark: an initial meem (مـ).

Older notes below use the new name throughout. The Arabic word «لوحة» stays where it means the
canvas («حجم اللوحة»).

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
  `mosamem://`, rendered correctly (`docs/shots/m1-release.jpg`).

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
| **All 27 blend modes**: the 16 requested plus Dissolve, Linear Burn/Dodge, Darker/Lighter Color, Vivid/Linear/Pin Light, Hard Mix, Subtract, Divide | CPU: each formula checked against an independent implementation over a value grid, plus known values, non-separable invariants, alpha compositing and PSD keys (`crates/mosamem-core/tests/blend_formulas.rs`). **GPU vs CPU: 27 modes × 2 opacities, every pixel within ±1/255** (`docs/gpu-blend-m2.json`) |
| **Live GPU preview**: dragging opacity or fill and changing the blend mode recomposites on the GPU from below / layer / above caches, at 60 fps (p50 16.7 ms, p95 19.4 ms). The engine's composite replaces it when ready, with no flash. It falls back to engine updates when the layer's position can't be previewed exactly. | Rust tests: live parts reproduce the composite within ±1; fallback for non-associative stacks |
| History: every edit is a step, with undo (⌘Z) and redo (⇧⌘Z), labels in the UI language in the Edit menu and History panel, and click-to-jump. Steps share unchanged pixels. Limits: 200 steps and a 2 GB memory budget. | Rust tests (undo/redo, transient drags don't add steps, step limit) |
| `.mosamem` native format (SQLite): layer tree, properties, tiles stored once per distinct content (BLAKE3 + LZ4). Saving over the same file is incremental and transactional. | Rust tests (exact round-trip, dedupe, rejecting non-Mosamem files). Harness: photo document round-trips with an identical pixel hash |
| **Autosave + crash recovery**: changes are saved about 2 s after you pause, or at least every 20 s, on a background thread. After a crash, a recovery dialog offers the documents on next launch. A clean quit removes the copies. | **`tools/crash-test.sh`: edit → `kill -9` → relaunch → restore → identical pixel hash (PASS)**; screenshot `m2-recovery` |
| Closing a document or the window with unsaved changes asks Save / Don't Save / Cancel | Screenshot `m2-unsaved-ar` |
| Mixed-direction text: names inserted into Arabic sentences are bidi-isolated | i18n test |

### Measurements (6000×4000, 30 full layers, debug app)

| Operation | Time |
|---|---|
| Save `.mosamem` (84 MB file) | 1.1 s |
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
| **Layer masks**: Reveal or hide all, reveal or hide selection, with the panel button (Alt for the hiding variant). Density, render-time feather, invert, disable (Shift-click), load as selection (⌘-click), apply, delete. Masks on groups, including pass-through. Saved in `.mosamem`. Live GPU previews include the mask. | Rust tests (hide/show, density, disable, feather ramp, apply bakes pixels, save/load round-trip with mask); screenshot `m3-mask-ar` |
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

- Painting on a layer mask needs the brush (done in M4).
- No "view mask alone" (Alt-click) mode yet.
- Quick select is the classic algorithm; the AI-assisted subject tools come in M10.
- Selections are not saved inside `.mosamem` files (same as other editors, which don't save them unless stored as a channel).

## Milestone 4: painting ✔︎ (2026-10-20)

| Feature | How it was checked |
|---|---|
| **Brush** and **eraser** with size, hardness, spacing, flow, opacity and smoothing. Opacity is a ceiling for the whole stroke and flow builds up inside it, as in other editors. Strokes are spaced evenly along the path, with Catmull-Rom curves between samples. | Rust tests (tip profiles, even spacing across batches, opacity ceiling and flow buildup, dabs crossing tile borders, one undo step per stroke); screenshots `m4-paint-*` |
| **Pen pressure** to size and/or opacity, with a minimum size. WebKit on macOS reports a tablet pen as a mouse without pressure, so a native bridge forwards the pressure from the system's tablet events. | Simulated input only (see below); `m4-pressure` |
| **Selections and locks** limit painting: the selection is a soft limit, locked pixels refuse the stroke, locked transparency keeps alpha (the eraser paints the background color). | Rust tests |
| **Painting on layer masks**: black hides, white reveals, gray in between. The eraser paints the opposite. | Rust test (the layer's pixels stay untouched; undo) |
| **Clone stamp** with aligned or fixed source and the option to sample all layers (Alt-click sets the source). | Rust test; demo (a cloned tree in `m4-paint-*`) |
| **Healing brush**: the cloned texture blends into the surrounding colors with a membrane (Poisson) solve. **Spot healing**: fills the brushed area by patch search (PatchMatch), then heals it. | Rust tests (the membrane reproduces a linear ramp exactly; healing keeps texture and matches the surroundings; patch search continues a stripe pattern; spot heal removes a blemish) |
| **Gradient**: linear, radial, angle, reflected and diamond, foreground→background or foreground→transparent, reverse and dither. **Paint bucket** with tolerance, contiguous, all layers. **Eyedropper** (also Alt with the brush). | Rust test (fill, gradient endpoints, sampled color); demo (Soft Light gradient layer in `m4-paint-*`) |
| **Brushes panel** with presets, **color panel** (HSB picker, swap/reset, X/D keys), brush cursor at the real size, `[` `]` for size, Shift+`[` `]` for hardness, number keys for opacity (Shift for flow). | Screenshots; manual runs |
| A stroke shows live on the GPU (the stroke layer over cached parts below and above) and is committed as one history step. | `m4-paint-*`; history panel shows one step per stroke |

### Latency (6000×4000, 30 full layers, 500 px brush, 60 Hz display)

Measured from the pointer event to the frame that shows the new paint, with synthetic pen events every 8 ms. Raw numbers are in `docs/perf-paint-m4.json`.

| Zoom | p50 | p95 | max |
|---|---|---|---|
| Fit (25%) | 25 ms | 35 ms | 43 ms |
| 100% | 14–16 ms | 34–39 ms | 42 ms |

The engine itself paints a batch in 1–6 ms. The rest is waiting for the next display frame, so at 60 Hz the screen trails the pen by one to two frames. The plan's target of under 16 ms from event to screen can't be met at 60 Hz: a frame alone takes 16.7 ms. It is reported here as it was measured.

### Fixed during this milestone

- **Autosave slowed painting down.** It hashed and compressed tiles on the same threads as the brush, so the first autosave of a large document froze strokes for up to 1 s. Autosave now has its own small, low-priority thread pool (utility QoS on macOS, lower priority on Windows and Linux).
- **The display could get stuck after a stroke.** A live tile still queued when the stroke was committed threw an error and stayed "loading" forever, using up one of the parallel download slots. Such tiles are now dropped.
- When a stroke is committed, the new composite now loads behind the stroke's last tiles, so the final dabs don't arrive late.
- Clone sampled the active layer instead of the painted one.
- A stale tile could be marked fresh by an older request that finished late (tiles now carry an epoch).
- The Brushes panel showed a horizontal scrollbar.

### Not done or not verified in M4

- **Real pen pressure is not verified**: there is no tablet to test with. Pressure was tested with simulated input on both paths: the browser's pen events and the native bridge (`tools/pressure-test.js`, size grows from 10 px to 58 px on both).
- **Performance on a real Windows PC is not verified**: there is only a Windows 11 ARM VM.
- No brush tip shapes beyond round (no textured, scattered or angled tips), and no mixer brush, smudge or dodge/burn.
- The latency above was measured on a dev build with optimized code. The release build (LTO) should be the same or faster, but it was not measured separately.

## Milestone 5: adjustments and filters ✔︎ (2026-10-20)

| Feature | How it was checked |
|---|---|
| **Adjustment layers**: Levels, Curves, Hue/Saturation, Brightness/Contrast, Exposure, Vibrance, Color Balance, Black & White, Gradient Map, Invert, Threshold. Added from the Adjustments panel, the Layers panel button or Layer › New Adjustment Layer. The selection becomes the layer's mask. Opacity, fill, blend modes, masks, clipping (only the base layer is changed) and groups (an isolated group keeps the change inside, a pass-through group lets it through) all work. Saved in `.mosamem`, and every change is one history step. | Rust tests: known input → output for every kind; defaults leave every color unchanged; scope in groups and clipping; blend modes; selection → mask; save/load round trip; histogram |
| **Properties panel editors**: Levels (per channel, histogram, draggable black/gray/white handles, output range), Curves (up to 14 points, histogram behind, per channel; drawn with the same monotone spline as the engine), Hue/Saturation (master and six hue ranges, colorize), Brightness/Contrast (with legacy), Exposure, Vibrance, Color Balance (three tones, preserve luminosity), Black & White (six weights, tint), Gradient Map (color stops, reverse, dither), Threshold (with histogram). | Screenshots `m5-curves-*`, `m5-levels-ar`; TypeScript test for the spline |
| **Live preview while dragging**: the GPU applies the adjustment (a GLSL port of the engine's math, fed the numbers the engine compiles) to the cached part below the layer. Where the tree can't be split exactly (an adjustment under another adjustment, a clipped adjustment), the engine recomposites instead. | GPU vs CPU: 16 settings × 4 blend modes, all within ±1/255 (`docs/gpu-adjust-m5.json`). The canvas during a drag and after release: identical at 100% zoom. At 25% zoom the mean difference is 0.2 levels, and about 1% of pixels differ by more than 2 (the preview adjusts colors after the zoomed-out view blends neighbouring pixels, not before). |
| **Filters**: Gaussian Blur, Motion Blur, Unsharp Mask, Add Noise, Mosaic (Filter menu). A floating panel with a live preview on the canvas at the zoom shown, computed tile by tile. Applied through the selection (soft edges included); locked pixels refuse, and locked transparency keeps alpha. Also works on layer masks (without a live preview). The canvas edge repeats, so blurring a full layer doesn't fade its border. | Rust tests (10 core, 6 engine): flat areas stay flat, Gaussian σ is checked by variance, transparent neighbours don't darken colors, unsharp threshold, noise is repeatable and tied to image coordinates, mosaic cells align to the canvas, tile-by-tile results equal whole-image results, the preview under an adjustment layer equals the applied result; screenshots `m5-blur-preview`, `m5-motion-ar` |

### Speed (6000×4000, 30 full layers, 60 Hz)

| Operation | Time |
|---|---|
| Adjustment slider, GPU preview | one frame per change: p50 16.6 ms, p95 18–20 ms (60 fps). The first change of a drag takes 0.15 s (100%) to 0.6 s (fit) to build the cached parts. |
| Adjustment slider, engine fallback | 65 ms (100%) to 235 ms (fit) per change |
| Gaussian blur on one full 24 MP layer | 146 ms (radius 4), 515 ms (radius 50), 323 ms (radius 250) |
| Unsharp mask / add noise / mosaic | 139 / 109 / 19 ms |
| Motion blur, 40 px | 1.2 s |

Raw numbers: `docs/perf-adjust-m5.json`, `docs/perf-filter-m5.json`.

### Fixed during this milestone

- Filters had no preview on a layer below an adjustment layer: the GPU split can't hold an adjustment above the layer. Such previews now use a whole-document session, where the engine composites everything per tile with the layer's pixels replaced by the filtered ones.
- Long-reaching filters ran tile by tile with large margins (1.3 s for a radius-50 blur). Past a 32 px reach they now run over the whole canvas at once (0.5 s).
- Test scripts: when Mosamem's window is behind other windows, WebKit slows its timers and replies to about 500 ms per step even with background throttling turned off. Timings in this report were all taken with the window in front.

### Not done or not verified in M5

- Several formulas are our own definitions, marked ✱ in `docs/research.md`: modern Brightness/Contrast, the Hue/Saturation and Vibrance saturation curves, Color Balance tone weights, the Black & White mix, the Curves spline (monotone, where Photoshop's may overshoot), the blur radius → σ mapping and the noise amounts. They are tested against their own formulas, not against Photoshop. Calibration comes with the Photoshop-made test files in M9.
- No destructive Image › Adjustments yet (adjustment layers only), no presets, no eyedroppers or Auto in Levels and Curves, no dragging on the image to edit a curve, and gradient stops are edited in a list rather than on the bar.
- No smart (re-editable) filters. Filters change only pixels inside the canvas, and on masks there is no live preview.
- Heavy motion blurs are slow: a 150 px preview over the whole photo took 6 s to fill in at fit.
- A long filter shows "Applying…" without a progress bar.

## Milestone 6: transform and resize ✔︎ (2026-10-20)

| Feature | How it was checked |
|---|---|
| **Move tool**: drag the selected layers (or, with a selection, only the selected pixels; the selection comes along). Arrow keys nudge by 1 px (Shift: 10). Moves are whole pixels and exact. | Rust tests (exact in every direction, one history step, selection-only moves leave a hole and keep the rest); scripted drag in the app |
| **Free Transform** (⌘T): scale (corners keep proportions, Shift frees them), rotate (Shift snaps to 15°), skew (⌘⇧ side), distort (⌘ corner), perspective (⌘⌥⇧ corner), a 9-point reference point, numeric X, Y, W, H and angle, flip and quarter-turn commands, and the interpolation method (nearest, bilinear, bicubic, Lanczos). One homography, resampled once on Enter. Masks follow their layers. | Rust tests; screenshot `m6-transform-en` |
| **Live preview**: the GPU draws the moving layer through the transform matrix over cached parts. For a layer under an adjustment layer the position is exact, but the moving layer is drawn without the adjustment until it lands. | Rust test (the floating selection splits correctly; the fallback session starts); speed below |
| **Crop tool**: ratio presets (free, original, 1:1, 4:5, 5:4, 2:3, 3:2, 16:9, 9:16), W/H and angle fields, rule of thirds, a shaded outside, rotate by dragging outside, Straighten (draw a line), delete or keep cropped pixels. A turned crop shrinks to stay inside the image. Image › Crop to Selection. | Rust tests (keep vs delete, a straightened crop fully covered); screenshot `m6-crop-ar` |
| **Image Size** (nearest, bilinear, bicubic, Lanczos, or resolution only), **Canvas Size** (absolute or relative, 3×3 anchor, color for the new area of the bottom layer; pixels outside are kept), **Image Rotation** (180°, 90° both ways, flips; exact). | Rust tests; screenshots `m6-imagesize-ar`, `m6-canvassize-en` |
| **Resampling core**: separable kernels, widened when shrinking; colors interpolated with their alpha. | Rust tests: kernel weights against closed forms (Keys a = −0.5, Lanczos-3 = sinc·sinc), weights sum to one, constant images stay constant, a checkerboard shrinks to flat gray (no aliasing), identity and whole-pixel moves change nothing, quarter turns land exactly, homographies hit their corners, no dark fringes at transparent edges |

### Speed (6000×4000, 30 full layers)

| Operation | Time |
|---|---|
| Free Transform preview | starts in 71 ms, then one frame per change (p50 16.6 ms, max 24 ms) |
| Move one layer by whole pixels | 8 ms |
| Rotate 15° and scale 90%, one layer | 157 ms (bicubic), 357 ms (Lanczos) |
| Image size to 3000×2000, all layers | 0.8 s |
| Canvas size, all layers | 0.6 s |
| Crop with a 3° straighten, all layers | 3.6 s |
| Rotate the image 90°, all layers | 0.9 s |

Raw numbers: `docs/perf-transform-m6.json`.

### Fixed during this milestone

- Whole-pixel moves went through one dense copy of the layer (382 ms); they now copy tile by tile in parallel (8 ms).
- A straightened crop brought in empty corners; it now shrinks to fit inside the image.

### Not done or not verified in M6

- The transform preview doesn't show layer pixels that lie outside the canvas until the transform is applied. Moving several layers at once shows only the outline until release.
- Warp, puppet warp, content-aware scale and perspective crop aren't there. The crop tool has no content-aware fill for empty corners.
- Image Size resamples each layer in one piece (about 1 GB of working memory for a 24 MP layer). Very large documents may need band-by-band resizing later.
- A pressure pen and the Windows build were not involved in this milestone.

## Milestone 7: Arabic-first text ✔︎ (2026-10-20)

| Feature | How it was checked |
|---|---|
| **Shaping** with harfrust (the HarfBuzz project's Rust port): joining forms, lam-alef, marks (shadda, fatha, tanween, wasla), contextual alternates (Amiri), mirrored parentheses. | Rust goldens with the fonts' own glyph names; **the whole corpus shaped again by HarfBuzz 14.6.0 itself (harfbuzzjs): 60 of 60 cases identical** in glyph ids, clusters, advances and offsets, across five font files including variable Cairo (`docs/shaping-oracle.json`, `tools/shaping-oracle.mjs`) |
| **Bidi** (UAX #9, unicode-bidi): right-to-left, left-to-right or automatic paragraphs; Latin and numbers inside Arabic read correctly. | Rust tests (positions of «مرحبا Hello 123», «عام 2026», direction from the first letter) |
| **Line breaking** (UAX #14, icu_segmenter) for box text; **alignment** start/center/end/justify; **justification** by spaces, or by **kashida** (tatweel, one point per word, ALREQ's preferences simplified) with short/medium/long, re-measured so contextual fonts still fill the line. | Rust tests (every justified line exactly the box width, in Plex and in Amiri); text sheet |
| **Digits**: Western, Arabic-Indic, Persian. **Letter spacing**, **line spacing** (auto = 120%). | Rust tests |
| **Bundled fonts** (SIL OFL, inside the program): IBM Plex Sans Arabic (7 weights), Cairo (variable 200–1000), Tajawal (7 weights), Amiri (regular, bold, italics). Characters a font lacks come from another bundled font, a whole word at a time so its letters still join. | Rust tests (every font loads; Tajawal's missing wasla and Persian digits filled in) |
| **Text layers**: click for point text or drag for a text box; type on the canvas in a native editor (caret, selection, input methods, mixed Arabic/English) shown in the same font, size and placement; ⌘Enter or a click outside commits, Esc cancels. Click a text layer (or double-click its T) to edit it again. The options bar sets font, weight, size, color, alignment, direction, spacing, digits and kashida, for the text being typed or the selected text layer. Moving, scaling, turning and image-wide changes keep text editable; distortions and painting need Layer › Rasterize Type. Saved in `.mosamem`. | Rust tests (render, edit, undo, name follows text, move keeps text, perspective rasterizes, save/load); the editor and the committed text compared on screen (`m7-editing-ar` vs `m7-committed-ar`: the same) |

Screenshot: `docs/shots/m7-textsheet-ar.jpg` (each font, mixed text, digits, vowelled Amiri, kashida vs space justification).

### Fixed during this milestone

- Painting refused every layer that wasn't a pixel layer, even on its mask, so adjustment-layer masks couldn't be painted (since M5). Masks of any layer can now be painted.
- Kashida in Amiri: tatweels change the letters around them, so the first try overflowed and the line was left ragged. Justification now measures, retries with fewer kashidas, and finishes with spaces.

### Not done or not verified in M7

- One style per text layer (no mixed fonts, sizes or colors inside one layer).
- Text on a path, vertical text, OpenType feature toggles (discretionary ligatures, stylistic sets), and per-character baseline shift aren't there.
- Box text doesn't clip what overflows the box.
- Kashida placement follows a simplified version of ALREQ's priorities; it is not checked against Photoshop.
- Engine error messages (for example "rasterize the text layer to paint on it") are in English only.

## Milestone 8: layer styles ✔︎ (2026-10-20)

| Feature | How it was checked |
|---|---|
| **Effects**: drop shadow (angle, distance, spread, size, knockout), inner shadow (choke), outer glow (softer or precise), stroke (outside, inside, center), color overlay, gradient overlay (linear, radial, reflected; angle, scale, reverse). Each has a blend mode and opacity and can be switched off without losing its settings. | Rust tests on the mattes: the shadow falls opposite the light, size blurs and spread hardens, knockout, the three stroke positions, glow and inner shadow on their side, gradient positions |
| **Compositing**: shadows and glow on what is below; the layer's pixels at the fill opacity, then inner shadow, overlays, clipped layers and stroke, blended with the layer's mode; the layer's opacity fades everything. Fill doesn't affect effects, and the layer mask shapes them. Works on pixel and text layers (text stays editable). Saved in `.mosamem`. | Rust tests (shadow beside the layer, fill 0 still shows overlay and shadow, opacity halves the shadow, stroke follows the mask, undo, save/load) |
| **Layer Style panel** with a live preview; the fx button and badge in the Layers panel; Layer › Layer Style… and Clear Layer Style. | Screenshots `m8-style-ar` (panel), `m8-styled-ar` (an Arabic title with gradient, stroke and shadow) |
| Effect mattes are computed per tile from a margin around it, at the zoom level shown, and cached by the layer's pixels, mask and style, so edits to other layers don't recompute them. | `docs/perf-style-m8.json`: on 6000×4000 with 30 layers, adding a size-40 shadow and a stroke redraws the view in about 0.18 s at fit and under 0.1 s at 100% |

### Not done or not verified in M8

- Groups can't have effects yet; layers clipped to a base don't get their own effects.
- Inner glow, satin, bevel and emboss, pattern overlay, contours, noise, and a global light angle aren't there.
- Effects blend inside the layer (interior effects) or onto the backdrop (shadow, glow) with our own simplified order; not compared with Photoshop yet (M9's test files will show the differences).
- Effects are drawn by the engine, not the GPU: dragging an effect slider updates about 4–5 times a second at fit on very large documents.
- A layer with effects can't use the GPU paint preview, so painting on it recomposites in the engine.

## Milestone 9: PSD ✔︎ (2026-10-20)

| Feature | How it was checked |
|---|---|
| **Open PSD/PSB** (File › Open, or drop): layers, groups (and pass through), blend modes, opacity and fill, clipping, locks, layer masks (density, feather, disabled), adjustment layers Mosamem has (Levels, Curves, Hue/Saturation, Brightness/Contrast, Exposure, Vibrance, Color Balance, Black & White, Gradient Map, Invert, Threshold), layer styles Mosamem has (kept even when switched off), text as editable text layers (Photoshop's pixels shown until edited). Vector masks become pixel masks; shape layers saved without pixels are drawn from their paths; 16/32-bit files become 8-bit (32-bit linear light encoded to sRGB). A file whose layer section is damaged opens flattened. The file is read by ag-psd (MIT) in a worker, so the window stays responsive. | `tools/psd-report.mjs` over the 90 public psd-tools test files: 87 open; **50 match Photoshop's own flattened image** (≥ 99.9% of pixels within 1 level), 8 are close (≥ 99% within 8 levels), 20 differ, 9 carry no image to compare with. Details: `docs/psd-report.md` |
| **Save a Copy as PSD** (File menu): layers, groups, masks, adjustment layers, styles and text, with a flattened image for other programs. | Same tool: each opened file is saved as PSD and opened again; **87 of 87 keep their layer structure and render identically** |
| Whatever isn't carried over is listed when the file opens, in the interface language. CMYK files get a clear message. | Screenshots `m9-psd-ar` (vector mask and text, Arabic warning), `m9-psd-en` (groups, pass through, clipping) |
| Fixed on the way: effect caches were keyed by memory address only, so a closed document's freed address could hand a new layer stale shadows or gradient bounds. Every such cache now holds a weak handle that keeps the address from being reused. | Was intermittent in the PSD round trip (2 of 87 files); 87 of 87 in two runs in a row after the fix |

### Not done or not verified in M9

- Not tested with files from a real, current Photoshop: the test files come from one open-source project. `docs/psd-checklist.md` lists the files to make and how to check them.
- Mosamem's adjustment formulas are its own (from Adobe's public help pages); files with adjustment layers differ from Photoshop, sometimes a lot (`fill_adjustments`: mean 53 levels). They are the calibration data for later.
- Not read: CMYK, Lab and other non-RGB modes; inner glow, bevel and emboss, satin, pattern overlay; gradient and pattern strokes; gradient and pattern fill layers saved without pixels; "blend if"; knockout; artboards; layer comps; smart object contents (kept as pixels); vector paths (kept as pixel masks, no path editing).
- 16/32-bit files lose their depth (8-bit only).

## Milestone 10: local AI ✔︎ (2026-10-21)

| Feature | How it was checked |
|---|---|
| **Select Subject** (Select menu, and a button on the Object Selection, Quick Select and Magic Wand options bar), **Remove Background** (Layer menu and the same bar: adds a layer mask, so the edit can be undone, changed or deleted), **Object Selection tool** (W: click an object, or drag a box around it; Shift adds, Alt subtracts; "sample all layers"), **Smart Fill** (Edit › Smart Fill…, Shift+Backspace: AI or classic, on a new layer by default). Results go through Mosamem's guided-filter edge snap at full size. | Rust tests: Smart Fill in place and on a new layer (the square is gone, other pixels untouched, undo), errors when a model is missing. `tools/ai/measure.sh` → `docs/ai-measure-m10.json`, 43 photos at 512×512, all on the GPU. Medians: Select Subject 2.7 s, SAM picture prep 0.18 s, then 17 ms per click, MI-GAN fill 0.16 s, classic fill 0.07 s. A box drawn around the subject gives the subject's own mask (IoU ≥ 0.9) in 21 of 43; median 0.89. MI-GAN beats the classic fill in 23 of 43 by PSNR against the hidden pixels, which is a weak measure for fills. Screenshots `m10-object-ar` (one click on the parrot), `m10-download-ar`, `m10-fill-en`, `m10-before-en` / `m10-filled-en` (Select Subject, expand 8 px, Smart Fill: the penguin is gone and the sky continues), `m10-models-ar` |
| **Models download on first use.** A dialog says what the model is, its size, license and source. Downloads are pinned to a commit and checked by SHA-256, go to a temporary folder first, and can be cancelled. Edit › AI Models… lists them, with Delete. Sources: BiRefNet lite (MIT, 115 MB), SAM 2.1 tiny (Apache-2.0, 88 MB), MI-GAN (MIT code and weights, 28 MB). | Rust tests against a local server: a good download installs and deletes, a wrong checksum and an unreachable host install nothing and leave no partial files. Real downloads in the app: 115 MB in 4 s, 88 MB in 6.5 s, 28 MB in 1.9 s |
| **Runs on the GPU** through ONNX Runtime's WebGPU provider (Dawn on Metal), falling back to the CPU if a model can't load or run there. Core ML was measured and dropped (docs/research.md §10). BiRefNet's 10–32-way Split/Concat nodes are split into trees after download (src-tauri/src/ai/onnx_wide.rs), so they fit the GPU's buffer limits. | A Rust test runs a model with 12- and 20-way nodes before and after the rewrite: identical output. BiRefNet on the GPU vs the CPU: 99.9–99.99% IoU on 6 photos |
| **No network after the download.** | `tools/ai/no-network.sh` → `docs/ai-no-network-m10.json`: (1) the AI code runs install + all three models on photos inside a macOS sandbox that denies every outbound connection, and a real download in that sandbox fails (the control); (2) the app runs every AI feature while its sockets are sampled every 50 ms: no connection to another host |

Fixed on the way:
- SAM's mask was flat. The image library clamps float pixels to 0–1 when resizing, which flattened the logits. Squashing to 0–1 first fixed it; a test now checks that a resized matte stays sharp. An early check had missed this: it compared masks at 0.5, and a flat mask above 0.5 still matched subjects that fill the frame.
- MI-GAN's pipeline returns the outermost hole pixels as they went in. The hole is now grown by about two model pixels before the model runs.
- ONNX Runtime's worker threads busy-wait between runs, holding cores the GPU path needs. They are turned off; the measuring run went from 372 s to 85 s.
- Smart Fill on a new layer sampled only the active layer, so a second fill copied the (empty) first one. It now samples everything visible.
- The AI status call ran on the app's main thread and waited for any running model, while the GPU path needs the main thread: with the Object Selection tool reading the picture in the background, a Smart Fill from the interface stalled for over two minutes, twice. The status call now runs off the main thread and never waits; the same steps then took 7 s and 2 s.

### Not done or not verified in M10

- **Speed was measured on a busy machine** (load average 10–14: an Android emulator and a background Neural Engine compile were running). On a 6000×4000 picture, Select Subject took 5–7 s and Remove Background 5.6 s, of which the model is 2.4–4.4 s. Not measured on an idle machine or on any other Mac.
- **Quality was measured against our own models and by eye.** There is no ground truth. The planned check against Apple's Vision subject mask is still to do: the Neural Engine was busy, and Vision waits for it.
- A box drawn around a whole frame-filling picture is ambiguous to SAM. It picked the background for the guitar photo.
- **Windows not tried yet** (M11): WebGPU through Dawn on Direct3D 12, with the CPU fallback. The Windows 11 ARM VM may have no usable GPU.
- **Packaging** (M11): the app links `libwebgpu_dawn.dylib` (9 MB), which has to go into the app bundle's Frameworks folder.
- Models come from their upstream Hugging Face repositories (pinned commits), not from Mosamem's own GitHub Releases as planned: there is no Mosamem repository yet. `MOSAMEM_MODEL_MIRROR` can point the app at another source.
- ONNX Runtime crashed once in three test runs while tearing down a session (a malloc trap in `~InferenceSession`). Mosamem now never destroys a session while it runs: replaced or deleted ones are parked until the app quits. The cause inside ONNX Runtime is not known.
- Object Selection uses one click or one box per selection. There is no refining with several points on one object.

## Milestone 11: export, reference, installers (Mac ✔︎, Windows partly) (2026-10-21)

| Feature | How it was checked |
|---|---|
| **Export**: the whole picture, one layer (trimmed to its pixels) or the selection (trimmed to it); any size with the aspect kept, a percentage, or a web preset (long side 3840/1920/1280/1080/800); the file's size and pixel size shown before saving. | Rust tests: a layer trims to its content, the selection is cut out, a resize scales the resolution, oversized requests are refused. Screenshot `m11-export-ar` |
| **Help › Keyboard Shortcuts** (⌘/): tool letters from the tool list itself, single keys, menu shortcuts, shown the platform's way (⌘⇧S / Ctrl+Shift+S). | A test checks every listed menu shortcut against `menu.ts` (same key, same action). Screenshots `m11-shortcuts-en`, `m11-shortcuts-ar` |
| **Icon and mark**: an initial meem (مـ) as one brush stroke on a graphite tile (`assets/icon.svg`), rendered for macOS and Windows. | By eye, in the Dock, the window and the Windows taskbar |
| **README** in English and Arabic: what it does, the unsigned-install steps (macOS "Open Anyway", Windows "More info → Run anyway"), building from source, privacy. `LICENSE` (GPL-3.0) and `THIRD-PARTY-NOTICES.md` (fonts under OFL, ONNX Runtime, Dawn, ag-psd, the AI models' licenses). Hero picture made in Mosamem itself. | Read through; `docs/media/hero-ar.jpg`, `hero-en.jpg` |
| **Mac installer** `Mosamem.dmg` (24 MB, Apple silicon, macOS 13.4+): the app bundles Dawn's dylib in Frameworks. | Built and opened from the bundle. A harness build of the same bundle ran Select Subject, Object Selection, Smart Fill and Remove Background, all on the GPU |
| **Windows installer** `Mosamem-Setup-x64.exe` (24 MB): cross-built on the Mac (cargo-xwin); the installer itself is made by NSIS in a Windows VM (`tools/windows/make-installer-vm.sh`, since Homebrew's makensis crashes on macOS 27). It carries DirectML.dll; AI runs through DirectML or the CPU. | Installed silently in the Windows 11 ARM VM (as x64, emulated), launched: Arabic interface, menus, icon. A harness build there downloaded the three models and ran all four AI features (on the CPU: the VM's GPU has no DirectML) |

Fixed in this milestone:
- **The Text tool couldn't type** (since M7). The editor focused itself once, when the canvas was built, so the editor that a click opened had no keyboard focus. Space didn't type either: the canvas took it for panning, because it didn't count the editor as a text field. Also:
  - A very quick click could open nothing.
  - While typing, the menu's Undo, Redo and Select All acted on the document.

  M7's checks set the text through the test harness and never typed. `tools/text-typing.js` now follows the user's path: a quick click, focus, keys reaching the editor, typing, committing, re-editing. The old code fails it.
- A fresh checkout's dev build looked for Dawn's dylib in the release folder. `build.rs` now copies it (and DirectML.dll on Windows) from ONNX Runtime's build output.

### Not done or not verified in M11

- **Windows is only partly tested.** Not tried there:
  - painting, layers, text, filters, files, PSD and export;
  - the installer's visible wizard, its Start menu shortcut and uninstalling;
  - the Visual C++ runtime prompt;
  - DirectML on a real GPU;
  - a real x64 PC.

  The x64 installer in `release/` predates the Text tool fixes: rebuild it before sharing. Windows work is paused at the user's request.
- **No Windows arm64 build.** ONNX Runtime's prebuilt for arm64 has DirectML only, and the cross-compiler couldn't build libwebp's NEON code with MSVC's headers. The x64 build runs on ARM PCs through emulation.
- **No Intel Mac build.**
- **Not tested with real keyboard and mouse input.** The tests drive the interface from inside the page: the agent has no Accessibility permission to post events. The user's own typing confirmed the Text tool fixes.
- **Not run: the GitHub Actions workflow and the release links in the README.** The repository isn't on GitHub yet; the links assume `motalim-ai/mosamem`.
- **Windows menu bar order.** The native menu bar runs left to right even in Arabic: Tauri's menus have no right-to-left mode.

## Milestone 12: vector shapes and the Pen ✔︎ (2026-10-21)

| Feature | How it was checked |
|---|---|
| **Shape layers** that stay editable: rectangle (with rounded corners), ellipse, polygon, star, line with arrowheads, and free paths. Fill (solid or none) and stroke (width, inside/center/outside, solid/dashed/dotted). Moving and scaling keep a rectangle or an ellipse live; turning makes it a free path. They take layer styles, save in `.mosamem`, and can be rasterized. | Rust tests: exact areas (rectangle, πr², rounded corners, hexagon), the stroke band for each alignment, dashes, arrowheads, the ellipse within 0.03 px of the circle, transforms, undo |
| **Shape tools** (U): drag to draw, Shift for a square or a 45° line, Alt from the center, a click for a 100 px shape. The options bar sets the tool's and the selected layer's fill, stroke and shape settings. The Properties panel edits geometry as numbers. | `tools/shape-tools.js` (14 checks) with pointer events. Screenshots `m12-shapes-ar`, `m12-shapes-en` |
| **Pen** (P): click for corners, drag for curves, close on the first point or finish with Enter/Esc; Shape or Path mode. Clicking the active path's outline adds a point; clicking a point removes it. **Direct Selection** (A): select (Shift adds), move points and segments, pull handles (smooth points keep them in line, Alt breaks them), Delete. | `tools/pen-tools.js` (11 checks). Screenshot `m12-pen-ar` |
| **Paths and selections**: Make Selection from a path, Path from Selection (traced edge, simplified to 2 px, gentle bends smooth), Fill Path and Stroke Path on a pixel layer in the foreground color. | Rust tests: a rectangle's selection and its traced path match exactly; a disc traces back within about a pixel; holes wind the other way. `tools/path-ops.js` (8 checks). Screenshot `m12-paths-ar` |
| **PSD**: Photoshop shape layers open as editable shapes when Mosamem draws them the same way (solid fill or none, solid stroke, paths that only combine, no raster mask). Live rectangles and ellipses stay live. Saving writes the vector mask, fill, stroke and the live-shape record. | The 90 public test files: every picture as close as before or closer (`pen-text.psd` 89% → 97.6% within one level), and about 15 files no longer list shapes as flattened (`docs/psd-report.md`). `tools/psd-shapes.js` (10 checks) writes four shapes, reads the file raw with ag-psd and opens it again |

Fixed in this milestone:
- **The paint bucket failed** on any canvas whose size isn't a multiple of 256 (since M4). Fills ran into the last tiles' spare pixels past the edge. Fills now stay on the canvas; a test covers it.
- The engine read the fields inside shape variants in snake_case, so lines from the interface were refused. `tools/shape-tools.js` found it.

### Not done or not verified in M12

- **Not opened in Photoshop.** PSD files Mosamem writes were only read back by ag-psd and Mosamem, so whether Photoshop shows their shapes as live is not verified.
- **PSD limits.** These still open as pixels with a vector mask: gradient or pattern fills, subtracted or intersected paths, and shapes with a raster mask. A line's arrowheads aren't written to PSD: the line is saved as an open path with a stroke.
- **Not done yet**: boolean operations between shapes, custom shapes, text on a path, a Paths panel. The work path isn't saved with the document and isn't in the history.
- Path from Selection makes straight segments with smooth points, with no real curve fitting, so a circle comes back with about 30 points.
- Stroke Path uses a round brush of the Brush tool's size, without pressure or the brush's softness.
- Same limits as before: no real keyboard or mouse input (the tests send pointer events from inside the page), no Windows run.
