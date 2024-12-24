use anyhow::{Context, Result};
use futures::{stream, StreamExt, TryStreamExt};
use image::DynamicImage;
use std::{path::Path, sync::Arc, usize};
use url::Url;

#[cfg(feature = "pdf")]
use crate::io::pdf::PdfWriter;
use crate::{
    data::MangaEpisode,
    error::ClientError,
    io::FileWriter,
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SaveFormat, WriterConifg},
    progress::ProgressConfig,
    solver::ImageSolver,
    utils::Bytes,
    viewer::{ViewerClient, ViewerConfigBuilder},
};

use super::{
    // data::{Episode, Page},
    pipeline::Pipeline,
    // solver::Solver,
    viewer::{Client, ConfigBuilder, Website},
};
