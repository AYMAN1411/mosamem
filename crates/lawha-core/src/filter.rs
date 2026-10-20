//! Filters: pixel operations that look at neighbours (blurs, sharpening, noise, mosaic).
//!
//! `apply` runs a filter on a dense straight-alpha RGBA8 region. Beyond the region's edge the edge
//! pixels repeat, which is right at the canvas edge; everywhere else the engine hands in a margin
//! of at least `Filter::reach` real pixels, so results computed tile by tile join without seams.
//! Colors are averaged with their alpha (premultiplied), so transparent pixels don't darken edges.
//!
//! Technique and parameter ranges: docs/research.md §9. Mappings marked ✱ there are ours.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Filter {
    /// Radius in px; σ = radius (✱).
    GaussianBlur { radius: f32 },
    /// Angle in degrees (0 = horizontal, counter-clockwise), distance in px.
    MotionBlur { angle: f32, distance: f32 },
    /// Amount in percent, radius in px, threshold in levels (0–255).
    UnsharpMask { amount: f32, radius: f32, threshold: f32 },
    /// Amount in percent; the seed fixes the pattern, so the preview and the result match.
    AddNoise { amount: f32, gaussian: bool, monochromatic: bool, seed: u32 },
    /// Cell size in px, cells aligned to the canvas origin.
    Pixelate { cell: f32 },
}

impl Filter {
    /// The same filter on an image scaled by `s` (a mip level): distances shrink, strengths stay.
    pub fn scaled(&self, s: f32) -> Filter {
        match *self {
            Filter::GaussianBlur { radius } => Filter::GaussianBlur { radius: radius * s },
            Filter::MotionBlur { angle, distance } => Filter::MotionBlur { angle, distance: distance * s },
            Filter::UnsharpMask { amount, radius, threshold } => Filter::UnsharpMask { amount, radius: radius * s, threshold },
            Filter::AddNoise { .. } => self.clone(),
            Filter::Pixelate { cell } => Filter::Pixelate { cell: (cell * s).max(1.0) },
        }
    }

    /// How far, in px, a result pixel reads from its own position.
    pub fn reach(&self) -> u32 {
        match *self {
            Filter::GaussianBlur { radius } | Filter::UnsharpMask { radius, .. } => gaussian_reach(radius),
            Filter::MotionBlur { distance, .. } => (distance / 2.0).ceil() as u32 + 1,
            Filter::AddNoise { .. } => 0,
            Filter::Pixelate { cell } => cell_size(cell) as u32,
        }
    }
}

fn gaussian_reach(sigma: f32) -> u32 {
    if sigma <= 0.0 {
        0
    } else {
        (3.0 * sigma).ceil() as u32 + 1
    }
}

fn cell_size(cell: f32) -> usize {
    (cell.round() as usize).max(1)
}

/// Applies a filter to a `w`×`h` straight RGBA8 region whose top-left pixel is at `origin` (in the
/// coordinates of the image level it comes from: noise and mosaic cells are tied to them).
pub fn apply(w: usize, h: usize, rgba: &[u8], f: &Filter, origin: (i64, i64)) -> Vec<u8> {
    match *f {
        Filter::GaussianBlur { radius } => {
            let mut p = Planes::premultiplied(w, h, rgba);
            p.planes.par_iter_mut().for_each(|pl| gaussian(w, h, pl, radius));
            p.to_rgba()
        }
        Filter::MotionBlur { angle, distance } => motion_blur(w, h, rgba, angle, distance),
        Filter::UnsharpMask { amount, radius, threshold } => unsharp(w, h, rgba, amount, radius, threshold),
        Filter::AddNoise { amount, gaussian, monochromatic, seed } => noise(w, h, rgba, amount, gaussian, monochromatic, seed, origin),
        Filter::Pixelate { cell } => pixelate(w, h, rgba, cell_size(cell), origin),
    }
}

/// Premultiplied planes in [0, 1]: red, green, blue (times alpha) and alpha.
struct Planes {
    w: usize,
    h: usize,
    planes: [Vec<f32>; 4],
}

