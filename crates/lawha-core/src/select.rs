//! Selection algorithms on dense masks. Sources for each technique are in docs/research.md §4.

use rayon::prelude::*;

use crate::mask::Dense;

/// How a new shape combines with the existing selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Combine {
    Replace,
    Add,
    Subtract,
    Intersect,
}

pub fn combine(dst: &mut Dense, src: &Dense, mode: Combine) {
    match mode {
        Combine::Replace => dst.data.copy_from_slice(&src.data),
        Combine::Add => dst.data.par_iter_mut().zip(&src.data).for_each(|(d, s)| *d = (*d).max(*s)),
        Combine::Subtract => dst.data.par_iter_mut().zip(&src.data).for_each(|(d, s)| *d = ((*d as u32 * (255 - *s as u32) + 127) / 255) as u8),
        Combine::Intersect => dst.data.par_iter_mut().zip(&src.data).for_each(|(d, s)| *d = (*d).min(*s)),
    }
}

pub fn invert(d: &mut Dense) {
    d.data.par_iter_mut().for_each(|v| *v = 255 - *v);
}

#[inline]
fn cov(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Axis-aligned rectangle; pixel coverage is the exact overlap area (anti-aliased edges for
/// fractional coordinates, hard edges for whole pixels).
pub fn rect(w: u32, h: u32, x0: f64, y0: f64, x1: f64, y1: f64) -> Dense {
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));
    let mut d = Dense::new(w, h, 0);
    let ox = |i: usize| (x1.min(i as f64 + 1.0) - x0.max(i as f64)).max(0.0);
    let oy = |j: usize| (y1.min(j as f64 + 1.0) - y0.max(j as f64)).max(0.0);
    let (ix0, ix1) = (x0.floor().max(0.0) as usize, (x1.ceil().max(0.0) as usize).min(w as usize));
    let (iy0, iy1) = (y0.floor().max(0.0) as usize, (y1.ceil().max(0.0) as usize).min(h as usize));
    d.data.par_chunks_mut(w as usize).enumerate().skip(iy0).take(iy1.saturating_sub(iy0)).for_each(|(j, row)| {
        let fy = oy(j);
        for (i, v) in row.iter_mut().enumerate().take(ix1).skip(ix0) {
            *v = cov(ox(i) * fy);
        }
    });
    d
}

/// Ellipse inscribed in the given box. Interior pixels are filled directly; pixels near the edge
/// are supersampled 8×8 when `antialias`, or sampled at their center otherwise.
pub fn ellipse(w: u32, h: u32, x0: f64, y0: f64, x1: f64, y1: f64, antialias: bool) -> Dense {
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (rx, ry) = (((x1 - x0) / 2.0).abs().max(1e-6), ((y1 - y0) / 2.0).abs().max(1e-6));
    let mut d = Dense::new(w, h, 0);
    let inside = |x: f64, y: f64| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
    let iy0 = (cy - ry).floor().max(0.0) as usize;
    let iy1 = ((cy + ry).ceil().max(0.0) as usize).min(h as usize);
    d.data.par_chunks_mut(w as usize).enumerate().skip(iy0).take(iy1.saturating_sub(iy0)).for_each(|(j, row)| {
        for (i, v) in row.iter_mut().enumerate() {
            let (px, py) = (i as f64 + 0.5, j as f64 + 0.5);
            let r = (((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2)).sqrt();
            let edge = 1.5 / rx.min(ry);
            if r < 1.0 - edge {
                *v = 255;
            } else if r > 1.0 + edge {
                continue;
            } else if antialias {
                let mut n = 0;
                for sy in 0..8 {
                    for sx in 0..8 {
                        if inside(i as f64 + (sx as f64 + 0.5) / 8.0, j as f64 + (sy as f64 + 0.5) / 8.0) {
                            n += 1;
                        }
                    }
                }
                *v = cov(n as f64 / 64.0);
            } else if inside(px, py) {
                *v = 255;
            }
        }
    });
    d
}

/// Polygon (lasso) with the non-zero winding rule. Anti-aliased: 4 sub-scanlines per pixel row with
/// exact horizontal span coverage.
pub fn polygon(w: u32, h: u32, pts: &[(f64, f64)], antialias: bool) -> Dense {
    let mut d = Dense::new(w, h, 0);
    if pts.len() < 3 {
        return d;
    }
    let edges: Vec<((f64, f64), (f64, f64))> = (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect();
    let ymin = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
    let ymax = (pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).ceil().max(0.0) as usize).min(h as usize);
    let subs: &[f64] = if antialias { &[0.125, 0.375, 0.625, 0.875] } else { &[0.5] };
    d.data.par_chunks_mut(w as usize).enumerate().skip(ymin).take(ymax.saturating_sub(ymin)).for_each(|(j, row)| {
        let mut acc = vec![0f64; w as usize + 1];
        for &sy in subs {
            let y = j as f64 + sy;
            let mut xs: Vec<(f64, i32)> = Vec::new();
            for &((x0, y0), (x1, y1)) in &edges {
                if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                    let x = x0 + (y - y0) / (y1 - y0) * (x1 - x0);
                    xs.push((x, if y1 > y0 { 1 } else { -1 }));
                }
            }
            xs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            let mut wind = 0;
            for k in 0..xs.len() {
                let was = wind != 0;
                wind += xs[k].1;
                if !was && wind != 0 {
                    // span starts at xs[k].0; find where it ends
                    let start = xs[k].0;
                    let mut end = start;
                    let mut wnd = wind;
                    for x in xs.iter().skip(k + 1) {
                        wnd += x.1;
                        if wnd == 0 {
                            end = x.0;
                            break;
                        }
                    }
                    add_span(&mut acc, start, end, if antialias { 0.25 } else { 1.0 }, antialias);
                }
            }
        }
        for (v, a) in row.iter_mut().zip(acc.iter()) {
            *v = cov(*a);
        }
    });
    d
}

