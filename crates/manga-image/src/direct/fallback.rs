//! Explicit raster fallback; the direct coefficient path always has priority.
use super::*;
use image::RgbaImage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebpMode {
    /// Preserve decoded 8-bit pixels; quality controls compression effort.
    Lossless,
    /// Lossy WebP; quality controls visual fidelity.
    Lossy,
}
/// Explicit WebP encoding settings. Quality must be in 0..=100.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebpOptions {
    pub mode: WebpMode,
    pub quality: u8,
}

fn err(kind: DirectErrorKind, message: impl Into<String>) -> DirectError {
    DirectError::new(kind, message)
}
fn validate_options(o: WebpOptions) -> Result<(), DirectError> {
    if o.quality > 100 {
        return Err(err(
            DirectErrorKind::InvalidArgument,
            "WebP quality must be in 0..=100",
        ));
    }
    Ok(())
}

// Only an RGB ICC profile can be carried over after JPEG is decoded to RGB pixels.
fn safe_jpeg_metadata(bytes: &[u8]) -> Result<Option<Vec<u8>>, DirectError> {
    let mut p = 2usize;
    let mut scan = false;
    let mut icc_parts: Vec<Option<Vec<u8>>> = Vec::new();
    let mut components = None;
    loop {
        if scan {
            while p < bytes.len() && bytes[p] != 0xff {
                p += 1;
            }
        }
        if bytes.get(p) != Some(&0xff) {
            return Err(err(DirectErrorKind::Malformed, "invalid JPEG marker"));
        }
        while bytes.get(p) == Some(&0xff) {
            p += 1;
        }
        let m = *bytes
            .get(p)
            .ok_or_else(|| err(DirectErrorKind::Malformed, "truncated JPEG"))?;
        p += 1;
        if scan && (m == 0 || (0xd0..=0xd7).contains(&m) || m == 1) {
            continue;
        }
        if m == 0xd9 {
            if p != bytes.len() {
                return Err(err(DirectErrorKind::Malformed, "trailing JPEG data"));
            }
            break;
        }
        if m == 1 {
            continue;
        }
        let len = u16::from_be_bytes(
            bytes
                .get(p..p + 2)
                .ok_or_else(|| err(DirectErrorKind::Malformed, "truncated JPEG segment"))?
                .try_into()
                .unwrap(),
        ) as usize;
        let end = p
            .checked_add(len)
            .filter(|&end| len >= 2 && end <= bytes.len())
            .ok_or_else(|| err(DirectErrorKind::Malformed, "invalid JPEG segment"))?;
        if matches!(m, 0xc0..=0xcf) && !matches!(m, 0xc4 | 0xc8 | 0xcc) {
            components = bytes.get(p + 7).copied();
        }
        if m == 0xe2 && bytes[p + 2..end].starts_with(b"ICC_PROFILE\0") {
            let payload = &bytes[p + 2..end];
            if payload.len() < 14
                || payload[12] == 0
                || payload[13] == 0
                || payload[12] > payload[13]
            {
                return Err(err(
                    DirectErrorKind::Unsupported,
                    "invalid JPEG ICC segment numbering",
                ));
            }
            if icc_parts.is_empty() {
                icc_parts.resize(payload[13] as usize, None);
            }
            let index = payload[12] as usize - 1;
            if icc_parts.len() != payload[13] as usize || icc_parts[index].is_some() {
                return Err(err(
                    DirectErrorKind::Unsupported,
                    "duplicate or inconsistent JPEG ICC segments",
                ));
            }
            icc_parts[index] = Some(payload[14..].to_vec());
        } else if (0xe0..=0xef).contains(&m) || m == 0xfe {
            let payload = &bytes[p + 2..end];
            if m != 0xe0
                || payload.len() != 14
                || !payload.starts_with(b"JFIF\0")
                || payload[7] != 0
                || payload[8..10] != payload[10..12]
                || payload[12] != 0
                || payload[13] != 0
            {
                return Err(err(
                    DirectErrorKind::Unsupported,
                    "JPEG metadata cannot be retained in WebP fallback",
                ));
            }
        }
        p = end;
        if m == 0xda {
            scan = true;
        } else if m != 0xdc || !scan {
            scan = false;
        }
    }
    if icc_parts.is_empty() {
        return Ok(None);
    }
    if components != Some(3) || icc_parts.iter().any(Option::is_none) {
        return Err(err(
            DirectErrorKind::Unsupported,
            "incomplete ICC profile or non-RGB JPEG",
        ));
    }
    let profile: Vec<u8> = icc_parts.into_iter().flatten().flatten().collect();
    if profile.len() < 132
        || u32::from_be_bytes(profile[0..4].try_into().unwrap()) as usize != profile.len()
        || &profile[16..20] != b"RGB "
        || &profile[36..40] != b"acsp"
        || !matches!(&profile[20..24], b"XYZ " | b"Lab ")
        || !matches!(profile[8], 2 | 4)
        || !matches!(&profile[12..16], b"mntr" | b"scnr" | b"prtr" | b"spac")
    {
        return Err(err(
            DirectErrorKind::Unsupported,
            "unsupported or malformed RGB ICC profile",
        ));
    }
    let count = u32::from_be_bytes(profile[128..132].try_into().unwrap()) as usize;
    let table_end = count
        .checked_mul(12)
        .and_then(|n| n.checked_add(132))
        .filter(|&n| n <= profile.len())
        .ok_or_else(|| err(DirectErrorKind::Unsupported, "invalid ICC tag table"))?;
    if count == 0 {
        return Err(err(DirectErrorKind::Unsupported, "empty ICC tag table"));
    }
    for tag in profile[132..table_end].as_chunks::<12>().0 {
        let offset = u32::from_be_bytes(tag[4..8].try_into().unwrap()) as usize;
        let size = u32::from_be_bytes(tag[8..12].try_into().unwrap()) as usize;
        if offset < table_end
            || size < 8
            || offset
                .checked_add(size)
                .is_none_or(|end| end > profile.len())
        {
            return Err(err(DirectErrorKind::Unsupported, "invalid ICC tag bounds"));
        }
    }
    Ok(Some(profile))
}
fn decode_raster(input: &[u8]) -> Result<RgbaImage, DirectError> {
    let f = source_format(input)?;
    let im = match f {
        DirectFormat::Jpeg => {
            jpeg::validate_complete(input)?;
            jpeg::validate_decode(input)?;
            safe_jpeg_metadata(input)?;
            decode_without_limits(input, image::ImageFormat::Jpeg)?
        }
        DirectFormat::Png => {
            png::validate_chunks(input, false)?;
            let im = decode_without_limits(input, image::ImageFormat::Png)?;
            png::validate_chunks(input, true)?;
            if matches!(
                im.color(),
                image::ColorType::L16
                    | image::ColorType::La16
                    | image::ColorType::Rgb16
                    | image::ColorType::Rgba16
            ) {
                return Err(err(
                    DirectErrorKind::Unsupported,
                    "16-bit PNG cannot be represented in WebP without loss of precision",
                ));
            }
            im
        }
        DirectFormat::WebP => {
            webp::validate(input)?;
            validate_simple_webp(input)?;
            decode_without_limits(input, image::ImageFormat::WebP)?
        }
    };
    if im.width() > libwebp_sys::WEBP_MAX_DIMENSION || im.height() > libwebp_sys::WEBP_MAX_DIMENSION
    {
        return Err(err(
            DirectErrorKind::LimitExceeded,
            "image dimensions exceed WebP format limit",
        ));
    }
    Ok(im.into_rgba8())
}

