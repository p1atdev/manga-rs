//! Structural completeness check independent of the JPEG decoder's truncated-input recovery.
use super::{DirectError, DirectErrorKind};

fn malformed() -> DirectError {
    DirectError::new(
        DirectErrorKind::Malformed,
        "incomplete or invalid JPEG marker structure",
    )
}

/// Walk marker segments and entropy data, including escaped FF bytes, restart markers, multiple
/// progressive scans and DNL in a scan. Segment contents are opaque, so marker-looking metadata
/// does not count as an end marker. The decoder performs the remaining JPEG validation.
pub(super) fn validate_complete(bytes: &[u8]) -> Result<(), DirectError> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(malformed());
    }
    let mut pos = 2usize;
    let mut in_scan = false;
    let mut saw_scan = false;
    loop {
        if in_scan {
            while pos < bytes.len() && bytes[pos] != 0xff {
                pos += 1;
            }
        } else if bytes.get(pos) != Some(&0xff) {
            return Err(malformed());
        }
        if pos >= bytes.len() {
            return Err(malformed());
        }
        while bytes.get(pos) == Some(&0xff) {
            pos += 1;
        }
        let marker = *bytes.get(pos).ok_or_else(malformed)?;
        pos += 1;
        match marker {
            0x00 if in_scan => continue,
            0xd0..=0xd7 if in_scan => continue,
            0x01 if in_scan => continue,
            0xd9 if saw_scan => return Ok(()),
            0x00 | 0xd8 | 0xd9 | 0xd0..=0xd7 => return Err(malformed()),
            0x01 => continue,
            _ => {}
        }
        let length_bytes = bytes
            .get(pos..pos.saturating_add(2))
            .ok_or_else(malformed)?;
        let len = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
        if len < 2 {
            return Err(malformed());
        }
        pos = pos
            .checked_add(len)
            .filter(|&end| end <= bytes.len())
            .ok_or_else(malformed)?;
        if marker == 0xda {
            saw_scan = true;
            in_scan = true;
        } else if marker != 0xdc || !in_scan {
            in_scan = false;
        }
    }
}

// Link mozjpeg even when the tile-restoration feature is disabled.
extern crate mozjpeg_sys;

unsafe extern "C" {
    fn dsj_validate_jpeg(
        input: *const u8,
        len: usize,
        message: *mut std::ffi::c_char,
        capacity: usize,
    ) -> std::ffi::c_int;
}

/// Fully decode without accepting libjpeg warnings or changing the source bytes.
pub(super) fn validate_decode(bytes: &[u8]) -> Result<(), DirectError> {
    let mut message = [0 as std::ffi::c_char; 256];
    // SAFETY: Input and message buffers remain valid and the C shim catches all libjpeg jumps.
    let status = unsafe {
        dsj_validate_jpeg(
            bytes.as_ptr(),
            bytes.len(),
            message.as_mut_ptr(),
            message.len(),
        )
    };
    if status == 0 {
        return Ok(());
    }
    let kind = match status {
        1 => DirectErrorKind::Unsupported,
        2 => DirectErrorKind::Malformed,
        3 => DirectErrorKind::LimitExceeded,
        _ => DirectErrorKind::Internal,
    };
    let text: Vec<u8> = message
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    Err(DirectError::new(
        kind,
        format!("invalid JPEG data: {}", String::from_utf8_lossy(&text)),
    ))
}

#[cfg(test)]
mod tests {
    use super::validate_complete;

    #[test]
    fn handles_stuffed_bytes_restart_dnl_and_multiple_scans() {
        let jpeg = [
            0xff, 0xd8, // SOI
            0xff, 0xe1, 0, 6, 0xff, 0xd9, 0xff, 0xda, // opaque metadata
            0xff, 0xda, 0, 2, 0x12, 0xff, 0x00, 0xd9, 0xff, 0xd0, 0x34, 0xff, 0xdc, 0, 4, 0, 5,
            0x56, // DNL inside scan
            0xff, 0xc4, 0, 2, // table between scans
            0xff, 0xda, 0, 2, 0x78, 0xff, 0xd9,
        ];
        assert!(validate_complete(&jpeg).is_ok());
        assert!(validate_complete(&jpeg[..jpeg.len() - 2]).is_err());
        let mut truncated_segment = jpeg.to_vec();
        truncated_segment[4] = 0xff;
        assert!(validate_complete(&truncated_segment).is_err());
    }
}
