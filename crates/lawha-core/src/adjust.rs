//! Adjustments: color operations on straight sRGB in [0, 1], applied to everything below an
//! adjustment layer (or to a layer's pixels).
//!
//! An `Adjustment` holds the user's settings. `compile` turns it into an `Op`: a kind, a table
//! (a per-channel curve or a gradient) and a small block of numbers. The CPU applies an `Op` with
//! `Op::apply`; the GPU preview uploads the same table and numbers to a shader that repeats the math
//! (see `ADJUST_GLSL` in the webview), and the two are tested against each other.
//!
//! Formulas come from docs/research.md §3.2. The ones marked ✱ there are our own definitions, to be
//! calibrated against Photoshop-made files later.

use serde::{Deserialize, Serialize};

use crate::blend::{blend_rgb, dissolve_threshold, lum, set_lum, BlendMode};
use crate::tile::{Accum, TILE};

/// Entries in a per-channel curve table. Inputs between entries are interpolated linearly.
pub const LUT_N: usize = 1024;
/// Entries in a gradient map table.
pub const GRADIENT_N: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Levels {
    /// 0–255
    pub in_black: f32,
    pub in_white: f32,
    /// 0.01–9.99, 1 = linear
    pub gamma: f32,
    pub out_black: f32,
    pub out_white: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Levels { in_black: 0.0, in_white: 255.0, gamma: 1.0, out_black: 0.0, out_white: 255.0 }
    }
}

/// Hue (degrees, ±180), saturation and lightness (±100).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hsl {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    /// 0–1
    pub pos: f32,
    pub color: [u8; 3],
}

/// The settings of one adjustment, as stored in documents and sent by the UI.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Adjustment {
    /// Master (RGB), then red, green, blue.
    Levels { channels: [Levels; 4] },
    /// Points in 0–255 (input, output) for master, red, green, blue.
    Curves { channels: [Vec<[f32; 2]>; 4] },
    /// Master, then reds, yellows, greens, cyans, blues, magentas.
    HueSaturation { master: Hsl, ranges: [Hsl; 6], colorize: bool, colorize_hsl: Hsl },
    /// Brightness ±150, contrast −50…100.
    BrightnessContrast { brightness: f32, contrast: f32, legacy: bool },
    /// Exposure in stops, offset, gamma.
    Exposure { exposure: f32, offset: f32, gamma: f32 },
    /// Both ±100.
    Vibrance { vibrance: f32, saturation: f32 },
    /// Cyan–red, magenta–green, yellow–blue (±100) for each tonal range.
    ColorBalance { shadows: [f32; 3], midtones: [f32; 3], highlights: [f32; 3], preserve_luminosity: bool },
    /// Percent for reds, yellows, greens, cyans, blues, magentas.
    BlackWhite { weights: [f32; 6], tint: bool, tint_color: [u8; 3] },
    GradientMap { stops: Vec<GradientStop>, reverse: bool, dither: bool },
    Invert,
    /// 1–255
    Threshold { level: f32 },
}

impl Adjustment {
    /// The default settings for a kind (the names match the serialized `kind`).
    pub fn default_for(kind: &str) -> Option<Adjustment> {
        Some(match kind {
            "levels" => Adjustment::Levels { channels: [Levels::default(); 4] },
            "curves" => Adjustment::Curves { channels: std::array::from_fn(|_| vec![[0.0, 0.0], [255.0, 255.0]]) },
            "hueSaturation" => Adjustment::HueSaturation {
                master: Hsl::default(),
                ranges: [Hsl::default(); 6],
                colorize: false,
                colorize_hsl: Hsl { hue: 0.0, saturation: 25.0, lightness: 0.0 },
            },
            "brightnessContrast" => Adjustment::BrightnessContrast { brightness: 0.0, contrast: 0.0, legacy: false },
            "exposure" => Adjustment::Exposure { exposure: 0.0, offset: 0.0, gamma: 1.0 },
            "vibrance" => Adjustment::Vibrance { vibrance: 0.0, saturation: 0.0 },
            "colorBalance" => Adjustment::ColorBalance { shadows: [0.0; 3], midtones: [0.0; 3], highlights: [0.0; 3], preserve_luminosity: true },
            "blackWhite" => Adjustment::BlackWhite { weights: [40.0, 60.0, 40.0, 60.0, 20.0, 80.0], tint: false, tint_color: [225, 211, 179] },
            "gradientMap" => Adjustment::GradientMap {
                stops: vec![GradientStop { pos: 0.0, color: [0, 0, 0] }, GradientStop { pos: 1.0, color: [255, 255, 255] }],
                reverse: false,
                dither: false,
            },
            "invert" => Adjustment::Invert,
            "threshold" => Adjustment::Threshold { level: 128.0 },
            _ => return None,
        })
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Adjustment::Levels { .. } => "levels",
            Adjustment::Curves { .. } => "curves",
            Adjustment::HueSaturation { .. } => "hueSaturation",
            Adjustment::BrightnessContrast { .. } => "brightnessContrast",
            Adjustment::Exposure { .. } => "exposure",
            Adjustment::Vibrance { .. } => "vibrance",
            Adjustment::ColorBalance { .. } => "colorBalance",
            Adjustment::BlackWhite { .. } => "blackWhite",
            Adjustment::GradientMap { .. } => "gradientMap",
            Adjustment::Invert => "invert",
            Adjustment::Threshold { .. } => "threshold",
        }
    }
}

