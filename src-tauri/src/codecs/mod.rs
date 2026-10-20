//! Reading and writing flat image files. Every decoded image is converted to sRGB RGBA8 with
//! straight alpha; every encoded image is tagged sRGB.

#[cfg(target_os = "macos")]
mod imageio;

use std::io::{BufWriter, Cursor};
use std::path::Path;

use image::ImageDecoder;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("could not read image: {0}")]
    Decode(String),
    #[error("could not write image: {0}")]
    Encode(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Decoded {
    pub width: u32,
    pub height: u32,
    /// sRGB, straight alpha, row-major.
    pub rgba: Vec<u8>,
    pub ppi: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Tiff,
    Heic,
}

impl Format {
    pub fn from_path(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" => Format::Png,
            "jpg" | "jpeg" => Format::Jpeg,
            "webp" => Format::Webp,
            "tif" | "tiff" => Format::Tiff,
            "heic" | "heif" => Format::Heic,
            _ => return None,
        })
    }
}

pub fn decode(path: &Path) -> Result<Decoded, CodecError> {
    let bytes = std::fs::read(path)?;
    let format = Format::from_path(path).or_else(|| sniff(&bytes));
    if format == Some(Format::Heic) {
        #[cfg(target_os = "macos")]
        return imageio::decode(&bytes);
        #[cfg(not(target_os = "macos"))]
        return Err(CodecError::Unsupported("HEIC is only supported on macOS".into()));
    }
    decode_bytes(&bytes)
}

fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.len() > 12 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..12], b"heic" | b"heix" | b"mif1" | b"heim" | b"hevc") {
        return Some(Format::Heic);
    }
    None
}

pub fn decode_bytes(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| CodecError::Decode(e.to_string()))?;
    let mut decoder = reader.into_decoder().map_err(|e| CodecError::Decode(e.to_string()))?;
    let icc = decoder.icc_profile().ok().flatten();
    let img = image::DynamicImage::from_decoder(decoder).map_err(|e| CodecError::Decode(e.to_string()))?;
    let (width, height) = (img.width(), img.height());
    let mut rgba = img.into_rgba8().into_raw();
    if let Some(icc) = icc {
        to_srgb(&icc, &mut rgba, width as usize)?;
    }
    Ok(Decoded { width, height, rgba, ppi: None })
}

/// Converts RGBA8 pixels from an embedded ICC profile to sRGB, in place. Profiles that already
/// describe sRGB, or that can't be parsed, leave the pixels unchanged.
fn to_srgb(icc: &[u8], rgba: &mut [u8], width: usize) -> Result<(), CodecError> {
    use moxcms::{ColorProfile, Layout, TransformOptions};
    let Ok(src) = ColorProfile::new_from_slice(icc) else { return Ok(()) };
    let dst = ColorProfile::new_srgb();
    let Ok(transform) = src.create_transform_8bit(Layout::Rgba, &dst, Layout::Rgba, TransformOptions::default()) else {
        return Ok(());
    };
    let row = width * 4;
    let mut out = vec![0u8; rgba.len()];
    for (src_row, dst_row) in rgba.chunks_exact(row).zip(out.chunks_exact_mut(row)) {
        transform.transform(src_row, dst_row).map_err(|e| CodecError::Decode(format!("color conversion: {e:?}")))?;
    }
    rgba.copy_from_slice(&out);
    Ok(())
}

pub struct EncodeOptions {
    /// 0–100 for lossy formats.
    pub quality: u8,
    pub lossless: bool,
    pub ppi: f64,
}

pub fn encode(format: Format, width: u32, height: u32, rgba: &[u8], opts: &EncodeOptions) -> Result<Vec<u8>, CodecError> {
    match format {
        Format::Png => encode_png(width, height, rgba, opts),
        Format::Jpeg => encode_jpeg(width, height, rgba, opts),
        Format::Webp => {
            let enc = webp::Encoder::from_rgba(rgba, width, height);
            let mem = if opts.lossless {
                // libwebp's default lossless effort is very slow on large images; method 2 / effort 30
                // keeps files within a few percent of it at a fraction of the time.
                let mut cfg = webp::WebPConfig::new().map_err(|_| CodecError::Encode("webp config".into()))?;
                cfg.lossless = 1;
                cfg.quality = 30.0;
                cfg.method = 2;
                cfg.thread_level = 1;
                enc.encode_advanced(&cfg).map_err(|e| CodecError::Encode(format!("{e:?}")))?
            } else {
                enc.encode(opts.quality as f32)
            };
            Ok(mem.to_vec())
        }
        Format::Tiff => encode_tiff(width, height, rgba),
        Format::Heic => {
            #[cfg(target_os = "macos")]
            return imageio::encode_heic(width, height, rgba, opts.quality);
            #[cfg(not(target_os = "macos"))]
            return Err(CodecError::Unsupported("HEIC is only supported on macOS".into()));
        }
    }
}

