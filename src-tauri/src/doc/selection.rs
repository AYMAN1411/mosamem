//! Selection and layer-mask operations on a document. Selections are part of history (like in
//! other editors); a refine-edge preview is not, until it is applied.

use std::sync::Arc;

use lawha_core::mask::{Dense, Mask};
use lawha_core::select::{self, Combine, Refine};
use lawha_core::tile::{Accum, Tile};
use serde::Deserialize;

use super::composite::{assemble, composite_nodes};
use super::model::{LayerMask, Node, NodeKind, Slot};
use super::{Document, Label};

pub type OpResult = Result<(), String>;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Shape {
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
    Ellipse { x0: f64, y0: f64, x1: f64, y1: f64, antialias: bool },
    Polygon { points: Vec<(f64, f64)>, antialias: bool },
}

fn dims(doc: &Document) -> (u32, u32) {
    (doc.content.width, doc.content.height)
}

pub fn current(doc: &Document) -> Dense {
    let (w, h) = dims(doc);
    match &doc.content.selection {
        Some(m) => m.to_dense(w, h),
        None => Dense::new(w, h, 0),
    }
}

/// Stores a selection (None if it selects nothing) and records a history step.
fn set(doc: &mut Document, d: Dense, label: &str) {
    let empty = d.data.iter().all(|v| *v == 0);
    if let Some(old) = doc.content.selection.take() {
        doc.last_selection = Some(old);
    }
    doc.content.selection = (!empty).then(|| Mask::from_dense(&d, 0));
    doc.preview = None;
    doc.commit(Label::new(label));
}

pub(crate) fn apply_combine(doc: &mut Document, shape: Dense, mode: Combine, label: &str) {
    let mut d = current(doc);
    select::combine(&mut d, &shape, if doc.content.selection.is_none() && mode != Combine::Intersect { Combine::Replace } else { mode });
    set(doc, d, label);
}

pub fn shape(doc: &mut Document, s: Shape, mode: Combine, feather: f32) -> OpResult {
    let (w, h) = dims(doc);
    let (mut d, label) = match s {
        Shape::Rect { x0, y0, x1, y1 } => (select::rect(w, h, x0, y0, x1, y1), "history.selectRect"),
        Shape::Ellipse { x0, y0, x1, y1, antialias } => (select::ellipse(w, h, x0, y0, x1, y1, antialias), "history.selectEllipse"),
        Shape::Polygon { points, antialias } => (select::polygon(w, h, &points, antialias), "history.selectLasso"),
    };
    if feather > 0.0 {
        d = select::feather(&d, feather);
    }
    apply_combine(doc, d, mode, label);
    Ok(())
}

/// The pixels the wand and quick selection sample: the active layer, or everything visible.
pub fn sample(doc: &Document, all_layers: bool) -> Vec<u8> {
    sample_from(doc, if all_layers { None } else { Some(doc.content.active) })
}

/// One layer's pixels on their own (Some), or everything visible (None), as full-canvas RGBA8.
pub fn sample_from(doc: &Document, layer: Option<u64>) -> Vec<u8> {
    use rayon::prelude::*;
    let c = &doc.content;
    let (tw, th) = c.tiles_wide();
    let nodes: Vec<Node> = match layer {
        None => c.root.clone(),
        Some(id) => match c.find(id) {
            Some(n) => {
                let mut n = n.clone();
                n.visible = true;
                n.clipped = false;
                vec![n]
            }
            None => Vec::new(),
        },
    };
    let tiles: Vec<((i32, i32), Arc<Tile>)> = (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|(tx, ty)| composite_nodes(&nodes, tx, ty).map(|t| ((tx, ty), Arc::new(t))))
        .collect();
    assemble(c.width, c.height, tiles)
}

#[allow(clippy::too_many_arguments)]
pub fn wand(doc: &mut Document, x: f64, y: f64, tolerance: u8, contiguous: bool, all_layers: bool, antialias: bool, mode: Combine) -> OpResult {
    let (w, h) = dims(doc);
    if x < 0.0 || y < 0.0 || x >= w as f64 || y >= h as f64 {
        return Ok(());
    }
    let img = sample(doc, all_layers);
    let d = select::wand(w, h, &img, (x as u32, y as u32), tolerance, contiguous, antialias);
    apply_combine(doc, d, mode, "history.selectWand");
    Ok(())
}