// Only plain still images can be re-encoded without discarding container information.
fn validate_simple_webp(input: &[u8]) -> Result<(), DirectError> {
    let malformed = || err(DirectErrorKind::Malformed, "invalid WebP container");
    if input.len() < 12 || &input[..4] != b"RIFF" || &input[8..12] != b"WEBP" {
        return Err(malformed());
    }
    let end = (u32::from_le_bytes(input[4..8].try_into().unwrap()) as usize)
        .checked_add(8)
        .filter(|&n| n == input.len())
        .ok_or_else(malformed)?;
    let mut pos = 12usize;
    while pos < end {
        let header = input
            .get(pos..pos.checked_add(8).ok_or_else(malformed)?)
            .ok_or_else(malformed)?;
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(len)
            .filter(|&n| n <= end)
            .ok_or_else(malformed)?;
        match &header[..4] {
            b"VP8 " | b"VP8L" | b"ALPH" => {}
            b"VP8X" if len == 10 && input[data_start] & !0x10 == 0 => {}
            _ => {
                return Err(err(
                    DirectErrorKind::Unsupported,
                    "WebP animation or metadata cannot be retained during restoration",
                ));
            }
        }
        pos = data_end
            .checked_add(len & 1)
            .filter(|&n| n <= end)
            .ok_or_else(malformed)?;
    }
    Ok(())
}
fn encode(image: &RgbaImage, options: WebpOptions) -> Result<ProcessedPage, DirectError> {
    use libwebp_sys::*;
    let mut config = WebPConfig::new().map_err(|_| {
        err(
            DirectErrorKind::Internal,
            "WebP config initialization failed",
        )
    })?;
    config.lossless = i32::from(options.mode == WebpMode::Lossless);
    config.quality = f32::from(options.quality);
    config.exact = 1;
    let mut picture = WebPPicture::new().map_err(|_| {
        err(
            DirectErrorKind::Internal,
            "WebP picture initialization failed",
        )
    })?;
    picture.width = image.width() as i32;
    picture.height = image.height() as i32;
    picture.use_argb = 1;
    // SAFETY: RGBA buffer is owned and sized by image, picture and writer are initialized by libwebp;
    // both allocations are freed after encode, including error paths.
    unsafe {
        if WebPPictureImportRGBA(&mut picture, image.as_raw().as_ptr(), picture.width * 4) == 0 {
            WebPPictureFree(&mut picture);
            return Err(err(
                DirectErrorKind::LimitExceeded,
                "WebP image allocation failed",
            ));
        }
        let mut writer = std::mem::MaybeUninit::<WebPMemoryWriter>::uninit();
        WebPMemoryWriterInit(writer.as_mut_ptr());
        let mut writer = writer.assume_init();
        picture.writer = Some(WebPMemoryWrite);
        picture.custom_ptr = (&mut writer as *mut WebPMemoryWriter).cast();
        let ok = WebPValidateConfig(&config) != 0 && WebPEncode(&config, &mut picture) != 0;
        let code = picture.error_code;
        let bytes = if ok {
            std::slice::from_raw_parts(writer.mem, writer.size).to_vec()
        } else {
            Vec::new()
        };
        WebPMemoryWriterClear(&mut writer);
        WebPPictureFree(&mut picture);
        if !ok {
            return Err(err(
                DirectErrorKind::Internal,
                format!("WebP encoding failed: {code:?}"),
            ));
        }
        Ok(ProcessedPage {
            bytes,
            format: DirectFormat::WebP,
        })
    }
}

