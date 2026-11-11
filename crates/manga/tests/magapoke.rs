#![cfg(feature = "magapoke")]

#[test]
fn official_wasm_tile_mapping_is_reproduced_for_both_seed_alphabets() {
    use manga::viewer::magapoke::tile_permutation;
    assert_eq!(
        tile_permutation(1379, 320763, "q").unwrap(),
        [11, 7, 14, 9, 2, 12, 1, 0, 4, 5, 8, 15, 3, 10, 6, 13]
    );
    assert_eq!(
        tile_permutation(1380, 320763, "s").unwrap(),
        [10, 14, 7, 8, 0, 1, 11, 5, 6, 3, 12, 15, 2, 9, 4, 13]
    );
    assert_eq!(
        tile_permutation(1379, 320763, "q6jtf2xnog").unwrap(),
        [6, 11, 3, 2, 12, 15, 14, 5, 0, 8, 10, 1, 13, 9, 7, 4]
    );
    assert_eq!(
        tile_permutation(1380, 320763, "svdk0m7acl").unwrap(),
        [11, 6, 10, 3, 9, 4, 14, 0, 15, 1, 13, 5, 7, 12, 8, 2]
    );
    assert!(tile_permutation(1379, 320763, "!").is_err());
    assert!(tile_permutation(1380, 320763, "q").is_err());
}

#[test]
fn image_restoration_preserves_outer_remainder_and_moves_tiles() {
    use image::{Rgba, RgbaImage};
    use manga::viewer::magapoke::restore_image;
    let mut img = RgbaImage::new(70, 70);
    for y in 0..70 {
        for x in 0..70 {
            img.put_pixel(
                x,
                y,
                Rgba([
                    if x < 64 && y < 64 {
                        ((y / 16) * 4 + x / 16) as u8
                    } else {
                        255
                    },
                    0,
                    0,
                    255,
                ]),
            );
        }
    }
    let out = restore_image(img, 1379, 320763, "q").unwrap();
    assert_eq!(out.get_pixel(0, 0).0[0], 11);
    assert_eq!(out.get_pixel(16, 0).0[0], 7);
    assert_eq!(out.get_pixel(69, 69).0[0], 255);
}

#[test]
fn viewer_request_signature_matches_observed_public_client() {
    assert_eq!(
        manga::viewer::magapoke::viewer_request_hash(320763),
        "6e198e7dbc9e78f42eb3865060b50c75c67a624d46c111823439b6d217afe52fce20f7250a24bc9233fe0dc76001af360f6eeb6612fb3f4f1719984b3b31cf27"
    );
}

#[test]
fn viewer_rejects_wrong_identity_and_foreign_image_hosts() {
    let good = r#"{"status":"success","response_code":0,"title_id":1379,"episode_id":320763,"scramble_seed":"q","page_list":["https://mgpk-cdn.magazinepocket.com/a.jpg"]}"#;
    assert!(manga::viewer::magapoke::parse_viewer(good, 1379, 320763).is_ok());
    assert!(manga::viewer::magapoke::parse_viewer(good, 1379, 10).is_err());
    assert!(
        manga::viewer::magapoke::parse_viewer(
            &good.replace("mgpk-cdn.magazinepocket.com", "evil.test"),
            1379,
            320763
        )
        .is_err()
    );
    assert!(
        manga::viewer::magapoke::parse_viewer(
            &good.replace("\"success\"", "\"error\""),
            1379,
            320763
        )
        .is_err()
    );
}

#[test]
fn episode_detail_api_requires_explicit_matching_unauthenticated_free_flags() {
    use manga::viewer::magapoke::parse_episode_detail;
    let free = r#"{"status":"success","response_code":0,"episode":{"title_id":1379,"episode_id":320763,"point":0,"is_page_visible":1,"ticket_rental_enabled":0,"is_available_by_subscribe":0}}"#;
    assert!(parse_episode_detail(free, 1379, 320763).unwrap());
    assert!(
        !parse_episode_detail(&free.replace("\"point\":0", "\"point\":90"), 1379, 320763).unwrap()
    );
    assert!(
        !parse_episode_detail(
            &free.replace("\"ticket_rental_enabled\":0", "\"ticket_rental_enabled\":1"),
            1379,
            320763
        )
        .unwrap()
    );
    assert!(
        !parse_episode_detail(
            &free.replace(
                "\"is_available_by_subscribe\":0",
                "\"is_available_by_subscribe\":1"
            ),
            1379,
            320763
        )
        .unwrap()
    );
    assert!(
        parse_episode_detail(&free.replace("\"is_page_visible\":1,", ""), 1379, 320763).is_err()
    );
    assert!(parse_episode_detail(free, 1379, 320764).is_err());
    assert!(parse_episode_detail(&free.replace("\"success\"", "\"error\""), 1379, 320763).is_err());
}

