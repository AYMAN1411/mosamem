//! The three networks' inputs and outputs: BiRefNet (subject matte), SAM 2.1 (object from clicks or
//! a box) and MI-GAN (fill). Images come in as straight RGBA8; masks go out as 0–1 per pixel at the
//! image's own size.

use std::path::Path;

use image::{imageops::FilterType, ImageBuffer, Luma, RgbImage};
use ort::value::Tensor;

use super::runtime::{AiResult, Runtime};

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// RGB over mid-gray (transparent pixels carry no color), resized to `size`² and normalized the
/// ImageNet way, as NCHW. Both BiRefNet and SAM 2 take their input like this.
pub fn normalized(rgba: &[u8], w: u32, h: u32, size: u32) -> Vec<f32> {
    let rgb = RgbImage::from_fn(w, h, |x, y| {
        let p = &rgba[((y * w + x) * 4) as usize..][..4];
        let a = p[3] as u32;
        image::Rgb([0, 1, 2].map(|c| ((p[c] as u32 * a + 128 * (255 - a) + 127) / 255) as u8))
    });
    let r = image::imageops::resize(&rgb, size, size, FilterType::Triangle);
    let n = (size * size) as usize;
    let mut v = vec![0f32; 3 * n];
    for (i, p) in r.pixels().enumerate() {
        for c in 0..3 {
            v[c * n + i] = (p[c] as f32 / 255.0 - MEAN[c]) / STD[c];
        }
    }
    v
}

fn sigmoid(v: f32) -> f32 {
    1.0 / (1.0 + (-v).exp())
}

/// A square map of 0–1 values resized to w×h (bilinear; the resize clamps to 0–1).
fn upsample(v: Vec<f32>, side: u32, w: u32, h: u32) -> Vec<f32> {
    let img: ImageBuffer<Luma<f32>, Vec<f32>> = ImageBuffer::from_raw(side, side, v).expect("square map");
    image::imageops::resize(&img, w, h, FilterType::Triangle).into_raw()
}

// ---- BiRefNet ----

pub const SUBJECT_SIZE: u32 = 1024;

/// The subject's coverage, 0–1 per pixel.
pub fn subject(rt: &Runtime, model: &Path, rgba: &[u8], w: u32, h: u32) -> AiResult<Vec<f32>> {
    let s = SUBJECT_SIZE;
    let input = normalized(rgba, w, h, s);
    let logits = rt.with(model, |sess| {
        let x = Tensor::from_array(([1usize, 3, s as usize, s as usize], input.clone())).map_err(err)?;
        let out = sess.run(ort::inputs![x]).map_err(err)?;
        let (_, d) = out[0].try_extract_tensor::<f32>().map_err(err)?;
        Ok(d.to_vec())
    })?;
    Ok(upsample(logits.into_iter().map(sigmoid).collect(), s, w, h))
}

// ---- SAM 2.1 ----

pub const SAM_SIZE: u32 = 1024;

/// The image encoder's output for one picture; clicks then only run the small decoder.
pub struct Embedding {
    pub w: u32,
    pub h: u32,
    parts: Vec<(Vec<i64>, Vec<f32>)>,
}

pub fn sam_encode(rt: &Runtime, encoder: &Path, rgba: &[u8], w: u32, h: u32) -> AiResult<Embedding> {
    let s = SAM_SIZE as usize;
    let input = normalized(rgba, w, h, SAM_SIZE);
    let parts = rt.with(encoder, |sess| {
        let x = Tensor::from_array(([1usize, 3, s, s], input.clone())).map_err(err)?;
        let out = sess.run(ort::inputs![x]).map_err(err)?;
        let mut parts = vec![];
        for k in ["image_embeddings.0", "image_embeddings.1", "image_embeddings.2"] {
            let (shape, d) = out[k].try_extract_tensor::<f32>().map_err(err)?;
            parts.push((shape.to_vec(), d.to_vec()));
        }
        Ok(parts)
    })?;
    Ok(Embedding { w, h, parts })
}

