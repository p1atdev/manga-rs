//! Lossless PNG tile restoration for non-indexed 8/16-bit images without ancillary metadata.
use std::io::Cursor;

use image::{DynamicImage, ImageBuffer, Pixel};

use super::{DirectError, DirectErrorKind, DirectTransform, decode_png_for_restoration};

fn malformed(message: &str) -> DirectError {
    DirectError::new(DirectErrorKind::Malformed, message)
}
fn unsupported(message: &str) -> DirectError {
    DirectError::new(DirectErrorKind::Unsupported, message)
}

// PNG ancillary chunks can describe a color profile, orientation, animation, physical dimensions,
// text, or other information. Until these can be retained accurately, never silently discard them.
pub(super) fn validate_chunks(input: &[u8], restoring: bool) -> Result<(), DirectError> {
    if !input.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(malformed("invalid PNG signature"));
    }
    let mut offset = 8usize;
    let mut first = true;
    let mut ended = false;
    while offset < input.len() {
        let header_end = offset
            .checked_add(8)
            .filter(|&n| n <= input.len())
            .ok_or_else(|| malformed("truncated PNG chunk header"))?;
        let length = u32::from_be_bytes(input[offset..offset + 4].try_into().unwrap()) as usize;
        let end = header_end
            .checked_add(length)
            .and_then(|n| n.checked_add(4))
            .filter(|&n| n <= input.len())
            .ok_or_else(|| malformed("truncated PNG chunk"))?;
        let kind = &input[offset + 4..header_end];
        let expected_crc = u32::from_be_bytes(input[end - 4..end].try_into().unwrap());
        if crc32fast::hash(&input[offset + 4..end - 4]) != expected_crc {
            return Err(malformed("PNG chunk CRC mismatch"));
        }
        if first {
            if kind != b"IHDR" || length != 13 {
                return Err(malformed("PNG must begin with a valid IHDR"));
            }
            let depth = input[header_end + 8];
            let color = input[header_end + 9];
            if restoring && (!matches!(depth, 8 | 16) || !matches!(color, 0 | 2 | 4 | 6)) {
                return Err(unsupported(
                    "PNG color type or bit depth is not supported without changing it",
                ));
            }
            first = false;
        } else if kind == b"IEND" {
            if length != 0 || end != input.len() {
                return Err(malformed("invalid PNG end marker or trailing data"));
            }
            ended = true;
            break;
        } else if kind == b"IHDR" {
            return Err(malformed("duplicate PNG header"));
        } else if restoring && kind != b"IDAT" {
            return Err(unsupported(
                "PNG metadata or unsupported chunk cannot be preserved during tile restoration",
            ));
        }
        offset = end;
    }
    if !ended {
        return Err(malformed("PNG end marker is missing"));
    }
    Ok(())
}

fn reordered<P>(
    source: &ImageBuffer<P, Vec<P::Subpixel>>,
    width: u32,
    height: u32,
    permutation: &[usize; 16],
) -> ImageBuffer<P, Vec<P::Subpixel>>
where
    P: Pixel + 'static,
    P::Subpixel: 'static,
{
    let mut output = source.clone();
    for (destination, &source_tile) in permutation.iter().enumerate() {
        let dx = (destination as u32 % 4) * width;
        let dy = (destination as u32 / 4) * height;
        let sx = (source_tile as u32 % 4) * width;
        let sy = (source_tile as u32 / 4) * height;
        for y in 0..height {
            for x in 0..width {
                output.put_pixel(dx + x, dy + y, *source.get_pixel(sx + x, sy + y));
            }
        }
    }
    output
}

pub(super) fn restore(
    input: &[u8],
    transform: DirectTransform<'_>,
) -> Result<Vec<u8>, DirectError> {
    let source = decode_png_for_restoration(input)?;
    let (w, h) = (source.width(), source.height());
    let (tile_width, tile_height, permutation) = match transform {
        DirectTransform::Giga => {
            let width = (w / 32) * 8;
            let height = (h / 32) * 8;
            if width == 0 || height == 0 {
                return Ok(input.to_vec());
            }
            let mut p = [0; 16];
            for (d, value) in p.iter_mut().enumerate() {
                *value = (d % 4) * 4 + d / 4;
            }
            (width, height, p)
        }
        DirectTransform::Tiles4x4 {
            tile_width,
            tile_height,
            source_by_destination,
        } => {
            if tile_width == 0 || tile_height == 0 || source_by_destination.len() != 16 {
                return Err(DirectError::new(
                    DirectErrorKind::InvalidArgument,
                    "tile size must be nonzero and permutation must contain 16 entries",
                ));
            }
            if u64::from(tile_width) * 4 > u64::from(w) || u64::from(tile_height) * 4 > u64::from(h)
            {
                return Err(DirectError::new(
                    DirectErrorKind::InvalidArgument,
                    "tile grid exceeds image bounds",
                ));
            }
            let mut seen = [false; 16];
            let mut p = [0; 16];
            for (d, &value) in source_by_destination.iter().enumerate() {
                if value >= 16 || seen[value] {
                    return Err(DirectError::new(
                        DirectErrorKind::InvalidArgument,
                        "tile permutation must be a bijection",
                    ));
                }
                seen[value] = true;
                p[d] = value;
            }
            (tile_width, tile_height, p)
        }
        _ => unreachable!(),
    };
    let result = match source {
        DynamicImage::ImageLuma8(i) => {
            DynamicImage::ImageLuma8(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageLumaA8(i) => {
            DynamicImage::ImageLumaA8(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageRgb8(i) => {
            DynamicImage::ImageRgb8(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageRgba8(i) => {
            DynamicImage::ImageRgba8(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageLuma16(i) => {
            DynamicImage::ImageLuma16(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageLumaA16(i) => {
            DynamicImage::ImageLumaA16(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageRgb16(i) => {
            DynamicImage::ImageRgb16(reordered(&i, tile_width, tile_height, &permutation))
        }
        DynamicImage::ImageRgba16(i) => {
            DynamicImage::ImageRgba16(reordered(&i, tile_width, tile_height, &permutation))
        }
        _ => return Err(unsupported("decoded PNG color type cannot be preserved")),
    };
    let mut output = Cursor::new(Vec::new());
    result
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| {
            DirectError::new(
                DirectErrorKind::Internal,
                format!("PNG encoding failed: {error}"),
            )
        })?;
    Ok(output.into_inner())
}
