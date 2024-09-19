use anyhow::Result;

use reqwest::header::{HeaderMap, HeaderValue};
use url::Url;

use crate::auth::EmptyAuth;
use crate::utils;
use crate::viewer::giga::data::Episode;
use crate::viewer::{ViewerClient, ViewerConfig, ViewerConfigBuilder, ViewerWebsite};

/// Preset websites of GigaViewer
pub enum Website {
    ShonenJumpPlus,
    TonarinoYJ,
    HerosWeb,
    ComicBushi,
    ComicBorder,
    ComicDays,
    ComicAction,
    ComicOgyaaa,
    ComicGardo,
    ComicZenon,
    Feelweb,
    Kuragebunch,
    SundayWebry,
    Magcomi,
}

impl ViewerWebsite for Website {
    fn base_url(&self) -> Url {
        let url = match &self {
            Website::ShonenJumpPlus => "https://shonenjumpplus.com",
            Website::TonarinoYJ => "https://tonarinoyj.jp",
            Website::HerosWeb => "https://viewer.heros-web.com",
            Website::ComicBushi => "https://comicbushi-web.com",
            Website::ComicBorder => "https://comicborder.com",
            Website::ComicDays => "https://comic-days.com",
            Website::ComicAction => "https://comic-action.com",
            Website::ComicOgyaaa => "https://comic-ogyaaa.com",
            Website::ComicGardo => "https://comic-gardo.com",
            Website::ComicZenon => "https://comic-zenon.com",
            Website::Feelweb => "https://feelweb.jp",
            Website::Kuragebunch => "https://kuragebunch.com",
            Website::SundayWebry => "https://www.sunday-webry.com",
            Website::Magcomi => "https://magcomi.com",
        };
        Url::parse(url).unwrap()
    }
}

/// viewer config
#[derive(Debug, Clone)]
pub struct Config {
    base_url: Url,
}

impl ViewerConfig for Config {
    fn create_header(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "User-Agent",
            HeaderValue::from_str(&utils::UserAgent::Bot.value())?,
        );
        Ok(headers)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    base_url: Url,
    auth: Option<EmptyAuth>,
}

impl ConfigBuilder {
    /// Create a new ConfigBuilder from preset
    pub fn new(website: Website) -> Self {
        Self {
            base_url: website.base_url(),
            auth: None,
        }
    }

    /// Create a new ConfigBuilder from custom url
    pub fn custom(url: String) -> Result<Self> {
        Ok(Self {
            base_url: Url::parse(&url)?,
            auth: None,
        })
    }
}

impl ViewerConfigBuilder<Config, EmptyAuth> for ConfigBuilder {
    fn set_auth(&mut self, auth: EmptyAuth) -> &mut Self {
        self.auth = Some(auth);
        self
    }

    fn build(&self) -> Config {
        Config {
            base_url: self.base_url.clone(),
        }
    }
}

/// ChojuGiga viewer client
pub struct Client {
    client: reqwest::Client,
    config: Config,
}

impl ViewerClient<Config> for Client {
    fn new(config: Config) -> Self {
        let client = reqwest::Client::new();
        Self { client, config }
    }

    async fn fetch_raw(&self, url: &str, method: reqwest::Method) -> Result<String> {
        let headers = self.config.create_header()?;
        let res = self
            .client
            .request(method, url)
            .headers(headers)
            .send()
            .await?
            .error_for_status()?;
        Ok(res.text().await?)
    }
}

impl Client {
    fn compose_episode_url(&self, episode_id: &str) -> Url {
        self.config
            .base_url
            .join(&format!("/episode/{}.json", episode_id))
            .unwrap()
    }

    /// Get episode
    pub async fn get_episode(&self, episode_id: &str) -> Result<Episode> {
        let url = self.compose_episode_url(episode_id);
        let res = self.fetch_raw(url.as_str(), reqwest::Method::GET).await?;
        let episode: Episode = serde_json::from_str(&res)?;
        Ok(episode)
    }
}

#[cfg(test)]
mod test {
    use crate::data::{MangaEpisode, MangaPage};

    use super::*;

    #[tokio::test]
    async fn test_get_episode() {
        let episode_ids = vec!["9324103625676410700"];

        for &episode_id in episode_ids.iter() {
            let config = ConfigBuilder::new(Website::ShonenJumpPlus).build();
            let client = Client::new(config);
            let episode = client.get_episode(episode_id).await.unwrap();
            assert_eq!(episode.id(), episode_id);
            assert!(episode.pages().len() > 0);

            let page = episode.pages();

            for p in page {
                let index = p.index().unwrap();
                let url = p.url().unwrap();
                println!("{}: {}", index, url);
            }
        }
    }
}
