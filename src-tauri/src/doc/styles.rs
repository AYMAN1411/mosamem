//! Compositing a layer with effects, tile by tile, and changing a layer's effects.
//!
//! Order (as in other editors): drop shadow and outer glow on what is below; then the layer itself:
//! its pixels at the fill opacity, inner shadow, color and gradient overlays, clipped layers and
//! the stroke, blended with the layer's mode; finally the layer's opacity fades all of it. Fill
//! doesn't reach the effects; the layer mask shapes them.
//!
//! Effect mattes are computed from a region around the tile (the effects' reach) and cached by
//! the layer's pixels, mask and style, so edits elsewhere don't recompute them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use lawha_core::blend::{composite_px, dissolve_threshold};
use lawha_core::style::{self, LayerStyle, Mattes};
use lawha_core::tile::{Accum, TILE};
use lawha_core::BlendMode;

use super::composite::{clip_onto, layer_tile, mask_factor, MaskFactor, Sub};
use super::filters::read_rect;
use super::model::{Node, TileMap};
use super::ops::OpResult;
use super::{Document, Label};

struct Cached {
    mattes: Mattes,
    /// The layer's shape (alpha through its mask) at the tile.
    shape: Vec<f32>,
}

type Key = (usize, u64, u64, u8, i32, i32);

fn mattes_cache() -> &'static Mutex<HashMap<Key, Arc<Cached>>> {
    static C: OnceLock<Mutex<HashMap<Key, Arc<Cached>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn mask_key(n: &Node) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(m) = n.mask.as_ref().filter(|m| m.enabled) {
        (Arc::as_ptr(&m.mask.tiles) as usize, m.mask.outside, m.density.to_bits(), m.feather.to_bits()).hash(&mut h);
    }
    h.finish()
}

/// Content bounds of a layer's pixels at full size (cached by the pixels' identity).
fn bounds(tiles: &Arc<TileMap>) -> Option<(i64, i64, i64, i64)> {
    static C: OnceLock<Mutex<HashMap<usize, Option<(i64, i64, i64, i64)>>>> = OnceLock::new();
    let key = Arc::as_ptr(tiles) as usize;
    if let Some(b) = C.get_or_init(Default::default).lock().unwrap().get(&key) {
        return *b;
    }
    let b = super::raster::content_bounds(tiles);
    let mut c = C.get().unwrap().lock().unwrap();
    if c.len() > 4096 {
        c.clear();
    }
    c.insert(key, b);
    b
}

/// The layer's alpha over a region at a level, through its mask.
fn shape_region(n: &Node, tiles: &Arc<TileMap>, lod: u8, x0: i64, y0: i64, w: usize, h: usize) -> Vec<f32> {
    let rgba = read_rect(tiles, lod, x0, y0, w, h);
    let mut a: Vec<f32> = rgba.chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
    if n.mask.as_ref().is_some_and(|m| m.enabled) {
        let t = TILE as i64;
        for ty in y0.div_euclid(t)..=(y0 + h as i64 - 1).div_euclid(t) {
            for tx in x0.div_euclid(t)..=(x0 + w as i64 - 1).div_euclid(t) {
                let Some(f) = mask_factor(n, lod, (tx as i32, ty as i32)) else { continue };
                for y in (ty * t).max(y0)..(ty * t + t).min(y0 + h as i64) {
                    for x in (tx * t).max(x0)..(tx * t + t).min(x0 + w as i64) {
                        let k = match &f {
                            MaskFactor::Const(k) => *k,
                            MaskFactor::Each(v) => v[((y - ty * t) * t + (x - tx * t)) as usize],
                        };
                        a[(y - y0) as usize * w + (x - x0) as usize] *= k;
                    }
                }
            }
        }
    }
    a
}

/// Draws a color through a coverage matte onto an accumulator with a blend mode.
fn paint(acc: &mut Accum, matte: &[f32], opacity: f32, mode: BlendMode, color: impl Fn(usize) -> [f32; 3], origin: (i64, i64)) {
    for (i, d) in acc.0.iter_mut().enumerate() {
        let mut a = matte[i] * opacity;
        if a <= 0.0 {
            continue;
        }
        if mode == BlendMode::Dissolve {
            a = if dissolve_threshold((i % TILE) as i64 + origin.0, (i / TILE) as i64 + origin.1) < a { 1.0 } else { 0.0 };
        }
        composite_px(mode, d, color(i), a);
    }
}

