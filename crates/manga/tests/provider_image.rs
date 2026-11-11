use manga::direct::{DirectErrorKind, DirectFormat, ProviderImage, process_provider_image};

const JPEG: &[u8] = include_bytes!("assets/magapoke-140481-page0-raw.jpg");
const WEBP: &[u8] = include_bytes!("assets/kadocomi-decrypted.webp");

#[test]
fn raw_provider_pages_preserve_bytes_and_format() {
    for provider in [
        ProviderImage::Piccoma { scrambled: false },
        ProviderImage::FuzRaw,
        ProviderImage::KadokomiRaw,
    ] {
        let result = process_provider_image(WEBP, provider).unwrap();
        assert_eq!(result.bytes(), WEBP);
        assert_eq!(result.format(), DirectFormat::WebP);
    }
}

#[test]
fn piccoma_scrambled_is_explicitly_unsupported() {
    let error =
        process_provider_image(JPEG, ProviderImage::Piccoma { scrambled: true }).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
}

#[cfg(feature = "kadokomi")]
#[test]
fn kadokomi_xor_matches_known_plaintext() {
    let encrypted = include_bytes!("assets/kadocomi-encrypted.webp");
    let key = "0123456789abcdef";
    // Test generated ciphertext with a non-secret key; existing fixture is verified elsewhere.
    let result =
        process_provider_image(encrypted, ProviderImage::KadokomiXor { key_hex: key }).unwrap();
    assert_eq!(result.bytes(), WEBP);
}

#[cfg(feature = "magapoke")]
#[test]
fn invalid_magapoke_seed_is_invalid_argument() {
    let error = process_provider_image(
        JPEG,
        ProviderImage::Magapoke {
            title: 29,
            episode: 140481,
            seed: "!",
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::InvalidArgument);
}

#[cfg(feature = "fuz")]
#[test]
fn fuz_aes_decrypts_exact_plaintext_and_rejects_bad_keys() {
    use aes::cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
    let key = [0x11u8; 32];
    let iv = [0x22u8; 16];
    let mut padded = vec![0u8; WEBP.len() + 16];
    padded[..WEBP.len()].copy_from_slice(WEBP);
    let encrypted = cbc::Encryptor::<aes::Aes256>::new(&key.into(), &iv.into())
        .encrypt_padded::<Pkcs7>(&mut padded, WEBP.len())
        .unwrap()
        .to_vec();
    let key_hex = "11".repeat(32);
    let iv_hex = "22".repeat(16);
    let output = process_provider_image(
        &encrypted,
        ProviderImage::FuzAesCbc {
            key_hex: &key_hex,
            iv_hex: &iv_hex,
        },
    )
    .unwrap();
    assert_eq!(output.bytes(), WEBP);
    assert_eq!(output.format(), DirectFormat::WebP);
    assert_eq!(
        process_provider_image(
            &encrypted,
            ProviderImage::FuzAesCbc {
                key_hex: "broken",
                iv_hex: &iv_hex
            }
        )
        .unwrap_err()
        .kind(),
        DirectErrorKind::InvalidArgument
    );
}

#[cfg(feature = "comici")]
#[test]
fn comici_real_unaligned_jpeg_is_unsupported() {
    let image = include_bytes!("../../cli/tests/assets/comici-youngchampion-public-20261111.jpg");
    let meta: serde_json::Value = serde_json::from_str(include_str!(
        "../../cli/tests/assets/comici-youngchampion-public-20261111.permutation.json"
    ))
    .unwrap();
    let sources = meta["tile_sources"].to_string();
    let scramble = serde_json::from_value(serde_json::Value::String(sources)).unwrap();
    let error = process_provider_image(
        image,
        ProviderImage::Comici {
            scramble: &scramble,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
}

#[cfg(feature = "comici")]
#[test]
fn comici_png_restores_column_major_mapping_and_keeps_rgba16_samples() {
    use image::{DynamicImage, ImageBuffer, Rgba};
    use std::io::Cursor;
    let original: ImageBuffer<Rgba<u16>, Vec<u16>> = ImageBuffer::from_fn(17, 19, |x, y| {
        Rgba([
            x as u16 * 100,
            y as u16 * 100,
            (x + y) as u16,
            x as u16 * 1000,
        ])
    });
    let sources = [8usize, 10, 4, 1, 11, 15, 2, 6, 5, 14, 13, 7, 3, 0, 12, 9];
    let scramble = serde_json::from_value(serde_json::Value::String(
        serde_json::to_string(&sources).unwrap(),
    ))
    .unwrap();
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba16(original.clone())
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let output = process_provider_image(
        bytes.get_ref(),
        ProviderImage::Comici {
            scramble: &scramble,
        },
    )
    .unwrap();
    assert_eq!(output.format(), DirectFormat::Png);
    let result = image::load_from_memory(output.bytes()).unwrap();
    assert_eq!(result.color(), image::ColorType::Rgba16);
    let result = result.to_rgba16();
    for y in 0..19 {
        for x in 0..17 {
            let expected = if x < 16 && y < 16 {
                let d = (x / 4 * 4 + y / 4) as usize;
                let s = sources[d] as u32;
                original.get_pixel(s / 4 * 4 + x % 4, s % 4 * 4 + y % 4)
            } else {
                original.get_pixel(x, y)
            };
            assert_eq!(result.get_pixel(x, y), expected, "{x},{y}");
        }
    }
}

#[cfg(feature = "comici")]
#[test]
fn png_metadata_is_rejected_for_restoration_but_raw_bytes_are_preserved() {
    use std::io::Cursor;
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        32,
        32,
        image::Rgba([10, 20, 30, 40]),
    ));
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut data = png.into_inner();
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&5u32.to_be_bytes());
    chunk.extend_from_slice(b"tEXt");
    chunk.extend_from_slice(b"a\0abc");
    chunk.extend_from_slice(&crc32fast::hash(&chunk[4..]).to_be_bytes());
    data.splice(33..33, chunk);
    let scramble = serde_json::from_value(serde_json::Value::String(
        "[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]".into(),
    ))
    .unwrap();
    assert_eq!(
        process_provider_image(
            &data,
            ProviderImage::Comici {
                scramble: &scramble
            }
        )
        .unwrap_err()
        .kind(),
        DirectErrorKind::Unsupported
    );
    assert_eq!(
        process_provider_image(&data, ProviderImage::FuzRaw)
            .unwrap()
            .bytes(),
        data
    );
}