pub fn quick(doc: &mut Document, points: &[(f64, f64)], radius: f64, all_layers: bool, subtract: bool) -> OpResult {
    let (w, h) = dims(doc);
    let img = sample(doc, all_layers);
    let region = select::quick_select(w, h, &img, points, radius);
    apply_combine(doc, region, if subtract { Combine::Subtract } else { Combine::Add }, "history.selectQuick");
    Ok(())
}

pub fn all(doc: &mut Document) -> OpResult {
    let (w, h) = dims(doc);
    set(doc, Dense::new(w, h, 255), "history.selectAll");
    Ok(())
}

pub fn deselect(doc: &mut Document) -> OpResult {
    if doc.content.selection.is_none() {
        return Ok(());
    }
    let (w, h) = dims(doc);
    set(doc, Dense::new(w, h, 0), "history.deselect");
    Ok(())
}

pub fn reselect(doc: &mut Document) -> OpResult {
    let Some(m) = doc.last_selection.clone() else { return Ok(()) };
    let (w, h) = dims(doc);
    set(doc, m.to_dense(w, h), "history.reselect");
    Ok(())
}

pub fn invert(doc: &mut Document) -> OpResult {
    let mut d = current(doc);
    select::invert(&mut d);
    set(doc, d, "history.inverse");
    Ok(())
}

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase", tag = "op", content = "amount")]
pub enum Modify {
    Feather(f32),
    Expand(f32),
    Contract(f32),
    Border(f32),
    Smooth(u32),
}

pub fn modify(doc: &mut Document, m: Modify) -> OpResult {
    if doc.content.selection.is_none() {
        return Err("nothing is selected".into());
    }
    let d = current(doc);
    let (out, label) = match m {
        Modify::Feather(r) => (select::feather(&d, r.clamp(0.1, 1000.0)), "history.feather"),
        Modify::Expand(r) => (select::expand(&d, r.clamp(1.0, 500.0)), "history.expand"),
        Modify::Contract(r) => (select::contract(&d, r.clamp(1.0, 500.0)), "history.contract"),
        Modify::Border(r) => (select::border(&d, r.clamp(1.0, 200.0)), "history.border"),
        Modify::Smooth(r) => (select::smooth(&d, r.clamp(1, 100)), "history.smooth"),
    };
    set(doc, out, label);
    Ok(())
}

/// Moves the selection outline (not the pixels) by whole pixels.
pub fn translate(doc: &mut Document, dx: i32, dy: i32) -> OpResult {
    if doc.content.selection.is_none() || (dx == 0 && dy == 0) {
        return Ok(());
    }
    let d = current(doc);
    let (w, h) = (d.w as i32, d.h as i32);
    let mut out = Dense::new(d.w, d.h, 0);
    for y in 0..h {
        let sy = y - dy;
        if sy < 0 || sy >= h {
            continue;
        }
        for x in 0..w {
            let sx = x - dx;
            if sx >= 0 && sx < w {
                out.data[(y * w + x) as usize] = d.data[(sy * w + sx) as usize];
            }
        }
    }
    set(doc, out, "history.moveSelection");
    Ok(())
}

// ---- refine edge ----

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RefineOutput {
    Selection,
    LayerMask,
    NewLayer,
    NewLayerWithMask,
}

fn refined(doc: &Document, p: &Refine, decontam: bool) -> (Dense, Vec<u8>) {
    let img = sample(doc, false);
    let d = select::refine(&current(doc), &img, p);
    let mut img = img;
    if decontam {
        select::decontaminate(&mut img, &d, p.radius.max(4.0));
    }
    (d, img)
}

/// Shows the refined selection without recording it (the dialog's live preview).
pub fn refine_preview(doc: &mut Document, p: &Refine) -> OpResult {
    if doc.content.selection.is_none() {
        return Err("nothing is selected".into());
    }
    let (d, _) = refined(doc, p, false);
    doc.preview = Some(Mask::from_dense(&d, 0));
    doc.touch();
    Ok(())
}

pub fn refine_cancel(doc: &mut Document) {
    if doc.preview.take().is_some() {
        doc.touch();
    }
}

