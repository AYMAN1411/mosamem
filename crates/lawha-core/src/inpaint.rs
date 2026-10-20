//! Filling a hole from the rest of the image: multi-scale PatchMatch (Barnes et al. 2009) with
//! EM-style patch voting, as described in docs/research.md §5.6. Used by spot healing and the
//! classic Smart Fill.

use rayon::prelude::*;

const P: i32 = 3; // patch radius (7×7 patches)

struct Level {
    w: usize,
    h: usize,
    /// RGB, 0–255
    img: Vec<[f32; 3]>,
    hole: Vec<bool>,
    /// patch centers whose whole patch is known (valid sources)
    source_ok: Vec<bool>,
}

fn build_source_ok(w: usize, h: usize, hole: &[bool]) -> Vec<bool> {
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            if x < P || y < P || x >= w as i32 - P || y >= h as i32 - P {
                return false;
            }
            for dy in -P..=P {
                for dx in -P..=P {
                    if hole[((y + dy) as usize) * w + (x + dx) as usize] {
                        return false;
                    }
                }
            }
            true
        })
        .collect()
}

fn downsample(l: &Level) -> Level {
    let (w, h) = (l.w.div_ceil(2), l.h.div_ceil(2));
    let mut img = vec![[0f32; 3]; w * h];
    let mut hole = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0f32; 3];
            let mut n = 0f32;
            let mut any_hole = false;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = (2 * x + dx, 2 * y + dy);
                if sx >= l.w || sy >= l.h {
                    continue;
                }
                let i = sy * l.w + sx;
                if l.hole[i] {
                    any_hole = true;
                } else {
                    for c in 0..3 {
                        s[c] += l.img[i][c];
                    }
                    n += 1.0;
                }
            }
            let j = y * w + x;
            hole[j] = any_hole;
            if n > 0.0 {
                img[j] = [s[0] / n, s[1] / n, s[2] / n];
            }
        }
    }
    let source_ok = build_source_ok(w, h, &hole);
    Level { w, h, img, hole, source_ok }
}

