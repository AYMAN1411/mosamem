//! Local AI (M10): Select Subject, Remove Background, Object Selection and Smart Fill. Models are
//! downloaded on first use (models.rs) and run with ONNX Runtime on the device (runtime.rs); no
//! picture ever leaves the machine. Each feature reads the document, lets go of it while the
//! model runs (so the canvas stays responsive), then applies the result as one history step.

pub mod models;
pub mod nets;
pub mod onnx_wide;
pub mod runtime;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Weak};

use mosamem_core::mask::{Dense, Mask};
use mosamem_core::select::{self, Combine, Refine};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::doc::model::{LayerMask, Node, NodeKind, Slot, TileMap};
use crate::doc::{raster, selection, DocHandle, Label};
use nets::{Click, Embedding};
use runtime::{AiResult, Runtime};

pub struct Ai {
    pub root: PathBuf,
    pub rt: Runtime,
    pub downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// The last SAM image embedding, with the pixels it was made from.
    embedding: Mutex<Option<(u64, Vec<Weak<TileMap>>, Arc<Embedding>)>>,
}

impl Ai {
    pub fn new(root: PathBuf) -> Self {
        Ai { root, rt: Runtime::default(), downloads: Mutex::default(), embedding: Mutex::default() }
    }

    /// The path of one of a pack's files, or `model-missing:<pack>` (the UI offers the download).
    fn file(&self, pack: &str, name: &str) -> AiResult<PathBuf> {
        let p = models::pack(pack).ok_or("unknown model")?;
        if !models::installed(&self.root, p) {
            return Err(format!("model-missing:{pack}"));
        }
        Ok(models::pack_dir(&self.root, p).join(name))
    }

    pub fn forget(&self, pack: &str) {
        if let Some(p) = models::pack(pack) {
            self.rt.unload(&models::pack_dir(&self.root, p));
        }
        if pack == "select" {
            *self.embedding.lock() = None;
        }
    }
}

fn to_dense(w: u32, h: u32, v: &[f32]) -> Dense {
    Dense { w, h, data: v.iter().map(|p| (p.clamp(0.0, 1.0) * 255.0).round() as u8).collect() }
}

/// Sharpens an upsampled model mask against the picture's own edges (the guided filter of Refine
/// Edge), in a band about as wide as one model pixel.
fn snap_edges(d: &Dense, rgba: &[u8], model_side: u32) -> Dense {
    let scale = d.w.max(d.h) as f32 / model_side as f32;
    if scale < 1.5 {
        return d.clone();
    }
    select::refine(d, rgba, &Refine { radius: (scale * 1.5).clamp(2.0, 24.0), smooth: 0, feather: 0.0, contrast: 0.0, shift: 0.0 })
}

