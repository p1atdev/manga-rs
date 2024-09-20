pub mod web_manga_viewer {
    use device_info::{DeviceType, ImageQuality};
    use web_manga_viewer_request::ChapterInterface;

    use crate::utils;
    utils::include_proto!("fuz.web_manga_viewer");

    impl DeviceInfo {
        pub fn web_pc() -> Self {
            Self {
                secret: "".to_string(),
                app_ver: "".to_string(),
                device_type: DeviceType::Browser.into(),
                os_ver: "".to_string(),
                is_tablet: false,
                image_quality: ImageQuality::High.into(),
            }
        }
    }

    impl UserPoint {
        pub fn empty() -> Self {
            Self { free: 0, paid: 0 }
        }
    }

    impl WebMangaViewerRequest {
        pub fn free_chapter_id(chapter_id: u32) -> Self {
            Self {
                device_info: Some(DeviceInfo::web_pc()),
                use_ticket: false,
                consume_point: Some(UserPoint::empty()),
                chapter_interface: Some(ChapterInterface::ChapterId(chapter_id)),
            }
        }
    }
}

pub struct Episode {}
