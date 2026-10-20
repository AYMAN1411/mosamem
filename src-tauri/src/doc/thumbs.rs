//! Small layer thumbnails for the Layers panel, cached by the layer's pixel data.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use mosamem_core::tile::TILE;
use parking_lot::Mutex;

use super::model::TileMap;

#[derive(Default)]
pub struct Cache {
    /// Keyed by the data's address; the weak handle keeps that address from being reused by other
    /// data while the entry exists, and tells when the data is gone.
    map: Mutex<HashMap<(usize, u32), (Weak<dyn Send + Sync>, Arc<Vec<u8>>)>>,
}

impl Cache {
    fn get(&self, key: &(usize, u32)) -> Option<Arc<Vec<u8>>> {
        self.map.lock().get(key).filter(|(w, _)| w.strong_count() > 0).map(|(_, t)| t.clone())
    }
    fn put(&self, key: (usize, u32), owner: Weak<dyn Send + Sync>, out: &Arc<Vec<u8>>) {
        let mut m = self.map.lock();
        if m.len() > 512 {
            m.retain(|_, (w, _)| w.strong_count() > 0);
        }
        m.insert(key, (owner, out.clone()));
    }
}

/// Thumbnail of a layer's pixels that fits in `size`×`size`, as `[w u16 LE][h u16 LE][RGBA…]`.
pub fn thumbnail(cache: &Cache, tiles: &Arc<TileMap>, width: u32, height: u32, size: u32) -> Arc<Vec<u8>> {
    let key = (Arc::as_ptr(tiles) as usize, size);
    if let Some(hit) = cache.get(&key) {
        return hit;
    }
    let scale = (width.max(height) as f64 / size as f64).max(1.0);
    let (tw, th) = (((width as f64 / scale).round() as u32).max(1), ((height as f64 / scale).round() as u32).max(1));
    // average every source pixel into its thumbnail cell (premultiplied)
    let mut sum = vec![[0f64; 4]; (tw * th) as usize];
    let mut count = vec![0u32; (tw * th) as usize];
    for (&(tx, ty), tile) in tiles.iter() {
        for y in 0..TILE {
            let dy = ty as usize * TILE + y;
            if dy >= height as usize {
                break;
            }
            let cy = ((dy as f64 / scale) as u32).min(th - 1);
            for x in 0..TILE {
                let dx = tx as usize * TILE + x;
                if dx >= width as usize {
                    break;
                }
                let cx = ((dx as f64 / scale) as u32).min(tw - 1);
                let p = tile.px(x, y);
                let a = p[3] as f64;
                let c = &mut sum[(cy * tw + cx) as usize];
                c[0] += p[0] as f64 * a;
                c[1] += p[1] as f64 * a;
                c[2] += p[2] as f64 * a;
                c[3] += a;
            }
        }
    }
    // cells never touched by a tile are transparent; count every pixel the cell covers
    for dy in 0..height {
        let cy = ((dy as f64 / scale) as u32).min(th - 1);
        let row = (cy * tw) as usize;
        let mut last = u32::MAX;
        let mut run = 0u32;
        for dx in 0..width {
            let cx = ((dx as f64 / scale) as u32).min(tw - 1);
            if cx != last {
                if last != u32::MAX {
                    count[row + last as usize] += run;
                }
                last = cx;
                run = 0;
            }
            run += 1;
        }
        count[row + last as usize] += run;
    }
    let mut out = Vec::with_capacity(4 + (tw * th * 4) as usize);
    out.extend_from_slice(&(tw as u16).to_le_bytes());
    out.extend_from_slice(&(th as u16).to_le_bytes());
    for (c, n) in sum.iter().zip(count.iter()) {
        let n = (*n).max(1) as f64;
        if c[3] <= 0.0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            out.extend_from_slice(&[(c[0] / c[3]).round() as u8, (c[1] / c[3]).round() as u8, (c[2] / c[3]).round() as u8, (c[3] / n).round() as u8]);
        }
    }
    let out = Arc::new(out);
    cache.put(key, Arc::downgrade(tiles) as Weak<dyn Send + Sync>, &out);
    out
}

/// Thumbnail of a layer mask (gray, opaque), same format as `thumbnail`.
pub fn mask_thumbnail(cache: &Cache, mask: &mosamem_core::mask::Mask, width: u32, height: u32, size: u32) -> Arc<Vec<u8>> {
    let key = (Arc::as_ptr(&mask.tiles) as usize ^ (mask.outside as usize) << 1 ^ 1, size);
    if let Some(hit) = cache.get(&key) {
        return hit;
    }
    let scale = (width.max(height) as f64 / size as f64).max(1.0);
    let (tw, th) = (((width as f64 / scale).round() as u32).max(1), ((height as f64 / scale).round() as u32).max(1));
    let mut out = Vec::with_capacity(4 + (tw * th * 4) as usize);
    out.extend_from_slice(&(tw as u16).to_le_bytes());
    out.extend_from_slice(&(th as u16).to_le_bytes());
    for cy in 0..th {
        for cx in 0..tw {
            // point-sample the cell center (masks are smooth enough for a thumbnail)
            let (x, y) = (((cx as f64 + 0.5) * scale) as i32, ((cy as f64 + 0.5) * scale) as i32);
            let v = mask.tile((x / TILE as i32, y / TILE as i32)).map_or(mask.outside, |t| t.get(x as usize % TILE, y as usize % TILE));
            out.extend_from_slice(&[v, v, v, 255]);
        }
    }
    let out = Arc::new(out);
    cache.put(key, Arc::downgrade(&mask.tiles) as Weak<dyn Send + Sync>, &out);
    out
}
