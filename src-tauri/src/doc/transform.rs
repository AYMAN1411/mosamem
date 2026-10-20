//! Moving, transforming and resizing: layers (or the selected pixels), crop, canvas size, image
//! size, and turning or flipping the whole image. Masks and the selection follow the pixels.
//!
//! Whole-pixel moves and quarter turns are exact; everything else is resampled once from the
//! original pixels, with the method the user picked (mosamem_core::resample).

use std::sync::Arc;

use mosamem_core::mask::Mask;
use mosamem_core::resample::{self, Mat3, Method};
use mosamem_core::tile::{Accum, Tile, TILE};
use rayon::prelude::*;
use serde::Serialize;

use super::model::{Node, NodeKind, TileMap};
use super::ops::OpResult;
use super::raster;
use super::{Document, Label};

/// A whole-pixel translation, if the matrix is one.
fn whole_pixel_shift(m: &Mat3) -> Option<(i64, i64)> {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let id = near(m[0], 1.0) && near(m[1], 0.0) && near(m[3], 0.0) && near(m[4], 1.0) && near(m[6], 0.0) && near(m[7], 0.0) && near(m[8], 1.0);
    (id && near(m[2], m[2].round()) && near(m[5], m[5].round())).then(|| (m[2].round() as i64, m[5].round() as i64))
}

/// Integer bounds of a rectangle mapped through `m`, clipped to `limit`.
fn mapped_bounds(m: &Mat3, r: (i64, i64, i64, i64), limit: (i64, i64, i64, i64)) -> Option<(i64, i64, i64, i64)> {
    let corners = [(r.0, r.1), (r.2, r.1), (r.2, r.3), (r.0, r.3)].map(|(x, y)| resample::apply(m, x as f64, y as f64));
    if corners.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return None;
    }
    let x0 = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min).floor() as i64;
    let y0 = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min).floor() as i64;
    let x1 = corners.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max).ceil() as i64;
    let y1 = corners.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max).ceil() as i64;
    let b = (x0.max(limit.0), y0.max(limit.1), x1.min(limit.2), y1.min(limit.3));
    (b.0 < b.2 && b.1 < b.3).then_some(b)
}

fn shift(x: f64, y: f64) -> Mat3 {
    [1.0, 0.0, x, 0.0, 1.0, y, 0.0, 0.0, 1.0]
}

/// Pixels mapped through `m` (document px → document px).
pub fn transform_map(map: &TileMap, m: &Mat3, method: Method, limit: (i64, i64, i64, i64)) -> TileMap {
    if let Some((dx, dy)) = whole_pixel_shift(m) {
        return raster::translate(map, dx, dy);
    }
    let Some(r) = raster::content_bounds(map) else { return TileMap::new() };
    let Some(o) = mapped_bounds(m, r, limit) else { return TileMap::new() };
    let (w, h) = ((r.2 - r.0) as usize, (r.3 - r.1) as usize);
    let src = raster::read(map, r.0, r.1, w, h);
    let local = resample::multiply(m, &shift(r.0 as f64, r.1 as f64));
    let (ow, oh) = ((o.2 - o.0) as usize, (o.3 - o.1) as usize);
    let out = resample::transform(w, h, &src, &local, (o.0, o.1), ow, oh, method);
    raster::write(&out, o.0, o.1, ow, oh)
}

/// A mask mapped through `m`; where nothing lands, the mask's outside value.
pub fn transform_mask(mask: &Mask, m: &Mat3, method: Method, limit: (i64, i64, i64, i64)) -> Mask {
    if let Some((dx, dy)) = whole_pixel_shift(m) {
        return raster::translate_mask(mask, dx, dy);
    }
    let Some(r) = raster::tiles_bbox(mask.tiles.keys().copied()) else { return mask.clone() };
    let Some(o) = mapped_bounds(m, r, limit) else { return Mask::uniform(mask.outside) };
    let (w, h) = ((r.2 - r.0) as usize, (r.3 - r.1) as usize);
    // the value as a gray color, coverage in alpha
    let gray: Vec<u8> = raster::mask_read(mask, r.0, r.1, w, h).into_iter().flat_map(|v| [v, v, v, 255]).collect();
    let local = resample::multiply(m, &shift(r.0 as f64, r.1 as f64));
    let (ow, oh) = ((o.2 - o.0) as usize, (o.3 - o.1) as usize);
    let out = resample::transform(w, h, &gray, &local, (o.0, o.1), ow, oh, method);
    let outside = mask.outside as f32;
    let values: Vec<u8> = out.chunks_exact(4).map(|p| (p[0] as f32 * p[3] as f32 / 255.0 + outside * (1.0 - p[3] as f32 / 255.0) + 0.5) as u8).collect();
    raster::mask_write(&values, o.0, o.1, ow, oh, mask.outside)
}

