use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    comic_advertising_pages: Vec<Option<serde_json::Value>>,
    comic_description: String,
    comic_good_count: i64,
    comic_id: String,
    comic_nsfw_type: String,
    comic_title: String,
    episode_expire_seconds: Option<serde_json::Value>,
    episode_id: String,
    episode_price: i64,
    episode_promotion: String,
    episode_status: String,
    episode_thumbnail_image_url: String,
    episode_title: String,
    is_first_view_spread: bool,
    page_direction: String,
    pages: Vec<Page>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    drm_hash: String,
    episode_id: String,
    id: String,
    page_image_url: String,
}
