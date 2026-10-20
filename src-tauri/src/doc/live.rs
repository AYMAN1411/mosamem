//! Live sessions: while the user drags a layer's opacity or tries blend modes (and later paints or
//! moves it), the GPU recomposites three cached parts instead of waiting for the CPU:
//!
//! - `below`: everything painted before the layer (the backdrop it blends onto),
//! - `layer`: the layer's own pixels,
//! - `above`: everything painted after it, flattened onto transparency.
//!
//! Flattening `above` separately is exact only when every part of it composites with plain
//! source-over (associative). `begin` checks that, and the UI falls back to CPU recompositing
//! otherwise.

use std::collections::HashMap;
use std::sync::Arc;

use lawha_core::tile::{downsample, Tile, TILE};
use lawha_core::BlendMode;
use parking_lot::Mutex;

use super::composite::{composite_nodes_lod, mask_factor, MaskFactor};
use super::model::{Content, Node, NodeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Below,
    Layer,
    Above,
}

pub struct Session {
    pub doc: u64,
    pub layer: u64,
    /// Identity of everything except the layer: a new session is needed only when this changes.
    pub fingerprint: u64,
    width: u32,
    height: u32,
    below: Vec<Node>,
    above: Vec<Node>,
    layer_tiles: parking_lot::RwLock<Arc<super::model::TileMap>>,
    /// The layer itself when it has an enabled mask (applied to the layer part).
    mask: parking_lot::RwLock<Option<Node>>,
    cache: Mutex<HashMap<(Part, u8, i32, i32), Option<Arc<Tile>>>>,
}

/// Can this node be flattened separately and laid over with plain source-over?
fn associative(n: &Node) -> bool {
    if !n.visible {
        return true;
    }
    match &n.kind {
        NodeKind::Group { children, pass_through: true, .. } => n.opacity >= 1.0 && children.iter().all(associative),
        // pixel layers and isolated groups: only their final blend matters
        _ => n.blend == BlendMode::Normal,
    }
}

