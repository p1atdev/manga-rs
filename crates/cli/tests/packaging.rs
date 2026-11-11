#[test]
fn binary_distribution_notices_are_bundled_with_the_cli_crate() {
    let notice = include_str!("../THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/NOTICE.md");
    assert!(notice.contains("Independent JPEG Group"));
    assert!(include_str!("../THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/README.ijg").contains("IJG"));
    assert!(
        !include_bytes!("../THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/UPSTREAM-LICENSE.md").is_empty()
    );
    assert!(!include_bytes!("../THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/ZLIB-NOTICE.txt").is_empty());
    assert!(!include_bytes!("../THIRD_PARTY_NOTICES/mozjpeg-sys-2.2.3/CRATE-LICENSE").is_empty());
    assert!(include_str!("../THIRD_PARTY_NOTICES/getpiccoma/LICENSE").contains("MIT License"));
}
