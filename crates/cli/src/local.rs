use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use image::{DynamicImage, ImageFormat};
use manga::{
    direct::{
        DirectError, DirectErrorKind, DirectFormat, DirectTransform, ProcessedPage, ProviderImage,
        WebpMode, WebpOptions, decode_png_for_restoration, process_direct,
        process_direct_with_fallback, process_provider_image_with_fallback_checked,
    },
    solver::ImageSolver,
    viewer::{comici, fuz, kadocomi, magapoke, piccoma, yanmaga},
};

#[derive(Debug, Clone, Args)]
pub struct DecodeArgs {
    /// Require the direct backend unless --fallback is supplied.
    #[arg(long, global = true)]
    direct: bool,
    /// Enable WebP fallback. Output extension must match the actual format.
    #[arg(long, global = true, value_parser = ["webp"])]
    fallback: Option<String>,
    /// Lossless WebP compression effort (0-100, default 75).
    #[arg(long, global = true, requires = "fallback", conflicts_with = "lossy", value_parser = clap::value_parser!(u8).range(0..=100))]
    lossless: Option<u8>,
    /// Lossy WebP visual quality (0-100).
    #[arg(long, global = true, requires = "fallback", conflicts_with = "lossless", value_parser = clap::value_parser!(u8).range(0..=100))]
    lossy: Option<u8>,
    #[command(subcommand)]
    provider: DecodeProvider,
}

#[derive(Debug, Clone, Subcommand)]
enum DecodeProvider {
    Giga(FileArgs),
    Comici(ComiciArgs),
    Magapoke(MagapokeArgs),
    Piccoma(PiccomaArgs),
    Yanmaga(YanmagaArgs),
    Kadokomi(KeyArgs),
    Fuz(FuzArgs),
}

#[derive(Debug, Clone, Args)]
pub struct DecryptArgs {
    #[command(subcommand)]
    provider: DecryptProvider,
}

#[derive(Debug, Clone, Subcommand)]
enum DecryptProvider {
    Kadokomi(KeyArgs),
    Fuz(FuzArgs),
}

#[derive(Debug, Clone, Args)]
struct FileArgs {
    /// Input image file.
    input: PathBuf,
    /// New output path; an existing file is never overwritten.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, Clone, Args)]
struct KeyArgs {
    #[command(flatten)]
    files: FileArgs,
    /// File containing the hexadecimal key.
    #[arg(long)]
    key_file: PathBuf,
}

#[derive(Debug, Clone, Args)]
struct FuzArgs {
    #[command(flatten)]
    key: KeyArgs,
    /// File containing the hexadecimal IV.
    #[arg(long)]
    iv_file: PathBuf,
}

#[derive(Debug, Clone, Args)]
struct ComiciArgs {
    #[command(flatten)]
    files: FileArgs,
    /// JSON array of 16 source tile indexes, in Comici's column-major order.
    #[arg(long)]
    scramble: String,
}

#[derive(Debug, Clone, Args)]
struct MagapokeArgs {
    #[command(flatten)]
    files: FileArgs,
    #[arg(long, required_unless_present = "permutation", requires_all = ["episode", "seed"], conflicts_with = "permutation")]
    title: Option<u32>,
    #[arg(
        long,
        required_unless_present = "permutation",
        conflicts_with = "permutation"
    )]
    episode: Option<u32>,
    #[arg(
        long,
        required_unless_present = "permutation",
        conflicts_with = "permutation"
    )]
    seed: Option<String>,
    /// JSON array mapping each row-major destination tile to its source.
    #[arg(long, conflicts_with_all = ["title", "episode", "seed"])]
    permutation: Option<String>,
}

#[derive(Debug, Clone, Args)]
struct PiccomaArgs {
    #[command(flatten)]
    files: FileArgs,
    /// Page shuffle seed.
    #[arg(long)]
    seed: String,
}

#[derive(Debug, Clone, Args)]
struct YanmagaArgs {
    #[command(flatten)]
    files: FileArgs,
    /// File containing the page's source tile key.
    #[arg(long)]
    source_file: PathBuf,
    /// File containing the page's destination tile key.
    #[arg(long)]
    destination_file: PathBuf,
}

pub fn decrypt(args: DecryptArgs) -> Result<()> {
    match args.provider {
        DecryptProvider::Kadokomi(args) => decrypt_kadokomi(args),
        DecryptProvider::Fuz(args) => decrypt_fuz(args),
    }
}