fn encode_with_source(
    image: &RgbaImage,
    options: WebpOptions,
    source: &[u8],
) -> Result<ProcessedPage, DirectError> {
    let mut page = encode(image, options)?;
    if source_format(source)? != DirectFormat::Jpeg {
        return Ok(page);
    }
    let Some(profile) = safe_jpeg_metadata(source)? else {
        return Ok(page);
    };
    use libwebp_sys::*;
    // SAFETY: mux copies the image and profile; assembled data is freed after copying.
    unsafe {
        let mux = WebPMuxNew();
        if mux.is_null() {
            return Err(err(
                DirectErrorKind::LimitExceeded,
                "WebP mux allocation failed",
            ));
        }
        let image_data = WebPData {
            bytes: page.bytes.as_ptr(),
            size: page.bytes.len(),
        };
        let icc_data = WebPData {
            bytes: profile.as_ptr(),
            size: profile.len(),
        };
        let mut assembled = WebPData::default();
        let ok = WebPMuxSetImage(mux, &image_data, 1) == WebPMuxError::WEBP_MUX_OK
            && WebPMuxSetChunk(mux, c"ICCP".as_ptr(), &icc_data, 1) == WebPMuxError::WEBP_MUX_OK
            && WebPMuxAssemble(mux, &mut assembled) == WebPMuxError::WEBP_MUX_OK;
        if ok {
            page.bytes = std::slice::from_raw_parts(assembled.bytes, assembled.size).to_vec();
        }
        WebPDataClear(&mut assembled);
        WebPMuxDelete(mux);
        if !ok {
            return Err(err(
                DirectErrorKind::Internal,
                "could not retain RGB ICC in WebP",
            ));
        }
    }
    Ok(page)
}
fn tiles(
    mut image: RgbaImage,
    tw: u32,
    th: u32,
    permutation: &[usize],
) -> Result<RgbaImage, DirectError> {
    let (w, h) = image.dimensions();
    if tw == 0
        || th == 0
        || u64::from(tw) * 4 > u64::from(w)
        || u64::from(th) * 4 > u64::from(h)
        || permutation.len() != 16
    {
        return Err(err(DirectErrorKind::InvalidArgument, "invalid tile grid"));
    }
    let mut seen = [false; 16];
    for &v in permutation {
        if v >= 16 || seen[v] {
            return Err(err(
                DirectErrorKind::InvalidArgument,
                "invalid tile permutation",
            ));
        }
        seen[v] = true;
    }
    let source = image.clone();
    for (d, &s) in permutation.iter().enumerate() {
        for y in 0..th {
            for x in 0..tw {
                image.put_pixel(
                    (d as u32 % 4) * tw + x,
                    (d as u32 / 4) * th + y,
                    *source.get_pixel((s as u32 % 4) * tw + x, (s as u32 / 4) * th + y),
                );
            }
        }
    }
    Ok(image)
}
fn raster_transform(input: &[u8], t: DirectTransform<'_>) -> Result<RgbaImage, DirectError> {
    let image = decode_raster(input)?;
    let (w, h) = image.dimensions();
    match t {
        DirectTransform::Preserve => Ok(image),
        DirectTransform::Giga => {
            if w < 32 || h < 32 {
                return Ok(image);
            }
            let p: [usize; 16] = std::array::from_fn(|d| (d % 4) * 4 + d / 4);
            tiles(image, w / 32 * 8, h / 32 * 8, &p)
        }
        DirectTransform::Tiles4x4 {
            tile_width,
            tile_height,
            source_by_destination,
        } => tiles(image, tile_width, tile_height, source_by_destination),
        DirectTransform::Kadocomi { .. } => Err(err(
            DirectErrorKind::Unsupported,
            "encrypted raster fallback unsupported",
        )),
    }
}

