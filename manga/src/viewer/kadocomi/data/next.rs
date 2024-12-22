use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeNextData {
    props: Props,
    page: String,
    query: EpisodeNextDataQuery,
    build_id: String,
    is_fallback: bool,
    is_experimental_compile: bool,
    gssp: bool,
    script_loader: Vec<Option<serde_json::Value>>,
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
    queries: Vec<QueryElement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryElement {
    state: State,
    query_key: Vec<QueryKeyElement>,
    query_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryKeyElement {
    QueryKeyClass(QueryKeyClass),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryKeyClass {
    work_code: Option<String>,
    label_id: Option<String>,
    episode_code: Option<serde_json::Value>,
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
    first_comic: Option<FirstComic>,
    latest_comic: Option<FirstComic>,
    first_episodes: Option<StEpisodes>,
    latest_episodes: Option<StEpisodes>,
    comics: Option<Comics>,
    promotions: Option<Vec<Option<serde_json::Value>>>,
    related_books: Option<RelatedBooks>,
    label: Option<Label>,
    labels: Option<Vec<Label>>,
    label_works: Option<Vec<LabelWork>>,
    latest_episode_id: Option<String>,
    follower_count: Option<i64>,
    episode: Option<DataEpisode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comics {
    total: i64,
    result: Vec<FirstComic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirstComic {
    id: String,
    title: String,
    thumbnail: String,
    release: String,
    episodes: Vec<EpisodeElement>,
    stores: Vec<Store>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeElement {
    id: String,
    code: String,
    title: String,
    sub_title: String,
    thumbnail: String,
    original_thumbnail: String,
    update_date: String,
    delivery_period: String,
    is_new: bool,
    has_read: bool,
    stores: Vec<Option<serde_json::Value>>,
    service_id: ServiceId,
    internal: EpisodeInternal,
    #[serde(rename = "type")]
    element_type: String,
    is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeInternal {
    episode_no: i64,
    page_count: i64,
    #[serde(rename = "episodetype")]
    episode_type: String,
    is_latest: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceId {
    Web,
    #[serde(rename = "web_trial")]
    WebTrial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Store {
    code: Code,
    name: Name,
    url: String,
    image: Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    Amazon,
    #[serde(rename = "book_live")]
    BookLive,
    #[serde(rename = "book_walker")]
    BookWalker,
    #[serde(rename = "comic_cmoa")]
    ComicCmoa,
    #[serde(rename = "ebook_japan")]
    EbookJapan,
    Honto,
    #[serde(rename = "line_manga")]
    LineManga,
    Mechacomic,
    Piccoma,
    Renta,
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
#[serde(rename_all = "snake_case")]
pub enum Name {
    #[serde(rename = "Amazon")]
    Amazon,
    #[serde(rename = "BookLive!")]
    BookLive,
    #[serde(rename = "BOOK☆WALKER")]
    BookWalker,
    Ebookjapan,
    #[serde(rename = "コミックシーモア")]
    Cmoa,
    Honto,
    #[serde(rename = "LINEマンガ")]
    LineManga,
    #[serde(rename = "めちゃコミック")]
    Mechacomic,
    #[serde(rename = "ピッコマ")]
    Piccoma,
    #[serde(rename = "Renta!")]
    Renta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEpisode {
    id: String,
    code: String,
    title: String,
    service_id: ServiceId,
    thumbnail: String,
    internal: EpisodeInternal,
    update_date: String,
    is_active: bool,
    original_thumbnail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StEpisodes {
    total: i64,
    result: Vec<EpisodeElement>,
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
    title: String,
    is_original: bool,
    language: String,
    serialization_status: String,
    service_ids: Vec<String>,
    internal: LabelWorkInternal,
    is_new: bool,
    episode: LabelWorkEpisode,
    book_cover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelWorkEpisode {
    #[serde(rename = "type")]
    start_type: String,
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
    resources: Vec<Resource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    id: String,
    title: String,
    thumbnail: String,
    stores: Vec<Store>,
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
    service_ids: Vec<ServiceId>,
    internal: WorkInternal,
    summary: String,
    genre: Genre,
    sub_genre: Genre,
    tags: Vec<Tag>,
    authors: Vec<Author>,
    follower_count: i64,
    is_new: bool,
    next_update_date_text: String,
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
pub struct EpisodeNextDataQuery {
    episode_type: Option<String>,
    work_code: String,
}
