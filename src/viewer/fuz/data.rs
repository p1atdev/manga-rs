use anyhow::{bail, Result};
use url::Url;
use web_manga_viewer::{
    viewer_page,
    web_manga_viewer_response::{viewer_data, ViewerData},
    WebMangaViewerResponse,
};

use crate::data::{MangaEpisode, MangaPage, ScrollDirection};

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

/// ComicFuz manga page
#[derive(Debug, Clone)]
pub enum Page {
    Image(ImagePage),
    WebView { url: String },
    Last,
    Extra(ExtraPage),
}

#[derive(Debug, Clone)]
pub struct ImagePage {
    index: usize,
    /// path for the image
    image_path: String,

    encryption_key: String,
    encryption_iv: String,

    image_width: u32,
    image_height: u32,
}

impl ImagePage {
    pub fn encryption_key(&self) -> &str {
        &self.encryption_key
    }

    pub fn encryption_iv(&self) -> &str {
        &self.encryption_iv
    }
}

#[derive(Debug, Clone)]
pub struct ExtraPage {
    id: u32,
    index: u32,
    slot_id: u32,
}

impl Page {
    pub fn new(page: web_manga_viewer::ViewerPage, index: usize) -> Self {
        match page.content.unwrap() {
            viewer_page::Content::Image(page) => {
                if page.is_extra_page() {
                    Page::Extra(ExtraPage {
                        id: page.extra_id(),
                        index: page.extra_index(),
                        slot_id: page.extra_slot_id(),
                    })
                } else {
                    Page::Image(ImagePage {
                        index,
                        image_path: page.image_url,
                        encryption_key: page.encryption_key.unwrap(),
                        encryption_iv: page.iv.unwrap(),
                        image_width: page.image_width,
                        image_height: page.image_height,
                    })
                }
            }
            viewer_page::Content::Webview(web_view) => Page::WebView { url: web_view.url },
            viewer_page::Content::LastPage(_) => Page::Last,
        }
    }

    pub fn image_path(&self) -> Result<String> {
        match self {
            Page::Image(ImagePage { image_path, .. }) => Ok(image_path.clone()),
            _ => bail!("Page is not an image"),
        }
    }
}

impl MangaPage for Page {
    fn index(&self) -> Result<usize> {
        match self {
            Page::Image(ImagePage { index, .. }) => Ok(*index),
            _ => bail!("Page is not an image"),
        }
    }

    fn is_image(&self) -> bool {
        match self {
            Page::Image(_) => true,
            _ => false,
        }
    }
}

/// ComicFuz manga episode
#[derive(Debug, Clone)]
pub struct Episode {
    id: String,
    index: usize,
    title: String,
    pages: Vec<Page>,
    scroll_direction: ScrollDirection,
}

impl From<WebMangaViewerResponse> for Episode {
    fn from(value: WebMangaViewerResponse) -> Self {
        let chapters: Vec<web_manga_viewer::Chapter> = value
            .chapters
            .into_iter()
            .flat_map(|group| group.chapters)
            .collect();
        let index = chapters
            .iter()
            .position(|c| c.chapter_id == value.chapter_id)
            .unwrap();
        let chapter = &chapters[index];

        let viewer_data = value.viewer_data.unwrap();
        let pages = &viewer_data
            .pages
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, page)| Page::new(page, i))
            .collect::<Vec<_>>();

        let scroll_direction = match &viewer_data.scroll_direction() {
            viewer_data::ScrollDirection::Left => ScrollDirection::RightToLeft,
            viewer_data::ScrollDirection::Right => ScrollDirection::LeftToRight,
            viewer_data::ScrollDirection::Vertical => ScrollDirection::TopToBottom,
            viewer_data::ScrollDirection::None => ScrollDirection::Unknown,
        };

        Self {
            id: chapter.chapter_id.to_string(),
            index,
            title: chapter.chapter_main_name.clone(),
            pages: pages.clone(),
            scroll_direction: scroll_direction,
        }
    }
}

impl MangaEpisode<Page> for Episode {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn index(&self) -> usize {
        self.index
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone())
    }

    fn pages(&self) -> Vec<Page> {
        self.pages.clone()
    }
}
