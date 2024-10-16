use std::sync::LazyLock;

use regex::Regex;

use crate::viewer::ViewerWebsite;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Website {
    Kadokomi,
}

static HOST_TO_WEBSITE: phf::Map<&str, Website> = phf::phf_map! {
    "comic-walker.com" => Website::Kadokomi,
};

static EPISODE_PATH_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"/detail/(\d+)&"#).unwrap());

impl ViewerWebsite<Website> for Website {
    fn host(&self) -> &str {
        match &self {
            Website::Kadokomi => "comic-walker.com",
        }
    }

    fn base_url(&self) -> url::Url {
        let url = match &self {
            Website::Kadokomi => "https://comic-walker.com",
        };
        url::Url::parse(url).unwrap()
    }

    fn lookup(host: &str) -> Option<Website> {
        HOST_TO_WEBSITE.get(host).map(|w| *w)
    }
}

impl Website {
    pub async fn get_viewer_contents(self, episode_id: &str, image_size_type: &str) {}
}
