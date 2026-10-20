//! Blend modes checked against the published formulas (docs/research.md §2.5). The reference
//! functions below are written independently from src/blend.rs, straight from the formula text.

use lawha_core::blend::{blend_rgb, composite_px, lum, soft_light_photoshop, soft_light_w3c, BlendMode};
use lawha_core::tile::{Accum, Tile, TILE};

const EPS: f32 = 1e-5;

fn grid() -> Vec<f32> {
    (0..=32).map(|i| i as f32 / 32.0).chain([0.001, 0.249, 0.251, 0.499, 0.501, 0.999]).collect()
}

/// Reference B(Cb, Cs) for the separable modes, from W3C §10.1 and the public reconstructions.
fn reference(mode: BlendMode, cb: f32, cs: f32) -> f32 {
    use BlendMode::*;
    let c = |x: f32| x.clamp(0.0, 1.0);
    match mode {
        Normal => cs,
        Multiply => cb * cs,
        Screen => 1.0 - (1.0 - cb) * (1.0 - cs),
        Overlay => {
            if cb <= 0.5 { 2.0 * cb * cs } else { 1.0 - 2.0 * (1.0 - cb) * (1.0 - cs) }
        }
        Darken => if cb < cs { cb } else { cs },
        Lighten => if cb > cs { cb } else { cs },
        ColorDodge => {
            if cb == 0.0 { 0.0 } else if cs == 1.0 { 1.0 } else { c(cb / (1.0 - cs)) }
        }
        ColorBurn => {
            if cb == 1.0 { 1.0 } else if cs == 0.0 { 0.0 } else { 1.0 - c((1.0 - cb) / cs) }
        }
        HardLight => {
            if cs <= 0.5 { cb * (2.0 * cs) } else { let s = 2.0 * cs - 1.0; cb + s - cb * s }
        }
        Difference => (cb - cs).abs(),
        Exclusion => cb + cs - 2.0 * cb * cs,
        LinearBurn => c(cb + cs - 1.0),
        LinearDodge => c(cb + cs),
        LinearLight => c(cb + 2.0 * cs - 1.0),
        PinLight => if cs <= 0.5 { cb.min(2.0 * cs) } else { cb.max(2.0 * cs - 1.0) },
        HardMix => if cb + cs >= 1.0 { 1.0 } else { 0.0 },
        Subtract => c(cb - cs),
        Divide => if cs == 0.0 { if cb == 0.0 { 0.0 } else { 1.0 } } else { c(cb / cs) },
        VividLight => {
            if cs < 0.5 {
                let s = 2.0 * cs;
                if cb == 1.0 { 1.0 } else if s == 0.0 { 0.0 } else { 1.0 - c((1.0 - cb) / s) }
            } else {
                let s = 2.0 * cs - 1.0;
                if cb == 0.0 { 0.0 } else if s == 1.0 { 1.0 } else { c(cb / (1.0 - s)) }
            }
        }
        SoftLight => {
            // Photoshop variant (docs/research.md §2.5)
            if cs < 0.5 { 2.0 * cb * cs + cb * cb * (1.0 - 2.0 * cs) } else { 2.0 * cb * (1.0 - cs) + cb.sqrt() * (2.0 * cs - 1.0) }
        }
        _ => unreachable!(),
    }
}

const SEPARABLE: [BlendMode; 21] = [
    BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen, BlendMode::Overlay, BlendMode::Darken,
    BlendMode::Lighten, BlendMode::ColorDodge, BlendMode::ColorBurn, BlendMode::HardLight, BlendMode::Difference,
    BlendMode::Exclusion, BlendMode::LinearBurn, BlendMode::LinearDodge, BlendMode::LinearLight, BlendMode::PinLight,
    BlendMode::HardMix, BlendMode::Subtract, BlendMode::Divide, BlendMode::VividLight, BlendMode::SoftLight,
    BlendMode::Normal,
];

