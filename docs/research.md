# Lawha: clean-room research

This file records, for every v1 feature, **how it should behave** and **where we learned it**.
Rules followed while writing it:

- Sources are public only: Adobe's public help pages, the public Photoshop File Formats
  Specification, W3C / ISO / Unicode specs, academic papers, and public documentation of open-source
  editors (GIMP, Krita) and other public write-ups.
- No Adobe software was opened, run, disassembled or instrumented. Photoshop being installed on this
  Mac changes nothing.
- Everything is paraphrased in our own words. Math formulas are restated, since a formula is a fact.
- No code from GIMP, Krita or any other project is copied. We learn ideas only.

**Status tags**
- **✔︎ fetched**: the page was loaded and read.
- **◐ indexed**: the URL exists and search shows its content, but the site blocked automated loading.
  This applies to all `helpx.adobe.com` pages and the Adobe PSD spec on curl; WebFetch loaded the spec.
- **✱ unverified**: from general knowledge or a third-party write-up. Must be checked against real
  files or a test before we rely on it.

Research date: 2026-10-20.

---

## 0. Primary references

| Ref | Source | Status |
|---|---|---|
| PSD | Adobe Photoshop File Formats Specification (Nov 2019 edition): https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/ | ✔︎ fetched (WebFetch) |
| W3C-CB | W3C Compositing and Blending Level 1: https://www.w3.org/TR/compositing-1/ | ✔︎ fetched |
| PDF-BM | Adobe PDF 1.6 Blend Modes addendum: https://printtechnologies.org/standards/files/pdf-reference-1.6-addendum-blend-modes.pdf · ISO 32000-1 §11.3.5: https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf | ✔︎ / section number ✱ |
| KHR-ADV | KHR_blend_equation_advanced: https://registry.khronos.org/OpenGL/extensions/KHR/KHR_blend_equation_advanced.txt | ✔︎ |
| KRITA-BM | Krita blending modes manual: https://docs.krita.org/en/reference_manual/blending_modes.html | ✔︎ |
| SUCAN | Blending-mode maths write-up: https://marius.sucan.ro/blog/2023/blending-modes-maths/ | ✔︎ |
| PEGTOP | Soft light analysis: http://www.pegtop.net/delphi/articles/blendmodes/softlight.htm | ✔︎ |
| WIKI-BM | https://en.wikipedia.org/wiki/Blend_modes | ✔︎ |
| PSDTOOLS | psd-tools docs (MIT, used only as reference docs and as a test oracle): https://psd-tools.readthedocs.io/en/latest/ | ✔︎ |
| ALREQ | W3C Arabic & Persian Layout Requirements: https://www.w3.org/TR/alreq/ | ✔︎ |
| KRITA-STROKES | Krita strokes framework design doc: https://docs.krita.org/en/untranslatable_pages/strokes_documentation.html | ✔︎ |
| GEGL | GEGL features (tiles, mipmaps, swap): https://gegl.org/features.html | ✔︎ |
| PHOTOPEA | Photopea WebGL notes: https://blog.photopea.com/photopea-1-3.html, https://blog.photopea.com/photopea-1-4.html | ✔︎ |

---

## 1. Documents

### 1.1 File formats
**Behavior**
- Open and save PNG, JPEG, WebP and TIFF.
- HEIC on macOS only.
- PSD (section 13).
- Embedded ICC profiles are honored on open: pixels are converted to sRGB. Exports are tagged sRGB.

**Technique**
- Rust `png`, `tiff` (Deflate/LZW/PackBits) and `jpeg-encoder` crates (pure Rust; the IJG notice is
  required).
- JPEG decoding via `zune-jpeg`, through `image`.
- WebP: `image` only encodes WebP losslessly, so lossy quality uses the `webp` crate (libwebp,
  BSD-3).
- HEIC: Apple ImageIO through `objc2-image-io` (`CGImageSource` / `CGImageDestination`). It uses the
  system codec, so there is no HEVC licensing for us.
- libheif is avoided: its LGPL/GPL codec dependencies.
- ICC: `lcms2` (Little CMS, MIT).
- Layered TIFF (Photoshop tag 37724) is not supported. TIFF opens flattened.

**Source**
- Crate docs on docs.rs, checked with the crates.io API on 2026-10-20.
- `image` WebP encoder limitation: https://docs.rs/image/latest/image/codecs/webp/struct.WebPEncoder.html ✔︎

**Test**
- Round-trip each format with 8-bit, alpha and ICC-tagged inputs.
- Decoded pixels must equal the originals (lossless formats) or meet a PSNR floor (lossy).

### 1.2 New document presets
**Behavior.** Presets are plain pixel sizes, plus a custom option.

| Preset | Size | Notes |
|---|---|---|
| Instagram post | 1080×1350 | 4:5. Also 1080×1080. |
| Instagram story | 1080×1920 | |
| YouTube thumbnail | 1280×720 | |
| A4 | 2480×3508 at 300 ppi | |
| Custom | width × height | units px/mm/cm/in, resolution, background white/black/transparent |

**Source.** Platform sizes are general public knowledge ✱, rechecked at implementation time. A4 is
ISO 216 (210×297 mm).

### 1.3 Autosave and crash recovery
**Behavior (reference)**
- Adobe's help describes an "automatically save recovery information every N minutes" preference
  (default 10 minutes).
- Recovery files are separate from the user's file.
- Recovered documents reopen on next launch.
- Source: https://helpx.adobe.com/photoshop/desktop/save-and-export/save-files/file-saving-properties-and-preferences.html ◐
  and https://helpx.adobe.com/photoshop/kb/file-recovery-photoshop.html ◐

**Our design (stricter)**
- Incremental autosave of dirty tiles only.
- Saves after 10 s idle, or at least every 60 s while editing.
- Written to a per-document SQLite journal in app data. Transactions keep it crash-safe.
- A lock file marks an unclean exit. On next launch, a recovery dialog lists the documents.
- Recovery never overwrites the user's file.

**Test.** Scripted edit → `kill -9` → relaunch. The recovered document must match the pre-kill state
as of the last autosave, compared pixel for pixel.

---

## 2. Layers