fn encode_png(width: u32, height: u32, rgba: &[u8], opts: &EncodeOptions) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(BufWriter::new(&mut out), width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        enc.set_compression(png::Compression::Fast);
        let ppm = (opts.ppi / 0.0254).round() as u32;
        enc.set_pixel_dims(Some(png::PixelDimensions { xppu: ppm, yppu: ppm, unit: png::Unit::Meter }));
        let mut w = enc.write_header().map_err(|e| CodecError::Encode(e.to_string()))?;
        w.write_image_data(rgba).map_err(|e| CodecError::Encode(e.to_string()))?;
    }
    Ok(out)
}

fn encode_jpeg(width: u32, height: u32, rgba: &[u8], opts: &EncodeOptions) -> Result<Vec<u8>, CodecError> {
    // JPEG has no alpha: flatten onto white.
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for p in rgba.chunks_exact(4) {
        let a = p[3] as u32;
        for c in &p[..3] {
            rgb.push(((*c as u32 * a + 255 * (255 - a) + 127) / 255) as u8);
        }
    }
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, opts.quality.clamp(1, 100));
    let dpi = opts.ppi.round().clamp(1.0, 65535.0) as u16;
    enc.set_density(jpeg_encoder::PixelDensity::dpi(dpi));
    if opts.quality >= 90 {
        enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
    }
    enc.encode(&rgb, width as u16, height as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(out)
}

fn encode_tiff(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, CodecError> {
    use tiff::encoder::{colortype, Compression, DeflateLevel, TiffEncoder};
    let mut out = Cursor::new(Vec::new());
    let mut enc = TiffEncoder::new(&mut out)
        .map_err(|e| CodecError::Encode(e.to_string()))?
        .with_compression(Compression::Deflate(DeflateLevel::default()));
    enc.write_image::<colortype::RGBA8>(width, height, rgba).map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, 128, if x < w / 2 { 255 } else { 128 }]);
            }
        }
        v
    }

    fn opts() -> EncodeOptions {
        EncodeOptions { quality: 92, lossless: false, ppi: 300.0 }
    }

    #[test]
    fn lossless_formats_round_trip_exactly() {
        let src = sample(97, 61);
        for f in [Format::Png, Format::Tiff] {
            let bytes = encode(f, 97, 61, &src, &opts()).unwrap();
            let d = decode_bytes(&bytes).unwrap();
            assert_eq!((d.width, d.height), (97, 61), "{f:?}");
            assert_eq!(d.rgba, src, "{f:?}");
        }
        let lossless = EncodeOptions { lossless: true, ..opts() };
        let d = decode_bytes(&encode(Format::Webp, 97, 61, &src, &lossless).unwrap()).unwrap();
        assert_eq!(d.rgba, src, "webp lossless");
    }

    fn psnr(a: &[u8], b: &[u8]) -> f64 {
        let mse: f64 = a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum::<f64>() / a.len() as f64;
        10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10()
    }

    #[test]
    fn lossy_formats_keep_quality() {
        let mut src = sample(128, 96);
        for p in src.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let jpg = decode_bytes(&encode(Format::Jpeg, 128, 96, &src, &opts()).unwrap()).unwrap();
        assert!(psnr(&jpg.rgba, &src) > 38.0, "jpeg psnr {}", psnr(&jpg.rgba, &src));
        let webp = decode_bytes(&encode(Format::Webp, 128, 96, &src, &opts()).unwrap()).unwrap();
        assert!(psnr(&webp.rgba, &src) > 36.0, "webp psnr {}", psnr(&webp.rgba, &src));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn heic_round_trips_on_mac() {
        let mut src = sample(64, 64);
        for p in src.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let bytes = encode(Format::Heic, 64, 64, &src, &opts()).unwrap();
        assert!(sniff(&bytes) == Some(Format::Heic));
        let d = imageio::decode(&bytes).unwrap();
        assert_eq!((d.width, d.height), (64, 64));
        assert!(psnr(&d.rgba, &src) > 34.0, "heic psnr {}", psnr(&d.rgba, &src));
    }
}