/// Try direct restoration first, falling back to WebP only where permitted.
/// PNG inputs are converted to WebP; unchanged JPEG and WebP pages retain their format.
pub fn process_direct_with_fallback(
    input: &[u8],
    transform: DirectTransform<'_>,
    options: WebpOptions,
) -> Result<ProcessedPage, DirectError> {
    validate_options(options)?;
    if matches!(transform, DirectTransform::Giga) && source_format(input)? == DirectFormat::WebP {
        let (w, h) = provider_dimensions(input)?;
        if w < 32 || h < 32 {
            return process_direct(input, DirectTransform::Preserve);
        }
    }
    match process_direct(input, transform) {
        Ok(page) if page.format() != DirectFormat::Png => Ok(page),
        Ok(page) => encode(&decode_raster(page.bytes())?, options),
        Err(e) if e.kind() == DirectErrorKind::Unsupported => {
            encode_with_source(&raster_transform(input, transform)?, options, input)
        }
        Err(e) => Err(e),
    }
}

/// Restore a provider page, using WebP when direct JPEG restoration is unavailable.
/// Unsupported metadata, animation and 16-bit samples are reported, never silently dropped.
/// Lossless describes the decoded pixels, not the JPEG's original DCT coefficients.
pub fn process_provider_image_with_fallback_checked(
    input: &[u8],
    provider: ProviderImage<'_>,
    expected_dimensions: Option<(u32, u32)>,
    options: WebpOptions,
) -> Result<ProcessedPage, DirectError> {
    validate_options(options)?;
    if matches!(expected_dimensions, Some((0, _)) | Some((_, 0))) {
        return Err(err(
            DirectErrorKind::InvalidArgument,
            "expected image dimensions must be nonzero",
        ));
    }
    let mut preserve_webp = false;
    if matches!(source_format(input), Ok(DirectFormat::WebP)) {
        preserve_webp = match provider {
            ProviderImage::Giga => {
                let (w, h) = provider_dimensions(input)?;
                w < 32 || h < 32
            }
            #[cfg(feature = "comici")]
            ProviderImage::Comici { scramble } => scramble
                .tile_sources()
                .iter()
                .enumerate()
                .all(|(i, &v)| i == v),
            _ => false,
        };
    }
    let direct = if preserve_webp {
        process_direct(input, DirectTransform::Preserve)
    } else {
        process_provider_image(input, provider)
    };
    let result = match direct {
        Ok(page) if page.format() != DirectFormat::Png => page,
        Ok(page) => encode(&decode_raster(page.bytes())?, options)?,
        Err(e) if e.kind() == DirectErrorKind::Unsupported => {
            let image = match provider {
                ProviderImage::PiccomaSeed { seed } => {
                    piccoma::restore_image(decode_raster(input)?, seed).map_err(|e| {
                        err(
                            DirectErrorKind::InvalidArgument,
                            format!("Piccoma restoration failed: {e}"),
                        )
                    })?
                }
                ProviderImage::Yanmaga {
                    source,
                    destination,
                } => yanmaga::restore_image(decode_raster(input)?, source, destination).map_err(
                    |e| {
                        err(
                            DirectErrorKind::InvalidArgument,
                            format!("YanMaga restoration failed: {e}"),
                        )
                    },
                )?,
                ProviderImage::Giga => raster_transform(input, DirectTransform::Giga)?,
                #[cfg(feature = "comici")]
                ProviderImage::Comici { scramble } => {
                    let image = decode_raster(input)?;
                    let (w, h) = image.dimensions();
                    if w < 4 || h < 4 {
                        image
                    } else {
                        let sources = scramble.tile_sources();
                        let p: [usize; 16] = std::array::from_fn(|d| {
                            let s = sources[(d % 4) * 4 + d / 4];
                            (s % 4) * 4 + s / 4
                        });
                        tiles(image, w / 4, h / 4, &p)?
                    }
                }
                #[cfg(feature = "magapoke")]
                ProviderImage::Magapoke {
                    title,
                    episode,
                    seed,
                } => {
                    let image = decode_raster(input)?;
                    let (w, h) = image.dimensions();
                    if w < 32 || h < 32 {
                        return Err(err(
                            DirectErrorKind::Unsupported,
                            "Magapoke dimensions must be at least 32",
                        ));
                    }
                    if image.pixels().any(|p| p[3] != 255) {
                        return Err(err(
                            DirectErrorKind::Unsupported,
                            "transparent Magapoke image",
                        ));
                    }
                    let p = crate::viewer::magapoke::tile_permutation(title, episode, seed)
                        .map_err(|_| {
                            err(
                                DirectErrorKind::InvalidArgument,
                                "invalid Magapoke metadata",
                            )
                        })?;
                    tiles(image, w / 32 * 8, h / 32 * 8, &p)?
                }
                _ => return Err(e),
            };
            encode_with_source(&image, options, input)?
        }
        Err(e) => return Err(e),
    };
    if let Some(expected) = expected_dimensions
        && provider_dimensions(result.bytes())? != expected
    {
        return Err(err(
            DirectErrorKind::Malformed,
            "image dimensions do not match provider metadata",
        ));
    }
    Ok(result)
}
