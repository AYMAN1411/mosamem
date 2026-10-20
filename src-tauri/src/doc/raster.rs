//! Moving between sparse tiles and dense rectangles, and exact whole-pixel shifts, for layers and
//! masks. Coordinates are document pixels and may be negative (layers keep pixels outside the
//! canvas).

use std::sync::Arc;

use mosamem_core::mask::{Mask, MaskMap, MaskTile};
use mosamem_core::tile::{Tile, TILE};
use rayon::prelude::*;

use super::model::TileMap;

const T: i64 = TILE as i64;

/// Pixel bounds (x0, y0, x1, y1), end-exclusive, of the tiles in a map.
pub fn tiles_bbox(keys: impl Iterator<Item = (i32, i32)>) -> Option<(i64, i64, i64, i64)> {
    let mut b: Option<(i64, i64, i64, i64)> = None;
    for (tx, ty) in keys {
        let (x0, y0) = (tx as i64 * T, ty as i64 * T);
        b = Some(match b {
            None => (x0, y0, x0 + T, y0 + T),
            Some((a, bb, c, d)) => (a.min(x0), bb.min(y0), c.max(x0 + T), d.max(y0 + T)),
        });
    }
    b
}

#[cfg(test)]
pub fn map_bbox(map: &TileMap) -> Option<(i64, i64, i64, i64)> {
    tiles_bbox(map.keys().copied())
}

/// Bounds of the pixels with any alpha, end-exclusive.
pub fn content_bounds(map: &TileMap) -> Option<(i64, i64, i64, i64)> {
    let per_tile: Vec<(i64, i64, i64, i64)> = map
        .par_iter()
        .filter_map(|(&(tx, ty), t)| {
            let mut b: Option<(usize, usize, usize, usize)> = None;
            for y in 0..TILE {
                for x in 0..TILE {
                    if t.0[(y * TILE + x) * 4 + 3] > 0 {
                        b = Some(match b {
                            None => (x, y, x + 1, y + 1),
                            Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x + 1), e.max(y + 1)),
                        });
                    }
                }
            }
            b.map(|(a, c, d, e)| (tx as i64 * T + a as i64, ty as i64 * T + c as i64, tx as i64 * T + d as i64, ty as i64 * T + e as i64))
        })
        .collect();
    per_tile.into_iter().reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

/// Straight RGBA8 of a rectangle (transparent where there are no tiles).
pub fn read(map: &TileMap, x0: i64, y0: i64, w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    if w == 0 || h == 0 {
        return out;
    }
    let (tx0, tx1) = (x0.div_euclid(T), (x0 + w as i64 - 1).div_euclid(T));
    out.par_chunks_mut(w * 4).enumerate().for_each(|(row, dst)| {
        let y = y0 + row as i64;
        let ty = y.div_euclid(T);
        for tx in tx0..=tx1 {
            let Some(tile) = map.get(&(tx as i32, ty as i32)) else { continue };
            let (ax, bx) = ((tx * T).max(x0), (tx * T + T).min(x0 + w as i64));
            let s = (((y - ty * T) * T + (ax - tx * T)) * 4) as usize;
            let d = ((ax - x0) * 4) as usize;
            let n = ((bx - ax) * 4) as usize;
            dst[d..d + n].copy_from_slice(&tile.0[s..s + n]);
        }
    });
    out
}

/// Tiles of a dense rectangle (fully transparent tiles are left out).
/// `map` with a rectangle replaced by straight RGBA8 (pixels outside the rectangle are kept; tiles
/// that end up fully transparent are dropped).
pub fn paste(map: &TileMap, rgba: &[u8], x0: i64, y0: i64, w: usize, h: usize) -> TileMap {
    let mut out = map.clone();
    if w == 0 || h == 0 {
        return out;
    }
    let (tx0, ty0, tx1, ty1) = (x0.div_euclid(T), y0.div_euclid(T), (x0 + w as i64 - 1).div_euclid(T), (y0 + h as i64 - 1).div_euclid(T));
    let keys: Vec<(i64, i64)> = (ty0..=ty1).flat_map(|ty| (tx0..=tx1).map(move |tx| (tx, ty))).collect();
    let tiles: Vec<((i32, i32), Option<Arc<Tile>>)> = keys
        .into_par_iter()
        .map(|(tx, ty)| {
            let key = (tx as i32, ty as i32);
            let mut t = map.get(&key).map(|t| (**t).clone()).unwrap_or_else(Tile::transparent);
            let (ax, ay) = ((tx * T).max(x0), (ty * T).max(y0));
            let (bx, by) = ((tx * T + T).min(x0 + w as i64), (ty * T + T).min(y0 + h as i64));
            for y in ay..by {
                let d = (((y - ty * T) * T + (ax - tx * T)) * 4) as usize;
                let s = (((y - y0) as usize * w) + (ax - x0) as usize) * 4;
                let n = ((bx - ax) * 4) as usize;
                t.0[d..d + n].copy_from_slice(&rgba[s..s + n]);
            }
            (key, (!t.is_transparent()).then(|| Arc::new(t)))
        })
        .collect();
    for (k, t) in tiles {
        match t {
            Some(t) => {
                out.insert(k, t);
            }
            None => {
                out.remove(&k);
            }
        }
    }
    out
}

