use std::{num::NonZeroUsize, path::PathBuf};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
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
#[command(
    name = "manga",
    version,
    about = "Download manga from supported official web viewers",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    /// Download a single episode.
    Episode(EpisodeArgs),

    /// Download the available episodes in a supported series.
    Series(SeriesArgs),
}

#[derive(Debug, Clone, Args)]
struct EpisodeArgs {
    /// Episode URL. The viewer is detected from the page contents.
    url: Url,

    #[command(flatten)]
    download: DownloadArgs,
}

#[derive(Debug, Clone, Args)]
struct SeriesArgs {
    /// Episode URL within the series. The viewer is detected from the page contents.
    url: Url,

    #[command(flatten)]
    download: DownloadArgs,
}

#[derive(Debug, Clone, Args)]
struct DownloadArgs {
    /// Directory in which downloaded directories or archives will be created.
    #[arg(short, long, default_value = ".")]
    output_dir: PathBuf,

    /// Output container.
    #[arg(short, long, default_value = "raw")]
    save_as: SaveFormat,

    /// Output image format.
    #[arg(short, long, default_value = "webp")]
    format: ImageFormat,

    /// Disable progress bars.
    #[arg(long)]
    no_progress: bool,

    /// Maximum concurrent image-processing jobs. Defaults to the logical CPU count.
    #[arg(short = 'j', long, value_name = "COUNT")]
    jobs: Option<NonZeroUsize>,

    /// Maximum concurrent network requests.
    #[arg(long, value_name = "COUNT", default_value = "8")]
    connections: NonZeroUsize,
}

impl DownloadArgs {
    fn configure<P: EpisodePipelineBuilder>(&self, pipeline: P) -> P {
        let progress = if self.no_progress {
            ProgressConfig::disabled()
        } else {
            ProgressConfig::default()
        };
        let writer = WriterConfig::new(self.save_as.into(), self.format.into());
        let mut pipeline = pipeline
            .set_progress(progress)
            .set_writer_config(writer)
            .set_num_connections(self.connections.get());

        if let Some(jobs) = self.jobs {
            pipeline = pipeline.set_num_threads(jobs.get());
        }

        pipeline
    }
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

    match detected.viewer_type() {
        ViewerType::Comici => {
            args.download
                .configure(ComiciPipeline::for_base_url(detected.base_url().clone()))
                .download_in(&args.url, &args.download.output_dir)
                .await
        }
        ViewerType::Giga => {
            args.download
                .configure(GigaPipeline::for_base_url(detected.base_url().clone()))
                .download_in(&args.url, &args.download.output_dir)
                .await
        }
        ViewerType::Kadokomi => {
            args.download
                .configure(KadokomiPipeline::for_base_url(detected.base_url().clone()))
                .download_in(&args.url, &args.download.output_dir)
                .await
        }
        viewer => bail!("the detected {viewer:?} viewer is not enabled in this CLI"),
    }
}

async fn download_series(args: SeriesArgs) -> Result<()> {
    let detected = detect(&args.url).await?;

    match detected.viewer_type() {
        ViewerType::Comici => {
            args.download
                .configure(ComiciPipeline::for_base_url(detected.base_url().clone()))
                .download_series(&args.url, &args.download.output_dir)
                .await
        }
        ViewerType::Giga => {
            args.download
                .configure(GigaPipeline::for_base_url(detected.base_url().clone()))
                .download_series(&args.url, &args.download.output_dir)
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
    use clap::{CommandFactory, error::ErrorKind};

    #[test]
    fn exposes_help_when_no_subcommand_is_given() {
        let error = Cli::try_parse_from(["manga"]).unwrap_err();

        assert_eq!(
            error.kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn exposes_binary_name_and_version() {
        let command = Cli::command();

        assert_eq!(command.get_name(), "manga");
        assert_eq!(command.get_version(), Some(env!("CARGO_PKG_VERSION")));
    }

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
            Command::Episode(args) => {
                assert_eq!(args.download.output_dir, PathBuf::from("downloads"));
                assert_eq!(args.download.save_as, SaveFormat::Cbz);
                assert_eq!(args.download.format, ImageFormat::Webp);
            }
            Command::Series(_) => panic!("expected episode command"),
        }
    }

    #[test]
    fn download_options_have_consistent_defaults() {
        let cli =
            Cli::try_parse_from(["manga", "series", "https://example.com/episode/1"]).unwrap();

        match cli.command {
            Command::Series(args) => {
                assert_eq!(args.download.output_dir, PathBuf::from("."));
                assert_eq!(args.download.save_as, SaveFormat::Raw);
                assert_eq!(args.download.format, ImageFormat::Webp);
                assert!(!args.download.no_progress);
                assert_eq!(args.download.jobs, None);
                assert_eq!(args.download.connections.get(), 8);
            }
            Command::Episode(_) => panic!("expected series command"),
        }
    }

    #[test]
    fn parses_progress_and_concurrency_options() {
        let cli = Cli::try_parse_from([
            "manga",
            "episode",
            "https://example.com/episode/1",
            "--no-progress",
            "--jobs",
            "3",
            "--connections",
            "5",
        ])
        .unwrap();

        match cli.command {
            Command::Episode(args) => {
                assert!(args.download.no_progress);
                assert_eq!(args.download.jobs.map(NonZeroUsize::get), Some(3));
                assert_eq!(args.download.connections.get(), 5);
            }
            Command::Series(_) => panic!("expected episode command"),
        }
    }

    #[test]
    fn rejects_zero_concurrency() {
        for option in ["--jobs", "--connections"] {
            let error = Cli::try_parse_from([
                "manga",
                "episode",
                "https://example.com/episode/1",
                option,
                "0",
            ])
            .unwrap_err();

            assert_eq!(error.kind(), ErrorKind::ValueValidation);
        }
    }
}
