//! Flattening the layer tree into tiles, per mip level, computed on demand and cached until the
//! document changes.
//!
//! Rules (docs/research.md §2):
//! - Pass-through groups: children blend straight into the backdrop; group opacity fades their
//!   combined effect.
//! - Isolated groups (any other mode): children composite onto transparency first, then the result
//!   blends as one layer.
//! - Clipping: a base layer and the clipped layers above it form a unit. Clipped layers draw only
//!   where the base has alpha (source-atop), and the unit blends with the base's mode and opacity.
//! - Fill fades a layer's own pixels; opacity fades everything (styles arrive in M8).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, Weak};

use mosamem_core::adjust::{apply_to_accum, Op};
use mosamem_core::mask::{Mask, MaskMap, MaskTile};
use mosamem_core::tile::{downsample, Accum, Tile, TILE};
use parking_lot::Mutex;

use super::model::{Content, LayerMask, Node, NodeKind, TileMap};
use super::DocHandle;

pub const MAX_LOD: u8 = 6;

#[derive(Default)]
pub struct Cache {
    inner: Mutex<CacheInner>,
}

#[derive(Default)]
struct CacheInner {
    version: u64,
    tiles: HashMap<(u8, i32, i32), Option<Arc<Tile>>>,
}

impl Cache {
    fn get(&self, version: u64, key: (u8, i32, i32)) -> Option<Option<Arc<Tile>>> {
        let mut c = self.inner.lock();
        if c.version != version {
            c.version = version;
            c.tiles.clear();
            return None;
        }
        c.tiles.get(&key).cloned()
    }

    fn put(&self, version: u64, key: (u8, i32, i32), tile: Option<Arc<Tile>>) {
        let mut c = self.inner.lock();
        if c.version == version {
            c.tiles.insert(key, tile);
        }
    }

    /// Forgets the cached composite at these lod-0 tiles and every coarser level above them (used
    /// while painting, when the document version stays the same until the stroke ends).
    pub fn invalidate(&self, keys: &[(i32, i32)]) {
        let mut c = self.inner.lock();
        for &(tx, ty) in keys {
            for lod in 0..=MAX_LOD {
                c.tiles.remove(&(lod, tx >> lod, ty >> lod));
            }
        }
    }
}

/// What one node contributes to a tile, before its own blend mode and opacity are applied.
enum Source {
    Tile(Arc<Tile>, f32),
    Accum(Accum),
}

type MipKey = (usize, u8, i32, i32);
type LayerMips = HashMap<MipKey, (Weak<TileMap>, Option<Arc<Tile>>)>;
type MaskMips = HashMap<MipKey, (Weak<MaskMap>, Option<Arc<MaskTile>>)>;
static LAYER_MIPS: OnceLock<Mutex<LayerMips>> = OnceLock::new();
static MASK_MIPS: OnceLock<Mutex<MaskMips>> = OnceLock::new();
const MIP_CACHE_LIMIT: usize = 60_000;

/// A layer's pixels at a mip level (lod 0 = its own tiles). Zoomed-out views composite these
/// directly, so a change to one layer only recomputes that layer's mips. Cached by the layer's tile
/// map, which is never mutated in place (edits make a new map).
pub fn layer_tile(tiles: &Arc<TileMap>, lod: u8, key: (i32, i32)) -> Option<Arc<Tile>> {
    if lod == 0 {
        return tiles.get(&key).cloned();
    }
    let ck = (Arc::as_ptr(tiles) as usize, lod, key.0, key.1);
    let cache = LAYER_MIPS.get_or_init(Default::default);
    if let Some((w, t)) = cache.lock().get(&ck) {
        if w.upgrade().is_some_and(|a| Arc::ptr_eq(&a, tiles)) {
            return t.clone();
        }
    }
    let kids: Vec<Option<Arc<Tile>>> = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| layer_tile(tiles, lod - 1, (key.0 * 2 + dx, key.1 * 2 + dy))).collect();
    let out = if kids.iter().all(Option::is_none) {
        None
    } else {
        let mut o = Tile::transparent();
        downsample([kids[0].as_deref(), kids[1].as_deref(), kids[2].as_deref(), kids[3].as_deref()], &mut o);
        Some(Arc::new(o))
    };
    let mut c = cache.lock();
    if c.len() > MIP_CACHE_LIMIT {
        c.retain(|_, (w, _)| w.strong_count() > 0);
        if c.len() > MIP_CACHE_LIMIT / 2 {
            c.clear();
        }
    }
    c.insert(ck, (Arc::downgrade(tiles), out.clone()));
    out
}

