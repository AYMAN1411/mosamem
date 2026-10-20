//! Drawing laid-out text: glyph outlines (skrifa) filled with anti-aliasing (tiny-skia), through
//! the text layer's 2D transform.

use skrifa::instance::{Location, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use super::fonts::Face;
use super::layout::Layout;

fn location(font: &FontRef, face: &Face) -> Location {
    match face.variable_weight {
        Some(w) => font.axes().location([("wght", w)]),
        None => Location::default(),
    }
}

/// Ascent and descent (both positive, px) at a size.
pub fn vertical_metrics(face: &Face, size: f32) -> (f32, f32) {
    let Ok(font) = FontRef::new(face.bytes) else { return (size * 0.8, size * 0.2) };
    let loc = location(&font, face);
    let m = font.metrics(Size::new(size), &loc);
    (m.ascent, -m.descent)
}

/// Collects outlines into a path, flipping y (fonts point up) and moving to the glyph's place.
struct Pen<'a> {
    b: &'a mut PathBuilder,
    x: f32,
    y: f32,
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.b.move_to(self.x + x, self.y - y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.b.line_to(self.x + x, self.y - y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.b.quad_to(self.x + cx0, self.y - cy0, self.x + x, self.y - y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.b.cubic_to(self.x + cx0, self.y - cy0, self.x + cx1, self.y - cy1, self.x + x, self.y - y);
    }
    fn close(&mut self) {
        self.b.close();
    }
}

/// Rendered text: straight RGBA8 at (x, y) in document px.
pub struct Rendered {
    pub x: i64,
    pub y: i64,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Fills the layout's glyphs with `color`, mapped by the affine `m` = [a, b, c, d, e, f]
/// (x' = a·x + c·y + e, y' = b·x + d·y + f), clipped to `clip` (x0, y0, x1, y1).
pub fn render(layout: &Layout, color: [u8; 4], m: [f32; 6], clip: (i64, i64, i64, i64)) -> Option<Rendered> {
    // each glyph from its own font (the chosen one, or a fallback for missing characters)
    let fonts: Vec<(FontRef, Location)> = layout
        .faces
        .iter()
        .filter_map(|f| {
            let font = FontRef::new(f.bytes).ok()?;
            let loc = location(&font, f);
            Some((font, loc))
        })
        .collect();
    let outlines: Vec<_> = fonts.iter().map(|(f, _)| f.outline_glyphs()).collect();
    let mut b = PathBuilder::new();
    for g in &layout.glyphs {
        let k = (g.face as usize).min(fonts.len().saturating_sub(1));
        let Some(o) = outlines.get(k).and_then(|c| c.get(GlyphId::new(g.id))) else { continue };
        let mut pen = Pen { b: &mut b, x: g.x, y: g.y };
        let _ = o.draw(DrawSettings::unhinted(Size::new(layout.size), &fonts[k].1), &mut pen);
    }
    let path = b.finish()?;
    let t = Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]);
    let path = path.transform(t)?;
    let r = path.bounds();
    let x0 = (r.left().floor() as i64 - 1).max(clip.0);
    let y0 = (r.top().floor() as i64 - 1).max(clip.1);
    let x1 = (r.right().ceil() as i64 + 1).min(clip.2);
    let y1 = (r.bottom().ceil() as i64 + 1).min(clip.3);
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut pm = Pixmap::new(w, h)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint.anti_alias = true;
    pm.fill_path(&path, &paint, FillRule::Winding, Transform::from_translate(-x0 as f32, -y0 as f32), None);
    // tiny-skia keeps premultiplied pixels; layers are straight
    let mut rgba = pm.take();
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in 0..3 {
                p[c] = ((p[c] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(Rendered { x: x0, y: y0, w, h, rgba })
}
