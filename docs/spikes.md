# Milestone 0: spike results

Date: 2026-10-20.
Machine: MacBook Pro, Apple M4 Pro, 24 GB, macOS 27.0.1.
Window: 1440×868 pt at 2× (canvas 2880×1736 px).
App: Tauri 2.12.4 debug build with the WKWebView dev server.

Raw results are in `spikes/out/*.json` (not committed). The spike code is in commit 4e826fe (`src/spike/`) and
`spikes/agpsd.mjs`.

## Environment (WKWebView)

| Check | Result |
|---|---|
| `crossOriginIsolated` / `SharedArrayBuffer` | Dev server: **true / true**. Release build on `tauri://` (checked in M1): `crossOriginIsolated` is **true** but `SharedArrayBuffer` is **undefined**. This doesn't affect the chosen architecture, which needs no wasm threads. |
| WebGL2 renderer | Apple GPU, max texture 16384, **2048** array layers |
| Float targets | `EXT_color_buffer_float`, `EXT_color_buffer_half_float`, `EXT_float_blend` and `OES_texture_float_linear` all present. `EXT_texture_norm16` is also there, useful for 16-bit later. |
| WebGPU | `navigator.gpu` present |
| wasm memory64 | not supported (matches research) |
| `hardwareConcurrency` | 8 (WebKit caps this value) |

## Data transfer between the UI and Rust

| Path | Result |
|---|---|
| `invoke` raw download, 10 MB | p50 2.3–2.9 ms (≈ 4 GB/s) |
| `invoke` raw upload, 10 MB | p50 2.4–3.5 ms |
| Custom scheme `tile://`, 256 KB tiles, 1 request at a time | 1.95 ms/tile (135 MB/s) |
| Custom scheme, 8 requests in flight | 0.27 ms/tile (≈ 950 MB/s) |

Moving tiles between the webview and Rust is cheap on Mac, so the Rust process can own the document
and the webview can be a cache. Windows is reported to be much slower; it gets measured in the VM
when we get to Windows.

## GPU compositing

The renderer under test:
- a virtual-texture atlas: a `TEXTURE_2D_ARRAY` of 258×258 tile slots;
- page tables per layer and per mip level;
- one full-screen pass per layer, reading the backdrop and blending in RGBA16F with the W3C general
  formula.

### 30 layers of 6000×4000, window visible

Frame cost is submit plus `gl.finish`, so it is the real per-frame work, not the display cadence.

| Scenario | Frame cost p50 / p95 / max | Frames > 16 ms |
|---|---|---|
| Zoom around fit (mips 1–2) | 3.2 / 3.9 / 4.0 ms | 0 |
| Pan at 100% | 3.1 / 3.8 / 4.0 ms | 0 |
| Pan at 200% | 0.1 / 3.1 / 3.8 ms | 0 |
| Zoom sweep 10% → 400% | 0.1 / 3.7 / 3.9 ms | 0 |

The pass cost itself is low. **But** every frame had missing tiles. At 2880×1736, one layer's visible
tiles (plus a prefetch margin) take about 126 slots, and 30 layers need about 3,800. The atlas has
2,048 slots, so it thrashes, and a frame with missing tiles costs less than a complete one. This is
an **atlas residency limit**, not a fill-rate limit.

### Fewer layers, window hidden

With the window behind other windows, WebKit throttled timers to about 1 per second, so these runs
were invalid (n ≈ 5 frames). `backgroundThrottling: "disabled"` in the Tauri config did not stop
timer throttling for a window covered by other windows.

### What this means for the design (now requirements)

1. **Below/above composite caches are required.** While the user works on a layer, the GPU keeps
   only three tile sets resident:
   - the composite of everything below the active layer,
   - the active layer,
   - the composite of everything above.

   That is about 380 slots at this canvas size. The caches are rebuilt in chunks of tiles when the
   active layer or the stack changes.
2. **The full document lives in Rust.** The webview holds only hot tiles. Transfer is fast enough
   (see above), and it also protects against WebKit's page-kill limits.
3. **Time-critical background work runs in Rust**: autosave timers and recovery journaling.
   Webview timers are throttled when the window is covered.

### Still to measure, with the window in front

- The cached three-set path and a fully resident 15-layer stack, under real display cadence.
- Paint latency (M4).

The test harness will bring the window forward only for the minute it runs.

## Memory

| Test | Result |
|---|---|
| 30 full layers of 6000×4000 as 256 KB tiles (3.02 GB) | Allocated in 293 ms; the page survived 30 s |
| Grown to 5.0 GB | Survived 30 s while visible. WebContent RSS peaked at about 3.6 GB (macOS compresses repeated fills). |

The hidden-page limit (about 4 GB per WebKit's source) was not triggered, because the page reported
itself as visible. The design rule stays: the webview keeps at most about 2 GB of pixels, and the
document lives in Rust.

## Text shaping (harfrust 0.14 + read-fonts, native and wasm32)

| Input | Amiri | IBM Plex Sans Arabic |
|---|---|---|
| «لا» | contextual lam-alef pair (`lam.init.LamAlfIsol` + `alef.fina.LamAlfIsol`) | single ligature glyph `uniFEFB` |
| «مرحبا» | init/medi/fina forms, with a meem-reh contextual alternate | `uniFE8E uniFE92 uniFEA3 uniFEAE uniFEE3` |
| «بِسْمِ ٱللَّهِ» | Allah ligature path (`.Lellah`), marks kept | shadda + fatha composed into one mark glyph |
| «سلام Hello ١٢٣» as one run | Latin comes out reversed | same |

Shaping is correct. The mixed case shows the **bidi step is mandatory**: split the text into runs
with unicode-bidi, then shape each run. harfrust, parley and read-fonts compile to
`wasm32-unknown-unknown`. Choosing between parley and our own layout waits until M7, with this
corpus as the test.

## PSD (ag-psd 31.0.3, Node 24)

Test file: 6000×4000, 30 layers (a full background plus 29 layers of 2400×1800), Arabic layer
names, ZIP compression.

| Step | Result |
|---|---|
| Write | 3.2 s |
| Read | 1.4 s |
| Process RSS | 1.6 GB after read |
| Arabic names | round-trip OK |

It needs an `initializeCanvas` shim that returns plain ImageData, even with `useImageData`. We'll run
it in a Worker.

## Pen pressure

There is no tablet. As decided, pressure is tested with simulated input:
- synthetic pointer events with pressure ramps,
- simulated samples fed into the native pressure bridge.

Real-pen behavior in WKWebView is reported as **not verified**.

## Decision

**GO** with the webview architecture, with the three requirements above built into M1.
