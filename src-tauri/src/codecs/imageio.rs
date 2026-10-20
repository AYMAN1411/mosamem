//! HEIC through Apple ImageIO (the system codec), macOS only.

use std::ffi::c_void;

use objc2_core_foundation::{CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CFType, CGRect, CGPoint, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGImageByteOrderInfo, CGContext, CGImage, CGImageAlphaInfo,
    kCGColorSpaceSRGB,
};
use objc2_image_io::{kCGImageDestinationLossyCompressionQuality, CGImageDestination, CGImageSource};

use super::{CodecError, Decoded};

fn srgb_context(width: usize, height: usize, data: *mut c_void) -> Result<CFRetained<CGContext>, CodecError> {
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or_else(|| CodecError::Decode("no sRGB color space".into()))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    unsafe { CGBitmapContextCreate(data, width, height, 8, width * 4, Some(&space), info) }
        .ok_or_else(|| CodecError::Decode("could not create bitmap context".into()))
}

/// Decodes any ImageIO-readable bytes (used for HEIC) into sRGB RGBA8, straight alpha. Drawing into
/// an sRGB context performs the color conversion from the file's embedded profile.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let data = CFData::from_bytes(bytes);
    let source = unsafe { CGImageSource::with_data(&data, None) }.ok_or_else(|| CodecError::Decode("ImageIO could not open the file".into()))?;
    let image = unsafe { source.image_at_index(0, None) }.ok_or_else(|| CodecError::Decode("ImageIO found no image".into()))?;
    let (w, h) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    let mut rgba = vec![0u8; w * h * 4];
    let ctx = srgb_context(w, h, rgba.as_mut_ptr().cast())?;
    let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64));
    CGContext::draw_image(Some(&ctx), rect, Some(&image));
    drop(ctx);
    // un-premultiply
    for p in rgba.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok(Decoded { width: w as u32, height: h as u32, rgba, ppi: None })
}

pub fn encode_heic(width: u32, height: u32, rgba: &[u8], quality: u8) -> Result<Vec<u8>, CodecError> {
    let (w, h) = (width as usize, height as usize);
    let mut pm = rgba.to_vec();
    for p in pm.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    let ctx = srgb_context(w, h, pm.as_mut_ptr().cast()).map_err(|e| CodecError::Encode(e.to_string()))?;
    let image = CGBitmapContextCreateImage(Some(&ctx)).ok_or_else(|| CodecError::Encode("could not create image".into()))?;
    let out = CFMutableData::new(None, 0).ok_or_else(|| CodecError::Encode("allocation failed".into()))?;
    let ty = CFString::from_static_str("public.heic");
    let dest = unsafe { CGImageDestination::with_data(&out, &ty, 1, None) }
        .ok_or_else(|| CodecError::Encode("this Mac can't write HEIC".into()))?;
    let q = CFNumber::new_f64(quality as f64 / 100.0);
    let key: &CFString = unsafe { kCGImageDestinationLossyCompressionQuality };
    let qv: &CFType = &q;
    let props = CFDictionary::<CFString, CFType>::from_slices(&[key], &[qv]);
    unsafe {
        dest.add_image(&image, Some(props.as_ref()));
        if !dest.finalize() {
            return Err(CodecError::Encode("HEIC encoding failed".into()));
        }
    }
    drop(ctx);
    Ok(out.to_vec())
}
