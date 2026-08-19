use anyhow::{Context, Result};
use serde::Deserialize;

/// Stable subset of Kadokomi's Next.js payload used by the downloader.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeNextData {
    props: Props,
}

impl EpisodeNextData {
    fn episode(&self) -> Result<&DataEpisode> {
        self.props
            .page_props
            .dehydrated_state
            .queries
            .iter()
            .find_map(|query| query.state.data.episode.as_ref())
            .context("episode not found in Kadokomi Next.js payload")
    }

    pub fn series_id(&self) -> &str {
        &self.props.page_props.work_code
    }

    pub fn series_title(&self) -> &str {
        &self.props.page_props.metadata.title
    }

    pub fn episode_id(&self) -> Result<String> {
        Ok(self.episode()?.id.clone())
    }

    pub fn episode_title(&self) -> Result<String> {
        Ok(self.episode()?.title.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Props {
    page_props: PageProps,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageProps {
    work_code: String,
    dehydrated_state: DehydratedState,
    metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct DehydratedState {
    queries: Vec<Query>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Query {
    state: State,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct State {
    data: Data,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Data {
    episode: Option<DataEpisode>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct DataEpisode {
    id: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Metadata {
    title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_the_required_next_data_fields() {
        let json = r#"{
          "props":{"pageProps":{
            "workCode":"work-1",
            "metadata":{"title":"Series"},
            "dehydratedState":{"queries":[
              {"state":{"data":{"episode":{"id":"episode-1","title":"Episode 1"}}}}
            ]}
          }},
          "unrelated":"ignored"
        }"#;

        let data: EpisodeNextData = serde_json::from_str(json).unwrap();
        assert_eq!(data.series_id(), "work-1");
        assert_eq!(data.series_title(), "Series");
        assert_eq!(data.episode_id().unwrap(), "episode-1");
        assert_eq!(data.episode_title().unwrap(), "Episode 1");
    }
}