#[test]
fn separable_modes_match_reference_formulas() {
    for mode in SEPARABLE {
        for cb in grid() {
            for cs in grid() {
                let got = blend_rgb(mode, [cb, cb, cb], [cs, cs, cs])[0];
                let want = reference(mode, cb, cs);
                assert!((got - want).abs() < EPS, "{mode:?} B({cb}, {cs}) = {got}, want {want}");
            }
        }
    }
}

#[test]
fn known_values() {
    let b = |m, cb, cs| blend_rgb(m, [cb; 3], [cs; 3])[0];
    assert_eq!(b(BlendMode::Multiply, 0.5, 0.5), 0.25);
    assert_eq!(b(BlendMode::Screen, 0.5, 0.5), 0.75);
    assert_eq!(b(BlendMode::Difference, 0.2, 0.7), 0.5);
    assert!((b(BlendMode::Exclusion, 0.5, 0.5) - 0.5).abs() < EPS);
    // white leaves multiply unchanged, black leaves screen unchanged (Adobe help descriptions)
    assert_eq!(b(BlendMode::Multiply, 0.37, 1.0), 0.37);
    assert_eq!(b(BlendMode::Screen, 0.37, 0.0), 0.37);
    // 50% gray is neutral for overlay-family modes on the source side
    for m in [BlendMode::HardLight, BlendMode::SoftLight, BlendMode::LinearLight, BlendMode::PinLight] {
        assert!((b(m, 0.3, 0.5) - 0.3).abs() < 1e-5, "{m:?}");
    }
}

#[test]
fn soft_light_variants_agree_except_light_source_on_dark_backdrop() {
    for cb in grid() {
        for cs in grid() {
            let (p, w) = (soft_light_photoshop(cb, cs), soft_light_w3c(cb, cs));
            if cs <= 0.5 || cb > 0.25 {
                assert!((p - w).abs() < EPS, "should agree at ({cb}, {cs}): {p} vs {w}");
            }
        }
    }
    // and they do differ where expected
    assert!((soft_light_photoshop(0.1, 0.9) - soft_light_w3c(0.1, 0.9)).abs() > 0.01);
}

