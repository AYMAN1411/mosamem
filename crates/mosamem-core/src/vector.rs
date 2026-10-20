//! Vector shapes and paths (M12): Bézier paths made of knots, the parametric shapes (rectangle with
//! rounded corners, ellipse, polygon, star, line with arrowheads) that turn into paths, editing
//! helpers for the Pen tool (splitting a segment, finding the nearest one), and drawing with a fill
//! and a stroke (inside, centered or outside) through tiny-skia.
//!
//! Coordinates are document px, y down. The math is the standard one: cubic Béziers, de Casteljau
//! subdivision, and the 4·(√2 − 1)/3 handle length for quarter circles.

use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Point, Shader, SpreadMode, StrokeDash, Transform};

use crate::adjust::GradientStop;

pub type Pt = [f64; 2];

/// An anchor and its two handles (absolute positions; a handle on its anchor means none).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Knot {
    pub p: Pt,
    /// Handle toward the previous knot.
    pub i: Pt,
    /// Handle toward the next knot.
    pub o: Pt,
    /// Smooth knots keep their handles in line when one moves.
    pub smooth: bool,
}

impl Knot {
    pub fn corner(p: Pt) -> Self {
        Knot { p, i: p, o: p, smooth: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubPath {
    pub knots: Vec<Knot>,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Path {
    pub subs: Vec<SubPath>,
}

/// The cubic segments of a subpath: (from, control 1, control 2, to).
pub fn segments(s: &SubPath) -> Vec<[Pt; 4]> {
    let n = s.knots.len();
    if n < 2 {
        return vec![];
    }
    let count = if s.closed { n } else { n - 1 };
    (0..count)
        .map(|k| {
            let a = &s.knots[k];
            let b = &s.knots[(k + 1) % n];
            [a.p, a.o, b.i, b.p]
        })
        .collect()
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// A point on a cubic segment.
pub fn point_at(c: &[Pt; 4], t: f64) -> Pt {
    let (a, b, d) = (lerp(c[0], c[1], t), lerp(c[1], c[2], t), lerp(c[2], c[3], t));
    let (e, f) = (lerp(a, b, t), lerp(b, d, t));
    lerp(e, f, t)
}

impl Path {
    pub fn is_empty(&self) -> bool {
        self.subs.iter().all(|s| s.knots.is_empty())
    }

    /// The path under an affine map m = [a, b, c, d, e, f] (x' = a·x + c·y + e, y' = b·x + d·y + f).
    pub fn transformed(&self, m: [f64; 6]) -> Path {
        let f = |p: Pt| [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];
        Path {
            subs: self
                .subs
                .iter()
                .map(|s| SubPath { closed: s.closed, knots: s.knots.iter().map(|k| Knot { p: f(k.p), i: f(k.i), o: f(k.o), smooth: k.smooth }).collect() })
                .collect(),
        }
    }

    /// Bounds of the anchors and the curves (sampled), as (x0, y0, x1, y1).
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut b: Option<(f64, f64, f64, f64)> = None;
        let mut add = |p: Pt| {
            b = Some(match b {
                None => (p[0], p[1], p[0], p[1]),
                Some((x0, y0, x1, y1)) => (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1])),
            })
        };
        for s in &self.subs {
            if let [k] = s.knots.as_slice() {
                add(k.p);
            }
            for c in segments(s) {
                for i in 0..=16 {
                    add(point_at(&c, i as f64 / 16.0));
                }
            }
        }
        b
    }

    /// Splits segment `seg` of subpath `sub` at `t` (de Casteljau), adding a smooth knot there. The
    /// curve's shape doesn't change.
    pub fn insert_knot(&mut self, sub: usize, seg: usize, t: f64) -> Option<usize> {
        let s = self.subs.get_mut(sub)?;
        let n = s.knots.len();
        let c = *segments(s).get(seg)?;
        let (a, b, d) = (lerp(c[0], c[1], t), lerp(c[1], c[2], t), lerp(c[2], c[3], t));
        let (e, f) = (lerp(a, b, t), lerp(b, d, t));
        let m = lerp(e, f, t);
        s.knots[seg].o = a;
        s.knots[(seg + 1) % n].i = d;
        let straight = c[0] == c[1] && c[2] == c[3];
        let k = Knot { p: m, i: e, o: f, smooth: !straight };
        s.knots.insert(seg + 1, k);
        Some(seg + 1)
    }

    /// The segment nearest to `p` within `tol` px: (subpath, segment, t, distance).
    pub fn nearest_segment(&self, p: Pt, tol: f64) -> Option<(usize, usize, f64, f64)> {
        let mut best: Option<(usize, usize, f64, f64)> = None;
        for (si, s) in self.subs.iter().enumerate() {
            for (k, c) in segments(s).iter().enumerate() {
                // coarse sampling, then refine around the best sample
                let mut bt = 0.0;
                let mut bd = f64::MAX;
                for i in 0..=64 {
                    let t = i as f64 / 64.0;
                    let q = point_at(c, t);
                    let d = (q[0] - p[0]).hypot(q[1] - p[1]);
                    if d < bd {
                        (bd, bt) = (d, t);
                    }
                }
                let mut step = 1.0 / 64.0;
                for _ in 0..20 {
                    step /= 2.0;
                    for t in [bt - step, bt + step] {
                        if (0.0..=1.0).contains(&t) {
                            let q = point_at(c, t);
                            let d = (q[0] - p[0]).hypot(q[1] - p[1]);
                            if d < bd {
                                (bd, bt) = (d, t);
                            }
                        }
                    }
                }
                if bd <= tol && best.is_none_or(|b| bd < b.3) {
                    best = Some((si, k, bt, bd));
                }
            }
        }
        best
    }

    /// Straight lines through the curves, `per` points per segment (for tests and hit tests).
    pub fn flatten(&self, per: usize) -> Vec<Vec<Pt>> {
        self.subs
            .iter()
            .map(|s| {
                let mut out = vec![];
                for c in segments(s) {
                    for i in 0..per {
                        out.push(point_at(&c, i as f64 / per as f64));
                    }
                }
                if !s.closed {
                    if let Some(k) = s.knots.last() {
                        out.push(k.p);
                    }
                }
                out
            })
            .collect()
    }

    fn to_skia(&self) -> Option<tiny_skia::Path> {
        let mut b = PathBuilder::new();
        for s in &self.subs {
            let Some(first) = s.knots.first() else { continue };
            b.move_to(first.p[0] as f32, first.p[1] as f32);
            for c in segments(s) {
                if c[0] == c[1] && c[2] == c[3] {
                    b.line_to(c[3][0] as f32, c[3][1] as f32);
                } else {
                    b.cubic_to(c[1][0] as f32, c[1][1] as f32, c[2][0] as f32, c[2][1] as f32, c[3][0] as f32, c[3][1] as f32);
                }
            }
            if s.closed {
                b.close();
            }
        }
        b.finish()
    }
}

// ---- shapes ----

/// The handle length of a quarter circle, as a fraction of its radius.
pub const KAPPA: f64 = 0.552_284_749_830_793_4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Shape {
    /// `radius` rounds every corner (clamped to half the short side).
    Rect { x: f64, y: f64, w: f64, h: f64, radius: f64 },
    Ellipse { x: f64, y: f64, w: f64, h: f64 },
    /// A regular polygon around (cx, cy); `angle` (degrees) turns it, 0 = a point at the top.
    Polygon { cx: f64, cy: f64, r: f64, sides: u32, angle: f64 },
    /// `inner` is the inner radius as a fraction of `r`.
    Star { cx: f64, cy: f64, r: f64, inner: f64, points: u32, angle: f64 },
    /// A straight line, drawn with the stroke; arrowheads use the stroke's color.
    Line { a: Pt, b: Pt, arrow_start: bool, arrow_end: bool },
    /// A path drawn with the Pen tool (or a shape whose points were edited).
    Free { path: Path },
}

fn rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> Path {
    let (x0, y0, x1, y1) = (x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h));
    let r = r.max(0.0).min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    if r <= 0.0 {
        let knots = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(Knot::corner).to_vec();
        return Path { subs: vec![SubPath { knots, closed: true }] };
    }
    let k = r * KAPPA;
    // clockwise from the top edge's left end; each corner is two knots joined by a quarter arc
    let knots = vec![
        Knot { p: [x0 + r, y0], i: [x0 + r - k, y0], o: [x0 + r, y0], smooth: true },
        Knot { p: [x1 - r, y0], i: [x1 - r, y0], o: [x1 - r + k, y0], smooth: true },
        Knot { p: [x1, y0 + r], i: [x1, y0 + r - k], o: [x1, y0 + r], smooth: true },
        Knot { p: [x1, y1 - r], i: [x1, y1 - r], o: [x1, y1 - r + k], smooth: true },
        Knot { p: [x1 - r, y1], i: [x1 - r + k, y1], o: [x1 - r, y1], smooth: true },
        Knot { p: [x0 + r, y1], i: [x0 + r, y1], o: [x0 + r - k, y1], smooth: true },
        Knot { p: [x0, y1 - r], i: [x0, y1 - r + k], o: [x0, y1 - r], smooth: true },
        Knot { p: [x0, y0 + r], i: [x0, y0 + r], o: [x0, y0 + r - k], smooth: true },
    ];
    Path { subs: vec![SubPath { knots, closed: true }] }
}

fn ellipse_path(x: f64, y: f64, w: f64, h: f64) -> Path {
    let (rx, ry) = (w.abs() / 2.0, h.abs() / 2.0);
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let knots = vec![
        Knot { p: [cx, cy - ry], i: [cx - kx, cy - ry], o: [cx + kx, cy - ry], smooth: true },
        Knot { p: [cx + rx, cy], i: [cx + rx, cy - ky], o: [cx + rx, cy + ky], smooth: true },
        Knot { p: [cx, cy + ry], i: [cx + kx, cy + ry], o: [cx - kx, cy + ry], smooth: true },
        Knot { p: [cx - rx, cy], i: [cx - rx, cy + ky], o: [cx - rx, cy - ky], smooth: true },
    ];
    Path { subs: vec![SubPath { knots, closed: true }] }
}

fn ring(cx: f64, cy: f64, radii: &[f64], n: u32, angle: f64) -> Path {
    let count = n as usize * radii.len();
    let knots = (0..count)
        .map(|i| {
            let a = (angle - 90.0).to_radians() + i as f64 * std::f64::consts::TAU / count as f64;
            let r = radii[i % radii.len()];
            Knot::corner([cx + r * a.cos(), cy + r * a.sin()])
        })
        .collect();
    Path { subs: vec![SubPath { knots, closed: true }] }
}

/// Arrowhead size for a stroke width: a triangle 3× the width wide and 3× long, at least 6 px.
fn arrow(tip: Pt, from: Pt, width: f64) -> SubPath {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let len = dx.hypot(dy).max(1e-9);
    let (ux, uy) = (dx / len, dy / len);
    let s = (width * 3.0).max(6.0);
    let base = [tip[0] - ux * s, tip[1] - uy * s];
    let knots = vec![Knot::corner(tip), Knot::corner([base[0] - uy * s / 2.0, base[1] + ux * s / 2.0]), Knot::corner([base[0] + uy * s / 2.0, base[1] - ux * s / 2.0])];
    SubPath { knots, closed: true }
}

impl Shape {
    /// The outline (for a Line: the line itself, as an open path).
    pub fn path(&self) -> Path {
        match self {
            Shape::Rect { x, y, w, h, radius } => rect_path(*x, *y, *w, *h, *radius),
            Shape::Ellipse { x, y, w, h } => ellipse_path(*x, *y, *w, *h),
            Shape::Polygon { cx, cy, r, sides, angle } => ring(*cx, *cy, &[*r], (*sides).max(3), *angle),
            Shape::Star { cx, cy, r, inner, points, angle } => ring(*cx, *cy, &[*r, r * inner.clamp(0.01, 1.0)], (*points).max(3), *angle),
            Shape::Line { a, b, .. } => Path { subs: vec![SubPath { knots: vec![Knot::corner(*a), Knot::corner(*b)], closed: false }] },
            Shape::Free { path } => path.clone(),
        }
    }

