//! Shape layers (M12): a vector shape with its fill and stroke stays with the layer and is drawn
//! again whenever it changes (or the layer is moved, scaled or turned), so it stays editable.

use std::sync::Arc;

use mosamem_core::vector::{self, Shape, ShapePaint};
use serde::{Deserialize, Serialize};

use super::model::{Node, NodeKind, Slot, TileMap};
use super::ops::{active_slot, OpResult};
use super::raster;
use super::{Document, Label};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeData {
    pub shape: Shape,
    pub paint: ShapePaint,
}

impl ShapeData {
    /// The same shape after an affine change of the document (`m` row-major 3×3; None if it isn't
    /// affine). Rectangles and ellipses stay parametric under moves and scales.
    pub fn transformed(&self, m: &mosamem_core::resample::Mat3) -> Option<ShapeData> {
        if m[6].abs() > 1e-12 || m[7].abs() > 1e-12 || (m[8] - 1.0).abs() > 1e-12 {
            return None;
        }
        let affine = [m[0], m[3], m[1], m[4], m[2], m[5]];
        let mut paint = self.paint.clone();
        // strokes scale with the shape (by the mean of the two axes' scales)
        if let Some(s) = paint.stroke.as_mut() {
            let sx = m[0].hypot(m[3]);
            let sy = m[1].hypot(m[4]);
            s.width *= (sx * sy).sqrt();
        }
        Some(ShapeData { shape: self.shape.transformed(affine), paint })
    }
}

/// Draws the shape into tiles (only pixels within a generous margin around the canvas).
pub fn render(data: &ShapeData, canvas_w: u32, canvas_h: u32) -> TileMap {
    let (w, h) = (canvas_w as i64, canvas_h as i64);
    match vector::render(&data.shape, &data.paint, (-w, -h, 2 * w, 2 * h)) {
        Some(r) => raster::write(&r.rgba, r.x, r.y, r.w as usize, r.h as usize),
        None => TileMap::new(),
    }
}

/// The name a new shape layer gets (the interface passes it translated).
pub fn new_shape(doc: &mut Document, data: ShapeData, name: String) -> OpResult {
    let tiles = render(&data, doc.content.width, doc.content.height);
    let slot = active_slot(doc);
    let mut node = Node::pixel(name.clone(), TileMap::new());
    node.kind = NodeKind::Shape { data, tiles: Arc::new(tiles) };
    let id = node.id;
    doc.content.insert(node, Slot { parent: slot.parent, index: slot.index + 1 });
    doc.content.active = id;
    doc.commit(Label::with("history.newShape", name));
    Ok(())
}

/// New geometry or paint for a shape layer. `transient` updates show while the user drags a slider
/// or a point; the last call (not transient) makes the history step.
pub fn set_shape(doc: &mut Document, id: u64, data: ShapeData, transient: bool) -> OpResult {
    let (w, h) = (doc.content.width, doc.content.height);
    let node = doc.content.find_mut(id).ok_or("layer not found")?;
    if !matches!(node.kind, NodeKind::Shape { .. }) {
        return Err("not a shape layer".into());
    }
    let tiles = render(&data, w, h);
    node.kind = NodeKind::Shape { data, tiles: Arc::new(tiles) };
    let name = node.name.clone();
    if transient {
        doc.touch();
    } else {
        doc.commit(Label::with("history.editShape", name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, Content};
    use mosamem_core::vector::{Fill, Stroke};

    fn doc() -> Document {
        let bg = Node::pixel("bg", filled_tiles(600, 400, [255, 255, 255, 255]));
        let id = bg.id;
        Document::new("t".into(), None, Content::new(600, 400, 72.0, id, vec![bg]), Label::new("history.new"))
    }

    fn red_rect() -> ShapeData {
        ShapeData {
            shape: Shape::Rect { x: 100.0, y: 100.0, w: 200.0, h: 100.0, radius: 0.0 },
            paint: ShapePaint { fill: Some(Fill::Solid { color: [220, 30, 30, 255] }), stroke: Some(Stroke { color: [0, 0, 0, 255], width: 4.0, align: Default::default(), cap: Default::default(), join: Default::default(), dash: vec![] }) },
        }
    }

    fn reds(d: &Document) -> usize {
        let mut n = 0;
        for tx in 0..3 {
            for ty in 0..2 {
                if let Some(t) = composite_lod0(&d.content, tx, ty) {
                    n += t.0.chunks_exact(4).filter(|p| p[0] > 200 && p[1] < 60).count();
                }
            }
        }
        n
    }

    #[test]
    fn a_shape_layer_draws_edits_and_undoes() {
        let mut d = doc();
        new_shape(&mut d, red_rect(), "Rectangle 1".into()).unwrap();
        let id = d.content.active;
        // the fill inside the 2 px of the centered stroke: 196 × 96
        assert_eq!(reds(&d), 196 * 96);
        let mut wider = red_rect();
        if let Shape::Rect { w, .. } = &mut wider.shape {
            *w = 300.0;
        }
        set_shape(&mut d, id, wider, false).unwrap();
        assert_eq!(reds(&d), 296 * 96);
        d.undo();
        assert_eq!(reds(&d), 196 * 96);
        assert!(matches!(d.content.find(id).unwrap().kind, NodeKind::Shape { .. }));
    }

    #[test]
    fn moving_and_scaling_keep_it_a_rectangle() {
        let mut d = doc();
        new_shape(&mut d, red_rect(), "r".into()).unwrap();
        let id = d.content.active;
        crate::doc::transform::transform_layers(&mut d, &[id], &[0.5, 0.0, 20.0, 0.0, 0.5, 10.0, 0.0, 0.0, 1.0], mosamem_core::resample::Method::Bicubic, "history.transform").unwrap();
        let NodeKind::Shape { data, .. } = &d.content.find(id).unwrap().kind else { panic!("still a shape") };
        assert_eq!(data.shape, Shape::Rect { x: 70.0, y: 60.0, w: 100.0, h: 50.0, radius: 0.0 });
        assert_eq!(data.paint.stroke.as_ref().unwrap().width, 2.0, "the stroke scales too");
        // a turn makes it a free path, still a shape layer
        let (c, s) = (0.8, 0.6);
        crate::doc::transform::transform_layers(&mut d, &[id], &[c, -s, 300.0, s, c, 0.0, 0.0, 0.0, 1.0], mosamem_core::resample::Method::Bicubic, "history.transform").unwrap();
        let NodeKind::Shape { data, .. } = &d.content.find(id).unwrap().kind else { panic!("still a shape") };
        assert!(matches!(data.shape, Shape::Free { .. }));
    }
}
