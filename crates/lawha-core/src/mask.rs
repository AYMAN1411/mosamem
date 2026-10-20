//! 8-bit coverage masks: selections and layer masks. Stored sparsely as 256×256 tiles (a missing
//! tile has the mask's `outside` value), and converted to a dense buffer for the image operations
//! in `select`, which work on the whole canvas at once.

use std::collections::HashMap;
use std::sync::Arc;

use crate::tile::TILE;

pub const MASK_TILE_BYTES: usize = TILE * TILE;

#[derive(Clone, PartialEq, Eq)]
pub struct MaskTile(pub Box<[u8]>);

impl MaskTile {
    pub fn filled(v: u8) -> Self {
        MaskTile(vec![v; MASK_TILE_BYTES].into_boxed_slice())
    }
    #[inline]
    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.0[y * TILE + x]
    }
    fn uniform(&self) -> Option<u8> {
        let v = self.0[0];
        self.0.iter().all(|x| *x == v).then_some(v)
    }
}

pub type MaskMap = HashMap<(i32, i32), Arc<MaskTile>>;

/// A sparse mask over the canvas.
#[derive(Clone)]
pub struct Mask {
    pub tiles: Arc<MaskMap>,
    /// Value of every pixel not covered by a stored tile.
    pub outside: u8,
}

impl Mask {
    pub fn uniform(v: u8) -> Self {
        Mask { tiles: Arc::new(MaskMap::new()), outside: v }
    }

    #[inline]
    pub fn tile(&self, key: (i32, i32)) -> Option<&MaskTile> {
        self.tiles.get(&key).map(|t| t.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        self.outside == 0 && self.tiles.values().all(|t| t.uniform() == Some(0))
    }

    pub fn to_dense(&self, w: u32, h: u32) -> Dense {
        let mut d = Dense { w, h, data: vec![self.outside; w as usize * h as usize] };
        for (&(tx, ty), t) in self.tiles.iter() {
            let (x0, y0) = (tx as usize * TILE, ty as usize * TILE);
            if tx < 0 || ty < 0 || x0 >= w as usize || y0 >= h as usize {
                continue;
            }
            let cw = (w as usize - x0).min(TILE);
            for y in 0..(h as usize - y0).min(TILE) {
                let dst = (y0 + y) * w as usize + x0;
                d.data[dst..dst + cw].copy_from_slice(&t.0[y * TILE..y * TILE + cw]);
            }
        }
        d
    }

    /// Sparse mask from a dense buffer; uniform tiles equal to `outside` are not stored.
    pub fn from_dense(d: &Dense, outside: u8) -> Self {
        use rayon::prelude::*;
        let (tw, th) = (d.w.div_ceil(TILE as u32) as i32, d.h.div_ceil(TILE as u32) as i32);
        let shared = Arc::new(MaskTile::filled(255));
        let tiles: MaskMap = (0..th)
            .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
            .collect::<Vec<_>>()
            .into_par_iter()
            .filter_map(|(tx, ty)| {
                let mut t = MaskTile::filled(outside);
                let (x0, y0) = (tx as usize * TILE, ty as usize * TILE);
                let cw = (d.w as usize - x0).min(TILE);
                for y in 0..(d.h as usize - y0).min(TILE) {
                    let src = (y0 + y) * d.w as usize + x0;
                    t.0[y * TILE..y * TILE + cw].copy_from_slice(&d.data[src..src + cw]);
                }
                match t.uniform() {
                    Some(v) if v == outside => None,
                    Some(255) => Some(((tx, ty), shared.clone())),
                    _ => Some(((tx, ty), Arc::new(t))),
                }
            })
            .collect();
        Mask { tiles: Arc::new(tiles), outside }
    }
}

/// A dense mask covering the canvas, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Dense {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl Dense {
    pub fn new(w: u32, h: u32, v: u8) -> Self {
        Dense { w, h, data: vec![v; w as usize * h as usize] }
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.w as usize + x]
    }
    pub fn count_over(&self, t: u8) -> usize {
        self.data.iter().filter(|v| **v > t).count()
    }
    /// Bounding box of pixels above zero: (x0, y0, x1, y1) exclusive, or None if empty.
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (w, h) = (self.w as usize, self.h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for y in 0..h {
            let row = &self.data[y * w..(y + 1) * w];
            if let Some(a) = row.iter().position(|v| *v > 0) {
                let b = row.iter().rposition(|v| *v > 0).unwrap();
                x0 = x0.min(a);
                x1 = x1.max(b + 1);
                y0 = y0.min(y);
                y1 = y + 1;
            }
        }
        (x1 > x0).then_some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
    }
}