#[test]
fn episode_url_requires_official_host_and_canonical_numeric_ids() {
    use manga::viewer::magapoke::episode_ids;
    let valid =
        url::Url::parse("https://pocket.shonenmagazine.com/title/01379/episode/320763").unwrap();
    assert_eq!(episode_ids(&valid).unwrap(), (1379, 320763));
    for bad in [
        "http://pocket.shonenmagazine.com/title/01379/episode/320763",
        "https://evil.example/title/01379/episode/320763",
        "https://pocket.shonenmagazine.com/title/1379/episode/320763",
        "https://pocket.shonenmagazine.com/title/01379/episode/0320763",
        "https://pocket.shonenmagazine.com/title/01379/episode/320763?x=1",
    ] {
        assert!(episode_ids(&url::Url::parse(bad).unwrap()).is_err());
    }
}

#[tokio::test]
async fn pipeline_preserves_identity_and_restores_page() {
    use manga::{
        data::{IndexNumber, MangaEpisode, MangaPage},
        pipeline::EpisodePipeline,
        viewer::magapoke::{Episode, Pipeline, parse_viewer},
    };
    let json = r#"{"status":"success","response_code":0,"title_id":1379,"episode_id":320763,"scramble_seed":"q","page_list":["https://mgpk-cdn.magazinepocket.com/a.jpg"]}"#;
    let episode = Episode::from_viewer(parse_viewer(json, 1379, 320763).unwrap());
    assert_eq!(episode.index(), IndexNumber::String("1379/320763".into()));
    assert_eq!(episode.pages()[0].index().unwrap(), 0);
    let image = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([((y / 16) * 4 + x / 16) as u8, 0, 0, 255])
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let decoded = Pipeline::new()
        .unwrap()
        .solve_image(bytes.into_inner(), episode.into_pages().into_iter().next())
        .await
        .unwrap()
        .to_rgba8();
    assert_eq!(decoded.get_pixel(0, 0).0[0], 11);
}

#[tokio::test]
#[ignore = "accesses a live guest-free Magazine Pocket episode"]
async fn live_guest_episode_and_one_restored_page() {
    use manga::{data::MangaEpisode, pipeline::EpisodePipeline, viewer::magapoke::Pipeline};
    let pipeline = Pipeline::new().unwrap();
    let episode = pipeline.fetch_episode("1379/320763").await.unwrap();
    let pages = episode.into_pages();
    assert!(!pages.is_empty());
    let bytes = pipeline.fetch_image(&pages[0]).await.unwrap();
    let image = pipeline
        .solve_image(bytes, pages.into_iter().next())
        .await
        .unwrap();
    assert!(image.width() > 0 && image.height() > 0);
}

#[tokio::test]
#[ignore = "downloads a live guest-free Magazine Pocket episode to a disposable directory"]
async fn live_pipeline_downloads_every_page() {
    use manga::{data::MangaEpisode, pipeline::EpisodePipeline, viewer::magapoke::Pipeline};
    struct TempPath(std::path::PathBuf);
    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let pipeline = Pipeline::new().unwrap();
    let expected = pipeline
        .fetch_episode("1379/320763")
        .await
        .unwrap()
        .pages()
        .len();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output = TempPath(
        std::env::temp_dir().join(format!("manga-magapoke-{}-{unique}", std::process::id())),
    );
    let url =
        url::Url::parse("https://pocket.shonenmagazine.com/title/01379/episode/320763").unwrap();
    pipeline.download(&url, &output.0).await.unwrap();
    let files = std::fs::read_dir(&output.0)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(files.len(), expected);
}

#[tokio::test]
async fn pipeline_rejects_noncanonical_episode_ids_before_network() {
    use manga::{error::ClientError, pipeline::EpisodePipeline, viewer::magapoke::Pipeline};
    let pipeline = Pipeline::new().unwrap();
    for id in ["01379/320763", "1379/0320763", "+1379/320763", "1379/0"] {
        assert!(matches!(
            pipeline.fetch_episode(id).await,
            Err(ClientError::InvalidUrl)
        ));
    }
}

#[test]
fn transparent_or_partially_transparent_pixels_are_rejected_including_remainder() {
    use manga::viewer::magapoke::{decode_image, restore_image};
    for (x, y, alpha) in [(0, 0, 0), (16, 0, 128), (69, 69, 0), (69, 0, 128)] {
        let mut input = image::RgbaImage::from_pixel(70, 70, image::Rgba([200, 40, 80, 255]));
        input.put_pixel(x, y, image::Rgba([200, 40, 80, alpha]));
        assert!(restore_image(input.clone(), 1379, 320763, "q").is_err());
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(input)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        assert!(decode_image(encoded.get_ref()).is_err());
    }
    let opaque = image::RgbaImage::from_pixel(70, 70, image::Rgba([200, 40, 80, 255]));
    assert!(restore_image(opaque, 1379, 320763, "q").is_ok());
}
