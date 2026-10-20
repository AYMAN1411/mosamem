//! Filters on a layer (or its mask): applied tile by tile with a margin of real neighbours, or over
//! the whole canvas when a filter reaches too far for that; and the same per-tile path at any mip
//! level for the live preview.

use std::sync::Arc;

use lawha_core::filter::{self, Filter};
use lawha_core::mask::{Mask, MaskMap, MaskTile};
use lawha_core::tile::{Tile, TILE};
use rayon::prelude::*;
use serde::Deserialize;

use super::composite::{layer_tile, mask_tile};
use super::model::{LayerMask, NodeKind, TileMap};
use super::ops::OpResult;
use super::{Document, Label};

/// Previews compute each tile with a margin of the filter's reach up to this many px (at the level
/// shown); beyond it they use a coarser level.
pub const TILE_REACH: u32 = 256;
/// Applying at full resolution goes tile by tile (bounded memory) only for short reaches; past this
/// the margins cost more than filtering the whole canvas at once (measured: σ = 50 took 1.3 s by
/// tiles, 0.34 s at once for σ = 250 on 24 MP).
pub const APPLY_TILE_REACH: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterTarget {
    Pixels,
    Mask,
}

/// Straight RGBA8 of a rectangle at a mip level, read from the layer's tiles (transparent where
/// there are none).
fn read_rect(tiles: &Arc<TileMap>, lod: u8, x0: i64, y0: i64, w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let t = TILE as i64;
    let (tx0, ty0, tx1, ty1) = (x0.div_euclid(t), y0.div_euclid(t), (x0 + w as i64 - 1).div_euclid(t), (y0 + h as i64 - 1).div_euclid(t));
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let Some(tile) = layer_tile(tiles, lod, (tx as i32, ty as i32)) else { continue };
            let (ax, ay) = ((tx * t).max(x0), (ty * t).max(y0));
            let (bx, by) = ((tx * t + t).min(x0 + w as i64), (ty * t + t).min(y0 + h as i64));
            for y in ay..by {
                let src = (((y - ty * t) * t + (ax - tx * t)) * 4) as usize;
                let dst = (((y - y0) as usize * w) + (ax - x0) as usize) * 4;
                let n = ((bx - ax) * 4) as usize;
                out[dst..dst + n].copy_from_slice(&tile.0[src..src + n]);
            }
        }
    }
    out
}

/// The canvas size at a mip level.
fn canvas_at(width: u32, height: u32, lod: u8) -> (i64, i64) {
    (((width as i64) + (1 << lod) - 1) >> lod, ((height as i64) + (1 << lod) - 1) >> lod)
}

/// One filtered tile at a mip level (the filter already scaled to that level). Pixels outside the
/// canvas are left as they were.
pub fn filter_tile(tiles: &Arc<TileMap>, lod: u8, key: (i32, i32), f: &Filter, width: u32, height: u32) -> Option<Tile> {
    let (cw, ch) = canvas_at(width, height, lod);
    let t = TILE as i64;
    let (tx, ty) = (key.0 as i64 * t, key.1 as i64 * t);
    if tx >= cw || ty >= ch || tx + t <= 0 || ty + t <= 0 {
        return layer_tile(tiles, lod, key).map(|t| (*t).clone());
    }
    let r = f.reach() as i64;
    let (x0, y0) = ((tx - r).max(0), (ty - r).max(0));
    let (x1, y1) = ((tx + t + r).min(cw), (ty + t + r).min(ch));
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let src = read_rect(tiles, lod, x0, y0, w, h);
    let out = filter::apply(w, h, &src, f, (x0, y0));
    let mut tile = layer_tile(tiles, lod, key).map(|t| (*t).clone()).unwrap_or_else(Tile::transparent);
    let (ax, ay, bx, by) = (tx.max(0), ty.max(0), (tx + t).min(cw), (ty + t).min(ch));
    for y in ay..by {
        let s = (((y - y0) as usize * w) + (ax - x0) as usize) * 4;
        let d = ((((y - ty) * t) + (ax - tx)) * 4) as usize;
        let n = ((bx - ax) * 4) as usize;
        tile.0[d..d + n].copy_from_slice(&out[s..s + n]);
    }
    Some(tile)
}