fn add_span(acc: &mut [f64], a: f64, b: f64, weight: f64, antialias: bool) {
    let w = acc.len() - 1;
    let (a, b) = (a.max(0.0), b.min(w as f64));
    if b <= a {
        return;
    }
    if !antialias {
        // pixel centers inside [a, b)
        let i0 = (a - 0.5).ceil().max(0.0) as usize;
        let i1 = ((b - 0.5).ceil().max(0.0) as usize).min(w);
        for v in acc.iter_mut().take(i1).skip(i0) {
            *v += weight;
        }
        return;
    }
    let (ia, ib) = (a.floor() as usize, b.floor() as usize);
    if ia == ib {
        acc[ia] += (b - a) * weight;
        return;
    }
    acc[ia] += (ia as f64 + 1.0 - a) * weight;
    for v in acc.iter_mut().take(ib).skip(ia + 1) {
        *v += weight;
    }
    if ib < w {
        acc[ib] += (b - ib as f64) * weight;
    }
}

/// Magic wand: pixels whose color is within `tolerance` (0–255, per channel, alpha included) of
/// the seed pixel. `rgba` is the sampled image (straight alpha).
pub fn wand(w: u32, h: u32, rgba: &[u8], seed: (u32, u32), tolerance: u8, contiguous: bool, antialias: bool) -> Dense {
    let (wu, hu) = (w as usize, h as usize);
    let mut d = Dense::new(w, h, 0);
    if seed.0 >= w || seed.1 >= h {
        return d;
    }
    let si = (seed.1 as usize * wu + seed.0 as usize) * 4;
    let s = [rgba[si], rgba[si + 1], rgba[si + 2], rgba[si + 3]];
    let t = tolerance as i32;
    let near = |i: usize| -> bool {
        let p = &rgba[i * 4..i * 4 + 4];
        // fully transparent pixels match each other whatever their stored color
        if p[3] == 0 && s[3] == 0 {
            return true;
        }
        (0..4).all(|k| (p[k] as i32 - s[k] as i32).abs() <= t)
    };
    if contiguous {
        // scanline flood fill (4-connected)
        let mut stack = vec![(seed.0 as usize, seed.1 as usize)];
        while let Some((x, y)) = stack.pop() {
            let row = y * wu;
            if d.data[row + x] != 0 || !near(row + x) {
                continue;
            }
            let mut l = x;
            while l > 0 && d.data[row + l - 1] == 0 && near(row + l - 1) {
                l -= 1;
            }
            let mut r = x;
            while r + 1 < wu && d.data[row + r + 1] == 0 && near(row + r + 1) {
                r += 1;
            }
            for i in l..=r {
                d.data[row + i] = 255;
            }
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny >= hu {
                    continue;
                }
                let nrow = ny * wu;
                let mut i = l;
                while i <= r {
                    if d.data[nrow + i] == 0 && near(nrow + i) {
                        stack.push((i, ny));
                        while i <= r && d.data[nrow + i] == 0 && near(nrow + i) {
                            i += 1;
                        }
                    }
                    i += 1;
                }
            }
        }
    } else {
        d.data.par_iter_mut().enumerate().for_each(|(i, v)| {
            if near(i) {
                *v = 255;
            }
        });
    }
    if antialias {
        soften_edges(&mut d);
    }
    d
}