impl Planes {
    fn premultiplied(w: usize, h: usize, rgba: &[u8]) -> Planes {
        const K: f32 = 1.0 / 255.0;
        let planes: [Vec<f32>; 4] = std::array::from_fn(|c| {
            rgba.par_chunks_exact(4)
                .map(|p| if c == 3 { p[3] as f32 * K } else { p[c] as f32 * K * (p[3] as f32 * K) })
                .collect()
        });
        Planes { w, h, planes }
    }

    fn to_rgba(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.w * self.h * 4];
        out.par_chunks_exact_mut(4).enumerate().for_each(|(i, o)| {
            let a = self.planes[3][i];
            if a <= 0.0 {
                return;
            }
            for c in 0..3 {
                o[c] = to_u8(self.planes[c][i] / a);
            }
            o[3] = to_u8(a);
        });
        out
    }
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

// ---- blurs ----

/// Gaussian blur with σ = `sigma`, edges repeating: an exact kernel for small σ, three box passes
/// (Kovesi) above that, where the difference can't be seen and the cost no longer grows with σ.
pub fn gaussian(w: usize, h: usize, data: &mut [f32], sigma: f32) {
    if sigma <= 0.0 || w == 0 || h == 0 {
        return;
    }
    if sigma >= 2.0 {
        return crate::select::gaussian(w, h, data, sigma);
    }
    let r = gaussian_reach(sigma) as i64;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = k.iter().sum();
    let k: Vec<f32> = k.iter().map(|v| v / sum).collect();
    let tmp = data.to_vec();
    data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let src = &tmp[y * w..(y + 1) * w];
        for (x, o) in row.iter_mut().enumerate() {
            *o = k.iter().enumerate().map(|(j, kv)| kv * src[(x as i64 + j as i64 - r).clamp(0, w as i64 - 1) as usize]).sum();
        }
    });
    let tmp = data.to_vec();
    data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            *o = k.iter().enumerate().map(|(j, kv)| kv * tmp[(y as i64 + j as i64 - r).clamp(0, h as i64 - 1) as usize * w + x]).sum();
        }
    });
}

fn motion_blur(w: usize, h: usize, rgba: &[u8], angle: f32, distance: f32) -> Vec<u8> {
    let p = Planes::premultiplied(w, h, rgba);
    if distance <= 0.0 {
        return rgba.to_vec();
    }
    let (dx, dy) = (angle.to_radians().cos(), -angle.to_radians().sin());
    let n = (distance.ceil() as usize).max(1) + 1;
    let offsets: Vec<(f32, f32)> = (0..n)
        .map(|k| {
            let t = -distance / 2.0 + distance * k as f32 / (n - 1) as f32;
            (t * dx, t * dy)
        })
        .collect();
    let mut out = Planes { w, h, planes: std::array::from_fn(|_| vec![0.0; w * h]) };
    // bilinear samples along the line, edges repeating
    let sample = |pl: &[f32], x: f32, y: f32| {
        let x = x.clamp(0.0, (w - 1) as f32);
        let y = y.clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let top = pl[y0 * w + x0] * (1.0 - fx) + pl[y0 * w + x1] * fx;
        let bot = pl[y1 * w + x0] * (1.0 - fx) + pl[y1 * w + x1] * fx;
        top * (1.0 - fy) + bot * fy
    };
    let inv = 1.0 / n as f32;
    for c in 0..4 {
        let src = &p.planes[c];
        out.planes[c].par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                *o = offsets.iter().map(|&(ox, oy)| sample(src, x as f32 + ox, y as f32 + oy)).sum::<f32>() * inv;
            }
        });
    }
    out.to_rgba()
}

fn unsharp(w: usize, h: usize, rgba: &[u8], amount: f32, radius: f32, threshold: f32) -> Vec<u8> {
    let mut blurred = Planes::premultiplied(w, h, rgba);
    blurred.planes.par_iter_mut().for_each(|pl| gaussian(w, h, pl, radius));
    let k = amount / 100.0;
    let mut out = rgba.to_vec();
    out.par_chunks_exact_mut(4).enumerate().for_each(|(i, o)| {
        if o[3] == 0 {
            return;
        }
        let ba = blurred.planes[3][i];
        for c in 0..3 {
            let x = o[c] as f32 / 255.0;
            // the blurred straight color: alpha-weighted, so transparent neighbours don't count
            let b = if ba > 0.0 { blurred.planes[c][i] / ba } else { x };
            let d = x - b;
            if d.abs() * 255.0 >= threshold {
                o[c] = to_u8(x + k * d);
            }
        }
    });
    out
}