/// Deterministic per-pixel random numbers.
#[inline]
fn rnd(seed: u64, a: u64) -> u64 {
    let mut z = seed.wrapping_add(a.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn patch_dist(l: &Level, a: (i32, i32), b: (i32, i32), best: f32) -> f32 {
    let w = l.w as i32;
    let mut d = 0f32;
    for dy in -P..=P {
        let (ay, by) = (a.1 + dy, b.1 + dy);
        if ay < 0 || ay >= l.h as i32 {
            continue;
        }
        for dx in -P..=P {
            let ax = a.0 + dx;
            if ax < 0 || ax >= w {
                continue;
            }
            let pa = l.img[(ay * w + ax) as usize];
            let pb = l.img[(by * w + b.0 + dx) as usize];
            d += (pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2);
        }
        if d > best {
            return d;
        }
    }
    d
}

/// Fills the hole at one level: alternating PatchMatch passes and voting.
fn solve(l: &mut Level, nnf: &mut [(i32, i32)], seed: u64, iterations: usize) {
    let (w, h) = (l.w as i32, l.h as i32);
    let sources: Vec<usize> = (0..l.w * l.h).filter(|&i| l.source_ok[i]).collect();
    if sources.is_empty() {
        return;
    }
    // targets: patch centers overlapping the hole
    let targets: Vec<usize> = (0..l.w * l.h)
        .filter(|&i| {
            let (x, y) = ((i % l.w) as i32, (i / l.w) as i32);
            (-P..=P).any(|dy| (-P..=P).any(|dx| {
                let (sx, sy) = (x + dx, y + dy);
                sx >= 0 && sy >= 0 && sx < w && sy < h && l.hole[(sy * w + sx) as usize]
            }))
        })
        .collect();
    for (k, &t) in targets.iter().enumerate() {
        let cur = nnf[t];
        let valid = cur.0 >= 0 && cur.1 >= 0 && cur.0 < w && cur.1 < h && l.source_ok[(cur.1 * w + cur.0) as usize];
        if !valid {
            let s = sources[(rnd(seed, k as u64) % sources.len() as u64) as usize];
            nnf[t] = ((s % l.w) as i32, (s / l.w) as i32);
        }
    }
    for it in 0..iterations {
        // PatchMatch: propagation + random search, alternating scan order
        let order: Box<dyn Iterator<Item = &usize>> = if it % 2 == 0 { Box::new(targets.iter()) } else { Box::new(targets.iter().rev()) };
        let step = if it % 2 == 0 { -1 } else { 1 };
        for &t in order {
            let (x, y) = ((t % l.w) as i32, (t / l.w) as i32);
            let mut best = nnf[t];
            let mut bd = patch_dist(l, (x, y), best, f32::INFINITY);
            for (nx, ny) in [(x + step, y), (x, y + step)] {
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let n = nnf[(ny * w + nx) as usize];
                // the neighbour's match, shifted back by the same offset
                let cand = (n.0 + (x - nx), n.1 + (y - ny));
                if cand.0 >= 0 && cand.1 >= 0 && cand.0 < w && cand.1 < h && l.source_ok[(cand.1 * w + cand.0) as usize] {
                    let d = patch_dist(l, (x, y), cand, bd);
                    if d < bd {
                        bd = d;
                        best = cand;
                    }
                }
            }
            let mut radius = w.max(h);
            let mut r = 0u64;
            while radius >= 1 {
                r += 1;
                let rx = (rnd(seed ^ 0x55, (t as u64) << 8 | r | (it as u64) << 40) % (2 * radius as u64 + 1)) as i32 - radius;
                let ry = (rnd(seed ^ 0xAA, (t as u64) << 8 | r | (it as u64) << 40) % (2 * radius as u64 + 1)) as i32 - radius;
                let cand = (best.0 + rx, best.1 + ry);
                if cand.0 >= 0 && cand.1 >= 0 && cand.0 < w && cand.1 < h && l.source_ok[(cand.1 * w + cand.0) as usize] {
                    let d = patch_dist(l, (x, y), cand, bd);
                    if d < bd {
                        bd = d;
                        best = cand;
                    }
                }
                radius /= 2;
            }
            nnf[t] = best;
        }
        // voting: each hole pixel = average of what the overlapping patches propose
        let mut acc = vec![[0f32; 4]; l.w * l.h];
        for &t in &targets {
            let (x, y) = ((t % l.w) as i32, (t / l.w) as i32);
            let m = nnf[t];
            for dy in -P..=P {
                for dx in -P..=P {
                    let (px, py) = (x + dx, y + dy);
                    if px < 0 || py < 0 || px >= w || py >= h {
                        continue;
                    }
                    let i = (py * w + px) as usize;
                    if !l.hole[i] {
                        continue;
                    }
                    let s = l.img[((m.1 + dy) * w + m.0 + dx) as usize];
                    let a = &mut acc[i];
                    a[0] += s[0];
                    a[1] += s[1];
                    a[2] += s[2];
                    a[3] += 1.0;
                }
            }
        }
        for (i, a) in acc.iter().enumerate() {
            if l.hole[i] && a[3] > 0.0 {
                l.img[i] = [a[0] / a[3], a[1] / a[3], a[2] / a[3]];
            }
        }
    }
}

/// Fills the pixels where `hole` is true. `rgb` is RGB 0–255 (alpha is handled by the caller).
pub fn inpaint(w: usize, h: usize, rgb: &[[f32; 3]], hole: &[bool], seed: u64) -> Vec<[f32; 3]> {
    let mut levels = vec![Level { w, h, img: rgb.to_vec(), hole: hole.to_vec(), source_ok: build_source_ok(w, h, hole) }];
    while {
        let l = levels.last().unwrap();
        l.w > 32 && l.h > 32 && l.hole.iter().filter(|v| **v).count() > 64
    } {
        let next = downsample(levels.last().unwrap());
        levels.push(next);
    }
    // coarsest: start from a smooth fill of the hole
    {
        let l = levels.last_mut().unwrap();
        let mut vals: Vec<f32> = l.img.iter().flat_map(|p| p.iter().copied()).collect();
        crate::heal::membrane(l.w, l.h, 3, &mut vals, &l.hole);
        for (i, p) in l.img.iter_mut().enumerate() {
            *p = [vals[i * 3], vals[i * 3 + 1], vals[i * 3 + 2]];
        }
    }
    let mut nnf: Vec<(i32, i32)> = vec![(-1, -1); levels.last().unwrap().w * levels.last().unwrap().h];
    for li in (0..levels.len()).rev() {
        {
            let l = &mut levels[li];
            solve(l, &mut nnf, seed ^ li as u64, if li == 0 { 4 } else { 6 });
        }
        if li > 0 {
            // upsample the fill and the matches to the next finer level
            let (cw, img_c) = (levels[li].w, levels[li].img.clone());
            let ch = levels[li].h;
            let fine = &mut levels[li - 1];
            let mut fnnf = vec![(-1, -1); fine.w * fine.h];
            for y in 0..fine.h {
                for x in 0..fine.w {
                    let (cx, cy) = ((x / 2).min(cw - 1), (y / 2).min(ch - 1));
                    let i = y * fine.w + x;
                    if fine.hole[i] {
                        fine.img[i] = img_c[cy * cw + cx];
                    }
                    let m = nnf[cy * cw + cx];
                    if m.0 >= 0 {
                        fnnf[i] = (m.0 * 2 + (x % 2) as i32, m.1 * 2 + (y % 2) as i32);
                    }
                }
            }
            nnf = fnnf;
        }
    }
    levels.swap_remove(0).img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_a_hole_in_a_stripe_pattern_with_the_pattern() {
        let (w, h) = (96, 96);
        let rgb: Vec<[f32; 3]> = (0..w * h).map(|i| if (i % w / 6) % 2 == 0 { [200.0, 40.0, 40.0] } else { [30.0, 30.0, 160.0] }).collect();
        let hole: Vec<bool> = (0..w * h).map(|i| (40..56).contains(&(i % w)) && (40..56).contains(&(i / w))).collect();
        let mut damaged = rgb.clone();
        for i in 0..w * h {
            if hole[i] {
                damaged[i] = [128.0, 128.0, 128.0];
            }
        }
        let out = inpaint(w, h, &damaged, &hole, 7);
        // most filled pixels must be one of the two stripe colors (not a gray blur)
        let mut good = 0;
        let mut total = 0;
        for i in 0..w * h {
            if hole[i] {
                total += 1;
                let p = out[i];
                if (p[0] - 200.0).abs() < 40.0 && p[2] < 90.0 || (p[2] - 160.0).abs() < 40.0 && p[0] < 90.0 {
                    good += 1;
                }
            }
        }
        assert!(good as f32 / total as f32 > 0.8, "{good}/{total}");
        assert_eq!(out[5], rgb[5], "outside untouched");
    }
}