/// A mask at a mip level; `None` where the whole tile has the mask's outside value.
pub(crate) fn mask_tile(m: &Mask, lod: u8, key: (i32, i32)) -> Option<Arc<MaskTile>> {
    if lod == 0 {
        return m.tiles.get(&key).cloned();
    }
    let ck = (Arc::as_ptr(&m.tiles) as usize, lod, key.0, key.1);
    let cache = MASK_MIPS.get_or_init(Default::default);
    if let Some((w, t)) = cache.lock().get(&ck) {
        if w.upgrade().is_some_and(|a| Arc::ptr_eq(&a, &m.tiles)) {
            return t.clone();
        }
    }
    let kids: Vec<Option<Arc<MaskTile>>> = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| mask_tile(m, lod - 1, (key.0 * 2 + dx, key.1 * 2 + dy))).collect();
    let out = if kids.iter().all(Option::is_none) {
        None
    } else {
        let mut o = MaskTile::filled(m.outside);
        const H: usize = TILE / 2;
        for (q, k) in kids.iter().enumerate() {
            let (ox, oy) = ((q % 2) * H, (q / 2) * H);
            for y in 0..H {
                for x in 0..H {
                    let v = match k {
                        Some(k) => (k.get(2 * x, 2 * y) as u32 + k.get(2 * x + 1, 2 * y) as u32 + k.get(2 * x, 2 * y + 1) as u32 + k.get(2 * x + 1, 2 * y + 1) as u32 + 2) / 4,
                        None => m.outside as u32,
                    };
                    o.0[(oy + y) * TILE + ox + x] = v as u8;
                }
            }
        }
        Some(Arc::new(o))
    };
    let mut c = cache.lock();
    if c.len() > MIP_CACHE_LIMIT {
        c.clear();
    }
    c.insert(ck, (Arc::downgrade(&m.tiles), out.clone()));
    out
}

/// The layer mask's effect on one tile: None = no mask (or disabled), Const = the same factor
/// everywhere, Each = one factor per pixel. Factors include density: f = 1 − density·(1 − m).
pub enum MaskFactor {
    Const(f32),
    Each(Vec<f32>),
}

type Feathered = HashMap<(usize, u32), (Weak<MaskMap>, Arc<Mask>)>;
static FEATHERED: OnceLock<Mutex<Feathered>> = OnceLock::new();

/// The mask after its render-time feather, cached by mask content and radius.
fn feathered(m: &LayerMask) -> Arc<Mask> {
    if m.feather <= 0.0 {
        return Arc::new(m.mask.clone()); // shares the same tile map (its pointer keys the mip cache)
    }
    let key = (Arc::as_ptr(&m.mask.tiles) as usize, m.feather.to_bits());
    let cache = FEATHERED.get_or_init(Default::default);
    // the weak handle keeps the address from being reused while the entry exists
    if let Some((w, hit)) = cache.lock().get(&key) {
        if w.strong_count() > 0 {
            return hit.clone();
        }
    }
    // the region the stored tiles cover, plus room for the blur to spread
    let margin = ((m.feather * 2.0) as i32 / TILE as i32) + 1;
    let (mx, my) = m.mask.tiles.keys().fold((0, 0), |(a, b), (x, y)| (a.max(*x), b.max(*y)));
    let (w, h) = (((mx + 1 + margin) * TILE as i32) as u32, ((my + 1 + margin) * TILE as i32) as u32);
    let dense = mosamem_core::select::feather(&m.mask.to_dense(w, h), m.feather);
    let out = Arc::new(Mask::from_dense(&dense, m.mask.outside));
    let mut c = cache.lock();
    if c.len() > 64 {
        c.clear();
    }
    c.insert(key, (Arc::downgrade(&m.mask.tiles), out.clone()));
    out
}

