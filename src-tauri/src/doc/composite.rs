//! Flattened tiles of the whole stack, per mip level, computed on demand and cached until the
//! document changes.

use std::collections::HashMap;
use std::sync::Arc;

use lawha_core::tile::{downsample, Accum, Tile, TILE};
use parking_lot::Mutex;

use super::{tiles_for, DocHandle, Document};

pub const MAX_LOD: u8 = 6;

#[derive(Default)]
pub struct Cache {
    inner: Mutex<CacheInner>,
}

#[derive(Default)]
struct CacheInner {
    version: u64,
    tiles: HashMap<(u8, i32, i32), Option<Arc<Tile>>>,
}

impl Cache {
    fn get(&self, version: u64, key: (u8, i32, i32)) -> Option<Option<Arc<Tile>>> {
        let mut c = self.inner.lock();
        if c.version != version {
            c.version = version;
            c.tiles.clear();
            return None;
        }
        c.tiles.get(&key).cloned()
    }

    fn put(&self, version: u64, key: (u8, i32, i32), tile: Option<Arc<Tile>>) {
        let mut c = self.inner.lock();
        if c.version == version {
            c.tiles.insert(key, tile);
        }
    }
}

/// Composite of all visible layers at lod 0 for one tile; `None` when fully transparent.
fn composite_lod0(doc: &Document, tx: i32, ty: i32) -> Option<Tile> {
    let mut acc: Option<Accum> = None;
    let origin = (tx as i64 * TILE as i64, ty as i64 * TILE as i64);
    for layer in doc.layers.iter().filter(|l| l.visible && l.opacity > 0.0) {
        if let Some(t) = layer.tiles.get(&(tx, ty)) {
            acc.get_or_insert_with(Accum::new).composite(t, layer.blend, layer.opacity, origin);
        }
    }
    let acc = acc?;
    let mut out = Tile::transparent();
    acc.to_tile(&mut out);
    (!out.is_transparent()).then_some(out)
}

/// Composite tile at `lod` (tile size stays 256 px; one tile covers 256·2^lod document pixels).
pub fn tile(h: &DocHandle, lod: u8, tx: i32, ty: i32) -> Option<Arc<Tile>> {
    let doc = h.doc.read();
    tile_in(h, &doc, lod, tx, ty)
}

fn tile_in(h: &DocHandle, doc: &Document, lod: u8, tx: i32, ty: i32) -> Option<Arc<Tile>> {
    let (tw, th) = tiles_for(doc.width, doc.height);
    let scale = 1i32 << lod;
    if tx < 0 || ty < 0 || tx * scale >= tw || ty * scale >= th {
        return None;
    }
    let key = (lod, tx, ty);
    if let Some(hit) = h.composite.get(doc.version, key) {
        return hit;
    }
    let result = if lod == 0 {
        composite_lod0(doc, tx, ty).map(Arc::new)
    } else {
        use rayon::prelude::*;
        let kids: Vec<Option<Arc<Tile>>> = [(0, 0), (1, 0), (0, 1), (1, 1)]
            .par_iter()
            .map(|(dx, dy)| tile_in(h, doc, lod - 1, tx * 2 + dx, ty * 2 + dy))
            .collect();
        if kids.iter().all(Option::is_none) {
            None
        } else {
            let mut out = Tile::transparent();
            downsample([kids[0].as_deref(), kids[1].as_deref(), kids[2].as_deref(), kids[3].as_deref()], &mut out);
            Some(Arc::new(out))
        }
    };
    h.composite.put(doc.version, key, result.clone());
    result
}

/// The whole flattened image as RGBA8 (straight alpha), for export.
pub fn flatten(h: &DocHandle) -> (u32, u32, Vec<u8>) {
    use rayon::prelude::*;
    let doc = h.doc.read();
    let (w, hgt) = (doc.width as usize, doc.height as usize);
    let (tw, th) = tiles_for(doc.width, doc.height);
    let tiles: Vec<((i32, i32), Option<Arc<Tile>>)> = (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(tx, ty)| ((tx, ty), tile_in(h, &doc, 0, tx, ty)))
        .collect();
    let mut out = vec![0u8; w * hgt * 4];
    for ((tx, ty), t) in tiles {
        let Some(t) = t else { continue };
        let (x0, y0) = (tx as usize * TILE, ty as usize * TILE);
        let cw = (w - x0).min(TILE);
        for y in 0..(hgt - y0).min(TILE) {
            let dst = ((y0 + y) * w + x0) * 4;
            out[dst..dst + cw * 4].copy_from_slice(&t.0[y * TILE * 4..y * TILE * 4 + cw * 4]);
        }
    }
    (doc.width, doc.height, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Document, Engine, Layer};

    fn doc(layers: Vec<Layer>, w: u32, h: u32) -> Document {
        Document { id: crate::doc::next_id(), name: "t".into(), path: None, width: w, height: h, ppi: 72.0, layers, active: 0, version: 1 }
    }

    #[test]
    fn flatten_round_trips_a_single_layer() {
        let (w, h) = (600, 300);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 13) as u8, 7, 255]).collect();
        let e = Engine::default();
        let hd = e.insert(doc(vec![Layer::from_rgba("a", w, h, &rgba)], w, h));
        let (_, _, out) = flatten(&hd);
        assert_eq!(out, rgba);
    }

    #[test]
    fn half_opacity_white_over_black_is_mid_gray() {
        let mut top = Layer::filled("w", 300, 300, [255, 255, 255, 255]);
        top.opacity = 0.5;
        let e = Engine::default();
        let hd = e.insert(doc(vec![Layer::filled("b", 300, 300, [0, 0, 0, 255]), top], 300, 300));
        let t = tile(&hd, 0, 0, 0).unwrap();
        assert_eq!(t.px(10, 10), [128, 128, 128, 255]);
    }

    /// Throughput of the CPU compositor (run with `cargo test --release -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn bench_composite_30_full_layers() {
        let (w, h) = (2048, 2048);
        let layers: Vec<Layer> = (0..30)
            .map(|i| {
                let mut l = Layer::filled(format!("l{i}"), w, h, [(i * 8) as u8, 100, 200, if i == 0 { 255 } else { 90 }]);
                l.blend = if i % 3 == 1 { lawha_core::BlendMode::Multiply } else { lawha_core::BlendMode::Normal };
                l
            })
            .collect();
        let e = Engine::default();
        let hd = e.insert(doc(layers, w, h));
        let t = std::time::Instant::now();
        let n = 64;
        for i in 0..n {
            composite_lod0(&hd.doc.read(), i % 8, i / 8);
        }
        let per = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("composite 30 layers: {per:.2} ms per 256² tile (single thread)");
    }

    #[test]
    fn mip_levels_average_and_cover_edges() {
        let e = Engine::default();
        let hd = e.insert(doc(vec![Layer::filled("w", 1000, 600, [200, 100, 50, 255])], 1000, 600));
        // lod 2: one tile covers 1024×1024 doc px; the doc fills 1000/4 = 250 px of it.
        let t = tile(&hd, 2, 0, 0).unwrap();
        assert_eq!(t.px(0, 0), [200, 100, 50, 255]);
        assert_eq!(t.px(249, 149), [200, 100, 50, 255]);
        assert_eq!(t.px(251, 10)[3], 0);
        assert!(tile(&hd, 2, 1, 0).is_none());
    }
}
