//! Painting on a layer (or its mask): strokes for the brush, eraser, clone stamp and healing tools,
//! plus the paint bucket, gradient and eyedropper.
//!
//! A stroke keeps the layer's tiles as they were when it started and a coverage buffer
//! (lawha_core::paint). Each batch of pointer samples adds dabs and recomputes only the tiles they
//! touched, writing them into the document's live content. The display picks up just those tiles
//! (through the live session, or by per-tile invalidation of the composite). The stroke becomes one
//! history step when it ends.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lawha_core::mask::{Mask, MaskMap, MaskTile};
use lawha_core::paint::{dither, gradient_t, Brush, GradientKind, Spacer, StrokeBuffer};
use lawha_core::tile::{Tile, TILE, TILE_PX};
use serde::Deserialize;

use super::model::{NodeKind, TileMap};
use super::selection::{sample, sample_from};
use super::{Document, Label};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaintTool {
    Brush,
    Eraser,
    Clone,
    Heal,
    SpotHeal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    Pixels,
    Mask,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeSpec {
    pub tool: PaintTool,
    pub target: Target,
    pub layer: u64,
    /// sRGB 0–255 (the background color for the eraser on locked transparency)
    pub color: [u8; 3],
    pub brush: Brush,
    /// Clone/heal: source minus destination, in document pixels.
    pub offset: Option<(f32, f32)>,
    /// Clone/heal: sample all visible layers instead of the layer itself.
    pub sample_all: bool,
}

pub struct Stroke {
    pub doc: u64,
    pub spec: StrokeSpec,
    w: u32,
    h: u32,
    orig: Arc<TileMap>,
    orig_mask: Option<Mask>,
    source: Option<Arc<Vec<u8>>>,
    buffer: StrokeBuffer,
    spacer: Spacer,
    selection: Option<Mask>,
    lock_alpha: bool,
    pub touched: HashSet<(i32, i32)>,
}

fn label_for(t: PaintTool) -> &'static str {
    match t {
        PaintTool::Brush => "history.brush",
        PaintTool::Eraser => "history.eraser",
        PaintTool::Clone => "history.clone",
        PaintTool::Heal => "history.heal",
        PaintTool::SpotHeal => "history.spotHeal",
    }
}

pub fn begin(doc: &Document, spec: StrokeSpec) -> Result<Stroke, String> {
    let node = doc.content.find(spec.layer).ok_or("layer not found")?;
    // pixels need a pixel layer; any layer with a mask can have its mask painted
    let tiles = match (&node.kind, spec.target) {
        (NodeKind::Pixel { tiles }, _) => tiles.clone(),
        (_, Target::Mask) => node.tiles().cloned().unwrap_or_default(),
        (NodeKind::Text { .. }, Target::Pixels) => return Err("rasterize the text layer to paint on it".into()),
        _ => return Err("choose a pixel layer to paint on".into()),
    };
    if spec.target == Target::Pixels && node.locks.pixels {
        return Err("the layer's pixels are locked".into());
    }
    let orig_mask = match spec.target {
        Target::Mask => Some(node.mask.as_ref().ok_or("the layer has no mask")?.mask.clone()),
        Target::Pixels => None,
    };
    let source = matches!(spec.tool, PaintTool::Clone | PaintTool::Heal | PaintTool::SpotHeal)
        .then(|| Arc::new(sample_from(doc, if spec.sample_all { None } else { Some(spec.layer) })));
    Ok(Stroke {
        doc: doc.id,
        w: doc.content.width,
        h: doc.content.height,
        orig: tiles,
        orig_mask,
        source,
        buffer: StrokeBuffer::default(),
        spacer: Spacer::default(),
        selection: doc.content.selection.clone(),
        lock_alpha: node.locks.transparent,
        touched: HashSet::new(),
        spec,
    })
}

#[inline]
fn sel_at(sel: &Option<Mask>, x: i64, y: i64) -> f32 {
    match sel {
        None => 1.0,
        Some(m) => {
            let t = TILE as i64;
            m.tile(((x / t) as i32, (y / t) as i32)).map_or(m.outside, |tile| tile.get((x % t) as usize, (y % t) as usize)) as f32 / 255.0
        }
    }
}

