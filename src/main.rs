use anyhow::{Context, Result};
#[cfg(feature = "fuz")]
use manga::viewer::fuz::{self, pipeline::Pipeline as FuzPipeline};
use manga::viewer::giga::{self, pipeline::Pipeline as GigaPipeline};
use manga::viewer::ViewerConfigBuilder;
use manga::{progress::ProgressConfig, viewer::ViewerWebsite};

use clap::{Parser, Subcommand, ValueEnum};
use url::Url;

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Source,
}

#[derive(Debug, Clone, Subcommand)]
enum Source {
    Episode {
        /// Episode URL of the manga
        url: Url,

        /// Output path
        #[arg(short, long)]
        output: String,

        /// Image format
        #[arg(short, long, default_value = "png")]
        format: ImageFormat,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("{:?}", cli);

    let progress = ProgressConfig::default();

    match cli.command {
        Source::Episode {
            url,
            output,
            format,
        } => {
            if let Some(website) =
                giga::viewer::Website::lookup(url.host_str().context("Url must have host")?)
            {
                let pipe = GigaPipeline::default();
            }
        }
    };

    Ok(())
}