#[cfg(feature = "magapoke")]
#[test]
fn magapoke_seed_restores_the_expected_png_tiles() {
    use std::io::Cursor;
    let source = image::RgbaImage::from_fn(65, 66, |x, y| {
        image::Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    });
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(source.clone())
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let result = process_provider_image(
        bytes.get_ref(),
        ProviderImage::Magapoke {
            title: 29,
            episode: 140481,
            seed: "q6j",
        },
    )
    .unwrap();
    assert_eq!(result.format(), DirectFormat::Png);
    let actual = image::load_from_memory(result.bytes()).unwrap().to_rgba8();
    let expected = manga::viewer::magapoke::restore_image(source, 29, 140481, "q6j").unwrap();
    assert_eq!(actual, expected);
}

#[cfg(feature = "comici")]
#[test]
fn comici_scramble_can_be_constructed_from_validated_provider_array() {
    use manga::viewer::comici::data::Scramble;
    let valid = [8, 10, 4, 1, 11, 15, 2, 6, 5, 14, 13, 7, 3, 0, 12, 9];
    let scramble = Scramble::try_from(valid).unwrap();
    assert_eq!(*scramble.tile_sources(), valid);
    let mut invalid = valid;
    invalid[0] = invalid[1];
    assert!(Scramble::try_from(invalid).is_err());
    invalid[0] = 16;
    assert!(Scramble::try_from(invalid).is_err());
}

#[cfg(feature = "comici")]
#[test]
fn comici_tiny_image_with_no_tiles_preserves_original_bytes() {
    use std::io::Cursor;
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        3,
        5,
        image::Rgba([3, 9, 7, 4]),
    ));
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let scramble = manga::viewer::comici::data::Scramble::try_from([
        8, 10, 4, 1, 11, 15, 2, 6, 5, 14, 13, 7, 3, 0, 12, 9,
    ])
    .unwrap();
    let output = process_provider_image(
        png.get_ref(),
        ProviderImage::Comici {
            scramble: &scramble,
        },
    )
    .unwrap();
    assert_eq!(output.bytes(), png.get_ref());
}

