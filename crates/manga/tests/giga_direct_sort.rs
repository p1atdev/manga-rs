#![cfg(all(feature = "giga", feature = "direct-sort"))]

use manga::{
    solver::ImageSolver,
    viewer::giga::{direct_sort::sort_jpeg, solver::Solver},
};

const PUBLIC_JPEG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/giga-tonari-yj-public-20261111.jpg"
));

#[test]
fn sort_jpeg_matches_solver_on_public_jpeg() {
    let output = sort_jpeg(PUBLIC_JPEG).unwrap();

    assert_eq!(&output[..2], &[0xFF, 0xD8], "missing JPEG SOI magic");
    assert_eq!(
        image::guess_format(&output).unwrap(),
        image::ImageFormat::Jpeg
    );

    let actual = image::load_from_memory(&output).unwrap().into_rgb8();
    let expected = Solver::new()
        .solve_from_bytes(PUBLIC_JPEG.to_vec())
        .unwrap()
        .into_rgb8();

    assert_eq!(actual.dimensions(), expected.dimensions());
    assert!(
        actual.as_raw() == expected.as_raw(),
        "sort_jpeg pixels differ from Solver::solve_from_bytes"
    );
}

#[test]
fn sort_jpeg_preserves_source_dimensions() {
    let source = image::load_from_memory(PUBLIC_JPEG).unwrap();
    let output = sort_jpeg(PUBLIC_JPEG).unwrap();
    let decoded = image::load_from_memory(&output).unwrap();

    assert_eq!(
        (decoded.width(), decoded.height()),
        (source.width(), source.height())
    );
}

#[test]
fn sort_jpeg_rejects_empty_input() {
    assert!(sort_jpeg(&[]).is_err());
}

#[test]
fn sort_jpeg_rejects_non_jpeg_input() {
    assert!(sort_jpeg(b"not a jpeg at all").is_err());
    assert!(sort_jpeg(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).is_err());
}

#[test]
fn sort_jpeg_rejects_header_only_input() {
    assert!(sort_jpeg(&PUBLIC_JPEG[..2]).is_err());
    assert!(sort_jpeg(&PUBLIC_JPEG[..64]).is_err());
}

#[test]
fn sort_jpeg_rejects_truncated_input() {
    for len in [
        PUBLIC_JPEG.len() / 4,
        PUBLIC_JPEG.len() / 2,
        PUBLIC_JPEG.len() - 2,
    ] {
        assert!(
            sort_jpeg(&PUBLIC_JPEG[..len]).is_err(),
            "truncated to {len} bytes was accepted"
        );
    }
}

// --- error classification -------------------------------------------------------------------

use manga::viewer::giga::direct_sort::DirectSortErrorKind;

fn kind_of(input: &[u8]) -> DirectSortErrorKind {
    sort_jpeg(input).expect_err("input must be rejected").kind()
}

/// Returns the offset of the first byte after the `FF <marker>` pair of the first `marker` segment.
fn find_segment(jpeg: &[u8], marker: u8) -> usize {
    let mut i = 2;
    while i + 4 <= jpeg.len() {
        assert_eq!(jpeg[i], 0xFF, "unexpected byte while scanning segments");
        let m = jpeg[i + 1];
        let len = ((jpeg[i + 2] as usize) << 8) | jpeg[i + 3] as usize;
        if m == marker {
            return i + 2;
        }
        assert_ne!(m, 0xDA, "reached SOS before marker {marker:#x}");
        i += 2 + len;
    }
    panic!("marker {marker:#x} not found");
}

/// Inserts a complete marker segment after SOI and after the JFIF APP0 (if any), so that only the
/// inserted segment, not the JFIF position rule, decides the outcome.
fn with_segment_after_soi(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
    let at = if jpeg[2..4] == [0xFF, 0xE0] && &jpeg[6..11] == b"JFIF\0" {
        4 + (((jpeg[4] as usize) << 8) | jpeg[5] as usize)
    } else {
        2
    };
    let len = (payload.len() + 2) as u16;
    let mut out = jpeg[..at].to_vec();
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&jpeg[at..]);
    out
}

#[test]
fn error_kind_empty_and_one_byte_are_malformed() {
    assert_eq!(kind_of(&[]), DirectSortErrorKind::Malformed);
    assert_eq!(kind_of(&[0xFF]), DirectSortErrorKind::Malformed);
}

