//! PSD import and export, the engine's half. The webview's PSD codec (ag-psd, in a worker) turns a
//! file into a manifest of layers (this module's `PsdDoc`) plus one buffer of pixels, and back.
//!
//! Wire format both ways: [u32 LE manifest length][manifest JSON][pixels], where each layer's
//! pixels (straight RGBA8) and mask (one byte per pixel) are slices of the pixel part.

use std::sync::Arc;

use mosamem_core::adjust::Adjustment;
use mosamem_core::style::LayerStyle;
use mosamem_core::BlendMode;
use serde::{Deserialize, Serialize};

use super::composite;
use super::model::{Content, LayerMask, Locks, Node, NodeKind, TileMap};
use super::raster;
use super::text::TextData;
use super::{Document, Label};

/// A rectangle of pixels inside the pixel buffer.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pixels {
    pub offset: usize,
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdMask {
    /// One byte per pixel; `None` when the mask is only its default color.
    pub pixels: Option<Pixels>,
    pub default_color: u8,
    pub enabled: bool,
    pub density: f32,
    pub feather: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdLayer {
    pub name: String,
    /// "pixel", "group", "adjustment" or "text"
    pub kind: String,
    pub visible: bool,
    pub opacity: f32,
    pub fill: f32,
    pub blend: BlendMode,
    #[serde(default)]
    pub pass_through: bool,
    pub clipped: bool,
    #[serde(default)]
    pub locks: Locks,
    #[serde(default)]
    pub expanded: bool,
    pub pixels: Option<Pixels>,
    pub mask: Option<PsdMask>,
    #[serde(default)]
    pub children: Vec<PsdLayer>,
    pub adjustment: Option<Adjustment>,
    pub text: Option<TextData>,
    pub style: Option<LayerStyle>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PsdDoc {
    pub width: u32,
    pub height: u32,
    pub ppi: f64,
    /// Bottom to top.
    pub layers: Vec<PsdLayer>,
    /// The flattened image (export only).
    pub composite: Option<Pixels>,
}

/// Splits a wire message into its manifest and pixel parts.
pub fn split(body: &[u8]) -> Result<(PsdDoc, &[u8]), String> {
    if body.len() < 4 {
        return Err("empty message".into());
    }
    let n = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
    let json = body.get(4..4 + n).ok_or("short message")?;
    let doc: PsdDoc = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok((doc, &body[4 + n..]))
}

fn slice<'a>(px: &'a [u8], p: &Pixels, bpp: usize) -> Result<&'a [u8], String> {
    px.get(p.offset..p.offset + p.w * p.h * bpp).ok_or_else(|| "pixel data out of range".to_string())
}

fn mask_of(m: &PsdMask, px: &[u8]) -> Result<LayerMask, String> {
    let mask = match &m.pixels {
        Some(p) => raster::mask_write(slice(px, p, 1)?, p.x, p.y, p.w, p.h, m.default_color),
        None => mosamem_core::mask::Mask::uniform(m.default_color),
    };
    Ok(LayerMask { mask, enabled: m.enabled, density: m.density.clamp(0.0, 1.0), feather: m.feather.max(0.0) })
}

fn node_of(l: &PsdLayer, px: &[u8]) -> Result<Node, String> {
    let tiles = match &l.pixels {
        Some(p) => raster::write(slice(px, p, 4)?, p.x, p.y, p.w, p.h),
        None => TileMap::new(),
    };
    let mut n = match (l.kind.as_str(), &l.adjustment, &l.text) {
        ("group", _, _) => {
            let children = l.children.iter().map(|c| node_of(c, px)).collect::<Result<Vec<_>, _>>()?;
            let mut g = Node::group(l.name.clone(), children);
            if let NodeKind::Group { pass_through, expanded, .. } = &mut g.kind {
                *pass_through = l.pass_through;
                *expanded = l.expanded;
            }
            g
        }
        ("adjustment", Some(a), _) => Node::adjustment(l.name.clone(), a.clone()),
        ("text", _, Some(t)) => {
            let mut n = Node::pixel(l.name.clone(), TileMap::new());
            // the file's own pixels stay until the text is edited
            n.kind = NodeKind::Text { data: TextData { keep_pixels: true, ..t.clone() }, tiles: Arc::new(tiles) };
            n
        }
        _ => Node::pixel(l.name.clone(), tiles),
    };
    n.visible = l.visible;
    n.opacity = l.opacity.clamp(0.0, 1.0);
    n.fill = l.fill.clamp(0.0, 1.0);
    n.blend = l.blend;
    n.clipped = l.clipped;
    n.locks = l.locks;
    // the file decides whether there is a mask (adjustment layers otherwise get a white one)
    n.mask = match &l.mask {
        Some(m) => Some(mask_of(m, px)?),
        None => None,
    };
    // effects are kept even when switched off (or not drawn by Mosamem), so saving keeps them
    n.style = l.style.clone();
    Ok(n)
}

/// A new document from an imported PSD.
pub fn import(body: &[u8], name: String) -> Result<Document, String> {
    let (doc, px) = split(body)?;
    if doc.width == 0 || doc.height == 0 || doc.width > 300_000 || doc.height > 300_000 {
        return Err("bad document size".into());
    }
    let root = doc.layers.iter().map(|l| node_of(l, px)).collect::<Result<Vec<_>, _>>()?;
    let active = root.last().map_or(0, |n| n.id);
    let content = Content::new(doc.width, doc.height, if doc.ppi > 0.0 { doc.ppi } else { 72.0 }, active, root);
    Ok(Document::new(name, None, content, Label::new("history.open")))
}