### 2.1 Basics
**Behavior**
- Unlimited layers.
- Groups.
- Visibility, lock, rename, duplicate.
- Merge down, merge visible, flatten.
- Drag to reorder, drag in and out of groups.

**Locks**
- Transparent pixels: painting is limited to existing alpha.
- Image pixels: no painting.
- Position: no moving.
- All: everything above.

Source: https://helpx.adobe.com/photoshop/desktop/create-manage-layers/transform-manipulate-layers/lock-layers.html ◐

### 2.2 Opacity vs fill
**Behavior**
- Opacity (0–100%) fades the layer **and** its styles.
- Fill (0–100%) fades only the layer's own pixels. Shadows, strokes and other styles stay at full
  strength.
- Groups have opacity only.
- ✱ Community sources say lowering Fill changes the blend math itself for 8 modes: Color Burn,
  Linear Burn, Color Dodge, Linear Dodge, Vivid Light, Linear Light, Hard Mix, Difference.
  Opacity only fades the finished result. Adobe doesn't document this. We implement the plain model
  first and compare with test PSDs (section 13).

Sources:
- https://helpx.adobe.com/photoshop/using/layer-opacity-blending.html ◐
- https://helpx.adobe.com/photoshop/desktop/create-manage-layers/apply-layer-effects/use-advanced-blending-options.html ◐

### 2.3 Groups and isolation
**Behavior**
- New groups default to **pass-through**: the group has no blending of its own, and each child blends
  straight into what is below the group.
- Any other mode isolates the group. Children composite onto transparent black first, and the result
  is blended as one layer.
- In an isolated group, adjustment layers inside it affect only the group's contents.
- In our UI this mode is called "Through" / «عابر».

Sources:
- Adobe groups page: https://helpx.adobe.com/photoshop/using/create-layers-groups.html ◐
- W3C-CB §10.3, "Group invariance and isolation" ✔︎

### 2.4 Clipping masks
**Behavior**
- A clipped layer only shows where the base layer (the lowest layer of the clip stack) has alpha.
- Clipped layers must sit directly above the base.
- They take the base's opacity.
- With "blend clipped layers as group" on (the default), the clip stack blends with the base's mode.
  When off, each clipped layer keeps its own mode.
- PSD: clipping byte 1 = clipped. `clbl` = blend-clipped-as-group flag.

Source: https://helpx.adobe.com/photoshop/desktop/create-masks/layer-masks/create-and-manage-clipping-masks.html ◐

### 2.5 Blend modes

**Compositing (W3C-CB §6, ✔︎)**
- Notation: Cs/αs is the layer, Cb/αb is the backdrop, B is the blend function.
- The two steps:
  - `Cs' = (1−αb)·Cs + αb·B(Cb,Cs)`
  - then source-over: `αo = αs + αb(1−αs)`, `co = αs·Cs' + αb·Cb·(1−αs)`
- The GPU works in premultiplied half-float. CPU reference uses f32.

**Separable modes (W3C-CB §10.1, ✔︎)**

| Mode | Formula |
|---|---|
| Normal | Cs |
| Multiply | Cb·Cs |
| Screen | Cb+Cs−Cb·Cs |
| Overlay | HardLight(Cs,Cb) |
| Darken | min |
| Lighten | max |
| Color Dodge | Cb=0→0; Cs=1→1; else min(1, Cb/(1−Cs)) |
| Color Burn | Cb=1→1; Cs=0→0; else 1−min(1,(1−Cb)/Cs) |
| Hard Light | Cs≤½ → Cb·2Cs; else Screen(Cb, 2Cs−1) |
| Difference | \|Cb−Cs\| |
| Exclusion | Cb+Cs−2CbCs |

**Soft Light: two variants**
- **W3C/PDF:**
  - Cs≤½: `Cb−(1−2Cs)Cb(1−Cb)`
  - otherwise: `Cb+(2Cs−1)(D(Cb)−Cb)`, with `D(x)=((16x−12)x+4)x` for x≤¼, else `√x`
- **Photoshop**, per WIKI-BM and PEGTOP ✔︎, with a = backdrop, b = layer:
  - b<½: `2ab + a²(1−2b)`
  - else: `2a(1−b) + √a(2b−1)`
- **Where they differ.** Working the algebra out, the dark half is identical. In the light half they
  differ only where the backdrop a ≤ ¼: W3C uses the cubic, Photoshop uses √a.
- **Our choice.** Use the Photoshop variant for layers (PSD `sLit` fidelity). W3C is kept in lawha-core
  for tests and SVG/CSS export.
- **Test.** A fixture with a ≤ ¼ shows which variant matches.

**Non-separable modes (W3C-CB §10.2, ✔︎)**
- Helpers:
  - `Lum(C)=0.3R+0.59G+0.11B`
  - `ClipColor`
  - `SetLum(C,l)`: add l−Lum(C) to each channel, then ClipColor
  - `Sat(C)=max−min`
  - `SetSat`: rescale so the middle channel keeps its proportion, max = s, min = 0
- The four modes:
  - Hue = `SetLum(SetSat(Cs,Sat(Cb)),Lum(Cb))`
  - Saturation = `SetLum(SetSat(Cb,Sat(Cs)),Lum(Cb))`
  - Color = `SetLum(Cs,Lum(Cb))`
  - Luminosity = `SetLum(Cb,Lum(Cs))`

**Remaining PSD modes**

These come from SUCAN, KRITA-BM and WIKI-BM ✔︎. Adobe publishes descriptions, not formulas, so these
are reconstructions ✱. Notation: B = backdrop, A = layer, clamped to [0,1].

| Mode | Formula |
|---|---|
| Linear Burn | A+B−1 |
| Linear Dodge (Add) | A+B |
| Vivid Light | A<½ → ColorBurn(B,2A); else ColorDodge(B,2A−1) |
| Linear Light | B+2A−1 |
| Pin Light | A≤½ → min(B,2A); else max(B,2A−1) |
| Hard Mix | A+B≥1 → 1, else 0 |
| Subtract | B−A |
| Divide | B/A (A=0 guarded) |
| Darker / Lighter Color | keep the whole RGB triple of whichever has the lower/higher composite value |

- **Darker/Lighter Color metric.** Third-party sources disagree: channel sum or luminance ✱. We start
  with Lum() and settle it with test PSDs.