pub fn write(rgba: &[u8], x0: i64, y0: i64, w: usize, h: usize) -> TileMap {
    if w == 0 || h == 0 {
        return TileMap::new();
    }
    let (tx0, ty0, tx1, ty1) = (x0.div_euclid(T), y0.div_euclid(T), (x0 + w as i64 - 1).div_euclid(T), (y0 + h as i64 - 1).div_euclid(T));
    let keys: Vec<(i64, i64)> = (ty0..=ty1).flat_map(|ty| (tx0..=tx1).map(move |tx| (tx, ty))).collect();
    keys.into_par_iter()
        .filter_map(|(tx, ty)| {
            let mut t = Tile::transparent();
            let (ax, ay) = ((tx * T).max(x0), (ty * T).max(y0));
            let (bx, by) = ((tx * T + T).min(x0 + w as i64), (ty * T + T).min(y0 + h as i64));
            let mut any = false;
            for y in ay..by {
                let d = (((y - ty * T) * T + (ax - tx * T)) * 4) as usize;
                let s = (((y - y0) as usize * w) + (ax - x0) as usize) * 4;
                let n = ((bx - ax) * 4) as usize;
                let row = &rgba[s..s + n];
                any |= row.chunks_exact(4).any(|p| p[3] > 0);
                t.0[d..d + n].copy_from_slice(row);
            }
            any.then(|| ((tx as i32, ty as i32), Arc::new(t)))
        })
        .collect()
}

/// The same pixels, shifted by whole pixels: each new tile is copied from the (up to four) old
/// tiles under it.
pub fn translate(map: &TileMap, dx: i64, dy: i64) -> TileMap {
    if dx == 0 && dy == 0 {
        return map.clone();
    }
    if dx.rem_euclid(T) == 0 && dy.rem_euclid(T) == 0 {
        let (kx, ky) = ((dx / T) as i32, (dy / T) as i32);
        return map.iter().map(|(&(tx, ty), t)| ((tx + kx, ty + ky), t.clone())).collect();
    }
    let mut keys: Vec<(i64, i64)> = map
        .keys()
        .flat_map(|&(tx, ty)| {
            let (x, y) = (tx as i64 * T + dx, ty as i64 * T + dy);
            let (a, b) = (x.div_euclid(T), y.div_euclid(T));
            [(a, b), (a + 1, b), (a, b + 1), (a + 1, b + 1)]
        })
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys.into_par_iter()
        .filter_map(|(ox, oy)| {
            let mut t = Tile::transparent();
            let mut any = false;
            // source pixels for this tile start at (ox·T − dx, oy·T − dy)
            let (sx, sy) = (ox * T - dx, oy * T - dy);
            for ty in [sy.div_euclid(T), sy.div_euclid(T) + 1] {
                for tx in [sx.div_euclid(T), sx.div_euclid(T) + 1] {
                    let Some(src) = map.get(&(tx as i32, ty as i32)) else { continue };
                    // overlap in source coordinates
                    let (ax, bx) = ((tx * T).max(sx), (tx * T + T).min(sx + T));
                    let (ay, by) = ((ty * T).max(sy), (ty * T + T).min(sy + T));
                    if ax >= bx || ay >= by {
                        continue;
                    }
                    for y in ay..by {
                        let s = (((y - ty * T) * T + (ax - tx * T)) * 4) as usize;
                        let d = (((y - sy) * T + (ax - sx)) * 4) as usize;
                        let n = ((bx - ax) * 4) as usize;
                        t.0[d..d + n].copy_from_slice(&src.0[s..s + n]);
                    }
                    any = true;
                }
            }
            (any && !t.is_transparent()).then(|| ((ox as i32, oy as i32), Arc::new(t)))
        })
        .collect()
}

