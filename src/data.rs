use anyhow::Result;
use chrono::Utc;
use url::Url;

/// A manga is a collection of series
pub trait MangaPage {
    /// Get the index of the page
    fn index(&self) -> Result<usize>;

    /// Get the url of the page
    fn url(&self) -> Result<Url>;

    /// Check if the page is an image
    fn is_image(&self) -> bool;
}

/// An episode is a single chapter or part of a series
pub trait MangaEpisode<P: MangaPage> {
    /// Get the id of the episode
    fn id(&self) -> String;

    /// Get the index of the episode
    fn index(&self) -> usize;

    /// Get the title of the episode
    fn title(&self) -> Option<String>;

    /// Get the date of the episode
    fn date(&self) -> Option<chrono::DateTime<Utc>>;

    /// Get the url of the episode
    fn url(&self) -> Url;

    /// Get the pages of the episode
    fn pages(&self) -> Vec<P>;
}

/// A series is a collection of episodes
pub trait MangaSeries<P: MangaPage, E: MangaEpisode<P>> {
    /// Get the id of the series
    fn id(&self) -> String;

    /// Get the title of the series
    fn title(&self) -> String;

    /// Get the author of the series
    fn author(&self) -> Option<String>;

    /// Get the description of the series
    fn description(&self) -> Option<String>;

    /// Get the url of the series
    fn url(&self) -> Option<Url>;

    /// Get the episodes of the series
    fn episodes(&self) -> Vec<E>;
}