const fn rgb(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn composite_styled(base: &Node, st: &LayerStyle, clipped: &[Node], acc: &mut Accum, lod: u8, key: (i32, i32), origin: (i64, i64), sub: Sub) {
    let Some(tiles) = base.tiles() else { return };
    let s = 1.0 / (1u32 << lod) as f32;
    let reach = st.reach(s) as i64;
    let t = TILE as i64;
    let (x0, y0) = (key.0 as i64 * t - reach, key.1 as i64 * t - reach);
    let (w, h) = ((t + 2 * reach) as usize, (t + 2 * reach) as usize);
    let ck = (Arc::as_ptr(tiles) as usize, st.key(), mask_key(base), lod, key.0, key.1);
    let cached = mattes_cache().lock().unwrap().get(&ck).cloned();
    let c = cached.unwrap_or_else(|| {
        let alpha = shape_region(base, tiles, lod, x0, y0, w, h);
        let r = reach as usize;
        let mut shape = vec![0.0; TILE * TILE];
        for y in 0..TILE {
            shape[y * TILE..(y + 1) * TILE].copy_from_slice(&alpha[(y + r) * w + r..(y + r) * w + r + TILE]);
        }
        let c = Arc::new(Cached { mattes: style::mattes(st, w, h, &alpha, r, r, s), shape });
        let mut m = mattes_cache().lock().unwrap();
        if m.len() > 3000 {
            m.clear();
        }
        m.insert(ck, c.clone());
        c
    });
    // effects below the layer, on the backdrop
    let mut with = Accum(acc.0.clone());
    if let (Some(e), Some(m)) = (st.drop_shadow(), &c.mattes.drop_shadow) {
        paint(&mut with, m, e.opacity, e.blend, |_| rgb(e.color), origin);
    }
    if let (Some(e), Some(m)) = (st.outer_glow(), &c.mattes.outer_glow) {
        paint(&mut with, m, e.opacity, e.blend, |_| rgb(e.color), origin);
    }
    // the layer itself: its pixels at the fill opacity (a filter preview may stand in for them)
    let content = match sub {
        Some(sb) if sb.layer == base.id => (sb.tile)(lod, key),
        _ => layer_tile(tiles, lod, key),
    };
    let mut layer = match content {
        Some(tile) => Accum::from_tile(&tile, base.fill),
        None => Accum::new(),
    };
    match mask_factor(base, lod, key) {
        None => {}
        Some(MaskFactor::Const(k)) => layer.0.iter_mut().for_each(|p| p.iter_mut().for_each(|v| *v *= k)),
        Some(MaskFactor::Each(v)) => layer.mul_factors(&v),
    }
    if let (Some(e), Some(m)) = (st.inner_shadow(), &c.mattes.inner_shadow) {
        paint(&mut layer, m, e.opacity, e.blend, |_| rgb(e.color), origin);
    }
    if let Some(e) = st.color_overlay() {
        paint(&mut layer, &c.shape, e.opacity, e.blend, |_| rgb(e.color), origin);
    }
    if let Some(e) = st.gradient_overlay() {
        let b = bounds(tiles).map_or((0.0, 0.0, 1.0, 1.0), |b| (b.0 as f32 * s, b.1 as f32 * s, b.2 as f32 * s, b.3 as f32 * s));
        let (ox, oy) = (origin.0 as f32, origin.1 as f32);
        paint(&mut layer, &c.shape, e.opacity, e.blend, |i| style::gradient_color(&e.stops, style::overlay_t(e, ox + (i % TILE) as f32 + 0.5, oy + (i / TILE) as f32 + 0.5, b)), origin);
    }
    clip_onto(&mut layer, clipped, lod, key, origin, sub);
    if let (Some(e), Some(m)) = (st.stroke(), &c.mattes.stroke) {
        paint(&mut layer, m, e.opacity, e.blend, |_| rgb(e.color), origin);
    }
    with.composite_accum(&layer, base.blend, 1.0, origin);
    // the layer's opacity fades it all, effects included
    acc.lerp_to(&with, base.opacity);
}

/// Sets (or clears) a layer's effects.
pub fn set_style(doc: &mut Document, id: u64, style: Option<LayerStyle>, transient: bool) -> OpResult {
    let node = doc.content.find_mut(id).ok_or("layer not found")?;
    if node.is_group() || node.is_adjustment() {
        return Err("effects apply to pixel and text layers".into());
    }
    node.style = style.filter(|s| *s != LayerStyle::default());
    let name = node.name.clone();
    if transient {
        doc.touch();
    } else {
        doc.commit(Label::with("history.layerStyle", name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, raster, Content};
    use lawha_core::style::{ColorOverlay, Shadow, Stroke, StrokePosition};

    /// White background, and a 100×100 blue square at (100, 100) on its own layer.
    fn doc() -> (Document, u64) {
        let bg = Node::pixel("bg", filled_tiles(400, 400, [255, 255, 255, 255]));
        let sq: Vec<u8> = [30, 60, 220, 255].repeat(100 * 100);
        let layer = Node::pixel("sq", raster::write(&sq, 100, 100, 100, 100));
        let id = layer.id;
        (Document::new("t".into(), None, Content::new(400, 400, 72.0, id, vec![bg, layer]), Label::new("history.new")), id)
    }

    fn px(d: &Document, x: usize, y: usize) -> [u8; 4] {
        composite_lod0(&d.content, (x / 256) as i32, (y / 256) as i32).unwrap().px(x % 256, y % 256)
    }

    fn shadow() -> Shadow {
        Shadow { enabled: true, blend: BlendMode::Multiply, color: [0, 0, 0], opacity: 1.0, angle: 135.0, distance: 20.0, spread: 100.0, size: 0.0, knockout: true }
    }

    #[test]
    fn a_drop_shadow_falls_beside_the_layer() {
        let (mut d, id) = doc();
        set_style(&mut d, id, Some(LayerStyle { drop_shadow: Some(shadow()), ..Default::default() }), false).unwrap();
        // 20 px at 135°: ~14 px right and down
        assert_eq!(px(&d, 210, 210), [0, 0, 0, 255], "shadow");
        assert_eq!(px(&d, 150, 150), [30, 60, 220, 255], "the layer covers its shadow");
        assert_eq!(px(&d, 90, 90), [255, 255, 255, 255], "nothing on the lit side");
        d.undo();
        assert_eq!(px(&d, 210, 210), [255, 255, 255, 255]);
    }

    #[test]
    fn fill_hides_the_pixels_but_not_the_effects_and_opacity_fades_all() {
        let (mut d, id) = doc();
        let overlay = ColorOverlay { enabled: true, blend: BlendMode::Normal, color: [250, 20, 20], opacity: 1.0 };
        set_style(&mut d, id, Some(LayerStyle { color_overlay: Some(overlay), drop_shadow: Some(shadow()), ..Default::default() }), false).unwrap();
        assert_eq!(px(&d, 150, 150), [250, 20, 20, 255], "overlay covers the layer");
        d.content.find_mut(id).unwrap().fill = 0.0;
        assert_eq!(px(&d, 150, 150), [250, 20, 20, 255], "fill 0 still shows the overlay");
        assert_eq!(px(&d, 210, 210), [0, 0, 0, 255], "and the shadow");
        d.content.find_mut(id).unwrap().opacity = 0.5;
        let p = px(&d, 210, 210);
        assert!((p[0] as i32 - 128).abs() <= 1, "opacity fades the shadow too: {p:?}");
    }

    #[test]
    fn an_outside_stroke_rings_the_shape_and_follows_the_mask() {
        let (mut d, id) = doc();
        let stroke = Stroke { enabled: true, blend: BlendMode::Normal, color: [255, 200, 0], opacity: 1.0, size: 6.0, position: StrokePosition::Outside };
        set_style(&mut d, id, Some(LayerStyle { stroke: Some(stroke), ..Default::default() }), false).unwrap();
        assert_eq!(px(&d, 97, 150), [255, 200, 0, 255]);
        assert_eq!(px(&d, 102, 150), [30, 60, 220, 255]);
        assert_eq!(px(&d, 90, 150), [255, 255, 255, 255]);
        // a mask hiding the left half: the stroke follows the visible shape
        let mut m = vec![255u8; 400 * 400];
        for y in 0..400 {
            for x in 0..150 {
                m[y * 400 + x] = 0;
            }
        }
        d.content.find_mut(id).unwrap().mask = Some(crate::doc::model::LayerMask::new(raster::mask_write(&m, 0, 0, 400, 400, 0)));
        assert_eq!(px(&d, 97, 150), [255, 255, 255, 255], "no stroke where the layer is hidden");
        assert_eq!(px(&d, 147, 150), [255, 200, 0, 255], "the stroke runs outside the visible edge");
        assert_eq!(px(&d, 152, 150), [30, 60, 220, 255]);
    }

    #[test]
    fn styled_layers_save_and_load_and_stop_the_gpu_split() {
        let (mut d, id) = doc();
        set_style(&mut d, id, Some(LayerStyle { drop_shadow: Some(shadow()), ..Default::default() }), false).unwrap();
        assert!(crate::doc::live::Session::begin(1, &d.content, id).is_none());
        // clearing every effect removes the style
        set_style(&mut d, id, Some(LayerStyle::default()), false).unwrap();
        assert!(d.content.find(id).unwrap().style.is_none());
    }
}