pub fn mask_factor(node: &Node, lod: u8, key: (i32, i32)) -> Option<MaskFactor> {
    let lm = node.mask.as_ref().filter(|m| m.enabled)?;
    let mask = feathered(lm);
    let d = lm.density;
    Some(match mask_tile(&mask, lod, key) {
        None => MaskFactor::Const(1.0 - d * (1.0 - mask.outside as f32 / 255.0)),
        Some(t) => MaskFactor::Each(t.0.iter().map(|m| 1.0 - d * (1.0 - *m as f32 / 255.0)).collect()),
    })
}

fn apply_mask(acc: &mut Accum, f: &MaskFactor) {
    match f {
        MaskFactor::Const(k) if *k >= 1.0 => {}
        MaskFactor::Const(k) => {
            for p in acc.0.iter_mut() {
                for c in p.iter_mut() {
                    *c *= k;
                }
            }
        }
        MaskFactor::Each(v) => acc.mul_factors(v),
    }
}

/// Isolated content of a node at one tile, with its layer mask applied (None when it draws
/// nothing there).
fn isolated(node: &Node, lod: u8, key: (i32, i32), origin: (i64, i64), sub: Sub) -> Option<Source> {
    let mask = mask_factor(node, lod, key);
    if matches!(mask, Some(MaskFactor::Const(k)) if k <= 0.0) {
        return None;
    }
    let src = match &node.kind {
        NodeKind::Pixel { tiles } | NodeKind::Text { tiles, .. } | NodeKind::Shape { tiles, .. } => match sub {
            Some(s) if s.layer == node.id => Source::Tile((s.tile)(lod, key)?, node.fill),
            _ => Source::Tile(layer_tile(tiles, lod, key)?, node.fill),
        },
        // adjustments have no content of their own (see `adjust`)
        NodeKind::Adjustment { .. } => return None,
        NodeKind::Group { children, .. } => {
            let mut acc = Accum::new();
            stack(children, &mut acc, lod, key, origin, sub);
            if acc.is_empty() {
                return None;
            }
            Source::Accum(acc)
        }
    };
    Some(match (src, mask) {
        (s, None) => s,
        (Source::Tile(t, fill), Some(f)) => {
            let mut a = Accum::from_tile(&t, fill);
            apply_mask(&mut a, &f);
            Source::Accum(a)
        }
        (Source::Accum(mut a), Some(f)) => {
            apply_mask(&mut a, &f);
            Source::Accum(a)
        }
    })
}

/// Composites a list of sibling nodes (bottom to top) onto `acc`, at a mip level (lod 0 = full
/// resolution; above that, the layers' own mips are blended, which is how zoomed-out views stay
/// cheap).
pub fn composite_stack(nodes: &[Node], acc: &mut Accum, lod: u8, key: (i32, i32), origin: (i64, i64)) {
    stack(nodes, acc, lod, key, origin, None);
}

/// Pixels to use instead of one layer's own (a filter preview), at a level and tile.
pub struct Substitute<'a> {
    pub layer: u64,
    pub tile: &'a (dyn Fn(u8, (i32, i32)) -> Option<Arc<Tile>> + Sync),
}
pub(crate) type Sub<'a> = Option<&'a Substitute<'a>>;

fn stack(nodes: &[Node], acc: &mut Accum, lod: u8, key: (i32, i32), origin: (i64, i64), sub: Sub) {
    let mut i = 0;
    while i < nodes.len() {
        let base = &nodes[i];
        let mut j = i + 1;
        while j < nodes.len() && nodes[j].clipped {
            j += 1;
        }
        // a clipped layer whose base is the first sibling (no base below) is drawn normally
        let clipped = &nodes[i + 1..j];
        if base.visible && base.opacity > 0.0 {
            composite_unit(base, clipped, acc, lod, key, origin, sub);
        }
        i = j;
    }
}