impl Stroke {
    /// Adds pointer samples (x, y, pressure 0–1) and updates the touched tiles in the document.
    /// Returns the keys of tiles that changed.
    pub fn add(&mut self, doc: &mut Document, points: &[(f32, f32, f32)]) -> Vec<(i32, i32)> {
        let mut dabs = Vec::new();
        for &(x, y, p) in points {
            self.spacer.push(&self.spec.brush, x, y, p.clamp(0.0, 1.0), &mut dabs);
        }
        let mut changed: HashSet<(i32, i32)> = HashSet::new();
        let sel = self.selection.clone();
        for d in &dabs {
            for k in self.buffer.add(d, self.spec.brush.hardness, self.w, self.h, &|x, y| sel_at(&sel, x, y)) {
                changed.insert(k);
            }
        }
        let keys: Vec<(i32, i32)> = changed.into_iter().collect();
        self.render(doc, &keys);
        self.touched.extend(keys.iter().copied());
        keys
    }

    /// Recomputes the given tiles from the original pixels and the coverage buffer.
    fn render(&self, doc: &mut Document, keys: &[(i32, i32)]) {
        use rayon::prelude::*;
        let opacity = self.spec.brush.opacity;
        match self.spec.target {
            Target::Pixels => {
                let tiles: Vec<((i32, i32), Option<Arc<Tile>>)> = keys
                    .par_iter()
                    .map(|&k| {
                        let cov = &self.buffer.tiles[&k];
                        (k, self.paint_tile(k, cov, opacity))
                    })
                    .collect();
                if let Some(n) = doc.content.find_mut(self.spec.layer) {
                    if let NodeKind::Pixel { tiles: map } = &mut n.kind {
                        // a new map (never mutated in place): mip caches key on the map's identity
                        let mut m: TileMap = (**map).clone();
                        for (k, t) in tiles {
                            match t {
                                Some(t) => m.insert(k, t),
                                None => m.remove(&k),
                            };
                        }
                        *map = Arc::new(m);
                    }
                }
            }
            Target::Mask => {
                let gray = (0.299 * self.spec.color[0] as f32 + 0.587 * self.spec.color[1] as f32 + 0.114 * self.spec.color[2] as f32).round();
                let base = self.orig_mask.as_ref().unwrap();
                let tiles: Vec<((i32, i32), Arc<MaskTile>)> = keys
                    .par_iter()
                    .map(|&k| {
                        let cov = &self.buffer.tiles[&k];
                        let mut t = base.tile(k).cloned().unwrap_or_else(|| MaskTile::filled(base.outside));
                        let target = if self.spec.tool == PaintTool::Eraser { 255.0 - gray } else { gray };
                        for (v, c) in t.0.iter_mut().zip(cov.iter()) {
                            let a = c * opacity;
                            if a > 0.0 {
                                *v = (*v as f32 + (target - *v as f32) * a).round().clamp(0.0, 255.0) as u8;
                            }
                        }
                        (k, Arc::new(t))
                    })
                    .collect();
                if let Some(n) = doc.content.find_mut(self.spec.layer) {
                    if let Some(lm) = n.mask.as_mut() {
                        let mut map: MaskMap = (*lm.mask.tiles).clone();
                        for (k, t) in tiles {
                            map.insert(k, t);
                        }
                        lm.mask = Mask { tiles: Arc::new(map), outside: lm.mask.outside };
                    }
                }
            }
        }
    }

    fn source_px(&self, x: i64, y: i64) -> [f32; 4] {
        let Some(src) = &self.source else { return [0.0; 4] };
        let (dx, dy) = self.spec.offset.unwrap_or((0.0, 0.0));
        let (sx, sy) = ((x as f32 + dx).round() as i64, (y as f32 + dy).round() as i64);
        if sx < 0 || sy < 0 || sx >= self.w as i64 || sy >= self.h as i64 {
            return [0.0; 4];
        }
        let i = ((sy * self.w as i64 + sx) * 4) as usize;
        [src[i] as f32, src[i + 1] as f32, src[i + 2] as f32, src[i + 3] as f32]
    }

