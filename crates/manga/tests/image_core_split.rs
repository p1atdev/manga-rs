#![cfg(feature = "direct-sort")]

//! The standalone core and the network-facing crate must expose the same page type and behavior.
use manga::direct::{DirectErrorKind, DirectFormat, ProviderImage, process_provider_image_checked};

#[test]
fn core_and_manga_share_provider_facade_and_strict_validation() {
    let input = include_bytes!("assets/giga-tonari-yj-public-20261111.jpg");
    let from_core =
        manga_image::process_provider_image_checked(input, ProviderImage::Giga, None).unwrap();
    let from_manga = process_provider_image_checked(input, ProviderImage::Giga, None).unwrap();
    assert_eq!(from_core.format(), DirectFormat::Jpeg);
    assert_eq!(from_core.bytes(), from_manga.bytes());
    let error = manga_image::process_provider_image_checked(
        &input[..input.len() / 2],
        ProviderImage::Giga,
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind(), DirectErrorKind::Malformed);
}