/// Applies an adjustment layer to what is below it, through its mask, opacity and fill.
fn adjust(node: &Node, op: &Op, acc: &mut Accum, lod: u8, key: (i32, i32), origin: (i64, i64)) {
    let k = node.opacity * node.fill;
    match mask_factor(node, lod, key) {
        None => apply_to_accum(acc, op, node.blend, k, None, origin),
        Some(MaskFactor::Const(m)) => apply_to_accum(acc, op, node.blend, k * m, None, origin),
        Some(MaskFactor::Each(v)) => apply_to_accum(acc, op, node.blend, k, Some(&v), origin),
    }
}

fn composite_unit(base: &Node, clipped: &[Node], acc: &mut Accum, lod: u8, key: (i32, i32), origin: (i64, i64), sub: Sub) {
    // an adjustment layer changes the backdrop; layers clipped to it have no content to clip to
    if let NodeKind::Adjustment { op, .. } = &base.kind {
        adjust(base, op, acc, lod, key, origin);
        return;
    }
    // a layer with effects
    if let Some(style) = base.active_style() {
        super::styles::composite_styled(base, style, clipped, acc, lod, key, origin, sub);
        return;
    }
    let has_clips = clipped.iter().any(|c| c.visible && c.opacity > 0.0);
    // pass-through group without clipped layers: children go straight onto the backdrop
    if let NodeKind::Group { children, pass_through: true, .. } = &base.kind {
        if !has_clips {
            let mask = mask_factor(base, lod, key);
            if base.opacity >= 1.0 && mask.is_none() {
                stack(children, acc, lod, key, origin, sub);
                return;
            }
            let mut with = Accum(acc.0.clone());
            stack(children, &mut with, lod, key, origin, sub);
            match mask {
                None => acc.lerp_to(&with, base.opacity),
                Some(MaskFactor::Const(k)) => acc.lerp_to(&with, base.opacity * k),
                Some(MaskFactor::Each(v)) => acc.lerp_to_each(&with, &v.iter().map(|k| k * base.opacity).collect::<Vec<_>>()),
            }
            return;
        }
    }
    let Some(src) = isolated(base, lod, key, origin, sub) else { return };
    if !has_clips {
        match src {
            Source::Tile(t, fill) => acc.composite(&t, base.blend, base.opacity * fill, origin),
            Source::Accum(a) => acc.composite_accum(&a, base.blend, base.opacity, origin),
        }
        return;
    }
    // clipping unit: base content, then each clipped layer source-atop, then blend the unit
    let mut unit = match src {
        Source::Tile(t, fill) => Accum::from_tile(&t, fill),
        Source::Accum(a) => a,
    };
    clip_onto(&mut unit, clipped, lod, key, origin, sub);
    acc.composite_accum(&unit, base.blend, base.opacity, origin);
}

/// Layers clipped to a base, drawn onto the base's content (source-atop).
pub(crate) fn clip_onto(unit: &mut Accum, clipped: &[Node], lod: u8, key: (i32, i32), origin: (i64, i64), sub: Sub) {
    for c in clipped.iter().filter(|c| c.visible && c.opacity > 0.0) {
        // a clipped adjustment changes only its base
        if let NodeKind::Adjustment { op, .. } = &c.kind {
            adjust(c, op, unit, lod, key, origin);
            continue;
        }
        match isolated(c, lod, key, origin, sub) {
            Some(Source::Tile(t, fill)) => unit.composite_atop(&t, c.blend, c.opacity * fill),
            Some(Source::Accum(a)) => unit.composite_accum_atop(&a, c.blend, c.opacity),
            None => {}
        }
    }
}