/// How an `Op` transforms a pixel. The numbers are the shader's `kind` uniform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OpKind {
    /// Per-channel curve from the table (levels, curves, brightness/contrast, exposure, invert).
    Curve = 1,
    HueSaturation = 2,
    Vibrance = 3,
    ColorBalance = 4,
    BlackWhite = 5,
    /// Luminosity → color from the table.
    GradientMap = 6,
    Threshold = 7,
}

/// A compiled adjustment: what the CPU and the GPU shader both run.
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub kind: OpKind,
    /// `LUT_N` RGB entries for `Curve`, `GRADIENT_N` for `GradientMap`, empty otherwise.
    pub table: Vec<[f32; 4]>,
    /// Kind-specific numbers (layout in `compile`).
    pub params: [[f32; 4]; 8],
}

pub fn compile(a: &Adjustment) -> Op {
    let mut p = [[0.0f32; 4]; 8];
    match a {
        Adjustment::Levels { channels } => curve_op(|c, x| levels(&channels[c + 1], levels(&channels[0], x))),
        Adjustment::Curves { channels } => {
            let curves: Vec<Spline> = channels.iter().map(|pts| Spline::new(pts)).collect();
            curve_op(|c, x| curves[c + 1].eval(curves[0].eval(x)))
        }
        Adjustment::BrightnessContrast { brightness, contrast, legacy } => {
            let (b, k) = (*brightness, *contrast);
            if *legacy {
                curve_op(|_, x| legacy_contrast(legacy_brightness(x, b), k))
            } else {
                curve_op(|_, x| modern_contrast(modern_brightness(x, b), k))
            }
        }
        Adjustment::Exposure { exposure, offset, gamma } => {
            let (gain, off, inv_g) = (2f32.powf(*exposure), *offset, 1.0 / gamma.clamp(0.01, 9.99));
            curve_op(|_, x| linear_to_srgb((srgb_to_linear(x) * gain + off).max(0.0).powf(inv_g)))
        }
        Adjustment::Invert => curve_op(|_, x| 1.0 - x),
        Adjustment::HueSaturation { master, ranges, colorize, colorize_hsl } => {
            // p[0] master, p[1..7] ranges, p[7] colorize; hue in degrees, the rest as fractions
            p[0] = [master.hue, master.saturation / 100.0, master.lightness / 100.0, *colorize as u8 as f32];
            for (i, r) in ranges.iter().enumerate() {
                p[i + 1] = [r.hue, r.saturation / 100.0, r.lightness / 100.0, 0.0];
            }
            p[7] = [colorize_hsl.hue, colorize_hsl.saturation / 100.0, colorize_hsl.lightness / 100.0, 0.0];
            Op { kind: OpKind::HueSaturation, table: Vec::new(), params: p }
        }
        Adjustment::Vibrance { vibrance, saturation } => {
            p[0] = [vibrance / 100.0, saturation / 100.0, 0.0, 0.0];
            Op { kind: OpKind::Vibrance, table: Vec::new(), params: p }
        }
        Adjustment::ColorBalance { shadows, midtones, highlights, preserve_luminosity } => {
            for (i, r) in [shadows, midtones, highlights].iter().enumerate() {
                p[i] = [r[0] / 100.0, r[1] / 100.0, r[2] / 100.0, 0.0];
            }
            p[3][0] = *preserve_luminosity as u8 as f32;
            Op { kind: OpKind::ColorBalance, table: Vec::new(), params: p }
        }
        Adjustment::BlackWhite { weights, tint, tint_color } => {
            let w = weights.map(|v| v / 100.0);
            p[0] = [w[0], w[1], w[2], w[3]];
            p[1] = [w[4], w[5], *tint as u8 as f32, 0.0];
            p[2] = [tint_color[0] as f32 / 255.0, tint_color[1] as f32 / 255.0, tint_color[2] as f32 / 255.0, 0.0];
            Op { kind: OpKind::BlackWhite, table: Vec::new(), params: p }
        }
        Adjustment::GradientMap { stops, reverse, dither } => {
            p[0] = [*reverse as u8 as f32, *dither as u8 as f32, 0.0, 0.0];
            Op { kind: OpKind::GradientMap, table: gradient_table(stops), params: p }
        }
        Adjustment::Threshold { level } => {
            p[0][0] = level.clamp(1.0, 255.0);
            Op { kind: OpKind::Threshold, table: Vec::new(), params: p }
        }
    }
}

