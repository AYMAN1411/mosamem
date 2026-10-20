//! Resampling: resizing an image and mapping it through a 3×3 transform (scale, rotate, skew,
//! distort, perspective).
//!
//! Kernels (docs/research.md §6): nearest, bilinear (triangle), bicubic (Keys, a = −0.5) and
//! Lanczos-3. Resizing is separable; when shrinking, the kernel widens by the scale factor so
//! detail averages out instead of aliasing. Colors are interpolated with their alpha
//! (premultiplied), so transparent pixels don't bleed dark edges.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos,
}

impl Method {
    /// Half-width of the kernel at scale 1.
    pub fn support(self) -> f64 {
        match self {
            Method::Nearest => 0.5,
            Method::Bilinear => 1.0,
            Method::Bicubic => 2.0,
            Method::Lanczos => 3.0,
        }
    }

    /// The kernel's weight at distance `x`.
    pub fn weight(self, x: f64) -> f64 {
        let x = x.abs();
        match self {
            Method::Nearest => {
                if x < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Method::Bilinear => (1.0 - x).max(0.0),
            Method::Bicubic => {
                // Keys (1981) with a = −0.5
                const A: f64 = -0.5;
                if x <= 1.0 {
                    ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
                } else if x < 2.0 {
                    ((A * x - 5.0 * A) * x + 8.0 * A) * x - 4.0 * A
                } else {
                    0.0
                }
            }
            Method::Lanczos => {
                if x < 1e-9 {
                    1.0
                } else if x < 3.0 {
                    let px = std::f64::consts::PI * x;
                    3.0 * px.sin() * (px / 3.0).sin() / (px * px)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Premultiplied f32 pixels, row-major.
fn premultiply(rgba: &[u8]) -> Vec<[f32; 4]> {
    const K: f32 = 1.0 / 255.0;
    rgba.par_chunks_exact(4)
        .map(|p| {
            let a = p[3] as f32 * K;
            [p[0] as f32 * K * a, p[1] as f32 * K * a, p[2] as f32 * K * a, a]
        })
        .collect()
}

fn unpremultiply(px: &[[f32; 4]]) -> Vec<u8> {
    let mut out = vec![0u8; px.len() * 4];
    out.par_chunks_exact_mut(4).zip(px.par_iter()).for_each(|(o, p)| {
        let a = p[3].clamp(0.0, 1.0);
        if a <= 1e-6 {
            return;
        }
        for c in 0..3 {
            o[c] = ((p[c] / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        o[3] = (a * 255.0 + 0.5) as u8;
    });
    out
}

/// For each output index along one axis: the first source index and its weights.
struct Contrib {
    first: Vec<usize>,
    weights: Vec<Vec<f32>>,
}

fn contributions(src: usize, dst: usize, m: Method) -> Contrib {
    let scale = dst as f64 / src as f64;
    // shrinking widens the kernel so every source pixel counts
    let stretch = if scale < 1.0 { 1.0 / scale } else { 1.0 };
    let support = m.support() * stretch;
    let mut first = Vec::with_capacity(dst);
    let mut weights = Vec::with_capacity(dst);
    for i in 0..dst {
        let center = (i as f64 + 0.5) / scale - 0.5;
        if m == Method::Nearest && stretch == 1.0 {
            let j = ((center + 0.5).floor() as i64).clamp(0, src as i64 - 1) as usize;
            first.push(j);
            weights.push(vec![1.0]);
            continue;
        }
        let lo = ((center - support).floor() as i64).max(0);
        let hi = ((center + support).ceil() as i64).min(src as i64 - 1);
        let mut w: Vec<f64> = (lo..=hi).map(|j| m.weight((j as f64 - center) / stretch)).collect();
        let sum: f64 = w.iter().sum();
        if sum.abs() < 1e-12 {
            // a nearest kernel stretched over no centre: take the closest pixel
            let j = (center.round() as i64).clamp(0, src as i64 - 1);
            first.push(j as usize);
            weights.push(vec![1.0]);
            continue;
        }
        for v in &mut w {
            *v /= sum;
        }
        first.push(lo as usize);
        weights.push(w.into_iter().map(|v| v as f32).collect());
    }
    Contrib { first, weights }
}

/// Resizes a straight RGBA8 image to `nw`×`nh`.
pub fn resize(w: usize, h: usize, rgba: &[u8], nw: usize, nh: usize, m: Method) -> Vec<u8> {
    if w == 0 || h == 0 || nw == 0 || nh == 0 {
        return vec![0; nw * nh * 4];
    }
    let src = premultiply(rgba);
    // horizontal
    let cx = contributions(w, nw, m);
    let mut mid = vec![[0f32; 4]; nw * h];
    mid.par_chunks_mut(nw).enumerate().for_each(|(y, row)| {
        let s = &src[y * w..(y + 1) * w];
        for (x, o) in row.iter_mut().enumerate() {
            let f = cx.first[x];
            let mut acc = [0f32; 4];
            for (k, wt) in cx.weights[x].iter().enumerate() {
                let p = s[f + k];
                for c in 0..4 {
                    acc[c] += p[c] * wt;
                }
            }
            *o = acc;
        }
    });
    // vertical
    let cy = contributions(h, nh, m);
    let mut out = vec![[0f32; 4]; nw * nh];
    out.par_chunks_mut(nw).enumerate().for_each(|(y, row)| {
        let f = cy.first[y];
        for (k, wt) in cy.weights[y].iter().enumerate() {
            let s = &mid[(f + k) * nw..(f + k + 1) * nw];
            for (o, p) in row.iter_mut().zip(s) {
                for c in 0..4 {
                    o[c] += p[c] * wt;
                }
            }
        }
    });
    unpremultiply(&out)
}

/// A 3×3 matrix (row-major) acting on (x, y, 1).
pub type Mat3 = [f64; 9];

pub const IDENTITY: Mat3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

pub fn apply(m: &Mat3, x: f64, y: f64) -> (f64, f64) {
    let w = m[6] * x + m[7] * y + m[8];
    ((m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w)
}

pub fn multiply(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut r = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            r[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    r
}

pub fn invert(m: &Mat3) -> Option<Mat3> {
    let [a, b, c, d, e, f, g, h, i] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        (e * i - f * h) * inv,
        (c * h - b * i) * inv,
        (b * f - c * e) * inv,
        (f * g - d * i) * inv,
        (a * i - c * g) * inv,
        (c * d - a * f) * inv,
        (d * h - e * g) * inv,
        (b * g - a * h) * inv,
        (a * e - b * d) * inv,
    ])
}

/// The projective transform taking the four `from` corners to the four `to` corners.
pub fn homography(from: &[(f64, f64); 4], to: &[(f64, f64); 4]) -> Option<Mat3> {
    // 8 unknowns (h33 = 1), two equations per corner; Gaussian elimination with pivoting
    let mut a = [[0f64; 9]; 8];
    for k in 0..4 {
        let ((x, y), (u, v)) = (from[k], to[k]);
        a[2 * k] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        a[2 * k + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    for col in 0..8 {
        let p = (col..8).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[p][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, p);
        for r in 0..8 {
            if r != col {
                let f = a[r][col] / a[col][col];
                for c in col..9 {
                    a[r][c] -= f * a[col][c];
                }
            }
        }
    }
    let s: Vec<f64> = (0..8).map(|i| a[i][8] / a[i][i]).collect();
    Some([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], 1.0])
}

/// How much a transform shrinks things near a point (the smaller axis scale, ≤ 1 when shrinking).
fn local_scale(m: &Mat3, x: f64, y: f64) -> (f64, f64) {
    let (x0, y0) = apply(m, x, y);
    let (x1, y1) = apply(m, x + 1.0, y);
    let (x2, y2) = apply(m, x, y + 1.0);
    ((x1 - x0).hypot(y1 - y0), (x2 - x0).hypot(y2 - y0))
}

/// Maps a straight RGBA8 source image through `forward` (source px → output px) into an output
/// rectangle of `ow`×`oh` whose top-left is at `origin` in output coordinates. Outside the source
/// is transparent. Strong shrinking first resizes the source so the result doesn't alias.
pub fn transform(w: usize, h: usize, rgba: &[u8], forward: &Mat3, origin: (i64, i64), ow: usize, oh: usize, m: Method) -> Vec<u8> {
    if w == 0 || h == 0 || ow == 0 || oh == 0 {
        return vec![0; ow * oh * 4];
    }
    // prefilter: shrink the source to about the output's density first
    let (sx, sy) = local_scale(forward, w as f64 / 2.0, h as f64 / 2.0);
    let (fx, fy) = (sx.min(1.0), sy.min(1.0));
    let shrunk;
    let (src, pw, ph, fwd): (&[u8], usize, usize, Mat3) = if (fx < 0.75 || fy < 0.75) && m != Method::Nearest {
        let (pw, ph) = (((w as f64 * fx).round() as usize).max(1), ((h as f64 * fy).round() as usize).max(1));
        shrunk = resize(w, h, rgba, pw, ph, m);
        // output = forward · scale(w/pw, h/ph) · small
        let up: Mat3 = [w as f64 / pw as f64, 0.0, 0.0, 0.0, h as f64 / ph as f64, 0.0, 0.0, 0.0, 1.0];
        (&shrunk, pw, ph, multiply(forward, &up))
    } else {
        (rgba, w, h, *forward)
    };
    let Some(inv) = invert(&fwd) else { return vec![0; ow * oh * 4] };
    let taps = match m {
        Method::Nearest => 0,
        _ => m.support().ceil() as i64,
    };
    // premultiplied on the fly: the source stays 4 bytes a pixel
    const K: f32 = 1.0 / 255.0;
    let at = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= pw as i64 || y >= ph as i64 {
            return [0.0; 4];
        }
        let p = &src[(y as usize * pw + x as usize) * 4..][..4];
        let a = p[3] as f32 * K;
        [p[0] as f32 * K * a, p[1] as f32 * K * a, p[2] as f32 * K * a, a]
    };
    let mut out = vec![0u8; ow * oh * 4];
    out.par_chunks_mut(ow * 4).enumerate().for_each(|(oy, row)| {
        for (ox, o) in row.chunks_exact_mut(4).enumerate() {
            let (dx, dy) = ((ox as i64 + origin.0) as f64 + 0.5, (oy as i64 + origin.1) as f64 + 0.5);
            let (sx, sy) = apply(&inv, dx, dy);
            let (sx, sy) = (sx - 0.5, sy - 0.5);
            if !(sx >= -1.0 && sy >= -1.0 && sx <= pw as f64 && sy <= ph as f64) {
                continue;
            }
            let acc = if m == Method::Nearest {
                let (x, y) = ((sx + 0.5).floor() as i64, (sy + 0.5).floor() as i64);
                if x < 0 || y < 0 || x >= pw as i64 || y >= ph as i64 {
                    continue;
                }
                o.copy_from_slice(&src[(y as usize * pw + x as usize) * 4..][..4]);
                continue;
            } else {
                let (bx, by) = (sx.floor() as i64, sy.floor() as i64);
                let mut wx = [0f64; 6];
                let mut wy = [0f64; 6];
                let n = (2 * taps) as usize;
                for i in 0..n {
                    wx[i] = m.weight((bx - taps + 1 + i as i64) as f64 - sx);
                    wy[i] = m.weight((by - taps + 1 + i as i64) as f64 - sy);
                }
                let (swx, swy): (f64, f64) = (wx[..n].iter().sum(), wy[..n].iter().sum());
                let mut acc = [0f32; 4];
                for j in 0..n {
                    for i in 0..n {
                        let wt = (wx[i] * wy[j] / (swx * swy)) as f32;
                        let p = at(bx - taps + 1 + i as i64, by - taps + 1 + j as i64);
                        for c in 0..4 {
                            acc[c] += p[c] * wt;
                        }
                    }
                }
                acc
            };
            let a = acc[3].clamp(0.0, 1.0);
            if a <= 1e-6 {
                continue;
            }
            for c in 0..3 {
                o[c] = ((acc[c] / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            o[3] = (a * 255.0 + 0.5) as u8;
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernels_match_their_closed_forms() {
        let b = Method::Bicubic;
        // Keys a = −0.5 at known points
        assert!((b.weight(0.0) - 1.0).abs() < 1e-12);
        assert!(b.weight(1.0).abs() < 1e-12 && b.weight(2.0).abs() < 1e-12);
        assert!((b.weight(0.5) - 0.5625).abs() < 1e-12);
        assert!((b.weight(1.5) + 0.0625).abs() < 1e-12);
        // Lanczos-3: sinc(x)·sinc(x/3)
        let sinc = |x: f64| (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x);
        for x in [0.3, 1.2, 2.7] {
            assert!((Method::Lanczos.weight(x) - sinc(x) * sinc(x / 3.0)).abs() < 1e-12);
        }
        assert!(Method::Lanczos.weight(1.0).abs() < 1e-12 && Method::Lanczos.weight(3.0) == 0.0);
        assert_eq!(Method::Bilinear.weight(0.25), 0.75);
        // every kernel sums to one over integer offsets (bicubic and bilinear exactly)
        for m in [Method::Bilinear, Method::Bicubic] {
            for f in [0.0, 0.2, 0.5, 0.9] {
                let s: f64 = (-3..=3).map(|j| m.weight(j as f64 - f)).sum();
                assert!((s - 1.0).abs() < 1e-12, "{m:?} {f}");
            }
        }
    }

    #[test]
    fn constant_images_stay_constant() {
        let img: Vec<u8> = [90u8, 160, 30, 255].iter().copied().cycle().take(37 * 23 * 4).collect();
        for m in [Method::Nearest, Method::Bilinear, Method::Bicubic, Method::Lanczos] {
            for (nw, nh) in [(100, 61), (12, 7), (37, 23)] {
                let out = resize(37, 23, &img, nw, nh, m);
                assert!(out.chunks_exact(4).all(|p| p == [90, 160, 30, 255]), "{m:?} {nw}×{nh}");
            }
        }
    }

    #[test]
    fn shrinking_a_checkerboard_does_not_alias() {
        let (w, h) = (256, 256);
        let img: Vec<u8> = (0..w * h).flat_map(|i| if (i % w + i / w) % 2 == 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        for m in [Method::Bilinear, Method::Bicubic, Method::Lanczos] {
            let out = resize(w, h, &img, 37, 37, m);
            for p in out.chunks_exact(4) {
                assert!((p[0] as i32 - 128).abs() <= 6, "{m:?}: {p:?}");
            }
        }
    }

    #[test]
    fn identity_and_exact_moves_keep_every_pixel() {
        let (w, h) = (20, 15);
        let img: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7 % 256) as u8, (i * 3 % 256) as u8, (i % 256) as u8, 255]).collect();
        for m in [Method::Nearest, Method::Bilinear, Method::Bicubic, Method::Lanczos] {
            assert_eq!(transform(w, h, &img, &IDENTITY, (0, 0), w, h, m), img, "{m:?}");
        }
        // a whole-pixel translation into a larger output
        let t: Mat3 = [1.0, 0.0, 3.0, 0.0, 1.0, 2.0, 0.0, 0.0, 1.0];
        let out = transform(w, h, &img, &t, (0, 0), w + 3, h + 2, Method::Bicubic);
        for y in 0..h {
            for x in 0..w {
                assert_eq!(&out[((y + 2) * (w + 3) + x + 3) * 4..][..4], &img[(y * w + x) * 4..][..4]);
            }
        }
        assert_eq!(out[3], 0, "uncovered corner stays transparent");
    }

    #[test]
    fn a_quarter_turn_moves_pixels_where_expected() {
        let (w, h) = (4, 3);
        let img: Vec<u8> = (0..w * h).flat_map(|i| [i as u8 * 20, 0, 0, 255]).collect();
        // rotate 90° clockwise: (x, y) → (h − y, x); output is h × w
        let r: Mat3 = [0.0, -1.0, h as f64, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let out = transform(w, h, &img, &r, (0, 0), h, w, Method::Nearest);
        // source (0, 0) lands at output (h − 1, 0)
        assert_eq!(out[((0 * h) + (h - 1)) * 4], 0);
        // source (3, 2) (value 11·20) lands at output (0, 3)
        assert_eq!(out[((3 * h) + 0) * 4], 220);
        let out_b = transform(w, h, &img, &r, (0, 0), h, w, Method::Bicubic);
        assert_eq!(out, out_b, "exact quarter turns need no interpolation");
    }

    #[test]
    fn homographies_map_corners_and_invert() {
        let from = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
        let to = [(10.0, 5.0), (120.0, 20.0), (90.0, 80.0), (-5.0, 60.0)];
        let m = homography(&from, &to).unwrap();
        for k in 0..4 {
            let (u, v) = apply(&m, from[k].0, from[k].1);
            assert!((u - to[k].0).abs() < 1e-9 && (v - to[k].1).abs() < 1e-9);
        }
        let inv = invert(&m).unwrap();
        let back = multiply(&inv, &m);
        for (i, v) in back.iter().enumerate() {
            let want = if i % 4 == 0 { 1.0 } else { 0.0 };
            assert!((v / back[8] - want).abs() < 1e-9);
        }
    }

    #[test]
    fn transformed_edges_fade_without_dark_fringes() {
        // a red square scaled by 1.5 with bicubic: edge pixels stay red, only alpha fades
        let (w, h) = (10, 10);
        let img: Vec<u8> = [255u8, 0, 0, 255].iter().copied().cycle().take(w * h * 4).collect();
        let s: Mat3 = [1.5, 0.0, 2.0, 0.0, 1.5, 2.0, 0.0, 0.0, 1.0];
        let out = transform(w, h, &img, &s, (0, 0), 20, 20, Method::Bicubic);
        for p in out.chunks_exact(4).filter(|p| p[3] > 0) {
            assert!(p[0] >= 250 && p[1] <= 3, "{p:?}");
        }
    }
}