pub fn decode(args: DecodeArgs) -> Result<()> {
    if args.fallback.is_some() {
        let options = match args.lossy {
            Some(quality) => WebpOptions {
                mode: WebpMode::Lossy,
                quality,
            },
            None => WebpOptions {
                mode: WebpMode::Lossless,
                quality: args.lossless.unwrap_or(75),
            },
        };
        return decode_with_fallback(args.provider, options);
    }
    if args.direct
        && matches!(
            &args.provider,
            DecodeProvider::Piccoma(_)
                | DecodeProvider::Yanmaga(_)
                | DecodeProvider::Kadokomi(_)
                | DecodeProvider::Fuz(_)
        )
    {
        bail!("unsupported: direct tile restoration is unavailable for this provider");
    }
    match args.provider {
        DecodeProvider::Kadokomi(args) => decrypt_kadokomi(args),
        DecodeProvider::Fuz(args) => decrypt_fuz(args),
        DecodeProvider::Giga(files) => {
            restore_direct(files, |bytes| direct(bytes, DirectTransform::Giga))
        }
        DecodeProvider::Comici(args) => {
            let scramble: comici::data::Scramble =
                serde_json::from_value(serde_json::Value::String(args.scramble))
                    .context("invalid Comici scramble")?;
            restore_direct(args.files, |bytes| {
                let (width, height) = image_dimensions(bytes)?;
                let sources = scramble.tile_sources();
                let permutation: [usize; 16] = std::array::from_fn(|destination| {
                    let x = destination % 4;
                    let y = destination / 4;
                    let source = sources[x * 4 + y];
                    (source % 4) * 4 + source / 4
                });
                direct(
                    bytes,
                    DirectTransform::Tiles4x4 {
                        tile_width: width / 4,
                        tile_height: height / 4,
                        source_by_destination: &permutation,
                    },
                )
            })
        }
        DecodeProvider::Magapoke(args) => restore_direct(args.files, |bytes| {
            let permutation: Vec<usize> = match args.permutation {
                Some(value) => {
                    serde_json::from_str(&value).context("invalid Magapoke permutation")?
                }
                None => magapoke::tile_permutation(
                    args.title.context("title is required")?,
                    args.episode.context("episode is required")?,
                    args.seed.as_deref().context("seed is required")?,
                )?
                .to_vec(),
            };
            let (width, height) = image_dimensions(bytes)?;
            ensure!(
                width >= 32 && height >= 32,
                "unsupported: Magazine Pocket image dimensions must be at least 32 pixels"
            );
            if supported_format(bytes)? == ImageFormat::Png {
                let mut reader =
                    image::ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
                reader.no_limits();
                let image = reader.decode()?;
                if image.color().has_alpha() {
                    ensure!(
                        image.to_rgba16().pixels().all(|p| p[3] == u16::MAX),
                        "unsupported: transparent Magazine Pocket image"
                    );
                }
            }
            direct(
                bytes,
                DirectTransform::Tiles4x4 {
                    tile_width: (width / 32) * 8,
                    tile_height: (height / 32) * 8,
                    source_by_destination: &permutation,
                },
            )
        }),
        DecodeProvider::Piccoma(args) => restore(args.files, |image| {
            let image = image.into_rgba8();
            Ok(DynamicImage::ImageRgba8(piccoma::restore_image(
                image, &args.seed,
            )?))
        }),
        DecodeProvider::Yanmaga(args) => {
            let source =
                fs::read_to_string(args.source_file).context("could not read source tile key")?;
            let destination = fs::read_to_string(args.destination_file)
                .context("could not read destination tile key")?;
            restore(args.files, |image| {
                let image = image.into_rgba8();
                Ok(DynamicImage::ImageRgba8(yanmaga::restore_image(
                    image,
                    source.trim(),
                    destination.trim(),
                )?))
            })
        }
    }
}