/// Splits a layer by the selection: (selected pixels, the rest).
pub fn split_selected(map: &TileMap, sel: &Mask) -> (TileMap, TileMap) {
    split_by(map, sel)
}

fn split_by(map: &TileMap, sel: &Mask) -> (TileMap, TileMap) {
    let parts: Vec<((i32, i32), Option<Arc<Tile>>, Option<Arc<Tile>>)> = map
        .par_iter()
        .map(|(&k, t)| {
            let (mut inside, mut rest) = ((**t).clone(), (**t).clone());
            let mt = sel.tile(k);
            for (i, (a, b)) in inside.0.chunks_exact_mut(4).zip(rest.0.chunks_exact_mut(4)).enumerate() {
                let c = mt.map_or(sel.outside, |m| m.0[i]) as u32;
                let al = a[3] as u32;
                a[3] = ((al * c + 127) / 255) as u8;
                b[3] = ((al * (255 - c) + 127) / 255) as u8;
            }
            let keep = |t: Tile| (!t.is_transparent()).then(|| Arc::new(t));
            (k, keep(inside), keep(rest))
        })
        .collect();
    let (mut a, mut b) = (TileMap::new(), TileMap::new());
    for (k, i, r) in parts {
        if let Some(i) = i {
            a.insert(k, i);
        }
        if let Some(r) = r {
            b.insert(k, r);
        }
    }
    (a, b)
}

/// `top` over `bottom`, tile by tile.
fn over(top: &TileMap, bottom: &TileMap) -> TileMap {
    let mut keys: Vec<(i32, i32)> = top.keys().chain(bottom.keys()).copied().collect();
    keys.sort_unstable();
    keys.dedup();
    keys.into_par_iter()
        .filter_map(|k| match (top.get(&k), bottom.get(&k)) {
            (Some(t), None) => Some((k, t.clone())),
            (None, Some(b)) => Some((k, b.clone())),
            (Some(t), Some(b)) => {
                let mut acc = Accum::from_tile(b, 1.0);
                acc.composite(t, mosamem_core::BlendMode::Normal, 1.0, (0, 0));
                let mut out = Tile::transparent();
                acc.to_tile(&mut out);
                Some((k, Arc::new(out)))
            }
            (None, None) => None,
        })
        .collect()
}