#[test]
fn non_separable_modes_keep_the_documented_components() {
    let colors = [[0.9, 0.2, 0.1], [0.1, 0.6, 0.8], [0.5, 0.5, 0.5], [0.95, 0.9, 0.05], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let sat = |c: [f32; 3]| c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
    for cb in colors {
        for cs in colors {
            let in_gamut = |c: [f32; 3]| c.iter().all(|v| (-EPS..=1.0 + EPS).contains(v));
            for m in [BlendMode::Hue, BlendMode::Saturation, BlendMode::Color, BlendMode::Luminosity] {
                let r = blend_rgb(m, cb, cs);
                assert!(in_gamut(r), "{m:?} out of gamut: {r:?}");
            }
            // Luminosity takes the source's luminance; Color/Hue/Saturation keep the backdrop's
            // (exactly, unless ClipColor had to pull the result back into gamut)
            let l = blend_rgb(BlendMode::Luminosity, cb, cs);
            let c = blend_rgb(BlendMode::Color, cb, cs);
            if (lum(l) - lum(cs)).abs() > 1e-4 {
                assert!(l.iter().any(|v| *v <= EPS || *v >= 1.0 - EPS), "luminosity only moves when clipped");
            }
            if (lum(c) - lum(cb)).abs() > 1e-4 {
                assert!(c.iter().any(|v| *v <= EPS || *v >= 1.0 - EPS), "color only moves when clipped");
            }
            // Saturation of a gray source makes the result gray
            if sat(cs) == 0.0 {
                let s = blend_rgb(BlendMode::Saturation, cb, cs);
                assert!(sat(s) < 1e-5, "{s:?}");
            }
        }
    }
    // a gray layer in Color mode desaturates the backdrop to its own luminance
    let g = blend_rgb(BlendMode::Color, [0.9, 0.2, 0.1], [0.5, 0.5, 0.5]);
    assert!((g[0] - g[1]).abs() < 1e-5 && (g[1] - g[2]).abs() < 1e-5);
    assert!((g[0] - lum([0.9, 0.2, 0.1])).abs() < 1e-5);
}

#[test]
fn darker_and_lighter_color_pick_a_whole_color() {
    let (a, b) = ([0.9, 0.1, 0.1], [0.2, 0.6, 0.3]);
    let d = blend_rgb(BlendMode::DarkerColor, a, b);
    let l = blend_rgb(BlendMode::LighterColor, a, b);
    assert!(d == a || d == b);
    assert!(l == a || l == b);
    assert_ne!(d, l);
}

#[test]
fn source_over_alpha_follows_the_general_formula() {
    for mode in BlendMode::ALL {
        if mode == BlendMode::Dissolve {
            continue;
        }
        for (ab, a_s) in [(0.0, 0.5), (1.0, 1.0), (0.5, 0.25), (0.8, 0.6), (1.0, 0.0)] {
            let cb = [0.2, 0.5, 0.9];
            let cs = [0.7, 0.3, 0.1];
            let mut d = [cb[0] * ab, cb[1] * ab, cb[2] * ab, ab];
            composite_px(mode, &mut d, cs, a_s);
            let ao = a_s + ab * (1.0 - a_s);
            assert!((d[3] - ao).abs() < EPS, "{mode:?} alpha");
            let bl = if ab > 0.0 { blend_rgb(mode, cb, cs) } else { cs };
            for i in 0..3 {
                let want = a_s * (1.0 - ab) * cs[i] + a_s * ab * bl[i] + (1.0 - a_s) * ab * cb[i];
                assert!((d[i] - want).abs() < EPS, "{mode:?} ab={ab} as={a_s} ch{i}: {} vs {want}", d[i]);
            }
        }
    }
}

#[test]
fn normal_fast_path_equals_the_general_path() {
    let mut src = Tile::transparent();
    for (i, p) in src.0.chunks_exact_mut(4).enumerate() {
        p.copy_from_slice(&[(i % 251) as u8, (i * 7 % 256) as u8, (i * 13 % 256) as u8, (i * 3 % 256) as u8]);
    }
    let mut a = Accum::new();
    let mut b = Accum::new();
    for (i, (x, y)) in a.0.iter_mut().zip(b.0.iter_mut()).enumerate() {
        let v = [0.3 * ((i % 7) as f32 / 7.0), 0.2, 0.1, (i % 5) as f32 / 4.0];
        *x = [v[0] * v[3], v[1] * v[3], v[2] * v[3], v[3]];
        *y = *x;
    }
    a.composite(&src, BlendMode::Normal, 0.8, (0, 0));
    for (d, s) in b.0.iter_mut().zip(src.0.chunks_exact(4)) {
        composite_px(BlendMode::Normal, d, [s[0] as f32 / 255.0, s[1] as f32 / 255.0, s[2] as f32 / 255.0], s[3] as f32 / 255.0 * 0.8);
    }
    for (x, y) in a.0.iter().zip(b.0.iter()) {
        for k in 0..4 {
            assert!((x[k] - y[k]).abs() < 1e-5);
        }
    }
}

#[test]
fn dissolve_keeps_the_expected_share_of_pixels() {
    let src = Tile::filled([255, 0, 0, 255]);
    let mut acc = Accum::new();
    acc.composite(&src, BlendMode::Dissolve, 0.3, (512, 256));
    let shown = acc.0.iter().filter(|p| p[3] > 0.5).count() as f32 / (TILE * TILE) as f32;
    assert!((shown - 0.3).abs() < 0.01, "dissolve showed {shown}");
    assert!(acc.0.iter().all(|p| p[3] == 0.0 || p[3] == 1.0));
}

#[test]
fn psd_keys_round_trip() {
    for m in BlendMode::ALL {
        assert_eq!(BlendMode::from_psd_key(m.psd_key()), Some(m));
    }
    assert_eq!(BlendMode::from_psd_key(b"pass"), None);
}
