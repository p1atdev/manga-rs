#![cfg(all(feature = "giga", feature = "direct-sort"))]

use image::RgbImage;
use manga::viewer::giga::direct_sort::{DirectSortErrorKind, permute_jpeg, sort_jpeg};

const GIGA_JPEG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/giga-tonari-yj-public-20261111.jpg"
));
const GRAY_JPEG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/magapoke-140481-page0-raw.jpg"
));
const GRAY_PERMUTATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/magapoke-140481.permutation.json"
));
const COLOR_JPEG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/magapoke-320763-page1-raw.jpg"
));
const COLOR_PERMUTATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/magapoke-320763.permutation.json"
));

const IDENTITY: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

fn parse_permutation(json: &str) -> Vec<usize> {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    value["source_tile_by_destination_row_major"]
        .as_array()
        .expect("permutation array")
        .iter()
        .map(|v| v.as_u64().expect("integer entry") as usize)
        .collect()
}

fn decode(jpeg: &[u8]) -> RgbImage {
    image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .unwrap()
        .into_rgb8()
}

/// Largest tile size (a multiple of 8) such that a 4x4 grid fits in `dim`.
fn tile_size(dim: u32) -> u32 {
    dim / 4 / 8 * 8
}

/// Independently builds the expected image: starts from the original and, for every destination
/// tile, copies the pixels of its source tile out of the untouched original.
fn expected_pixels(original: &RgbImage, tw: u32, th: u32, perm: &[usize]) -> RgbImage {
    let mut expected = original.clone();
    for (dest, &source) in perm.iter().enumerate() {
        let (dx, dy) = ((dest % 4) as u32 * tw, (dest / 4) as u32 * th);
        let (sx, sy) = ((source % 4) as u32 * tw, (source / 4) as u32 * th);
        for y in 0..th {
            for x in 0..tw {
                expected.put_pixel(dx + x, dy + y, *original.get_pixel(sx + x, sy + y));
            }
        }
    }
    expected
}

fn assert_permutation_matches(jpeg: &[u8], perm: &[usize], tw: u32, th: u32) {
    let original = decode(jpeg);
    let output = permute_jpeg(jpeg, tw, th, perm).unwrap();

    assert_eq!(&output[..2], &[0xFF, 0xD8], "missing JPEG SOI magic");
    let actual = decode(&output);
    assert_eq!(actual.dimensions(), original.dimensions());

    let expected = expected_pixels(&original, tw, th, perm);
    assert!(
        actual.as_raw() == expected.as_raw(),
        "permute_jpeg pixels differ from independently copied blocks"
    );

    // Right and bottom strips outside the grid must be bit-identical to the original.
    let (w, h) = original.dimensions();
    for y in 0..h {
        for x in 0..w {
            if x >= 4 * tw || y >= 4 * th {
                assert_eq!(
                    actual.get_pixel(x, y),
                    original.get_pixel(x, y),
                    "strip pixel ({x}, {y}) changed"
                );
            }
        }
    }
}

fn kind_of(
    result: Result<Vec<u8>, manga::viewer::giga::direct_sort::DirectSortError>,
) -> DirectSortErrorKind {
    result.expect_err("expected an error").kind()
}

#[test]
fn fixture_permutations_are_bijections() {
    for json in [GRAY_PERMUTATION, COLOR_PERMUTATION] {
        let mut perm = parse_permutation(json);
        assert_eq!(perm.len(), 16);
        perm.sort_unstable();
        assert_eq!(perm, IDENTITY);
    }
}

#[test]
fn permutes_grayscale_magapoke_page() {
    let (w, h) = decode(GRAY_JPEG).dimensions();
    let perm = parse_permutation(GRAY_PERMUTATION);
    assert_permutation_matches(GRAY_JPEG, &perm, tile_size(w), tile_size(h));
}

#[test]
fn permutes_color_magapoke_page() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    let perm = parse_permutation(COLOR_PERMUTATION);
    assert_permutation_matches(COLOR_JPEG, &perm, tile_size(w), tile_size(h));
}

#[test]
fn smaller_grid_leaves_larger_strips_untouched() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    let (tw, th) = (tile_size(w) - 8, tile_size(h) - 8);
    let perm = parse_permutation(COLOR_PERMUTATION);
    assert_permutation_matches(COLOR_JPEG, &perm, tw, th);
}

#[test]
fn non_square_tiles_and_a_reversal_permutation() {
    let (w, h) = decode(GRAY_JPEG).dimensions();
    let (tw, th) = (tile_size(w), tile_size(h) - 8);
    let perm: Vec<usize> = (0..16).rev().collect();
    assert_permutation_matches(GRAY_JPEG, &perm, tw, th);
}

#[test]
fn identity_permutation_preserves_pixels() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    assert_permutation_matches(COLOR_JPEG, &IDENTITY, tile_size(w), tile_size(h));
}

#[test]
fn permutation_is_reversible_with_inverse() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    let (tw, th) = (tile_size(w), tile_size(h));
    let perm = parse_permutation(COLOR_PERMUTATION);
    let mut inverse = vec![0usize; 16];
    for (dest, &source) in perm.iter().enumerate() {
        inverse[source] = dest;
    }
    let once = permute_jpeg(COLOR_JPEG, tw, th, &perm).unwrap();
    let twice = permute_jpeg(&once, tw, th, &inverse).unwrap();
    assert_eq!(decode(&twice).as_raw(), decode(COLOR_JPEG).as_raw());
}