fn curve_op(f: impl Fn(usize, f32) -> f32) -> Op {
    let table = (0..LUT_N)
        .map(|i| {
            let x = i as f32 / (LUT_N - 1) as f32;
            [f(0, x).clamp(0.0, 1.0), f(1, x).clamp(0.0, 1.0), f(2, x).clamp(0.0, 1.0), 0.0]
        })
        .collect();
    Op { kind: OpKind::Curve, table, params: [[0.0; 4]; 8] }
}

/// Table lookup with linear interpolation; `x` in [0, 1].
#[inline]
fn lookup(table: &[[f32; 4]], x: f32, ch: usize) -> f32 {
    let p = x.clamp(0.0, 1.0) * (table.len() - 1) as f32;
    let i = (p as usize).min(table.len() - 2);
    let f = p - i as f32;
    table[i][ch] + (table[i + 1][ch] - table[i][ch]) * f
}

impl Op {
    /// Applies the adjustment to one straight sRGB color; (x, y) is the pixel (for dithering).
    pub fn apply(&self, c: [f32; 3], x: i64, y: i64) -> [f32; 3] {
        let p = &self.params;
        match self.kind {
            OpKind::Curve => [lookup(&self.table, c[0], 0), lookup(&self.table, c[1], 1), lookup(&self.table, c[2], 2)],
            OpKind::HueSaturation => hue_saturation(c, p),
            OpKind::Vibrance => vibrance(c, p[0][0], p[0][1]),
            OpKind::ColorBalance => color_balance(c, p),
            OpKind::BlackWhite => black_white(c, p),
            OpKind::GradientMap => {
                let mut t = lum(c).clamp(0.0, 1.0);
                if p[0][0] > 0.5 {
                    t = 1.0 - t;
                }
                if p[0][1] > 0.5 {
                    t += (dissolve_threshold(x, y) - 0.5) / 255.0;
                }
                [lookup(&self.table, t, 0), lookup(&self.table, t, 1), lookup(&self.table, t, 2)]
            }
            OpKind::Threshold => {
                // a small epsilon so 8-bit inputs land on the same side on the CPU and the GPU
                let v = if lum(c) * 255.0 + 1e-3 >= p[0][0] { 1.0 } else { 0.0 };
                [v; 3]
            }
        }
    }
}

/// Applies an adjustment to a premultiplied accumulator, the way an adjustment layer affects what
/// is below it: the adjusted color is blended with the layer's mode, then mixed in by `k` (opacity ×
/// fill) times the mask factor. Alpha is unchanged.
pub fn apply_to_accum(acc: &mut Accum, op: &Op, mode: BlendMode, k: f32, mask: Option<&[f32]>, origin: (i64, i64)) {
    for (i, d) in acc.0.iter_mut().enumerate() {
        let a = d[3];
        if a <= 0.0 {
            continue;
        }
        let mut t = k * mask.map_or(1.0, |m| m[i]);
        let (x, y) = ((i % TILE) as i64 + origin.0, (i / TILE) as i64 + origin.1);
        if mode == BlendMode::Dissolve {
            t = if dissolve_threshold(x, y) < t { 1.0 } else { 0.0 };
        }
        if t <= 0.0 {
            continue;
        }
        let inv = 1.0 / a;
        let cb = [(d[0] * inv).clamp(0.0, 1.0), (d[1] * inv).clamp(0.0, 1.0), (d[2] * inv).clamp(0.0, 1.0)];
        let adj = op.apply(cb, x, y);
        let b = match mode {
            BlendMode::Normal | BlendMode::Dissolve => adj,
            m => blend_rgb(m, cb, adj),
        };
        for c in 0..3 {
            d[c] = (cb[c] + (b[c] - cb[c]) * t) * a;
        }
    }
}

// ---- curves ----

fn levels(l: &Levels, x: f32) -> f32 {
    let (ib, iw) = (l.in_black / 255.0, l.in_white / 255.0);
    let t = if iw > ib { ((x - ib) / (iw - ib)).clamp(0.0, 1.0) } else if x >= ib { 1.0 } else { 0.0 };
    let t = t.powf(1.0 / l.gamma.clamp(0.01, 9.99));
    (l.out_black + (l.out_white - l.out_black) * t) / 255.0
}

/// Monotone cubic interpolation through the points (Fritsch–Carlson), flat beyond the ends.
struct Spline {
    x: Vec<f32>,
    y: Vec<f32>,
    m: Vec<f32>,
}