/// A click: document px, and whether it adds (true) or removes (false).
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub struct Click {
    pub x: f64,
    pub y: f64,
    pub add: bool,
}

/// The object at the clicks and/or inside the box (document px), 0–1 per pixel.
pub fn sam_decode(rt: &Runtime, decoder: &Path, e: &Embedding, clicks: &[Click], bbox: Option<[f64; 4]>) -> AiResult<Vec<f32>> {
    let (sx, sy) = (SAM_SIZE as f64 / e.w as f64, SAM_SIZE as f64 / e.h as f64);
    let mut pts: Vec<f32> = clicks.iter().flat_map(|c| [(c.x * sx) as f32, (c.y * sy) as f32]).collect();
    let mut labels: Vec<i64> = clicks.iter().map(|c| c.add as i64).collect();
    // a box goes in as its two corners, labelled 2 (top-left) and 3 (bottom-right): SAM's own
    // prompt convention
    if let Some(b) = bbox {
        pts.extend([(b[0] * sx) as f32, (b[1] * sy) as f32, (b[2] * sx) as f32, (b[3] * sy) as f32]);
        labels.extend([2, 3]);
    }
    if pts.is_empty() {
        return Ok(vec![0.0; (e.w * e.h) as usize]);
    }
    let n = labels.len();
    let (boxes, nb): (Vec<f32>, usize) = (vec![], 0);
    let (low, side) = rt.with(decoder, |sess| {
        let p = Tensor::from_array(([1usize, 1, n, 2], pts.clone())).map_err(err)?;
        let l = Tensor::from_array(([1usize, 1, n], labels.clone())).map_err(err)?;
        let b = Tensor::from_array(([1usize, nb, 4], boxes.clone())).map_err(err)?;
        let e0 = Tensor::from_array((e.parts[0].0.clone(), e.parts[0].1.clone())).map_err(err)?;
        let e1 = Tensor::from_array((e.parts[1].0.clone(), e.parts[1].1.clone())).map_err(err)?;
        let e2 = Tensor::from_array((e.parts[2].0.clone(), e.parts[2].1.clone())).map_err(err)?;
        let out = sess
            .run(ort::inputs!["input_points" => p, "input_labels" => l, "input_boxes" => b, "image_embeddings.0" => e0, "image_embeddings.1" => e1, "image_embeddings.2" => e2])
            .map_err(err)?;
        let (_, iou) = out["iou_scores"].try_extract_tensor::<f32>().map_err(err)?;
        let (shape, masks) = out["pred_masks"].try_extract_tensor::<f32>().map_err(err)?;
        // pred_masks: [1, 1, candidates, side, side]; keep the candidate SAM rates best
        let (cands, side) = (shape[2] as usize, shape[4] as usize);
        if std::env::var("MOSAMEM_SAM_DEBUG").is_ok() {
            let (lo, hi) = masks.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
            eprintln!("[sam] pred_masks {:?} range {lo}..{hi}, iou {:?}", shape, &iou[..cands.min(iou.len())]);
        }
        let best = (0..cands.min(iou.len())).max_by(|a, b| iou[*a].total_cmp(&iou[*b])).unwrap_or(0);
        Ok((masks[best * side * side..(best + 1) * side * side].to_vec(), side as u32))
    })?;
    // the low-res logits cover the whole (squashed) image. Squash to 0–1 first: the resize clamps
    // float pixels to 0–1, which would flatten the logits
    Ok(upsample(low.into_iter().map(sigmoid).collect(), side, e.w, e.h))
}

// ---- MI-GAN ----

