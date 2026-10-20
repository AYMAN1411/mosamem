//! Layer styles (effects): drop shadow, inner shadow, outer glow, stroke, color overlay and gradient
//! overlay. docs/research.md §8.
//!
//! Every effect starts from the layer's shape matte (its alpha, through its mask):
//! - spread/choke grow the matte with the exact distance transform, size blurs it (Gaussian);
//! - shadows are offset along the light angle;
//! - the stroke is a band of the signed distance to the shape's edge.
//!
//! `mattes` computes the effect coverage for one 256×256 tile from a region around it (the tile
//! plus `reach` px), so tiles join without seams. The engine blends the mattes with the colors.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::adjust::GradientStop;
use crate::blend::BlendMode;
use crate::select::{distance, gaussian};
use crate::tile::TILE;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shadow {
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: [u8; 3],
    /// 0–1
    pub opacity: f32,
    /// Light angle in degrees (counter-clockwise from the right); the shadow falls opposite.
    pub angle: f32,
    /// px
    pub distance: f32,
    /// 0–100: how much of `size` is solid (spread for drop shadows, choke for inner shadows).
    pub spread: f32,
    /// Blur size, px.
    pub size: f32,
    /// Drop shadows only: the layer's own shape cuts the shadow out beneath it.
    #[serde(default = "yes")]
    pub knockout: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlowTechnique {
    Softer,
    Precise,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Glow {
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: [u8; 3],
    pub opacity: f32,
    pub spread: f32,
    pub size: f32,
    pub technique: GlowTechnique,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StrokePosition {
    Outside,
    Inside,
    Center,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stroke {
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: [u8; 3],
    pub opacity: f32,
    /// px
    pub size: f32,
    pub position: StrokePosition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorOverlay {
    pub enabled: bool,
    pub blend: BlendMode,
    pub color: [u8; 3],
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverlayStyle {
    Linear,
    Radial,
    Reflected,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientOverlay {
    pub enabled: bool,
    pub blend: BlendMode,
    pub opacity: f32,
    pub stops: Vec<GradientStop>,
    pub style: OverlayStyle,
    /// degrees, counter-clockwise; 90 = from bottom to top
    pub angle: f32,
    /// percent of the layer's size
    pub scale: f32,
    pub reverse: bool,
}

/// A layer's effects. Each is optional and can be switched off without losing its settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_shadow: Option<Shadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_shadow: Option<Shadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_glow: Option<Glow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_overlay: Option<ColorOverlay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_overlay: Option<GradientOverlay>,
}

fn on<T>(e: &Option<T>, enabled: impl Fn(&T) -> bool) -> Option<&T> {
    e.as_ref().filter(|x| enabled(x))
}

impl LayerStyle {
    pub fn drop_shadow(&self) -> Option<&Shadow> {
        on(&self.drop_shadow, |s| s.enabled && s.opacity > 0.0)
    }
    pub fn inner_shadow(&self) -> Option<&Shadow> {
        on(&self.inner_shadow, |s| s.enabled && s.opacity > 0.0)
    }
    pub fn outer_glow(&self) -> Option<&Glow> {
        on(&self.outer_glow, |s| s.enabled && s.opacity > 0.0)
    }
    pub fn stroke(&self) -> Option<&Stroke> {
        on(&self.stroke, |s| s.enabled && s.opacity > 0.0 && s.size > 0.0)
    }
    pub fn color_overlay(&self) -> Option<&ColorOverlay> {
        on(&self.color_overlay, |s| s.enabled && s.opacity > 0.0)
    }
    pub fn gradient_overlay(&self) -> Option<&GradientOverlay> {
        on(&self.gradient_overlay, |s| s.enabled && s.opacity > 0.0)
    }

    /// True when any effect draws.
    pub fn active(&self) -> bool {
        self.drop_shadow().is_some() || self.inner_shadow().is_some() || self.outer_glow().is_some() || self.stroke().is_some() || self.color_overlay().is_some() || self.gradient_overlay().is_some()
    }

    /// How far (px at scale `s`) the effects read around a pixel.
    pub fn reach(&self, s: f32) -> u32 {
        let mut r: f32 = 0.0;
        if let Some(d) = self.drop_shadow() {
            r = r.max(d.distance + d.size * 1.6);
        }
        if let Some(d) = self.inner_shadow() {
            r = r.max(d.distance + d.size * 1.6);
        }
        if let Some(g) = self.outer_glow() {
            r = r.max(g.size * 1.6);
        }
        if let Some(k) = self.stroke() {
            r = r.max(k.size + 1.0);
        }
        (r * s).ceil() as u32 + 2
    }

    /// A hash of the settings, for caches.
    pub fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{self:?}").hash(&mut h);
        h.finish()
    }
}

/// Shadow offset (dx, dy) in px: opposite the light, y down.
pub fn shadow_offset(angle: f32, distance: f32) -> (f32, f32) {
    let a = angle.to_radians();
    (-a.cos() * distance, a.sin() * distance)
}

/// The shape grown by `grow` px then blurred by `blur` px (Gaussian, σ = blur / 2).
fn grow_blur(w: usize, h: usize, a: &[f32], grow: f32, blur: f32) -> Vec<f32> {
    let mut m: Vec<f32> = if grow > 0.0 {
        let d = distance(w as u32, h as u32, |i| a[i] >= 0.5);
        d.par_iter().zip(a).map(|(&dd, &v)| v.max((grow + 0.5 - dd).clamp(0.0, 1.0))).collect()
    } else {
        a.to_vec()
    };
    if blur > 0.0 {
        gaussian(w, h, &mut m, blur / 2.0);
    }
    m
}

/// Bilinear sample with transparency outside.
fn sample(w: usize, h: usize, a: &[f32], x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xi: i64, yi: i64| if xi < 0 || yi < 0 || xi >= w as i64 || yi >= h as i64 { 0.0 } else { a[yi as usize * w + xi as usize] };
    let (xi, yi) = (x0 as i64, y0 as i64);
    let top = at(xi, yi) * (1.0 - fx) + at(xi + 1, yi) * fx;
    let bot = at(xi, yi + 1) * (1.0 - fx) + at(xi + 1, yi + 1) * fx;
    top * (1.0 - fy) + bot * fy
}

/// Effect coverage for one tile (each 256×256, 0–1), or None where an effect is off.
#[derive(Default)]
pub struct Mattes {
    pub drop_shadow: Option<Vec<f32>>,
    pub outer_glow: Option<Vec<f32>>,
    pub inner_shadow: Option<Vec<f32>>,
    pub stroke: Option<Vec<f32>>,
}

/// Computes the mattes of a tile. `alpha` is the shape over a `w`×`h` region whose pixel
/// (`ox`, `oy`) is the tile's top-left; `s` scales the style's px to this image level.
pub fn mattes(style: &LayerStyle, w: usize, h: usize, alpha: &[f32], ox: usize, oy: usize, s: f32) -> Mattes {
    let crop = |m: &[f32]| -> Vec<f32> {
        let mut out = vec![0.0; TILE * TILE];
        for y in 0..TILE {
            let sy = oy + y;
            if sy >= h {
                break;
            }
            for x in 0..TILE {
                let sx = ox + x;
                if sx < w {
                    out[y * TILE + x] = m[sy * w + sx];
                }
            }
        }
        out
    };
    let shifted = |m: &[f32], dx: f32, dy: f32| -> Vec<f32> {
        let mut out = vec![0.0; TILE * TILE];
        out.par_chunks_mut(TILE).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                *o = sample(w, h, m, (ox + x) as f32 - dx, (oy + y) as f32 - dy);
            }
        });
        out
    };
    let mut out = Mattes::default();
    if let Some(d) = style.drop_shadow() {
        let size = d.size * s;
        let m = grow_blur(w, h, alpha, size * d.spread / 100.0, size * (1.0 - d.spread / 100.0));
        let (dx, dy) = shadow_offset(d.angle, d.distance * s);
        let mut sh = shifted(&m, dx, dy);
        if d.knockout {
            let a = crop(alpha);
            for (v, k) in sh.iter_mut().zip(&a) {
                *v *= 1.0 - k;
            }
        }
        out.drop_shadow = Some(sh);
    }
    if let Some(g) = style.outer_glow() {
        let size = g.size * s;
        let m = match g.technique {
            GlowTechnique::Softer => grow_blur(w, h, alpha, size * g.spread / 100.0, size * (1.0 - g.spread / 100.0)),
            GlowTechnique::Precise => {
                // a linear falloff over the distance from the shape
                let d = distance(w as u32, h as u32, |i| alpha[i] >= 0.5);
                let solid = size * g.spread / 100.0;
                let fade = (size - solid).max(1e-3);
                d.par_iter().zip(alpha).map(|(&dd, &a)| a.max((1.0 - (dd - solid) / fade).clamp(0.0, 1.0))).collect()
            }
        };
        out.outer_glow = Some(crop(&m));
    }
    if let Some(d) = style.inner_shadow() {
        // the outside of the shape, choked and blurred, offset, kept inside the shape
        let size = d.size * s;
        let inv: Vec<f32> = alpha.iter().map(|a| 1.0 - a).collect();
        let m = grow_blur(w, h, &inv, size * d.spread / 100.0, size * (1.0 - d.spread / 100.0));
        let (dx, dy) = shadow_offset(d.angle, d.distance * s);
        let mut sh = shifted(&m, dx, dy);
        // beyond the region edge the shape continues, so outside-of-region counts as outside
        let a = crop(alpha);
        for (v, k) in sh.iter_mut().zip(&a) {
            *v *= k;
        }
        out.inner_shadow = Some(sh);
    }
    if let Some(k) = style.stroke() {
        let size = k.size * s;
        // signed distance to the edge: positive outside, negative inside
        let d_out = distance(w as u32, h as u32, |i| alpha[i] >= 0.5);
        let d_in = distance(w as u32, h as u32, |i| alpha[i] < 0.5);
        let band = |sd: f32, lo: f32, hi: f32| ((sd - lo + 0.5).clamp(0.0, 1.0)).min((hi - sd + 0.5).clamp(0.0, 1.0));
        let m: Vec<f32> = d_out
            .par_iter()
            .zip(&d_in)
            .map(|(&o, &i)| {
                // edge at 0: outside pixels are 0.5 past it, inside pixels 0.5 before
                let sd = if o > 0.0 { o - 0.5 } else { 0.5 - i };
                match k.position {
                    StrokePosition::Outside => band(sd, 0.0, size),
                    StrokePosition::Inside => band(sd, -size, 0.0),
                    StrokePosition::Center => band(sd, -size / 2.0, size / 2.0),
                }
            })
            .collect();
        out.stroke = Some(crop(&m));
    }
    out
}

/// Position 0–1 of a point in a gradient overlay over the layer's bounds (x0, y0, x1, y1).
pub fn overlay_t(g: &GradientOverlay, x: f32, y: f32, b: (f32, f32, f32, f32)) -> f32 {
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let (hw, hh) = (((b.2 - b.0) / 2.0).max(1.0), ((b.3 - b.1) / 2.0).max(1.0));
    let a = g.angle.to_radians();
    let (ux, uy) = (a.cos(), -a.sin());
    // half the extent of the bounds along the direction, times the scale
    let half = (hw * ux.abs() + hh * uy.abs()) * (g.scale / 100.0).max(0.01);
    let (px, py) = (x - cx, y - cy);
    let t = match g.style {
        OverlayStyle::Linear => (px * ux + py * uy) / (2.0 * half) + 0.5,
        OverlayStyle::Reflected => ((px * ux + py * uy) / half).abs(),
        OverlayStyle::Radial => (px * px + py * py).sqrt() / (hw.max(hh) * (g.scale / 100.0).max(0.01)),
    };
    let t = t.clamp(0.0, 1.0);
    if g.reverse {
        1.0 - t
    } else {
        t
    }
}

/// The color of a gradient (straight sRGB 0–1) at t.
pub fn gradient_color(stops: &[GradientStop], t: f32) -> [f32; 3] {
    let mut s: Vec<&GradientStop> = stops.iter().collect();
    s.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    let col = |g: &GradientStop| g.color.map(|v| v as f32 / 255.0);
    match s.as_slice() {
        [] => [0.0; 3],
        [one] => col(one),
        _ => {
            if t <= s[0].pos {
                return col(s[0]);
            }
            if t >= s[s.len() - 1].pos {
                return col(s[s.len() - 1]);
            }
            let k = s.partition_point(|g| g.pos <= t) - 1;
            let (a, b) = (s[k], s[k + 1]);
            let f = if b.pos > a.pos { (t - a.pos) / (b.pos - a.pos) } else { 0.0 };
            let (ca, cb) = (col(a), col(b));
            [ca[0] + (cb[0] - ca[0]) * f, ca[1] + (cb[1] - ca[1]) * f, ca[2] + (cb[2] - ca[2]) * f]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100×100 region with a filled square 30..70.
    fn square() -> Vec<f32> {
        (0..100 * 100).map(|i| if (30..70).contains(&(i % 100)) && (30..70).contains(&(i / 100)) { 1.0 } else { 0.0 }).collect()
    }

    fn shadow(distance: f32, size: f32, spread: f32) -> Shadow {
        Shadow { enabled: true, blend: BlendMode::Multiply, color: [0, 0, 0], opacity: 0.75, angle: 135.0, distance, spread, size, knockout: false }
    }

    // the region is smaller than a tile here: mattes read from (0, 0) and pad with zeros
    fn at(m: &[f32], x: usize, y: usize) -> f32 {
        m[y * TILE + x]
    }

    #[test]
    fn drop_shadow_falls_opposite_the_light() {
        let st = LayerStyle { drop_shadow: Some(shadow(10.0, 0.0, 0.0)), ..Default::default() };
        let m = mattes(&st, 100, 100, &square(), 0, 0, 1.0);
        let s = m.drop_shadow.unwrap();
        // light from the upper left (135°): the shadow moves right and down by 10·cos45
        let d = 10.0 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((shadow_offset(135.0, 10.0).0 - d).abs() < 1e-4 && (shadow_offset(135.0, 10.0).1 - d).abs() < 1e-4);
        assert!(at(&s, 75, 75) > 0.99, "{}", at(&s, 75, 75));
        assert!(at(&s, 32, 32) < 0.01);
    }

    #[test]
    fn size_blurs_and_spread_hardens() {
        let soft = mattes(&LayerStyle { drop_shadow: Some(shadow(0.0, 12.0, 0.0)), ..Default::default() }, 100, 100, &square(), 0, 0, 1.0).drop_shadow.unwrap();
        let hard = mattes(&LayerStyle { drop_shadow: Some(shadow(0.0, 12.0, 100.0)), ..Default::default() }, 100, 100, &square(), 0, 0, 1.0).drop_shadow.unwrap();
        // just outside the edge: the soft shadow fades, the full-spread one is solid (grown 12 px)
        assert!(at(&soft, 74, 50) > 0.05 && at(&soft, 74, 50) < 0.6, "{}", at(&soft, 74, 50));
        assert!(at(&hard, 78, 50) > 0.99 && at(&hard, 84, 50) < 0.01, "{} {}", at(&hard, 78, 50), at(&hard, 84, 50));
    }

    #[test]
    fn knockout_removes_the_shadow_under_the_layer() {
        let mut sh = shadow(0.0, 6.0, 50.0);
        sh.knockout = true;
        let m = mattes(&LayerStyle { drop_shadow: Some(sh), ..Default::default() }, 100, 100, &square(), 0, 0, 1.0).drop_shadow.unwrap();
        assert_eq!(at(&m, 50, 50), 0.0);
        assert!(at(&m, 72, 50) > 0.3);
    }

    #[test]
    fn strokes_sit_outside_inside_or_centered() {
        for (pos, inside_px, outside_px) in [(StrokePosition::Outside, 68, 73), (StrokePosition::Inside, 67, 71), (StrokePosition::Center, 69, 71)] {
            let st = LayerStyle { stroke: Some(Stroke { enabled: true, blend: BlendMode::Normal, color: [255, 0, 0], opacity: 1.0, size: 4.0, position: pos }), ..Default::default() };
            let m = mattes(&st, 100, 100, &square(), 0, 0, 1.0).stroke.unwrap();
            let profile: Vec<f32> = (60..80).map(|x| at(&m, x, 50)).collect();
            match pos {
                StrokePosition::Outside => {
                    assert!(at(&m, 71, 50) > 0.99 && at(&m, inside_px, 50) < 0.01 && at(&m, 76, 50) < 0.01, "{profile:?}");
                    assert!(at(&m, outside_px, 50) > 0.99);
                }
                StrokePosition::Inside => assert!(at(&m, inside_px, 50) > 0.99 && at(&m, outside_px, 50) < 0.01 && at(&m, 63, 50) < 0.01, "{profile:?}"),
                StrokePosition::Center => assert!(at(&m, inside_px, 50) > 0.99 && at(&m, outside_px, 50) > 0.99 && at(&m, 65, 50) < 0.01 && at(&m, 75, 50) < 0.01, "{profile:?}"),
            }
        }
    }

    #[test]
    fn glow_and_inner_shadow_stay_on_their_side() {
        let g = Glow { enabled: true, blend: BlendMode::Screen, color: [255, 255, 200], opacity: 0.8, spread: 0.0, size: 10.0, technique: GlowTechnique::Precise };
        let m = mattes(&LayerStyle { outer_glow: Some(g), ..Default::default() }, 100, 100, &square(), 0, 0, 1.0).outer_glow.unwrap();
        // 3 px from the edge with size 10: 1 − 3/10
        assert!((at(&m, 72, 50) - 0.7).abs() < 1e-4 && at(&m, 82, 50) < 0.05);
        let st = LayerStyle { inner_shadow: Some(shadow(6.0, 4.0, 0.0)), ..Default::default() };
        let m = mattes(&st, 100, 100, &square(), 0, 0, 1.0).inner_shadow.unwrap();
        // light from the upper left: the inner shadow sits along the upper-left inside edges
        assert!(at(&m, 32, 50) > 0.5, "{}", at(&m, 32, 50));
        assert!(at(&m, 67, 50) < 0.2 && at(&m, 20, 50) == 0.0);
    }

    #[test]
    fn gradient_overlay_positions() {
        let g = GradientOverlay {
            enabled: true,
            blend: BlendMode::Normal,
            opacity: 1.0,
            stops: vec![GradientStop { pos: 0.0, color: [0, 0, 0] }, GradientStop { pos: 1.0, color: [255, 255, 255] }],
            style: OverlayStyle::Linear,
            angle: 0.0,
            scale: 100.0,
            reverse: false,
        };
        let b = (0.0, 0.0, 100.0, 50.0);
        assert!(overlay_t(&g, 0.0, 25.0, b) < 0.01 && overlay_t(&g, 100.0, 25.0, b) > 0.99);
        assert!((overlay_t(&g, 50.0, 10.0, b) - 0.5).abs() < 1e-4);
        // 90°: from the bottom up
        let up = GradientOverlay { angle: 90.0, ..g.clone() };
        assert!(overlay_t(&up, 50.0, 50.0, b) < 0.01 && overlay_t(&up, 50.0, 0.0, b) > 0.99);
        assert_eq!(gradient_color(&g.stops, 0.5), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn styles_round_trip_and_report_their_reach() {
        let st = LayerStyle { drop_shadow: Some(shadow(10.0, 8.0, 0.0)), ..Default::default() };
        assert!(st.active());
        assert_eq!(st.reach(1.0), (10.0f32 + 8.0 * 1.6).ceil() as u32 + 2);
        let j = serde_json::to_string(&st).unwrap();
        assert_eq!(serde_json::from_str::<LayerStyle>(&j).unwrap(), st);
        let off = LayerStyle { drop_shadow: Some(Shadow { enabled: false, ..shadow(1.0, 1.0, 0.0) }), ..Default::default() };
        assert!(!off.active());
    }
}