/// Turns a hard edge into a one-pixel ramp (3×3 tent filter on edge pixels only).
pub fn soften_edges(d: &mut Dense) {
    let (w, h) = (d.w as usize, d.h as usize);
    let src = d.data.clone();
    d.data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let c = src[y * w + x];
            let mut edge = false;
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && src[ny as usize * w + nx as usize] != c {
                    edge = true;
                }
            }
            if !edge {
                continue;
            }
            let mut sum = 0u32;
            let mut wsum = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let k = (2 - dx.abs()) as u32 * (2 - dy.abs()) as u32;
                    sum += src[ny as usize * w + nx as usize] as u32 * k;
                    wsum += k;
                }
            }
            row[x] = ((sum + wsum / 2) / wsum) as u8;
        }
    });
}

// ---- distance transform (Felzenszwalb & Huttenlocher) ----

const INF: f32 = 1e20;

fn dt1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    let mut k = 0;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for q in 1..n {
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        if s <= z[k] {
            // k == 0 and the new parabola dominates
            v[0] = q;
            z[0] = -INF;
            z[1] = INF;
            continue;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = INF;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        *out = (q as f32 - p as f32).powi(2) + f[p];
    }
}

/// Exact Euclidean distance (in pixels) from every pixel to the nearest pixel where `is_source`.
pub fn distance(w: u32, h: u32, is_source: impl Fn(usize) -> bool + Sync) -> Vec<f32> {
    let (w, h) = (w as usize, h as usize);
    let mut g: Vec<f32> = (0..w * h).into_par_iter().map(|i| if is_source(i) { 0.0 } else { INF }).collect();
    // columns
    let mut cols = vec![0f32; w * h];
    cols.par_chunks_mut(h).enumerate().for_each(|(x, out)| {
        let f: Vec<f32> = (0..h).map(|y| g[y * w + x]).collect();
        let (mut v, mut z) = (vec![0usize; h], vec![0f32; h + 1]);
        dt1d(&f, out, &mut v, &mut z);
    });
    // rows
    g.par_chunks_mut(w).enumerate().for_each(|(y, out)| {
        let f: Vec<f32> = (0..w).map(|x| cols[x * h + y]).collect();
        let (mut v, mut z) = (vec![0usize; w], vec![0f32; w + 1]);
        dt1d(&f, out, &mut v, &mut z);
    });
    g.par_iter_mut().for_each(|d| *d = d.sqrt());
    g
}

/// Grows the selection by `r` px (soft one-pixel edge).
pub fn expand(d: &Dense, r: f32) -> Dense {
    let dist = distance(d.w, d.h, |i| d.data[i] >= 128);
    let data = dist.par_iter().zip(&d.data).map(|(&dd, &v)| v.max(cov((r + 1.0 - dd) as f64))).collect();
    Dense { w: d.w, h: d.h, data }
}

/// Shrinks the selection by `r` px. The canvas border is not an edge (unselected space is only
/// what lies inside the canvas), matching the documented behavior.
pub fn contract(d: &Dense, r: f32) -> Dense {
    let dist = distance(d.w, d.h, |i| d.data[i] < 128);
    let data = dist.par_iter().zip(&d.data).map(|(&dd, &v)| v.min(cov((dd - r) as f64))).collect();
    Dense { w: d.w, h: d.h, data }
}