- **Dissolve.** Each pixel randomly shows the layer or the backdrop, with probability equal to alpha.
  We use a deterministic hash of pixel coordinates.

**Where the modes are listed**
- Adobe's mode list and grouping: https://helpx.adobe.com/photoshop/using/blending-modes.html ◐
- PSD 4-character keys (PSD ✔︎): `pass norm diss dark mul␠ idiv lbrn dkCl lite scrn div␠ lddg lgCl over
  sLit hLit vLit lLit pLit hMix diff smud fsub fdiv hue␠ sat␠ colr lum␠`. Note `idiv` = Color Burn,
  `div␠` = Color Dodge, `smud` = Exclusion.

**Tests**
- Per mode: a 256×256 grid of (a,b) pairs, each at α ∈ {0, 0.25, 0.5, 1}.
- lawha-core output is checked against the formulas above, written independently in the test.
- The GPU shader is checked against lawha-core with ±1/255 tolerance.

---

## 3. Masks and adjustment layers

### 3.1 Layer masks
**Behavior**
- Grayscale mask: white shows, black hides.
- Paint on it with any painting tool.
- Density (100% = full effect) and Feather (a blur of the mask edge applied at render time) are live
  properties.
- Invert, disable (Shift-click), apply, delete.
- "Mask hides effects" flags (`lmgm`).

Sources:
- https://helpx.adobe.com/photoshop/desktop/create-masks/layer-masks/disable-or-enable-layer-masks.html ◐
- PSD layer mask record and mask parameters (density/feather) ✔︎

### 3.2 Adjustment layers
**Common behavior**
- Each adjustment is a layer with no pixels, plus an implicit white mask.
- It affects everything below it within its group, or only its clip base when clipped.
- Parameters are edited in the Properties panel.
- The GPU applies each one as a per-pixel shader (lookup tables for curve-type adjustments), the way
  PHOTOPEA describes.

| Adjustment | Parameters (from Adobe help ◐) | Math we implement | Source of math |
|---|---|---|---|
| Levels | Input black/gamma/white (0 / 1.00 / 255), output black/white; per-channel; eyedroppers | `t = clamp((x−inB)/(inW−inB))`, `t = t^(1/γ)`, `out = outB + t·(outW−outB)`, applied as a 256-entry LUT per channel after the master pass | Standard levels definition; PSD `levl` record ✔︎ |
| Curves | Up to 14 points; histogram behind; per-channel; on-image drag | Monotone cubic through the points (Fritsch–Carlson), baked into a LUT. Photoshop's exact spline isn't public ✱, so test PSDs measure the gap. | https://en.wikipedia.org/wiki/Monotone_cubic_interpolation ✔︎; PSD `curv` ✔︎ |
| Hue/Saturation | Hue ±180, Sat ±100, Light ±100; master + 6 ranges with 30° inner band and 30° falloff; Colorize | HSL-based. Each range weight = trapezoid over hue. Lightness blends toward white/black. Colorize sets hue and saturation and keeps lightness. Exact curves ✱ to be calibrated with test PSDs. | Adobe help ◐; PSD `hue2` ✔︎ |
| Brightness/Contrast | Brightness ±150, Contrast −50..+100; Legacy | Legacy: linear offset and contrast around mid-gray. Modern: tone curve that protects the ends ✱, calibrated against test PSDs | https://helpx.adobe.com/photoshop/using/apply-brightness-contrast-adjustment.html ◐ |
| Exposure | Exposure (stops), Offset, Gamma | In linear light: `x·2^E + offset`, then `^(1/γ)`. Adobe's help says it runs in linear gamma 1.0. | https://helpx.adobe.com/photoshop/using/adjusting-hdr-exposure-toning.html ◐ |
| Vibrance | Vibrance, Saturation | Saturation: uniform HSL scaling. Vibrance: scaled by (1 − current saturation), with a reduced effect near skin hues ✱ | https://helpx.adobe.com/photoshop/using/adjust-vibrance.html ◐ |
| Color Balance | Shadows/Midtones/Highlights × 3 sliders ±100; preserve luminosity (on by default) | A tonal weight per range (smooth functions of luminance) shifts R/G/B. Preserve luminosity restores Lum() afterwards. | https://helpx.adobe.com/photoshop/using/applying-color-balance-adjustment.html ◐ |
| Black & White | Reds, Yellows, Greens, Cyans, Blues, Magentas; tint | Gray = Lum-like mix where each pixel's hue picks a weight by interpolating between the 6 sliders. Defaults ✱ 40/60/40/60/20/80, to confirm from a PSD `blwh` record. | Adobe help ◐; PSD `blwh` ✔︎ |
| Gradient Map | Gradient, reverse, dither | Luminance → gradient position → color | https://helpx.adobe.com/photoshop/using/applying-special-color-effects-images.html ◐ |
| Invert | none | `1−x` per channel | same |
| Threshold | Level 1–255 (default 128), with histogram | `lum ≥ level → white else black` | same |

**Tests**
- Known input → known output vectors (identity settings give identity output).
- Monotonicity where expected.
- GPU vs CPU agreement.
- Comparison against Photoshop-made composites in test PSDs (section 13), which measures the ✱
  calibrations.

---

## 4. Selections

