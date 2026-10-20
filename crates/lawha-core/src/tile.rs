//! 256×256 RGBA8 tiles (straight alpha, sRGB) and the per-tile operations the engine needs.

use crate::blend::{composite_px, dissolve_threshold, BlendMode};

pub const TILE: usize = 256;
pub const TILE_PX: usize = TILE * TILE;
pub const TILE_BYTES: usize = TILE_PX * 4;

/// One tile of pixels. Boxed so tiles move cheaply and can sit behind `Arc` for copy-on-write.
#[derive(Clone)]
pub struct Tile(pub Box<[u8]>);

impl Tile {
    pub fn transparent() -> Self {
        Tile(vec![0u8; TILE_BYTES].into_boxed_slice())
    }

    pub fn filled(rgba: [u8; 4]) -> Self {
        let mut t = Self::transparent();
        for px in t.0.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
        t
    }

    pub fn is_transparent(&self) -> bool {
        self.0.chunks_exact(4).all(|p| p[3] == 0)
    }

    #[inline]
    pub fn px(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * TILE + x) * 4;
        [self.0[i], self.0[i + 1], self.0[i + 2], self.0[i + 3]]
    }
}

/// Premultiplied f32 accumulator for compositing one tile.
pub struct Accum(pub Vec<[f32; 4]>);

impl Default for Accum {
    fn default() -> Self {
        Self::new()
    }
}

impl Accum {
    pub fn new() -> Self {
        Accum(vec![[0.0; 4]; TILE_PX])
    }

    pub fn clear(&mut self) {
        self.0.fill([0.0; 4]);
    }

    /// Composites a source tile with the given mode and opacity. `origin` is the tile's top-left
    /// in document pixels (used by Dissolve).
    pub fn composite(&mut self, src: &Tile, mode: BlendMode, opacity: f32, origin: (i64, i64)) {
        const K: f32 = 1.0 / 255.0;
        if mode == BlendMode::Normal {
            // source-over: co = cs·as + cb·(1 − as), in premultiplied form
            for (d, s) in self.0.iter_mut().zip(src.0.chunks_exact(4)) {
                let a = s[3] as f32 * K * opacity;
                let inv = 1.0 - a;
                let ak = a * K;
                d[0] = s[0] as f32 * ak + d[0] * inv;
                d[1] = s[1] as f32 * ak + d[1] * inv;
                d[2] = s[2] as f32 * ak + d[2] * inv;
                d[3] = a + d[3] * inv;
            }
            return;
        }
        for (i, (d, s)) in self.0.iter_mut().zip(src.0.chunks_exact(4)).enumerate() {
            if s[3] == 0 {
                continue;
            }
            let mut a = s[3] as f32 * K * opacity;
            if mode == BlendMode::Dissolve {
                let (x, y) = ((i % TILE) as i64 + origin.0, (i / TILE) as i64 + origin.1);
                a = if dissolve_threshold(x, y) < a { 1.0 } else { 0.0 };
            }
            composite_px(mode, d, [s[0] as f32 * K, s[1] as f32 * K, s[2] as f32 * K], a);
        }
    }

    /// Premultiplied copy of a tile, with its alpha scaled by `opacity`.
    pub fn from_tile(src: &Tile, opacity: f32) -> Self {
        const K: f32 = 1.0 / 255.0;
        let mut a = Accum::new();
        for (d, s) in a.0.iter_mut().zip(src.0.chunks_exact(4)) {
            let al = s[3] as f32 * K * opacity;
            let k = al * K;
            *d = [s[0] as f32 * k, s[1] as f32 * k, s[2] as f32 * k, al];
        }
        a
    }

    /// Composites another premultiplied accumulator (an isolated group or clipping group) onto this
    /// one with a blend mode and opacity.
    pub fn composite_accum(&mut self, src: &Accum, mode: BlendMode, opacity: f32, origin: (i64, i64)) {
        for (i, (d, s)) in self.0.iter_mut().zip(src.0.iter()).enumerate() {
            let sa = s[3];
            if sa <= 0.0 {
                continue;
            }
            let mut a = sa * opacity;
            if mode == BlendMode::Dissolve {
                let (x, y) = ((i % TILE) as i64 + origin.0, (i / TILE) as i64 + origin.1);
                a = if dissolve_threshold(x, y) < a { 1.0 } else { 0.0 };
            }
            let inv = 1.0 / sa;
            composite_px(mode, d, [s[0] * inv, s[1] * inv, s[2] * inv], a);
        }
    }

