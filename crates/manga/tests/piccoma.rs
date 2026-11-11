#![cfg(feature = "piccoma")]

use image::{Rgba, RgbaImage};
use manga::viewer::piccoma::{
    parse_free_episodes, parse_guest_viewer, parse_seed, restore_image, shuffle_order,
};
use url::Url;

#[test]
fn free_listing_excludes_waiting_and_paid_episodes() {
    let html = r#"<ul id="js_episodeList"><li><a data-product_id="123" data-episode_id="10"><div class="PCM-epList_title"><h2>Free</h2></div><div class="PCM-epList_status"><p class="PCM-epList_status_free">¥0</p></div></a></li><li><a data-product_id="123" data-episode_id="11"><div class="PCM-epList_title"><h2>Wait</h2></div><div class="PCM-epList_status"><p class="PCM-epList_status_waitfree">¥0</p></div></a></li></ul>"#;
    let episodes = parse_free_episodes(html, "123").unwrap();
    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].id, "10");
    assert!(parse_free_episodes("<html></html>", "123").is_err());
}

#[test]
fn synthetic_viewer_validates_identity_and_image_origin() {
    let html = "<script>var _pdata_ = {'product_id':123,'episode_id':10,'isScrambled':true,'img': [\n{'path':'https://pcm.kakaocdn.net/path/ABCDEF0123/file.jpg?expires=123','width':100,'height':150},\n],}; var _rcm_ = {};</script>";
    let viewer = parse_guest_viewer(html, "123", "10").unwrap();
    assert!(viewer.scrambled);
    assert_eq!((viewer.pages[0].width, viewer.pages[0].height), (100, 150));
    assert!(parse_guest_viewer(html, "123", "11").is_err());
    assert!(
        parse_guest_viewer(
            &html.replace("pcm.kakaocdn.net", "evil.example"),
            "123",
            "10"
        )
        .is_err()
    );
}

#[test]
fn shuffle_and_restoration_match_known_viewer_order_and_preserve_edges() {
    assert_eq!(
        shuffle_order(12, "EXAMPLESEED").unwrap(),
        vec![3, 7, 6, 5, 11, 1, 8, 2, 9, 10, 0, 4]
    );
    assert!(shuffle_order(1, "").is_err());
    let image = RgbaImage::from_fn(213, 163, |x, y| {
        Rgba([(y / 50 * 5 + x / 50) as u8, 0, 0, 255])
    });
    let result = restore_image(image.clone(), "EXAMPLESEED").unwrap();
    for (destination, source) in shuffle_order(12, "EXAMPLESEED")
        .unwrap()
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            result.get_pixel((destination % 4 * 50) as u32, (destination / 4 * 50) as u32),
            image.get_pixel((source % 4 * 50) as u32, (source / 4 * 50) as u32)
        );
    }
    assert_eq!(result.dimensions(), image.dimensions());
}

#[test]
fn seed_parsing_rejects_invalid_expiry_without_disclosing_url() {
    let url = Url::parse("https://pcm.kakaocdn.net/path/CHECKSUM/file.jpg?sig=SECRET_TEST_VALUE")
        .unwrap();
    assert!(!format!("{:#}", parse_seed(&url).unwrap_err()).contains("SECRET_TEST_VALUE"));
    assert!(
        parse_seed(
            &Url::parse("https://pcm.kakaocdn.net/path/CHECKSUM/file.jpg?expires=bad").unwrap()
        )
        .is_err()
    );
    assert!(
        parse_seed(
            &Url::parse("https://pcm.kakaocdn.net/path/ABC0123456789XYZ/file.jpg?expires=123")
                .unwrap()
        )
        .unwrap()
        .is_some()
    );
}

