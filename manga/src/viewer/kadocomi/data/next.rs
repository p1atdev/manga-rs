use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeNextData {
    props: Props,
    page: String,
    query: NextDataQuery,
    build_id: String,
    is_fallback: bool,
    is_experimental_compile: bool,
    gssp: bool,
    script_loader: Vec<Option<serde_json::Value>>,
}

impl EpisodeNextData {
    pub fn episode(&self) -> Result<DataEpisode> {
        let episode = self
            .props
            .page_props
            .dehydrated_state
            .queries
            .iter()
            .find_map(|query| query.state.data.episode.clone())
            .context("Episode not found")?;
        Ok(episode)
    }

    pub fn series_id(&self) -> String {
        self.props.page_props.work_code.clone()
    }

    pub fn series_title(&self) -> String {
        self.props.page_props.metadata.title.clone()
    }

    pub fn episode_id(&self) -> Result<String> {
        Ok(self.episode()?.id)
    }

    pub fn episode_title(&self) -> Result<String> {
        Ok(self.episode()?.title)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Props {
    page_props: PageProps,
    #[serde(rename = "__N_SSP")]
    n_ssp: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageProps {
    work_id: String,
    work_code: String,
    episode_type: String,
    episode_code: Option<String>,
    dehydrated_state: DehydratedState,
    metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DehydratedState {
    mutations: Vec<Option<serde_json::Value>>,
    queries: Vec<EpisodeNextDataQuery>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeNextDataQuery {
    state: State,
    query_key: Vec<QueryKeyElement>,
    query_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryKeyElement {
    UrlPath(String),
    QueryKeyClass(QueryKeyClass),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryKeyClass {
    work_code: Option<String>,
    label_id: Option<String>,
    episode_code: Option<String>,
    episode_type: Option<String>,
    latest_episode_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    data: Data,
    data_update_count: i64,
    data_updated_at: i64,
    error: Option<serde_json::Value>,
    error_update_count: i64,
    error_updated_at: i64,
    fetch_failure_count: i64,
    fetch_failure_reason: Option<serde_json::Value>,
    fetch_meta: Option<serde_json::Value>,
    is_invalidated: bool,
    status: String,
    fetch_status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    work: Option<Work>,
    first_comic: Option<EdgeComic>,
    latest_comic: Option<EdgeComic>,
    first_episodes: Option<Comics>,
    latest_episodes: Option<Comics>,
    comics: Option<Comics>,
    promotions: Option<Vec<Option<serde_json::Value>>>,
    related_books: Option<RelatedBooks>,
    label: Option<Label>,
    labels: Option<Vec<Label>>,
    label_works: Option<Vec<LabelWork>>,
    follower_count: Option<i64>,
    episode: Option<DataEpisode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comics {
    total: i64,
    result: Vec<EdgeComic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeComic {
    id: String,
    title: String,
    thumbnail: String,
    release: Option<String>,
    episodes: Option<Vec<EpisodeElement>>,
    stores: Vec<Store>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeElement {
    id: String,
    code: String,
    title: String,
    sub_title: String,
    update_date: String,
    delivery_period: String,
    is_new: bool,
    has_read: bool,
    stores: Vec<Option<Store>>,
    service_id: String,
    internal: EpisodeInternal,
    #[serde(rename = "type")]
    episode_type: String,
    is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeInternal {
    episode_no: i64,
    page_count: i64,
    episodetype: String,
    is_latest: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Store {
    code: String,
    name: String,
    url: String,
    image: Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    src: String,
    height: i64,
    width: i64,
    #[serde(rename = "blurDataURL")]
    blur_data_url: String,
    blur_width: i64,
    blur_height: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEpisode {
    id: String,
    code: String,
    title: String,
    service_id: String,
    internal: EpisodeInternal,
    update_date: String,
    is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    id: Option<String>,
    name: String,
    description: String,
    code: String,
    color: String,
    icon_image_url: String,
    logo_image_url: String,
    cover_image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelWork {
    code: String,
    id: String,
    thumbnail: String,
    original_thumbnail: String,
    book_cover: Option<String>,
    title: String,
    is_original: bool,
    language: String,
    serialization_status: String,
    service_ids: Vec<String>,
    internal: LabelWorkInternal,
    is_new: bool,
    episode: LabelWorkEpisode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelWorkEpisode {
    #[serde(rename = "type")]
    episode_type: String,
    code: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelWorkInternal {
    label_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedBooks {
    total_count: i64,
    resources: Vec<Option<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    code: String,
    id: String,
    thumbnail: String,
    original_thumbnail: String,
    book_cover: String,
    title: String,
    is_original: bool,
    label_info: Label,
    language: String,
    serialization_status: String,
    service_ids: Vec<String>,
    internal: WorkInternal,
    summary: String,
    genre: Genre,
    sub_genre: Genre,
    tags: Vec<Tag>,
    authors: Vec<Author>,
    follower_count: i64,
    is_new: bool,
    is_one_shot: bool,
    rating_level: String,
    free_campaigns: FreeCampaigns,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Author {
    id: String,
    name: String,
    role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreeCampaigns {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Genre {
    code: String,
    id: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkInternal {
    department_code: String,
    scroll_type: String,
    label_names: Vec<String>,
    fair_ids: Vec<Option<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    id: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    title: String,
    description: String,
    ogp_image_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextDataQuery {
    work_code: String,
}

// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "snake_case")]
// pub enum Name {
//     #[serde(rename = "Amazon")]
//     Amazon,
//     #[serde(rename = "BookLive!")]
//     BookLive,
//     #[serde(rename = "BOOK☆WALKER")]
//     BookWalker,
//     Ebookjapan,
//     #[serde(rename = "コミックシーモア")]
//     Cmoa,
//     Honto,
//     #[serde(rename = "LINEマンガ")]
//     LineManga,
//     #[serde(rename = "めちゃコミック")]
//     Mechacomic,
//     #[serde(rename = "ピッコマ")]
//     Piccoma,
//     #[serde(rename = "Renta!")]
//     Renta,
// }
