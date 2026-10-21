//! Paths and selections (M12): a path (the work path, or a shape layer's outline) made into a
//! selection, filled or stroked on a pixel layer, and a selection's edge made into a path.

use mosamem_core::mask::Dense;
use mosamem_core::select::{self, Combine};
use mosamem_core::vector::{self, Path};

use super::ops::OpResult;
use super::paint::{fill_with, sel_at};
use super::selection::{apply_combine, current};
use super::Document;

/// Loads a path as a selection.
pub fn select(doc: &mut Document, path: &Path, mode: Combine, feather: f32) -> OpResult {
    let (w, h) = (doc.content.width, doc.content.height);
    let mut d = Dense { w, h, data: vector::coverage(path, w, h) };
    if feather > 0.0 {
        d = select::feather(&d, feather);
    }
    apply_combine(doc, d, mode, "history.pathSelection");
    Ok(())
}

/// Fills the path's area on a pixel layer (within the selection, if there is one).
pub fn fill(doc: &mut Document, layer: u64, path: &Path, color: [u8; 3], opacity: f32) -> OpResult {
    let (w, h) = (doc.content.width, doc.content.height);
    let cov = vector::coverage(path, w, h);
    paint(doc, layer, cov, color, opacity, "history.fillPath")
}

/// Strokes the path on a pixel layer with a round brush `width` px wide.
pub fn stroke(doc: &mut Document, layer: u64, path: &Path, color: [u8; 3], width: f64, opacity: f32) -> OpResult {
    let (w, h) = (doc.content.width, doc.content.height);
    let cov = vector::stroke_coverage(path, width, w, h);
    paint(doc, layer, cov, color, opacity, "history.strokePath")
}

fn paint(doc: &mut Document, layer: u64, cov: Vec<u8>, color: [u8; 3], opacity: f32, label: &str) -> OpResult {
    let sel = doc.content.selection.clone();
    let wu = doc.content.width as i64;
    let col = [color[0] as f32, color[1] as f32, color[2] as f32, 255.0];
    fill_with(doc, layer, &|x, y| cov[(y * wu + x) as usize] as f32 / 255.0 * sel_at(&sel, x, y) * opacity, &|_, _| col, label)
}

/// The selection's edge as a path (None without a selection).
pub fn from_selection(doc: &Document, tolerance: f64) -> Option<Path> {
    doc.content.selection.as_ref()?;
    let d = current(doc);
    let p = vector::trace(&d.data, d.w, d.h, tolerance);
    (!p.subs.is_empty()).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::model::Node;
    use crate::doc::{filled_tiles, Content, Label};
    use mosamem_core::vector::Shape;

    fn doc() -> Document {
        let bg = Node::pixel("bg", filled_tiles(300, 200, [255, 255, 255, 255]));
        let id = bg.id;
        Document::new("t".into(), None, Content::new(300, 200, 72.0, id, vec![bg]), Label::new("history.new"))
    }

    #[test]
    fn a_path_round_trips_through_a_selection() {
        let mut d = doc();
        let p = Shape::Rect { x: 50.0, y: 40.0, w: 100.0, h: 60.0, radius: 0.0 }.path();
        select(&mut d, &p, Combine::Replace, 0.0).unwrap();
        let sel = current(&d);
        assert_eq!(sel.data.iter().filter(|v| **v == 255).count(), 6000);
        let back = from_selection(&d, 1.0).unwrap();
        assert_eq!(back.bounds(), Some((50.0, 40.0, 150.0, 100.0)));
    }

    #[test]
    fn fill_and_stroke_paint_the_layer() {
        let mut d = doc();
        let id = d.content.active;
        let p = Shape::Rect { x: 50.0, y: 40.0, w: 100.0, h: 60.0, radius: 0.0 }.path();
        fill(&mut d, id, &p, [255, 0, 0], 1.0).unwrap();
        let red = |d: &Document| {
            let t = d.content.find(id).unwrap().tiles().unwrap().get(&(0, 0)).unwrap().clone();
            t.0.chunks_exact(4).filter(|p| p[0] == 255 && p[1] == 0).count()
        };
        assert_eq!(red(&d), 6000);
        stroke(&mut d, id, &p, [0, 0, 255], 4.0, 1.0).unwrap();
        assert!(red(&d) < 6000 && red(&d) > 5000, "the stroke covers the fill's edge");
        d.undo();
        assert_eq!(red(&d), 6000);
    }
}