/// Mask values of a rectangle (the mask's outside value where it has no tiles).
pub fn mask_read(m: &Mask, x0: i64, y0: i64, w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![m.outside; w * h];
    if w == 0 || h == 0 {
        return out;
    }
    let (tx0, ty0, tx1, ty1) = (x0.div_euclid(T), y0.div_euclid(T), (x0 + w as i64 - 1).div_euclid(T), (y0 + h as i64 - 1).div_euclid(T));
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let Some(tile) = m.tiles.get(&(tx as i32, ty as i32)) else { continue };
            let (ax, ay) = ((tx * T).max(x0), (ty * T).max(y0));
            let (bx, by) = ((tx * T + T).min(x0 + w as i64), (ty * T + T).min(y0 + h as i64));
            for y in ay..by {
                let s = ((y - ty * T) * T + (ax - tx * T)) as usize;
                let d = (y - y0) as usize * w + (ax - x0) as usize;
                let n = (bx - ax) as usize;
                out[d..d + n].copy_from_slice(&tile.0[s..s + n]);
            }
        }
    }
    out
}

/// A mask from dense values over a rectangle, `outside` everywhere else.
pub fn mask_write(values: &[u8], x0: i64, y0: i64, w: usize, h: usize, outside: u8) -> Mask {
    if w == 0 || h == 0 {
        return Mask::uniform(outside);
    }
    let (tx0, ty0, tx1, ty1) = (x0.div_euclid(T), y0.div_euclid(T), (x0 + w as i64 - 1).div_euclid(T), (y0 + h as i64 - 1).div_euclid(T));
    let keys: Vec<(i64, i64)> = (ty0..=ty1).flat_map(|ty| (tx0..=tx1).map(move |tx| (tx, ty))).collect();
    let tiles: MaskMap = keys
        .into_par_iter()
        .filter_map(|(tx, ty)| {
            let mut t = MaskTile::filled(outside);
            let (ax, ay) = ((tx * T).max(x0), (ty * T).max(y0));
            let (bx, by) = ((tx * T + T).min(x0 + w as i64), (ty * T + T).min(y0 + h as i64));
            for y in ay..by {
                let d = ((y - ty * T) * T + (ax - tx * T)) as usize;
                let s = (y - y0) as usize * w + (ax - x0) as usize;
                let n = (bx - ax) as usize;
                t.0[d..d + n].copy_from_slice(&values[s..s + n]);
            }
            t.0.iter().any(|v| *v != outside).then(|| ((tx as i32, ty as i32), Arc::new(t)))
        })
        .collect();
    Mask { tiles: Arc::new(tiles), outside }
}

pub fn translate_mask(m: &Mask, dx: i64, dy: i64) -> Mask {
    if dx == 0 && dy == 0 {
        return m.clone();
    }
    let Some((x0, y0, x1, y1)) = tiles_bbox(m.tiles.keys().copied()) else { return m.clone() };
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    mask_write(&mask_read(m, x0, y0, w, h), x0 + dx, y0 + dy, w, h, m.outside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TileMap {
        let (w, h) = (300usize, 200usize);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 7) as u8, 9, if i % 13 == 0 { 0 } else { 255 }]).collect();
        write(&rgba, 0, 0, w, h)
    }

    #[test]
    fn read_write_round_trip_and_bounds() {
        let m = sample();
        assert_eq!(map_bbox(&m), Some((0, 0, 512, 256)));
        let back = read(&m, 0, 0, 300, 200);
        assert_eq!(read(&write(&back, -40, 70, 300, 200), -40, 70, 300, 200), back);
        assert_eq!(content_bounds(&m), Some((0, 0, 300, 200)));
    }

    #[test]
    fn translations_are_exact_in_any_direction() {
        let m = sample();
        let orig = read(&m, 0, 0, 300, 200);
        for (dx, dy) in [(1, 0), (-37, 15), (256, -512), (-300, -200)] {
            let t = translate(&m, dx, dy);
            assert_eq!(read(&t, dx, dy, 300, 200), orig, "{dx},{dy}");
            assert_eq!(content_bounds(&t), Some((dx, dy, dx + 300, dy + 200)));
        }
        let mask = mask_write(&vec![200u8; 50 * 40], 10, 20, 50, 40, 0);
        let moved = translate_mask(&mask, -5, 300);
        assert_eq!(mask_read(&moved, 5, 320, 50, 40), vec![200u8; 50 * 40]);
        assert_eq!(mask_read(&moved, 10, 20, 1, 1), vec![0]);
    }
}