fn decode_with_fallback(provider: DecodeProvider, options: WebpOptions) -> Result<()> {
    let (files, page) = match provider {
        DecodeProvider::Giga(files) => {
            let bytes = fs::read(&files.input)?;
            let page = process_provider_image_with_fallback_checked(
                &bytes,
                ProviderImage::Giga,
                None,
                options,
            )
            .map_err(direct_error)?;
            (files, page)
        }
        DecodeProvider::Comici(args) => {
            let bytes = fs::read(&args.files.input)?;
            let scramble: comici::data::Scramble =
                serde_json::from_value(serde_json::Value::String(args.scramble))
                    .context("invalid Comici scramble")?;
            let page = process_provider_image_with_fallback_checked(
                &bytes,
                ProviderImage::Comici {
                    scramble: &scramble,
                },
                None,
                options,
            )
            .map_err(direct_error)?;
            (args.files, page)
        }
        DecodeProvider::Piccoma(args) => {
            let bytes = fs::read(&args.files.input)?;
            let page = process_provider_image_with_fallback_checked(
                &bytes,
                ProviderImage::PiccomaSeed { seed: &args.seed },
                None,
                options,
            )
            .map_err(direct_error)?;
            (args.files, page)
        }
        DecodeProvider::Magapoke(args) => {
            let bytes = fs::read(&args.files.input)?;
            let (w, h) = image_dimensions(&bytes)?;
            ensure!(
                w >= 32 && h >= 32,
                "unsupported: Magazine Pocket dimensions must be at least 32 pixels"
            );
            let format = supported_format(&bytes)?;
            if matches!(format, ImageFormat::Png | ImageFormat::WebP) {
                let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
                reader.no_limits();
                let image = reader.decode()?;
                ensure!(
                    !image.color().has_alpha()
                        || image.to_rgba16().pixels().all(|p| p[3] == u16::MAX),
                    "unsupported: transparent Magazine Pocket image"
                );
            }
            let p: Vec<usize> = match args.permutation {
                Some(value) => {
                    serde_json::from_str(&value).context("invalid Magapoke permutation")?
                }
                None => magapoke::tile_permutation(
                    args.title.context("title required")?,
                    args.episode.context("episode required")?,
                    args.seed.as_deref().context("seed required")?,
                )?
                .to_vec(),
            };
            let page = process_direct_with_fallback(
                &bytes,
                DirectTransform::Tiles4x4 {
                    tile_width: w / 32 * 8,
                    tile_height: h / 32 * 8,
                    source_by_destination: &p,
                },
                options,
            )
            .map_err(direct_error)?;
            (args.files, page)
        }
        DecodeProvider::Yanmaga(args) => {
            let source =
                fs::read_to_string(&args.source_file).context("could not read source tile key")?;
            let destination = fs::read_to_string(&args.destination_file)
                .context("could not read destination tile key")?;
            let bytes = fs::read(&args.files.input)?;
            let page = process_provider_image_with_fallback_checked(
                &bytes,
                ProviderImage::Yanmaga {
                    source: source.trim(),
                    destination: destination.trim(),
                },
                None,
                options,
            )
            .map_err(direct_error)?;
            (args.files, page)
        }
        DecodeProvider::Kadokomi(args) => {
            let key = fs::read_to_string(&args.key_file)?;
            let bytes = fs::read(&args.files.input)?;
            let page = process_provider_image_with_fallback_checked(
                &bytes,
                ProviderImage::KadokomiXor {
                    key_hex: key.trim(),
                },
                None,
                options,
            )
            .map_err(direct_error)?;
            (args.files, page)
        }
        DecodeProvider::Fuz(args) => {
            let key = fs::read_to_string(&args.key.key_file)?;
            let iv = fs::read_to_string(&args.iv_file)?;
            let bytes = fs::read(&args.key.files.input)?;
            let page = process_provider_image_with_fallback_checked(
                &bytes,
                ProviderImage::FuzAesCbc {
                    key_hex: key.trim(),
                    iv_hex: iv.trim(),
                },
                None,
                options,
            )
            .map_err(direct_error)?;
            (args.key.files, page)
        }
    };
    let format = match page.format() {
        DirectFormat::Jpeg => ImageFormat::Jpeg,
        DirectFormat::Png => ImageFormat::Png,
        DirectFormat::WebP => ImageFormat::WebP,
    };
    check_extension(&files.output, format)?;
    write_new(&files.output, page.bytes())
}

fn image_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    Ok(image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .into_dimensions()?)
}

fn direct(bytes: &[u8], transform: DirectTransform<'_>) -> Result<ProcessedPage> {
    process_direct(bytes, transform).map_err(direct_error)
}

fn direct_error(error: DirectError) -> anyhow::Error {
    if error.kind() == DirectErrorKind::Unsupported {
        anyhow::anyhow!("unsupported: {error}")
    } else {
        anyhow::anyhow!("image restoration failed: {error}")
    }
}

fn restore_direct(
    files: FileArgs,
    solve: impl FnOnce(&[u8]) -> Result<ProcessedPage>,
) -> Result<()> {
    let input = fs::read(&files.input).context("could not read input file")?;
    let format = supported_format(&input)?;
    check_extension(&files.output, format)?;
    let output = solve(&input)?;
    write_new(&files.output, output.bytes())
}