#[test]
fn rejects_non_bijective_permutations_as_invalid_argument() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    let (tw, th) = (tile_size(w), tile_size(h));

    let mut duplicate = IDENTITY;
    duplicate[5] = duplicate[4];
    let mut out_of_range = IDENTITY;
    out_of_range[0] = 16;
    let mut huge = IDENTITY;
    huge[0] = 256;

    for bad in [
        duplicate.to_vec(),
        out_of_range.to_vec(),
        huge.to_vec(),
        IDENTITY[..15].to_vec(),
        [IDENTITY.as_slice(), &[0]].concat(),
        Vec::new(),
    ] {
        assert_eq!(
            kind_of(permute_jpeg(COLOR_JPEG, tw, th, &bad)),
            DirectSortErrorKind::InvalidArgument,
            "permutation {bad:?}"
        );
    }
}

#[test]
fn malformed_permutation_wins_over_unaligned_tiles() {
    let mut duplicate = IDENTITY;
    duplicate[1] = 0;
    assert_eq!(
        kind_of(permute_jpeg(COLOR_JPEG, 12, 12, &duplicate)),
        DirectSortErrorKind::InvalidArgument
    );
}

#[test]
fn rejects_zero_tile_dimensions_as_invalid_argument() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    for (tw, th) in [(0, tile_size(h)), (tile_size(w), 0), (0, 0)] {
        assert_eq!(
            kind_of(permute_jpeg(COLOR_JPEG, tw, th, &IDENTITY)),
            DirectSortErrorKind::InvalidArgument,
            "tile {tw}x{th}"
        );
    }
}

#[test]
fn rejects_non_multiple_of_eight_tiles_as_unsupported() {
    for (tw, th) in [(4, 8), (8, 4), (12, 16), (16, 20), (9, 9), (31, 32)] {
        let error = permute_jpeg(COLOR_JPEG, tw, th, &IDENTITY).unwrap_err();
        assert_eq!(
            error.kind(),
            DirectSortErrorKind::Unsupported,
            "tile {tw}x{th}"
        );
        assert!(error.is_unsupported());
    }
}

#[test]
fn rejects_grid_outside_image_as_invalid_argument() {
    let (w, h) = decode(COLOR_JPEG).dimensions();
    // One 8-pixel step larger than fits.
    let too_wide = tile_size(w) + 8;
    let too_tall = tile_size(h) + 8;
    assert_eq!(
        kind_of(permute_jpeg(COLOR_JPEG, too_wide, 8, &IDENTITY)),
        DirectSortErrorKind::InvalidArgument
    );
    assert_eq!(
        kind_of(permute_jpeg(COLOR_JPEG, 8, too_tall, &IDENTITY)),
        DirectSortErrorKind::InvalidArgument
    );
    // Huge tile dimensions must not overflow anything.
    for tile in [u32::MAX - 7, 1 << 31, 16_384 + 8] {
        let kind = kind_of(permute_jpeg(COLOR_JPEG, tile, tile, &IDENTITY));
        assert!(
            matches!(
                kind,
                DirectSortErrorKind::InvalidArgument | DirectSortErrorKind::LimitExceeded
            ),
            "tile {tile}: {kind:?}"
        );
    }
}

#[test]
fn permute_jpeg_keeps_the_jpeg_input_policy() {
    assert_eq!(
        kind_of(permute_jpeg(&[], 8, 8, &IDENTITY)),
        DirectSortErrorKind::Malformed
    );
    assert_eq!(
        kind_of(permute_jpeg(b"not a jpeg at all", 8, 8, &IDENTITY)),
        DirectSortErrorKind::Unsupported
    );
    let truncated = &COLOR_JPEG[..COLOR_JPEG.len() / 2];
    assert_eq!(
        kind_of(permute_jpeg(truncated, 8, 8, &IDENTITY)),
        DirectSortErrorKind::Malformed
    );
    let mut trailing = COLOR_JPEG.to_vec();
    trailing.extend_from_slice(b"junk");
    assert_eq!(
        kind_of(permute_jpeg(&trailing, 8, 8, &IDENTITY)),
        DirectSortErrorKind::Unsupported
    );
}

#[test]
fn sort_jpeg_equals_transpose_permutation() {
    let (w, h) = decode(GIGA_JPEG).dimensions();
    // Giga cells are dim / 32 blocks of 8 pixels, the same rule `sort_jpeg` applies.
    let (tw, th) = (w / 32 * 8, h / 32 * 8);
    let transpose: Vec<usize> = (0..16).map(|d| (d % 4) * 4 + d / 4).collect();

    let sorted = sort_jpeg(GIGA_JPEG).unwrap();
    let permuted = permute_jpeg(GIGA_JPEG, tw, th, &transpose).unwrap();
    assert_eq!(
        sorted, permuted,
        "sort_jpeg must stay a transpose permutation"
    );

    let original = decode(GIGA_JPEG);
    let expected = expected_pixels(&original, tw, th, &transpose);
    assert!(decode(&sorted).as_raw() == expected.as_raw());
}

#[test]
fn sort_jpeg_still_rejects_bad_input() {
    assert_eq!(kind_of(sort_jpeg(&[])), DirectSortErrorKind::Malformed);
    assert_eq!(
        kind_of(sort_jpeg(b"not a jpeg at all")),
        DirectSortErrorKind::Unsupported
    );
}