/// `known` with every hole grown by `r` px (a square max filter, done separably).
fn grow_holes(known: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let mut rows = vec![true; w * h];
    for y in 0..h {
        let mut last_hole: Option<usize> = None;
        // distance to the nearest hole on the left, then on the right
        let mut left = vec![usize::MAX; w];
        for x in 0..w {
            if !known[y * w + x] {
                last_hole = Some(x);
            }
            if let Some(l) = last_hole {
                left[x] = x - l;
            }
        }
        last_hole = None;
        for x in (0..w).rev() {
            if !known[y * w + x] {
                last_hole = Some(x);
            }
            let right = last_hole.map_or(usize::MAX, |l| l - x);
            rows[y * w + x] = left[x].min(right) > r;
        }
    }
    let mut out = vec![true; w * h];
    for x in 0..w {
        let mut last_hole: Option<usize> = None;
        let mut up = vec![usize::MAX; h];
        for y in 0..h {
            if !rows[y * w + x] {
                last_hole = Some(y);
            }
            if let Some(l) = last_hole {
                up[y] = y - l;
            }
        }
        last_hole = None;
        for y in (0..h).rev() {
            if !rows[y * w + x] {
                last_hole = Some(y);
            }
            let down = last_hole.map_or(usize::MAX, |l| l - y);
            out[y * w + x] = up[y].min(down) > r;
        }
    }
    out
}

/// Fills the holes (`known[i] == false`) of an RGB image (HWC, u8). The pipeline model crops,
/// resizes to 512², fills and blends back by itself. Because it works on a resized mask, the
/// outermost hole pixels can come back as they went in; the holes are grown by about two model
/// pixels first, so callers can blend the result over their own (smaller) hole.
pub fn fill(rt: &Runtime, model: &Path, rgb: &[u8], known: &[bool], w: u32, h: u32) -> AiResult<Vec<u8>> {
    let n = (w * h) as usize;
    let r = (2.0 * w.max(h) as f64 / 512.0).ceil() as usize + 1;
    let known = grow_holes(known, w as usize, h as usize, r);
    let mut chw = vec![0u8; 3 * n];
    for i in 0..n {
        for c in 0..3 {
            chw[c * n + i] = rgb[i * 3 + c];
        }
    }
    let mask: Vec<u8> = known.iter().map(|k| if *k { 255 } else { 0 }).collect();
    let out = rt.with(model, |sess| {
        let img = Tensor::from_array(([1usize, 3, h as usize, w as usize], chw.clone())).map_err(err)?;
        let m = Tensor::from_array(([1usize, 1, h as usize, w as usize], mask.clone())).map_err(err)?;
        let out = sess.run(ort::inputs!["image" => img, "mask" => m]).map_err(err)?;
        let (_, d) = out["result"].try_extract_tensor::<u8>().map_err(err)?;
        Ok(d.to_vec())
    })?;
    if out.len() != 3 * n {
        return Err(format!("fill: unexpected output size {}", out.len()));
    }
    Ok((0..n).flat_map(|i| [out[i], out[n + i], out[2 * n + i]]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resize keeps 0–1 values (it clamps anything outside, which is why logits are squashed
    /// before it).
    #[test]
    fn upsample_keeps_a_sharp_matte() {
        let v: Vec<f32> = (0..16).map(|i| if i % 4 < 2 { 0.0 } else { 1.0 }).collect();
        let up = upsample(v, 4, 64, 64);
        assert!(up[0] < 0.01 && up[63] > 0.99, "{} {}", up[0], up[63]);
    }

    #[test]
    fn holes_grow_by_r_in_every_direction() {
        let (w, h) = (9, 7);
        let mut known = vec![true; w * h];
        known[3 * w + 4] = false;
        let g = grow_holes(&known, w, h, 2);
        let holes: Vec<(usize, usize)> = (0..w * h).filter(|i| !g[*i]).map(|i| (i % w, i / w)).collect();
        assert_eq!(holes.len(), 25, "a 5×5 square");
        assert!(holes.iter().all(|(x, y)| (2..=6).contains(x) && (1..=5).contains(y)));
    }
}