/// A 32-bit hash of a pixel, channel and seed, uniform in [0, 1).
fn hash(x: i64, y: i64, c: u32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ c.wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16_777_216.0
}

#[allow(clippy::too_many_arguments)]
fn noise(w: usize, _h: usize, rgba: &[u8], amount: f32, gaussian: bool, mono: bool, seed: u32, origin: (i64, i64)) -> Vec<u8> {
    // uniform: ±amount/2 of the full range; Gaussian: σ = amount/4 (✱)
    let a = amount / 100.0;
    let mut out = rgba.to_vec();
    out.par_chunks_exact_mut(4).enumerate().for_each(|(i, o)| {
        if o[3] == 0 {
            return;
        }
        let (x, y) = ((i % w) as i64 + origin.0, (i / w) as i64 + origin.1);
        let draw = |c: u32| {
            if gaussian {
                // Box–Muller
                let u1 = hash(x, y, 2 * c, seed).max(1e-7);
                let u2 = hash(x, y, 2 * c + 1, seed);
                (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * a / 4.0
            } else {
                (hash(x, y, c, seed) - 0.5) * a
            }
        };
        let shared = if mono { Some(draw(0)) } else { None };
        for c in 0..3 {
            let n = shared.unwrap_or_else(|| draw(c as u32));
            o[c] = to_u8(o[c] as f32 / 255.0 + n);
        }
    });
    out
}

fn pixelate(w: usize, h: usize, rgba: &[u8], cell: usize, origin: (i64, i64)) -> Vec<u8> {
    if cell <= 1 {
        return rgba.to_vec();
    }
    let c = cell as i64;
    // cell bounds in region coordinates, aligned to the image origin
    let first = |o: i64| o.div_euclid(c) * c - o;
    let (x_start, y_start) = (first(origin.0), first(origin.1));
    let cols: Vec<i64> = (0..).map(|k| x_start + k * c).take_while(|&x| x < w as i64).collect();
    let rows: Vec<i64> = (0..).map(|k| y_start + k * c).take_while(|&y| y < h as i64).collect();
    let mut out = vec![0u8; w * h * 4];
    let row_bands: Vec<(i64, Vec<u8>)> = rows
        .par_iter()
        .map(|&y0| {
            let (ya, yb) = (y0.max(0) as usize, ((y0 + c) as usize).min(h));
            let mut band = vec![0u8; (yb - ya) * w * 4];
            for &x0 in &cols {
                let (xa, xb) = (x0.max(0) as usize, ((x0 + c) as usize).min(w));
                let mut s = [0f32; 4];
                for y in ya..yb {
                    for x in xa..xb {
                        let p = &rgba[(y * w + x) * 4..][..4];
                        let al = p[3] as f32;
                        s[0] += p[0] as f32 * al;
                        s[1] += p[1] as f32 * al;
                        s[2] += p[2] as f32 * al;
                        s[3] += al;
                    }
                }
                let n = ((yb - ya) * (xb - xa)) as f32;
                let px = if s[3] > 0.0 {
                    [(s[0] / s[3] + 0.5) as u8, (s[1] / s[3] + 0.5) as u8, (s[2] / s[3] + 0.5) as u8, (s[3] / n + 0.5) as u8]
                } else {
                    [0; 4]
                };
                for y in 0..yb - ya {
                    for x in xa..xb {
                        band[(y * w + x) * 4..][..4].copy_from_slice(&px);
                    }
                }
            }
            (ya as i64, band)
        })
        .collect();
    for (ya, band) in row_bands {
        let start = ya as usize * w * 4;
        out[start..start + band.len()].copy_from_slice(&band);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take(w * h * 4).collect()
    }

    fn at(img: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn blurs_keep_flat_areas_and_repeat_the_edges() {
        // a flat opaque image stays exactly the same: no darkening at the borders
        let img = solid(64, 48, [120, 30, 200, 255]);
        for f in [Filter::GaussianBlur { radius: 1.2 }, Filter::GaussianBlur { radius: 9.0 }, Filter::MotionBlur { angle: 30.0, distance: 25.0 }] {
            let out = apply(64, 48, &img, &f, (0, 0));
            assert!(out.iter().zip(&img).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 1), "{f:?}");
        }
    }

    #[test]
    fn gaussian_spreads_a_dot_symmetrically_with_the_right_sigma() {
        let (w, h) = (101, 101);
        let mut d = vec![0f32; w * h];
        d[50 * w + 50] = 1.0;
        gaussian(w, h, &mut d, 1.5);
        let total: f32 = d.iter().sum();
        assert!((total - 1.0).abs() < 1e-4);
        // variance along x equals σ²
        let var: f32 = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| d[y * w + x] * ((x as f32 - 50.0).powi(2))).sum();
        assert!((var - 2.25).abs() < 0.05, "{var}");
        assert!((d[50 * w + 47] - d[50 * w + 53]).abs() < 1e-6 && (d[47 * w + 50] - d[53 * w + 50]).abs() < 1e-6);
        // box-pass version for larger σ: variance close to σ² as well
        let mut d = vec![0f32; w * h];
        d[50 * w + 50] = 1.0;
        gaussian(w, h, &mut d, 6.0);
        let var: f32 = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| d[y * w + x] * ((x as f32 - 50.0).powi(2))).sum();
        assert!((var - 36.0).abs() < 4.0, "{var}");
    }

    #[test]
    fn transparent_neighbours_do_not_darken_colors() {
        // a red square on transparency: blurred colors stay red, only alpha fades
        let (w, h) = (40, 40);
        let mut img = vec![0u8; w * h * 4];
        for y in 10..30 {
            for x in 10..30 {
                img[(y * w + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let out = apply(w, h, &img, &Filter::GaussianBlur { radius: 3.0 }, (0, 0));
        let p = at(&out, w, 9, 20);
        assert!(p[3] > 0 && p[3] < 255 && p[0] >= 254 && p[1] <= 1, "{p:?}");
    }

    #[test]
    fn motion_blur_smears_along_its_angle_only() {
        let (w, h) = (60, 60);
        let mut img = solid(w, h, [0, 0, 0, 255]);
        img[(30 * w + 30) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
        let out = apply(w, h, &img, &Filter::MotionBlur { angle: 0.0, distance: 10.0 }, (0, 0));
        assert!(at(&out, w, 34, 30)[0] > 0, "smeared horizontally");
        assert_eq!(at(&out, w, 30, 34)[0], 0, "not vertically");
        let out = apply(w, h, &img, &Filter::MotionBlur { angle: 90.0, distance: 10.0 }, (0, 0));
        assert!(at(&out, w, 30, 34)[0] > 0 && at(&out, w, 34, 30)[0] == 0);
    }

    #[test]
    fn unsharp_mask_raises_edge_contrast_and_respects_the_threshold() {
        let (w, h) = (40, 10);
        let img: Vec<u8> = (0..w * h).flat_map(|i| if i % w < 20 { [100, 100, 100, 255] } else { [150, 150, 150, 255] }).collect();
        let out = apply(w, h, &img, &Filter::UnsharpMask { amount: 100.0, radius: 2.0, threshold: 0.0 }, (0, 0));
        assert!(at(&out, w, 19, 5)[0] < 100 && at(&out, w, 20, 5)[0] > 150, "edges overshoot");
        assert_eq!(at(&out, w, 2, 5)[0], 100, "flat areas stay");
        // a threshold above the edge difference leaves everything alone
        let out = apply(w, h, &img, &Filter::UnsharpMask { amount: 300.0, radius: 2.0, threshold: 60.0 }, (0, 0));
        assert_eq!(out, img);
    }

    #[test]
    fn noise_is_repeatable_and_tied_to_image_coordinates() {
        let img = solid(32, 32, [128, 128, 128, 255]);
        let f = Filter::AddNoise { amount: 20.0, gaussian: true, monochromatic: false, seed: 7 };
        let a = apply(32, 32, &img, &f, (100, 50));
        assert_eq!(a, apply(32, 32, &img, &f, (100, 50)));
        // the same pixel computed as part of a shifted region gets the same value
        let b = apply(32, 32, &img, &f, (96, 50));
        assert_eq!(at(&a, 32, 0, 0), at(&b, 32, 4, 0));
        assert_ne!(a, img);
        // monochromatic noise keeps pixels gray; the alpha never changes
        let m = apply(32, 32, &img, &Filter::AddNoise { amount: 50.0, gaussian: false, monochromatic: true, seed: 1 }, (0, 0));
        assert!(m.chunks_exact(4).all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255));
        // the mean stays put
        let mean = a.chunks_exact(4).map(|p| p[0] as f32).sum::<f32>() / 1024.0;
        assert!((mean - 128.0).abs() < 4.0, "{mean}");
    }

    #[test]
    fn pixelate_averages_cells_aligned_to_the_origin() {
        let (w, h) = (8, 4);
        let img: Vec<u8> = (0..w * h).flat_map(|i| [((i % w) * 30) as u8, 0, 0, 255]).collect();
        // cells of 4 starting at image x = 2: the region's columns 0–1 belong to the cell 0..4
        let out = apply(w, h, &img, &Filter::Pixelate { cell: 4.0 }, (2, 0));
        assert_eq!(at(&out, w, 0, 0)[0], at(&out, w, 1, 0)[0]);
        assert_eq!(at(&out, w, 2, 0)[0], ((60 + 90 + 120 + 150) / 4) as u8);
        assert_eq!(at(&out, w, 2, 3), at(&out, w, 5, 0));
    }

    #[test]
    fn scaled_filters_shrink_distances_only() {
        assert_eq!(Filter::GaussianBlur { radius: 8.0 }.scaled(0.25), Filter::GaussianBlur { radius: 2.0 });
        assert_eq!(Filter::Pixelate { cell: 3.0 }.scaled(0.25), Filter::Pixelate { cell: 1.0 });
        let n = Filter::AddNoise { amount: 10.0, gaussian: false, monochromatic: false, seed: 3 };
        assert_eq!(n.scaled(0.5), n);
        assert_eq!(Filter::GaussianBlur { radius: 10.0 }.reach(), 31);
    }

    #[test]
    fn tiles_with_a_margin_join_without_seams() {
        // filtering a crop with `reach` margin gives the same center as filtering the whole image
        let (w, h) = (120, 90);
        let img: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7 % 256) as u8, (i * 13 % 256) as u8, (i / 3 % 256) as u8, if i % 17 == 0 { 0 } else { 255 }]).collect();
        for f in [Filter::GaussianBlur { radius: 4.0 }, Filter::MotionBlur { angle: 35.0, distance: 9.0 }, Filter::UnsharpMask { amount: 150.0, radius: 3.0, threshold: 2.0 }, Filter::Pixelate { cell: 6.0 }, Filter::AddNoise { amount: 30.0, gaussian: true, monochromatic: false, seed: 9 }] {
            let whole = apply(w, h, &img, &f, (0, 0));
            let r = f.reach() as usize;
            let (x0, y0, cw, ch) = (40usize, 30usize, 30usize, 20usize);
            let (ax, ay) = (x0 - r.min(x0), y0 - r.min(y0));
            let (bx, by) = ((x0 + cw + r).min(w), (y0 + ch + r).min(h));
            let crop: Vec<u8> = (ay..by).flat_map(|y| img[(y * w + ax) * 4..(y * w + bx) * 4].to_vec()).collect();
            let part = apply(bx - ax, by - ay, &crop, &f, (ax as i64, ay as i64));
            for y in 0..ch {
                for x in 0..cw {
                    let a = at(&whole, w, x0 + x, y0 + y);
                    let b = at(&part, bx - ax, x0 - ax + x, y0 - ay + y);
                    assert!((0..4).all(|k| (a[k] as i32 - b[k] as i32).abs() <= 1), "{f:?} at {x},{y}: {a:?} vs {b:?}");
                }
            }
        }
    }
}
