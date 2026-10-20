//! Text layers: the source text and its style stay with the layer and are drawn again whenever
//! they (or the layer's 2D transform) change, so the text stays editable.

use std::sync::Arc;

use lawha_core::resample::Mat3;
use lawha_core::text::{self, TextStyle};
use serde::{Deserialize, Serialize};

use super::model::{Node, NodeKind, Slot, TileMap};
use super::ops::{active_slot, OpResult};
use super::raster;
use super::{Document, Label};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextData {
    pub text: String,
    pub style: TextStyle,
    /// Box width for paragraph text; `None` for point text (lines break only at newlines).
    pub width: Option<f32>,
    /// Box height, for the frame shown while editing.
    pub height: Option<f32>,
    /// Layout px → document px, row-major 2×3 (x' = m0·x + m1·y + m2, y' = m3·x + m4·y + m5).
    pub matrix: [f64; 6],
}

impl TextData {
    pub fn mat3(&self) -> Mat3 {
        let m = self.matrix;
        [m[0], m[1], m[2], m[3], m[4], m[5], 0.0, 0.0, 1.0]
    }

    /// The same text after an affine change of the document (`None` if `m` isn't affine).
    pub fn transformed(&self, m: &Mat3) -> Option<TextData> {
        if m[6].abs() > 1e-12 || m[7].abs() > 1e-12 || (m[8] - 1.0).abs() > 1e-12 {
            return None;
        }
        let n = lawha_core::resample::multiply(m, &self.mat3());
        Some(TextData { matrix: [n[0], n[1], n[2], n[3], n[4], n[5]], ..self.clone() })
    }
}

/// Draws the text into tiles (only pixels within a generous margin around the canvas).
pub fn render(data: &TextData, canvas_w: u32, canvas_h: u32) -> TileMap {
    let l = text::layout(&data.text, &data.style, data.width);
    let m = data.matrix;
    // tiny-skia's order: a, b, c, d, e, f with x' = a·x + c·y + e, y' = b·x + d·y + f
    let ts = [m[0] as f32, m[3] as f32, m[1] as f32, m[4] as f32, m[2] as f32, m[5] as f32];
    let (w, h) = (canvas_w as i64, canvas_h as i64);
    match text::render(&l, data.style.color, ts, (-w, -h, 2 * w, 2 * h)) {
        Some(r) => raster::write(&r.rgba, r.x, r.y, r.w as usize, r.h as usize),
        None => TileMap::new(),
    }
}

/// A short layer name from the text (the way other editors name text layers).
pub fn name_for(text: &str) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = t.chars().take(32).collect();
    if t.chars().count() > 32 {
        out.push('…');
    }
    out
}

pub fn new_text(doc: &mut Document, data: TextData) -> OpResult {
    let tiles = render(&data, doc.content.width, doc.content.height);
    let slot = active_slot(doc);
    let mut node = Node::pixel(name_for(&data.text), TileMap::new());
    node.kind = NodeKind::Text { data, tiles: Arc::new(tiles) };
    let id = node.id;
    let name = node.name.clone();
    doc.content.insert(node, Slot { parent: slot.parent, index: slot.index + 1 });
    doc.content.active = id;
    doc.commit(Label::with("history.newText", name));
    Ok(())
}

/// New text, style, box or placement for a text layer. The layer keeps its name unless it still
/// carried the old text's name.
pub fn set_text(doc: &mut Document, id: u64, data: TextData, transient: bool) -> OpResult {
    let (w, h) = (doc.content.width, doc.content.height);
    let node = doc.content.find_mut(id).ok_or("layer not found")?;
    let NodeKind::Text { data: old, .. } = &node.kind else { return Err("not a text layer".into()) };
    if node.name == name_for(&old.text) {
        node.name = name_for(&data.text);
    }
    let tiles = render(&data, w, h);
    node.kind = NodeKind::Text { data, tiles: Arc::new(tiles) };
    let name = node.name.clone();
    if transient {
        doc.touch();
    } else {
        doc.commit(Label::with("history.editText", name));
    }
    Ok(())
}

