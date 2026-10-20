//! Adjustment layers: creating them, changing their settings, and the histogram their panels show.

use lawha_core::adjust::Adjustment;
use lawha_core::blend::lum;
use lawha_core::tile::TILE;
use serde::Serialize;

use super::composite::composite_nodes_lod;
use super::model::{LayerMask, Node, Slot};
use super::ops::{active_slot, OpResult};
use super::{Document, Label};

/// A new adjustment layer above the active layer. The current selection becomes its mask.
pub fn new_adjustment(doc: &mut Document, name: String, adj: Adjustment) -> OpResult {
    let slot = active_slot(doc);
    let mut node = Node::adjustment(name.clone(), adj);
    if let Some(sel) = &doc.content.selection {
        node.mask = Some(LayerMask::new(sel.clone()));
    }
    let id = node.id;
    doc.content.insert(node, Slot { parent: slot.parent, index: slot.index + 1 });
    doc.content.active = id;
    doc.commit(Label::with("history.newAdjustment", name));
    Ok(())
}

/// Changes an adjustment layer's settings. `transient` updates (while a slider is dragged) don't
/// add history steps; the final one does.
pub fn set_adjustment(doc: &mut Document, id: u64, adj: Adjustment, transient: bool) -> OpResult {
    let node = doc.content.find_mut(id).ok_or("layer not found")?;
    if !node.is_adjustment() {
        return Err("not an adjustment layer".into());
    }
    node.set_adjustment(adj);
    let name = node.name.clone();
    if transient {
        doc.touch();
    } else {
        doc.commit(Label::with("history.adjustment", name));
    }
    Ok(())
}

/// Counts per value (0–255), weighted by alpha (255 = one opaque pixel).
#[derive(Serialize, Debug)]
pub struct Histogram {
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    pub luminosity: Vec<u32>,
}

/// Pixels sampled for a histogram, at most (a mip level of the image).
const HISTOGRAM_PIXELS: u64 = 1 << 20;

