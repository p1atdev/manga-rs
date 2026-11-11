//! Processing of already acquired image bytes while preserving the source format.
//!
//! The caller is responsible for acquiring bytes and identifying the correct viewer and
//! metadata. This module performs no network requests. Unsupported transformations return an
//! error; they are never replaced by a lossy re-encode or a different image format.
//!
//! PNG tile restoration losslessly re-encodes 8/16-bit grayscale, grayscale-alpha, RGB and RGBA
//! images. PNGs with ancillary metadata or unsupported color types are held rather than silently
//! dropping metadata or changing bit depth. Scrambled WebP is held; `Preserve` retains its bytes.

use std::fmt;

mod png;

/// Supported source and output image formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectFormat {
    Jpeg,
    Png,
    WebP,
}

impl DirectFormat {
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::WebP => "image/webp",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
        }
    }
}

/// A page whose content and MIME type are derived from the same processing result.
#[derive(Clone)]
pub struct ProcessedPage {
    bytes: Vec<u8>,
    format: DirectFormat,
}

impl fmt::Debug for ProcessedPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProcessedPage")
            .field("length", &self.bytes.len())
            .field("format", &self.format)
            .finish()
    }
}

impl ProcessedPage {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn format(&self) -> DirectFormat {
        self.format
    }
    pub fn mime_type(&self) -> &'static str {
        self.format.mime_type()
    }
}

/// Restoration required for this page. Use `Preserve` only when the caller has established
/// that the supplied bytes need no decryption or tile rearrangement.
#[derive(Clone, Copy)]
pub enum DirectTransform<'a> {
    Preserve,
    Giga,
    /// `source_by_destination[d]` identifies the source tile for destination `d`, row-major.
    Tiles4x4 {
        tile_width: u32,
        tile_height: u32,
        source_by_destination: &'a [usize],
    },
    Kadocomi {
        key: &'a str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectErrorKind {
    Unsupported,
    Malformed,
    LimitExceeded,
    InvalidArgument,
    Internal,
}

#[derive(Debug)]
pub struct DirectError {
    kind: DirectErrorKind,
    message: String,
}

impl DirectError {
    pub fn kind(&self) -> DirectErrorKind {
        self.kind
    }
    fn new(kind: DirectErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl fmt::Display for DirectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for DirectError {}

fn source_format(bytes: &[u8]) -> Result<DirectFormat, DirectError> {
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Jpeg) => Ok(DirectFormat::Jpeg),
        Ok(image::ImageFormat::Png) => Ok(DirectFormat::Png),
        Ok(image::ImageFormat::WebP) => Ok(DirectFormat::WebP),
        _ => Err(DirectError::new(
            DirectErrorKind::Unsupported,
            "input is not a supported JPEG, PNG, or WebP image",
        )),
    }
}

fn preserve(bytes: Vec<u8>) -> Result<ProcessedPage, DirectError> {
    let format = source_format(&bytes)?;
    if format == DirectFormat::Png {
        png::validate_chunks(&bytes, false)?;
    }
    // Validate image data before attaching an image MIME type, while returning the original bytes.
    decode_without_limits(
        &bytes,
        match format {
            DirectFormat::Jpeg => image::ImageFormat::Jpeg,
            DirectFormat::Png => image::ImageFormat::Png,
            DirectFormat::WebP => image::ImageFormat::WebP,
        },
    )?;
    Ok(ProcessedPage { bytes, format })
}

fn decode_without_limits(
    bytes: &[u8],
    format: image::ImageFormat,
) -> Result<image::DynamicImage, DirectError> {
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    reader.no_limits();
    reader.decode().map_err(|error| {
        let kind = match error {
            image::ImageError::Limits(_) => DirectErrorKind::LimitExceeded,
            image::ImageError::Unsupported(_) => DirectErrorKind::Unsupported,
            _ => DirectErrorKind::Malformed,
        };
        DirectError::new(kind, format!("invalid image data: {error}"))
    })
}