#[cfg(not(feature = "fuz"))]
#[test]
fn disabled_fuz_encryption_is_unsupported() {
    assert_eq!(
        process_provider_image(
            WEBP,
            ProviderImage::FuzAesCbc {
                key_hex: "",
                iv_hex: ""
            }
        )
        .unwrap_err()
        .kind(),
        DirectErrorKind::Unsupported
    );
}
#[cfg(not(feature = "magapoke"))]
#[test]
fn disabled_magapoke_is_unsupported() {
    assert_eq!(
        process_provider_image(
            JPEG,
            ProviderImage::Magapoke {
                title: 1,
                episode: 1,
                seed: "q"
            }
        )
        .unwrap_err()
        .kind(),
        DirectErrorKind::Unsupported
    );
}
#[cfg(not(feature = "kadokomi"))]
#[test]
fn disabled_kadokomi_encryption_is_unsupported() {
    assert_eq!(
        process_provider_image(WEBP, ProviderImage::KadokomiXor { key_hex: "" })
            .unwrap_err()
            .kind(),
        DirectErrorKind::Unsupported
    );
}

#[test]
fn expected_dimensions_are_checked_after_processing_without_changing_bytes() {
    use image::GenericImageView;
    use manga::direct::process_provider_image_checked;
    let (width, height) = image::load_from_memory(WEBP).unwrap().dimensions();
    let output =
        process_provider_image_checked(WEBP, ProviderImage::FuzRaw, Some((width, height))).unwrap();
    assert_eq!(output.bytes(), WEBP);
    let error =
        process_provider_image_checked(WEBP, ProviderImage::FuzRaw, Some((width + 1, height)))
            .unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Malformed);
    assert_eq!(
        process_provider_image_checked(WEBP, ProviderImage::FuzRaw, Some((0, height)))
            .unwrap_err()
            .kind(),
        DirectErrorKind::InvalidArgument
    );
}

#[test]
fn preserve_rejects_jpeg_without_structural_eoi_even_with_fake_marker_in_metadata() {
    use std::io::Cursor;
    let source = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(40, 36, |x, y| {
        image::Rgb([(x * 3) as u8, (y * 5) as u8, (x ^ y) as u8])
    }));
    let mut encoded = Cursor::new(Vec::new());
    source
        .write_to(&mut encoded, image::ImageFormat::Jpeg)
        .unwrap();
    let original = encoded.into_inner();
    assert_eq!(&original[original.len() - 2..], &[0xff, 0xd9]);
    let mut metadata = original.clone();
    metadata.splice(2..2, [0xff, 0xe1, 0, 6, 0x11, 0xff, 0xd9, 0x22]);
    assert_eq!(
        process_provider_image(&metadata, ProviderImage::FuzRaw)
            .unwrap()
            .bytes(),
        metadata
    );
    for broken in [
        &original[..original.len() / 2],
        &metadata[..metadata.len() - 2],
    ] {
        assert_eq!(
            process_provider_image(broken, ProviderImage::FuzRaw)
                .unwrap_err()
                .kind(),
            DirectErrorKind::Malformed
        );
    }
}

#[cfg(feature = "direct-sort")]
#[test]
fn sixteen_parallel_giga_restorations_match_serial_bytes_and_pixels() {
    use std::sync::{Arc, Barrier};
    let input = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    let expected = process_provider_image(input, ProviderImage::Giga).unwrap();
    let barrier = Arc::new(Barrier::new(16));
    let handles: Vec<_> = (0..16)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                let webp = process_provider_image(WEBP, ProviderImage::FuzRaw).unwrap();
                assert_eq!(webp.bytes(), WEBP);
                process_provider_image(input, ProviderImage::Giga).unwrap()
            })
        })
        .collect();
    let pixels = image::load_from_memory(expected.bytes()).unwrap().to_rgb8();
    for handle in handles {
        let page = handle.join().unwrap();
        assert_eq!(page.format(), DirectFormat::Jpeg);
        assert_eq!(page.bytes(), expected.bytes());
        assert_eq!(
            image::load_from_memory(page.bytes()).unwrap().to_rgb8(),
            pixels
        );
    }
}