pub fn refine_apply(doc: &mut Document, p: &Refine, output: RefineOutput, decontam: bool, layer_name: &str) -> OpResult {
    if doc.content.selection.is_none() {
        return Err("nothing is selected".into());
    }
    let (d, img) = refined(doc, p, decontam && matches!(output, RefineOutput::NewLayer | RefineOutput::NewLayerWithMask));
    doc.preview = None;
    match output {
        RefineOutput::Selection => set(doc, d, "history.refineEdge"),
        RefineOutput::LayerMask => {
            let id = doc.content.active;
            let n = doc.content.find_mut(id).ok_or("layer not found")?;
            n.mask = Some(LayerMask::new(Mask::from_dense(&d, 0)));
            doc.content.selection = None;
            doc.commit(Label::new("history.refineEdge"));
        }
        RefineOutput::NewLayer | RefineOutput::NewLayerWithMask => {
            let (w, h) = dims(doc);
            let mut rgba = img;
            if output == RefineOutput::NewLayer {
                // bake the mask into alpha
                for (px, m) in rgba.chunks_exact_mut(4).zip(&d.data) {
                    px[3] = ((px[3] as u32 * *m as u32 + 127) / 255) as u8;
                }
            }
            let mut layer = Node::pixel(layer_name, super::tiles_from_rgba(w, h, &rgba));
            if output == RefineOutput::NewLayerWithMask {
                layer.mask = Some(LayerMask::new(Mask::from_dense(&d, 0)));
            }
            let slot = doc.content.slot_of(doc.content.active).unwrap_or(Slot { parent: None, index: doc.content.root.len() - 1 });
            let id = layer.id;
            // the source layer is hidden, like other editors do when outputting to a new layer
            if let Some(src) = doc.content.find_mut(doc.content.active) {
                src.visible = false;
            }
            doc.content.insert(layer, Slot { parent: slot.parent, index: slot.index + 1 });
            doc.content.active = id;
            doc.content.selection = None;
            doc.commit(Label::new("history.refineEdge"));
        }
    }
    Ok(())
}

// ---- layer masks ----

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum NewMask {
    RevealAll,
    HideAll,
    RevealSelection,
    HideSelection,
}

pub fn mask_add(doc: &mut Document, id: u64, kind: NewMask) -> OpResult {
    let (w, h) = dims(doc);
    let sel = current(doc);
    let has_sel = doc.content.selection.is_some();
    let mask = match kind {
        NewMask::RevealAll => Mask::uniform(255),
        NewMask::HideAll => Mask::uniform(0),
        NewMask::RevealSelection if has_sel => Mask::from_dense(&sel, 0),
        NewMask::HideSelection if has_sel => {
            let mut d = sel;
            select::invert(&mut d);
            Mask::from_dense(&d, 255)
        }
        _ => Mask::uniform(255),
    };
    let _ = (w, h);
    let n = doc.content.find_mut(id).ok_or("layer not found")?;
    if n.mask.is_some() {
        return Err("the layer already has a mask".into());
    }
    n.mask = Some(LayerMask::new(mask));
    if matches!(kind, NewMask::RevealSelection | NewMask::HideSelection) {
        doc.content.selection = None;
    }
    doc.commit(Label::new("history.addMask"));
    Ok(())
}

pub fn mask_delete(doc: &mut Document, id: u64) -> OpResult {
    let n = doc.content.find_mut(id).ok_or("layer not found")?;
    if n.mask.take().is_none() {
        return Ok(());
    }
    doc.commit(Label::new("history.deleteMask"));
    Ok(())
}

