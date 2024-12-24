use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeNextData {
    props: Props,
    page: String,
    query: Query,
    build_id: String,
    is_fallback: bool,
    is_experimental_compile: bool,
    gsp: bool,
    script_loader: Vec<Option<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Props {
    page_props: PageProps,
    #[serde(rename = "__N_SSG")]
    n_ssg: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageProps {
    episode_id: String,
    meta: Meta,
    fallback_data: FallbackData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackData {
    comic_advertising_pages: Vec<Option<serde_json::Value>>,
    comic_description: String,
    comic_good_count: i64,
    comic_id: String,
    comic_nsfw_type: NsfwType,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    title: String,
    description: String,
    og_image_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    episode_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NsfwType {
    None,
    Any,
}
