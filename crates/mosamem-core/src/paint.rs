//! Brush engine math: dab placement, tip profile, and the stroke buffer.
//!
//! Model (docs/research.md §5.1): during a stroke, dabs build up a coverage buffer C in [0, 1] with
//! `C += flow·dab·(1 − C)`. The layer shows its original pixels with the paint laid over at alpha
//! `C·opacity`, so overlapping dabs inside one stroke never exceed the opacity, while flow sets how
//! fast they get there. The stroke is committed as one history step.

use std::collections::HashMap;

use crate::tile::{TILE, TILE_PX};

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Brush {
    /// px
    pub diameter: f32,
    /// 0–1: share of the radius that is fully solid.
    pub hardness: f32,
    /// Distance between dabs as a fraction of the diameter.
    pub spacing: f32,
    /// 0–1: per-dab strength (how fast paint builds toward the opacity).
    pub flow: f32,
    /// 0–1: ceiling for the whole stroke.
    pub opacity: f32,
    /// Pen pressure controls the size (down to `min_size` of the diameter).
    pub pressure_size: bool,
    /// Pen pressure controls the per-dab strength.
    pub pressure_opacity: bool,
    pub min_size: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Brush { diameter: 30.0, hardness: 0.0, spacing: 0.25, flow: 1.0, opacity: 1.0, pressure_size: true, pressure_opacity: false, min_size: 0.1 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// flow × pressure (when pressure controls strength)
    pub strength: f32,
}

/// Coverage of a brush tip at distance `d` from its center. Hard tips get a one-pixel anti-aliased
/// edge; soft tips fall off smoothly from the solid core to the rim.
#[inline]
pub fn tip(d: f32, radius: f32, hardness: f32) -> f32 {
    if d >= radius + 0.5 {
        return 0.0;
    }
    let core = radius * hardness.clamp(0.0, 1.0);
    if hardness >= 0.995 || radius - core < 1.0 {
        return (radius - d + 0.5).clamp(0.0, 1.0);
    }
    if d <= core {
        return 1.0;
    }
    let t = ((d - core) / (radius - core)).min(1.0);
    let u = 1.0 - t * t;
    u * u
}

/// Turns pointer samples into evenly spaced dabs (by arc length), carrying the leftover distance
/// between calls so the spacing stays even across event batches.
#[derive(Default)]
pub struct Spacer {
    last: Option<(f32, f32, f32)>,
    carry: f32,
}

impl Spacer {
    pub fn push(&mut self, b: &Brush, x: f32, y: f32, pressure: f32, out: &mut Vec<Dab>) {
        let make = |x: f32, y: f32, p: f32| {
            let size = if b.pressure_size { b.min_size + (1.0 - b.min_size) * p } else { 1.0 };
            let strength = b.flow * if b.pressure_opacity { p } else { 1.0 };
            Dab { x, y, radius: (b.diameter * size / 2.0).max(0.5), strength }
        };
        let Some((lx, ly, lp)) = self.last else {
            out.push(make(x, y, pressure));
            self.last = Some((x, y, pressure));
            self.carry = 0.0;
            return;
        };
        let (dx, dy) = (x - lx, y - ly);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0.0 {
            return;
        }
        let mut pos = -self.carry;
        loop {
            // spacing follows the dab size at that point along the segment
            let t_here = (pos.max(0.0) / len).min(1.0);
            let p_here = lp + (pressure - lp) * t_here;
            let d = make(0.0, 0.0, p_here).radius * 2.0;
            let step = (d * b.spacing).max(1.0);
            pos += step;
            if pos > len {
                self.carry = len - (pos - step);
                break;
            }
            let t = pos / len;
            out.push(make(lx + dx * t, ly + dy * t, lp + (pressure - lp) * t));
        }
        self.last = Some((x, y, pressure));
    }
}

/// Centripetal Catmull-Rom interpolation through p1→p2 (with neighbours p0, p3), used to turn
/// smoothed pointer samples into a curve before spacing dabs.
pub fn catmull_rom(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), steps: usize) -> Vec<(f32, f32)> {
    let knot = |a: (f32, f32), b: (f32, f32), t: f32| t + (((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()).sqrt().max(1e-4);
    let (t0, t1) = (0.0, knot(p0, p1, 0.0));
    let t2 = knot(p1, p2, t1);
    let t3 = knot(p2, p3, t2);
    let lerp = |a: (f32, f32), b: (f32, f32), ta: f32, tb: f32, t: f32| {
        let w = (t - ta) / (tb - ta);
        (a.0 + (b.0 - a.0) * w, a.1 + (b.1 - a.1) * w)
    };
    (1..=steps)
        .map(|i| {
            let t = t1 + (t2 - t1) * i as f32 / steps as f32;
            let a1 = lerp(p0, p1, t0, t1, t);
            let a2 = lerp(p1, p2, t1, t2, t);
            let a3 = lerp(p2, p3, t2, t3, t);
            let b1 = lerp(a1, a2, t0, t2, t);
            let b2 = lerp(a2, a3, t1, t3, t);
            lerp(b1, b2, t1, t2, t)
        })
        .collect()
}

/// Per-tile coverage accumulated during one stroke.
#[derive(Default)]
pub struct StrokeBuffer {
    pub tiles: HashMap<(i32, i32), Box<[f32]>>,
}

/// Pixel bounds touched by a dab: (x0, y0, x1, y1), exclusive.
pub fn dab_bounds(d: &Dab) -> (i64, i64, i64, i64) {
    let r = d.radius + 1.0;
    ((d.x - r).floor() as i64, (d.y - r).floor() as i64, (d.x + r).ceil() as i64 + 1, (d.y + r).ceil() as i64 + 1)
}

impl StrokeBuffer {
    /// Adds a dab, limited to the canvas. `limit(x, y)` scales coverage per pixel (the selection);
    /// returns the tiles it touched.
    pub fn add(&mut self, d: &Dab, hardness: f32, w: u32, h: u32, limit: &dyn Fn(i64, i64) -> f32) -> Vec<(i32, i32)> {
        let (x0, y0, x1, y1) = dab_bounds(d);
        let (x0, y0) = (x0.max(0), y0.max(0));
        let (x1, y1) = (x1.min(w as i64), y1.min(h as i64));
        let mut touched = Vec::new();
        if x1 <= x0 || y1 <= y0 {
            return touched;
        }
        let t = TILE as i64;
        for ty in y0 / t..=(y1 - 1) / t {
            for tx in x0 / t..=(x1 - 1) / t {
                let key = (tx as i32, ty as i32);
                let buf = self.tiles.entry(key).or_insert_with(|| vec![0f32; TILE_PX].into_boxed_slice());
                let (bx0, by0) = (tx * t, ty * t);
                for y in y0.max(by0)..y1.min(by0 + t) {
                    let fy = y as f32 + 0.5 - d.y;
                    for x in x0.max(bx0)..x1.min(bx0 + t) {
                        let fx = x as f32 + 0.5 - d.x;
                        let c = tip((fx * fx + fy * fy).sqrt(), d.radius, hardness) * d.strength;
                        if c <= 0.0 {
                            continue;
                        }
                        let c = c * limit(x, y);
                        let i = ((y - by0) * t + (x - bx0)) as usize;
                        let v = &mut buf[i];
                        *v += c * (1.0 - *v);
                    }
                }
                touched.push(key);
            }
        }
        touched
    }
}

// ---- gradients ----

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientKind {
    Linear,
    Radial,
    Angle,
    Reflected,
    Diamond,
}

/// Position 0–1 along the gradient for a point, given the drag from `a` to `b`.
#[inline]
pub fn gradient_t(kind: GradientKind, x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-6);
    let len = len2.sqrt();
    let (px, py) = (x - a.0, y - a.1);
    match kind {
        GradientKind::Linear => ((px * dx + py * dy) / len2).clamp(0.0, 1.0),
        GradientKind::Reflected => ((px * dx + py * dy) / len2).abs().min(1.0),
        GradientKind::Radial => ((px * px + py * py).sqrt() / len).min(1.0),
        GradientKind::Diamond => {
            // L1 distance in the frame rotated to the drag direction
            let (ux, uy) = (dx / len, dy / len);
            let along = (px * ux + py * uy).abs();
            let across = (-px * uy + py * ux).abs();
            ((along + across) / len).min(1.0)
        }
        GradientKind::Angle => {
            let base = dy.atan2(dx);
            let ang = py.atan2(px) - base;
            let t = ang / std::f32::consts::TAU;
            t - t.floor()
        }
    }
}

/// 4×4 ordered-dither offset in (−0.5, 0.5), for banding-free gradients in 8 bits.
#[inline]
pub fn dither(x: i64, y: i64) -> f32 {
    const M: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    (M[((y & 3) * 4 + (x & 3)) as usize] as f32 + 0.5) / 16.0 - 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_profiles() {
        // hard: solid to the rim with a 1 px ramp
        assert_eq!(tip(0.0, 10.0, 1.0), 1.0);
        assert_eq!(tip(9.0, 10.0, 1.0), 1.0);
        assert_eq!(tip(10.0, 10.0, 1.0), 0.5);
        assert_eq!(tip(11.0, 10.0, 1.0), 0.0);
        // soft: monotone fall-off from the core
        let mut prev = 1.0;
        for i in 0..=20 {
            let v = tip(i as f32 * 0.5, 10.0, 0.3);
            assert!(v <= prev + 1e-6);
            prev = v;
        }
        assert_eq!(tip(2.9, 10.0, 0.3), 1.0);
        assert!(tip(9.99, 10.0, 0.3) < 0.01);
    }

    #[test]
    fn spacing_is_even_across_batches() {
        let b = Brush { diameter: 20.0, spacing: 0.25, pressure_size: false, ..Default::default() };
        let mut s = Spacer::default();
        let mut dabs = Vec::new();
        // the same line delivered in uneven pieces
        for x in [0.0, 3.0, 4.0, 17.0, 18.5, 40.0, 100.0] {
            s.push(&b, x, 0.0, 1.0, &mut dabs);
        }
        let xs: Vec<f32> = dabs.iter().map(|d| d.x).collect();
        for w in xs.windows(2) {
            assert!((w[1] - w[0] - 5.0).abs() < 1e-3, "{xs:?}");
        }
        assert_eq!(xs.len(), 21); // 0, 5, …, 100
    }

    #[test]
    fn pressure_scales_size_and_strength() {
        let b = Brush { diameter: 40.0, min_size: 0.0, pressure_size: true, pressure_opacity: true, flow: 1.0, ..Default::default() };
        let mut s = Spacer::default();
        let mut dabs = Vec::new();
        s.push(&b, 0.0, 0.0, 0.25, &mut dabs);
        assert_eq!(dabs[0].radius, 5.0);
        assert_eq!(dabs[0].strength, 0.25);
    }

    #[test]
    fn stroke_buffer_respects_opacity_ceiling_and_flow_buildup() {
        let mut buf = StrokeBuffer::default();
        let d = Dab { x: 128.0, y: 128.0, radius: 20.0, strength: 0.3 };
        for _ in 0..3 {
            buf.add(&d, 1.0, 512, 512, &|_, _| 1.0);
        }
        let c = buf.tiles[&(0, 0)][128 * 256 + 128];
        // 1 − (1 − 0.3)³
        assert!((c - (1.0 - 0.7f32.powi(3))).abs() < 1e-5);
        for _ in 0..50 {
            buf.add(&d, 1.0, 512, 512, &|_, _| 1.0);
        }
        assert!(buf.tiles[&(0, 0)][128 * 256 + 128] <= 1.0);
    }

    #[test]
    fn selection_limits_coverage() {
        let mut buf = StrokeBuffer::default();
        let d = Dab { x: 128.0, y: 128.0, radius: 50.0, strength: 1.0 };
        buf.add(&d, 1.0, 512, 512, &|x, _| if x < 128 { 1.0 } else { 0.0 });
        let t = &buf.tiles[&(0, 0)];
        assert_eq!(t[128 * 256 + 100], 1.0);
        assert_eq!(t[128 * 256 + 150], 0.0);
    }

    #[test]
    fn dabs_across_tile_borders_touch_every_tile() {
        let mut buf = StrokeBuffer::default();
        let touched = buf.add(&Dab { x: 256.0, y: 256.0, radius: 10.0, strength: 1.0 }, 1.0, 1024, 1024, &|_, _| 1.0);
        assert_eq!(touched.len(), 4);
    }

    #[test]
    fn gradient_positions() {
        let (a, b) = ((0.0, 0.0), (100.0, 0.0));
        assert_eq!(gradient_t(GradientKind::Linear, 50.0, 7.0, a, b), 0.5);
        assert_eq!(gradient_t(GradientKind::Linear, -5.0, 0.0, a, b), 0.0);
        assert_eq!(gradient_t(GradientKind::Reflected, -50.0, 0.0, a, b), 0.5);
        assert_eq!(gradient_t(GradientKind::Radial, 0.0, 50.0, a, b), 0.5);
        assert!((gradient_t(GradientKind::Diamond, 25.0, 25.0, a, b) - 0.5).abs() < 1e-6);
        assert!((gradient_t(GradientKind::Angle, 0.0, 10.0, a, b) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn catmull_rom_passes_through_the_points() {
        let pts = catmull_rom((0.0, 0.0), (10.0, 0.0), (20.0, 10.0), (30.0, 10.0), 8);
        let last = pts.last().unwrap();
        assert!((last.0 - 20.0).abs() < 1e-3 && (last.1 - 10.0).abs() < 1e-3);
    }
}