/// Bakes the mask into the layer's pixels (pixel layers only).
pub fn mask_apply(doc: &mut Document, id: u64) -> OpResult {
    let n = doc.content.find(id).ok_or("layer not found")?;
    let NodeKind::Pixel { tiles } = &n.kind else { return Err("only pixel layers".into()) };
    if n.mask.is_none() {
        return Ok(());
    }
    use rayon::prelude::*;
    let node = n.clone();
    let baked: super::TileMap = tiles
        .par_iter()
        .filter_map(|(&key, t)| {
            let mut acc = Accum::from_tile(t, 1.0);
            match super::composite::mask_factor(&node, 0, key) {
                Some(super::composite::MaskFactor::Const(k)) => acc.mul_factors(&vec![k; acc.0.len()]),
                Some(super::composite::MaskFactor::Each(v)) => acc.mul_factors(&v),
                None => {}
            }
            if acc.is_empty() {
                return None;
            }
            let mut out = Tile::transparent();
            acc.to_tile(&mut out);
            Some((key, Arc::new(out)))
        })
        .collect();
    let n = doc.content.find_mut(id).unwrap();
    n.kind = NodeKind::Pixel { tiles: Arc::new(baked) };
    n.mask = None;
    doc.commit(Label::new("history.applyMask"));
    Ok(())
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MaskProps {
    pub enabled: Option<bool>,
    pub density: Option<f32>,
    pub feather: Option<f32>,
    pub invert: Option<bool>,
}

pub fn mask_set(doc: &mut Document, id: u64, p: MaskProps, transient: bool) -> OpResult {
    let (w, h) = dims(doc);
    let n = doc.content.find_mut(id).ok_or("layer not found")?;
    let m = n.mask.as_mut().ok_or("the layer has no mask")?;
    let mut label = "history.mask";
    if let Some(v) = p.enabled {
        m.enabled = v;
        label = if v { "history.enableMask" } else { "history.disableMask" };
    }
    if let Some(v) = p.density {
        m.density = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p.feather {
        m.feather = v.clamp(0.0, 1000.0);
    }
    if p.invert == Some(true) {
        let mut d = m.mask.to_dense(w, h);
        select::invert(&mut d);
        m.mask = Mask::from_dense(&d, 255 - m.mask.outside);
        label = "history.invertMask";
    }
    if transient {
        doc.touch();
    } else {
        doc.commit(Label::new(label));
    }
    Ok(())
}

/// Loads a layer mask as the selection (Cmd/Ctrl-click on the mask thumbnail).
pub fn mask_to_selection(doc: &mut Document, id: u64) -> OpResult {
    let (w, h) = dims(doc);
    let n = doc.content.find(id).ok_or("layer not found")?;
    let m = n.mask.as_ref().ok_or("the layer has no mask")?;
    let d = m.mask.to_dense(w, h);
    set(doc, d, "history.loadSelection");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, Content};

    fn doc() -> Document {
        let bg = Node::pixel("bg", filled_tiles(300, 200, [0, 0, 0, 255]));
        let top = Node::pixel("top", filled_tiles(300, 200, [255, 255, 255, 255]));
        let active = top.id;
        Document::new("t".into(), None, Content::new(300, 200, 72.0, active, vec![bg, top]), Label::new("history.new"))
    }

    #[test]
    fn select_combine_and_undo() {
        let mut d = doc();
        shape(&mut d, Shape::Rect { x0: 10.0, y0: 10.0, x1: 110.0, y1: 60.0 }, Combine::Replace, 0.0).unwrap();
        assert_eq!(current(&d).count_over(127), 5000);
        shape(&mut d, Shape::Rect { x0: 60.0, y0: 10.0, x1: 160.0, y1: 60.0 }, Combine::Add, 0.0).unwrap();
        assert_eq!(current(&d).count_over(127), 7500);
        shape(&mut d, Shape::Rect { x0: 0.0, y0: 0.0, x1: 50.0, y1: 200.0 }, Combine::Subtract, 0.0).unwrap();
        assert_eq!(current(&d).count_over(127), 7500 - 2000);
        d.undo();
        assert_eq!(current(&d).count_over(127), 7500);
        deselect(&mut d).unwrap();
        assert!(d.content.selection.is_none());
        reselect(&mut d).unwrap();
        assert_eq!(current(&d).count_over(127), 7500);
        invert(&mut d).unwrap();
        assert_eq!(current(&d).count_over(127), 300 * 200 - 7500);
    }

    #[test]
    fn reveal_selection_mask_hides_the_rest() {
        let mut d = doc();
        shape(&mut d, Shape::Rect { x0: 0.0, y0: 0.0, x1: 150.0, y1: 200.0 }, Combine::Replace, 0.0).unwrap();
        let id = d.content.active;
        mask_add(&mut d, id, NewMask::RevealSelection).unwrap();
        assert!(d.content.selection.is_none());
        let t = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t.px(10, 10), [255, 255, 255, 255]);
        let t2 = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t2.px(200, 10), [0, 0, 0, 255]);
        mask_set(&mut d, id, MaskProps { invert: Some(true), ..Default::default() }, false).unwrap();
        let t3 = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t3.px(10, 10), [0, 0, 0, 255]);
        assert_eq!(t3.px(200, 10), [255, 255, 255, 255]);
        // applying bakes it into the pixels
        mask_apply(&mut d, id).unwrap();
        assert!(d.content.find(id).unwrap().mask.is_none());
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(200, 10), [255, 255, 255, 255]);
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(10, 10), [0, 0, 0, 255]);
    }

    #[test]
    fn translate_moves_the_outline() {
        let mut d = doc();
        shape(&mut d, Shape::Rect { x0: 10.0, y0: 10.0, x1: 20.0, y1: 20.0 }, Combine::Replace, 0.0).unwrap();
        translate(&mut d, 5, -3).unwrap();
        let c = current(&d);
        assert_eq!(c.at(15, 7), 255);
        assert_eq!(c.at(12, 12), 0);
    }
}
