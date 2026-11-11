use manga::direct::{
    DirectFormat, ProviderImage, WebpMode, WebpOptions,
    process_provider_image_with_fallback_checked,
};

#[test]
fn unscrambled_webp_is_preserved_even_when_fallback_requested() {
    let bytes = include_bytes!("assets/kadocomi-decrypted.webp");
    let result = process_provider_image_with_fallback_checked(
        bytes,
        ProviderImage::KadokomiRaw,
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    assert_eq!(result.format(), DirectFormat::WebP);
    assert_eq!(result.bytes(), bytes);
}

#[cfg(feature = "comici")]
#[test]
fn real_comici_unaligned_jpeg_restores_to_exact_lossless_webp() {
    use manga::solver::ImageSolver;
    let input = include_bytes!("../../cli/tests/assets/comici-youngchampion-public-20261111.jpg");
    let meta: serde_json::Value = serde_json::from_str(include_str!(
        "../../cli/tests/assets/comici-youngchampion-public-20261111.permutation.json"
    ))
    .unwrap();
    let scramble: manga::viewer::comici::data::Scramble =
        serde_json::from_value(serde_json::Value::String(meta["tile_sources"].to_string()))
            .unwrap();
    let expected = manga::viewer::comici::solver::Solver::new(scramble.clone())
        .solve_from_bytes(input.to_vec())
        .unwrap()
        .to_rgba8();
    let page = process_provider_image_with_fallback_checked(
        input,
        ProviderImage::Comici {
            scramble: &scramble,
        },
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    assert_eq!(page.format(), DirectFormat::WebP);
    assert_eq!(
        image::load_from_memory(page.bytes()).unwrap().to_rgba8(),
        expected
    );
}

#[cfg(feature = "piccoma")]
#[test]
fn piccoma_seed_fallback_matches_existing_solver() {
    let input = include_bytes!("assets/piccoma-public-20261111.jpg");
    let seed = "synthetic-test-seed";
    let expected = manga::viewer::piccoma::restore_image(
        image::load_from_memory(input).unwrap().into_rgba8(),
        seed,
    )
    .unwrap();
    let page = process_provider_image_with_fallback_checked(
        input,
        ProviderImage::PiccomaSeed { seed },
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    assert_eq!(page.format(), DirectFormat::WebP);
    assert_eq!(
        image::load_from_memory(page.bytes()).unwrap().to_rgba8(),
        expected
    );
}

#[test]
fn png_transparent_rgb_is_retained_losslessly() {
    use std::io::Cursor;
    let image = image::RgbaImage::from_fn(3, 3, |x, y| {
        image::Rgba([x as u8 * 51, y as u8 * 43, 123, 0])
    });
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let page = process_provider_image_with_fallback_checked(
        png.get_ref(),
        ProviderImage::KadokomiRaw,
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    assert_eq!(
        image::load_from_memory(page.bytes()).unwrap().to_rgba8(),
        image
    );
}

#[test]
fn png_16bit_is_rejected_instead_of_reduced_to_eight_bits() {
    use std::io::Cursor;
    let image = image::ImageBuffer::from_pixel(1, 1, image::Rgba([12345u16, 2000, 3000, 65535]));
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba16(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let result = process_provider_image_with_fallback_checked(
        png.get_ref(),
        ProviderImage::KadokomiRaw,
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    );
    assert_eq!(
        result.unwrap_err().kind(),
        manga::direct::DirectErrorKind::Unsupported
    );
}

#[test]
fn corrupted_jpeg_is_not_reinterpreted_as_fallback() {
    let input = include_bytes!("../../cli/tests/assets/comici-youngchampion-public-20261111.jpg");
    let truncated = &input[..input.len() - 5];
    let result = process_provider_image_with_fallback_checked(
        truncated,
        ProviderImage::Giga,
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    );
    assert_eq!(
        result.unwrap_err().kind(),
        manga::direct::DirectErrorKind::Malformed
    );
}

#[test]
fn jpg_resolution_metadata_is_held_when_fallback_would_discard_it() {
    let input = include_bytes!("../../cli/tests/assets/comici-youngchampion-public-20261111.jpg");
    let mut bytes = input.to_vec();
    let pos = bytes.windows(5).position(|s| s == b"JFIF\0").unwrap();
    bytes[pos + 7] = 1;
    let result = process_provider_image_with_fallback_checked(
        &bytes,
        ProviderImage::PiccomaSeed { seed: "example" },
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    );
    assert_eq!(
        result.unwrap_err().kind(),
        manga::direct::DirectErrorKind::Unsupported
    );
}

fn jpeg_with_icc_segments(input: &[u8], profile: &[u8], duplicate: bool) -> Vec<u8> {
    assert!(input.starts_with(&[0xff, 0xd8]));
    let halves = [&profile[..profile.len() / 2], &profile[profile.len() / 2..]];
    let mut output = input[..2].to_vec();
    for (i, part) in halves.iter().enumerate().rev() {
        let len = (2 + 14 + part.len()) as u16;
        output.extend_from_slice(&[0xff, 0xe2]);
        output.extend_from_slice(&len.to_be_bytes());
        output.extend_from_slice(b"ICC_PROFILE\0");
        output.extend_from_slice(&[if duplicate { 1 } else { (i + 1) as u8 }, 2]);
        output.extend_from_slice(part);
    }
    output.extend_from_slice(&input[2..]);
    output
}

#[test]
fn rgb_icc_profile_is_reassembled_and_preserved_in_webp() {
    use libwebp_sys::*;
    let jpeg = include_bytes!("assets/piccoma-public-20261111.jpg");
    let icc = include_bytes!("assets/generated-srgb.icc");
    let input = jpeg_with_icc_segments(jpeg, icc, false);
    let page = process_provider_image_with_fallback_checked(
        &input,
        ProviderImage::PiccomaSeed { seed: "example" },
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    unsafe {
        let data = WebPData {
            bytes: page.bytes().as_ptr(),
            size: page.bytes().len(),
        };
        let mux = WebPMuxCreateInternal(&data, 0, WebPGetMuxABIVersion());
        assert!(!mux.is_null());
        let mut profile = WebPData::default();
        assert_eq!(
            WebPMuxGetChunk(mux, c"ICCP".as_ptr(), &mut profile),
            WebPMuxError::WEBP_MUX_OK
        );
        assert_eq!(std::slice::from_raw_parts(profile.bytes, profile.size), icc);
        WebPMuxDelete(mux);
    }
}

#[test]
fn bad_or_non_rgb_icc_profiles_are_held() {
    let jpeg = include_bytes!("assets/piccoma-public-20261111.jpg");
    let mut icc = include_bytes!("assets/generated-srgb.icc").to_vec();
    for input in [jpeg_with_icc_segments(jpeg, &icc, true), {
        icc[16..20].copy_from_slice(b"CMYK");
        jpeg_with_icc_segments(jpeg, &icc, false)
    }] {
        let result = process_provider_image_with_fallback_checked(
            &input,
            ProviderImage::PiccomaSeed { seed: "example" },
            None,
            WebpOptions {
                mode: WebpMode::Lossless,
                quality: 75,
            },
        );
        assert_eq!(
            result.unwrap_err().kind(),
            manga::direct::DirectErrorKind::Unsupported
        );
    }
}

#[test]
fn scrambled_static_webp_can_be_restored_without_losing_pixels() {
    use std::io::Cursor;
    let src = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([x as u8, y as u8, (x + y) as u8, 255])
    });
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(src.clone())
        .write_to(&mut encoded, image::ImageFormat::WebP)
        .unwrap();
    let page = process_provider_image_with_fallback_checked(
        encoded.get_ref(),
        ProviderImage::Giga,
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    let result = image::load_from_memory(page.bytes()).unwrap().to_rgba8();
    for y in 0..64 {
        for x in 0..64 {
            let sx = (y / 16) * 16 + x % 16;
            let sy = (x / 16) * 16 + y % 16;
            assert_eq!(result.get_pixel(x, y), src.get_pixel(sx, sy));
        }
    }
}

#[cfg(feature = "yanmaga")]
#[test]
fn yanmaga_padded_tiles_match_existing_solver() {
    use std::io::Cursor;
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let data = format!("HHHHHHHHHHHHHHHH{alphabet}");
    let src = format!("=8-8-4-{data}");
    let dst = format!("=8-8+4-{data}");
    let image = image::RgbaImage::from_fn(392, 392, |x, y| {
        image::Rgba([x as u8, y as u8, (x + y) as u8, 255])
    });
    let expected = manga::viewer::yanmaga::restore_image(image.clone(), &src, &dst).unwrap();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let page = process_provider_image_with_fallback_checked(
        png.get_ref(),
        ProviderImage::Yanmaga {
            source: &src,
            destination: &dst,
        },
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    )
    .unwrap();
    assert_eq!(
        image::load_from_memory(page.bytes()).unwrap().to_rgba8(),
        expected
    );
    let invalid = process_provider_image_with_fallback_checked(
        png.get_ref(),
        ProviderImage::Yanmaga {
            source: &src,
            destination: "invalid",
        },
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    );
    assert_eq!(
        invalid.unwrap_err().kind(),
        manga::direct::DirectErrorKind::InvalidArgument
    );
}

#[test]
fn corrupt_png_with_metadata_is_reported_as_malformed() {
    use std::io::Cursor;
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        32,
        32,
        image::Rgba([1, 2, 3, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let mut bytes = png.into_inner();
    let idat = bytes.windows(4).position(|b| b == b"IDAT").unwrap();
    let len = u32::from_be_bytes(bytes[idat - 4..idat].try_into().unwrap()) as usize;
    bytes[idat + 4] = 0;
    let crc = crc32fast::hash(&bytes[idat..idat + 4 + len]);
    bytes[idat + 4 + len..idat + 8 + len].copy_from_slice(&crc.to_be_bytes());
    let text = b"Comment\0hello";
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(text.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(text);
    chunk.extend_from_slice(&crc32fast::hash(&chunk[4..]).to_be_bytes());
    bytes.splice(33..33, chunk);
    let result = process_provider_image_with_fallback_checked(
        &bytes,
        ProviderImage::Giga,
        None,
        WebpOptions {
            mode: WebpMode::Lossless,
            quality: 75,
        },
    );
    assert_eq!(
        result.unwrap_err().kind(),
        manga::direct::DirectErrorKind::Malformed
    );
}

#[test]
fn tiny_webp_that_needs_no_giga_tiles_keeps_original_bytes() {
    use std::io::Cursor;
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        16,
        24,
        image::Rgba([1, 2, 3, 4]),
    ))
    .write_to(&mut bytes, image::ImageFormat::WebP)
    .unwrap();
    let page = process_provider_image_with_fallback_checked(
        bytes.get_ref(),
        ProviderImage::Giga,
        None,
        WebpOptions {
            mode: WebpMode::Lossy,
            quality: 10,
        },
    )
    .unwrap();
    assert_eq!(page.bytes(), bytes.get_ref());
}

#[cfg(feature = "comici")]
#[test]
fn identity_comici_preserves_webp_bytes() {
    use std::io::Cursor;
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        64,
        64,
        image::Rgba([1, 2, 3, 4]),
    ))
    .write_to(&mut bytes, image::ImageFormat::WebP)
    .unwrap();
    let scramble =
        manga::viewer::comici::data::Scramble::try_from(std::array::from_fn(|i| i)).unwrap();
    let page = process_provider_image_with_fallback_checked(
        bytes.get_ref(),
        ProviderImage::Comici {
            scramble: &scramble,
        },
        None,
        WebpOptions {
            mode: WebpMode::Lossy,
            quality: 10,
        },
    )
    .unwrap();
    assert_eq!(page.bytes(), bytes.get_ref());
}
