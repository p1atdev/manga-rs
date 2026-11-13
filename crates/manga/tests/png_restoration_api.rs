use image::{DynamicImage, ImageBuffer, Luma, LumaA, Rgb, Rgba};
use manga::direct::{DirectErrorKind, decode_png_for_restoration};
use std::io::Cursor;

fn png(image: &DynamicImage) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

#[test]
fn returns_decoded_pixels_without_changing_color_depth_or_alpha() {
    let images = [
        DynamicImage::ImageLuma8(ImageBuffer::from_fn(11, 7, |x, y| {
            Luma([(x * 3 + y) as u8])
        })),
        DynamicImage::ImageLuma16(ImageBuffer::from_fn(11, 7, |x, y| {
            Luma([(x * 313 + y * 127) as u16])
        })),
        DynamicImage::ImageLumaA8(ImageBuffer::from_fn(11, 7, |x, y| {
            LumaA([x as u8, (y * 17) as u8])
        })),
        DynamicImage::ImageLumaA16(ImageBuffer::from_fn(11, 7, |x, y| {
            LumaA([(x * 313) as u16, (y * 211) as u16])
        })),
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(11, 7, |x, y| {
            Rgb([x as u8, y as u8, (x ^ y) as u8])
        })),
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(11, 7, |x, y| {
            Rgb([(x * 313) as u16, (y * 211) as u16, ((x ^ y) * 73) as u16])
        })),
        DynamicImage::ImageRgba8(ImageBuffer::from_fn(11, 7, |x, y| {
            Rgba([x as u8, y as u8, (x ^ y) as u8, ((x + y) * 7) as u8])
        })),
        DynamicImage::ImageRgba16(ImageBuffer::from_fn(11, 7, |x, y| {
            Rgba([
                (x * 313) as u16,
                (y * 211) as u16,
                ((x ^ y) * 73) as u16,
                ((x + y) * 101) as u16,
            ])
        })),
    ];
    for original in images {
        let bytes = png(&original);
        let decoded = decode_png_for_restoration(&bytes).unwrap();
        assert_eq!(decoded.color(), original.color());
        assert_eq!(decoded.to_rgba16(), original.to_rgba16());
    }
}

#[test]
fn rejects_non_png_and_metadata_without_changing_the_input() {
    let jpeg = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    let error = decode_png_for_restoration(jpeg).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
    let mut bytes = png(&DynamicImage::new_rgb8(32, 32));
    let pos = bytes.len() - 12;
    bytes.splice(
        pos..pos,
        [
            0, 0, 0, 3, b't', b'E', b'X', b't', b'a', 0, b'b', 0xdc, 0x49, 0xa2, 0x3b,
        ],
    );
    let before = bytes.clone();
    let error = decode_png_for_restoration(&bytes).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
    assert_eq!(bytes, before);
}

#[test]
fn rejects_invalid_crc_and_truncated_data() {
    let original = png(&DynamicImage::new_rgba8(32, 32));
    let mut bad = original.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert_eq!(
        decode_png_for_restoration(&bad).unwrap_err().kind(),
        DirectErrorKind::Malformed
    );
    assert_eq!(
        decode_png_for_restoration(&original[..original.len() - 1])
            .unwrap_err()
            .kind(),
        DirectErrorKind::Malformed
    );
}

#[test]
fn decodes_valid_png_above_default_512_mib_limit() {
    let bytes = include_bytes!("assets/direct-synthetic-large-rgba8.png");
    let image = decode_png_for_restoration(bytes).unwrap();
    assert_eq!((image.width(), image.height()), (16_384, 8_193));
    assert_eq!(image.color(), image::ColorType::Rgba8);
}

fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out.extend_from_slice(&crc32fast::hash(&out[4..]).to_be_bytes());
    out
}

fn tiny_png(depth: u8, color: u8, palette: bool) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&1u32.to_be_bytes());
    header.extend_from_slice(&[depth, color, 0, 0, 0]);
    bytes.extend_from_slice(&chunk(b"IHDR", &header));
    if palette {
        bytes.extend_from_slice(&chunk(b"PLTE", &[0, 0, 0]));
    }
    // zlib stream for filter byte 0 followed by one zero sample/index byte.
    bytes.extend_from_slice(&chunk(b"IDAT", &[120, 156, 99, 96, 0, 0, 0, 2, 0, 1]));
    bytes.extend_from_slice(&chunk(b"IEND", &[]));
    bytes
}

#[test]
fn rejects_valid_low_depth_and_indexed_pngs_as_unsupported() {
    for image in [tiny_png(1, 0, false), tiny_png(8, 3, true)] {
        image::load_from_memory(&image).expect("fixture must be a valid PNG");
        let error = decode_png_for_restoration(&image).unwrap_err();
        assert_eq!(error.kind(), DirectErrorKind::Unsupported);
    }
}

#[test]
fn rejects_icc_and_exif_chunks_instead_of_losing_metadata() {
    let original = png(&DynamicImage::new_rgb8(32, 32));
    for (kind, payload) in [
        (&b"iCCP"[..], &b"profile\0\0\x78\x9c\x03\0\0\0\0\x01"[..]),
        (&b"eXIf"[..], &b"II\x2a\0\x08\0\0\0\0\0\0\0\0\0"[..]),
    ] {
        let mut bytes = original.clone();
        let position = bytes.len() - 12;
        bytes.splice(position..position, chunk(kind.try_into().unwrap(), payload));
        assert_eq!(
            decode_png_for_restoration(&bytes).unwrap_err().kind(),
            DirectErrorKind::Unsupported
        );
    }
}

#[test]
fn rejects_corrupt_compressed_data_even_with_a_valid_chunk_crc() {
    let mut bytes = png(&DynamicImage::new_rgb8(32, 32));
    let mut offset = 8usize;
    loop {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset + 8 + length;
        if &bytes[offset + 4..offset + 8] == b"IDAT" {
            bytes[offset + 8] ^= 0xff;
            let crc = crc32fast::hash(&bytes[offset + 4..end]);
            bytes[end..end + 4].copy_from_slice(&crc.to_be_bytes());
            break;
        }
        offset = end + 4;
    }
    assert_eq!(
        decode_png_for_restoration(&bytes).unwrap_err().kind(),
        DirectErrorKind::Malformed
    );
}