/// A band of width `width` px centered on the selection edge.
pub fn border(d: &Dense, width: f32) -> Dense {
    let out = distance(d.w, d.h, |i| d.data[i] >= 128);
    let inn = distance(d.w, d.h, |i| d.data[i] < 128);
    let half = width / 2.0;
    let data = out.par_iter().zip(&inn).map(|(&o, &i)| {
        let sd = if o > 0.0 { o } else { i }; // distance to the edge on either side
        cov((half - sd + 0.5) as f64)
    }).collect();
    Dense { w: d.w, h: d.h, data }
}

// ---- blurs ----

/// Box blur of radius r (window 2r+1), horizontal then vertical, on f32 data.
fn box_blur(w: usize, h: usize, data: &mut [f32], r: usize) {
    if r == 0 {
        return;
    }
    let tmp: Vec<f32> = data.to_vec();
    data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let src = &tmp[y * w..(y + 1) * w];
        let mut acc = 0f32;
        let n = (2 * r + 1) as f32;
        for i in 0..=r {
            acc += src[i.min(w - 1)];
        }
        acc += src[0] * r as f32;
        for x in 0..w {
            row[x] = acc / n;
            let add = src[(x + r + 1).min(w - 1)];
            let sub = src[x.saturating_sub(r)];
            acc += add - sub;
        }
    });
    let tmp: Vec<f32> = data.to_vec();
    let mut cols = vec![0f32; w * h];
    cols.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        let at = |y: usize| tmp[y.min(h - 1) * w + x];
        let mut acc = 0f32;
        let n = (2 * r + 1) as f32;
        for i in 0..=r {
            acc += at(i);
        }
        acc += at(0) * r as f32;
        for (y, c) in col.iter_mut().enumerate() {
            *c = acc / n;
            acc += at(y + r + 1) - at(y.saturating_sub(r));
        }
    });
    data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = cols[x * h + y];
        }
    });
}

/// Gaussian blur with standard deviation `sigma`, approximated by three box passes (Kovesi).
pub fn gaussian(w: usize, h: usize, data: &mut [f32], sigma: f32) {
    if sigma <= 0.0 {
        return;
    }
    let n = 3.0f32;
    let wi = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = wi.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round() as i32;
    for i in 0..3 {
        let width = if i < m { wl } else { wu };
        box_blur(w, h, data, ((width - 1) / 2).max(0) as usize);
    }
}

/// Feathers the selection edge. The radius maps to a Gaussian σ of radius / 2 (✱ docs/research.md).
pub fn feather(d: &Dense, radius: f32) -> Dense {
    let mut f: Vec<f32> = d.data.par_iter().map(|v| *v as f32).collect();
    gaussian(d.w as usize, d.h as usize, &mut f, radius / 2.0);
    Dense { w: d.w, h: d.h, data: f.par_iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect() }
}

/// Majority smoothing: averages a (2r+1)² window and re-thresholds with a soft one-pixel ramp.
pub fn smooth(d: &Dense, r: u32) -> Dense {
    let mut f: Vec<f32> = d.data.par_iter().map(|v| *v as f32 / 255.0).collect();
    box_blur(d.w as usize, d.h as usize, &mut f, r as usize);
    Dense { w: d.w, h: d.h, data: f.par_iter().map(|v| cov(((v - 0.5) * 4.0 + 0.5) as f64)).collect() }
}

// ---- refine edge ----

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refine {
    /// Edge band, px: the area around the edge where the guided filter recomputes coverage.
    pub radius: f32,
    /// Majority smoothing radius, px.
    pub smooth: u32,
    /// Feather, px.
    pub feather: f32,
    /// 0–100: steepens the edge ramp.
    pub contrast: f32,
    /// −100–100: moves the edge out (positive) or in (negative), as % of the radius.
    pub shift: f32,
}