/// Composite of a list of nodes at lod 0 for one tile, onto transparency; `None` when empty.
pub fn composite_nodes(nodes: &[Node], tx: i32, ty: i32) -> Option<Tile> {
    composite_nodes_lod(nodes, 0, tx, ty)
}

/// Composite of a list of nodes at a mip level, onto transparency; `None` when empty.
pub fn composite_nodes_lod(nodes: &[Node], lod: u8, tx: i32, ty: i32) -> Option<Tile> {
    composite_nodes_with(nodes, lod, tx, ty, None)
}

/// Like `composite_nodes_lod`, with one layer's pixels substituted.
pub fn composite_nodes_with(nodes: &[Node], lod: u8, tx: i32, ty: i32, sub: Option<&Substitute>) -> Option<Tile> {
    let mut acc = Accum::new();
    stack(nodes, &mut acc, lod, (tx, ty), (tx as i64 * TILE as i64, ty as i64 * TILE as i64), sub);
    if acc.is_empty() {
        return None;
    }
    let mut out = Tile::transparent();
    acc.to_tile(&mut out);
    Some(out)
}

/// Composite of the whole tree at lod 0 for one tile; `None` when fully transparent.
#[cfg(test)]
pub fn composite_lod0(content: &Content, tx: i32, ty: i32) -> Option<Tile> {
    composite_nodes(&content.root, tx, ty)
}

/// Composite tile at `lod` (tile size stays 256 px; one tile covers 256·2^lod document pixels).
pub fn tile(h: &DocHandle, lod: u8, tx: i32, ty: i32) -> Option<Arc<Tile>> {
    let doc = h.doc.read();
    tile_in(h, &doc.content, doc.version, lod, tx, ty)
}

fn tile_in(h: &DocHandle, c: &Content, version: u64, lod: u8, tx: i32, ty: i32) -> Option<Arc<Tile>> {
    let (tw, th) = c.tiles_wide();
    let scale = 1i32 << lod;
    if tx < 0 || ty < 0 || tx * scale >= tw || ty * scale >= th {
        return None;
    }
    let key = (lod, tx, ty);
    if let Some(hit) = h.composite.get(version, key) {
        return hit;
    }
    // full resolution is exact; zoomed-out levels blend the layers' own mips (display only)
    let result = composite_nodes_lod(&c.root, lod, tx, ty).map(Arc::new);
    h.composite.put(version, key, result.clone());
    result
}

/// Selection (or refine preview) as display tiles: the coverage repeated in all four channels.
/// Mip levels average 2×2 blocks.
pub fn selection_tile(h: &DocHandle, lod: u8, tx: i32, ty: i32) -> Option<Arc<Vec<u8>>> {
    let doc = h.doc.read();
    let mask = doc.preview.as_ref().or(doc.content.selection.as_ref())?.clone();
    let version = doc.version;
    let (tw, th) = doc.content.tiles_wide();
    drop(doc);
    fn level(h: &DocHandle, m: &Mask, version: u64, tw: i32, th: i32, lod: u8, tx: i32, ty: i32) -> Option<Arc<Vec<u8>>> {
        let scale = 1i32 << lod;
        if tx < 0 || ty < 0 || tx * scale >= tw || ty * scale >= th {
            return None;
        }
        if let Some(hit) = h.selection.get(version, (lod, tx, ty)) {
            return hit;
        }
        let out: Option<Arc<Vec<u8>>> = if lod == 0 {
            let vals: Option<Vec<u8>> = match m.tile((tx, ty)) {
                Some(t) => Some(t.0.to_vec()),
                None if m.outside > 0 => Some(vec![m.outside; TILE * TILE]),
                None => None,
            };
            vals.filter(|v| v.iter().any(|x| *x > 0)).map(|v| Arc::new(v.iter().flat_map(|x| [*x; 4]).collect()))
        } else {
            let kids: Vec<Option<Arc<Vec<u8>>>> = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| level(h, m, version, tw, th, lod - 1, tx * 2 + dx, ty * 2 + dy)).collect();
            if kids.iter().all(Option::is_none) {
                None
            } else {
                let mut o = vec![0u8; TILE * TILE * 4];
                const H: usize = TILE / 2;
                for (q, k) in kids.iter().enumerate() {
                    let Some(k) = k else { continue };
                    let (ox, oy) = ((q % 2) * H, (q / 2) * H);
                    for y in 0..H {
                        for x in 0..H {
                            let s = |dx: usize, dy: usize| k[((2 * y + dy) * TILE + 2 * x + dx) * 4] as u32;
                            let v = ((s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1) + 2) / 4) as u8;
                            let i = ((oy + y) * TILE + ox + x) * 4;
                            o[i..i + 4].copy_from_slice(&[v; 4]);
                        }
                    }
                }
                Some(Arc::new(o))
            }
        };
        h.selection.put(version, (lod, tx, ty), out.clone());
        out
    }
    level(h, &mask, version, tw, th, lod, tx, ty)
}