/// Histogram of what an adjustment layer sees: the image without that layer and everything above
/// it. With `below: None`, the whole image.
pub fn histogram(doc: &Document, below: Option<u64>) -> Histogram {
    let mut root = doc.content.root.clone();
    if let Some(id) = below {
        // hide the layer and everything painted after it; its ancestors stay, holding what is below
        fn hide_from(nodes: &mut [Node], id: u64, found: &mut bool) {
            for n in nodes {
                if *found {
                    n.visible = false;
                } else if n.id == id {
                    n.visible = false;
                    *found = true;
                } else if let Some(c) = n.children_mut() {
                    hide_from(c, id, found);
                }
            }
        }
        hide_from(&mut root, id, &mut false);
    }
    let (w, h) = (doc.content.width as u64, doc.content.height as u64);
    let mut lod = 0u8;
    while (w >> lod) * (h >> lod) > HISTOGRAM_PIXELS && lod < super::composite::MAX_LOD {
        lod += 1;
    }
    let span = (TILE as u64) << lod;
    let keys: Vec<(i32, i32)> = (0..h.div_ceil(span) as i32).flat_map(|ty| (0..w.div_ceil(span) as i32).map(move |tx| (tx, ty))).collect();
    use rayon::prelude::*;
    let parts: Vec<[[u32; 256]; 4]> = keys
        .par_iter()
        .filter_map(|&(tx, ty)| composite_nodes_lod(&root, lod, tx, ty))
        .map(|t| {
            let mut c = [[0u32; 256]; 4];
            for p in t.0.chunks_exact(4) {
                let a = p[3] as u32;
                if a == 0 {
                    continue;
                }
                c[0][p[0] as usize] += a;
                c[1][p[1] as usize] += a;
                c[2][p[2] as usize] += a;
                let l = lum([p[0] as f32, p[1] as f32, p[2] as f32]);
                c[3][(l + 0.5).min(255.0) as usize] += a;
            }
            c
        })
        .collect();
    let mut sum = [[0u32; 256]; 4];
    for c in parts {
        for k in 0..4 {
            for i in 0..256 {
                sum[k][i] = sum[k][i].saturating_add(c[k][i]);
            }
        }
    }
    let [red, green, blue, luminosity] = sum.map(|v| v.to_vec());
    Histogram { red, green, blue, luminosity }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, Content};
    use lawha_core::mask::Mask;
    use lawha_core::BlendMode;

    fn solid(name: &str, rgba: [u8; 4]) -> Node {
        Node::pixel(name, filled_tiles(300, 300, rgba))
    }

    fn doc(root: Vec<Node>) -> Document {
        let active = root.last().unwrap().id;
        Document::new("t".into(), None, Content::new(300, 300, 72.0, active, root), Label::new("history.new"))
    }

    fn px(d: &Document, x: usize, y: usize) -> [u8; 4] {
        composite_lod0(&d.content, (x / 256) as i32, (y / 256) as i32).unwrap().px(x % 256, y % 256)
    }

    #[test]
    fn an_adjustment_layer_changes_everything_below() {
        let mut d = doc(vec![solid("a", [200, 100, 50, 255])]);
        new_adjustment(&mut d, "invert".into(), Adjustment::Invert).unwrap();
        assert_eq!(px(&d, 10, 10), [55, 155, 205, 255]);
        // and only where there is something: transparency stays transparent
        let mut e = doc(vec![Node::pixel("empty", Default::default())]);
        new_adjustment(&mut e, "invert".into(), Adjustment::Invert).unwrap();
        assert!(composite_lod0(&e.content, 0, 0).is_none());
        // hidden, a quarter opacity, undo
        let id = d.content.active;
        d.content.find_mut(id).unwrap().opacity = 0.25;
        assert_eq!(px(&d, 10, 10), [164, 114, 89, 255]);
        d.content.find_mut(id).unwrap().visible = false;
        assert_eq!(px(&d, 10, 10), [200, 100, 50, 255]);
        d.content.find_mut(id).unwrap().visible = true;
        set_adjustment(&mut d, id, Adjustment::Threshold { level: 128.0 }, false).unwrap();
        // luminosity 124.5 is below the level: black, mixed in at a quarter
        assert_eq!(px(&d, 10, 10), [150, 75, 38, 255]);
        d.undo();
        assert_eq!(d.content.find(id).unwrap().adjustment_settings(), Some(&Adjustment::Invert));
    }

    #[test]
    fn the_selection_becomes_the_mask() {
        let mut d = doc(vec![solid("a", [200, 100, 50, 255])]);
        let mut sel = lawha_core::mask::Dense::new(300, 300, 0);
        for y in 0..300 {
            for x in 0..150 {
                sel.data[y * 300 + x] = 255;
            }
        }
        d.content.selection = Some(Mask::from_dense(&sel, 0));
        new_adjustment(&mut d, "invert".into(), Adjustment::Invert).unwrap();
        assert_eq!(px(&d, 10, 10), [55, 155, 205, 255]);
        assert_eq!(px(&d, 200, 10), [200, 100, 50, 255]);
    }

    #[test]
    fn clipped_and_grouped_adjustments_stay_in_their_scope() {
        // background red, then a half-covering blue layer with an inverting adjustment clipped to it
        let bg = solid("bg", [255, 0, 0, 255]);
        let mut half = TileMapHalf::new();
        half.fill_left([0, 0, 255, 255]);
        let blue = Node::pixel("blue", half.0);
        let mut inv = Node::adjustment("inv", Adjustment::Invert);
        inv.clipped = true;
        let d = doc(vec![bg.clone(), blue.clone(), inv.clone()]);
        assert_eq!(px(&d, 10, 10), [255, 255, 0, 255]); // inverted blue
        assert_eq!(px(&d, 200, 10), [255, 0, 0, 255]); // red untouched
        // inside an isolated group the adjustment sees only the group's content
        let mut inv2 = inv.clone();
        inv2.clipped = false;
        let mut g = Node::group("g", vec![blue, inv2]);
        if let crate::doc::NodeKind::Group { pass_through, .. } = &mut g.kind {
            *pass_through = false;
        }
        let d = doc(vec![bg.clone(), g.clone()]);
        assert_eq!(px(&d, 10, 10), [255, 255, 0, 255]);
        assert_eq!(px(&d, 200, 10), [255, 0, 0, 255]);
        // a pass-through group lets it reach the background too
        if let crate::doc::NodeKind::Group { pass_through, .. } = &mut g.kind {
            *pass_through = true;
        }
        let d = doc(vec![bg, g]);
        assert_eq!(px(&d, 200, 10), [0, 255, 255, 255]);
    }

    #[test]
    fn blend_modes_apply_to_the_adjusted_color() {
        let mut d = doc(vec![solid("a", [128, 128, 128, 255])]);
        new_adjustment(&mut d, "inv".into(), Adjustment::Invert).unwrap();
        let id = d.content.active;
        d.content.find_mut(id).unwrap().blend = BlendMode::Multiply;
        // 0.502 × 0.498 ≈ 0.25
        assert_eq!(px(&d, 5, 5), [64, 64, 64, 255]);
    }

    #[test]
    fn live_sessions_refuse_a_layer_under_an_adjustment() {
        let mut d = doc(vec![solid("bg", [10, 20, 30, 255]), Node::pixel("paint", Default::default())]);
        let paint = d.content.active;
        assert!(crate::doc::live::Session::begin(1, &d.content, paint).is_some());
        new_adjustment(&mut d, "inv".into(), Adjustment::Invert).unwrap();
        assert!(crate::doc::live::Session::begin(1, &d.content, paint).is_none());
        // below an adjustment is fine
        let inv = d.content.active;
        d.content.active = paint;
        crate::doc::ops::move_layers(&mut d, &[inv], None, 0).unwrap();
        assert!(crate::doc::live::Session::begin(1, &d.content, paint).is_some());
    }

    #[test]
    fn live_parts_reproduce_an_adjustment_layer() {
        use crate::doc::live::{Part, Session};
        use lawha_core::tile::Accum;
        let mut d = doc(vec![solid("bg", [200, 100, 50, 255])]);
        let mut sel = lawha_core::mask::Dense::new(300, 300, 0);
        for y in 0..300 {
            for x in 0..300 {
                sel.data[y * 300 + x] = (x * 255 / 299) as u8;
            }
        }
        d.content.selection = Some(Mask::from_dense(&sel, 0));
        let adj = Adjustment::HueSaturation { master: lawha_core::adjust::Hsl { hue: 90.0, saturation: 30.0, lightness: -10.0 }, ranges: Default::default(), colorize: false, colorize_hsl: Default::default() };
        new_adjustment(&mut d, "hs".into(), adj.clone()).unwrap();
        let id = d.content.active;
        d.content.selection = None;
        d.content.find_mut(id).unwrap().opacity = 0.8;
        crate::doc::ops::new_layer(&mut d, "top".into()).unwrap();
        let top = d.content.active;
        let mut t = lawha_core::Tile::transparent();
        for p in t.0.chunks_exact_mut(4).step_by(3) {
            p.copy_from_slice(&[10, 200, 30, 128]);
        }
        if let crate::doc::NodeKind::Pixel { tiles } = &mut d.content.find_mut(top).unwrap().kind {
            std::sync::Arc::make_mut(tiles).insert((0, 0), std::sync::Arc::new(t));
        }
        let s = Session::begin(1, &d.content, id).expect("an adjustment at the top level can be previewed live");
        let (below, cover, above) = (s.tile(Part::Below, 0, 0, 0).unwrap(), s.tile(Part::Layer, 0, 0, 0).unwrap(), s.tile(Part::Above, 0, 0, 0).unwrap());
        let mask: Vec<f32> = cover.0.chunks_exact(4).map(|p| p[3] as f32 / 255.0).collect();
        let mut acc = Accum::from_tile(&below, 1.0);
        lawha_core::adjust::apply_to_accum(&mut acc, &lawha_core::adjust::compile(&adj), BlendMode::Normal, 0.8, Some(&mask), (0, 0));
        acc.composite(&above, BlendMode::Normal, 1.0, (0, 0));
        let mut out = lawha_core::Tile::transparent();
        acc.to_tile(&mut out);
        let want = composite_lod0(&d.content, 0, 0).unwrap();
        let worst = out.0.iter().zip(want.0.iter()).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 1, "worst difference {worst}");
    }

    #[test]
    fn histogram_sees_only_what_is_below() {
        let mut d = doc(vec![solid("a", [200, 100, 50, 255])]);
        new_adjustment(&mut d, "inv".into(), Adjustment::Invert).unwrap();
        let id = d.content.active;
        let below = histogram(&d, Some(id));
        let all = histogram(&d, None);
        assert_eq!(below.red[200], 300 * 300 * 255);
        assert_eq!(all.red[55], 300 * 300 * 255);
        assert_eq!(below.luminosity[(0.3f32 * 200.0 + 0.59 * 100.0 + 0.11 * 50.0).round() as usize], 300 * 300 * 255);
    }

    /// A 300×300 tile map with only the left half filled.
    struct TileMapHalf(crate::doc::TileMap);
    impl TileMapHalf {
        fn new() -> Self {
            TileMapHalf(Default::default())
        }
        fn fill_left(&mut self, rgba: [u8; 4]) {
            let mut t = lawha_core::Tile::transparent();
            for y in 0..256 {
                for x in 0..150 {
                    t.0[(y * 256 + x) * 4..][..4].copy_from_slice(&rgba);
                }
            }
            self.0.insert((0, 0), std::sync::Arc::new(t.clone()));
            self.0.insert((0, 1), std::sync::Arc::new(t));
        }
    }
}