    /// Source-atop: composites a tile only where this accumulator already has alpha, keeping that
    /// alpha (used for layers clipped to a base layer). W3C general formula with Fa = αb, Fb = 1 − αs.
    pub fn composite_atop(&mut self, src: &Tile, mode: BlendMode, opacity: f32) {
        const K: f32 = 1.0 / 255.0;
        for (d, s) in self.0.iter_mut().zip(src.0.chunks_exact(4)) {
            let ab = d[3];
            if s[3] == 0 || ab <= 0.0 {
                continue;
            }
            let a_s = s[3] as f32 * K * opacity;
            let cs = [s[0] as f32 * K, s[1] as f32 * K, s[2] as f32 * K];
            let cb = [d[0] / ab, d[1] / ab, d[2] / ab];
            let b = crate::blend::blend_rgb(mode, cb, cs);
            for k in 0..3 {
                let cs2 = (1.0 - ab) * cs[k] + ab * b[k];
                d[k] = a_s * ab * cs2 + ab * (1.0 - a_s) * cb[k];
            }
        }
    }

    /// Like `composite_atop`, for a premultiplied source (a clipped group).
    pub fn composite_accum_atop(&mut self, src: &Accum, mode: BlendMode, opacity: f32) {
        for (d, s) in self.0.iter_mut().zip(src.0.iter()) {
            let ab = d[3];
            if s[3] <= 0.0 || ab <= 0.0 {
                continue;
            }
            let a_s = s[3] * opacity;
            let inv = 1.0 / s[3];
            let cs = [s[0] * inv, s[1] * inv, s[2] * inv];
            let cb = [d[0] / ab, d[1] / ab, d[2] / ab];
            let b = crate::blend::blend_rgb(mode, cb, cs);
            for k in 0..3 {
                let cs2 = (1.0 - ab) * cs[k] + ab * b[k];
                d[k] = a_s * ab * cs2 + ab * (1.0 - a_s) * cb[k];
            }
        }
    }

    /// self = self + (other − self)·t, per premultiplied component. Used for the opacity of
    /// pass-through groups, whose children blend straight into the backdrop.
    pub fn lerp_to(&mut self, other: &Accum, t: f32) {
        for (d, o) in self.0.iter_mut().zip(other.0.iter()) {
            for k in 0..4 {
                d[k] += (o[k] - d[k]) * t;
            }
        }
    }

    /// Multiplies every pixel by a coverage factor (a layer mask).
    pub fn mul_factors(&mut self, f: &[f32]) {
        for (d, k) in self.0.iter_mut().zip(f) {
            for c in d.iter_mut() {
                *c *= k;
            }
        }
    }

    /// Per-pixel lerp toward `other` by `t[i]` (pass-through groups with a mask).
    pub fn lerp_to_each(&mut self, other: &Accum, t: &[f32]) {
        for ((d, o), k) in self.0.iter_mut().zip(other.0.iter()).zip(t) {
            for c in 0..4 {
                d[c] += (o[c] - d[c]) * k;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|p| p[3] <= 0.0)
    }

    /// Writes straight-alpha RGBA8.
    pub fn to_tile(&self, out: &mut Tile) {
        for (d, s) in out.0.chunks_exact_mut(4).zip(self.0.iter()) {
            let a = s[3];
            if a <= 0.0 {
                d.copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let inv = 1.0 / a;
            d[0] = to_u8(s[0] * inv);
            d[1] = to_u8(s[1] * inv);
            d[2] = to_u8(s[2] * inv);
            d[3] = to_u8(a);
        }
    }
}

#[inline]
pub fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// 2× box downsample of four child tiles (top-left, top-right, bottom-left, bottom-right) into one
/// tile, averaging in premultiplied space so transparent pixels don't darken edges.
pub fn downsample(children: [Option<&Tile>; 4], out: &mut Tile) {
    const H: usize = TILE / 2;
    for (q, child) in children.iter().enumerate() {
        let (ox, oy) = ((q % 2) * H, (q / 2) * H);
        for y in 0..H {
            for x in 0..H {
                let o = ((oy + y) * TILE + ox + x) * 4;
                let Some(c) = child else {
                    out.0[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                    continue;
                };
                let mut acc = [0u32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let i = ((2 * y + dy) * TILE + 2 * x + dx) * 4;
                    let a = c.0[i + 3] as u32;
                    acc[0] += c.0[i] as u32 * a;
                    acc[1] += c.0[i + 1] as u32 * a;
                    acc[2] += c.0[i + 2] as u32 * a;
                    acc[3] += a;
                }
                if acc[3] == 0 {
                    out.0[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                } else {
                    for k in 0..3 {
                        out.0[o + k] = ((acc[k] + acc[3] / 2) / acc[3]) as u8;
                    }
                    out.0[o + 3] = ((acc[3] + 2) / 4) as u8;
                }
            }
        }
    }
}