    /// One tile of the working layer for the current coverage.
    fn paint_tile(&self, k: (i32, i32), cov: &[f32], opacity: f32) -> Option<Arc<Tile>> {
        let orig = self.orig.get(&k);
        let mut out = orig.map(|t| (**t).clone()).unwrap_or_else(Tile::transparent);
        let (ox, oy) = (k.0 as i64 * TILE as i64, k.1 as i64 * TILE as i64);
        let c = self.spec.color;
        for i in 0..TILE_PX {
            let a = cov[i] * opacity;
            if a <= 0.0 {
                continue;
            }
            let px = &mut out.0[i * 4..i * 4 + 4];
            let (x, y) = (ox + (i % TILE) as i64, oy + (i / TILE) as i64);
            let src: [f32; 4] = match self.spec.tool {
                PaintTool::Brush => [c[0] as f32, c[1] as f32, c[2] as f32, 255.0],
                PaintTool::Clone | PaintTool::Heal => self.source_px(x, y),
                // spot healing previews as a dark translucent stroke until the fill is computed
                PaintTool::SpotHeal => [0.0, 0.0, 0.0, 90.0],
                PaintTool::Eraser if self.lock_alpha => [c[0] as f32, c[1] as f32, c[2] as f32, 255.0],
                PaintTool::Eraser => {
                    px[3] = (px[3] as f32 * (1.0 - a)).round() as u8;
                    continue;
                }
            };
            let sa = src[3] / 255.0 * a;
            if sa <= 0.0 {
                continue;
            }
            let da = px[3] as f32 / 255.0;
            if self.lock_alpha {
                // color only where pixels already exist, alpha unchanged
                for ch in 0..3 {
                    px[ch] = (px[ch] as f32 + (src[ch] - px[ch] as f32) * sa).round() as u8;
                }
                continue;
            }
            let oa = sa + da * (1.0 - sa);
            for ch in 0..3 {
                let v = (src[ch] * sa + px[ch] as f32 * da * (1.0 - sa)) / oa;
                px[ch] = v.round().clamp(0.0, 255.0) as u8;
            }
            px[3] = (oa * 255.0).round() as u8;
        }
        (!out.is_transparent()).then(|| Arc::new(out))
    }

    /// Finishes the stroke: healing tools compute their result now; then one history step.
    pub fn end(mut self, doc: &mut Document) -> Vec<(i32, i32)> {
        if matches!(self.spec.tool, PaintTool::Heal | PaintTool::SpotHeal) && !self.touched.is_empty() {
            self.finish_heal(doc);
        }
        let keys: Vec<(i32, i32)> = self.touched.iter().copied().collect();
        if !keys.is_empty() {
            doc.commit(Label::new(label_for(self.spec.tool)));
        }
        keys
    }

