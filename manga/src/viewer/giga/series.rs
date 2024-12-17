use anyhow::{Context, Ok, Result};
use futures::{stream, StreamExt, TryStreamExt};
use image::DynamicImage;
use std::{path::Path, sync::Arc, usize};
use url::Url;

use crate::{
    data::MangaEpisode,
    io::FileWriter,
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SaveFormat, SeriesPipeline, WriterConifg},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::Bytes,
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::{
    data::{Episode, Page},
    pipeline::Pipeline,
    solver::Solver,
    viewer::{Client, ConfigBuilder, Website},
};

impl Pipeline {
    async fn get_html(&self, url: Url) -> Result<String> {
        let res = self.client.get(url).await?;
        let body = res.text().await?;
        Ok(body)
    }
}

impl SeriesPipeline<Page, Episode> for Pipeline {
    async fn get_episode_urls(&self, url: Url) -> Result<Vec<Url>> {
        todo!()
    }

    async fn download_series<T: AsRef<Path>>(&self, url: &Url, dir: &T) -> Result<()> {
        todo!()
    }

    async fn download_episodes<T: AsRef<Path>>(&self, urls: Vec<Url>, dir: &T) -> Result<()> {
        todo!()
    }
}
