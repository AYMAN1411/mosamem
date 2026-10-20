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
use std::sync::Arc;

use lawha_core::tile::{downsample, Accum, Tile, TILE};
use parking_lot::Mutex;

use super::model::{Content, Node, NodeKind};
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
}

/// What one node contributes to a tile, before its own blend mode and opacity are applied.
enum Source<'a> {
    Tile(&'a Tile, f32),
    Accum(Accum),
}

/// Isolated content of a node at one tile (None when it draws nothing there).
fn isolated(node: &Node, key: (i32, i32), origin: (i64, i64)) -> Option<Source<'_>> {
    match &node.kind {
        NodeKind::Pixel { tiles } => tiles.get(&key).map(|t| Source::Tile(t, node.fill)),
        NodeKind::Group { children, .. } => {
            let mut acc = Accum::new();
            composite_stack(children, &mut acc, key, origin);
            (!acc.is_empty()).then_some(Source::Accum(acc))
        }
    }
}

/// Composites a list of sibling nodes (bottom to top) onto `acc`.
pub fn composite_stack(nodes: &[Node], acc: &mut Accum, key: (i32, i32), origin: (i64, i64)) {
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
            composite_unit(base, clipped, acc, key, origin);
        }
        i = j;
    }
}

fn composite_unit(base: &Node, clipped: &[Node], acc: &mut Accum, key: (i32, i32), origin: (i64, i64)) {
    let has_clips = clipped.iter().any(|c| c.visible && c.opacity > 0.0);
    // pass-through group without clipped layers: children go straight onto the backdrop
    if let NodeKind::Group { children, pass_through: true, .. } = &base.kind {
        if !has_clips {
            if base.opacity >= 1.0 {
                composite_stack(children, acc, key, origin);
            } else {
                let mut with = Accum(acc.0.clone());
                composite_stack(children, &mut with, key, origin);
                acc.lerp_to(&with, base.opacity);
            }
            return;
        }
    }
    let Some(src) = isolated(base, key, origin) else { return };
    if !has_clips {
        match src {
            Source::Tile(t, fill) => acc.composite(t, base.blend, base.opacity * fill, origin),
            Source::Accum(a) => acc.composite_accum(&a, base.blend, base.opacity, origin),
        }
        return;
    }
    // clipping unit: base content, then each clipped layer source-atop, then blend the unit
    let mut unit = match src {
        Source::Tile(t, fill) => Accum::from_tile(t, fill),
        Source::Accum(a) => a,
    };
    for c in clipped.iter().filter(|c| c.visible && c.opacity > 0.0) {
        match isolated(c, key, origin) {
            Some(Source::Tile(t, fill)) => unit.composite_atop(t, c.blend, c.opacity * fill),
            Some(Source::Accum(a)) => unit.composite_accum_atop(&a, c.blend, c.opacity),
            None => {}
        }
    }
    acc.composite_accum(&unit, base.blend, base.opacity, origin);
}

/// Composite of a list of nodes at lod 0 for one tile, onto transparency; `None` when empty.
pub fn composite_nodes(nodes: &[Node], tx: i32, ty: i32) -> Option<Tile> {
    let mut acc = Accum::new();
    composite_stack(nodes, &mut acc, (tx, ty), (tx as i64 * TILE as i64, ty as i64 * TILE as i64));
    if acc.is_empty() {
        return None;
    }
    let mut out = Tile::transparent();
    acc.to_tile(&mut out);
    Some(out)
}

/// Composite of the whole tree at lod 0 for one tile; `None` when fully transparent.
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
    let result = if lod == 0 {
        composite_lod0(c, tx, ty).map(Arc::new)
    } else {
        use rayon::prelude::*;
        let kids: Vec<Option<Arc<Tile>>> =
            [(0, 0), (1, 0), (0, 1), (1, 1)].par_iter().map(|(dx, dy)| tile_in(h, c, version, lod - 1, tx * 2 + dx, ty * 2 + dy)).collect();
        if kids.iter().all(Option::is_none) {
            None
        } else {
            let mut out = Tile::transparent();
            downsample([kids[0].as_deref(), kids[1].as_deref(), kids[2].as_deref(), kids[3].as_deref()], &mut out);
            Some(Arc::new(out))
        }
    };
    h.composite.put(version, key, result.clone());
    result
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
    use lawha_core::BlendMode;

    pub fn doc(root: Vec<Node>, w: u32, h: u32) -> Document {
        let c = Content { width: w, height: h, ppi: 72.0, active: 0, root };
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
