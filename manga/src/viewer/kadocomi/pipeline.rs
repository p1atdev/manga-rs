use url::Url;

use crate::{
    pipeline::{SaveFormat, WriterConfig},
    progress::ProgressConfig,
    viewer::ViewerConfigBuilder,
};

use super::viewer::{Client, ConfigBuilder};

/// Pipeline for downloading an episode from a Kadokomi-compatible site.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub(crate) client: Client,
    pub(crate) progress: ProgressConfig,
    pub(crate) writer_config: WriterConfig,
    pub(crate) num_threads: usize,
    pub(crate) num_connections: usize,
}

impl Pipeline {
    pub fn new(
        base_url: Url,
        progress: ProgressConfig,
        writer_config: WriterConfig,
        num_threads: usize,
        num_connections: usize,
    ) -> Self {
        Self {
            client: Client::new(ConfigBuilder::new(base_url).build()),
            progress,
            writer_config,
            num_threads,
            num_connections,
        }
    }

    pub fn for_base_url(base_url: Url) -> Self {
        Self {
            client: Client::new(ConfigBuilder::new(base_url).build()),
            progress: ProgressConfig::default(),
            writer_config: WriterConfig::new(SaveFormat::Raw, image::ImageFormat::Png),
            num_threads: num_cpus::get(),
            num_connections: 8,
        }
    }
}
