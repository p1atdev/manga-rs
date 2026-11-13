#![cfg(all(feature = "giga", feature = "direct-sort"))]

use manga::{
    solver::ImageSolver,
    viewer::giga::{direct_sort::sort_jpeg, solver::Solver},
};

#[test]
fn original_grayscale_pages_match_every_decoded_pixel() {
    for input in [
        include_bytes!("assets/jumpplus-page2-raw.jpg").as_slice(),
        include_bytes!("assets/kuragebunch-page5-raw.jpg").as_slice(),
    ] {
        let expected = Solver::new()
            .solve_from_bytes(input.to_vec())
            .unwrap()
            .into_rgb8();
        let actual = image::load_from_memory(&sort_jpeg(input).unwrap())
            .unwrap()
            .into_rgb8();
        assert_eq!(actual.dimensions(), expected.dimensions());
        assert_eq!(actual.as_raw(), expected.as_raw());
    }
}