#[test]
fn error_kind_non_jpeg_is_unsupported() {
    assert_eq!(
        kind_of(b"not a jpeg at all"),
        DirectSortErrorKind::Unsupported
    );
    assert!(
        sort_jpeg(b"not a jpeg at all")
            .unwrap_err()
            .is_unsupported()
    );
}

#[test]
fn error_kind_truncated_is_malformed() {
    for len in [64, PUBLIC_JPEG.len() / 2, PUBLIC_JPEG.len() - 2] {
        assert_eq!(
            kind_of(&PUBLIC_JPEG[..len]),
            DirectSortErrorKind::Malformed,
            "len {len}"
        );
    }
}

#[test]
fn rejects_dimensions_that_do_not_match_the_encoded_data() {
    let mut data = PUBLIC_JPEG.to_vec();
    let sof = find_segment(&data, 0xC0);
    // Segment layout after the marker: length(2) precision(1) height(2) width(2).
    data[sof + 5..sof + 7].copy_from_slice(&20000u16.to_be_bytes());
    assert!(sort_jpeg(&data).is_err());

    let mut data = PUBLIC_JPEG.to_vec();
    data[sof + 3..sof + 5].copy_from_slice(&20000u16.to_be_bytes());
    assert!(sort_jpeg(&data).is_err());
}

#[test]
fn error_kind_chroma_subsampling_is_unsupported() {
    let mut data = PUBLIC_JPEG.to_vec();
    let sof = find_segment(&data, 0xC0);
    assert_eq!(
        data[sof + 7],
        3,
        "fixture is expected to be a 3-component JPEG"
    );
    // First component: id(1) sampling(1) quant(1); 0x22 turns 4:4:4 into 4:2:0.
    data[sof + 9] = 0x22;
    assert_eq!(kind_of(&data), DirectSortErrorKind::Unsupported);
}

#[test]
fn error_kind_progressive_is_unsupported() {
    let mut data = PUBLIC_JPEG.to_vec();
    let sof = find_segment(&data, 0xC0);
    data[sof - 1] = 0xC2;
    assert_eq!(kind_of(&data), DirectSortErrorKind::Unsupported);
}

#[test]
fn error_kind_trailing_data_after_eoi_is_unsupported() {
    let mut data = PUBLIC_JPEG.to_vec();
    data.extend_from_slice(b"trailing");
    assert!(sort_jpeg(&data).is_err());
}

// --- strict metadata ------------------------------------------------------------------------

#[test]
fn metadata_comment_exif_xmp_and_unknown_app_are_rejected() {
    let cases: [(u8, &[u8]); 4] = [
        (0xFE, b"comment"),
        (0xE1, b"Exif\0\0II*\0\x08\0\0\0\0\0"),
        (0xE1, b"http://ns.adobe.com/xap/1.0/\0<x/>"),
        (0xEB, b"unknown application data"),
    ];
    for (marker, payload) in cases {
        let data = with_segment_after_soi(PUBLIC_JPEG, marker, payload);
        assert_eq!(
            kind_of(&data),
            DirectSortErrorKind::Unsupported,
            "marker {marker:#x} was not rejected as unsupported"
        );
    }
}

#[test]
fn metadata_adobe_app14_is_rejected() {
    let data = with_segment_after_soi(PUBLIC_JPEG, 0xEE, b"Adobe\0\x64\x80\0\0\0\0\x01");
    assert_eq!(kind_of(&data), DirectSortErrorKind::Unsupported);
}

#[test]
fn metadata_jfif_thumbnail_is_rejected() {
    // Rewrite the thumbnail dimensions inside the existing JFIF segment (only if there is one).
    let mut data = PUBLIC_JPEG.to_vec();
    if data[2..4] != [0xFF, 0xE0] || &data[6..11] != b"JFIF\0" {
        return;
    }
    data[6 + 12] = 1;
    data[6 + 13] = 1;
    // libjpeg may flag the inconsistent length as a warning (Malformed) before the shim sees it.
    assert!(sort_jpeg(&data).is_err());
}