    fn finish_heal(&mut self, doc: &mut Document) {
        // work on the bounding box of the stroke plus a margin
        let t = TILE as i64;
        let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for (k, cov) in &self.buffer.tiles {
            for (i, c) in cov.iter().enumerate() {
                if *c > 0.0 {
                    let (x, y) = (k.0 as i64 * t + (i % TILE) as i64, k.1 as i64 * t + (i / TILE) as i64);
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x1 <= x0 {
            return;
        }
        let margin = (((x1 - x0).max(y1 - y0)) as f32 * if self.spec.tool == PaintTool::SpotHeal { 1.5 } else { 0.25 }) as i64 + 8;
        let (bx0, by0) = ((x0 - margin).max(0), (y0 - margin).max(0));
        let (bx1, by1) = ((x1 + margin).min(self.w as i64), (y1 + margin).min(self.h as i64));
        let (bw, bh) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
        let orig_px = |x: i64, y: i64| -> [f32; 4] {
            match self.orig.get(&((x / t) as i32, (y / t) as i32)) {
                Some(tile) => {
                    let p = tile.px((x % t) as usize, (y % t) as usize);
                    [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
                }
                None => [0.0; 4],
            }
        };
        let cov_at = |x: i64, y: i64| -> f32 { self.buffer.tiles.get(&((x / t) as i32, (y / t) as i32)).map_or(0.0, |c| c[((y % t) * t + x % t) as usize]) };
        let mut dest = vec![0f32; bw * bh * 4];
        let mut region = vec![0f32; bw * bh];
        for yy in 0..bh {
            for xx in 0..bw {
                let (x, y) = (bx0 + xx as i64, by0 + yy as i64);
                dest[(yy * bw + xx) * 4..][..4].copy_from_slice(&orig_px(x, y));
                region[yy * bw + xx] = (cov_at(x, y) * self.spec.brush.opacity).min(1.0);
            }
        }
        let src: Vec<f32> = if self.spec.tool == PaintTool::SpotHeal {
            // fill the spot from its surroundings (PatchMatch), then blend it in
            let rgb: Vec<[f32; 3]> = (0..bw * bh).map(|i| [dest[i * 4], dest[i * 4 + 1], dest[i * 4 + 2]]).collect();
            let hole: Vec<bool> = region.iter().map(|r| *r > 0.02).collect();
            let filled = lawha_core::inpaint::inpaint(bw, bh, &rgb, &hole, doc.change_counter);
            (0..bw * bh).flat_map(|i| [filled[i][0], filled[i][1], filled[i][2], dest[i * 4 + 3]]).collect()
        } else {
            let mut s = vec![0f32; bw * bh * 4];
            for yy in 0..bh {
                for xx in 0..bw {
                    s[(yy * bw + xx) * 4..][..4].copy_from_slice(&self.source_px(bx0 + xx as i64, by0 + yy as i64));
                }
            }
            s
        };
        let healed = lawha_core::heal::heal(bw, bh, &dest, &src, &region);
        // write back into the layer
        let mut map: TileMap = (*self.orig).clone();
        for k in self.buffer.tiles.keys() {
            let mut tile = self.orig.get(k).map(|t| (**t).clone()).unwrap_or_else(Tile::transparent);
            let (ox, oy) = (k.0 as i64 * t, k.1 as i64 * t);
            for y in 0..TILE as i64 {
                for x in 0..TILE as i64 {
                    let (gx, gy) = (ox + x, oy + y);
                    if gx < bx0 || gy < by0 || gx >= bx1 || gy >= by1 {
                        continue;
                    }
                    let j = ((gy - by0) as usize * bw + (gx - bx0) as usize) * 4;
                    let i = ((y * t + x) * 4) as usize;
                    for c in 0..4 {
                        tile.0[i + c] = healed[j + c].round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            if tile.is_transparent() {
                map.remove(k);
            } else {
                map.insert(*k, Arc::new(tile));
            }
        }
        if let Some(n) = doc.content.find_mut(self.spec.layer) {
            n.kind = NodeKind::Pixel { tiles: Arc::new(map) };
        }
    }
}

// ---- one-shot fills ----

fn layer_for_fill(doc: &Document, id: u64) -> Result<(Arc<TileMap>, bool), String> {
    let n = doc.content.find(id).ok_or("layer not found")?;
    let NodeKind::Pixel { tiles } = &n.kind else { return Err("choose a pixel layer".into()) };
    if n.locks.pixels {
        return Err("the layer's pixels are locked".into());
    }
    Ok((tiles.clone(), n.locks.transparent))
}

/// Lays `color` over the layer with per-pixel alpha `alpha(x, y)` (already including opacity).
fn fill_with(doc: &mut Document, id: u64, alpha: &(dyn Fn(i64, i64) -> f32 + Sync), color: &(dyn Fn(i64, i64) -> [f32; 4] + Sync), label: &str) -> Result<(), String> {
    use rayon::prelude::*;
    let (tiles, lock_alpha) = layer_for_fill(doc, id)?;
    let (tw, th) = doc.content.tiles_wide();
    let t = TILE as i64;
    let map: TileMap = (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|k| {
            let orig = tiles.get(&k);
            let mut out = orig.map(|x| (**x).clone()).unwrap_or_else(Tile::transparent);
            let (ox, oy) = (k.0 as i64 * t, k.1 as i64 * t);
            let mut any = false;
            for i in 0..TILE_PX {
                let (x, y) = (ox + (i % TILE) as i64, oy + (i / TILE) as i64);
                let a = alpha(x, y);
                if a <= 0.0 {
                    continue;
                }
                any = true;
                let c = color(x, y);
                let px = &mut out.0[i * 4..i * 4 + 4];
                let sa = a * c[3] / 255.0;
                let da = px[3] as f32 / 255.0;
                if lock_alpha {
                    for ch in 0..3 {
                        px[ch] = (px[ch] as f32 + (c[ch] - px[ch] as f32) * sa).round() as u8;
                    }
                    continue;
                }
                let oa = sa + da * (1.0 - sa);
                if oa <= 0.0 {
                    continue;
                }
                for ch in 0..3 {
                    px[ch] = ((c[ch] * sa + px[ch] as f32 * da * (1.0 - sa)) / oa).round().clamp(0.0, 255.0) as u8;
                }
                px[3] = (oa * 255.0).round() as u8;
            }
            if !any {
                return orig.map(|o| (k, o.clone()));
            }
            (!out.is_transparent()).then(|| (k, Arc::new(out)))
        })
        .collect();
    if let Some(n) = doc.content.find_mut(id) {
        n.kind = NodeKind::Pixel { tiles: Arc::new(map) };
    }
    doc.commit(Label::new(label));
    Ok(())
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BucketSpec {
    pub layer: u64,
    pub x: f64,
    pub y: f64,
    pub tolerance: u8,
    pub contiguous: bool,
    pub all_layers: bool,
    pub antialias: bool,
    pub color: [u8; 3],
    pub opacity: f32,
}

pub fn bucket(doc: &mut Document, s: BucketSpec) -> Result<(), String> {
    let (w, h) = (doc.content.width, doc.content.height);
    if s.x < 0.0 || s.y < 0.0 || s.x >= w as f64 || s.y >= h as f64 {
        return Ok(());
    }
    let img = sample(doc, s.all_layers);
    let region = lawha_core::select::wand(w, h, &img, (s.x as u32, s.y as u32), s.tolerance, s.contiguous, s.antialias);
    let sel = doc.content.selection.clone();
    let col = [s.color[0] as f32, s.color[1] as f32, s.color[2] as f32, 255.0];
    let wu = w as i64;
    fill_with(doc, s.layer, &|x, y| region.data[(y * wu + x) as usize] as f32 / 255.0 * sel_at(&sel, x, y) * s.opacity, &|_, _| col, "history.bucket")
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GradientSpec {
    pub layer: u64,
    pub kind: GradientKind,
    pub a: (f32, f32),
    pub b: (f32, f32),
    pub from: [u8; 4],
    pub to: [u8; 4],
    pub opacity: f32,
    pub reverse: bool,
    pub dither: bool,
}

pub fn gradient(doc: &mut Document, s: GradientSpec) -> Result<(), String> {
    let sel = doc.content.selection.clone();
    let (from, to) = if s.reverse { (s.to, s.from) } else { (s.from, s.to) };
    let color = move |x: i64, y: i64| -> [f32; 4] {
        let tt = gradient_t(s.kind, x as f32 + 0.5, y as f32 + 0.5, s.a, s.b);
        let d = if s.dither { dither(x, y) } else { 0.0 };
        let mut c = [0f32; 4];
        for i in 0..4 {
            c[i] = (from[i] as f32 + (to[i] as f32 - from[i] as f32) * tt + d).clamp(0.0, 255.0);
        }
        c
    };
    let op = s.opacity;
    fill_with(doc, s.layer, &|x, y| sel_at(&sel, x, y) * op, &color, "history.gradient")
}

/// Average color in a square around a point: from the layer, or everything visible.
pub fn sample_color(doc: &Document, x: f64, y: f64, size: u32, all_layers: bool) -> Option<[u8; 4]> {
    let (w, h) = (doc.content.width as i64, doc.content.height as i64);
    let (cx, cy) = (x.floor() as i64, y.floor() as i64);
    if cx < 0 || cy < 0 || cx >= w || cy >= h {
        return None;
    }
    let img = sample(doc, all_layers);
    let r = (size.max(1) as i64 - 1) / 2;
    let mut acc = [0f64; 4];
    let mut n = 0f64;
    for yy in (cy - r).max(0)..=(cy + r).min(h - 1) {
        for xx in (cx - r).max(0)..=(cx + r).min(w - 1) {
            let i = ((yy * w + xx) * 4) as usize;
            let a = img[i + 3] as f64;
            for c in 0..3 {
                acc[c] += img[i + c] as f64 * a;
            }
            acc[3] += a;
            n += 1.0;
        }
    }
    if acc[3] <= 0.0 {
        return Some([0, 0, 0, 0]);
    }
    Some([(acc[0] / acc[3]).round() as u8, (acc[1] / acc[3]).round() as u8, (acc[2] / acc[3]).round() as u8, (acc[3] / n).round() as u8])
}

/// Live strokes in progress, by id.
#[derive(Default)]
pub struct Strokes {
    pub map: parking_lot::Mutex<HashMap<u64, Stroke>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::model::Node;
    use crate::doc::{filled_tiles, Content};

    fn doc() -> Document {
        let bg = Node::pixel("bg", filled_tiles(512, 512, [255, 255, 255, 255]));
        let top = Node::pixel("top", TileMap::new());
        let active = top.id;
        Document::new("t".into(), None, Content::new(512, 512, 72.0, active, vec![bg, top]), Label::new("history.new"))
    }

    fn spec(layer: u64, tool: PaintTool) -> StrokeSpec {
        StrokeSpec {
            tool,
            target: Target::Pixels,
            layer,
            color: [255, 0, 0],
            brush: Brush { diameter: 40.0, hardness: 1.0, spacing: 0.1, flow: 1.0, opacity: 1.0, pressure_size: false, pressure_opacity: false, min_size: 0.0 },
            offset: None,
            sample_all: false,
        }
    }

    #[test]
    fn brush_stroke_paints_and_is_one_undo_step() {
        let mut d = doc();
        let id = d.content.active;
        let steps = d.history.states.len();
        let mut s = begin(&d, spec(id, PaintTool::Brush)).unwrap();
        s.add(&mut d, &[(100.0, 100.0, 1.0), (300.0, 100.0, 1.0)]);
        s.add(&mut d, &[(300.0, 300.0, 1.0)]);
        s.end(&mut d);
        assert_eq!(d.history.states.len(), steps + 1);
        let t = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t.px(200, 100), [255, 0, 0, 255]);
        assert_eq!(t.px(200, 200), [255, 255, 255, 255]);
        d.undo();
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(200, 100), [255, 255, 255, 255]);
    }

    #[test]
    fn opacity_is_a_ceiling_within_one_stroke() {
        let mut d = doc();
        let id = d.content.active;
        let mut sp = spec(id, PaintTool::Brush);
        sp.brush.opacity = 0.5;
        let mut s = begin(&d, sp).unwrap();
        // scribble over the same spot many times
        for _ in 0..10 {
            s.add(&mut d, &[(100.0, 100.0, 1.0), (140.0, 100.0, 1.0), (100.0, 100.0, 1.0)]);
        }
        s.end(&mut d);
        let p = composite_lod0(&d.content, 0, 0).unwrap().px(120, 100);
        // half-transparent red over white, whatever the number of passes (±1 for 8-bit rounding)
        assert_eq!(p[0], 255);
        assert!((p[1] as i32 - 128).abs() <= 1 && p[1] == p[2], "{p:?}");
    }

    #[test]
    fn selection_limits_painting_and_eraser_clears() {
        let mut d = doc();
        let id = d.content.active;
        crate::doc::selection::shape(&mut d, crate::doc::selection::Shape::Rect { x0: 0.0, y0: 0.0, x1: 200.0, y1: 512.0 }, lawha_core::select::Combine::Replace, 0.0).unwrap();
        let mut s = begin(&d, spec(id, PaintTool::Brush)).unwrap();
        s.add(&mut d, &[(50.0, 100.0, 1.0), (400.0, 100.0, 1.0)]);
        s.end(&mut d);
        let t = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t.px(150, 100), [255, 0, 0, 255]);
        assert_eq!(composite_lod0(&d.content, 1, 0).unwrap().px(44, 100), [255, 255, 255, 255]);
        crate::doc::selection::deselect(&mut d).unwrap();
        let mut e = begin(&d, spec(id, PaintTool::Eraser)).unwrap();
        e.add(&mut d, &[(50.0, 100.0, 1.0), (180.0, 100.0, 1.0)]);
        e.end(&mut d);
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(100, 100), [255, 255, 255, 255]);
    }

    #[test]
    fn locked_pixels_refuse_and_locked_transparency_keeps_alpha() {
        let mut d = doc();
        let id = d.content.active;
        d.content.find_mut(id).unwrap().locks.pixels = true;
        assert!(begin(&d, spec(id, PaintTool::Brush)).is_err());
        d.content.find_mut(id).unwrap().locks.pixels = false;
        d.content.find_mut(id).unwrap().locks.transparent = true;
        let mut s = begin(&d, spec(id, PaintTool::Brush)).unwrap();
        s.add(&mut d, &[(100.0, 100.0, 1.0)]);
        s.end(&mut d);
        // the empty layer stays empty
        assert!(d.content.find(id).unwrap().tiles().unwrap().is_empty());
    }

    #[test]
    fn clone_copies_from_the_offset() {
        let mut d = doc();
        let bg = d.content.root[0].id;
        // paint a red dot on the background, then clone it 200 px to the right
        let mut s = begin(&d, spec(bg, PaintTool::Brush)).unwrap();
        s.add(&mut d, &[(100.0, 100.0, 1.0)]);
        s.end(&mut d);
        let mut sp = spec(bg, PaintTool::Clone);
        sp.offset = Some((-200.0, 0.0));
        let mut c = begin(&d, sp).unwrap();
        c.add(&mut d, &[(300.0, 100.0, 1.0)]);
        c.end(&mut d);
        assert_eq!(composite_lod0(&d.content, 1, 0).unwrap().px(300 - 256, 100), [255, 0, 0, 255]);
    }

    #[test]
    fn spot_heal_removes_a_blemish() {
        let mut d = doc();
        let bg = d.content.root[0].id;
        // a gray background with a black blemish
        d.content.find_mut(bg).unwrap().kind = NodeKind::Pixel { tiles: Arc::new(filled_tiles(512, 512, [150, 150, 150, 255])) };
        let mut s = begin(&d, StrokeSpec { color: [0, 0, 0], ..spec(bg, PaintTool::Brush) }).unwrap();
        s.add(&mut d, &[(200.0, 200.0, 1.0)]);
        s.end(&mut d);
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(200, 200)[0], 0);
        let mut sp = spec(bg, PaintTool::SpotHeal);
        sp.brush.diameter = 60.0;
        let mut h = begin(&d, sp).unwrap();
        h.add(&mut d, &[(200.0, 200.0, 1.0)]);
        h.end(&mut d);
        let p = composite_lod0(&d.content, 0, 0).unwrap().px(200, 200);
        assert!((p[0] as i32 - 150).abs() < 12, "{p:?}");
    }

    #[test]
    fn painting_on_a_mask_hides_and_the_eraser_reveals() {
        let mut d = doc();
        let id = d.content.active;
        d.content.find_mut(id).unwrap().kind = NodeKind::Pixel { tiles: Arc::new(filled_tiles(512, 512, [255, 0, 0, 255])) };
        crate::doc::selection::mask_add(&mut d, id, crate::doc::selection::NewMask::RevealAll).unwrap();
        let mut sp = spec(id, PaintTool::Brush);
        sp.target = Target::Mask;
        sp.color = [0, 0, 0];
        let mut s = begin(&d, sp.clone()).unwrap();
        s.add(&mut d, &[(100.0, 100.0, 1.0), (300.0, 100.0, 1.0)]);
        s.end(&mut d);
        // black on the mask hides the red layer, the layer's pixels are untouched
        let t = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t.px(200, 100), [255, 255, 255, 255]);
        assert_eq!(t.px(200, 200), [255, 0, 0, 255]);
        assert!(d.content.find(id).unwrap().tiles().unwrap().get(&(0, 0)).unwrap().px(200, 100) == [255, 0, 0, 255]);
        // the eraser paints the opposite of the color on a mask: it reveals again
        sp.tool = PaintTool::Eraser;
        let mut s = begin(&d, sp).unwrap();
        s.add(&mut d, &[(150.0, 100.0, 1.0), (250.0, 100.0, 1.0)]);
        s.end(&mut d);
        let t = composite_lod0(&d.content, 0, 0).unwrap();
        assert_eq!(t.px(200, 100), [255, 0, 0, 255]);
        assert_eq!(t.px(110, 100), [255, 255, 255, 255]);
        d.undo();
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(200, 100), [255, 255, 255, 255]);
    }

    #[test]
    fn bucket_and_gradient_and_eyedropper() {
        let mut d = doc();
        let id = d.content.active;
        // bucket on the empty layer, sampling all layers: fills the white canvas
        bucket(&mut d, BucketSpec { layer: id, x: 10.0, y: 10.0, tolerance: 10, contiguous: true, all_layers: true, antialias: false, color: [0, 0, 255], opacity: 1.0 }).unwrap();
        assert_eq!(sample_color(&d, 300.0, 300.0, 1, true), Some([0, 0, 255, 255]));
        gradient(&mut d, GradientSpec { layer: id, kind: GradientKind::Linear, a: (0.0, 0.0), b: (512.0, 0.0), from: [0, 0, 0, 255], to: [255, 255, 255, 255], opacity: 1.0, reverse: false, dither: false }).unwrap();
        let mid = sample_color(&d, 256.0, 10.0, 1, true).unwrap();
        assert!((mid[0] as i32 - 128).abs() <= 1, "{mid:?}");
        let avg = sample_color(&d, 256.0, 10.0, 11, true).unwrap();
        assert!((avg[0] as i32 - 128).abs() <= 2);
    }
}