impl Spline {
    fn new(points: &[[f32; 2]]) -> Spline {
        let mut pts: Vec<[f32; 2]> = points.iter().map(|p| [p[0].clamp(0.0, 255.0) / 255.0, p[1].clamp(0.0, 255.0) / 255.0]).collect();
        pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6);
        if pts.is_empty() {
            pts = vec![[0.0, 0.0], [1.0, 1.0]];
        }
        let x: Vec<f32> = pts.iter().map(|p| p[0]).collect();
        let y: Vec<f32> = pts.iter().map(|p| p[1]).collect();
        let n = x.len();
        let mut m = vec![0.0; n];
        if n >= 2 {
            let d: Vec<f32> = (0..n - 1).map(|k| (y[k + 1] - y[k]) / (x[k + 1] - x[k])).collect();
            m[0] = d[0];
            m[n - 1] = d[n - 2];
            for k in 1..n - 1 {
                m[k] = if d[k - 1] * d[k] <= 0.0 { 0.0 } else { (d[k - 1] + d[k]) / 2.0 };
            }
            for k in 0..n - 1 {
                if d[k] == 0.0 {
                    m[k] = 0.0;
                    m[k + 1] = 0.0;
                    continue;
                }
                let (a, b) = (m[k] / d[k], m[k + 1] / d[k]);
                let s = a * a + b * b;
                if s > 9.0 {
                    let t = 3.0 / s.sqrt();
                    m[k] = t * a * d[k];
                    m[k + 1] = t * b * d[k];
                }
            }
        }
        Spline { x, y, m }
    }

    fn eval(&self, v: f32) -> f32 {
        let n = self.x.len();
        if n == 1 || v <= self.x[0] {
            return self.y[0];
        }
        if v >= self.x[n - 1] {
            return self.y[n - 1];
        }
        let k = self.x.partition_point(|&xi| xi <= v) - 1;
        let h = self.x[k + 1] - self.x[k];
        let t = (v - self.x[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        (h00 * self.y[k] + h10 * h * self.m[k] + h01 * self.y[k + 1] + h11 * h * self.m[k + 1]).clamp(0.0, 1.0)
    }
}

// Brightness/contrast. Legacy: a shift and a slope around mid-gray, clipping at the ends. Modern
// (✱ our definition): curves that keep black and white in place.
fn legacy_brightness(x: f32, b: f32) -> f32 {
    x + b / 255.0
}
fn legacy_contrast(x: f32, c: f32) -> f32 {
    // −50 halves the slope, +100 makes it very steep
    let slope = if c >= 0.0 { 1.0 / (1.0 - 0.99 * c / 100.0) } else { 1.0 + c / 100.0 };
    (x - 0.5) * slope + 0.5
}
fn modern_brightness(x: f32, b: f32) -> f32 {
    // a gamma curve: +150 lifts mid-gray to about 0.78, −150 lowers it to about 0.14
    x.clamp(0.0, 1.0).powf(2f32.powf(-b / 100.0))
}
fn modern_contrast(x: f32, c: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if c >= 0.0 {
        // blend toward an S-curve (smoothstep) that keeps 0, 0.5 and 1 in place
        let s = x * x * (3.0 - 2.0 * x);
        x + (s - x) * (c / 100.0)
    } else {
        0.5 + (x - 0.5) * (1.0 + c / 100.0)
    }
}

pub fn srgb_to_linear(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(x: f32) -> f32 {
    if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn gradient_table(stops: &[GradientStop]) -> Vec<[f32; 4]> {
    let mut s: Vec<GradientStop> = stops.to_vec();
    s.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    if s.is_empty() {
        s = vec![GradientStop { pos: 0.0, color: [0, 0, 0] }, GradientStop { pos: 1.0, color: [255, 255, 255] }];
    }
    let col = |g: &GradientStop| g.color.map(|v| v as f32 / 255.0);
    (0..GRADIENT_N)
        .map(|i| {
            let t = i as f32 / (GRADIENT_N - 1) as f32;
            let c = if t <= s[0].pos {
                col(&s[0])
            } else if t >= s[s.len() - 1].pos {
                col(&s[s.len() - 1])
            } else {
                let k = s.partition_point(|g| g.pos <= t) - 1;
                let (a, b) = (&s[k], &s[k + 1]);
                let f = if b.pos > a.pos { (t - a.pos) / (b.pos - a.pos) } else { 0.0 };
                let (ca, cb) = (col(a), col(b));
                [ca[0] + (cb[0] - ca[0]) * f, ca[1] + (cb[1] - ca[1]) * f, ca[2] + (cb[2] - ca[2]) * f]
            };
            [c[0], c[1], c[2], 0.0]
        })
        .collect()
}

// ---- per-pixel operations ----

/// RGB → (hue in degrees [0, 360), saturation, lightness).
pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d <= 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    [h * 60.0, s.min(1.0), l]
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// Angular distance between two hues, in [0, 180].
fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Weight of a hue range centered at `center`: full within 15°, fading to zero at 45°. Neighbouring
/// ranges sum to one.
fn range_weight(h: f32, center: f32) -> f32 {
    ((45.0 - hue_distance(h, center)) / 30.0).clamp(0.0, 1.0)
}

/// Lightness ±1: toward white or black.
fn lighten(c: [f32; 3], dl: f32) -> [f32; 3] {
    if dl >= 0.0 {
        c.map(|v| v + (1.0 - v) * dl)
    } else {
        c.map(|v| v * (1.0 + dl))
    }
}

fn hue_saturation(c: [f32; 3], p: &[[f32; 4]; 8]) -> [f32; 3] {
    let [h, s, l] = rgb_to_hsl(c);
    if p[0][3] > 0.5 {
        // colorize: one hue and saturation, the pixel's own lightness
        let k = p[7];
        return lighten(hsl_to_rgb(k[0], k[1].clamp(0.0, 1.0), l), k[2]);
    }
    let (mut dh, mut ds, mut dl) = (p[0][0], p[0][1], p[0][2]);
    // ranges only apply to colored pixels
    if s > 0.0 {
        for i in 0..6 {
            let w = range_weight(h, i as f32 * 60.0);
            dh += w * p[i + 1][0];
            ds += w * p[i + 1][1];
            dl += w * p[i + 1][2];
        }
    }
    let s2 = (s * (1.0 + ds.clamp(-1.0, 1.0))).clamp(0.0, 1.0);
    lighten(hsl_to_rgb(h + dh, s2, l), dl.clamp(-1.0, 1.0))
}

fn vibrance(c: [f32; 3], v: f32, sat: f32) -> [f32; 3] {
    let [h, s, l] = rgb_to_hsl(c);
    // vibrance works most on weak colors, and (when raising) less on skin tones (✱ our definition)
    let skin = if v > 0.0 { 1.0 - 0.5 * (1.0 - hue_distance(h, 25.0) / 35.0).max(0.0) } else { 1.0 };
    let s1 = (s * (1.0 + v * (1.0 - s) * skin)).clamp(0.0, 1.0);
    let s2 = (s1 * (1.0 + sat)).clamp(0.0, 1.0);
    hsl_to_rgb(h, s2, l)
}

fn color_balance(c: [f32; 3], p: &[[f32; 4]; 8]) -> [f32; 3] {
    let l = lum(c).clamp(0.0, 1.0);
    // tonal weights: the quadratic Bernstein basis (sums to one)
    let w = [(1.0 - l) * (1.0 - l), 2.0 * l * (1.0 - l), l * l];
    let mut out = c;
    for (k, v) in out.iter_mut().enumerate() {
        let shift = w[0] * p[0][k] + w[1] * p[1][k] + w[2] * p[2][k];
        *v = (*v + 0.5 * shift).clamp(0.0, 1.0);
    }
    if p[3][0] > 0.5 {
        set_lum(out, lum(c))
    } else {
        out
    }
}

fn black_white(c: [f32; 3], p: &[[f32; 4]; 8]) -> [f32; 3] {
    // weights: reds, yellows, greens, cyans, blues, magentas
    let w = [p[0][0], p[0][1], p[0][2], p[0][3], p[1][0], p[1][1]];
    let (r, g, b) = (c[0], c[1], c[2]);
    // gray = min + (mid − min)·w(secondary) + (max − mid)·w(primary), where the primary is the
    // largest channel and the secondary mixes the two largest
    let (mx, mid, mn, primary, secondary) = if r >= g && r >= b {
        if g >= b { (r, g, b, 0, 1) } else { (r, b, g, 0, 5) }
    } else if g >= r && g >= b {
        if r >= b { (g, r, b, 2, 1) } else { (g, b, r, 2, 3) }
    } else if r >= g {
        (b, r, g, 4, 5)
    } else {
        (b, g, r, 4, 3)
    };
    let gray = (mn + (mid - mn) * w[secondary] + (mx - mid) * w[primary]).clamp(0.0, 1.0);
    if p[1][2] > 0.5 {
        set_lum([p[2][0], p[2][1], p[2][2]], gray)
    } else {
        [gray; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Vec<[f32; 3]> {
        let v = [0.0, 0.1, 0.25, 0.5, 0.73, 0.9, 1.0];
        let mut out = Vec::new();
        for r in v {
            for g in v {
                for b in v {
                    out.push([r, g, b]);
                }
            }
        }
        out
    }

    fn close(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= eps)
    }

    fn run(a: &Adjustment, c: [f32; 3]) -> [f32; 3] {
        compile(a).apply(c, 0, 0)
    }

    #[test]
    fn defaults_are_identity() {
        for kind in ["levels", "curves", "hueSaturation", "brightnessContrast", "exposure", "vibrance", "colorBalance"] {
            let op = compile(&Adjustment::default_for(kind).unwrap());
            for c in grid() {
                let out = op.apply(c, 0, 0);
                assert!(close(out, c, 2e-3), "{kind}: {c:?} → {out:?}");
            }
        }
        let legacy = compile(&Adjustment::BrightnessContrast { brightness: 0.0, contrast: 0.0, legacy: true });
        for c in grid() {
            assert!(close(legacy.apply(c, 0, 0), c, 1e-3));
        }
    }

    #[test]
    fn levels_known_values() {
        let mut ch = [Levels::default(); 4];
        ch[0] = Levels { in_black: 50.0, in_white: 200.0, gamma: 1.0, out_black: 0.0, out_white: 255.0 };
        let a = Adjustment::Levels { channels: ch };
        assert!(close(run(&a, [50.0 / 255.0; 3]), [0.0; 3], 1e-3));
        assert!(close(run(&a, [200.0 / 255.0; 3]), [1.0; 3], 1e-3));
        assert!(close(run(&a, [125.0 / 255.0; 3]), [0.5; 3], 1e-3));
        assert!(close(run(&a, [0.0; 3]), [0.0; 3], 1e-6));
        // gamma 2 lifts the middle: 0.5^(1/2)
        ch[0] = Levels { gamma: 2.0, ..Levels::default() };
        assert!(close(run(&Adjustment::Levels { channels: ch }, [0.5; 3]), [0.5f32.sqrt(); 3], 1e-3));
        // output levels compress the range; the red channel alone
        let mut ch = [Levels::default(); 4];
        ch[1] = Levels { out_black: 51.0, out_white: 204.0, ..Levels::default() };
        let out = run(&Adjustment::Levels { channels: ch }, [0.0, 0.0, 1.0]);
        assert!(close(out, [0.2, 0.0, 1.0], 1e-3), "{out:?}");
    }

    #[test]
    fn curves_pass_through_points_and_stay_monotone() {
        let mut ch: [Vec<[f32; 2]>; 4] = std::array::from_fn(|_| vec![[0.0, 0.0], [255.0, 255.0]]);
        ch[0] = vec![[0.0, 0.0], [64.0, 40.0], [128.0, 200.0], [255.0, 255.0]];
        let op = compile(&Adjustment::Curves { channels: ch.clone() });
        assert!(close(op.apply([128.0 / 255.0; 3], 0, 0), [200.0 / 255.0; 3], 2e-3));
        assert!(close(op.apply([64.0 / 255.0; 3], 0, 0), [40.0 / 255.0; 3], 2e-3));
        let mut prev = -1.0;
        for i in 0..=255 {
            let y = op.apply([i as f32 / 255.0; 3], 0, 0)[0];
            assert!(y >= prev - 1e-6, "not monotone at {i}");
            prev = y;
        }
        // a blue-only curve leaves red and green alone
        let mut ch: [Vec<[f32; 2]>; 4] = std::array::from_fn(|_| vec![[0.0, 0.0], [255.0, 255.0]]);
        ch[3] = vec![[0.0, 255.0], [255.0, 0.0]];
        let out = run(&Adjustment::Curves { channels: ch }, [0.2, 0.4, 0.25]);
        assert!(close(out, [0.2, 0.4, 0.75], 2e-3), "{out:?}");
    }

    #[test]
    fn hue_saturation_known_values() {
        let mut a = Adjustment::default_for("hueSaturation").unwrap();
        if let Adjustment::HueSaturation { master, .. } = &mut a {
            master.hue = 120.0;
        }
        assert!(close(run(&a, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0], 1e-3));
        if let Adjustment::HueSaturation { master, .. } = &mut a {
            *master = Hsl { hue: 0.0, saturation: -100.0, lightness: 0.0 };
        }
        // fully desaturated: gray at the HSL lightness
        assert!(close(run(&a, [0.8, 0.2, 0.4]), [0.5; 3], 1e-3));
        if let Adjustment::HueSaturation { master, .. } = &mut a {
            *master = Hsl { hue: 0.0, saturation: 0.0, lightness: 100.0 };
        }
        assert!(close(run(&a, [0.3, 0.6, 0.1]), [1.0; 3], 1e-3));
        // a reds-only change leaves blue alone and grays alone
        let mut a = Adjustment::default_for("hueSaturation").unwrap();
        if let Adjustment::HueSaturation { ranges, .. } = &mut a {
            ranges[0] = Hsl { hue: 0.0, saturation: -100.0, lightness: 0.0 };
        }
        assert!(close(run(&a, [1.0, 0.0, 0.0]), [0.5; 3], 1e-3));
        assert!(close(run(&a, [0.0, 0.0, 1.0]), [0.0, 0.0, 1.0], 1e-3));
        assert!(close(run(&a, [0.4; 3]), [0.4; 3], 1e-3));
        // colorize: mid-gray becomes the colorize hue at full saturation
        let mut a = Adjustment::default_for("hueSaturation").unwrap();
        if let Adjustment::HueSaturation { colorize, colorize_hsl, .. } = &mut a {
            *colorize = true;
            *colorize_hsl = Hsl { hue: 240.0, saturation: 100.0, lightness: 0.0 };
        }
        assert!(close(run(&a, [0.5; 3]), [0.0, 0.0, 1.0], 1e-3));
    }

    #[test]
    fn hue_ranges_sum_to_one() {
        for h in 0..360 {
            let s: f32 = (0..6).map(|i| range_weight(h as f32, i as f32 * 60.0)).sum();
            assert!((s - 1.0).abs() < 1e-5, "{h}: {s}");
        }
    }

    #[test]
    fn hsl_round_trips() {
        for c in grid() {
            let [h, s, l] = rgb_to_hsl(c);
            assert!(close(hsl_to_rgb(h, s, l), c, 1e-4), "{c:?}");
        }
    }

    #[test]
    fn brightness_contrast_curves() {
        for legacy in [false, true] {
            let up = compile(&Adjustment::BrightnessContrast { brightness: 60.0, contrast: 0.0, legacy });
            let more = compile(&Adjustment::BrightnessContrast { brightness: 0.0, contrast: 60.0, legacy });
            let mut prev = -1.0;
            for i in 0..=255 {
                let x = i as f32 / 255.0;
                let y = up.apply([x; 3], 0, 0)[0];
                assert!(y >= x - 1e-6 && y >= prev - 1e-6);
                prev = y;
                // more contrast pushes values away from mid-gray
                let z = more.apply([x; 3], 0, 0)[0];
                assert!((z - 0.5).abs() >= (x - 0.5).abs() - 1e-4, "{legacy} {x} → {z}");
            }
        }
        // modern keeps black and white; legacy brightness clips
        let m = compile(&Adjustment::BrightnessContrast { brightness: 100.0, contrast: 100.0, legacy: false });
        assert!(close(m.apply([0.0; 3], 0, 0), [0.0; 3], 1e-4) && close(m.apply([1.0; 3], 0, 0), [1.0; 3], 1e-4));
        let l = compile(&Adjustment::BrightnessContrast { brightness: 51.0, contrast: 0.0, legacy: true });
        assert!(close(l.apply([0.0; 3], 0, 0), [0.2; 3], 2e-3));
    }

    #[test]
    fn exposure_is_in_linear_light() {
        let a = Adjustment::Exposure { exposure: 1.0, offset: 0.0, gamma: 1.0 };
        let x = linear_to_srgb(0.25);
        assert!(close(run(&a, [x; 3]), [linear_to_srgb(0.5); 3], 2e-3));
        let g = Adjustment::Exposure { exposure: 0.0, offset: 0.0, gamma: 2.0 };
        assert!(close(run(&g, [linear_to_srgb(0.25); 3]), [linear_to_srgb(0.5); 3], 2e-3));
    }

    #[test]
    fn vibrance_favors_weak_colors_and_keeps_grays() {
        let a = Adjustment::Vibrance { vibrance: 100.0, saturation: 0.0 };
        assert!(close(run(&a, [0.4; 3]), [0.4; 3], 1e-4));
        let weak = rgb_to_hsl(run(&a, hsl_to_rgb(200.0, 0.2, 0.5)))[1] / 0.2;
        let strong = rgb_to_hsl(run(&a, hsl_to_rgb(200.0, 0.8, 0.5)))[1] / 0.8;
        assert!(weak > strong && strong >= 1.0, "{weak} {strong}");
        // skin hues get less
        let skin = rgb_to_hsl(run(&a, hsl_to_rgb(25.0, 0.2, 0.5)))[1] / 0.2;
        assert!(skin < weak);
    }

    #[test]
    fn color_balance_shifts_and_keeps_luminosity() {
        let a = Adjustment::ColorBalance { shadows: [0.0; 3], midtones: [100.0, 0.0, 0.0], highlights: [0.0; 3], preserve_luminosity: false };
        let out = run(&a, [0.5; 3]);
        assert!(out[0] > 0.6 && (out[1] - 0.5).abs() < 1e-4, "{out:?}");
        let p = Adjustment::ColorBalance { shadows: [0.0; 3], midtones: [100.0, 0.0, -50.0], highlights: [0.0; 3], preserve_luminosity: true };
        let c = [0.3, 0.5, 0.6];
        assert!((lum(run(&p, c)) - lum(c)).abs() < 1e-4);
    }

    #[test]
    fn black_white_default_weights() {
        let a = Adjustment::default_for("blackWhite").unwrap();
        let cases = [
            ([1.0, 0.0, 0.0], 0.4),
            ([1.0, 1.0, 0.0], 0.6),
            ([0.0, 1.0, 0.0], 0.4),
            ([0.0, 1.0, 1.0], 0.6),
            ([0.0, 0.0, 1.0], 0.2),
            ([1.0, 0.0, 1.0], 0.8),
            ([1.0, 1.0, 1.0], 1.0),
            ([0.3, 0.3, 0.3], 0.3),
        ];
        for (c, g) in cases {
            assert!(close(run(&a, c), [g; 3], 1e-4), "{c:?}");
        }
        // half-saturated red: 0.5 + 0.5·0.4
        assert!(close(run(&a, [1.0, 0.5, 0.5]), [0.7; 3], 1e-4));
        // a tint keeps the gray's luminosity
        let t = Adjustment::BlackWhite { weights: [40.0, 60.0, 40.0, 60.0, 20.0, 80.0], tint: true, tint_color: [225, 211, 179] };
        assert!((lum(run(&t, [0.0, 1.0, 1.0])) - 0.6).abs() < 1e-4);
    }

    #[test]
    fn gradient_map_follows_luminosity() {
        let a = Adjustment::default_for("gradientMap").unwrap();
        for c in grid() {
            assert!(close(run(&a, c), [lum(c); 3], 2e-3), "{c:?}");
        }
        let r = Adjustment::GradientMap {
            stops: vec![GradientStop { pos: 0.0, color: [255, 0, 0] }, GradientStop { pos: 1.0, color: [0, 0, 255] }],
            reverse: true,
            dither: false,
        };
        assert!(close(run(&r, [1.0; 3]), [1.0, 0.0, 0.0], 1e-3));
        assert!(close(run(&r, [0.5; 3]), [0.5, 0.0, 0.5], 3e-3));
    }

    #[test]
    fn invert_and_threshold() {
        assert!(close(run(&Adjustment::Invert, [0.2, 0.5, 1.0]), [0.8, 0.5, 0.0], 1e-4));
        let t = Adjustment::Threshold { level: 128.0 };
        assert_eq!(run(&t, [128.0 / 255.0; 3]), [1.0; 3]);
        assert_eq!(run(&t, [127.0 / 255.0; 3]), [0.0; 3]);
    }

    #[test]
    fn accum_mix_keeps_alpha_and_follows_the_mask() {
        let mut acc = Accum::new();
        for p in acc.0.iter_mut() {
            *p = [0.1, 0.2, 0.3, 0.5]; // straight (0.2, 0.4, 0.6) at half alpha
        }
        let op = compile(&Adjustment::Invert);
        let mut mask = vec![1.0; crate::TILE_PX];
        mask[1] = 0.5;
        mask[2] = 0.0;
        apply_to_accum(&mut acc, &op, BlendMode::Normal, 1.0, Some(&mask), (0, 0));
        let st = |p: [f32; 4]| [p[0] / p[3], p[1] / p[3], p[2] / p[3]];
        assert!(close(st(acc.0[0]), [0.8, 0.6, 0.4], 1e-5) && acc.0[0][3] == 0.5);
        assert!(close(st(acc.0[1]), [0.5, 0.5, 0.5], 1e-5));
        assert!(close(st(acc.0[2]), [0.2, 0.4, 0.6], 1e-5));
        // a blend mode combines the adjusted color with the original: multiply by the inverse
        let mut acc = Accum::new();
        acc.0[0] = [0.5, 0.5, 0.5, 1.0];
        apply_to_accum(&mut acc, &op, BlendMode::Multiply, 1.0, None, (0, 0));
        assert!(close(st(acc.0[0]), [0.25; 3], 1e-5));
    }

    #[test]
    fn settings_round_trip_as_json() {
        for kind in ["levels", "curves", "hueSaturation", "brightnessContrast", "exposure", "vibrance", "colorBalance", "blackWhite", "gradientMap", "invert", "threshold"] {
            let a = Adjustment::default_for(kind).unwrap();
            assert_eq!(a.kind(), kind);
            let json = serde_json::to_string(&a).unwrap();
            assert!(json.contains(&format!("\"kind\":\"{kind}\"")), "{json}");
            assert_eq!(serde_json::from_str::<Adjustment>(&json).unwrap(), a);
        }
    }
}
