use manga::direct::{DirectErrorKind, DirectFormat, DirectTransform, process_direct};

#[test]
fn preserve_returns_original_bytes_and_actual_jpeg_mime() {
    let input = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    let output = process_direct(input, DirectTransform::Preserve).unwrap();
    assert_eq!(output.bytes(), input);
    assert_eq!(output.format(), DirectFormat::Jpeg);
    assert_eq!(output.mime_type(), "image/jpeg");
}

#[test]
fn invalid_input_does_not_get_a_fictitious_mime() {
    let err = process_direct(b"not an image", DirectTransform::Preserve).unwrap_err();
    assert_eq!(err.kind(), DirectErrorKind::Unsupported);
}

#[cfg(feature = "direct-sort")]
#[test]
fn giga_restores_a_real_jpeg_without_changing_its_format() {
    use manga::{solver::ImageSolver, viewer::giga::solver::Solver};
    let input = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    let output = process_direct(input, DirectTransform::Giga).unwrap();
    assert_eq!(output.format(), DirectFormat::Jpeg);
    assert_eq!(output.mime_type(), "image/jpeg");
    let result = image::load_from_memory(output.bytes()).unwrap().into_rgb8();
    let expected = Solver::new()
        .solve_from_bytes(input.to_vec())
        .unwrap()
        .into_rgb8();
    assert_eq!(result, expected);
}

#[cfg(feature = "direct-sort")]
#[test]
fn invalid_permutation_is_a_caller_error() {
    let input = include_bytes!("assets/magapoke-140481-page0-raw.jpg");
    let err = process_direct(
        input,
        DirectTransform::Tiles4x4 {
            tile_width: 240,
            tile_height: 344,
            source_by_destination: &[0; 16],
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), DirectErrorKind::InvalidArgument);
}

#[test]
fn preserve_keeps_png_and_webp_bytes_without_reencoding() {
    use std::io::Cursor;
    let image = image::DynamicImage::new_rgb8(4, 4);
    for (source, format, mime) in [
        (image::ImageFormat::Png, DirectFormat::Png, "image/png"),
        (image::ImageFormat::WebP, DirectFormat::WebP, "image/webp"),
    ] {
        let mut encoded = Cursor::new(Vec::new());
        image.write_to(&mut encoded, source).unwrap();
        let input = encoded.into_inner();
        let result = process_direct(&input, DirectTransform::Preserve).unwrap();
        assert_eq!(result.bytes(), input);
        assert_eq!(result.format(), format);
        assert_eq!(result.mime_type(), mime);
    }
}

#[cfg(feature = "kadokomi")]
#[test]
fn kadocomi_decrypts_and_preserves_original_webp_bytes() {
    let encrypted = include_bytes!("assets/kadocomi-encrypted.webp");
    let original = include_bytes!("assets/kadocomi-decrypted.webp");
    let result = process_direct(
        encrypted,
        DirectTransform::Kadocomi {
            key: "0123456789abcdef",
        },
    )
    .unwrap();
    assert_eq!(result.bytes(), original);
    assert_eq!(result.format(), DirectFormat::WebP);
    assert_eq!(result.mime_type(), "image/webp");
}

#[test]
fn format_signature_alone_does_not_make_malformed_data_valid() {
    let error = process_direct(b"\x89PNG\r\n\x1a\n", DirectTransform::Preserve).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Malformed);
}

#[cfg(not(feature = "direct-sort"))]
#[test]
fn disabled_coefficient_backend_returns_a_clear_reason() {
    let input = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    let error = process_direct(input, DirectTransform::Giga).unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Unsupported);
    assert!(error.to_string().contains("direct-sort"));
}
