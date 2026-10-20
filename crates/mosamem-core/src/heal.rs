//! Healing: copy texture from a source but match the destination's color and light at the edges.
//!
//! Following Pérez et al. (Poisson Image Editing) and Georgiev (Healing Brush), the healed result is
//! `src + h`, where `h` is the smooth membrane that equals `dest − src` on the boundary of the
//! region and is harmonic (Δh = 0) inside it. We solve for `h` with a coarse-to-fine pyramid
//! (fast initial guess) followed by Gauss-Seidel relaxation at each level.

/// Solves Δh = 0 inside `inside` with Dirichlet values from `h` outside, in place.
/// `h` has `ch` interleaved channels per pixel.
pub fn membrane(w: usize, h: usize, ch: usize, vals: &mut [f32], inside: &[bool]) {
    if w < 3 || h < 3 || !inside.iter().any(|v| *v) {
        return;
    }
    // coarse level first, for a good starting guess
    if w > 16 && h > 16 {
        let (cw, chh) = (w.div_ceil(2), h.div_ceil(2));
        let mut cv = vec![0f32; cw * chh * ch];
        let mut cin = vec![false; cw * chh];
        for y in 0..chh {
            for x in 0..cw {
                let mut sum = vec![0f32; ch];
                let mut n = 0f32;
                let mut any_in = false;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx >= w || sy >= h {
                        continue;
                    }
                    let i = sy * w + sx;
                    if inside[i] {
                        any_in = true;
                    } else {
                        for c in 0..ch {
                            sum[c] += vals[i * ch + c];
                        }
                        n += 1.0;
                    }
                }
                let j = y * cw + x;
                // a coarse pixel is unknown only if all its children are unknown
                cin[j] = any_in && n == 0.0;
                for c in 0..ch {
                    cv[j * ch + c] = if n > 0.0 { sum[c] / n } else { 0.0 };
                }
            }
        }
        membrane(cw, chh, ch, &mut cv, &cin);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if inside[i] {
                    let j = (y / 2).min(chh - 1) * cw + (x / 2).min(cw - 1);
                    for c in 0..ch {
                        vals[i * ch + c] = cv[j * ch + c];
                    }
                }
            }
        }
    }
    // relax at this level (red-black ordering keeps it deterministic)
    let iters = 40 + (w.max(h) as f32).log2() as usize * 10;
    for _ in 0..iters {
        for parity in 0..2 {
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if !inside[i] || (x + y) % 2 != parity {
                        continue;
                    }
                    for c in 0..ch {
                        let mut s = 0f32;
                        let mut n = 0f32;
                        if x > 0 {
                            s += vals[(i - 1) * ch + c];
                            n += 1.0;
                        }
                        if x + 1 < w {
                            s += vals[(i + 1) * ch + c];
                            n += 1.0;
                        }
                        if y > 0 {
                            s += vals[(i - w) * ch + c];
                            n += 1.0;
                        }
                        if y + 1 < h {
                            s += vals[(i + w) * ch + c];
                            n += 1.0;
                        }
                        vals[i * ch + c] = s / n;
                    }
                }
            }
        }
    }
}

/// Heals `dest` (RGBA f32, 0–255, straight) with texture from `src` over the pixels where
/// `region > 0`; `region` (0–1) also feathers the result into the original.
pub fn heal(w: usize, h: usize, dest: &[f32], src: &[f32], region: &[f32]) -> Vec<f32> {
    let n = w * h;
    let inside: Vec<bool> = region.iter().map(|r| *r > 0.0).collect();
    // offsets on the boundary (outside the region), membrane inside
    let mut diff: Vec<f32> = (0..n * 4).map(|k| dest[k] - src[k]).collect();
    for i in 0..n {
        if inside[i] {
            for c in 0..4 {
                diff[i * 4 + c] = 0.0;
            }
        }
    }
    membrane(w, h, 4, &mut diff, &inside);
    let mut out = dest.to_vec();
    for i in 0..n {
        if !inside[i] {
            continue;
        }
        let r = region[i].clamp(0.0, 1.0);
        for c in 0..4 {
            let healed = (src[i * 4 + c] + diff[i * 4 + c]).clamp(0.0, 255.0);
            out[i * 4 + c] = dest[i * 4 + c] + (healed - dest[i * 4 + c]) * r;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membrane_reproduces_a_linear_ramp() {
        // a linear function is harmonic: the solver must recover it exactly inside a hole
        let (w, h) = (40, 30);
        let f = |x: usize, y: usize| 2.0 * x as f32 + 3.0 * y as f32;
        let mut v: Vec<f32> = (0..w * h).map(|i| f(i % w, i / w)).collect();
        let inside: Vec<bool> = (0..w * h).map(|i| (10..30).contains(&(i % w)) && (8..22).contains(&(i / w))).collect();
        for i in 0..w * h {
            if inside[i] {
                v[i] = 0.0;
            }
        }
        membrane(w, h, 1, &mut v, &inside);
        for i in 0..w * h {
            assert!((v[i] - f(i % w, i / w)).abs() < 0.5, "at {i}: {} vs {}", v[i], f(i % w, i / w));
        }
    }

    #[test]
    fn healing_keeps_texture_and_matches_the_surroundings() {
        // destination: a smooth dark-to-light ramp; source: mid gray with a checker texture
        let (w, h) = (64, 64);
        let mut dest = vec![0f32; w * h * 4];
        let mut src = vec![0f32; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let ramp = 40.0 + x as f32 * 2.5;
                dest[i..i + 4].copy_from_slice(&[ramp, ramp, ramp, 255.0]);
                let tex = if (x / 2 + y / 2) % 2 == 0 { 10.0 } else { -10.0 };
                src[i..i + 4].copy_from_slice(&[128.0 + tex, 128.0 + tex, 128.0 + tex, 255.0]);
            }
        }
        let region: Vec<f32> = (0..w * h).map(|i| if (20..44).contains(&(i % w)) && (20..44).contains(&(i / w)) { 1.0 } else { 0.0 }).collect();
        let out = heal(w, h, &dest, &src, &region);
        // the low-frequency level follows the destination ramp, the checker survives
        let at = |x: usize, y: usize| out[(y * w + x) * 4];
        let local_mean = (at(31, 32) + at(32, 32) + at(31, 33) + at(32, 33)) / 4.0;
        assert!((local_mean - (40.0 + 31.5 * 2.5)).abs() < 6.0, "mean {local_mean}");
        // the correction (healed − source) is smooth: neighbours differ by about the ramp slope
        let corr = |x: usize, y: usize| at(x, y) - src[(y * w + x) * 4];
        assert!((corr(32, 32) - corr(33, 32)).abs() < 4.0 && (corr(32, 32) - corr(32, 33)).abs() < 4.0);
        // and the source texture survives (adjacent checker cells differ by ~20)
        assert!((at(32, 32) - at(34, 32)).abs() > 12.0, "texture kept");
        assert_eq!(at(5, 5), dest[(5 * w + 5) * 4], "outside untouched");
    }
}
