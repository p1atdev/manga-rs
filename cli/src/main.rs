use anyhow::{bail, Context, Result};
use manga::pipeline::{EpisodePipeline, EpisodePipelineBuilder, SeriesPipeline, WriterConifg};
use manga::viewer::fuz::{self, pipeline::Pipeline as FuzPipeline};
use manga::viewer::giga::{self, pipeline::Pipeline as GigaPipeline};
use manga::{progress::ProgressConfig, viewer::ViewerWebsite};

use clap::{Parser, Subcommand, ValueEnum};
use url::Url;

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Target,
}

#[derive(Debug, Clone, Subcommand)]
enum Target {
    Episode {
        /// Episode URL of the manga
        url: Url,

        /// Output directory.
        /// New directory or file will be created in this directory.
        #[arg(short, long)]
        output_dir: String,

        /// Save as
        #[arg(short, long, default_value = "raw")]
        save_as: SaveFormat,

        /// Image format
        #[arg(short, long, default_value = "png")]
        format: ImageFormat,
    },
    Series {
        /// Series URL of the manga
        url: Url,

        /// Output directory.
        /// New directory or file will be created in this directory.
        #[arg(short, long)]
        output_dir: String,

        /// Save as
        #[arg(short, long, default_value = "raw")]
        save_as: SaveFormat,

        /// Image format
        #[arg(short, long, default_value = "webp")]
        format: ImageFormat,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum ImageFormat {
    Png,
    #[value(alias = "jpg")]
    Jpeg,
    Webp,
}

#[derive(Debug, Clone, ValueEnum)]
enum SaveFormat {
    Raw,
    Zip,
    Cbz,
    // Pdf,
}

fn get_save_format(save: SaveFormat) -> manga::pipeline::SaveFormat {
    match save {
        SaveFormat::Raw => manga::pipeline::SaveFormat::Raw,
        SaveFormat::Zip => manga::pipeline::SaveFormat::Zip {
            compression_method: zip::CompressionMethod::Deflated,
            extension: None,
        },
        SaveFormat::Cbz => manga::pipeline::SaveFormat::Zip {
            compression_method: zip::CompressionMethod::Deflated,
            extension: Some("cbz".to_string()),
        },
        // SaveFormat::Pdf => manga::pipeline::SaveFormat::Pdf,
    }
}

fn get_image_format(format: ImageFormat) -> image::ImageFormat {
    match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Webp => image::ImageFormat::WebP,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("{:?}", cli);

    let progress = ProgressConfig::default();

    match cli.command {
        Target::Episode {
            url,
            output_dir,
            save_as,
            format,
        } => {
            let host = url.host_str().context("Url must have host")?;

            let save_format = get_save_format(save_as);
            let image_format = get_image_format(format);

            if let Some(website) = giga::viewer::Website::lookup(host) {
                let pipe = GigaPipeline::default()
                    .set_website(website)
                    .set_progress(progress)
                    .set_writer_config(WriterConifg::new(save_format, image_format));

                pipe.download_in(&url, &output_dir).await?;

                return Ok(());
            }

            if let Some(website) = fuz::viewer::Website::lookup(host) {
                let pipe = FuzPipeline::default()
                    .set_website(website)
                    .set_progress(progress)
                    .set_writer_config(WriterConifg::new(save_format, image_format));

                pipe.download_in(&url, &output_dir).await?;

                return Ok(());
            }

            bail!("Website not supported: {}", host);
        }
        Target::Series {
            url,
            output_dir,
            save_as,
            format,
        } => {
            let host = url.host_str().context("Url must have host")?;

            let save_format = get_save_format(save_as);
            let image_format = get_image_format(format);

            if let Some(website) = giga::viewer::Website::lookup(host) {
                let pipe = GigaPipeline::default()
                    .set_website(website)
                    .set_progress(progress)
                    .set_writer_config(WriterConifg::new(save_format, image_format));

                pipe.download_series(&url, &output_dir).await?;

                return Ok(());
            }

            if let Some(website) = fuz::viewer::Website::lookup(host) {
                let pipe = FuzPipeline::default()
                    .set_website(website)
                    .set_progress(progress)
                    .set_writer_config(WriterConifg::new(save_format, image_format));

                // pipe.download_series(&url, &output_dir).await?;
                todo!();

                return Ok(());
            }

            bail!("Website not supported: {}", host);
        }
    };
}
