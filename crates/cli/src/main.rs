use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use manga::{
    pipeline::{EpisodePipeline, EpisodePipelineBuilder, SeriesPipeline, WriterConfig},
    progress::ProgressConfig,
    viewer::{
        ViewerType, comici::pipeline::Pipeline as ComiciPipeline, detect::detect,
        giga::pipeline::Pipeline as GigaPipeline, kadocomi::pipeline::Pipeline as KadokomiPipeline,
    },
};
use url::Url;

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Episode(EpisodeArgs),
    Series(SeriesArgs),
}

#[derive(Debug, Clone, clap::Args)]
struct EpisodeArgs {
    /// Episode URL. The viewer is detected from the page contents.
    url: Url,

    /// Directory in which a new directory or archive will be created.
    #[arg(short, long)]
    output_dir: PathBuf,

    /// Output container.
    #[arg(short, long, default_value = "raw")]
    save_as: SaveFormat,

    /// Output image format.
    #[arg(short, long, default_value = "png")]
    format: ImageFormat,
}

#[derive(Debug, Clone, clap::Args)]
struct SeriesArgs {
    /// Episode URL within the series. The viewer is detected from the page contents.
    url: Url,

    /// Directory in which the downloaded episodes will be created.
    #[arg(short, long)]
    output_dir: PathBuf,

    /// Output container.
    #[arg(short, long, default_value = "raw")]
    save_as: SaveFormat,

    /// Output image format.
    #[arg(short, long, default_value = "webp")]
    format: ImageFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ImageFormat {
    Png,
    #[value(alias = "jpg")]
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SaveFormat {
    Raw,
    Zip,
    Cbz,
}

impl From<SaveFormat> for manga::pipeline::SaveFormat {
    fn from(value: SaveFormat) -> Self {
        match value {
            SaveFormat::Raw => Self::Raw,
            SaveFormat::Zip => Self::Zip {
                compression_method: zip::CompressionMethod::Deflated,
                extension: None,
            },
            SaveFormat::Cbz => Self::Zip {
                compression_method: zip::CompressionMethod::Deflated,
                extension: Some("cbz".to_owned()),
            },
        }
    }
}

impl From<ImageFormat> for image::ImageFormat {
    fn from(value: ImageFormat) -> Self {
        match value {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Webp => Self::WebP,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Episode(args) => download_episode(args).await,
        Command::Series(args) => download_series(args).await,
    }
}

async fn download_episode(args: EpisodeArgs) -> Result<()> {
    let detected = detect(&args.url).await?;
    let writer = WriterConfig::new(args.save_as.into(), args.format.into());
    let progress = ProgressConfig::default();

    match detected.viewer_type() {
        ViewerType::Comici => {
            ComiciPipeline::for_base_url(detected.base_url().clone())
                .set_progress(progress)
                .set_writer_config(writer)
                .download_in(&args.url, &args.output_dir)
                .await
        }
        ViewerType::Giga => {
            GigaPipeline::for_base_url(detected.base_url().clone())
                .set_progress(progress)
                .set_writer_config(writer)
                .download_in(&args.url, &args.output_dir)
                .await
        }
        ViewerType::Kadokomi => {
            KadokomiPipeline::for_base_url(detected.base_url().clone())
                .set_progress(progress)
                .set_writer_config(writer)
                .download_in(&args.url, &args.output_dir)
                .await
        }
        viewer => bail!("the detected {viewer:?} viewer is not enabled in this CLI"),
    }
}

async fn download_series(args: SeriesArgs) -> Result<()> {
    let detected = detect(&args.url).await?;
    let writer = WriterConfig::new(args.save_as.into(), args.format.into());

    match detected.viewer_type() {
        ViewerType::Comici => {
            bail!("series download is not supported for the detected Comici viewer")
        }
        ViewerType::Giga => {
            GigaPipeline::for_base_url(detected.base_url().clone())
                .set_progress(ProgressConfig::default())
                .set_writer_config(writer)
                .download_series(&args.url, &args.output_dir)
                .await
        }
        ViewerType::Kadokomi => {
            bail!("series download is not supported for the detected Kadokomi viewer")
        }
        viewer => bail!("the detected {viewer:?} viewer is not enabled in this CLI"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_episode_command() {
        let cli = Cli::try_parse_from([
            "manga",
            "episode",
            "https://example.com/episode/1",
            "--output-dir",
            "downloads",
            "--save-as",
            "cbz",
        ])
        .unwrap();

        match cli.command {
            Command::Episode(args) => assert_eq!(args.format, ImageFormat::Png),
            Command::Series(_) => panic!("expected episode command"),
        }
    }

    #[test]
    fn series_defaults_to_webp() {
        let cli = Cli::try_parse_from([
            "manga",
            "series",
            "https://example.com/episode/1",
            "--output-dir",
            "downloads",
        ])
        .unwrap();

        match cli.command {
            Command::Series(args) => assert_eq!(args.format, ImageFormat::Webp),
            Command::Episode(_) => panic!("expected series command"),
        }
    }
}