#[test]
fn episode_url_requires_exact_official_origin_and_numeric_ids() {
    use manga::viewer::piccoma::episode_ids;
    assert_eq!(
        episode_ids(&Url::parse("https://piccoma.com/web/viewer/123/456").unwrap()).unwrap(),
        ("123".to_string(), "456".to_string())
    );
    for bad in [
        "https://evil.example/web/viewer/123/456",
        "https://piccoma.com/web/viewer/123/456/extra",
        "https://piccoma.com/web/viewer/123/456?x=1",
        "http://piccoma.com/web/viewer/123/456",
    ] {
        assert!(episode_ids(&Url::parse(bad).unwrap()).is_err());
    }
}

#[test]
fn page_decode_checks_dimensions_and_handles_unscrambled_pages() {
    use manga::viewer::piccoma::{ViewerPage, solve_page};
    let image = image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 255])));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let page = ViewerPage {
        url: Url::parse("https://pcm.kakaocdn.net/path/abcdef0123/file.jpg?expires=123").unwrap(),
        width: 3,
        height: 2,
    };
    assert_eq!(
        solve_page(&bytes.into_inner(), &page, false)
            .unwrap()
            .to_rgba8(),
        image.to_rgba8()
    );
    let mut mismatch = page;
    mismatch.width = 4;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    assert!(solve_page(&bytes.into_inner(), &mismatch, false).is_err());
}

#[tokio::test]
async fn pipeline_solves_plain_page_with_correct_metadata() {
    use manga::{
        data::{MangaEpisode, MangaPage},
        pipeline::EpisodePipeline,
        viewer::piccoma::{Episode, GuestViewer, Pipeline, ViewerPage},
    };
    let viewer = GuestViewer {
        scrambled: false,
        pages: vec![ViewerPage {
            url: Url::parse("https://pcm.kakaocdn.net/path/abcdef0123/file.jpg?expires=123")
                .unwrap(),
            width: 2,
            height: 2,
        }],
    };
    let episode = Episode::from_viewer("123/10".into(), "Free".into(), viewer);
    assert_eq!(episode.id(), "123/10");
    assert_eq!(
        episode.index(),
        manga::data::IndexNumber::String("123/10".into())
    );
    assert_eq!(episode.pages()[0].index().unwrap(), 0);
    let mut encoded = std::io::Cursor::new(Vec::new());
    let original =
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([3, 4, 5, 255])));
    original
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let pipeline = Pipeline::new().unwrap();
    let actual = pipeline
        .solve_image(encoded.into_inner(), Some(episode.into_pages().remove(0)))
        .await
        .unwrap();
    assert_eq!(actual.to_rgba8(), original.to_rgba8());
}

#[tokio::test]
#[ignore = "accesses live public Piccoma pages"]
async fn live_guest_episode_page_decodes() {
    use manga::viewer::piccoma::{Client, solve_page};
    let client = Client::new().unwrap();
    let url = Url::parse("https://piccoma.com/web/viewer/184067/5351609").unwrap();
    let (_, viewer) = client.get_episode(&url).await.unwrap();
    let page = &viewer.pages[2];
    let bytes = client.fetch_page(page).await.unwrap();
    let image = solve_page(&bytes, page, viewer.scrambled).unwrap();
    assert_eq!((image.width(), image.height()), (page.width, page.height));
}

#[tokio::test]
#[ignore = "downloads a live guest-free Piccoma episode to a disposable directory"]
async fn live_pipeline_downloads_guest_episode() {
    use manga::{
        pipeline::EpisodePipeline,
        viewer::piccoma::{Client, Pipeline},
    };
    struct TempPath(std::path::PathBuf);
    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = TempPath(
        std::env::temp_dir().join(format!("manga-piccoma-{}-{unique}", std::process::id())),
    );
    let url = Url::parse("https://piccoma.com/web/viewer/184067/5351609").unwrap();
    let (_, viewer) = Client::new().unwrap().get_episode(&url).await.unwrap();
    let expected_count = viewer.pages.len();
    Pipeline::new()
        .unwrap()
        .download(&url, &output.0)
        .await
        .unwrap();
    let files = std::fs::read_dir(&output.0)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(files.len(), expected_count);
}