/// Guided filter (He, Sun, Tang) of `p` with guide `g`, window radius `r`, regularization `eps`.
pub fn guided_filter(w: usize, h: usize, g: &[f32], p: &[f32], r: usize, eps: f32) -> Vec<f32> {
    let n = w * h;
    let mut mean_i = g.to_vec();
    box_blur(w, h, &mut mean_i, r);
    let mut mean_p = p.to_vec();
    box_blur(w, h, &mut mean_p, r);
    let mut corr_ip: Vec<f32> = (0..n).into_par_iter().map(|i| g[i] * p[i]).collect();
    box_blur(w, h, &mut corr_ip, r);
    let mut corr_ii: Vec<f32> = (0..n).into_par_iter().map(|i| g[i] * g[i]).collect();
    box_blur(w, h, &mut corr_ii, r);
    let mut a: Vec<f32> = (0..n)
        .into_par_iter()
        .map(|i| {
            let var = corr_ii[i] - mean_i[i] * mean_i[i];
            let cov = corr_ip[i] - mean_i[i] * mean_p[i];
            cov / (var + eps)
        })
        .collect();
    let mut b: Vec<f32> = (0..n).into_par_iter().map(|i| mean_p[i] - a[i] * mean_i[i]).collect();
    box_blur(w, h, &mut a, r);
    box_blur(w, h, &mut b, r);
    (0..n).into_par_iter().map(|i| a[i] * g[i] + b[i]).collect()
}