fn decrypt_kadokomi(args: KeyArgs) -> Result<()> {
    let key = fs::read_to_string(&args.key_file).context("could not read key file")?;
    let solver = kadocomi::solver::Solver::from_hex(key.trim())
        .map_err(|_| anyhow::anyhow!("invalid Kadokomi key: expected 16 hex digits"))?;
    let encrypted = fs::read(&args.files.input).context("could not read input file")?;
    let decrypted = solver.solve(encrypted)?;
    write_decrypted(&args.files, &decrypted)
}

fn decrypt_fuz(args: FuzArgs) -> Result<()> {
    let key = fs::read_to_string(&args.key.key_file).context("could not read key file")?;
    let iv = fs::read_to_string(&args.iv_file).context("could not read IV file")?;
    let encrypted = fs::read(&args.key.files.input).context("could not read input file")?;
    let decrypted = fuz::solver::Solver::new(key.trim(), iv.trim())
        .solve(encrypted)
        .map_err(|_| anyhow::anyhow!("FUZ decryption failed: check key, IV, and ciphertext"))?;
    write_decrypted(&args.key.files, &decrypted)
}

fn write_decrypted(files: &FileArgs, bytes: &[u8]) -> Result<()> {
    let validated = direct(bytes, DirectTransform::Preserve)?;
    let format = match validated.format() {
        DirectFormat::Jpeg => ImageFormat::Jpeg,
        DirectFormat::Png => ImageFormat::Png,
        DirectFormat::WebP => ImageFormat::WebP,
    };
    check_extension(&files.output, format)?;
    write_new(&files.output, validated.bytes())
}

fn restore(
    files: FileArgs,
    solve: impl FnOnce(DynamicImage) -> Result<DynamicImage>,
) -> Result<()> {
    let input = fs::read(&files.input).context("could not read input file")?;
    let format = supported_format(&input)?;
    check_extension(&files.output, format)?;
    match format {
        ImageFormat::Png => {}
        ImageFormat::Jpeg => {
            bail!("unsupported: lossless JPEG tile restoration is not available in this build")
        }
        ImageFormat::WebP => bail!("unsupported: WebP encoding is currently on hold"),
        _ => unreachable!(),
    }
    let image = decode_png_for_restoration(&input).map_err(direct_error)?;
    let color = image.color();
    ensure!(
        !matches!(
            color,
            image::ColorType::L16
                | image::ColorType::La16
                | image::ColorType::Rgb16
                | image::ColorType::Rgba16
        ),
        "unsupported: the existing tile solvers cannot preserve 16-bit PNG precision"
    );
    let image = solve(image)?;
    // Both provider solvers rearrange RGBA pixels. Restore the original PNG
    // color type after the permutation so grayscale and alpha are preserved.
    let image = match color {
        image::ColorType::L8 => DynamicImage::ImageLuma8(image.into_luma8()),
        image::ColorType::La8 => DynamicImage::ImageLumaA8(image.into_luma_alpha8()),
        image::ColorType::Rgb8 => DynamicImage::ImageRgb8(image.into_rgb8()),
        image::ColorType::Rgba8 => image,
        _ => bail!("unsupported: PNG color type cannot be preserved"),
    };
    let mut encoded = Cursor::new(Vec::new());
    image.write_to(&mut encoded, ImageFormat::Png)?;
    write_new(&files.output, &encoded.into_inner())
}

fn supported_format(bytes: &[u8]) -> Result<ImageFormat> {
    match image::guess_format(bytes) {
        Ok(f @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)) => Ok(f),
        _ => bail!("unsupported: expected a JPEG, PNG, or WebP image"),
    }
}

fn check_extension(path: &Path, format: ImageFormat) -> Result<()> {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let matches = match format {
        ImageFormat::Png => extension.eq_ignore_ascii_case("png"),
        ImageFormat::Jpeg => {
            extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg")
        }
        ImageFormat::WebP => extension.eq_ignore_ascii_case("webp"),
        _ => false,
    };
    ensure!(
        matches,
        "output extension must match the actual output image format"
    );
    Ok(())
}

fn write_new(output: &Path, bytes: &[u8]) -> Result<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).context("could not create temporary output")?;
    temporary
        .write_all(bytes)
        .context("could not write output")?;
    temporary
        .persist_noclobber(output)
        .map_err(|_| anyhow::anyhow!("could not create output; it may already exist"))?;
    Ok(())
}