    /// The shape under an affine map. Rectangles and ellipses stay parametric under moves and
    /// scales; anything else (rotation, skew) becomes a free path.
    pub fn transformed(&self, m: [f64; 6]) -> Shape {
        let axis_aligned = m[1].abs() < 1e-12 && m[2].abs() < 1e-12;
        let f = |p: Pt| [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]];
        match self {
            Shape::Rect { x, y, w, h, radius } if axis_aligned && (m[0].abs() - m[3].abs()).abs() < 1e-9 => {
                let p = f([*x, *y]);
                Shape::Rect { x: p[0], y: p[1], w: w * m[0], h: h * m[3], radius: radius * m[0].abs() }
            }
            Shape::Ellipse { x, y, w, h } if axis_aligned => {
                let p = f([*x, *y]);
                Shape::Ellipse { x: p[0], y: p[1], w: w * m[0], h: h * m[3] }
            }
            Shape::Line { a, b, arrow_start, arrow_end } => Shape::Line { a: f(*a), b: f(*b), arrow_start: *arrow_start, arrow_end: *arrow_end },
            _ => Shape::Free { path: self.path().transformed(m) },
        }
    }
}

// ---- paint ----

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Fill {
    Solid { color: [u8; 4] },
    /// Across the shape's bounds at `angle` degrees (0 = left to right, 90 = bottom to top).
    Linear { stops: Vec<GradientStop>, angle: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Inside,
    #[default]
    Center,
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stroke {
    pub color: [u8; 4],
    pub width: f64,
    #[serde(default)]
    pub align: Align,
    #[serde(default)]
    pub cap: Cap,
    #[serde(default)]
    pub join: Join,
    /// Dash and gap lengths as multiples of the width (empty: solid).
    #[serde(default)]
    pub dash: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShapePaint {
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

const MITER_LIMIT: f64 = 10.0;

/// Drawn pixels: straight RGBA8 at (x, y) in document px.
pub struct Rendered {
    pub x: i64,
    pub y: i64,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

fn paint_for<'a>(fill: &Fill, b: (f64, f64, f64, f64)) -> Paint<'a> {
    let mut p = Paint { anti_alias: true, ..Default::default() };
    match fill {
        Fill::Solid { color } => p.set_color_rgba8(color[0], color[1], color[2], color[3]),
        Fill::Linear { stops, angle } => {
            let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
            let a = angle.to_radians();
            let (dx, dy) = (a.cos(), -a.sin());
            // half the extent of the bounds along the gradient's direction
            let half = ((b.2 - b.0) * dx.abs() + (b.3 - b.1) * dy.abs()) / 2.0;
            let start = Point::from_xy((cx - dx * half) as f32, (cy - dy * half) as f32);
            let end = Point::from_xy((cx + dx * half) as f32, (cy + dy * half) as f32);
            let st: Vec<tiny_skia::GradientStop> =
                stops.iter().map(|s| tiny_skia::GradientStop::new(s.pos, tiny_skia::Color::from_rgba8(s.color[0], s.color[1], s.color[2], 255))).collect();
            let first = stops.first().map_or(tiny_skia::Color::BLACK, |s| tiny_skia::Color::from_rgba8(s.color[0], s.color[1], s.color[2], 255));
            p.shader = linear_shader(start, end, st, first);
        }
    }
    p
}

/// A linear gradient, or the first color when it can't be made (one stop, or zero length).
fn linear_shader(start: Point, end: Point, stops: Vec<tiny_skia::GradientStop>, first: tiny_skia::Color) -> Shader<'static> {
    tiny_skia::LinearGradient::new(start, end, stops, SpreadMode::Pad, Transform::identity()).unwrap_or(Shader::SolidColor(first))
}

fn skia_stroke(s: &Stroke, width: f64) -> tiny_skia::Stroke {
    let dash = (!s.dash.is_empty() && s.dash.iter().all(|d| *d > 0.0)).then(|| {
        let mut v: Vec<f32> = s.dash.iter().map(|d| (d * s.width) as f32).collect();
        if v.len() % 2 == 1 {
            v.extend(v.clone());
        }
        StrokeDash::new(v, 0.0)
    });
    tiny_skia::Stroke {
        width: width as f32,
        miter_limit: MITER_LIMIT as f32,
        line_cap: match s.cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
            Cap::Square => LineCap::Square,
        },
        line_join: match s.join {
            Join::Miter => LineJoin::Miter,
            Join::Round => LineJoin::Round,
            Join::Bevel => LineJoin::Bevel,
        },
        dash: dash.flatten(),
    }
}

/// Draws a shape with its fill and stroke, clipped to `clip` (x0, y0, x1, y1).
pub fn render(shape: &Shape, paint: &ShapePaint, clip: (i64, i64, i64, i64)) -> Option<Rendered> {
    let path = shape.path();
    let line = matches!(shape, Shape::Line { .. });
    let sk = path.to_skia()?;
    let pb = path.bounds()?;
    // room outside the outline: half the stroke (all of it when outside) times the miter limit,
    // and the arrowheads of a line
    let reach = paint.stroke.as_ref().map_or(0.0, |s| {
        let band = match s.align {
            Align::Inside => 0.0,
            Align::Center => s.width / 2.0,
            Align::Outside => s.width,
        };
        let heads = if line { (s.width * 3.0).max(6.0) } else { 0.0 };
        (band * MITER_LIMIT).max(heads)
    }) + 2.0;
    let x0 = ((pb.0 - reach).floor() as i64).max(clip.0);
    let y0 = ((pb.1 - reach).floor() as i64).max(clip.1);
    let x1 = ((pb.2 + reach).ceil() as i64).min(clip.2);
    let y1 = ((pb.3 + reach).ceil() as i64).min(clip.3);
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut pm = Pixmap::new(w, h)?;
    let t = Transform::from_translate(-x0 as f32, -y0 as f32);
    // the shape's own area (also the clip for inside/outside strokes)
    let area = || {
        let mut m = Mask::new(w, h)?;
        m.fill_path(&sk, FillRule::Winding, true, t);
        Some(m)
    };
    if let (Some(fill), false) = (&paint.fill, line) {
        pm.fill_path(&sk, &paint_for(fill, pb), FillRule::Winding, t, None);
    }
    if let Some(s) = paint.stroke.as_ref().filter(|s| s.width > 0.0) {
        let solid = Fill::Solid { color: s.color };
        let p = paint_for(&solid, pb);
        // a line with arrowheads stops at their bases, so it can't poke out of their tips
        let shortened = match shape {
            Shape::Line { a, b, arrow_start, arrow_end } if *arrow_start || *arrow_end => {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = dx.hypot(dy).max(1e-9);
                let cut = (s.width * 3.0).max(6.0).min(len / 2.0);
                let (ux, uy) = (dx / len * cut, dy / len * cut);
                let a2 = if *arrow_start { [a[0] + ux, a[1] + uy] } else { *a };
                let b2 = if *arrow_end { [b[0] - ux, b[1] - uy] } else { *b };
                Shape::Line { a: a2, b: b2, arrow_start: false, arrow_end: false }.path().to_skia()
            }
            _ => None,
        };
        let body = shortened.as_ref().unwrap_or(&sk);
        match (s.align, line) {
            (Align::Center, _) | (_, true) => pm.stroke_path(body, &p, &skia_stroke(s, s.width), t, None),
            (Align::Inside, false) => {
                let m = area()?;
                pm.stroke_path(&sk, &p, &skia_stroke(s, s.width * 2.0), t, Some(&m));
            }
            (Align::Outside, false) => {
                let mut m = area()?;
                m.invert();
                pm.stroke_path(&sk, &p, &skia_stroke(s, s.width * 2.0), t, Some(&m));
            }
        }
        if let Shape::Line { a, b, arrow_start, arrow_end } = shape {
            let mut heads = Path::default();
            if *arrow_end {
                heads.subs.push(arrow(*b, *a, s.width));
            }
            if *arrow_start {
                heads.subs.push(arrow(*a, *b, s.width));
            }
            if let Some(hp) = heads.to_skia() {
                pm.fill_path(&hp, &p, FillRule::Winding, t, None);
            }
        }
    }
    let mut rgba = pm.take();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in 0..3 {
                px[c] = ((px[c] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(Rendered { x: x0, y: y0, w, h, rgba })
}

/// The path's area as a coverage mask (0–255) over a w×h canvas (non-zero winding), for turning a
/// path into a selection.
pub fn coverage(path: &Path, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h) as usize];
    let (Some(sk), Some(mut m)) = (path.to_skia(), Mask::new(w, h)) else { return out };
    m.fill_path(&sk, FillRule::Winding, true, Transform::identity());
    out.copy_from_slice(m.data());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(r: &Rendered) -> f64 {
        r.rgba.chunks_exact(4).map(|p| p[3] as f64 / 255.0).sum()
    }
    fn fill(color: [u8; 4]) -> ShapePaint {
        ShapePaint { fill: Some(Fill::Solid { color }), stroke: None }
    }
    fn stroked(width: f64, align: Align) -> ShapePaint {
        ShapePaint { fill: None, stroke: Some(Stroke { color: [0, 0, 0, 255], width, align, cap: Cap::Butt, join: Join::Miter, dash: vec![] }) }
    }
    const CLIP: (i64, i64, i64, i64) = (-1000, -1000, 4000, 4000);

    #[test]
    fn filled_areas_match_the_formulas() {
        let rect = render(&Shape::Rect { x: 10.0, y: 20.0, w: 100.0, h: 50.0, radius: 0.0 }, &fill([255, 0, 0, 255]), CLIP).unwrap();
        assert!((area(&rect) - 5000.0).abs() < 1.0, "{}", area(&rect));
        let circle = render(&Shape::Ellipse { x: 0.0, y: 0.0, w: 200.0, h: 200.0 }, &fill([0, 0, 0, 255]), CLIP).unwrap();
        let want = std::f64::consts::PI * 100.0 * 100.0;
        assert!((area(&circle) - want).abs() / want < 0.002, "{} vs {want}", area(&circle));
        // a rounded rectangle loses (4 − π)·r² from its corners
        let rr = render(&Shape::Rect { x: 0.0, y: 0.0, w: 200.0, h: 100.0, radius: 20.0 }, &fill([0, 0, 0, 255]), CLIP).unwrap();
        let want = 20000.0 - (4.0 - std::f64::consts::PI) * 400.0;
        assert!((area(&rr) - want).abs() / want < 0.002, "{} vs {want}", area(&rr));
        // a regular hexagon: (3√3/2)·r²
        let hex = render(&Shape::Polygon { cx: 300.0, cy: 300.0, r: 100.0, sides: 6, angle: 0.0 }, &fill([0, 0, 0, 255]), CLIP).unwrap();
        let want = 1.5 * 3f64.sqrt() * 10000.0;
        assert!((area(&hex) - want).abs() / want < 0.002, "{} vs {want}", area(&hex));
    }

    #[test]
    fn stroke_alignment_puts_the_band_where_asked() {
        let r = Shape::Rect { x: 100.0, y: 100.0, w: 100.0, h: 50.0, radius: 0.0 };
        // a 4 px band: centered 104·54 − 96·46, inside 100·50 − 92·42, outside 108·58 − 100·50
        for (align, want) in [(Align::Center, 1200.0), (Align::Inside, 1136.0), (Align::Outside, 1264.0)] {
            let got = area(&render(&r, &stroked(4.0, align), CLIP).unwrap());
            assert!((got - want).abs() < 6.0, "{align:?}: {got} vs {want}");
        }
    }

    #[test]
    fn dashes_cover_about_half() {
        let line = Shape::Line { a: [0.0, 50.0], b: [400.0, 50.0], arrow_start: false, arrow_end: false };
        let mut p = stroked(4.0, Align::Center);
        let solid = area(&render(&line, &p, CLIP).unwrap());
        p.stroke.as_mut().unwrap().dash = vec![2.0, 2.0];
        let dashed = area(&render(&line, &p, CLIP).unwrap());
        assert!((solid - 1600.0).abs() < 4.0, "{solid}");
        assert!((dashed / solid - 0.5).abs() < 0.03, "{dashed}");
    }

    #[test]
    fn arrowheads_add_their_triangles() {
        let mut l = Shape::Line { a: [0.0, 50.0], b: [300.0, 50.0], arrow_start: false, arrow_end: false };
        let p = stroked(4.0, Align::Center);
        let plain = area(&render(&l, &p, CLIP).unwrap());
        if let Shape::Line { arrow_end, .. } = &mut l {
            *arrow_end = true;
        }
        let with = area(&render(&l, &p, CLIP).unwrap());
        // a 12 × 12 triangle (area 72) replaces the line's last 12 px (12 × 4)
        assert!((with - plain - 24.0).abs() < 3.0, "{plain} → {with}");
    }

    #[test]
    fn the_ellipse_stays_on_the_circle() {
        let p = Shape::Ellipse { x: -100.0, y: -100.0, w: 200.0, h: 200.0 }.path();
        let worst = p.flatten(64)[0].iter().map(|q| (q[0].hypot(q[1]) - 100.0).abs()).fold(0.0, f64::max);
        assert!(worst < 0.03, "{worst}");
    }

    #[test]
    fn splitting_a_segment_keeps_the_shape() {
        let mut p = Shape::Ellipse { x: 0.0, y: 0.0, w: 300.0, h: 160.0 }.path();
        let before = p.flatten(400);
        let k = p.insert_knot(0, 1, 0.37).unwrap();
        assert_eq!(k, 2);
        assert_eq!(p.subs[0].knots.len(), 5);
        let after = p.flatten(400)[0].clone();
        // every point of the new curve lies on the old one
        for q in &after {
            let d = before[0].iter().map(|r| (r[0] - q[0]).hypot(r[1] - q[1])).fold(f64::MAX, f64::min);
            assert!(d < 0.5, "{q:?} off by {d}");
        }
    }

    #[test]
    fn the_nearest_segment_is_found() {
        let p = Shape::Rect { x: 0.0, y: 0.0, w: 100.0, h: 50.0, radius: 0.0 }.path();
        let (s, seg, t, d) = p.nearest_segment([50.0, 52.0], 4.0).unwrap();
        assert_eq!((s, seg), (0, 2), "the bottom edge (it runs right to left)");
        assert!((t - 0.5).abs() < 1e-3 && (d - 2.0).abs() < 1e-6);
        assert!(p.nearest_segment([50.0, 25.0], 4.0).is_none());
    }

    #[test]
    fn moves_keep_shapes_parametric_and_turns_make_paths() {
        let r = Shape::Rect { x: 10.0, y: 10.0, w: 40.0, h: 20.0, radius: 5.0 };
        assert_eq!(r.transformed([2.0, 0.0, 0.0, 2.0, 5.0, -5.0]), Shape::Rect { x: 25.0, y: 15.0, w: 80.0, h: 40.0, radius: 10.0 });
        let (s, c) = (0.5f64.sqrt(), 0.5f64.sqrt());
        let turned = r.transformed([c, s, -s, c, 0.0, 0.0]);
        assert!(matches!(turned, Shape::Free { .. }));
        // the same outline, turned
        let a = r.path().transformed([c, s, -s, c, 0.0, 0.0]).flatten(8);
        assert_eq!(turned.path().flatten(8), a);
    }

    #[test]
    fn coverage_is_the_selection_of_the_area() {
        let p = Shape::Rect { x: 10.5, y: 10.0, w: 20.0, h: 10.0, radius: 0.0 }.path();
        let m = coverage(&p, 40, 30);
        let sum: f64 = m.iter().map(|v| *v as f64 / 255.0).sum();
        assert!((sum - 200.0).abs() < 0.5, "{sum}");
        assert_eq!(m[15 * 40 + 20], 255);
        assert_eq!(m[15 * 40 + 10], 128, "half a pixel at the left edge");
    }

    #[test]
    fn shapes_round_trip_through_json() {
        let s = Shape::Star { cx: 1.0, cy: 2.0, r: 3.0, inner: 0.5, points: 5, angle: 0.0 };
        let p = ShapePaint { fill: Some(Fill::Linear { stops: vec![GradientStop { pos: 0.0, color: [1, 2, 3] }, GradientStop { pos: 1.0, color: [4, 5, 6] }], angle: 90.0 }), stroke: Some(Stroke { color: [0, 0, 0, 255], width: 2.0, align: Align::Outside, cap: Cap::Round, join: Join::Bevel, dash: vec![1.0, 2.0] }) };
        let js = serde_json::to_string(&(s.clone(), p.clone())).unwrap();
        assert!(js.contains("\"kind\":\"star\"") && js.contains("\"align\":\"outside\""), "{js}");
        assert_eq!(serde_json::from_str::<(Shape, ShapePaint)>(&js).unwrap(), (s, p));
    }
}
