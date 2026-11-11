#![cfg(feature = "piccoma")]

use image::{Rgba, RgbaImage};
use manga::viewer::piccoma::{parse_seed, restore_image, shuffle_order};
use url::Url;

// All fixtures are synthetic. Expected seeds were computed independently with a
// separate script implementing the getpiccoma rotate-then-bitflip algorithm.
fn u(checksum: &str, query: &str) -> Url {
    Url::parse(&format!(
        "https://example.invalid/img/{checksum}/001.jpg{query}"
    ))
    .unwrap()
}
const EXP: &str = "?expires=1234567890";

#[test]
fn uppercase_with_punctuation_and_digits_derives_seed() {
    let url = u("AB-CD_EF.GH12IJ+KLMNO~", EXP);
    assert_eq!(
        parse_seed(&url).unwrap().unwrap(),
        "\x7f@C-CD^DF/GH13HJ*KLMON"
    );
}

#[test]
fn uppercase_alnum_baseline_unchanged() {
    let url = u("ABCDEFGHIJKLMNOPQRSTUV", EXP);
    assert_eq!(parse_seed(&url).unwrap().unwrap(), "W@CCDEGFHHJKLLOOQQRSUT");
}

#[test]
fn lowercase_without_uppercase_is_none() {
    assert_eq!(
        parse_seed(&u("abcdefghij-klmnop_qrstu", EXP)).unwrap(),
        None
    );
    assert_eq!(parse_seed(&u("abcdefghij0123456789", EXP)).unwrap(), None);
}

#[test]
fn mixed_case_digits_only_and_punctuation_only_rejected() {
    assert!(parse_seed(&u("ABCDEFGHIJkLMNOPQRSTUV", EXP)).is_err());
    assert!(parse_seed(&u("12345678901234567890", EXP)).is_err());
    assert!(parse_seed(&u("----------____________", EXP)).is_err());
}

#[test]
fn malformed_checksum_rejected() {
    assert!(parse_seed(&u("ABCDEFGHI", EXP)).is_err()); // too short
    assert!(parse_seed(&u(&"A".repeat(129), EXP)).is_err());
    assert!(
        parse_seed(&Url::parse("https://example.invalid/001.jpg?expires=1234567890").unwrap())
            .is_err()
    );
    // encoded slash stays a literal segment, not a path split
    let url = u("ABCDEFGH%2FIJKLMNOP", EXP);
    assert!(parse_seed(&url).unwrap().is_some());
}

#[test]
fn invalid_characters_rejected() {
    // Url only exposes raw segments; control bytes and non-ASCII are
    // percent-encoded by the parser and stay literal ASCII, never decoded.
    // Spaces inside the raw segment cannot occur; an empty checksum segment fails.
    let url = Url::parse("https://example.invalid//001.jpg?expires=1234567890").unwrap();
    assert!(parse_seed(&url).is_err());
}

#[test]
fn invalid_expiry_rejected() {
    let c = "AB-CD_EF.GH12IJ+KLMNO~";
    for q in [
        "",
        "?expires=",
        "?expires=12a",
        "?expires=-1",
        "?expires=12345678901234567",
        "?expires=1&expires=2",
        "?other=1",
    ] {
        assert!(parse_seed(&u(c, q)).is_err(), "{q}");
    }
}

#[test]
fn restore_with_derived_seed_inverts_shuffle() {
    let seed = parse_seed(&u("AB-CD_EF.GH12IJ+KLMNO~", EXP))
        .unwrap()
        .unwrap();
    let (w, h) = (150u32, 100u32);
    let mut scrambled = RgbaImage::new(w, h);
    let cols = 3u32;
    // tile index n gets a unique colour
    let order = shuffle_order(6, &seed).unwrap();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(sorted, (0..6).collect::<Vec<_>>());
    // restore_image copies tiles[order[k]] to tiles[k]; original tile k sits at order[k].
    for (k, pos) in order.iter().enumerate() {
        let src = k;
        let dest = *pos;
        let (dx, dy) = ((dest as u32 % cols) * 50, (dest as u32 / cols) * 50);
        for y in 0..50 {
            for x in 0..50 {
                scrambled.put_pixel(dx + x, dy + y, Rgba([src as u8, 1, 2, 255]));
            }
        }
    }
    let restored = restore_image(scrambled, &seed).unwrap();
    for n in 0..6u32 {
        let p = restored.get_pixel((n % cols) * 50 + 1, (n / cols) * 50 + 1);
        assert_eq!(p.0[0] as u32, n);
    }
}

#[test]
fn errors_do_not_leak_checksum_or_expiry() {
    let err = parse_seed(&u("ABCDEFGHIJkLMNOPQRSTUV", EXP))
        .unwrap_err()
        .to_string();
    assert!(!err.contains("ABCDEFGHIJ") && !err.contains("1234567890"));
}
