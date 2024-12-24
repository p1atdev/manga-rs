use std::{path::Path, usize};

use crate::{
    pipeline::{SaveFormat, WriterConifg},
    progress::ProgressConfig,
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::viewer::{Client, ConfigBuilder, Website};

/// Pipeline for downloading an episode of ChojuGiga manga
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub(crate) client: Client,
    pub(crate) progress: ProgressConfig,
    pub(crate) writer_config: WriterConifg,
    pub(crate) num_threads: usize,
    pub(crate) num_connections: usize,
}

impl Default for Pipeline {
    fn default() -> Self {
        let writer_config = WriterConifg::new(SaveFormat::Raw, image::ImageFormat::Png);
        Self {
            client: Client::new(ConfigBuilder::new(Website::Kadocomi).build()),
            progress: ProgressConfig::default(),
            num_threads: num_cpus::get(),
            num_connections: 8,
            writer_config,
        }
    }
}

impl Pipeline {
    pub fn new<P: AsRef<Path>>(
        website: Website,
        progress: ProgressConfig,
        writer_config: WriterConifg,
        num_threads: usize,
        num_connections: usize,
    ) -> Self {
        let client = Client::new(ConfigBuilder::new(website).build());
        Self {
            client,
            progress: progress.clone(),
            num_threads,
            num_connections,
            writer_config,
        }
    }
}