/// Version-keyed cache of selection display tiles.
#[derive(Default)]
pub struct SelCache {
    inner: Mutex<(u64, HashMap<(u8, i32, i32), Option<Arc<Vec<u8>>>>)>,
}

impl SelCache {
    fn get(&self, version: u64, key: (u8, i32, i32)) -> Option<Option<Arc<Vec<u8>>>> {
        let mut c = self.inner.lock();
        if c.0 != version {
            c.0 = version;
            c.1.clear();
            return None;
        }
        c.1.get(&key).cloned()
    }
    fn put(&self, version: u64, key: (u8, i32, i32), v: Option<Arc<Vec<u8>>>) {
        let mut c = self.inner.lock();
        if c.0 == version {
            c.1.insert(key, v);
        }
    }
}

/// Copies tiles into a full RGBA8 image (straight alpha).
pub fn assemble(width: u32, height: u32, tiles: impl IntoIterator<Item = ((i32, i32), Arc<Tile>)>) -> Vec<u8> {
    let (w, hgt) = (width as usize, height as usize);
    let mut out = vec![0u8; w * hgt * 4];
    for ((tx, ty), t) in tiles {
        let (x0, y0) = (tx as usize * TILE, ty as usize * TILE);
        if tx < 0 || ty < 0 || x0 >= w || y0 >= hgt {
            continue;
        }
        let cw = (w - x0).min(TILE);
        for y in 0..(hgt - y0).min(TILE) {
            let dst = ((y0 + y) * w + x0) * 4;
            out[dst..dst + cw * 4].copy_from_slice(&t.0[y * TILE * 4..y * TILE * 4 + cw * 4]);
        }
    }
    out
}

