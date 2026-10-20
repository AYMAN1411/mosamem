//! What File › Export writes: the whole picture, one layer (trimmed to its pixels) or the selected
//! part (trimmed to the selection), optionally resized.

use mosamem_core::resample::{self, Method};
use serde::Deserialize;

use super::{composite, selection, DocHandle};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ExportSource {
    #[default]
    Document,
    Layer {
        id: u64,
    },
    Selection,
}

pub struct Pixels {
    pub w: u32,
    pub h: u32,
    /// Straight RGBA8.
    pub rgba: Vec<u8>,
    pub ppi: f64,
}

/// The smallest box holding every pixel where `keep(i)` is true; None if there's none.
fn bounds(w: u32, h: u32, keep: impl Fn(usize) -> bool) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if keep((y * w + x) as usize) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x1 > x0).then_some((x0, y0, x1, y1))
}

fn crop(w: u32, rgba: &[u8], (x0, y0, x1, y1): (u32, u32, u32, u32)) -> Vec<u8> {
    let mut out = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
    for y in y0..y1 {
        let at = ((y * w + x0) * 4) as usize;
        out.extend_from_slice(&rgba[at..at + ((x1 - x0) * 4) as usize]);
    }
    out
}

/// The source's pixels at their own size.
pub fn source(h: &DocHandle, src: ExportSource) -> Result<Pixels, String> {
    let ppi = h.doc.read().content.ppi;
    match src {
        ExportSource::Document => {
            let (w, hh, rgba) = composite::flatten(h);
            Ok(Pixels { w, h: hh, rgba, ppi })
        }
        ExportSource::Layer { id } => {
            let (w, hh, rgba) = {
                let d = h.doc.read();
                d.content.find(id).ok_or("layer not found")?;
                (d.content.width, d.content.height, selection::sample_from(&d, Some(id)))
            };
            let b = bounds(w, hh, |i| rgba[i * 4 + 3] > 0).ok_or("empty-layer")?;
            Ok(Pixels { w: b.2 - b.0, h: b.3 - b.1, rgba: crop(w, &rgba, b), ppi })
        }
        ExportSource::Selection => {
            let sel = {
                let d = h.doc.read();
                if d.content.selection.is_none() {
                    return Err("no-selection".into());
                }
                selection::current(&d)
            };
            let (w, hh, mut rgba) = composite::flatten(h);
            for (p, s) in rgba.chunks_exact_mut(4).zip(&sel.data) {
                p[3] = ((p[3] as u32 * *s as u32 + 127) / 255) as u8;
            }
            let b = bounds(w, hh, |i| sel.data[i] > 0).ok_or("no-selection")?;
            Ok(Pixels { w: b.2 - b.0, h: b.3 - b.1, rgba: crop(w, &rgba, b), ppi })
        }
    }
}

/// The source, resized to `size` if given (Lanczos; the resolution scales with it).
pub fn pixels(h: &DocHandle, src: ExportSource, size: Option<(u32, u32)>) -> Result<Pixels, String> {
    let p = source(h, src)?;
    match size {
        Some((nw, nh)) if nw > 0 && nh > 0 && (nw, nh) != (p.w, p.h) => {
            if nw > 30000 || nh > 30000 {
                return Err("too-large".into());
            }
            let rgba = resample::resize(p.w as usize, p.h as usize, &p.rgba, nw as usize, nh as usize, Method::Lanczos);
            Ok(Pixels { w: nw, h: nh, rgba, ppi: p.ppi * nw as f64 / p.w as f64 })
        }
        _ => Ok(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::model::Node;
    use crate::doc::{filled_tiles, Content, Document, Engine, Label};
    use mosamem_core::select::Combine;

    fn doc() -> (Engine, std::sync::Arc<DocHandle>, u64) {
        let bg = Node::pixel("bg", filled_tiles(400, 300, [255, 255, 255, 255]));
        let mut dot = Node::pixel("dot", crate::doc::raster::write(&[200, 10, 10, 255].repeat(50 * 20), 100, 60, 50, 20));
        dot.opacity = 1.0;
        let (bid, did) = (bg.id, dot.id);
        let doc = Document::new("t".into(), None, Content::new(400, 300, 144.0, bid, vec![bg, dot]), Label::new("history.new"));
        let e = Engine::default();
        let h = e.insert(doc);
        (e, h, did)
    }

    #[test]
    fn the_whole_picture() {
        let (_e, h, _) = doc();
        let p = pixels(&h, ExportSource::Document, None).unwrap();
        assert_eq!((p.w, p.h, p.ppi), (400, 300, 144.0));
        assert_eq!(&p.rgba[((70 * 400 + 110) * 4)..][..4], &[200, 10, 10, 255]);
    }

    #[test]
    fn a_layer_is_trimmed_to_its_pixels() {
        let (_e, h, dot) = doc();
        let p = pixels(&h, ExportSource::Layer { id: dot }, None).unwrap();
        assert_eq!((p.w, p.h), (50, 20));
        assert!(p.rgba.chunks_exact(4).all(|q| q == [200, 10, 10, 255]));
    }

    #[test]
    fn the_selection_is_cut_out() {
        let (_e, h, _) = doc();
        assert_eq!(pixels(&h, ExportSource::Selection, None).err().as_deref(), Some("no-selection"));
        selection::shape(&mut h.doc.write(), selection::Shape::Rect { x0: 90.0, y0: 50.0, x1: 130.0, y1: 90.0 }, Combine::Replace, 0.0).unwrap();
        let p = pixels(&h, ExportSource::Selection, None).unwrap();
        assert_eq!((p.w, p.h), (40, 40));
        // white background inside the selection, the red dot from x 100 / y 60
        assert_eq!(&p.rgba[..4], &[255, 255, 255, 255]);
        assert_eq!(&p.rgba[((15 * 40 + 15) * 4)..][..4], &[200, 10, 10, 255]);
    }

    #[test]
    fn resizing_scales_the_resolution() {
        let (_e, h, _) = doc();
        let p = pixels(&h, ExportSource::Document, Some((200, 150))).unwrap();
        assert_eq!((p.w, p.h, p.ppi), (200, 150, 72.0));
        assert_eq!(p.rgba.len(), 200 * 150 * 4);
        assert_eq!(pixels(&h, ExportSource::Document, Some((40000, 10))).err().as_deref(), Some("too-large"));
    }
}