#[test]
fn metadata_malformed_icc_numbering_is_rejected() {
    // Chunk 2 of 1 is invalid numbering.
    let mut payload = b"ICC_PROFILE\0".to_vec();
    payload.extend_from_slice(&[2, 1, 0xAA, 0xBB]);
    let data = with_segment_after_soi(PUBLIC_JPEG, 0xE2, &payload);
    assert_eq!(kind_of(&data), DirectSortErrorKind::Malformed);

    // Chunk 1 of 2 without chunk 2 is an incomplete profile.
    let mut payload = b"ICC_PROFILE\0".to_vec();
    payload.extend_from_slice(&[1, 2, 0xAA, 0xBB]);
    let data = with_segment_after_soi(PUBLIC_JPEG, 0xE2, &payload);
    assert_eq!(kind_of(&data), DirectSortErrorKind::Malformed);
}

// --- real grayscale fixtures ----------------------------------------------------------------

macro_rules! grayscale_fixture_tests {
    ($($name:ident => $file:literal),* $(,)?) => {$(
        #[test]
        fn $name() {
            const SOURCE: &[u8] = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/",
                $file
            ));
            assert_grayscale_fixture(SOURCE);
        }
    )*};
}

grayscale_fixture_tests! {
    grayscale_jumpplus_page0 => "jumpplus-page0.jpg",
    grayscale_jumpplus_page1 => "jumpplus-page1.jpg",
    grayscale_kuragebunch_page3 => "kuragebunch-page3.jpg",
    grayscale_kuragebunch_page4 => "kuragebunch-page4.jpg",
}

fn assert_grayscale_fixture(source: &[u8]) {
    let source_image = image::load_from_memory(source).unwrap();
    assert_eq!(
        source_image.color(),
        image::ColorType::L8,
        "fixture is expected to be grayscale"
    );

    let output = sort_jpeg(source).unwrap();
    let decoded = image::load_from_memory(&output).unwrap();
    assert_eq!(decoded.color(), image::ColorType::L8);
    assert_eq!(
        (decoded.width(), decoded.height()),
        (source_image.width(), source_image.height()),
        "output dimensions changed"
    );

    let expected = Solver::new().solve_from_bytes(source.to_vec()).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (expected.width(), expected.height())
    );
    assert!(
        decoded.to_luma8().as_raw() == expected.to_luma8().as_raw(),
        "sort_jpeg pixels differ from Solver"
    );

    // The unscramble is deterministic.
    assert_eq!(
        output,
        sort_jpeg(source).unwrap(),
        "repeated output differs"
    );
}

// --- repeated sort --------------------------------------------------------------------------

/// The tile swap is an involution, so sorting twice must give back the source coefficients. Only
/// decoded pixels are compared here (an exact coefficient comparison is done outside this test).
#[test]
fn sort_twice_restores_source_pixels() {
    let once = sort_jpeg(PUBLIC_JPEG).unwrap();
    let twice = sort_jpeg(&once).unwrap();
    let source = image::load_from_memory(PUBLIC_JPEG).unwrap().into_rgb8();
    let restored = image::load_from_memory(&twice).unwrap().into_rgb8();
    assert!(
        source.as_raw() == restored.as_raw(),
        "double sort changed pixels"
    );
}

fn generated_grayscale_jpeg(width: u32, height: u32) -> Vec<u8> {
    let pixels = vec![128u8; (width as usize) * (height as usize)];
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut output)
        .encode(&pixels, width, height, image::ExtendedColorType::L8)
        .unwrap();
    output
}

#[test]
fn valid_jpeg_wider_than_16384_is_not_rejected_by_a_custom_limit() {
    let jpeg = generated_grayscale_jpeg(16_385, 1);
    let output = sort_jpeg(&jpeg).expect("valid dimensions should not be rejected");
    assert_eq!(
        image::ImageReader::new(std::io::Cursor::new(output))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap(),
        (16_385, 1)
    );
}

#[test]
fn valid_jpeg_over_32_megapixels_is_not_rejected_by_a_custom_limit() {
    let jpeg = generated_grayscale_jpeg(8_192, 4_097);
    let output = sort_jpeg(&jpeg).expect("valid pixel count should not be rejected");
    assert_eq!(
        image::ImageReader::new(std::io::Cursor::new(output))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap(),
        (8_192, 4_097)
    );
}

#[test]
fn large_input_is_classified_by_content_instead_of_a_custom_byte_limit() {
    let mut input = PUBLIC_JPEG.to_vec();
    input.resize(64 * 1024 * 1024 + 1, 0);
    let error = sort_jpeg(&input).unwrap_err();
    assert_eq!(error.kind(), DirectSortErrorKind::Unsupported);
    assert!(error.to_string().contains("trailing data"));
}
