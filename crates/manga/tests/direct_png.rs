use image::{DynamicImage, ImageBuffer, Luma, LumaA, Rgb, Rgba};
use manga::direct::{DirectErrorKind, DirectFormat, DirectTransform, process_direct};
use std::io::Cursor;

fn png(image: &DynamicImage) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

fn samples() -> Vec<DynamicImage> {
    let (w, h) = (70, 70);
    vec![
        DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| Luma([(x + y * 3) as u8]))),
        DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| {
            Luma([(x * 257 + y * 17) as u16])
        })),
        DynamicImage::ImageLumaA8(ImageBuffer::from_fn(w, h, |x, y| {
            LumaA([x as u8, (y * 3) as u8])
        })),
        DynamicImage::ImageLumaA16(ImageBuffer::from_fn(w, h, |x, y| {
            LumaA([(x * 257) as u16, (y * 313) as u16])
        })),
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([x as u8, y as u8, (x ^ y) as u8])
        })),
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(w, h, |x, y| {
            Rgb([(x * 257) as u16, (y * 313) as u16, ((x ^ y) * 103) as u16])
        })),
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([x as u8, y as u8, (x ^ y) as u8, ((x + y) * 2) as u8])
        })),
        DynamicImage::ImageRgba16(ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([
                (x * 257) as u16,
                (y * 313) as u16,
                ((x ^ y) * 103) as u16,
                ((x + y) * 313) as u16,
            ])
        })),
    ]
}

fn expected(image: &DynamicImage, permutation: &[usize]) -> ImageBuffer<Rgba<u16>, Vec<u16>> {
    let source = image.to_rgba16();
    let mut out = source.clone();
    for (destination, &input) in permutation.iter().enumerate() {
        for y in 0..16 {
            for x in 0..16 {
                out.put_pixel(
                    (destination as u32 % 4) * 16 + x,
                    (destination as u32 / 4) * 16 + y,
                    *source.get_pixel((input as u32 % 4) * 16 + x, (input as u32 / 4) * 16 + y),
                );
            }
        }
    }
    out
}

#[test]
fn restores_png_pixels_depth_alpha_and_remainders() {
    let permutation = [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0];
    for original in samples() {
        let output = process_direct(
            &png(&original),
            DirectTransform::Tiles4x4 {
                tile_width: 16,
                tile_height: 16,
                source_by_destination: &permutation,
            },
        )
        .unwrap();
        assert_eq!(output.format(), DirectFormat::Png);
        assert_eq!(output.mime_type(), "image/png");
        let restored = image::load_from_memory(output.bytes()).unwrap();
        assert_eq!(restored.color(), original.color());
        assert_eq!(restored.to_rgba16(), expected(&original, &permutation));
    }
}

#[test]
fn giga_restores_png_with_its_transpose() {
    let original = &samples()[7];
    let permutation = [0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15];
    let output = process_direct(&png(original), DirectTransform::Giga).unwrap();
    let decoded = image::load_from_memory(output.bytes()).unwrap();
    assert_eq!(decoded.color(), original.color());
    assert_eq!(decoded.to_rgba16(), expected(original, &permutation));
}

#[test]
fn png_with_unpreserved_metadata_is_held() {
    let mut bytes = png(&samples()[0]);
    let position = bytes.len() - 12;
    // Valid tEXt chunk and CRC, inserted before IEND.
    let chunk = [
        0, 0, 0, 3, b't', b'E', b'X', b't', b'a', 0, b'b', 0xdc, 0x49, 0xa2, 0x3b,
    ];
    bytes.splice(position..position, chunk);
    assert_eq!(
        process_direct(&bytes, DirectTransform::Preserve)
            .unwrap()
            .bytes(),
        bytes
    );
    let error = process_direct(&bytes, DirectTransform::Giga).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
    assert!(error.to_string().contains("metadata"));
}

#[test]
fn png_rejects_bad_tile_metadata_and_does_not_require_eight_pixel_alignment() {
    let original = &samples()[7];
    let input = png(original);
    for permutation in [&[0usize; 16][..], &[0usize; 15][..]] {
        let error = process_direct(
            &input,
            DirectTransform::Tiles4x4 {
                tile_width: 16,
                tile_height: 16,
                source_by_destination: permutation,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind(), DirectErrorKind::InvalidArgument);
    }
    let identity: Vec<_> = (0..16).collect();
    let error = process_direct(
        &input,
        DirectTransform::Tiles4x4 {
            tile_width: u32::MAX,
            tile_height: 16,
            source_by_destination: &identity,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::InvalidArgument);
    let restored = process_direct(
        &input,
        DirectTransform::Tiles4x4 {
            tile_width: 15,
            tile_height: 15,
            source_by_destination: &identity,
        },
    )
    .unwrap();
    assert_eq!(
        image::load_from_memory(restored.bytes())
            .unwrap()
            .to_rgba16(),
        original.to_rgba16()
    );
}

#[test]
fn scrambled_webp_is_held_without_conversion() {
    let mut output = Cursor::new(Vec::new());
    samples()[0]
        .write_to(&mut output, image::ImageFormat::WebP)
        .unwrap();
    let error = process_direct(output.get_ref(), DirectTransform::Giga).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
    assert!(error.to_string().contains("WebP"));
}

#[test]
fn malformed_png_does_not_panic_or_return_an_image() {
    let valid = png(&samples()[0]);
    for n in [8, 15, 24, valid.len() - 1] {
        assert!(process_direct(&valid[..n], DirectTransform::Giga).is_err());
    }
}

#[test]
fn rejects_corrupt_iend_crc_in_preserve_and_restore_modes() {
    let mut bytes = png(&samples()[0]);
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    for mode in [DirectTransform::Preserve, DirectTransform::Giga] {
        let error = process_direct(&bytes, mode).unwrap_err();
        assert_eq!(error.kind(), DirectErrorKind::Malformed);
    }
}

#[test]
fn rejects_corrupt_ihdr_and_idat_crcs_in_both_modes() {
    let original = png(&samples()[0]);
    for kind in [b"IHDR", b"IDAT"] {
        let mut bytes = original.clone();
        let mut offset = 8usize;
        loop {
            let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            if &bytes[offset + 4..offset + 8] == kind {
                bytes[offset + 8 + len] ^= 1;
                break;
            }
            offset += len + 12;
        }
        for mode in [DirectTransform::Preserve, DirectTransform::Giga] {
            assert_eq!(
                process_direct(&bytes, mode).unwrap_err().kind(),
                DirectErrorKind::Malformed
            );
        }
    }
}

#[test]
fn large_valid_png_has_no_default_512_mib_decode_ceiling() {
    // Synthetic solid-color image: 16,384 × 8,193 × 4 bytes = 536,936,448 bytes.
    let input = include_bytes!("assets/direct-synthetic-large-rgba8.png");
    let preserved = process_direct(input, DirectTransform::Preserve).unwrap();
    assert_eq!(preserved.bytes(), input);
    let restored = process_direct(input, DirectTransform::Giga).unwrap();
    assert_eq!(restored.format(), DirectFormat::Png);
    let dimensions = image::ImageReader::new(Cursor::new(restored.bytes()))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap();
    assert_eq!(dimensions, (16_384, 8_193));
}
