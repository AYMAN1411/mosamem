//! Blend modes and compositing.
//!
//! Formulas follow W3C Compositing and Blending Level 1 (§6 general formula, §10.1 separable and
//! §10.2 non-separable modes). The remaining Photoshop-file modes use the public reconstructions
//! listed in docs/research.md §2.5. Colors are gamma-encoded sRGB in [0, 1], as editors do for 8-bit
//! documents.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlendMode {
    Normal,
    Dissolve,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    DarkerColor,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    LighterColor,
    Overlay,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [BlendMode; 27] = [
        Self::Normal, Self::Dissolve, Self::Darken, Self::Multiply, Self::ColorBurn, Self::LinearBurn,
        Self::DarkerColor, Self::Lighten, Self::Screen, Self::ColorDodge, Self::LinearDodge,
        Self::LighterColor, Self::Overlay, Self::SoftLight, Self::HardLight, Self::VividLight,
        Self::LinearLight, Self::PinLight, Self::HardMix, Self::Difference, Self::Exclusion,
        Self::Subtract, Self::Divide, Self::Hue, Self::Saturation, Self::Color, Self::Luminosity,
    ];

    /// Stable index shared with the GPU shaders.
    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|m| *m == self).unwrap() as u32
    }

    /// Photoshop file key (4 characters). `pass` (pass-through) belongs to groups, not here.
    pub fn psd_key(self) -> &'static [u8; 4] {
        match self {
            Self::Normal => b"norm",
            Self::Dissolve => b"diss",
            Self::Darken => b"dark",
            Self::Multiply => b"mul ",
            Self::ColorBurn => b"idiv",
            Self::LinearBurn => b"lbrn",
            Self::DarkerColor => b"dkCl",
            Self::Lighten => b"lite",
            Self::Screen => b"scrn",
            Self::ColorDodge => b"div ",
            Self::LinearDodge => b"lddg",
            Self::LighterColor => b"lgCl",
            Self::Overlay => b"over",
            Self::SoftLight => b"sLit",
            Self::HardLight => b"hLit",
            Self::VividLight => b"vLit",
            Self::LinearLight => b"lLit",
            Self::PinLight => b"pLit",
            Self::HardMix => b"hMix",
            Self::Difference => b"diff",
            Self::Exclusion => b"smud",
            Self::Subtract => b"fsub",
            Self::Divide => b"fdiv",
            Self::Hue => b"hue ",
            Self::Saturation => b"sat ",
            Self::Color => b"colr",
            Self::Luminosity => b"lum ",
        }
    }

    pub fn from_psd_key(key: &[u8; 4]) -> Option<Self> {
        Self::ALL.iter().copied().find(|m| m.psd_key() == key)
    }

    fn is_separable(self) -> bool {
        !matches!(self, Self::Hue | Self::Saturation | Self::Color | Self::Luminosity | Self::DarkerColor | Self::LighterColor)
    }
}

#[inline]
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// Separable blend function B(cb, cs) for one channel. `cb` = backdrop, `cs` = source.
#[inline]
pub fn blend_channel(mode: BlendMode, cb: f32, cs: f32) -> f32 {
    use BlendMode::*;
    match mode {
        Normal | Dissolve => cs,
        Multiply => cb * cs,
        Screen => cb + cs - cb * cs,
        Overlay => hard_light(cs, cb),
        Darken => cb.min(cs),
        Lighten => cb.max(cs),
        ColorDodge => color_dodge(cb, cs),
        ColorBurn => color_burn(cb, cs),
        HardLight => hard_light(cb, cs),
        SoftLight => soft_light_photoshop(cb, cs),
        Difference => (cb - cs).abs(),
        Exclusion => cb + cs - 2.0 * cb * cs,
        LinearBurn => clamp01(cb + cs - 1.0),
        LinearDodge => clamp01(cb + cs),
        VividLight => {
            if cs < 0.5 {
                color_burn(cb, 2.0 * cs)
            } else {
                color_dodge(cb, 2.0 * cs - 1.0)
            }
        }
        LinearLight => clamp01(cb + 2.0 * cs - 1.0),
        PinLight => {
            if cs <= 0.5 {
                cb.min(2.0 * cs)
            } else {
                cb.max(2.0 * cs - 1.0)
            }
        }
        HardMix => {
            if cb + cs >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        Subtract => clamp01(cb - cs),
        Divide => {
            if cs <= 0.0 {
                if cb <= 0.0 { 0.0 } else { 1.0 }
            } else {
                clamp01(cb / cs)
            }
        }
        Hue | Saturation | Color | Luminosity | DarkerColor | LighterColor => cs,
    }
}

#[inline]
fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb * 2.0 * cs
    } else {
        let s = 2.0 * cs - 1.0;
        cb + s - cb * s
    }
}