/// A hash of everything that shapes the below/above parts and how the layer is drawn: node
/// structure, properties and pixel identities (tile maps are never mutated in place).
fn fingerprint(below: &[Node], above: &[Node], layer: &Node) -> u64 {
    use std::hash::{Hash, Hasher};
    fn walk(nodes: &[Node], h: &mut std::collections::hash_map::DefaultHasher) {
        for n in nodes {
            n.id.hash(h);
            (n.visible, n.clipped, n.blend.index()).hash(h);
            (n.opacity.to_bits(), n.fill.to_bits()).hash(h);
            if let Some(m) = &n.mask {
                (Arc::as_ptr(&m.mask.tiles) as usize, m.mask.outside, m.enabled, m.density.to_bits(), m.feather.to_bits()).hash(h);
            }
            match &n.kind {
                NodeKind::Pixel { tiles } => (Arc::as_ptr(tiles) as usize).hash(h),
                NodeKind::Group { children, pass_through, .. } => {
                    pass_through.hash(h);
                    walk(children, h);
                }
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    walk(below, &mut h);
    0xabcdu16.hash(&mut h);
    walk(above, &mut h);
    // the layer's mask settings matter to the layer part; its pixels are refreshed separately
    layer.mask.as_ref().map(|m| (m.enabled, m.density.to_bits(), m.feather.to_bits())).hash(&mut h);
    h.finish()
}

/// Splits the tree around `layer`. Returns None when a live session can't be exact.
fn split(content: &Content, layer: u64) -> Option<(Vec<Node>, Vec<Node>)> {
    // path of indices from the root to the layer
    fn path(nodes: &[Node], id: u64, out: &mut Vec<usize>) -> bool {
        for (i, n) in nodes.iter().enumerate() {
            if n.id == id {
                out.push(i);
                return true;
            }
            if let Some(c) = n.children() {
                out.push(i);
                if path(c, id, out) {
                    return true;
                }
                out.pop();
            }
        }
        false
    }
    let mut p = Vec::new();
    if !path(&content.root, layer, &mut p) {
        return None;
    }
    let mut below_levels: Vec<Vec<Node>> = Vec::new();
    let mut above: Vec<Node> = Vec::new();
    let mut nodes = &content.root;
    for (depth, &i) in p.iter().enumerate() {
        let last = depth + 1 == p.len();
        let n = &nodes[i];
        if !last {
            // ancestors must be visible pass-through groups at full opacity
            match &n.kind {
                NodeKind::Group { pass_through: true, .. } if n.visible && n.opacity >= 1.0 && !n.clipped => {}
                _ => return None,
            }
        } else {
            // the layer itself: a pixel layer, not part of a clipping unit
            if n.is_group() || n.clipped || nodes.get(i + 1).is_some_and(|x| x.clipped) {
                return None;
            }
        }
        below_levels.push(nodes[..i].to_vec());
        let later = &nodes[i + 1..];
        if !later.iter().all(associative) {
            return None;
        }
        // later siblings at deeper levels paint before later siblings at shallower levels
        above.splice(0..0, later.iter().cloned());
        if !last {
            nodes = n.children().unwrap();
        }
    }
    // below: rebuild the nesting so pass-through ancestors still contain their prefix
    let mut below: Vec<Node> = Vec::new();
    for (depth, mut level) in below_levels.into_iter().enumerate().rev() {
        if depth + 1 < p.len() {
            // `below` so far is the content of the ancestor at this depth
            let mut anc = {
                let mut ns = &content.root;
                for &k in &p[..depth] {
                    ns = ns[k].children().unwrap();
                }
                ns[p[depth]].clone()
            };
            if let Some(c) = anc.children_mut() {
                *c = std::mem::take(&mut below);
            }
            level.push(anc);
        }
        below = level;
    }
    Some((below, above))
}

impl Session {
    pub fn begin(doc: u64, content: &Content, layer: u64) -> Option<Session> {
        let node = content.find(layer)?;
        let layer_tiles = node.tiles()?.clone();
        let mask = node.mask.as_ref().filter(|m| m.enabled).map(|_| node.clone());
        let (below, above) = split(content, layer)?;
        let fingerprint = fingerprint(&below, &above, node);
        Some(Session { doc, layer, fingerprint, width: content.width, height: content.height, below, above, layer_tiles: parking_lot::RwLock::new(layer_tiles), mask: parking_lot::RwLock::new(mask), cache: Mutex::new(HashMap::new()) })
    }

    fn lod0(&self, part: Part, tx: i32, ty: i32) -> Option<Tile> {
        match part {
            Part::Below => composite_nodes_lod(&self.below, 0, tx, ty),
            Part::Above => composite_nodes_lod(&self.above, 0, tx, ty),
            Part::Layer => {
                let tiles = self.layer_tiles.read().clone();
                let t = tiles.get(&(tx, ty))?;
                // the layer mask scales alpha (straight colors stay as they are)
                match self.mask.read().as_ref().and_then(|n| mask_factor(n, 0, (tx, ty))) {
                    None => Some((**t).clone()),
                    Some(f) => {
                        let mut out = (**t).clone();
                        for (i, px) in out.0.chunks_exact_mut(4).enumerate() {
                            let k = match &f {
                                MaskFactor::Const(k) => *k,
                                MaskFactor::Each(v) => v[i],
                            };
                            px[3] = (px[3] as f32 * k + 0.5) as u8;
                        }
                        Some(out)
                    }
                }
            }
        }
    }

    /// The same session can serve a new stroke when nothing but the layer changed since it started.
    pub fn matches(content: &Content, layer: u64) -> Option<u64> {
        let node = content.find(layer)?;
        node.tiles()?;
        let (below, above) = split(content, layer)?;
        Some(fingerprint(&below, &above, node))
    }

    /// Forgets every cached tile of the layer part (the layer may have changed in any way).
    pub fn reset_layer(&self, node: &Node) {
        if let Some(t) = node.tiles() {
            *self.layer_tiles.write() = t.clone();
        }
        *self.mask.write() = node.mask.as_ref().filter(|m| m.enabled).map(|_| node.clone());
        self.cache.lock().retain(|k, _| k.0 != Part::Layer);
    }

    /// The layer changed during a stroke: swap in its new pixels (and mask) and forget the cached
    /// tiles of the layer part at those keys and every coarser level above them.
    pub fn refresh_layer(&self, node: &Node, keys: &[(i32, i32)]) {
        if let Some(t) = node.tiles() {
            *self.layer_tiles.write() = t.clone();
        }
        if node.mask.as_ref().is_some_and(|m| m.enabled) {
            *self.mask.write() = Some(node.clone());
        }
        let mut c = self.cache.lock();
        for &(tx, ty) in keys {
            for lod in 0..=super::composite::MAX_LOD {
                c.remove(&(Part::Layer, lod, tx >> lod, ty >> lod));
            }
        }
    }

    pub fn tile(&self, part: Part, lod: u8, tx: i32, ty: i32) -> Option<Arc<Tile>> {
        let (tw, th) = (self.width.div_ceil(TILE as u32) as i32, self.height.div_ceil(TILE as u32) as i32);
        let scale = 1i32 << lod;
        if tx < 0 || ty < 0 || tx * scale >= tw || ty * scale >= th {
            return None;
        }
        let key = (part, lod, tx, ty);
        if let Some(hit) = self.cache.lock().get(&key) {
            return hit.clone();
        }
        let result = if lod == 0 {
            if part == Part::Layer && self.mask.read().is_none() {
                self.layer_tiles.read().get(&(tx, ty)).cloned()
            } else {
                self.lod0(part, tx, ty).map(Arc::new)
            }
        } else if part == Part::Below {
            composite_nodes_lod(&self.below, lod, tx, ty).map(Arc::new)
        } else if part == Part::Above {
            composite_nodes_lod(&self.above, lod, tx, ty).map(Arc::new)
        } else {
            use rayon::prelude::*;
            let kids: Vec<Option<Arc<Tile>>> = [(0, 0), (1, 0), (0, 1), (1, 1)].par_iter().map(|(dx, dy)| self.tile(part, lod - 1, tx * 2 + dx, ty * 2 + dy)).collect();
            if kids.iter().all(Option::is_none) {
                None
            } else {
                let mut out = Tile::transparent();
                downsample([kids[0].as_deref(), kids[1].as_deref(), kids[2].as_deref(), kids[3].as_deref()], &mut out);
                Some(Arc::new(out))
            }
        };
        self.cache.lock().insert(key, result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::filled_tiles;
    use lawha_core::tile::Accum;

    fn solid(name: &str, rgba: [u8; 4]) -> Node {
        Node::pixel(name, filled_tiles(256, 256, rgba))
    }

    /// below + layer (with its blend/opacity/fill) + above, composited the way the GPU does it,
    /// must equal the CPU composite of the whole tree.
    fn check(content: &Content, layer: u64) {
        let s = Session::begin(1, content, layer).expect("live should be supported");
        let l = content.find(layer).unwrap();
        let mut acc = match s.tile(Part::Below, 0, 0, 0) {
            Some(t) => Accum::from_tile(&t, 1.0),
            None => Accum::new(),
        };
        if l.visible {
            if let Some(t) = s.tile(Part::Layer, 0, 0, 0) {
                acc.composite(&t, l.blend, l.opacity * l.fill, (0, 0));
            }
        }
        if let Some(t) = s.tile(Part::Above, 0, 0, 0) {
            acc.composite(&t, BlendMode::Normal, 1.0, (0, 0));
        }
        let mut live = Tile::transparent();
        acc.to_tile(&mut live);
        let want = composite_lod0(content, 0, 0).unwrap();
        for (a, b) in live.0.iter().zip(want.0.iter()) {
            assert!((*a as i32 - *b as i32).abs() <= 1, "live {a} vs composite {b}");
        }
    }

    #[test]
    fn live_parts_reproduce_the_composite() {
        let bg = solid("bg", [40, 90, 200, 255]);
        let mut mid = solid("mid", [230, 120, 20, 180]);
        mid.blend = BlendMode::Overlay;
        mid.opacity = 0.7;
        mid.fill = 0.8;
        let mut top = solid("top", [255, 255, 255, 90]);
        top.opacity = 0.5;
        let g = Node::group("g", vec![top]);
        let id = mid.id;
        let c = Content::new(256, 256, 72.0, id, vec![bg, mid, g]);
        check(&c, id);
    }

    #[test]
    fn layer_inside_a_pass_through_group() {
        let bg = solid("bg", [200, 200, 200, 255]);
        let low = solid("low", [10, 200, 10, 140]);
        let mut target = solid("t", [200, 30, 30, 200]);
        target.blend = BlendMode::Multiply;
        let after = solid("after", [0, 0, 255, 60]);
        let id = target.id;
        let g = Node::group("g", vec![low, target, after]);
        let top = solid("top", [255, 255, 0, 40]);
        let c = Content::new(256, 256, 72.0, id, vec![bg, g, top]);
        check(&c, id);
    }

    #[test]
    fn non_associative_layers_above_disable_live_mode() {
        let bg = solid("bg", [0, 0, 0, 255]);
        let target = solid("t", [255, 0, 0, 255]);
        let mut scr = solid("s", [0, 0, 255, 255]);
        scr.blend = BlendMode::Screen;
        let id = target.id;
        let c = Content::new(256, 256, 72.0, id, vec![bg, target, scr]);
        assert!(Session::begin(1, &c, id).is_none());
    }
}
