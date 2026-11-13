//! Validate still and animated bitstreams using libwebp while retaining the original container.
use super::{DirectError, DirectErrorKind};
use libwebp_sys::*;

fn status_error(status: VP8StatusCode) -> DirectError {
    let kind = match status {
        VP8StatusCode::VP8_STATUS_OUT_OF_MEMORY => DirectErrorKind::LimitExceeded,
        VP8StatusCode::VP8_STATUS_UNSUPPORTED_FEATURE => DirectErrorKind::Unsupported,
        _ => DirectErrorKind::Malformed,
    };
    DirectError::new(kind, "invalid or unsupported WebP bitstream")
}

pub(super) fn validate(input: &[u8]) -> Result<(), DirectError> {
    // SAFETY: the initialized config belongs to this invocation, input is valid for the given
    // length, and any libwebp output buffer is released on every path after WebPDecode.
    unsafe {
        let mut config = WebPDecoderConfig::new()
            .map_err(|_| DirectError::new(DirectErrorKind::Internal, "libwebp ABI mismatch"))?;
        let status = WebPGetFeatures(input.as_ptr(), input.len(), &mut config.input);
        if status != VP8StatusCode::VP8_STATUS_OK {
            return Err(status_error(status));
        }
        if config.input.has_animation == 0 {
            let status = WebPDecode(input.as_ptr(), input.len(), &mut config);
            WebPFreeDecBuffer(&mut config.output);
            return if status == VP8StatusCode::VP8_STATUS_OK {
                Ok(())
            } else {
                Err(status_error(status))
            };
        }
        validate_animation(input)
    }
}

unsafe fn validate_animation(input: &[u8]) -> Result<(), DirectError> {
    let frame_chunks = count_frame_chunks(input)?;
    // SAFETY: the input lives until the decoder is deleted, and all C output pointers point to
    // locals initialized below. No libwebp buffer is used after the next iteration or deletion.
    unsafe {
        let data = WebPData {
            bytes: input.as_ptr(),
            size: input.len(),
        };
        let decoder = WebPAnimDecoderNew(&data, std::ptr::null());
        if decoder.is_null() {
            return Err(DirectError::new(
                DirectErrorKind::Malformed,
                "invalid animated WebP",
            ));
        }
        let mut info = WebPAnimInfo::default();
        let mut count = 0u32;
        let mut valid = WebPAnimDecoderGetInfo(decoder, &mut info) != 0 && info.frame_count > 0;
        while valid && WebPAnimDecoderHasMoreFrames(decoder) != 0 {
            if count >= info.frame_count {
                valid = false;
                break;
            }
            let mut frame = std::ptr::null_mut();
            let mut timestamp = 0;
            valid = WebPAnimDecoderGetNext(decoder, &mut frame, &mut timestamp) != 0
                && !frame.is_null();
            if valid {
                count += 1;
            }
        }
        WebPAnimDecoderDelete(decoder);
        if !valid || count != info.frame_count || count as usize != frame_chunks {
            return Err(DirectError::new(
                DirectErrorKind::Malformed,
                "invalid animated WebP frame",
            ));
        }
        Ok(())
    }
}

fn count_frame_chunks(input: &[u8]) -> Result<usize, DirectError> {
    let bad = || {
        DirectError::new(
            DirectErrorKind::Malformed,
            "invalid animated WebP container",
        )
    };
    if input.len() < 12 || &input[..4] != b"RIFF" || &input[8..12] != b"WEBP" {
        return Err(bad());
    }
    let riff_size = u32::from_le_bytes(input[4..8].try_into().unwrap()) as usize;
    let end = riff_size
        .checked_add(8)
        .filter(|&n| n <= input.len() && n >= 12)
        .ok_or_else(bad)?;
    let mut pos = 12usize;
    let mut frames = 0usize;
    while pos < end {
        let header = input
            .get(pos..pos.checked_add(8).ok_or_else(bad)?)
            .filter(|_| pos + 8 <= end)
            .ok_or_else(bad)?;
        let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        if &header[..4] == b"ANMF" {
            frames = frames.checked_add(1).ok_or_else(bad)?;
        }
        pos = pos
            .checked_add(8)
            .and_then(|n| n.checked_add(size))
            .and_then(|n| n.checked_add(size & 1))
            .filter(|&n| n <= end)
            .ok_or_else(bad)?;
    }
    Ok(frames)
}
