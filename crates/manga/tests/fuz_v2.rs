#![cfg(feature = "fuz")]
use manga::viewer::fuz::v2::{
    ViewerData, ViewerImage, ViewerPage, ViewerResponse, parse_free_chapter, parse_viewer,
};
use prost::Message;

#[test]
fn viewer_validates_pages_and_skips_advertisements() {
    let image = ViewerImage {
        image_url: "/image.enc".into(),
        iv: "11".repeat(16),
        encryption_key: "22".repeat(32),
        image_width: 100,
        image_height: 100,
        ..Default::default()
    };
    let response = |pages| ViewerResponse {
        viewer_data: Some(ViewerData { pages }),
        chapter_id: 1,
    };
    let page = ViewerPage {
        image: Some(image),
        ..Default::default()
    };
    let data = response(vec![
        page.clone(),
        ViewerPage {
            google_ads: Some(vec![]),
            ..Default::default()
        },
    ])
    .encode_to_vec();
    assert_eq!(parse_viewer(&data, 1).unwrap().len(), 1);
    assert!(parse_viewer(&data, 2).is_err());
    assert!(parse_viewer(&response(vec![ViewerPage::default()]).encode_to_vec(), 1).is_err());
    assert!(
        parse_viewer(
            &response(vec![ViewerPage {
                webview: Some(vec![]),
                ..Default::default()
            }])
            .encode_to_vec(),
            1
        )
        .is_err()
    );
    let mut unsafe_page = page;
    unsafe_page.image.as_mut().unwrap().image_url = "https://evil.example/x".into();
    assert!(parse_viewer(&response(vec![unsafe_page]).encode_to_vec(), 1).is_err());
}

#[test]
fn detail_confirms_only_explicitly_free_matching_chapter() {
    let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"manga":{"mangaId":42,"mangaName":"Title"},"chapters":[{"chapters":[{"chapterId":1,"chapterMainName":"free","pointConsumption":{}},{"chapterId":2,"chapterMainName":"paid","pointConsumption":{"amount":10}},{"chapterId":3,"chapterMainName":"unknown"}]}]}}}</script>"#;
    assert_eq!(
        parse_free_chapter(html, 42, 1).unwrap().as_deref(),
        Some("free")
    );
    assert_eq!(parse_free_chapter(html, 42, 2).unwrap(), None);
    assert_eq!(parse_free_chapter(html, 42, 3).unwrap(), None);
    assert!(parse_free_chapter(html, 43, 1).is_err());
    assert!(parse_free_chapter(html, 42, 99).is_err());
}

#[tokio::test]
#[ignore = "downloads a live guest-free FUZ chapter to a disposable directory"]
async fn live_pipeline_downloads_every_page() {
    use manga::{data::MangaEpisode, pipeline::EpisodePipeline, viewer::fuz::v2::Pipeline};
    struct TempPath(std::path::PathBuf);
    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let pipeline = Pipeline::for_work(4012).unwrap();
    let first = pipeline.fetch_episode("81187").await.unwrap();
    let id = first.id();
    let expected = first.pages().len();
    assert_eq!(
        pipeline.fetch_episode(&id).await.unwrap().pages().len(),
        expected
    );
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output =
        TempPath(std::env::temp_dir().join(format!("manga-fuz-{}-{unique}", std::process::id())));
    let url = url::Url::parse("https://comic-fuz.com/manga/viewer/81187").unwrap();
    pipeline.download(&url, &output.0).await.unwrap();
    let files = std::fs::read_dir(&output.0)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(files.len(), expected);
}

#[tokio::test]
async fn mismatched_work_and_malformed_episode_ids_fail_before_network() {
    use manga::{error::ClientError, pipeline::EpisodePipeline, viewer::fuz::v2::Pipeline};
    let pipeline = Pipeline::for_work(42).unwrap();
    for id in ["41/7", "042/7", "42/07", "42/0", "42/7/8", "0", "+7"] {
        assert!(
            matches!(
                pipeline.fetch_episode(id).await,
                Err(ClientError::InvalidUrl)
            ),
            "{id}"
        );
    }
}