/// Pixel and adjustment layers among `ids`, with groups standing for everything inside them.
fn targets(doc: &Document, ids: &[u64]) -> Vec<u64> {
    fn collect(n: &Node, out: &mut Vec<u64>) {
        match &n.kind {
            NodeKind::Group { children, .. } => children.iter().for_each(|c| collect(c, out)),
            _ => out.push(n.id),
        }
    }
    let mut out = Vec::new();
    for id in ids {
        if let Some(n) = doc.content.find(*id) {
            collect(n, &mut out);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn limit_for(doc: &Document) -> (i64, i64, i64, i64) {
    let (w, h) = (doc.content.width as i64, doc.content.height as i64);
    (-2 * w, -2 * h, 3 * w, 3 * h)
}

/// Transforms layers by `m` (document px → document px). With a selection, only the selected
/// pixels move (the rest stays) and the selection moves with them; without one, whole layers move
/// together with their masks.
pub fn transform_layers(doc: &mut Document, ids: &[u64], m: &Mat3, method: Method, label: &str) -> OpResult {
    let ids = targets(doc, ids);
    if ids.is_empty() {
        return Err("nothing to transform".into());
    }
    if resample::invert(m).is_none() {
        return Err("the transform is flat".into());
    }
    let limit = limit_for(doc);
    let sel = doc.content.selection.clone();
    for id in &ids {
        let node = doc.content.find(*id).unwrap();
        if node.locks.position {
            return Err("the layer's position is locked".into());
        }
    }
    let (cw, ch) = (doc.content.width, doc.content.height);
    for id in ids {
        let node = doc.content.find_mut(id).unwrap();
        // text stays text under moves, scales, turns and skews of the whole layer
        if let (NodeKind::Text { data, .. }, None) = (&node.kind, &sel) {
            if let Some(nd) = data.transformed(m) {
                let tiles = super::text::render(&nd, cw, ch);
                node.kind = NodeKind::Text { data: nd, tiles: Arc::new(tiles) };
                if let Some(lm) = node.mask.as_mut() {
                    lm.mask = transform_mask(&lm.mask, m, method, limit);
                }
                continue;
            }
        }
        // pixels (and text that can't stay text: it becomes pixels)
        if let Some(tiles) = node.tiles().cloned() {
            let new = match &sel {
                Some(s) => {
                    let (inside, rest) = split_by(&tiles, s);
                    over(&transform_map(&inside, m, method, limit), &rest)
                }
                None => transform_map(&tiles, m, method, limit),
            };
            node.kind = NodeKind::Pixel { tiles: Arc::new(new) };
        }
        if sel.is_none() {
            if let Some(lm) = node.mask.as_mut() {
                lm.mask = transform_mask(&lm.mask, m, method, limit);
            }
        }
    }
    if let Some(s) = sel {
        doc.content.selection = Some(transform_mask(&s, m, method, limit));
    }
    doc.commit(Label::new(label));
    Ok(())
}

/// Bounds of what a transform would move: the selection, or the layers' pixels.
#[derive(Serialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn bounds(doc: &Document, ids: &[u64]) -> Option<Bounds> {
    let mut b: Option<(i64, i64, i64, i64)> = None;
    let mut add = |r: (i64, i64, i64, i64)| {
        b = Some(match b {
            None => r,
            Some(o) => (o.0.min(r.0), o.1.min(r.1), o.2.max(r.2), o.3.max(r.3)),
        })
    };
    if let Some(s) = &doc.content.selection {
        let (w, h) = (doc.content.width, doc.content.height);
        let (x0, y0, x1, y1) = s.to_dense(w, h).bounds()?;
        add((x0 as i64, y0 as i64, x1 as i64, y1 as i64));
    } else {
        for id in targets(doc, ids) {
            let n = doc.content.find(id)?;
            match &n.kind {
                NodeKind::Pixel { tiles } | NodeKind::Text { tiles, .. } => {
                    if let Some(r) = raster::content_bounds(tiles) {
                        add(r);
                    }
                }
                // an adjustment covers the canvas
                _ => add((0, 0, doc.content.width as i64, doc.content.height as i64)),
            }
        }
    }
    b.map(|(x0, y0, x1, y1)| Bounds { x: x0 as f64, y: y0 as f64, w: (x1 - x0) as f64, h: (y1 - y0) as f64 })
}

// ---- the whole image ----

/// Applies a change of the whole image to every layer: `pixels` to pixel layers and masks, the
/// affine `geometry` to text layers (which stay editable; `size` is the canvas afterwards), and
/// `mask` to the selection.
fn every_layer(doc: &mut Document, size: (u32, u32), geometry: &Mat3, pixels: &(dyn Fn(&TileMap) -> TileMap + Sync), mask: &(dyn Fn(&Mask) -> Mask + Sync)) {
    type Px<'a> = &'a (dyn Fn(&TileMap) -> TileMap + Sync);
    type Mk<'a> = &'a (dyn Fn(&Mask) -> Mask + Sync);
    fn walk(nodes: &mut [Node], size: (u32, u32), geometry: &Mat3, pixels: Px, mask: Mk) {
        for n in nodes {
            match &n.kind {
                NodeKind::Pixel { tiles } => n.kind = NodeKind::Pixel { tiles: Arc::new(pixels(tiles)) },
                NodeKind::Text { data, tiles } => {
                    n.kind = match data.transformed(geometry) {
                        Some(nd) => {
                            let t = super::text::render(&nd, size.0, size.1);
                            NodeKind::Text { data: nd, tiles: Arc::new(t) }
                        }
                        None => NodeKind::Pixel { tiles: Arc::new(pixels(tiles)) },
                    }
                }
                _ => {}
            }
            if let Some(lm) = n.mask.as_mut() {
                lm.mask = mask(&lm.mask);
            }
            if let Some(c) = n.children_mut() {
                walk(c, size, geometry, pixels, mask);
            }
        }
    }
    walk(&mut doc.content.root, size, geometry, pixels, mask);
    if let Some(s) = doc.content.selection.take() {
        doc.content.selection = Some(mask(&s));
    }
}

/// Image › Image Size. With `method: None` only the resolution changes.
pub fn image_size(doc: &mut Document, width: u32, height: u32, ppi: f64, method: Option<Method>) -> OpResult {
    if width == 0 || height == 0 || width > 60_000 || height > 60_000 {
        return Err("size out of range".into());
    }
    let Some(method) = method else {
        doc.content.ppi = ppi;
        doc.commit(Label::new("history.imageSize"));
        return Ok(());
    };
    let (w, h) = (doc.content.width as f64, doc.content.height as f64);
    let (sx, sy) = (width as f64 / w, height as f64 / h);
    // each layer's pixels together with the canvas, so canvas-aligned content stays aligned exactly
    let canvas = (0i64, 0i64, doc.content.width as i64, doc.content.height as i64);
    let scale_rect = move |r: (i64, i64, i64, i64)| ((r.0 as f64 * sx).round() as i64, (r.1 as f64 * sy).round() as i64, (r.2 as f64 * sx).round() as i64, (r.3 as f64 * sy).round() as i64);
    let union = move |r: Option<(i64, i64, i64, i64)>| r.map_or(canvas, |r| (r.0.min(canvas.0), r.1.min(canvas.1), r.2.max(canvas.2), r.3.max(canvas.3)));
    let pixels = move |map: &TileMap| -> TileMap {
        if map.is_empty() {
            return TileMap::new();
        }
        let r = union(raster::content_bounds(map));
        let n = scale_rect(r);
        let (rw, rh, nw, nh) = ((r.2 - r.0) as usize, (r.3 - r.1) as usize, ((n.2 - n.0) as usize).max(1), ((n.3 - n.1) as usize).max(1));
        let out = resample::resize(rw, rh, &raster::read(map, r.0, r.1, rw, rh), nw, nh, method);
        raster::write(&out, n.0, n.1, nw, nh)
    };
    let mask = move |m: &Mask| -> Mask {
        let Some(tb) = raster::tiles_bbox(m.tiles.keys().copied()) else { return m.clone() };
        let r = union(Some(tb));
        let n = scale_rect(r);
        let (rw, rh, nw, nh) = ((r.2 - r.0) as usize, (r.3 - r.1) as usize, ((n.2 - n.0) as usize).max(1), ((n.3 - n.1) as usize).max(1));
        let gray: Vec<u8> = raster::mask_read(m, r.0, r.1, rw, rh).into_iter().flat_map(|v| [v, v, v, 255]).collect();
        let out = resample::resize(rw, rh, &gray, nw, nh, method);
        raster::mask_write(&out.chunks_exact(4).map(|p| p[0]).collect::<Vec<_>>(), n.0, n.1, nw, nh, m.outside)
    };
    every_layer(doc, (width, height), &[sx, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 1.0], &pixels, &mask);
    doc.content.width = width;
    doc.content.height = height;
    doc.content.ppi = ppi;
    doc.commit(Label::new("history.imageSize"));
    Ok(())
}

/// Image › Canvas Size. `anchor` is where the old canvas sits in the new one (0, ½ or 1 on each
/// axis). With `fill`, the new area of the bottom layer gets that color; otherwise it stays
/// transparent. Pixels outside the new canvas are kept.
pub fn canvas_size(doc: &mut Document, width: u32, height: u32, anchor: (f64, f64), fill: Option<[u8; 4]>) -> OpResult {
    if width == 0 || height == 0 || width > 60_000 || height > 60_000 {
        return Err("size out of range".into());
    }
    let (ow, oh) = (doc.content.width as i64, doc.content.height as i64);
    let dx = ((width as i64 - ow) as f64 * anchor.0).round() as i64;
    let dy = ((height as i64 - oh) as f64 * anchor.1).round() as i64;
    every_layer(doc, (width, height), &shift(dx as f64, dy as f64), &|m| raster::translate(m, dx, dy), &|m| raster::translate_mask(m, dx, dy));
    doc.content.width = width;
    doc.content.height = height;
    if let Some(color) = fill {
        if let Some(Node { kind: NodeKind::Pixel { tiles }, .. }) = doc.content.root.first_mut() {
            let old = (dx, dy, dx + ow, dy + oh);
            let (w, h) = (width as i64, height as i64);
            let filled = Tile::filled(color);
            let keys: Vec<(i32, i32)> = (0..((h + TILE as i64 - 1) / TILE as i64) as i32).flat_map(|ty| (0..((w + TILE as i64 - 1) / TILE as i64) as i32).map(move |tx| (tx, ty))).collect();
            let fresh: Vec<((i32, i32), Arc<Tile>)> = keys
                .into_par_iter()
                .filter_map(|(tx, ty)| {
                    let (x0, y0) = (tx as i64 * TILE as i64, ty as i64 * TILE as i64);
                    // tiles wholly inside the old canvas keep their pixels
                    if x0 >= old.0 && y0 >= old.1 && x0 + TILE as i64 <= old.2 && y0 + TILE as i64 <= old.3 {
                        return None;
                    }
                    let mut under = filled.clone();
                    for (i, p) in under.0.chunks_exact_mut(4).enumerate() {
                        let (x, y) = (x0 + (i % TILE) as i64, y0 + (i / TILE) as i64);
                        if x >= w || y >= h || (x >= old.0 && x < old.2 && y >= old.1 && y < old.3) {
                            p.copy_from_slice(&[0, 0, 0, 0]);
                        }
                    }
                    let mut acc = Accum::from_tile(&under, 1.0);
                    if let Some(t) = tiles.get(&(tx, ty)) {
                        acc.composite(t, mosamem_core::BlendMode::Normal, 1.0, (0, 0));
                    }
                    let mut out = Tile::transparent();
                    acc.to_tile(&mut out);
                    Some(((tx, ty), Arc::new(out)))
                })
                .collect();
            let mut map = (**tiles).clone();
            map.extend(fresh);
            *tiles = Arc::new(map);
        }
    }
    doc.commit(Label::new("history.canvasSize"));
    Ok(())
}

/// Removes every layer's pixels outside the canvas.
fn trim_to_canvas(doc: &mut Document) {
    let (w, h) = (doc.content.width as i64, doc.content.height as i64);
    let trim = move |map: &TileMap| -> TileMap {
        map.par_iter()
            .filter_map(|(&(tx, ty), t)| {
                let (x0, y0) = (tx as i64 * TILE as i64, ty as i64 * TILE as i64);
                if x0 >= w || y0 >= h || x0 + TILE as i64 <= 0 || y0 + TILE as i64 <= 0 {
                    return None;
                }
                if x0 >= 0 && y0 >= 0 && x0 + TILE as i64 <= w && y0 + TILE as i64 <= h {
                    return Some(((tx, ty), t.clone()));
                }
                let mut c = (**t).clone();
                for (i, p) in c.0.chunks_exact_mut(4).enumerate() {
                    let (x, y) = (x0 + (i % TILE) as i64, y0 + (i / TILE) as i64);
                    if x < 0 || y < 0 || x >= w || y >= h {
                        p.copy_from_slice(&[0, 0, 0, 0]);
                    }
                }
                (!c.is_transparent()).then(|| ((tx, ty), Arc::new(c)))
            })
            .collect()
    };
    let size = (doc.content.width, doc.content.height);
    every_layer(doc, size, &resample::IDENTITY, &trim, &|m| m.clone());
}

/// Image › Crop / the Crop tool: the canvas becomes the rectangle (x, y, w, h), turned by `angle`
/// degrees around its center (the straighten angle). With `delete`, pixels outside are dropped.
pub fn crop(doc: &mut Document, x: f64, y: f64, w: f64, h: f64, angle: f64, delete: bool, method: Method) -> OpResult {
    let (nw, nh) = (w.round() as u32, h.round() as u32);
    if nw == 0 || nh == 0 || nw > 60_000 || nh > 60_000 {
        return Err("size out of range".into());
    }
    if angle.abs() < 1e-6 {
        let (dx, dy) = (-(x.round() as i64), -(y.round() as i64));
        every_layer(doc, (nw, nh), &shift(dx as f64, dy as f64), &|m| raster::translate(m, dx, dy), &|m| raster::translate_mask(m, dx, dy));
    } else {
        // turn the crop rectangle upright around its center, then move it to the origin
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (s, c) = (-angle).to_radians().sin_cos();
        let rot: Mat3 = [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0];
        let m = resample::multiply(&shift(w / 2.0, h / 2.0), &resample::multiply(&rot, &shift(-cx, -cy)));
        let limit = (-(nw as i64) * 2, -(nh as i64) * 2, nw as i64 * 3, nh as i64 * 3);
        every_layer(doc, (nw, nh), &m, &|t| transform_map(t, &m, method, limit), &|mk| transform_mask(mk, &m, method, limit));
    }
    doc.content.width = nw;
    doc.content.height = nh;
    doc.content.selection = None;
    if delete {
        trim_to_canvas(doc);
    }
    doc.commit(Label::new("history.crop"));
    Ok(())
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Turn {
    Cw90,
    Ccw90,
    Half,
    FlipH,
    FlipV,
}

/// Image › Image Rotation: exact quarter turns and flips of everything.
pub fn turn_image(doc: &mut Document, t: Turn) -> OpResult {
    let (w, h) = (doc.content.width as f64, doc.content.height as f64);
    let (m, nw, nh): (Mat3, f64, f64) = match t {
        Turn::Cw90 => ([0.0, -1.0, h, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0], h, w),
        Turn::Ccw90 => ([0.0, 1.0, 0.0, -1.0, 0.0, w, 0.0, 0.0, 1.0], h, w),
        Turn::Half => ([-1.0, 0.0, w, 0.0, -1.0, h, 0.0, 0.0, 1.0], w, h),
        Turn::FlipH => ([-1.0, 0.0, w, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], w, h),
        Turn::FlipV => ([1.0, 0.0, 0.0, 0.0, -1.0, h, 0.0, 0.0, 1.0], w, h),
    };
    let limit = (-(w.max(h) as i64) * 3, -(w.max(h) as i64) * 3, w.max(h) as i64 * 4, w.max(h) as i64 * 4);
    every_layer(doc, (nw as u32, nh as u32), &m, &|t| transform_map(t, &m, Method::Nearest, limit), &|mk| transform_mask(mk, &m, Method::Nearest, limit));
    doc.content.width = nw as u32;
    doc.content.height = nh as u32;
    doc.commit(Label::new(match t {
        Turn::FlipH | Turn::FlipV => "history.flipImage",
        _ => "history.rotateImage",
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{Content, Node};

    fn doc_with(rgba: &[u8], w: u32, h: u32) -> Document {
        let n = Node::pixel("a", raster::write(rgba, 0, 0, w as usize, h as usize));
        let id = n.id;
        Document::new("t".into(), None, Content::new(w, h, 72.0, id, vec![n]), Label::new("history.new"))
    }

    fn pattern(w: usize, h: usize) -> Vec<u8> {
        (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, (i / w * 255 / h) as u8, 90, 255]).collect()
    }

    fn pixels(d: &Document, id: u64) -> Vec<u8> {
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        raster::read(&t, 0, 0, d.content.width as usize, d.content.height as usize)
    }

    #[test]
    fn moving_by_whole_pixels_is_exact_and_one_step() {
        let mut d = doc_with(&pattern(300, 200), 300, 200);
        let id = d.content.active;
        let before = pixels(&d, id);
        transform_layers(&mut d, &[id], &shift(17.0, -9.0), Method::Bicubic, "history.move").unwrap();
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::read(&t, 17, -9, 300, 200), before);
        d.undo();
        assert_eq!(pixels(&d, id), before);
    }

    #[test]
    fn a_selection_moves_only_its_pixels_and_comes_along() {
        let mut d = doc_with(&pattern(300, 200), 300, 200);
        let id = d.content.active;
        let before = pixels(&d, id);
        let mut sel = mosamem_core::mask::Dense::new(300, 200, 0);
        for y in 50..100 {
            for x in 50..100 {
                sel.data[y * 300 + x] = 255;
            }
        }
        d.content.selection = Some(Mask::from_dense(&sel, 0));
        transform_layers(&mut d, &[id], &shift(100.0, 0.0), Method::Bilinear, "history.move").unwrap();
        let after = pixels(&d, id);
        let px = |b: &[u8], x: usize, y: usize| b[(y * 300 + x) * 4..][..4].to_vec();
        assert_eq!(px(&after, 160, 60), px(&before, 60, 60), "moved");
        assert_eq!(px(&after, 60, 60)[3], 0, "a hole where it was");
        assert_eq!(px(&after, 20, 20), px(&before, 20, 20), "the rest stays");
        let s = d.content.selection.as_ref().unwrap().to_dense(300, 200);
        assert_eq!((s.at(160, 60), s.at(60, 60)), (255, 0));
    }

    #[test]
    fn transform_previews_float_the_selection_and_work_under_adjustments() {
        use crate::doc::live::{Part, Session};
        let mut d = doc_with(&[50u8, 100, 150, 255].repeat(300 * 200), 300, 200);
        let id = d.content.active;
        let mut sel = mosamem_core::mask::Dense::new(300, 200, 0);
        for y in 0..200 {
            for x in 0..100 {
                sel.data[y * 300 + x] = 255;
            }
        }
        d.content.selection = Some(Mask::from_dense(&sel, 0));
        let s = Session::begin_transform(1, &d.content, id).unwrap();
        // what moves: the selected pixels only; what stays: the rest, in the part below
        let layer = s.tile(Part::Layer, 0, 0, 0).unwrap();
        assert_eq!((layer.px(50, 50)[3], layer.px(150, 50)[3]), (255, 0));
        let below = s.tile(Part::Below, 0, 0, 0).unwrap();
        assert_eq!((below.px(50, 50)[3], below.px(150, 50)[3]), (0, 255));
        assert!(!s.reusable());
        // under an adjustment layer the split is impossible, but a preview still starts
        d.content.selection = None;
        crate::doc::adjustments::new_adjustment(&mut d, "inv".into(), mosamem_core::adjust::Adjustment::Invert).unwrap();
        let s = Session::begin_transform(1, &d.content, id).unwrap();
        assert!(s.tile(Part::Below, 0, 0, 0).is_none(), "everything but the moving layer: nothing here");
        assert!(s.tile(Part::Layer, 0, 0, 0).is_some());
    }

    #[test]
    fn a_turn_and_back_restores_the_image() {
        let mut d = doc_with(&pattern(300, 200), 300, 200);
        let id = d.content.active;
        let before = pixels(&d, id);
        turn_image(&mut d, Turn::Cw90).unwrap();
        assert_eq!((d.content.width, d.content.height), (200, 300));
        // the top-left pixel went to the top-right
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::read(&t, 199, 0, 1, 1), before[..4].to_vec());
        turn_image(&mut d, Turn::Ccw90).unwrap();
        assert_eq!(pixels(&d, id), before);
        turn_image(&mut d, Turn::FlipH).unwrap();
        turn_image(&mut d, Turn::FlipH).unwrap();
        assert_eq!(pixels(&d, id), before);
    }

    #[test]
    fn rotation_keeps_the_picture_and_its_mask_together() {
        let mut d = doc_with(&[200u8, 30, 30, 255].repeat(300 * 200), 300, 200);
        let id = d.content.active;
        d.content.find_mut(id).unwrap().mask = Some(crate::doc::model::LayerMask::new(raster::mask_write(&vec![255u8; 100 * 200], 0, 0, 100, 200, 0)));
        // 180° about the center: the visible left third ends up on the right
        let m = resample::multiply(&shift(150.0, 100.0), &resample::multiply(&[-1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0], &shift(-150.0, -100.0)));
        transform_layers(&mut d, &[id], &m, Method::Bicubic, "history.transform").unwrap();
        let c = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(c.px(250, 100)[3], 255);
        assert_eq!(c.px(50, 100)[3], 0);
    }

    #[test]
    fn image_size_scales_everything_and_ppi_alone_does_not_resample() {
        let mut d = doc_with(&pattern(300, 200), 300, 200);
        let id = d.content.active;
        image_size(&mut d, 150, 100, 144.0, Some(Method::Bicubic)).unwrap();
        assert_eq!((d.content.width, d.content.height, d.content.ppi), (150, 100, 144.0));
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::content_bounds(&t), Some((0, 0, 150, 100)));
        // the gradient survives: left dark red, right bright red
        let p = raster::read(&t, 0, 50, 150, 1);
        assert!(p[0] < 10 && p[149 * 4] > 240, "{} {}", p[0], p[149 * 4]);
        image_size(&mut d, 150, 100, 300.0, None).unwrap();
        assert_eq!(d.content.ppi, 300.0);
    }

    #[test]
    fn canvas_size_anchors_and_fills_the_new_area() {
        let mut d = doc_with(&[10u8, 20, 30, 255].repeat(100 * 100), 100, 100);
        let id = d.content.active;
        canvas_size(&mut d, 300, 200, (0.5, 1.0), Some([255, 255, 255, 255])).unwrap();
        assert_eq!((d.content.width, d.content.height), (300, 200));
        let c = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(c.px(150, 150), [10, 20, 30, 255], "old pixels at the bottom center");
        assert_eq!(c.px(10, 10), [255, 255, 255, 255], "new area filled");
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::read(&t, 100, 100, 1, 1), vec![10, 20, 30, 255]);
        // shrinking keeps pixels outside the canvas
        canvas_size(&mut d, 50, 50, (0.0, 0.0), None).unwrap();
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::content_bounds(&t), Some((0, 0, 300, 200)));
    }

    #[test]
    fn crop_keeps_or_deletes_and_can_straighten() {
        let mut d = doc_with(&pattern(300, 200), 300, 200);
        let id = d.content.active;
        let before = pixels(&d, id);
        crop(&mut d, 50.0, 40.0, 100.0, 80.0, 0.0, false, Method::Bicubic).unwrap();
        assert_eq!((d.content.width, d.content.height), (100, 80));
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::read(&t, 0, 0, 1, 1), before[(40 * 300 + 50) * 4..][..4].to_vec());
        assert_eq!(raster::content_bounds(&t), Some((-50, -40, 250, 160)), "hidden pixels kept");
        d.undo();
        crop(&mut d, 50.0, 40.0, 100.0, 80.0, 0.0, true, Method::Bicubic).unwrap();
        let t = d.content.find(id).unwrap().tiles().unwrap().clone();
        assert_eq!(raster::content_bounds(&t), Some((0, 0, 100, 80)), "deleted");
        d.undo();
        // a 10° straighten: the canvas is the crop size and fully covered
        crop(&mut d, 60.0, 50.0, 150.0, 100.0, 10.0, true, Method::Bicubic).unwrap();
        let c = composite_lod0(&d.content, 0, 0).unwrap();
        assert!((0..100).all(|y| c.px(0, y)[3] == 255 && c.px(149, y)[3] == 255));
    }
}