#[test]
fn corrupt_webp_is_rejected_when_libwebp_cannot_decode() {
    let rgb = image::RgbImage::from_fn(67, 45, |x, y| {
        image::Rgb([
            (x * 13 + y * 17) as u8,
            (x * 47 + y * 3) as u8,
            (x * 2 + y * 77) as u8,
        ])
    });
    let mut lossy = webp::Encoder::from_rgb(rgb.as_raw(), 67, 45)
        .encode(80.0)
        .to_vec();
    let mut lossless = webp::Encoder::from_rgb(rgb.as_raw(), 67, 45)
        .encode_lossless()
        .to_vec();
    for valid in [&lossy, &lossless] {
        let result = process_provider_image(valid, ProviderImage::KadokomiRaw).unwrap();
        assert_eq!(result.bytes(), valid);
    }
    lossy[286] ^= 1 << 1;
    lossless[67] ^= 1 << 6;
    for broken in [&lossy, &lossless] {
        assert!(webp::Decoder::new(broken).decode().is_none());
        assert_eq!(
            process_provider_image(broken, ProviderImage::KadokomiRaw)
                .unwrap_err()
                .kind(),
            DirectErrorKind::Malformed
        );
    }
}

#[test]
fn webp_animation_frames_are_validated_without_changing_source_bytes() {
    let first = vec![10; 12 * 12 * 3];
    let second = vec![70; 12 * 12 * 3];
    let config = webp::WebPConfig::new().unwrap();
    let mut encoder = webp::AnimEncoder::new(12, 12, &config);
    encoder.add_frame(webp::AnimFrame::from_rgb(&first, 12, 12, 0));
    encoder.add_frame(webp::AnimFrame::from_rgb(&second, 12, 12, 100));
    let bytes = encoder.encode().to_vec();
    assert!(
        webp::BitstreamFeatures::new(&bytes)
            .unwrap()
            .has_animation()
    );
    assert_eq!(
        process_provider_image(&bytes, ProviderImage::FuzRaw)
            .unwrap()
            .bytes(),
        bytes
    );
    assert_eq!(
        manga::direct::process_provider_image_checked(
            &bytes,
            ProviderImage::FuzRaw,
            Some((12, 12))
        )
        .unwrap()
        .bytes(),
        bytes
    );
    let positions: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == b"ANMF").then_some(i))
        .collect();
    assert!(positions.len() >= 2);
    let mut damaged = bytes.clone();
    damaged[positions[1] + 24] ^= 0x40;
    assert_eq!(
        process_provider_image(&damaged, ProviderImage::FuzRaw)
            .unwrap_err()
            .kind(),
        DirectErrorKind::Malformed
    );
}

#[test]
fn webp_icc_metadata_and_image_bytes_are_preserved() {
    let rgb = vec![23u8; 12 * 12 * 3];
    let original = webp::Encoder::from_rgb(&rgb, 12, 12)
        .encode_lossless()
        .to_vec();
    let mut extended = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0".to_vec();
    extended.extend_from_slice(&[0x20, 0, 0, 0, 11, 0, 0, 11, 0, 0]);
    extended.extend_from_slice(b"ICCP\x04\0\0\0test");
    extended.extend_from_slice(&original[12..]);
    let length = (extended.len() - 8) as u32;
    extended[4..8].copy_from_slice(&length.to_le_bytes());
    assert!(webp::Decoder::new(&extended).decode().is_some());
    assert_eq!(
        process_provider_image(&extended, ProviderImage::KadokomiRaw)
            .unwrap()
            .bytes(),
        extended
    );
}

#[test]
fn raw_jpeg_rejects_truncated_entropy_even_with_a_forged_eoi() {
    let input = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    for percent in [10, 25, 50, 75, 90, 99] {
        let mut truncated = input[..input.len() * percent / 100].to_vec();
        truncated.extend_from_slice(&[0xff, 0xd9]);
        let error = process_provider_image(&truncated, ProviderImage::FuzRaw).unwrap_err();
        assert_eq!(error.kind(), DirectErrorKind::Malformed, "at {percent}%");
    }
    let result = process_provider_image(input, ProviderImage::FuzRaw).unwrap();
    assert_eq!(result.bytes(), input);
}

#[test]
fn raw_progressive_jpeg_with_exif_is_preserved_byte_for_byte() {
    let input = include_bytes!("assets/giga-public-progressive.jpg");
    let result = process_provider_image(input, ProviderImage::FuzRaw).unwrap();
    assert_eq!(result.bytes(), input);
    assert_eq!(result.mime_type(), "image/jpeg");
    for percent in [10, 25, 50, 75, 90, 99] {
        let mut truncated = input[..input.len() * percent / 100].to_vec();
        truncated.extend_from_slice(&[0xff, 0xd9]);
        let error = process_provider_image(&truncated, ProviderImage::FuzRaw).unwrap_err();
        assert_eq!(error.kind(), DirectErrorKind::Malformed, "at {percent}%");
    }
}
