#![cfg(feature = "yanmaga")]
use manga::viewer::yanmaga::decode_table;

#[test]
fn viewer_access_denial_is_distinct_from_invalid_or_failed_responses() {
    use manga::viewer::yanmaga::{ViewerAccessDenied, viewer_item};
    let full = serde_json::json!({"result":1,"items":[{"ContentID":"test","ViewMode":1}]});
    assert!(viewer_item(&full, "test").is_ok());
    let mut trial = full.clone();
    trial["items"][0]["ViewMode"] = serde_json::json!(2);
    assert!(
        viewer_item(&trial, "test")
            .unwrap_err()
            .downcast_ref::<ViewerAccessDenied>()
            .is_some()
    );
    let mut invalid = full.clone();
    invalid["items"][0]["ViewMode"] = serde_json::json!(null);
    assert!(
        viewer_item(&invalid, "test")
            .unwrap_err()
            .downcast_ref::<ViewerAccessDenied>()
            .is_none()
    );
    let failed = serde_json::json!({"result":-100,"items":[]});
    assert!(
        viewer_item(&failed, "test")
            .unwrap_err()
            .downcast_ref::<ViewerAccessDenied>()
            .is_none()
    );
}

#[test]
fn corrupted_scramble_tables_are_rejected() {
    assert!(decode_table("test", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "not a table").is_err());
}

#[test]
fn synthetic_official_js_table_vector_decodes_successfully() {
    let actual = decode_table(
        "synthetic-id",
        "AbCdEfGhIjKlMnOpQrStUvWxYz012345",
        "]Ry.2T{$-0;lQi}k z387-Y!Q3VnR5Pt(",
    )
    .unwrap();
    assert_eq!(actual, vec!["a", "b", "c", "d", "e", "f", "g", "h"]);
}

#[test]
fn type_f_restoration_removes_padding_and_preserves_pixels() {
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let data = format!("HHHHHHHHHHHHHHHH{alphabet}");
    let src = format!("=8-8-4-{data}");
    let dst = format!("=8-8+4-{data}");
    let mut expected = image::RgbaImage::new(328, 328);
    for (x, y, p) in expected.enumerate_pixels_mut() {
        *p = image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]);
    }
    let mut padded = image::RgbaImage::from_pixel(392, 392, image::Rgba([255, 0, 255, 255]));
    for row in 0..8 {
        for col in 0..8 {
            for y in 0..41 {
                for x in 0..41 {
                    padded.put_pixel(
                        4 + col * 49 + x,
                        4 + row * 49 + y,
                        *expected.get_pixel(col * 41 + x, row * 41 + y),
                    );
                }
            }
        }
    }
    assert_eq!(
        manga::viewer::yanmaga::restore_image(padded, &src, &dst).unwrap(),
        expected
    );
}

#[test]
fn matching_type_f_permutations_restore_pixels_and_invalid_keys_fail() {
    use image::{Rgba, RgbaImage};
    let mut img = RgbaImage::new(328, 328);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]);
    }
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let data = format!("HHHHHHHHHHHHHHHH{alphabet}");
    let src = format!("=8-8-0-{data}");
    let dst = format!("=8-8+0-{data}");
    assert_eq!(
        manga::viewer::yanmaga::restore_image(img.clone(), &src, &dst).unwrap(),
        img
    );
    let shifted = format!(
        "=8-8+0-HHHHHHHHHHHHHHHH{}{}",
        &alphabet[1..],
        &alphabet[..1]
    );
    let out = manga::viewer::yanmaga::restore_image(img.clone(), &src, &shifted).unwrap();
    assert_eq!(out.get_pixel(41, 0), img.get_pixel(0, 0));
    assert_eq!(out.get_pixel(0, 41), img.get_pixel(287, 0));
    assert!(manga::viewer::yanmaga::restore_image(img, &src, "invalid").is_err());
}

#[test]
fn episode_url_accepts_only_official_work_and_episode_paths() {
    use manga::viewer::yanmaga::episode_url;
    use url::Url;
    let valid = "https://yanmaga.jp/comics/test/0123456789abcdef0123456789abcdef";
    assert!(episode_url(&Url::parse(valid).unwrap()).is_ok());
    for bad in [
        "http://yanmaga.jp/comics/test/0123456789abcdef0123456789abcdef",
        "https://evil.example/comics/test/0123456789abcdef0123456789abcdef",
        "https://yanmaga.jp/comics/test/0123456789abcdef0123456789abcdef?x=1",
        "https://yanmaga.jp/comics/test/short",
        "https://yanmaga.jp/comics/test%2Fother/0123456789abcdef0123456789abcdef",
    ] {
        assert!(episode_url(&Url::parse(bad).unwrap()).is_err());
    }
}