#[inline]
fn color_dodge(cb: f32, cs: f32) -> f32 {
    if cb <= 0.0 {
        0.0
    } else if cs >= 1.0 {
        1.0
    } else {
        (cb / (1.0 - cs)).min(1.0)
    }
}

#[inline]
fn color_burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 {
        1.0
    } else if cs <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - cb) / cs).min(1.0)
    }
}

/// Soft Light as documented for Photoshop (docs/research.md §2.5): differs from W3C only where the
/// source is light and the backdrop is ≤ 0.25.
#[inline]
pub fn soft_light_photoshop(cb: f32, cs: f32) -> f32 {
    if cs < 0.5 {
        2.0 * cb * cs + cb * cb * (1.0 - 2.0 * cs)
    } else {
        2.0 * cb * (1.0 - cs) + cb.sqrt() * (2.0 * cs - 1.0)
    }
}

/// Soft Light per W3C / PDF.
#[inline]
pub fn soft_light_w3c(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}

// ---- non-separable helpers (W3C §10.2) ----

#[inline]
pub fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        for v in &mut out {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 {
        for v in &mut out {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    out
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    // indices sorted by value: min, mid, max
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&a, &b| c[a].partial_cmp(&c[b]).unwrap());
    let (lo, mid, hi) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0; 3];
    if c[hi] > c[lo] {
        out[mid] = (c[mid] - c[lo]) * s / (c[hi] - c[lo]);
        out[hi] = s;
    }
    out[lo] = 0.0;
    out
}

/// B(Cb, Cs) for all three channels.
pub fn blend_rgb(mode: BlendMode, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
    use BlendMode::*;
    if mode.is_separable() {
        return [blend_channel(mode, cb[0], cs[0]), blend_channel(mode, cb[1], cs[1]), blend_channel(mode, cb[2], cs[2])];
    }
    match mode {
        Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        Color => set_lum(cs, lum(cb)),
        Luminosity => set_lum(cb, lum(cs)),
        // ✱ metric unconfirmed (docs/research.md §16 #3): luminance for now
        DarkerColor => if lum(cs) < lum(cb) { cs } else { cb },
        LighterColor => if lum(cs) > lum(cb) { cs } else { cb },
        _ => unreachable!(),
    }
}

/// Deterministic per-pixel threshold for Dissolve, in [0, 1). A 32-bit hash so the GPU shader can
/// compute exactly the same value (see `DISSOLVE_GLSL` in the webview compositor).
#[inline]
pub fn dissolve_threshold(x: i64, y: i64) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h >> 8) as f32 / 16_777_216.0
}

/// Composites one straight-alpha source pixel onto a premultiplied backdrop (W3C §6, source-over).
/// `src` is straight RGBA in [0, 1]; `alpha` already includes layer opacity.
#[inline]
pub fn composite_px(mode: BlendMode, dst: &mut [f32; 4], src: [f32; 3], alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let ab = dst[3];
    let cb = if ab > 0.0 { [dst[0] / ab, dst[1] / ab, dst[2] / ab] } else { [0.0; 3] };
    let b = if ab > 0.0 && mode != BlendMode::Normal { blend_rgb(mode, cb, src) } else { src };
    let a_s = alpha;
    for i in 0..3 {
        // co = as(1-ab)Cs + as·ab·B + (1-as)·ab·Cb ; dst[i] = ab·Cb
        dst[i] = a_s * (1.0 - ab) * src[i] + a_s * ab * b[i] + (1.0 - a_s) * dst[i];
    }
    dst[3] = a_s + ab * (1.0 - a_s);
}
