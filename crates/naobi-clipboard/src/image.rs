//! A clipboard bitmap (`CF_DIB`) to PNG, in safe Rust (D-015). The 64 MiB size check comes first, so a
//! huge bitmap never reaches the converter.
use crate::limits::{MAX_BITMAP_BYTES, MAX_PNG_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("the bitmap is over 64 MiB")]
    TooLargeBitmap,
    #[error("the PNG is over 10 MB")]
    TooLargePng,
    #[error("this bitmap format is not supported")]
    Unsupported,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Png {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Converts 24 or 32 bit uncompressed (or default-mask bit-fields) bitmaps. The alpha byte of a
/// 32 bit bitmap is dropped: clipboard producers leave it at zero.
pub fn dib_to_png(dib: &[u8]) -> Result<Png, ImageError> {
    if dib.len() > MAX_BITMAP_BYTES {
        return Err(ImageError::TooLargeBitmap);
    }
    let header = u32_at(dib, 0).ok_or(ImageError::Unsupported)? as usize;
    let width = u32_at(dib, 4).ok_or(ImageError::Unsupported)? as i32;
    let height = u32_at(dib, 8).ok_or(ImageError::Unsupported)? as i32;
    let bpp = u16::from_le_bytes(
        dib.get(14..16)
            .and_then(|b| b.try_into().ok())
            .ok_or(ImageError::Unsupported)?,
    ) as usize;
    let compression = u32_at(dib, 16).ok_or(ImageError::Unsupported)?;
    let supported = matches!(header, 40 | 108 | 124)
        && matches!(bpp, 24 | 32)
        && matches!(compression, 0 | 3)
        && width > 0
        && height != 0
        && width <= 32_768
        && height.unsigned_abs() <= 32_768;
    if !supported {
        return Err(ImageError::Unsupported);
    }
    // A 40-byte header with bit fields is followed by three mask DWORDs; V4 and V5 hold them inside.
    let offset = header
        + if header == 40 && compression == 3 {
            12
        } else {
            0
        };
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let stride = (w * bpp).div_ceil(32) * 4;
    let pixels = dib
        .get(offset..offset + stride * h)
        .ok_or(ImageError::Unsupported)?;
    let bytes_per_pixel = bpp / 8;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        // Positive heights are stored bottom-up.
        let row = if height > 0 { h - 1 - y } else { y };
        for px in pixels[row * stride..][..w * bytes_per_pixel].chunks_exact(bytes_per_pixel) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(|_| ImageError::Unsupported)?;
    writer
        .write_image_data(&rgb)
        .and_then(|()| writer.finish())
        .map_err(|_| ImageError::Unsupported)?;
    if out.len() > MAX_PNG_BYTES {
        return Err(ImageError::TooLargePng);
    }
    Ok(Png {
        bytes: out,
        width: w as u32,
        height: h as u32,
    })
}