#[tokio::test]
#[ignore = "accesses a live anonymous YanMaga episode"]
async fn live_guest_episode_metadata_and_page_are_usable() {
    use manga::{
        data::{MangaEpisode, MangaPage},
        pipeline::EpisodePipeline,
        viewer::yanmaga::Pipeline,
    };
    use scraper::{Html, Selector};
    let html = reqwest::get("https://yanmaga.jp/comics/%E5%85%AC%E5%BC%8F%E4%B8%8D%E5%80%AB")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let episode = {
        let doc = Html::parse_document(&html);
        let selector = Selector::parse("li.mod-episode-item").unwrap();
        doc.select(&selector)
            .find(|item| {
                item.value()
                    .attr("data-episode-title")
                    .is_some_and(|title| title.starts_with("第１話"))
                    && !item.value().classes().any(|c| c == "js-modal")
            })
            .and_then(|item| item.value().attr("data-original-url"))
            .map(str::to_owned)
            .unwrap()
    };
    let url = url::Url::parse("https://yanmaga.jp/")
        .unwrap()
        .join(&episode)
        .unwrap();
    let pipeline = Pipeline::new().unwrap();
    let episode = pipeline.fetch_episode(url.as_str()).await.unwrap();
    let pages = episode.into_pages();
    assert!(!pages.is_empty());
    assert_eq!(pages[0].index().unwrap(), 0);
    let bytes = pipeline.fetch_image(&pages[0]).await.unwrap();
    let image = pipeline
        .solve_image(bytes, pages.into_iter().next())
        .await
        .unwrap();
    assert!(image.width() > 0 && image.height() > 0);
}

#[tokio::test]
#[ignore = "downloads a live anonymous YanMaga episode to a disposable directory"]
async fn live_pipeline_downloads_every_page() {
    use manga::{data::MangaEpisode, pipeline::EpisodePipeline, viewer::yanmaga::Pipeline};
    use scraper::{Html, Selector};
    struct TempPath(std::path::PathBuf);
    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let html = reqwest::get("https://yanmaga.jp/comics/%E5%85%AC%E5%BC%8F%E4%B8%8D%E5%80%AB")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let episode = {
        let doc = Html::parse_document(&html);
        let selector = Selector::parse("li.mod-episode-item").unwrap();
        doc.select(&selector)
            .find(|item| {
                item.value()
                    .attr("data-episode-title")
                    .is_some_and(|title| title.starts_with("第１話"))
                    && !item.value().classes().any(|c| c == "js-modal")
            })
            .and_then(|item| item.value().attr("data-original-url"))
            .map(str::to_owned)
            .unwrap()
    };
    let url = url::Url::parse("https://yanmaga.jp/")
        .unwrap()
        .join(&episode)
        .unwrap();
    let pipeline = Pipeline::new().unwrap();
    let expected = pipeline
        .fetch_episode(url.as_str())
        .await
        .unwrap()
        .pages()
        .len();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = TempPath(
        std::env::temp_dir().join(format!("manga-yanmaga-{}-{unique}", std::process::id())),
    );
    pipeline.download(&url, &output.0).await.unwrap();
    let files = std::fs::read_dir(&output.0)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(files.len(), expected);
}

#[test]
fn nondivisible_dimensions_reject_mapped_tiles_with_different_sizes() {
    use manga::viewer::yanmaga::restore_image;
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let source = format!("=8-8-0-HHHHHHHHHHHHHHHH{alphabet}");
    // The identity mapping must not move the short edge to a different column.
    let mismatched_width = format!("=8-8+0-HHHHHHHHGGGGGGGG{alphabet}");
    let mismatched_height = format!("=8-8+0-GGGGGGGGHHHHHHHH{alphabet}");
    assert!(restore_image(image::RgbaImage::new(329, 328), &source, &mismatched_width).is_err());
    assert!(restore_image(image::RgbaImage::new(328, 329), &source, &mismatched_height).is_err());
    // When dimensions divide evenly these two keys describe equal-sized tiles.
    assert!(restore_image(image::RgbaImage::new(328, 328), &source, &mismatched_width).is_ok());
}