/// Mixes the filtered tile into the original by the selection (soft edges included); with
/// `keep_alpha`, the original alpha stays (locked transparency).
fn mix(orig: Option<&Tile>, filtered: &Tile, sel: Option<&MaskTile>, sel_outside: u8, keep_alpha: bool) -> Tile {
    let mut out = filtered.clone();
    let blank = Tile::transparent();
    let o = orig.unwrap_or(&blank);
    for (i, (d, s)) in out.0.chunks_exact_mut(4).zip(o.0.chunks_exact(4)).enumerate() {
        let k = sel.map_or(sel_outside, |m| m.0[i]) as f32 / 255.0;
        if k < 1.0 {
            // premultiplied mix so colors of transparent pixels don't bleed
            let (fa, oa) = (d[3] as f32 * k, s[3] as f32 * (1.0 - k));
            let a = fa + oa;
            for c in 0..3 {
                d[c] = if a > 0.0 { ((d[c] as f32 * fa + s[c] as f32 * oa) / a + 0.5) as u8 } else { 0 };
            }
            d[3] = (a + 0.5).min(255.0) as u8;
        }
        if keep_alpha {
            d[3] = s[3];
        }
    }
    out
}

/// The mask as a gray opaque image, so the same filters run on it.
fn mask_as_tiles(m: &Mask, width: u32, height: u32) -> TileMap {
    let (tw, th) = (width.div_ceil(TILE as u32) as i32, height.div_ceil(TILE as u32) as i32);
    (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .map(|k| {
            let mut t = Tile::transparent();
            for (i, p) in t.0.chunks_exact_mut(4).enumerate() {
                let v = m.tile(k).map_or(m.outside, |mt| mt.0[i]);
                p.copy_from_slice(&[v, v, v, 255]);
            }
            (k, Arc::new(t))
        })
        .collect()
}

/// Every tile key covering the canvas.
fn canvas_keys(width: u32, height: u32) -> Vec<(i32, i32)> {
    let (tw, th) = (width.div_ceil(TILE as u32) as i32, height.div_ceil(TILE as u32) as i32);
    (0..th).flat_map(|ty| (0..tw).map(move |tx| (tx, ty))).collect()
}

/// Filtered tiles of the whole canvas at full resolution.
fn filter_all(tiles: &Arc<TileMap>, f: &Filter, width: u32, height: u32) -> Vec<((i32, i32), Tile)> {
    let keys = canvas_keys(width, height);
    if f.reach() <= APPLY_TILE_REACH {
        return keys.into_par_iter().filter_map(|k| filter_tile(tiles, 0, k, f, width, height).map(|t| (k, t))).collect();
    }
    // a long reach: once over the whole canvas
    let (w, h) = (width as usize, height as usize);
    let src = read_rect(tiles, 0, 0, 0, w, h);
    let out = filter::apply(w, h, &src, f, (0, 0));
    keys.into_par_iter()
        .map(|(tx, ty)| {
            let mut tile = layer_tile(tiles, 0, (tx, ty)).map(|t| (*t).clone()).unwrap_or_else(Tile::transparent);
            let (x0, y0) = (tx as usize * TILE, ty as usize * TILE);
            for y in y0..(y0 + TILE).min(h) {
                let n = ((x0 + TILE).min(w) - x0) * 4;
                let s = (y * w + x0) * 4;
                let d = (y - y0) * TILE * 4;
                tile.0[d..d + n].copy_from_slice(&out[s..s + n]);
            }
            ((tx, ty), tile)
        })
        .collect()
}

/// Applies a filter to the active layer's pixels or mask, through the selection. One history step.
pub fn apply(doc: &mut Document, id: u64, f: &Filter, target: FilterTarget, name: String) -> OpResult {
    let (width, height) = (doc.content.width, doc.content.height);
    let selection = doc.content.selection.clone();
    let node = doc.content.find(id).ok_or("layer not found")?;
    match target {
        FilterTarget::Pixels => {
            let NodeKind::Pixel { tiles } = &node.kind else { return Err("filters apply to pixel layers".into()) };
            if node.locks.pixels {
                return Err("the layer's pixels are locked".into());
            }
            let keep_alpha = node.locks.transparent;
            let tiles = tiles.clone();
            let filtered = filter_all(&tiles, f, width, height);
            let mut map: TileMap = (*tiles).clone();
            let results: Vec<((i32, i32), Option<Arc<Tile>>)> = filtered
                .into_par_iter()
                .map(|(k, t)| {
                    let sel_tile = selection.as_ref().and_then(|s| s.tile(k).cloned());
                    let outside = selection.as_ref().map_or(255, |s| s.outside);
                    let out = mix(tiles.get(&k).map(|t| &**t), &t, sel_tile.as_ref(), outside, keep_alpha);
                    (k, if out.is_transparent() { None } else { Some(Arc::new(out)) })
                })
                .collect();
            for (k, t) in results {
                match t {
                    Some(t) => map.insert(k, t),
                    None => map.remove(&k),
                };
            }
            let n = doc.content.find_mut(id).unwrap();
            n.kind = NodeKind::Pixel { tiles: Arc::new(map) };
        }
        FilterTarget::Mask => {
            let lm = node.mask.clone().ok_or("the layer has no mask")?;
            let gray = Arc::new(mask_as_tiles(&lm.mask, width, height));
            let filtered = filter_all(&gray, f, width, height);
            let mut map: MaskMap = (*lm.mask.tiles).clone();
            for (k, t) in filtered {
                let sel_tile = selection.as_ref().and_then(|s| s.tile(k).cloned());
                let outside = selection.as_ref().map_or(255, |s| s.outside);
                let out = mix(gray.get(&k).map(|t| &**t), &t, sel_tile.as_ref(), outside, false);
                let mut mt = MaskTile::filled(0);
                for (i, p) in out.0.chunks_exact(4).enumerate() {
                    mt.0[i] = p[0];
                }
                map.insert(k, Arc::new(mt));
            }
            let n = doc.content.find_mut(id).unwrap();
            n.mask = Some(LayerMask { mask: Mask { tiles: Arc::new(map), outside: lm.mask.outside }, ..lm });
        }
    }
    doc.commit(Label::with("history.filter", name));
    Ok(())
}

/// The live preview of a filter at a mip level: the layer's tile there, filtered and mixed in by
/// the selection. Filters reaching too far at this level are computed one level coarser and
/// enlarged (they are strong blurs, so nothing is lost to the eye).
pub struct Preview {
    pub filter: Filter,
    pub selection: Option<Mask>,
    pub keep_alpha: bool,
}

impl Preview {
    pub fn tile(&self, tiles: &Arc<TileMap>, lod: u8, key: (i32, i32), width: u32, height: u32, coarser: impl Fn(u8, (i32, i32)) -> Option<Arc<Tile>>) -> Option<Tile> {
        let f = self.filter.scaled(1.0 / (1u32 << lod) as f32);
        let filtered = if f.reach() > TILE_REACH && lod < super::composite::MAX_LOD {
            let parent = coarser(lod + 1, (key.0 >> 1, key.1 >> 1))?;
            enlarge_quadrant(&parent, (key.0 & 1) as usize, (key.1 & 1) as usize)
        } else {
            filter_tile(tiles, lod, key, &f, width, height)?
        };
        let orig = layer_tile(tiles, lod, key);
        let sel = self.selection.as_ref().and_then(|s| mask_tile(s, lod, key));
        let outside = self.selection.as_ref().map_or(255, |s| s.outside);
        Some(mix(orig.as_deref(), &filtered, sel.as_deref(), outside, self.keep_alpha))
    }
}

/// Bilinear 2× enlargement of one quadrant of a tile.
fn enlarge_quadrant(parent: &Tile, qx: usize, qy: usize) -> Tile {
    let mut out = Tile::transparent();
    let half = TILE / 2;
    for y in 0..TILE {
        let sy = ((qy * half) as f32 + (y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (TILE - 1) as f32);
        let (y0, fy) = (sy.floor() as usize, sy.fract());
        let y1 = (y0 + 1).min(TILE - 1);
        for x in 0..TILE {
            let sx = ((qx * half) as f32 + (x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (TILE - 1) as f32);
            let (x0, fx) = (sx.floor() as usize, sx.fract());
            let x1 = (x0 + 1).min(TILE - 1);
            let p = |xx: usize, yy: usize| {
                let i = (yy * TILE + xx) * 4;
                [parent.0[i] as f32, parent.0[i + 1] as f32, parent.0[i + 2] as f32, parent.0[i + 3] as f32]
            };
            let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
            let o = (y * TILE + x) * 4;
            // premultiplied interpolation
            let wts = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
            let al: f32 = [a[3], b[3], c[3], d[3]].iter().zip(&wts).map(|(v, w)| v * w).sum();
            if al > 0.0 {
                for k in 0..3 {
                    let v: f32 = [a, b, c, d].iter().zip(&wts).map(|(p, w)| p[k] * p[3] * w).sum::<f32>() / al;
                    out.0[o + k] = (v + 0.5).min(255.0) as u8;
                }
            }
            out.0[o + 3] = (al + 0.5).min(255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, Content, Node};

    fn doc_with(tiles: TileMap, w: u32, h: u32) -> Document {
        let n = Node::pixel("a", tiles);
        let id = n.id;
        Document::new("t".into(), None, Content::new(w, h, 72.0, id, vec![n]), Label::new("history.new"))
    }

    /// A 600×400 image with vertical stripes, so blurs and sharpening show.
    fn stripes() -> TileMap {
        let (w, h) = (600u32, 400u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| if (i % w) / 8 % 2 == 0 { [40, 80, 160, 255] } else { [230, 200, 60, 255] }).collect();
        crate::doc::tiles_from_rgba(w, h, &rgba)
    }

    #[test]
    fn filtering_tile_by_tile_matches_filtering_everything_at_once() {
        let tiles = Arc::new(stripes());
        for f in [Filter::GaussianBlur { radius: 6.0 }, Filter::MotionBlur { angle: 20.0, distance: 30.0 }, Filter::UnsharpMask { amount: 120.0, radius: 3.0, threshold: 0.0 }, Filter::Pixelate { cell: 12.0 }] {
            let per_tile = filter_all(&tiles, &f, 600, 400);
            let src = read_rect(&tiles, 0, 0, 0, 600, 400);
            let whole = filter::apply(600, 400, &src, &f, (0, 0));
            for ((tx, ty), t) in per_tile {
                for (i, p) in t.0.chunks_exact(4).enumerate() {
                    let (x, y) = (tx as usize * 256 + i % 256, ty as usize * 256 + i / 256);
                    if x >= 600 || y >= 400 {
                        continue;
                    }
                    let q = &whole[(y * 600 + x) * 4..][..4];
                    assert!((0..4).all(|k| (p[k] as i32 - q[k] as i32).abs() <= 1), "{f:?} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn a_filter_is_one_undo_step_and_follows_the_selection() {
        let mut d = doc_with(stripes(), 600, 400);
        let id = d.content.active;
        let before = composite_lod0(&d.content, 0, 0).unwrap();
        // select the left half
        let mut sel = lawha_core::mask::Dense::new(600, 400, 0);
        for y in 0..400 {
            for x in 0..300 {
                sel.data[y * 600 + x] = 255;
            }
        }
        d.content.selection = Some(Mask::from_dense(&sel, 0));
        apply(&mut d, id, &Filter::GaussianBlur { radius: 8.0 }, FilterTarget::Pixels, "blur".into()).unwrap();
        let after = composite_lod0(&d.content, 0, 0).unwrap();
        assert_ne!(after.px(100, 100), before.px(100, 100), "blurred inside");
        let right = composite_lod0(&d.content, 1, 0).unwrap();
        assert_eq!(right.px(100, 100), {
            let x = 256 + 100;
            if x / 8 % 2 == 0 { [40, 80, 160, 255] } else { [230, 200, 60, 255] }
        }, "untouched outside");
        d.undo();
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().0, before.0);
    }

    #[test]
    fn the_canvas_edge_does_not_fade_and_locks_are_respected() {
        let mut d = doc_with(filled_tiles(300, 200, [10, 120, 200, 255]), 300, 200);
        let id = d.content.active;
        apply(&mut d, id, &Filter::GaussianBlur { radius: 20.0 }, FilterTarget::Pixels, "blur".into()).unwrap();
        assert_eq!(composite_lod0(&d.content, 0, 0).unwrap().px(0, 0), [10, 120, 200, 255]);
        d.content.find_mut(id).unwrap().locks.pixels = true;
        assert!(apply(&mut d, id, &Filter::GaussianBlur { radius: 2.0 }, FilterTarget::Pixels, "blur".into()).is_err());
    }

    #[test]
    fn a_long_reach_filters_the_whole_canvas_and_masks_can_be_blurred() {
        let mut d = doc_with(stripes(), 600, 400);
        let id = d.content.active;
        // σ = 100: past the per-tile limit
        apply(&mut d, id, &Filter::GaussianBlur { radius: 100.0 }, FilterTarget::Pixels, "blur".into()).unwrap();
        let t = composite_lod0(&d.content, 1, 0).unwrap();
        // stripes are averaged away
        assert!((t.px(10, 10)[0] as i32 - 135).abs() <= 8, "{:?}", t.px(10, 10));
        // a hard-edged mask becomes soft
        let mut sel = lawha_core::mask::Dense::new(600, 400, 0);
        for y in 0..400 {
            for x in 0..300 {
                sel.data[y * 600 + x] = 255;
            }
        }
        d.content.find_mut(id).unwrap().mask = Some(LayerMask::new(Mask::from_dense(&sel, 0)));
        apply(&mut d, id, &Filter::GaussianBlur { radius: 10.0 }, FilterTarget::Mask, "blur".into()).unwrap();
        let m = d.content.find(id).unwrap().mask.as_ref().unwrap().mask.to_dense(600, 400);
        assert!(m.at(300, 200) > 60 && m.at(300, 200) < 200 && m.at(250, 200) == 255 && m.at(350, 200) == 0, "{}", m.at(300, 200));
    }

    #[test]
    fn a_layer_under_an_adjustment_is_previewed_through_the_whole_document() {
        use crate::doc::live::{Part, Session};
        let mut d = doc_with(stripes(), 600, 400);
        let id = d.content.active;
        crate::doc::adjustments::new_adjustment(&mut d, "inv".into(), lawha_core::adjust::Adjustment::Invert).unwrap();
        d.content.active = id;
        assert!(Session::begin(1, &d.content, id).is_none(), "the split can't hold an adjustment above");
        let s = Session::begin_full(1, &d.content, id).unwrap();
        let f = Filter::GaussianBlur { radius: 5.0 };
        s.set_filter(Some(Preview { filter: f.clone(), selection: None, keep_alpha: false }));
        let preview = s.tile(Part::Below, 0, 1, 0).unwrap();
        // the real thing
        apply(&mut d, id, &f, FilterTarget::Pixels, "blur".into()).unwrap();
        let want = composite_lod0(&d.content, 1, 0).unwrap();
        let worst = preview.0.iter().zip(want.0.iter()).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 1, "{worst}");
        // turning the preview off shows the document as it is
        s.set_filter(None);
        d.undo();
        assert_eq!(s.tile(Part::Below, 0, 1, 0).unwrap().0, composite_lod0(&d.content, 1, 0).unwrap().0);
    }

    #[test]
    fn previews_at_mip_levels_look_like_the_result() {
        let tiles = Arc::new(stripes());
        let p = Preview { filter: Filter::GaussianBlur { radius: 16.0 }, selection: None, keep_alpha: false };
        let full = filter_all(&tiles, &p.filter, 600, 400);
        // level 1: compare with the result scaled down
        let prev = p.tile(&tiles, 1, (0, 0), 600, 400, |_, _| None).unwrap();
        let full00 = &full.iter().find(|(k, _)| *k == (0, 0)).unwrap().1;
        for (x, y) in [(20, 20), (60, 100), (120, 40)] {
            let a = prev.px(x, y);
            let b = full00.px(2 * x, 2 * y);
            assert!((0..3).all(|k| (a[k] as i32 - b[k] as i32).abs() <= 12), "{a:?} vs {b:?}");
        }
    }
}