/// Decode a PNG that can be tile-restored without losing its original pixel format or metadata.
///
/// This checks the PNG signature, each chunk's CRC, the final IEND and the absence of trailing
/// bytes, then fully decodes the image. It accepts only non-indexed 8- or 16-bit grayscale,
/// grayscale-alpha, RGB or RGBA PNGs with no ancillary or otherwise unsupported chunks. Successful
/// output has the same bit depth, color channels, alpha values and pixel samples as the input.
/// The returned image contains no PNG chunks; this API does not validate a provider's tile mapping
/// or perform any permutation or encoding. Callers remain responsible for preserving these pixel
/// properties when processing and saving the returned image.
///
/// Non-PNG inputs and unsupported PNG colors, bit depths or metadata return
/// [`DirectErrorKind::Unsupported`]; corrupt data returns [`DirectErrorKind::Malformed`].
/// Representation or allocation limits reported by the decoder return
/// [`DirectErrorKind::LimitExceeded`]. Decoding uses the image library with its optional limits
/// disabled, so resource use depends on the input and available memory; an allocator abort is not
/// guaranteed to be recoverable. No network request or acquisition-size limit is applied.
pub fn decode_png_for_restoration(input: &[u8]) -> Result<image::DynamicImage, DirectError> {
    if !input.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(DirectError::new(
            DirectErrorKind::Unsupported,
            "input is not a PNG image",
        ));
    }
    png::validate_chunks(input, true)?;
    let decoded = decode_without_limits(input, image::ImageFormat::Png)?;
    let expected = match (input[25], input[24]) {
        (0, 8) => image::ColorType::L8,
        (0, 16) => image::ColorType::L16,
        (2, 8) => image::ColorType::Rgb8,
        (2, 16) => image::ColorType::Rgb16,
        (4, 8) => image::ColorType::La8,
        (4, 16) => image::ColorType::La16,
        (6, 8) => image::ColorType::Rgba8,
        (6, 16) => image::ColorType::Rgba16,
        _ => {
            return Err(DirectError::new(
                DirectErrorKind::Unsupported,
                "unsupported PNG pixel format",
            ));
        }
    };
    if decoded.color() != expected {
        return Err(DirectError::new(
            DirectErrorKind::Unsupported,
            "decoded PNG pixel format differs from the source",
        ));
    }
    Ok(decoded)
}

#[cfg(feature = "direct-sort")]
fn jpeg_error(error: crate::viewer::giga::direct_sort::DirectSortError) -> DirectError {
    use crate::viewer::giga::direct_sort::DirectSortErrorKind as K;
    let kind = match error.kind() {
        K::Unsupported => DirectErrorKind::Unsupported,
        K::Malformed => DirectErrorKind::Malformed,
        K::LimitExceeded => DirectErrorKind::LimitExceeded,
        K::InvalidArgument => DirectErrorKind::InvalidArgument,
        K::Internal => DirectErrorKind::Internal,
    };
    DirectError::new(kind, error.to_string())
}

/// Process downloaded bytes. Errors leave the input untouched and must be reported to the caller.
pub fn process_direct(
    input: &[u8],
    transform: DirectTransform<'_>,
) -> Result<ProcessedPage, DirectError> {
    match transform {
        DirectTransform::Preserve => preserve(input.to_vec()),
        DirectTransform::Kadocomi { key } => {
            #[cfg(feature = "kadokomi")]
            {
                use crate::{solver::ImageSolver, viewer::kadocomi::solver::Solver};
                let solver = Solver::from_hex(key).map_err(|err| {
                    DirectError::new(DirectErrorKind::InvalidArgument, err.to_string())
                })?;
                let bytes = solver
                    .solve(input.to_vec())
                    .map_err(|err| DirectError::new(DirectErrorKind::Malformed, err.to_string()))?;
                preserve(bytes)
            }
            #[cfg(not(feature = "kadokomi"))]
            {
                let _ = key;
                Err(DirectError::new(
                    DirectErrorKind::Unsupported,
                    "Kadocomi support is not enabled",
                ))
            }
        }
        DirectTransform::Giga | DirectTransform::Tiles4x4 { .. } => {
            match source_format(input)? {
                DirectFormat::Png => {
                    return Ok(ProcessedPage {
                        bytes: png::restore(input, transform)?,
                        format: DirectFormat::Png,
                    });
                }
                DirectFormat::WebP => {
                    return Err(DirectError::new(
                        DirectErrorKind::Unsupported,
                        "scrambled WebP restoration is not enabled",
                    ));
                }
                DirectFormat::Jpeg => {}
            }
            #[cfg(feature = "direct-sort")]
            {
                use crate::viewer::giga::direct_sort::{permute_jpeg, sort_jpeg};
                let bytes = match transform {
                    DirectTransform::Giga => sort_jpeg(input),
                    DirectTransform::Tiles4x4 {
                        tile_width,
                        tile_height,
                        source_by_destination,
                    } => permute_jpeg(input, tile_width, tile_height, source_by_destination),
                    _ => unreachable!(),
                }
                .map_err(jpeg_error)?;
                Ok(ProcessedPage {
                    bytes,
                    format: DirectFormat::Jpeg,
                })
            }
            #[cfg(not(feature = "direct-sort"))]
            {
                Err(DirectError::new(
                    DirectErrorKind::Unsupported,
                    "JPEG coefficient restoration requires the direct-sort feature",
                ))
            }
        }
    }
}