/// Everything a PSD writer needs: the layer tree with each layer's pixels (cropped to what they
/// cover), masks, and the flattened image.
pub fn export(d: &Document, flat: (u32, u32, Vec<u8>)) -> Vec<u8> {
    let mut px: Vec<u8> = Vec::new();
    fn push(px: &mut Vec<u8>, data: &[u8], x: i64, y: i64, w: usize, h: usize) -> Pixels {
        let offset = px.len();
        px.extend_from_slice(data);
        Pixels { offset, x, y, w, h }
    }
    fn walk(nodes: &[Node], px: &mut Vec<u8>) -> Vec<PsdLayer> {
        nodes
            .iter()
            .map(|n| {
                let pixels = n.tiles().and_then(|t| {
                    let (x0, y0, x1, y1) = raster::content_bounds(t)?;
                    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
                    Some(push(px, &raster::read(t, x0, y0, w, h), x0, y0, w, h))
                });
                let mask = n.mask.as_ref().map(|m| {
                    let pixels = raster::tiles_bbox(m.mask.tiles.keys().copied()).map(|(x0, y0, x1, y1)| {
                        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
                        push(px, &raster::mask_read(&m.mask, x0, y0, w, h), x0, y0, w, h)
                    });
                    PsdMask { pixels, default_color: m.mask.outside, enabled: m.enabled, density: m.density, feather: m.feather }
                });
                let (kind, pass_through, expanded, children) = match &n.kind {
                    NodeKind::Group { children, pass_through, expanded } => ("group", *pass_through, *expanded, walk(children, px)),
                    NodeKind::Adjustment { .. } => ("adjustment", false, false, Vec::new()),
                    NodeKind::Text { .. } => ("text", false, false, Vec::new()),
                    NodeKind::Pixel { .. } => ("pixel", false, false, Vec::new()),
                };
                PsdLayer {
                    name: n.name.clone(),
                    kind: kind.into(),
                    visible: n.visible,
                    opacity: n.opacity,
                    fill: n.fill,
                    blend: n.blend,
                    pass_through,
                    clipped: n.clipped,
                    locks: n.locks,
                    expanded,
                    pixels,
                    mask,
                    children,
                    adjustment: n.adjustment_settings().cloned(),
                    text: n.text_data().cloned(),
                    style: n.style.clone(),
                }
            })
            .collect()
    }
    let layers = walk(&d.content.root, &mut px);
    let (w, h, rgba) = flat;
    let composite = Some(push(&mut px, &rgba, 0, 0, w as usize, h as usize));
    let doc = PsdDoc { width: d.content.width, height: d.content.height, ppi: d.content.ppi, layers, composite };
    let json = serde_json::to_vec(&doc).expect("manifest");
    let mut out = Vec::with_capacity(4 + json.len() + px.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(&px);
    out
}

/// The flattened image of a document (for export and for comparing with a file's own composite).
pub fn flatten_content(c: &Content) -> (u32, u32, Vec<u8>) {
    use rayon::prelude::*;
    let (w, h) = (c.width, c.height);
    let (tw, th) = (w.div_ceil(256) as i32, h.div_ceil(256) as i32);
    let keys: Vec<(i32, i32)> = (0..th).flat_map(|ty| (0..tw).map(move |tx| (tx, ty))).collect();
    let tiles: TileMap = keys.into_par_iter().filter_map(|(tx, ty)| composite::composite_nodes(&c.root, tx, ty).map(|t| ((tx, ty), Arc::new(t)))).collect();
    (w, h, raster::read(&tiles, 0, 0, w as usize, h as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::filled_tiles;

    #[test]
    fn export_then_import_keeps_the_document() {
        // a background, a half-transparent masked layer in a group, an adjustment and a text layer
        let bg = Node::pixel("خلفية", filled_tiles(300, 200, [240, 230, 220, 255]));
        let mut l = Node::pixel("طبقة", raster::write(&[200u8, 30, 40, 255].repeat(100 * 80), 50, 40, 100, 80));
        l.opacity = 0.6;
        l.blend = BlendMode::Multiply;
        l.mask = Some(LayerMask::new(raster::mask_write(&vec![255u8; 60 * 80], 50, 40, 60, 80, 0)));
        let mut g = Node::group("مجموعة", vec![l]);
        if let NodeKind::Group { pass_through, .. } = &mut g.kind {
            *pass_through = false;
        }
        let adj = Node::adjustment("عكس", Adjustment::Invert);
        let data = TextData { text: "نص".into(), style: Default::default(), width: None, height: None, matrix: [1.0, 0.0, 250.0, 0.0, 1.0, 150.0], keep_pixels: false };
        let mut text = Node::pixel("نص", Default::default());
        text.kind = NodeKind::Text { tiles: Arc::new(super::super::text::render(&data, 300, 200)), data };
        let active = text.id;
        let d = Document::new("t".into(), None, Content::new(300, 200, 144.0, active, vec![bg, g, adj, text]), Label::new("history.new"));
        let bytes = export(&d, flatten_content(&d.content));
        let back = import(&bytes, "back".into()).unwrap();
        assert_eq!((back.content.width, back.content.height, back.content.ppi), (300, 200, 144.0));
        let names: Vec<String> = back.content.root.iter().map(|n| n.name.clone()).collect();
        assert_eq!(names, vec!["خلفية", "مجموعة", "عكس", "نص"]);
        for (tx, ty) in [(0, 0), (1, 0)] {
            assert_eq!(composite_lod0(&d.content, tx, ty).map(|t| t.0), composite_lod0(&back.content, tx, ty).map(|t| t.0), "same picture");
        }
        let t = back.content.root[3].text_data().unwrap();
        assert!(t.keep_pixels && t.text == "نص");
        let (_, _, flat) = flatten_content(&back.content);
        let (_, _, want) = flatten_content(&d.content);
        assert_eq!(flat, want);
    }
}