/// Turns a text layer into plain pixels (for painting, filters and free distortions).
pub fn rasterize(doc: &mut Document, id: u64) -> OpResult {
    let node = doc.content.find_mut(id).ok_or("layer not found")?;
    let NodeKind::Text { tiles, .. } = &node.kind else { return Err("not a text layer".into()) };
    node.kind = NodeKind::Pixel { tiles: tiles.clone() };
    let name = node.name.clone();
    doc.commit(Label::with("history.rasterize", name));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, Content};

    fn doc() -> Document {
        let bg = Node::pixel("bg", filled_tiles(800, 400, [255, 255, 255, 255]));
        let id = bg.id;
        Document::new("t".into(), None, Content::new(800, 400, 72.0, id, vec![bg]), Label::new("history.new"))
    }

    fn data(text: &str) -> TextData {
        TextData {
            text: text.into(),
            style: TextStyle { size: 80.0, color: [20, 60, 200, 255], ..TextStyle::default() },
            width: None,
            height: None,
            matrix: [1.0, 0.0, 600.0, 0.0, 1.0, 200.0],
        }
    }

    fn ink(d: &Document) -> usize {
        let mut n = 0;
        for tx in 0..4 {
            for ty in 0..2 {
                if let Some(t) = composite_lod0(&d.content, tx, ty) {
                    n += t.0.chunks_exact(4).filter(|p| p[2] > 150 && p[0] < 100).count();
                }
            }
        }
        n
    }

    #[test]
    fn text_layers_render_stay_editable_and_undo() {
        let mut d = doc();
        new_text(&mut d, data("مرحبا بالعالم")).unwrap();
        let id = d.content.active;
        assert_eq!(d.content.find(id).unwrap().name, "مرحبا بالعالم");
        let a = ink(&d);
        assert!(a > 2000, "{a}");
        // right-to-left point text ends at its anchor (x = 600)
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        let (x0, _, x1, _) = raster::content_bounds(&t).unwrap();
        assert!(x1 <= 603 && x0 < 400, "{x0}..{x1}");
        let mut more = data("مرحبا بالعالم كله");
        more.style.size = 60.0;
        set_text(&mut d, id, more, false).unwrap();
        assert_eq!(d.content.find(id).unwrap().name, "مرحبا بالعالم كله", "the name follows the text");
        d.undo();
        assert_eq!(ink(&d), a);
    }

    #[test]
    fn moving_and_turning_keep_text_editable() {
        let mut d = doc();
        new_text(&mut d, data("لوحة")).unwrap();
        let id = d.content.active;
        let before = raster::content_bounds(d.content.find(id).unwrap().tiles().unwrap()).unwrap();
        crate::doc::transform::transform_layers(&mut d, &[id], &[1.0, 0.0, -100.0, 0.0, 1.0, 50.0, 0.0, 0.0, 1.0], lawha_core::resample::Method::Bicubic, "history.movePixels").unwrap();
        let n = d.content.find(id).unwrap();
        let NodeKind::Text { data: moved, tiles } = &n.kind else { panic!("still text") };
        assert_eq!(moved.matrix[2], 500.0);
        let after = raster::content_bounds(tiles).unwrap();
        assert_eq!((after.0, after.1), (before.0 - 100, before.1 + 50));
        // a perspective distortion can't stay text: it becomes pixels
        crate::doc::transform::transform_layers(&mut d, &[id], &[1.0, 0.1, 0.0, 0.0, 1.0, 0.0, 0.0005, 0.0, 1.0], lawha_core::resample::Method::Bicubic, "history.transform").unwrap();
        assert!(matches!(d.content.find(id).unwrap().kind, NodeKind::Pixel { .. }));
    }

    #[test]
    fn rasterize_keeps_the_pixels() {
        let mut d = doc();
        new_text(&mut d, data("نص")).unwrap();
        let id = d.content.active;
        let a = ink(&d);
        rasterize(&mut d, id).unwrap();
        assert!(matches!(d.content.find(id).unwrap().kind, NodeKind::Pixel { .. }));
        assert_eq!(ink(&d), a);
    }
}