/// A key for the pixels a sample is made of, plus weak handles that keep those tile maps' addresses
/// from being reused while the key is held (tile maps are never changed in place).
fn content_key(nodes: &[Node], doc_id: u64, layer: Option<u64>) -> (u64, Vec<Weak<TileMap>>) {
    use std::hash::{Hash, Hasher};
    fn walk(nodes: &[Node], h: &mut std::collections::hash_map::DefaultHasher, weak: &mut Vec<Weak<TileMap>>) {
        for n in nodes {
            (n.id, n.visible, n.clipped, n.blend.index(), n.opacity.to_bits(), n.fill.to_bits()).hash(h);
            n.style.as_ref().map(|s| s.key()).hash(h);
            if let Some(m) = &n.mask {
                (Arc::as_ptr(&m.mask.tiles) as usize, m.mask.outside, m.enabled, m.density.to_bits(), m.feather.to_bits()).hash(h);
            }
            match &n.kind {
                NodeKind::Pixel { tiles } | NodeKind::Text { tiles, .. } | NodeKind::Shape { tiles, .. } => {
                    (Arc::as_ptr(tiles) as usize).hash(h);
                    weak.push(Arc::downgrade(tiles));
                }
                NodeKind::Group { children, pass_through, .. } => {
                    pass_through.hash(h);
                    walk(children, h, weak);
                }
                NodeKind::Adjustment { op, .. } => (Arc::as_ptr(op) as usize).hash(h),
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (doc_id, layer).hash(&mut h);
    let mut weak = vec![];
    walk(nodes, &mut h, &mut weak);
    (h.finish(), weak)
}

// ---- Select Subject ----

pub fn select_subject(ai: &Ai, h: &DocHandle, mode: Combine) -> AiResult<()> {
    let model = ai.file("subject", "birefnet_lite_fp16.onnx")?;
    let (w, hh, rgba) = {
        let d = h.doc.read();
        (d.content.width, d.content.height, selection::sample_from(&d, None))
    };
    let p = nets::subject(&ai.rt, &model, &rgba, w, hh)?;
    let mask = snap_edges(&to_dense(w, hh, &p), &rgba, nets::SUBJECT_SIZE);
    let mut d = h.doc.write();
    if (d.content.width, d.content.height) != (w, hh) {
        return Err("the image size changed".into());
    }
    selection::apply_combine(&mut d, mask, mode, "history.selectSubject");
    Ok(())
}

// ---- Remove Background ----

/// Adds a layer mask that hides the layer's background (and keeps any mask it already had: the
/// two are multiplied). The model looks at the layer's own pixels, cropped to its content.
pub fn remove_background(ai: &Ai, h: &DocHandle, id: u64) -> AiResult<()> {
    let model = ai.file("subject", "birefnet_lite_fp16.onnx")?;
    let (tiles, (x0, y0, x1, y1)) = {
        let d = h.doc.read();
        let n = d.content.find(id).ok_or("layer not found")?;
        let NodeKind::Pixel { tiles } = &n.kind else { return Err("not-pixel-layer".into()) };
        let b = raster::content_bounds(tiles).ok_or("empty-layer")?;
        (tiles.clone(), b)
    };
    let (w, hh) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let rgba = raster::read(&tiles, x0, y0, w as usize, hh as usize);
    let p = nets::subject(&ai.rt, &model, &rgba, w, hh)?;
    let crop = snap_edges(&to_dense(w, hh, &p), &rgba, nets::SUBJECT_SIZE);
    let mut d = h.doc.write();
    let (cw, ch) = (d.content.width, d.content.height);
    // the crop placed on the canvas; outside the layer's content the mask hides (nothing is there)
    let mut full = Dense::new(cw, ch, 0);
    for y in 0..hh as i64 {
        let gy = y0 + y;
        if gy < 0 || gy >= ch as i64 {
            continue;
        }
        for x in 0..w as i64 {
            let gx = x0 + x;
            if gx >= 0 && gx < cw as i64 {
                full.data[(gy as usize) * cw as usize + gx as usize] = crop.data[(y as usize) * w as usize + x as usize];
            }
        }
    }
    let n = d.content.find_mut(id).ok_or("layer not found")?;
    if let Some(old) = &n.mask {
        let prev = old.mask.to_dense(cw, ch);
        for (v, o) in full.data.iter_mut().zip(&prev.data) {
            *v = ((*v as u32 * *o as u32 + 127) / 255) as u8;
        }
    }
    let keep = n.mask.as_ref().map(|m| (m.density, m.feather));
    let mut lm = LayerMask::new(Mask::from_dense(&full, 0));
    if let Some((density, feather)) = keep {
        (lm.density, lm.feather) = (density, feather);
    }
    n.mask = Some(lm);
    d.commit(Label::new("history.removeBackground"));
    Ok(())
}

// ---- Object Selection ----

fn sample_key(h: &DocHandle, all_layers: bool) -> (u64, Vec<Weak<TileMap>>) {
    let d = h.doc.read();
    let layer = (!all_layers).then_some(d.content.active);
    let nodes: Vec<Node> = match layer {
        None => d.content.root.clone(),
        Some(id) => d.content.find(id).cloned().into_iter().collect(),
    };
    content_key(&nodes, d.id, layer)
}

/// The image embedding for the current pixels, computed once and reused for every click.
fn embedding(ai: &Ai, h: &DocHandle, all_layers: bool) -> AiResult<Arc<Embedding>> {
    let encoder = ai.file("select", "vision_encoder_fp16.onnx")?;
    let (key, weak) = sample_key(h, all_layers);
    if let Some((k, w, e)) = ai.embedding.lock().as_ref() {
        if *k == key && w.iter().all(|w| w.strong_count() > 0) {
            return Ok(e.clone());
        }
    }
    let (w, hh, rgba) = {
        let d = h.doc.read();
        (d.content.width, d.content.height, selection::sample(&d, all_layers))
    };
    let e = Arc::new(nets::sam_encode(&ai.rt, &encoder, &rgba, w, hh)?);
    *ai.embedding.lock() = Some((key, weak, e.clone()));
    Ok(e)
}

/// Computes the embedding ahead of the first click (when the tool is picked).
pub fn object_prepare(ai: &Ai, h: &DocHandle, all_layers: bool) -> AiResult<()> {
    embedding(ai, h, all_layers).map(|_| ())
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSpec {
    pub clicks: Vec<Click>,
    pub bbox: Option<[f64; 4]>,
    pub mode: Combine,
    pub all_layers: bool,
}

pub fn object_select(ai: &Ai, h: &DocHandle, spec: &ObjectSpec) -> AiResult<()> {
    let decoder = ai.file("select", "prompt_encoder_mask_decoder.onnx")?;
    let e = embedding(ai, h, spec.all_layers)?;
    let p = nets::sam_decode(&ai.rt, &decoder, &e, &spec.clicks, spec.bbox)?;
    let rgba = {
        let d = h.doc.read();
        selection::sample(&d, spec.all_layers)
    };
    let mask = snap_edges(&to_dense(e.w, e.h, &p), &rgba, 256);
    let mut d = h.doc.write();
    if (d.content.width, d.content.height) != (e.w, e.h) {
        return Err("the image size changed".into());
    }
    selection::apply_combine(&mut d, mask, spec.mode, "history.selectObject");
    Ok(())
}

// ---- Smart Fill ----

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FillMethod {
    /// MI-GAN (downloaded model).
    Ai,
    /// PatchMatch (no model).
    Classic,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FillSpec {
    pub method: FillMethod,
    /// Put the fill on a new layer (named this) instead of changing the active layer.
    pub new_layer: Option<String>,
}

enum Source {
    Layer(Arc<TileMap>),
    /// Everything visible, full canvas RGBA8.
    Visible(Vec<u8>),
}

/// Fills the selected part of the active layer (or a new layer above it) from its surroundings.
pub fn smart_fill(ai: &Ai, h: &DocHandle, spec: &FillSpec) -> AiResult<()> {
    let model = match spec.method {
        FillMethod::Ai => Some(ai.file("fill", "migan_pipeline_v2.onnx")?),
        FillMethod::Classic => None,
    };
    // on a new layer the fill takes its colors from everything visible (so it can retouch a
    // picture made of several layers, and a second fill doesn't sample the first); in place, from
    // the active layer only
    let (cw, ch, sel, source, id, seed) = {
        let d = h.doc.read();
        let Some(sel) = d.content.selection.as_ref() else { return Err("no-selection".into()) };
        let n = d.content.find(d.content.active).ok_or("layer not found")?;
        let source = match (&spec.new_layer, &n.kind) {
            (Some(_), _) => Source::Visible(selection::sample_from(&d, None)),
            (None, NodeKind::Pixel { tiles }) => Source::Layer(tiles.clone()),
            (None, _) => return Err("not-pixel-layer".into()),
        };
        (d.content.width, d.content.height, sel.to_dense(d.content.width, d.content.height), source, n.id, d.change_counter)
    };
    // the hole: the selection grown a little, so its soft edge is filled too
    let grown = select::expand(&sel, 3.0);
    let (mut x0, mut y0, mut x1, mut y1) = (cw as i64, ch as i64, 0i64, 0i64);
    for y in 0..ch as i64 {
        for x in 0..cw as i64 {
            if grown.data[(y * cw as i64 + x) as usize] > 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 {
        return Err("no-selection".into());
    }
    // context around the hole: half its size on each side, at least 32 px
    let m = (((x1 - x0).max(y1 - y0)) / 2).max(32);
    let (bx0, by0, bx1, by1) = ((x0 - m).max(0), (y0 - m).max(0), (x1 + m).min(cw as i64), (y1 + m).min(ch as i64));
    let (bw, bh) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
    let rgba = match &source {
        Source::Layer(tiles) => raster::read(tiles, bx0, by0, bw, bh),
        Source::Visible(all) => (0..bh).flat_map(|y| {
            let at = ((by0 as usize + y) * cw as usize + bx0 as usize) * 4;
            all[at..at + bw * 4].iter().copied()
        }).collect(),
    };
    let at = |x: usize, y: usize| (by0 as usize + y) * cw as usize + bx0 as usize + x;
    let hole: Vec<bool> = (0..bw * bh).map(|i| grown.data[at(i % bw, i / bw)] > 0).collect();
    // straight RGB over the layer's own colors (transparent pixels count as gray)
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| {
        let a = p[3] as u32;
        [0, 1, 2].map(|c| ((p[c] as u32 * a + 128 * (255 - a) + 127) / 255) as u8)
    }).collect();
    let filled: Vec<u8> = match &model {
        Some(model) => {
            let known: Vec<bool> = hole.iter().map(|h| !h).collect();
            nets::fill(&ai.rt, model, &rgb, &known, bw as u32, bh as u32)?
        }
        None => {
            let f: Vec<[f32; 3]> = rgb.chunks_exact(3).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
            mosamem_core::inpaint::inpaint(bw, bh, &f, &hole, seed).into_iter().flat_map(|p| p.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect()
        }
    };
    // blend by the (grown, softened) selection: full fill inside, fading over the grown edge
    let soft = select::feather(&grown, 1.5);
    let mut out = rgba.clone();
    for i in 0..bw * bh {
        let t = soft.data[at(i % bw, i / bw)] as f32 / 255.0;
        if t <= 0.0 {
            if spec.new_layer.is_some() {
                out[i * 4 + 3] = 0;
            }
            continue;
        }
        let o = &mut out[i * 4..i * 4 + 4];
        if spec.new_layer.is_some() {
            o[..3].copy_from_slice(&filled[i * 3..i * 3 + 3]);
            o[3] = (t * 255.0).round() as u8;
        } else {
            for c in 0..3 {
                o[c] = (o[c] as f32 + (filled[i * 3 + c] as f32 - o[c] as f32) * t).round() as u8;
            }
            o[3] = (o[3] as f32 + (255.0 - o[3] as f32) * t).round() as u8;
        }
    }
    let mut d = h.doc.write();
    match &spec.new_layer {
        Some(name) => {
            let patch = raster::write(&out, bx0, by0, bw, bh);
            let slot = d.content.slot_of(id).ok_or("layer not found")?;
            let node = Node::pixel(name.clone(), patch);
            let nid = node.id;
            d.content.insert(node, Slot { parent: slot.parent, index: slot.index + 1 });
            d.content.active = nid;
        }
        None => {
            let n = d.content.find_mut(id).ok_or("layer not found")?;
            let NodeKind::Pixel { tiles } = &mut n.kind else { return Err("not-pixel-layer".into()) };
            *tiles = Arc::new(raster::paste(tiles, &out, bx0, by0, bw, bh));
        }
    }
    d.commit(Label::new("history.smartFill"));
    Ok(())
}

#[cfg(test)]
mod doc_tests {
    use super::*;
    use crate::doc::{tiles_from_rgba, Content, Document, Engine};

    /// A 300×200 picture: a smooth gradient with a red square in it.
    fn picture() -> (Engine, Arc<DocHandle>, u64) {
        let (w, h) = (300u32, 200u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let red = (120..160).contains(&x) && (80..120).contains(&y);
                rgba[i..i + 4].copy_from_slice(&if red { [230, 20, 20, 255] } else { [(x * 255 / w) as u8, 120, (y * 255 / h) as u8, 255] });
            }
        }
        let bg = Node::pixel("bg", tiles_from_rgba(w, h, &rgba));
        let id = bg.id;
        let doc = Document::new("t".into(), None, Content::new(w, h, 72.0, id, vec![bg]), Label::new("history.new"));
        let e = Engine::default();
        let h = e.insert(doc);
        (e, h, id)
    }

    fn select_square(h: &DocHandle) {
        let mut d = h.doc.write();
        crate::doc::selection::shape(&mut d, crate::doc::selection::Shape::Rect { x0: 118.0, y0: 78.0, x1: 162.0, y1: 122.0 }, Combine::Replace, 0.0).unwrap();
    }

    fn reds(h: &DocHandle) -> usize {
        let d = h.doc.read();
        selection::sample_from(&d, None).chunks_exact(4).filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count()
    }

    fn no_models() -> Ai {
        Ai::new(std::env::temp_dir().join(format!("mosamem-no-models-{}", std::process::id())))
    }

    #[test]
    fn classic_fill_removes_the_square_in_place() {
        let (_e, h, id) = picture();
        assert_eq!(reds(&h), 1600);
        select_square(&h);
        smart_fill(&no_models(), &h, &FillSpec { method: FillMethod::Classic, new_layer: None }).unwrap();
        assert_eq!(reds(&h), 0, "the red square is filled");
        let d = h.doc.read();
        assert_eq!(d.content.root.len(), 1, "no new layer");
        assert!(d.content.selection.is_some(), "the selection stays");
        // pixels away from the selection are untouched
        let px = raster::read(d.content.find(id).unwrap().tiles().unwrap(), 10, 10, 1, 1);
        assert_eq!(px, vec![(10 * 255 / 300) as u8, 120, (10 * 255 / 200) as u8, 255]);
    }

    #[test]
    fn fill_on_a_new_layer_keeps_the_picture_and_undoes() {
        let (_e, h, id) = picture();
        select_square(&h);
        smart_fill(&no_models(), &h, &FillSpec { method: FillMethod::Classic, new_layer: Some("Smart Fill".into()) }).unwrap();
        assert_eq!(reds(&h), 0, "covered by the new layer");
        {
            let d = h.doc.read();
            assert_eq!(d.content.root.len(), 2);
            let original = raster::read(d.content.find(id).unwrap().tiles().unwrap(), 140, 100, 1, 1);
            assert_eq!(original, vec![230, 20, 20, 255], "the photo itself is unchanged");
            let top = d.content.root.iter().find(|n| n.id != id).unwrap();
            assert_eq!(top.name, "Smart Fill");
            let b = raster::content_bounds(top.tiles().unwrap()).unwrap();
            assert!(b.0 >= 110 && b.2 <= 170, "only around the hole: {b:?}");
        }
        h.doc.write().undo();
        assert_eq!(reds(&h), 1600);
    }

    #[test]
    fn missing_models_and_bad_targets_say_so() {
        let (_e, h, id) = picture();
        let ai = no_models();
        assert_eq!(select_subject(&ai, &h, Combine::Replace).unwrap_err(), "model-missing:subject");
        assert_eq!(remove_background(&ai, &h, id).unwrap_err(), "model-missing:subject");
        let spec = ObjectSpec { clicks: vec![], bbox: None, mode: Combine::Replace, all_layers: false };
        assert_eq!(object_select(&ai, &h, &spec).unwrap_err(), "model-missing:select");
        assert_eq!(smart_fill(&ai, &h, &FillSpec { method: FillMethod::Ai, new_layer: None }).unwrap_err(), "model-missing:fill");
        assert_eq!(smart_fill(&ai, &h, &FillSpec { method: FillMethod::Classic, new_layer: None }).unwrap_err(), "no-selection");
    }
}