/// Refines a selection against the image (`rgba`, straight alpha): recomputes coverage near the
/// edge with a guided filter on the image luminance, then smooths, feathers, sharpens and shifts.
pub fn refine(d: &Dense, rgba: &[u8], p: &Refine) -> Dense {
    let (w, h) = (d.w as usize, d.h as usize);
    let n = w * h;
    let guide: Vec<f32> = (0..n).into_par_iter().map(|i| {
        let c = &rgba[i * 4..i * 4 + 4];
        (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0
    }).collect();
    let src: Vec<f32> = d.data.par_iter().map(|v| *v as f32 / 255.0).collect();
    let mut out = src.clone();
    if p.radius >= 1.0 {
        let r = (p.radius.round() as usize).max(1);
        // only inside the band around the edge
        let edge_out = distance(d.w, d.h, |i| d.data[i] >= 128);
        let edge_in = distance(d.w, d.h, |i| d.data[i] < 128);
        let band: Vec<bool> = (0..n).into_par_iter().map(|i| edge_out[i].max(edge_in[i]) <= p.radius).collect();
        // Iterated guided filter: each pass pulls the transition further onto the image's own
        // edges; a soft re-threshold inside the band keeps the result from washing out.
        for _ in 0..3 {
            let filtered = guided_filter(w, h, &guide, &out, r, 1e-3);
            out.par_iter_mut().enumerate().for_each(|(i, v)| {
                if band[i] {
                    let x = filtered[i].clamp(0.0, 1.0);
                    *v = x * x * (3.0 - 2.0 * x); // smoothstep
                }
            });
        }
    }
    let mut res = Dense { w: d.w, h: d.h, data: out.par_iter().map(|v| cov(*v as f64)).collect() };
    if p.smooth > 0 {
        res = smooth(&res, p.smooth);
    }
    if p.feather > 0.0 {
        res = feather(&res, p.feather);
    }
    if p.contrast > 0.0 || p.shift != 0.0 {
        let k = 1.0 + p.contrast / 100.0 * 9.0;
        let s = p.shift / 100.0 * 0.5;
        res.data.par_iter_mut().for_each(|v| {
            let x = *v as f64 / 255.0 + s as f64;
            *v = cov((x - 0.5) * k as f64 + 0.5);
        });
    }
    res
}

/// Replaces the colors of partly selected edge pixels with nearby fully selected colors, so a cut-out
/// doesn't carry a fringe of the old background ("decontaminate colors").
pub fn decontaminate(rgba: &mut [u8], d: &Dense, radius: f32) {
    let (w, h) = (d.w as usize, d.h as usize);
    let n = w * h;
    // normalized convolution of the colors of confident foreground pixels
    let weight: Vec<f32> = d.data.par_iter().map(|v| if *v >= 242 { 1.0 } else { 0.0 }).collect();
    let mut wsum = weight.clone();
    gaussian(w, h, &mut wsum, radius.max(1.0));
    for c in 0..3 {
        let mut ch: Vec<f32> = (0..n).into_par_iter().map(|i| rgba[i * 4 + c] as f32 * weight[i]).collect();
        gaussian(w, h, &mut ch, radius.max(1.0));
        rgba.par_chunks_mut(4).enumerate().for_each(|(i, px)| {
            let a = d.data[i];
            if a > 0 && a < 242 && wsum[i] > 1e-3 {
                px[c] = (ch[i] / wsum[i]).round().clamp(0.0, 255.0) as u8;
            }
        });
    }
}

// ---- quick selection (classic, no AI) ----

fn kmeans(samples: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    if samples.is_empty() {
        return Vec::new();
    }
    let k = k.min(samples.len());
    // deterministic init: spread over the sample list
    let mut c: Vec<[f32; 3]> = (0..k).map(|i| samples[i * samples.len() / k]).collect();
    for _ in 0..8 {
        let mut sum = vec![[0f32; 4]; k];
        for s in samples {
            let j = nearest(&c, s).0;
            for t in 0..3 {
                sum[j][t] += s[t];
            }
            sum[j][3] += 1.0;
        }
        for (cj, sj) in c.iter_mut().zip(&sum) {
            if sj[3] > 0.0 {
                *cj = [sj[0] / sj[3], sj[1] / sj[3], sj[2] / sj[3]];
            }
        }
    }
    c
}

fn nearest(c: &[[f32; 3]], s: &[f32; 3]) -> (usize, f32) {
    let mut best = (0, f32::INFINITY);
    for (j, cj) in c.iter().enumerate() {
        let d = (cj[0] - s[0]).powi(2) + (cj[1] - s[1]).powi(2) + (cj[2] - s[2]).powi(2);
        if d < best.1 {
            best = (j, d);
        }
    }
    best
}

/// Quick selection from a brush stroke: learns the colors under the stroke, grows from the stroke
/// through connected pixels whose color is close to them (an adaptive threshold from the stroke's
/// own color spread), within a reach around the stroke, then snaps the edge to the image. Returns
/// the new region only; the caller combines it with the selection.
pub fn quick_select(w: u32, h: u32, rgba: &[u8], stroke: &[(f64, f64)], radius: f64) -> Dense {
    let (wu, hu) = (w as usize, h as usize);
    let mut out = Dense::new(w, h, 0);
    if stroke.is_empty() {
        return out;
    }
    let reach = (radius * 12.0).max(64.0);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in stroke {
        x0 = x0.min(x - reach);
        y0 = y0.min(y - reach);
        x1 = x1.max(x + reach);
        y1 = y1.max(y + reach);
    }
    let (rx0, ry0) = (x0.max(0.0) as usize, y0.max(0.0) as usize);
    let (rx1, ry1) = ((x1.ceil() as usize).min(wu), (y1.ceil() as usize).min(hu));
    if rx1 <= rx0 || ry1 <= ry0 {
        return out;
    }
    let color = |x: usize, y: usize| {
        let p = &rgba[(y * wu + x) * 4..][..4];
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    // distance of each ROI pixel to the stroke (polyline of discs)
    let dist_to_stroke = |x: f64, y: f64| -> f64 {
        let mut best = f64::INFINITY;
        for seg in stroke.windows(2).chain(std::iter::once(&stroke[stroke.len() - 1..])) {
            let (ax, ay) = seg[0];
            let (bx, by) = *seg.last().unwrap();
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 { (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            best = best.min(((x - ax - t * dx).powi(2) + (y - ay - t * dy).powi(2)).sqrt());
        }
        best
    };
    let (rw, rh) = (rx1 - rx0, ry1 - ry0);
    let ds: Vec<f64> = (0..rw * rh).into_par_iter().map(|i| dist_to_stroke((rx0 + i % rw) as f64 + 0.5, (ry0 + i / rw) as f64 + 0.5)).collect();
    // colors under the stroke → a few cluster centers and their spread
    let mut fg = Vec::new();
    let step = ((rw * rh) as f64 / 20000.0).sqrt().max(1.0) as usize;
    for yy in (0..rh).step_by(step) {
        for xx in (0..rw).step_by(step) {
            if ds[yy * rw + xx] <= radius {
                fg.push(color(rx0 + xx, ry0 + yy));
            }
        }
    }
    let fgc = kmeans(&fg, 4);
    let spread = if fg.is_empty() { 0.0 } else { fg.iter().map(|c| nearest(&fgc, c).1).sum::<f32>() / fg.len() as f32 };
    let limit = (9.0 * spread).max(30.0 * 30.0);
    // grow from the stroke through pixels that look like it, staying near the stroke (the ROI)
    let looks_fg: Vec<bool> = (0..rw * rh).into_par_iter().map(|i| ds[i] <= radius || nearest(&fgc, &color(rx0 + i % rw, ry0 + i / rw)).1 < limit).collect();
    let mut keep = vec![false; rw * rh];
    let mut stack: Vec<usize> = (0..rw * rh).filter(|&i| ds[i] <= radius).collect();
    for &i in &stack {
        keep[i] = true;
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % rw, i / rw);
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if nx < rw && ny < rh {
                let j = ny * rw + nx;
                if !keep[j] && looks_fg[j] {
                    keep[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    for yy in 0..rh {
        for xx in 0..rw {
            if keep[yy * rw + xx] {
                out.data[(ry0 + yy) * wu + rx0 + xx] = 255;
            }
        }
    }
    // tidy: fill pinholes and snap the edge to the image
    let tidy = smooth(&out, 1);
    refine(&tidy, rgba, &Refine { radius: (radius / 4.0).clamp(2.0, 8.0) as f32, smooth: 0, feather: 0.0, contrast: 0.0, shift: 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(d: &Dense) -> f64 {
        d.data.iter().map(|v| *v as f64 / 255.0).sum()
    }

    #[test]
    fn rect_area_and_fractional_edges() {
        let d = rect(100, 100, 10.0, 20.0, 60.0, 50.0);
        assert_eq!(area(&d), 1500.0);
        assert_eq!(d.at(10, 20), 255);
        assert_eq!(d.at(9, 20), 0);
        let f = rect(10, 10, 2.5, 2.0, 5.0, 4.0);
        assert_eq!(f.at(2, 2), 128); // half covered
        assert!((area(&f) - 5.0).abs() < 0.02);
    }

    #[test]
    fn ellipse_area_matches_pi_ab() {
        let d = ellipse(400, 300, 50.0, 40.0, 350.0, 260.0, true);
        let want = std::f64::consts::PI * 150.0 * 110.0;
        assert!((area(&d) - want).abs() / want < 0.002, "{} vs {want}", area(&d));
        assert!(d.data.iter().any(|v| *v > 0 && *v < 255), "anti-aliased edge");
        let hard = ellipse(400, 300, 50.0, 40.0, 350.0, 260.0, false);
        assert!(hard.data.iter().all(|v| *v == 0 || *v == 255));
    }

    #[test]
    fn polygon_area_and_winding() {
        let tri = polygon(200, 200, &[(10.0, 10.0), (110.0, 10.0), (10.0, 110.0)], true);
        assert!((area(&tri) - 5000.0).abs() < 20.0, "{}", area(&tri));
        // a self-overlapping star still covers its center (non-zero winding)
        let star: Vec<(f64, f64)> = (0..5)
            .map(|k| {
                let a = std::f64::consts::PI * 2.0 * (k as f64 * 2.0) / 5.0 - std::f64::consts::FRAC_PI_2;
                (100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin())
            })
            .collect();
        let s = polygon(200, 200, &star, true);
        assert_eq!(s.at(100, 100), 255);
    }

    #[test]
    fn combine_modes() {
        let a = rect(10, 1, 0.0, 0.0, 6.0, 1.0);
        let b = rect(10, 1, 4.0, 0.0, 10.0, 1.0);
        let mut d = a.clone();
        combine(&mut d, &b, Combine::Add);
        assert_eq!(area(&d), 10.0);
        let mut d = a.clone();
        combine(&mut d, &b, Combine::Subtract);
        assert_eq!(area(&d), 4.0);
        let mut d = a.clone();
        combine(&mut d, &b, Combine::Intersect);
        assert_eq!(area(&d), 2.0);
    }

    #[test]
    fn wand_contiguous_vs_global() {
        // two red squares separated by blue
        let (w, h) = (30u32, 10u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let red = x < 10 || x >= 20;
                img[(y * w as usize + x) * 4..][..4].copy_from_slice(if red { &[250, 10, 10, 255] } else { &[10, 10, 250, 255] });
            }
        }
        let c = wand(w, h, &img, (2, 2), 32, true, false);
        assert_eq!(area(&c), 100.0);
        let g = wand(w, h, &img, (2, 2), 32, false, false);
        assert_eq!(area(&g), 200.0);
        let none = wand(w, h, &img, (2, 2), 0, true, false);
        assert_eq!(area(&none), 100.0, "exact match only");
    }

    #[test]
    fn distance_transform_is_exact() {
        let (w, h) = (64u32, 48u32);
        let src = [(10usize, 7usize), (50, 40), (33, 20)];
        let d = distance(w, h, |i| src.contains(&(i % 64, i / 64)));
        for y in 0..h as usize {
            for x in 0..w as usize {
                let want = src.iter().map(|&(sx, sy)| (((x as f64 - sx as f64).powi(2) + (y as f64 - sy as f64).powi(2)) as f64).sqrt()).fold(f64::INFINITY, f64::min);
                assert!((d[y * 64 + x] as f64 - want).abs() < 1e-3, "({x},{y})");
            }
        }
    }

    #[test]
    fn expand_and_contract_move_the_edge() {
        let d = rect(100, 100, 30.0, 30.0, 70.0, 70.0);
        let e = expand(&d, 5.0);
        assert_eq!(e.at(25, 50), 255);
        assert_eq!(e.at(23, 50), 0);
        let c = contract(&d, 5.0);
        assert_eq!(c.at(35, 50), 255);
        assert_eq!(c.at(33, 50), 0);
        // a full selection doesn't shrink away from the canvas border
        let all = Dense::new(50, 50, 255);
        assert_eq!(contract(&all, 10.0).data, all.data);
    }

    #[test]
    fn feather_softens_and_preserves_area() {
        let d = rect(200, 200, 50.0, 50.0, 150.0, 150.0);
        let f = feather(&d, 10.0);
        assert!((area(&f) - area(&d)).abs() / area(&d) < 0.01);
        assert!(f.at(50, 100) > 64 && f.at(50, 100) < 192);
        assert_eq!(f.at(100, 100), 255);
    }

    #[test]
    fn border_is_a_band() {
        let d = rect(100, 100, 30.0, 30.0, 70.0, 70.0);
        let b = border(&d, 6.0);
        assert_eq!(b.at(50, 50), 0);
        assert_eq!(b.at(30, 50), 255);
        assert_eq!(b.at(10, 50), 0);
    }

    #[test]
    fn quick_select_grows_into_the_stroked_region_only() {
        // a red disc on a noisy green background
        let (w, h) = (200u32, 200u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        for y in 0..200usize {
            for x in 0..200usize {
                let inside = ((x as f64 - 100.0).powi(2) + (y as f64 - 100.0).powi(2)).sqrt() < 60.0;
                let n = ((x * 7 + y * 13) % 23) as u8;
                let c = if inside { [200 + n / 2, 30 + n, 30, 255] } else { [40 + n, 160 + n, 50, 255] };
                img[(y * 200 + x) * 4..][..4].copy_from_slice(&c);
            }
        }
        let d = quick_select(w, h, &img, &[(90.0, 100.0), (110.0, 100.0)], 8.0);
        let disc = ellipse(w, h, 40.0, 40.0, 160.0, 160.0, true);
        let inter: f64 = d.data.iter().zip(&disc.data).map(|(a, b)| (*a).min(*b) as f64).sum();
        let uni: f64 = d.data.iter().zip(&disc.data).map(|(a, b)| (*a).max(*b) as f64).sum();
        assert!(inter / uni > 0.95, "IoU {}", inter / uni);
    }

    #[test]
    fn guided_filter_snaps_a_rough_edge_to_the_image() {
        // image: left half dark, right half bright; rough selection edge 4 px off
        let (w, h) = (80u32, 40u32);
        let mut img = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                let v = if x < 40 { 20 } else { 230 };
                img[(y * 80 + x) * 4..][..4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let rough = rect(w, h, 44.0, 0.0, 80.0, 40.0);
        let r = refine(&rough, &img, &Refine { radius: 8.0, smooth: 0, feather: 0.0, contrast: 0.0, shift: 0.0 });
        let ideal = rect(w, h, 40.0, 0.0, 80.0, 40.0);
        let err = |d: &Dense| d.data.iter().zip(&ideal.data).map(|(a, b)| (*a as f64 - *b as f64).abs()).sum::<f64>();
        assert!(err(&r) < err(&rough) * 0.6, "refined error {} vs rough {}", err(&r), err(&rough));
        assert!(r.at(41, 20) > 100, "the edge moves toward the image edge: {}", r.at(41, 20));
        assert!(r.at(37, 20) < 40, "the dark side stays out: {}", r.at(37, 20));
    }
}