**Common behavior**
- A selection is an 8-bit coverage mask (anti-aliased), shown as marching ants.
- Modes: New, Add (Shift), Subtract (Alt/Option), Intersect (Shift+Alt).
- The outline is traced with marching squares at 0.5 coverage
  (https://en.wikipedia.org/wiki/Marching_squares ✔︎).

| Tool | Behavior (Adobe help ◐) | Technique |
|---|---|---|
| Rectangle / Ellipse | Normal, Fixed Ratio, Fixed Size; Feather 0–250 px; anti-alias (ellipse) | Analytic coverage per pixel |
| Lasso | Freehand; Alt switches to straight segments | Polygon scan conversion with 4×4 supersampled coverage |
| Polygonal lasso | Click points; close by double-click or clicking the start; Shift snaps to 45° | Same |
| Magic wand | Tolerance 0–255 (default 32), Contiguous, Sample all layers, Anti-alias | Scanline flood fill (https://en.wikipedia.org/wiki/Flood_fill ✔︎) with a per-channel max-difference test |
| Quick selection (classic) | A brush that grows the selection to nearby edges; add/subtract | Our design: seed region from the brush, then graph-cut style growth on color and edge strength, plus guided-filter refinement. Not an Adobe method; Adobe's isn't public. |
| Select subject | One click selects the main subject | BiRefNet-lite on device (section 10) |
| All / Deselect / Reselect / Inverse | Standard | Mask ops |
| Feather | Radius px | Gaussian blur of the mask |
| Expand / Contract | 1–100 px; edges on the canvas border stay | Exact Euclidean distance transform (Felzenszwalb–Huttenlocher, https://cs.brown.edu/people/pfelzens/papers/dt-final.pdf ✔︎), thresholded at r |
| Border, Smooth | Border 1–200 px; Smooth = majority vote | Distance band; box median |
| Refine edge | View modes (overlay, on black, on white, B&W, marching ants); edge radius; Smooth, Feather, Contrast, Shift Edge; decontaminate colors; output to selection/mask/new layer | Guided filter (He, Sun, Tang, https://people.csail.mit.edu/kaiming/publications/pami12guidedfilter.pdf ✔︎) inside the edge band; color decontamination by re-estimating foreground color |

Adobe selection help (◐):
- marquee: https://helpx.adobe.com/photoshop/using/selecting-marquee-tools.html
- lasso: https://helpx.adobe.com/photoshop/using/selecting-lasso-tools.html
- magic wand: https://helpx.adobe.com/photoshop/desktop/make-selections/automatic-color-based-selections/select-areas-by-color-with-the-magic-wand-tool.html
- quick selection: https://helpx.adobe.com/photoshop/desktop/make-selections/automatic-color-based-selections/paint-a-selection-with-quick-selection-tool.html
- expand/contract: https://helpx.adobe.com/photoshop/desktop/make-selections/refine-modify-selections/expand-or-contract-selection.html
- feather: https://helpx.adobe.com/photoshop/desktop/make-selections/refine-modify-selections/define-feathered-edges.html
- refine: https://helpx.adobe.com/photoshop/desktop/make-selections/refine-modify-selections/refine-your-selection-and-mask.html

**Tests.** Masks for known shapes (area within ±0.5%), flood fill against brute force, expand/contract
against brute-force distance.

---

## 5. Painting

### 5.1 Brush engine
**Behavior (Adobe help ◐)**
- Tip parameters: size (px), hardness (% of the diameter that is fully solid), spacing (% of the
  diameter), angle, roundness.
- **Opacity** is a ceiling for the whole stroke: overlapping dabs in one stroke never go above it.
- **Flow** is how much each dab adds toward that ceiling.
- Airbrush keeps adding paint while the pointer is held still.
- Number keys set opacity; Shift+number sets flow.
- Pen pressure can control size and/or opacity, with a minimum diameter.
- **Smoothing** 0–100% (default 10%), with a leash ("pulled string") mode, catch-up while paused,
  catch-up at stroke end, and adjust for zoom.

Sources:
- https://helpx.adobe.com/photoshop/using/painting-tools.html ◐
- https://helpx.adobe.com/photoshop/desktop/apply-painting-techniques/brushes-presets/display-brush-panel-brush-options.html ◐
- https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/create-smoother-more-polished-brush-strokes-with-stroke-smoothing.html ◐

**Technique**
- **Stroke buffer model.** Dabs accumulate with flow into a per-stroke coverage buffer, using
  `buf = buf + flow·dab·(1−buf)` capped at 1. The buffer × opacity is composited onto the layer each
  frame. When the stroke ends it is committed as one undo step. This is exactly the opacity-as-ceiling
  behavior described above.
- **Dab profile.** Radial falloff: 1 inside hardness × r, smoothstep to 0 at r. Anti-aliased by
  analytic coverage at small sizes.
- **Spacing.** Dabs are placed by arc length along the interpolated path, with the remainder carried
  between pointer events.
- **Smoothing.** A leash (lazy-mouse): the brush only moves when the pointer is further than
  `R = smoothing·k` away. Then raw samples are simplified (Ramer–Douglas–Peucker) and interpolated
  with a centripetal Catmull-Rom spline.
  - https://en.wikipedia.org/wiki/Centripetal_Catmull%E2%80%93Rom_spline ✔︎
  - Krita's smoothing modes (Basic, Weighted, Stabilizer) are a design reference:
    https://docs.krita.org/en/reference_manual/tools/freehand_brush.html#smoothing ✔︎
- **Stroke jobs on a worker queue** (init, dab, finish, cancel), with updates batched per frame.
  Idea from KRITA-STROKES ✔︎.
- **Pen pressure**
  - Windows: WebView2 Pointer Events give pressure and tilt (Windows Ink).
  - macOS: WKWebView reportedly reports Wacom pens as a mouse with no pressure ✱. We plan a native
    bridge: Rust reads `NSEvent.pressure` and tilt and streams them to the UI. To be verified in
    milestone 0.

**Perf target.** Paint latency p95 < 16 ms with a 500 px brush on the 6000×4000×30 test document.

### 5.2 Eraser
**Behavior (◐)**
- Brush/pencil/block modes.
- Erases alpha.
- On a layer with locked transparency, paints the background color instead.
- Source: https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/erase-parts-of-an-image-with-the-eraser-tool.html

**Technique.** The same stroke buffer, composited with destination-out.

### 5.3 Clone stamp
**Behavior (◐)**
- Alt/Option-click sets the source.
- Aligned: the offset is kept across strokes. Not aligned: every stroke restarts from the source point.
- Sample: current layer, current & below, or all layers.
- An overlay preview shows the source.
- Source: https://helpx.adobe.com/photoshop/desktop/repair-retouch/heal-clone/retouch-images-with-the-clone-stamp-tool.html

**Technique.** The stroke buffer is used as a mask over the source pixels shifted by the offset. The
source is snapshotted when the stroke starts, so painting doesn't read its own output.

### 5.4 Healing brush and spot healing
**Behavior (◐)**
- Healing copies texture from the source but matches the destination's color and brightness.
- Spot healing needs no source: it picks content around the stroke.
- A diffusion setting controls how fast the patch adapts.
- Sources:
  - https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/healing-brush-tool.html
  - https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/spot-healing-brush-tool.html

**Technique: public papers**
- **Georgiev 2004**, "Photoshop Healing Brush: a Tool for Seamless Cloning"
  (https://ftp.fau.de/gimp/references/Photoshop_Healing_Brush_a_Tool_for_Seamless_Clonin.pdf ✔︎)
  1. Take the difference between destination and source.
  2. Solve Laplace inside the brushed region from its boundary values.
  3. Refine with a biharmonic iteration so derivatives match too.
  4. Add the source back.
  - Unselected neighbors are replaced by the center pixel (Neumann-like), so colors don't bleed.
  - Diffusion = iteration count / regularization.
- **Pérez et al. 2003**, "Poisson Image Editing"
  (https://www.cs.jhu.edu/~misha/Fall07/Papers/Perez03.pdf ✔︎): the general solver (multigrid).
- **Spot healing.** The source is chosen automatically with PatchMatch (5.6), then healed as above.

**Test.** On a smooth gradient with a pasted texture patch, the healed result's boundary gradient
error must fall under a threshold.

### 5.5 Gradient, paint bucket, eyedropper
**Gradient** (◐ https://helpx.adobe.com/photoshop/desktop/adjust-color/color-effects-techniques/apply-gradient-fill.html)
- Types: linear, radial, angle, reflected, diamond.
- Options: reverse, dither, transparency.
- Shift snaps to 45°.
- Technique: an analytic position function per type, color stops interpolated in sRGB or linear
  light, and ordered dither in the last bit.

**Paint bucket**
(◐ https://helpx.adobe.com/photoshop/desktop/apply-painting-techniques/fill-objects-selections-layers/fill-paint-bucket-tool.html)
- Tolerance 0–255 (default 32), contiguous, all layers, anti-alias, mode and opacity.
- Technique: scanline flood fill, shared with the magic wand.

**Eyedropper** (◐ https://helpx.adobe.com/photoshop/using/tool-techniques/eyedropper-tool.html)
- Sample size: point, 3×3, 5×5, 11×11, 31×31, 51×51, 101×101.
- Sample: current layer, current & below, all layers.
- Alt while painting picks a color.

### 5.6 PatchMatch (shared by spot healing and Smart Fill)
**Barnes et al. 2009**
(https://pixl.cs.princeton.edu/gfx/pubs/Barnes_2009_PAR/patchmatch.pdf ✔︎)
- Random nearest-neighbor field.
- Alternating propagation and random search with radius w·½ⁱ.
- Coarse-to-fine completion by EM-style patch voting.
- Implementation notes: IPOL "Non-Local Patch-Based Image Inpainting", https://www.ipol.im/pub/art/2017/189/ ✔︎

---

## 6. Transform and resize

**Free transform**
(◐ https://helpx.adobe.com/photoshop/using/free-transformations-images-shapes-paths.html)
- One mode handles scale, rotate, skew, distort and perspective.
- Corner drags keep proportions by default; Shift toggles.
- Shift while rotating snaps to 15°.
- A 9-point reference point; flip H/V.
- Ctrl/Cmd+drag distorts; Ctrl/Cmd+Shift+side handle skews; Ctrl/Cmd+Alt+Shift+corner applies
  perspective.
- Several transforms are combined into one matrix and resampled once.
- Technique: a 3×3 homography; inverse-map every output pixel and resample (below).
- The preview is a GPU-textured quad. Commit happens at full resolution on the CPU.

**Crop**
(◐ https://helpx.adobe.com/photoshop/desktop/crop-resize-transform/crop-straighten/crop-tool-options.html)
- Ratio presets (1:1, 4:5, 2:3, 16:9, original, custom) or W×H.
- Straighten: draw a horizon line to set the rotation.
- "Delete cropped pixels" off keeps hidden pixels outside the canvas.
- Rule-of-thirds overlay, shield color and opacity.

**Canvas size**
(◐ https://helpx.adobe.com/photoshop/desktop/crop-resize-transform/resize-adjust-resolution/change-the-canvas-size.html)
- Absolute or relative values.
- 3×3 anchor grid.
- New area takes the extension color on a background layer, otherwise transparent.

**Image size**
(◐ https://helpx.adobe.com/photoshop/desktop/crop-resize-transform/resize-adjust-resolution/resampling-options.html)
- Resample on/off: off changes only the ppi.
- Methods: nearest, bilinear, bicubic, Lanczos.
- Adobe's "Preserve Details" methods aren't public. We offer Lanczos-3 as our high-quality option
  under our own name.
- Technique:
  - Keys bicubic with a = −0.5 (https://en.wikipedia.org/wiki/Bicubic_interpolation#Bicubic_convolution_algorithm ✔︎)
  - Lanczos-3 (https://en.wikipedia.org/wiki/Lanczos_resampling ✔︎)
  - Separable passes; kernel widened by the scale factor when downscaling; resample in premultiplied
    alpha.
  - IPOL overview: https://www.ipol.im/pub/art/2011/g_lmii/ ✔︎

**Tests.** Each kernel's weights against a closed-form reference. Downscaling a checkerboard must not
alias. Upscaling a constant image must stay constant.

---

## 7. Text (Arabic first)

### 7.1 Behavior
Reference: Adobe's Middle Eastern features
- https://helpx.adobe.com/photoshop/using/arabic-hebrew.html ◐
- https://helpx.adobe.com/photoshop/desktop/text-typography/international-text-languages/overview-of-unified-text-engine.html ◐
  Adobe's page says its current engine uses HarfBuzz for shaping and FriBidi for bidi, so the
  open-source path is industry standard.

Features:
- Paragraph direction RTL/LTR; per-run direction override.
- Digit shapes: Arabic (0-9), Arabic-Indic (٠-٩) and Persian.
- Kashida justification: none, short, medium, long. Only in justified paragraphs.
- Diacritic position offsets.
- Standard and discretionary ligatures.
- Auto leading = 120% of the font size.
- Tracking in 1/1000 em.
- Kerning: metrics or manual.
- Baseline shift.

Our v1 controls: font, weight, size, color, line spacing, letter spacing, alignment (start, center,
end, justify), direction, digit style, kashida.

### 7.2 Technique
**Shaping: harfrust**
- The HarfBuzz organization's official pure-Rust port. Tracks HarfBuzz 14.x. MIT.
  https://github.com/harfbuzz/harfrust ✔︎
- rustybuzz and ttf-parser were archived in 2026 with RustSec "unmaintained" advisories, so we don't
  use them ✔︎.

**Fonts: skrifa / read-fonts** (Google fontations, MIT/Apache) parse fonts and give glyph outlines.

**Bidi: `unicode-bidi`** (UAX #9, https://www.unicode.org/reports/tr9/ ✔︎).

**Line breaking: `icu_segmenter`** (UAX #14, https://www.unicode.org/reports/tr14/ ✔︎).

**Arabic shaping stages** (the shaper performs these; listed so the tests can check them)
- Source: https://github.com/n8willis/opentype-shaping-documents/blob/master/opentype-shaping-arabic-general.md ✔︎
1. Joining types from ArabicShaping.txt (https://www.unicode.org/Public/UCD/latest/ucd/ArabicShaping.txt ✔︎).
2. Mark reordering per UAX #53 (https://www.unicode.org/reports/tr53/ ✔︎).
3. GSUB: `isol fina medi init rlig calt liga`.
4. GPOS: `curs kern mark mkmk`.

**Kashida**
- Follows W3C ALREQ §7.2.5 (https://www.w3.org/TR/alreq/#x7-2-5-kashida ✔︎).
- Insert tatweel (U+0640) or use the font's `jalt`/stretch at one chosen joining point per word,
  following ALREQ's priority list, until the line is filled. Spaces are a fallback.

**Rendering**
- Glyph outlines are filled with our own anti-aliased rasterizer (tiny-skia, BSD-3) into the text
  layer's tiles.
- Text layers keep the source text, runs, and paragraph settings, and re-render on change.

**Parley** (linebender, harfrust + ICU4X) is evaluated in M0 as a ready-made layout layer instead of
building our own.

### 7.3 Fonts (all SIL OFL 1.1 ✔︎, from google/fonts metadata)

| Font | Weights | Source |
|---|---|---|
| IBM Plex Sans Arabic | 100–700 | https://github.com/IBM/plex |
| Cairo | variable 200–1000 | https://github.com/Gue3bara/Cairo |
| Tajawal | 200, 300, 400, 500, 700, 800, 900 | https://github.com/googlefonts/tajawal |
| Amiri | 400, 700, plus italics | https://github.com/aliftype/amiri |

OFL lets us bundle and redistribute them with the license text. We keep the font names unchanged and
don't modify the fonts.

### 7.4 Tests
- **Shaping goldens.** Glyph ID and position sequences for a test corpus:
  - lam-alef ligatures
  - all four joining forms
  - shadda + fatha stacking
  - tanween
  - mixed «مرحبا Hello 123»
  - numbers inside RTL
  - parentheses mirroring
- **Cross-check.** The same corpus is shaped with harfbuzzjs (real HarfBuzz) as an oracle.
- **Screenshots.** An Arabic test sheet in each font, checked by eye.

---

## 8. Layer styles

**Behavior**
- Sources:
  - https://helpx.adobe.com/photoshop/desktop/create-manage-layers/apply-layer-effects/layer-style-effects-and-options-overview.html ◐
  - https://helpx.adobe.com/photoshop/desktop/create-manage-layers/apply-layer-effects/set-a-global-lighting-angle-for-all-layers.html ◐
- Every effect has a blend mode and an opacity.
- Drop and inner shadow: color, angle (with a global light option), distance, spread or choke, size,
  contour, noise. Drop shadow also has "layer knocks out drop shadow".
- Outer glow: color or gradient, softer or precise technique, spread, size, range, jitter.
- Stroke: size, position (outside, inside, center), fill (color or gradient).
- Color overlay; gradient overlay (style, angle, scale, reverse, dither).
- Effects ignore Fill and respect Opacity (see 2.2).
- Scaling a layer doesn't scale its effects. Scaling effects is a separate command.

**Technique**
- Build an alpha matte from the layer.
- Spread/choke = threshold the distance transform.
- Size = Gaussian blur (Kovesi's box-blur approximation for large radii:
  https://www.peterkovesi.com/papers/FastGaussianSmoothing.pdf ✔︎).
- Precise glow and stroke use the exact distance transform (section 4).
- Offset by angle and distance.
- Contour = a LUT on the matte. Noise = hashed per-pixel jitter.
- Rendered on the GPU per visible tile with a margin, and cached until the layer changes. This is the
  same lazy re-rendering idea PHOTOPEA describes.

**PSD.** `lfx2` object-based effects, read and written through ag-psd (section 13).

---

## 9. Filters
All filters show a live preview at screen resolution on the GPU, then apply at full resolution on the
CPU with progress.

| Filter | Parameters (◐) | Technique (✔︎ sources) |
|---|---|---|
| Gaussian blur | Radius 0.1–1000 px | Separable Gaussian with σ = radius ✱ (mapping to be calibrated), 3σ support. Kovesi 3-pass box approximation above radius ~20. https://en.wikipedia.org/wiki/Gaussian_blur |
| Motion blur | Angle, Distance 1–999 px | 1-D line kernel along the angle, sampled with bilinear taps |
| Unsharp mask | Amount 1–500%, Radius 0.1–250, Threshold 0–255 | `out = x + amount·(x − blur(x))` where `\|x−blur\| ≥ threshold`. https://en.wikipedia.org/wiki/Unsharp_masking |
| Add noise | Amount, Uniform/Gaussian, Monochromatic | Seeded counter-based random numbers (deterministic per pixel, so the preview and the final match). Gaussian via Box–Muller. |
| Pixelate (mosaic) | Cell size | Block average aligned to the canvas origin |

Adobe filter reference: https://helpx.adobe.com/photoshop/using/filter-effects-reference.html ◐ ·
unsharp mask: https://helpx.adobe.com/photoshop/desktop/effects-filters/smart-filters/sharpen-images-with-unsharp-mask.html ◐

---

## 10. Local AI (no cloud, no account)

**Behavior reference (◐)**
- Remove background adds a layer mask that hides the background. Non-destructive and editable.
  - Source: https://helpx.adobe.com/photoshop/desktop/repair-retouch/remove-objects-fill-space/remove-background-in-your-images.html
- Object-aware fill has a sampling area and color adaptation.
  - Source: https://helpx.adobe.com/photoshop/using/content-aware-fill.html
- Our names: «إزالة الخلفية» Remove Background, «تحديد العنصر» Detect Subject, «تعبئة ذكية» Smart Fill.

**Models.** Licenses checked on 2026-10-20 from the repos and Hugging Face API. Only permissive,
commercial-safe weights are used.

| Use | Model | License | Size | Status |
|---|---|---|---|---|
| Remove background / detect subject | BiRefNet_lite (`onnx-community/BiRefNet_lite-ONNX`) | MIT | 115 MB fp16, input 1024² | ✔︎ chosen |
| Click-to-select, quick selection assist | SAM 2.1 hiera-tiny (`onnx-community/sam2.1-hiera-tiny-ONNX`) | Apache-2.0 | about 67 MB fp16 encoder + decoder | ✔︎ chosen |
| Smart Fill (learned) | MI-GAN (`migan_pipeline_v2.onnx`) | MIT code **and** MIT weights | 28 MB, input 512² | ✔︎ chosen |
| Smart Fill (classic) | PatchMatch (our implementation, section 5.6) | — | 0 MB | ✔︎ chosen |
| Portrait fallback | MODNet | Apache-2.0 | 13 MB | optional |
| — | BRIA RMBG-1.4 / 2.0 | non-commercial / CC BY-NC | — | ✘ excluded |
| — | EdgeSAM | S-Lab non-commercial | — | ✘ excluded |
| — | ISNet/DIS, U²-Net weights | weights license unclear; one ONNX export is tagged AGPL | — | ✘ excluded |
| — | LaMa | Apache code, but trained on Places2 (non-commercial dataset terms ✱) | 208 MB | ✘ excluded for now |

**Runtime**
- `ort` 2.0 (ONNX Runtime 1.31). Prebuilt binaries include CoreML on macOS and DirectML on Windows.
- DirectML is in maintenance mode, but it works. Windows ML is the future path.
- CPU fallback always works.
- Source: https://ort.pyke.io/perf/execution-providers ✔︎

**Pipeline**
1. Downscale to the model input size, run inference, upsample the mask.
2. Refine with the guided filter at full resolution.
3. Output a layer mask or a selection.

**Test**
- Mask IoU on a small set of our own public-domain photos.
- A no-network test: REA captures the process network activity of **our own** app while every AI
  feature runs; it must be zero.

---

## 11. History and shortcuts

**History**
- Every operation records the before-images of the tiles it touches (copy-on-write). An undo step is
  only as big as what changed.
- Memory budget: 1 GB by default. Older steps spill to disk, and the oldest drop past a step limit
  (default 200).
- The history panel lists named steps; clicking one jumps there.
- Design reference: KRITA-STROKES undo notes ✔︎.

**Shortcuts**
- Adobe's public shortcut sheet (◐ https://helpx.adobe.com/content/dam/help/en/photoshop/using/default-keyboard-shortcuts/photoshop-keyboard-shortcuts.pdf)
  confirms the conventions in the brief.
- We set our own mapping. Single-letter tool keys are a shared convention across editors (GIMP and
  Krita use many of the same ones).
- We don't copy Adobe's table wholesale.
- The keys:
  - V move · M marquee · L lasso · W magic wand / quick select · C crop · I eyedropper
  - J healing · B brush · S clone · E eraser · G gradient / bucket · T text
  - D/X colors · [ ] brush size · Shift+[ ] hardness
  - Cmd/Ctrl+J duplicate layer · Cmd/Ctrl+T free transform · Cmd/Ctrl+Z undo (Shift for redo)
  - Cmd/Ctrl+Alt+Z step back · Cmd/Ctrl+D deselect · Cmd/Ctrl+Shift+I inverse
  - Space pan · Cmd/Ctrl +/− zoom · Cmd/Ctrl+0 fit · Cmd/Ctrl+1 100%
- In Arabic keyboard layout, shortcuts work by **physical key** (`KeyboardEvent.code`), so they don't
  break when the Arabic layout is active.

---

## 12. Export
**Behavior (◐)**
- Export As: PNG (with or without transparency), JPEG (quality 1–100), WebP (quality, lossless option).
- Size by pixels or scale %, with @2x-style extra sizes.
- Optional metadata; convert to sRGB.
- File-size preview.
- Sources:
  - https://helpx.adobe.com/photoshop/desktop/save-and-export/export-files-to-different-formats/fine-tune-your-export-settings-using-the-export-as-option.html
  - quick export: https://helpx.adobe.com/photoshop/desktop/save-and-export/export-files-to-different-formats/export-your-work-using-the-quick-export-as-option.html

**Ours**
- Export document, selection or layer.
- Web size presets: 2048, 1600, 1080, 720 px long edge.
- Preview with the real encoded size and a before/after comparison.

---

## 13. PSD

**Spec (PSD ✔︎)**

| Part | Contents |
|---|---|
| Header | `8BPS`, version 1 (PSB = 2), channels, height, width, depth, color mode |
| Color mode data | |
| Image resources | `8BIM` blocks: 1005 resolution, 1039 ICC, 1060 XMP, 1024 target layer |
| Layer and mask info | Layer records stored bottom-first ✱. Each record: bounds, channels (−1 alpha, −2 mask, −3 real mask), blend key, opacity, clipping, flags, mask data, blending ranges, name, tagged blocks |
| Channel compression | raw / PackBits RLE / ZIP / ZIP with prediction |
| Merged composite | Comes last. It may be empty when "maximize compatibility" was off. |

Tagged blocks we need:
- `lsct`: group open/closed/end markers
- `luni`: Unicode name
- `lyid`: layer ID
- `iOpa`: fill opacity (in psd-tools' key list; not in Adobe's text ✱)
- `clbl`, `infx`, `knko`
- `lfx2`: effects
- `TySh`: text (EngineData is undocumented ✱)
- Adjustments: `levl curv hue2 brit expA vibA blnc blwh grdm nvrt thrs`

**Library: ag-psd 31.0.3 (MIT, active, released 2026-10-07)**
- Reads and writes:
  - layers, groups, masks, vector masks, blend modes
  - 16 adjustment types
  - effects: shadows, glows, stroke, overlays
  - text: runs and styles parsed from EngineData; raw EngineData kept
- Reads 16/32-bit and PSB.
- **Limits to design around:**
  - writes 8-bit only (fine for v1)
  - no compositing: we supply every layer's pixels and the composite ourselves
  - edited text needs `invalidateTextLayers`, and Photoshop then asks to update text layers
    (expected and acceptable)
- Known write bugs to avoid: #158 vector mask feather, #251 text bounds, #175 CR in text
  (https://github.com/Agamnentzar/ag-psd/issues ✔︎).
- We wrap it behind `PsdCodec`, so a fix or replacement doesn't touch the app.

**Text mapping**
- PSD text with EngineData becomes an editable Lawha text layer when we support every style used.
  Otherwise it opens as an editable layer with a "may differ" badge, and the original pixels are kept
  until the user edits it.
- On save we write both TySh and the rendered pixels.

**Round-trip test design**
- **Inputs**
  - Public test PSDs from MIT-licensed test suites (ag-psd and psd-tools fixtures).
  - PSDs you make in your own Photoshop. Using it as a normal user to create test files is not
    reverse engineering, but it is your call.
- **Steps for each file**
  1. Render it with Lawha and compare against the merged composite Photoshop stored in the file.
     Report max and mean ΔE and the % of pixels within 1/255.
  2. Save with Lawha and re-open in Lawha. The structure must match: layers, names, modes, opacity,
     masks, styles.
  3. Re-open the saved file with psd-tools as an independent oracle. Its structure must match too.
- **Output.** Results go to `docs/psd-report.md`, one row per file, with the features each file uses.
  These comparisons also settle the ✱ items above (Fill-sensitive modes, Soft Light variant,
  Darker/Lighter Color metric, curve spline, Hue/Saturation and Brightness/Contrast calibration).

---

## 14. Rendering engine and performance

**Tiles and mipmaps**
- Sparse tiled buffers, swap for images larger than memory, and lazy mip levels for zoomed-out views.
  Source: GEGL ✔︎ https://gegl.org/features.html
- Each mip level is built only when the zoom needs it, as a 2× reduction of the level above.

**Below/above caches**
- Keep, per tile, the composite of layers below the active layer and of layers above it. While
  painting, only the active layer is recomposited.
- Adobe's own public community post (Oct 2025) describes this caching
  (https://community.adobe.com/questions-700/compositing-performance-foreground-caching-670288 ✔︎),
  and Foundry Mari documents a similar "cache up to here".
- **Caveat.** The cache above is only valid when no pass-through group or non-normal stack crosses the
  active layer. Otherwise it falls back.

**GPU compositing**
- PHOTOPEA ✔︎ reports WebGL running every blend mode, mask and style, with large speed-ups. This is
  our existence proof for the webview approach.

**Webview limits (verified)**
- WKWebView: a hidden or occluded page is killed above about 4 GB, a visible one above about 15 GB
  (source: WebKit `MemoryPressureHandler.cpp`,
  https://github.com/WebKit/WebKit/blob/main/Source/WTF/wtf/MemoryPressureHandler.cpp ✔︎).
- Safari doesn't ship wasm memory64.
- WebGL2: use RGBA16F render targets and feature-detect `EXT_color_buffer_float`. Apple GPUs lacked
  float32 blending (WebKit bug 247240 ✔︎).
- WebGPU: on by default in macOS 26+ WebKit, but behind a flag on Windows ARM64. That's why WebGL2
  comes first.

**Tauri 2.12** (✔︎ https://v2.tauri.app)
- Raw binary IPC works, but is reported much slower on Windows than on macOS (about 200 ms vs 5 ms for
  10 MB, anecdotal). We benchmark tile transfer over a custom URI scheme.
- `app.security.headers` sets COOP/COEP for wasm threads. Whether `crossOriginIsolated` becomes true
  on `tauri://` in WKWebView is ✱, checked in M0.
- E2E testing: WebdriverIO with `tauri-plugin-wdio-webdriver` works on macOS; tauri-driver doesn't.

**Docking**
- dockview, golden-layout and FlexLayout all lack RTL support (open or closed-as-won't-fix issues ✔︎).
- So we build a small docking layer of our own.

---

## 15. Naming and identity
- Avoid Adobe trademarks and coined feature names. Adobe's trademark list
  (https://www.adobe.com/cc-shared/assets/pdf/legal/documents/adobe-trademark-database-external.pdf ✔︎)
  includes Photoshop, Lightroom, Firefly, Sensei, Content Credentials, Cutout and Toolbox.
- Also avoided as feature labels: "Content-Aware", "Select Subject", "Select and Mask", "Refine Edge",
  "Smart Object", "Pulled String", "Spot Healing Brush", "Pass Through", "Preserve Details".
- We use our own names, for example:
  - Smart Fill «تعبئة ذكية»
  - Detect Subject «تحديد العنصر»
  - Refine Edges «تنقيح الحواف»
  - Through «عابر»
  - Leash smoothing «تنعيم بالمقود»
- Generic industry terms are fine: Levels, Curves, Hue/Saturation, Lasso, Magic Wand, Clone Stamp,
  Gaussian Blur, Unsharp Mask, and the blend mode names.
- Icons, logo, colors and layout artwork are all drawn from scratch for Lawha.

---

## 16. Open questions to settle by testing

| # | Question | How we settle it |
|---|---|---|
| 1 | Do the 8 "fill-sensitive" modes behave differently at Fill < 100%? | Test PSDs (section 13) |
| 2 | Which Soft Light variant matches a ≤ ¼ backdrops? | Test PSDs |
| 3 | Darker/Lighter Color: channel sum or luminance? | Test PSDs |
| 4 | Curves spline shape, Hue/Saturation range falloff, modern Brightness/Contrast curve, Black & White defaults, Gaussian radius-to-σ | Calibrate against Photoshop-made test PSD composites, and report remaining error honestly |
| 5 | Pen pressure in WKWebView, and whether the native NSEvent bridge is needed | M0 spike (needs a tablet) |
| 6 | `crossOriginIsolated` under `tauri://` | M0 spike |
| 7 | Tile transfer speed on Windows | M0 spike on the VM, then confirmed on real hardware |