/// The whole flattened image as RGBA8 (straight alpha), for export.
pub fn flatten(h: &DocHandle) -> (u32, u32, Vec<u8>) {
    use rayon::prelude::*;
    let doc = h.doc.read();
    let c = &doc.content;
    let (tw, th) = c.tiles_wide();
    let tiles: Vec<((i32, i32), Arc<Tile>)> = (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|(tx, ty)| tile_in(h, c, doc.version, 0, tx, ty).map(|t| ((tx, ty), t)))
        .collect();
    (c.width, c.height, assemble(c.width, c.height, tiles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{filled_tiles, tiles_from_rgba, Content, Document, Engine, Label, Node};
    use mosamem_core::BlendMode;

    pub fn doc(root: Vec<Node>, w: u32, h: u32) -> Document {
        let c = Content::new(w, h, 72.0, 0, root);
        Document::new("t".into(), None, c, Label::new("history.open"))
    }

    fn px(content: &Content, x: usize, y: usize) -> [u8; 4] {
        composite_lod0(content, (x / TILE) as i32, (y / TILE) as i32).map(|t| t.px(x % TILE, y % TILE)).unwrap_or([0; 4])
    }

    fn solid(name: &str, rgba: [u8; 4]) -> Node {
        Node::pixel(name, filled_tiles(300, 300, rgba))
    }

    #[test]
    fn flatten_round_trips_a_single_layer() {
        let (w, h) = (600, 300);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 13) as u8, 7, 255]).collect();
        let e = Engine::default();
        let hd = e.insert(doc(vec![Node::pixel("a", tiles_from_rgba(w, h, &rgba))], w, h));
        let (_, _, out) = flatten(&hd);
        assert_eq!(out, rgba);
    }

    #[test]
    fn half_opacity_white_over_black_is_mid_gray() {
        let mut top = solid("w", [255, 255, 255, 255]);
        top.opacity = 0.5;
        let d = doc(vec![solid("b", [0, 0, 0, 255]), top], 300, 300);
        assert_eq!(px(&d.content, 10, 10), [128, 128, 128, 255]);
    }

    #[test]
    fn fill_and_opacity_multiply_for_plain_layers() {
        let mut top = solid("w", [255, 255, 255, 255]);
        top.opacity = 0.5;
        top.fill = 0.5;
        let d = doc(vec![solid("b", [0, 0, 0, 255]), top], 300, 300);
        assert_eq!(px(&d.content, 5, 5), [64, 64, 64, 255]);
    }

    #[test]
    fn hidden_layers_and_groups_draw_nothing() {
        let mut top = solid("w", [255, 0, 0, 255]);
        top.visible = false;
        let mut g = Node::group("g", vec![solid("x", [0, 255, 0, 255])]);
        g.visible = false;
        let d = doc(vec![solid("b", [0, 0, 255, 255]), top, g], 300, 300);
        assert_eq!(px(&d.content, 1, 1), [0, 0, 255, 255]);
    }

    #[test]
    fn pass_through_group_lets_children_blend_with_the_backdrop() {
        // a Multiply child inside a pass-through group multiplies with the layer below the group
        let mut child = solid("m", [128, 128, 128, 255]);
        child.blend = BlendMode::Multiply;
        let g = Node::group("g", vec![child.clone()]);
        let d = doc(vec![solid("b", [200, 100, 50, 255]), g], 300, 300);
        assert_eq!(px(&d.content, 3, 3), [100, 50, 25, 255]);

        // the same group isolated (Normal): the multiply happens against transparency first, so the
        // gray simply covers the backdrop
        let mut g2 = Node::group("g", vec![child]);
        if let NodeKind::Group { pass_through, .. } = &mut g2.kind {
            *pass_through = false;
        }
        let d2 = doc(vec![solid("b", [200, 100, 50, 255]), g2], 300, 300);
        assert_eq!(px(&d2.content, 3, 3), [128, 128, 128, 255]);
    }

    #[test]
    fn pass_through_group_opacity_fades_its_effect() {
        let mut g = Node::group("g", vec![solid("w", [255, 255, 255, 255])]);
        g.opacity = 0.25;
        let d = doc(vec![solid("b", [0, 0, 0, 255]), g], 300, 300);
        assert_eq!(px(&d.content, 3, 3), [64, 64, 64, 255]);
    }

    #[test]
    fn clipped_layer_draws_only_inside_its_base() {
        // base: opaque only on the left half; clipped: red everywhere
        let mut rgba = vec![0u8; 300 * 300 * 4];
        for y in 0..300 {
            for x in 0..150 {
                rgba[(y * 300 + x) * 4..(y * 300 + x) * 4 + 4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let base = Node::pixel("base", tiles_from_rgba(300, 300, &rgba));
        let mut clip = solid("clip", [255, 0, 0, 255]);
        clip.clipped = true;
        let d = doc(vec![solid("bg", [255, 255, 255, 255]), base, clip], 300, 300);
        assert_eq!(px(&d.content, 10, 10), [255, 0, 0, 255], "inside the base");
        assert_eq!(px(&d.content, 200, 10), [255, 255, 255, 255], "outside the base");
    }

    #[test]
    fn clipping_unit_uses_the_base_opacity() {
        let mut base = solid("base", [0, 0, 255, 255]);
        base.opacity = 0.5;
        let mut clip = solid("clip", [255, 0, 0, 255]);
        clip.clipped = true;
        let d = doc(vec![solid("bg", [0, 0, 0, 255]), base, clip], 300, 300);
        assert_eq!(px(&d.content, 5, 5), [128, 0, 0, 255]);
    }

    #[test]
    fn layer_mask_hides_where_black_and_density_softens_it() {
        use mosamem_core::mask::Dense;
        let mut top = solid("w", [255, 255, 255, 255]);
        // mask: left half white (show), right half black (hide)
        let mut d = Dense::new(300, 300, 0);
        for y in 0..300 {
            for x in 0..150 {
                d.data[y * 300 + x] = 255;
            }
        }
        top.mask = Some(crate::doc::model::LayerMask::new(Mask::from_dense(&d, 0)));
        let c = Content::new(300, 300, 72.0, 0, vec![solid("b", [0, 0, 0, 255]), top.clone()]);
        assert_eq!(px(&c, 10, 10), [255, 255, 255, 255]);
        assert_eq!(px(&c, 290, 10), [0, 0, 0, 255]);
        // density 50%: hidden areas show at half strength
        top.mask.as_mut().unwrap().density = 0.5;
        let c = Content::new(300, 300, 72.0, 0, vec![solid("b", [0, 0, 0, 255]), top.clone()]);
        assert_eq!(px(&c, 290, 10), [128, 128, 128, 255]);
        // disabled: no effect
        top.mask.as_mut().unwrap().enabled = false;
        let c = Content::new(300, 300, 72.0, 0, vec![solid("b", [0, 0, 0, 255]), top]);
        assert_eq!(px(&c, 290, 10), [255, 255, 255, 255]);
    }

    #[test]
    fn feathered_mask_ramps_across_the_edge() {
        use mosamem_core::mask::Dense;
        let mut top = solid("w", [255, 255, 255, 255]);
        let mut d = Dense::new(300, 300, 0);
        for y in 0..300 {
            for x in 0..150 {
                d.data[y * 300 + x] = 255;
            }
        }
        let mut lm = crate::doc::model::LayerMask::new(Mask::from_dense(&d, 0));
        lm.feather = 20.0;
        top.mask = Some(lm);
        let c = Content::new(300, 300, 72.0, 0, vec![solid("b", [0, 0, 0, 255]), top]);
        let mid = px(&c, 150, 100)[0];
        assert!(mid > 100 && mid < 155, "{mid}");
        assert!(px(&c, 120, 100)[0] > px(&c, 140, 100)[0]);
    }

    #[test]
    fn mip_levels_average_and_cover_edges() {
        let e = Engine::default();
        let hd = e.insert(doc(vec![Node::pixel("w", filled_tiles(1000, 600, [200, 100, 50, 255]))], 1000, 600));
        let t = tile(&hd, 2, 0, 0).unwrap();
        assert_eq!(t.px(0, 0), [200, 100, 50, 255]);
        assert_eq!(t.px(249, 149), [200, 100, 50, 255]);
        assert_eq!(t.px(251, 10)[3], 0);
        assert!(tile(&hd, 2, 1, 0).is_none());
    }

    /// Throughput of the CPU compositor (run with `cargo test --release -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn bench_composite_30_full_layers() {
        let (w, h) = (2048, 2048);
        let root: Vec<Node> = (0..30)
            .map(|i| {
                let mut l = Node::pixel(format!("l{i}"), filled_tiles(w, h, [(i * 8) as u8, 100, 200, if i == 0 { 255 } else { 90 }]));
                l.blend = if i % 3 == 1 { BlendMode::Multiply } else { BlendMode::Normal };
                l
            })
            .collect();
        let d = doc(root, w, h);
        let t = std::time::Instant::now();
        let n = 64;
        for i in 0..n {
            composite_lod0(&d.content, i % 8, i / 8);
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("composite 30 layers: {per:.2} ms per 256² tile (single thread)");
    }
}
