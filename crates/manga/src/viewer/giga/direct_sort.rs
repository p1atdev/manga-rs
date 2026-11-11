//! Lossless GigaViewer unscrambling performed directly on JPEG DCT coefficients.
//!
//! [`permute_jpeg`] applies an arbitrary 4x4 tile permutation the same way. [`sort_jpeg`] swaps the 4x4 tile grid used by [`super::solver::Solver`] by moving whole 8x8
//! coefficient blocks, so the image is neither decoded to pixels nor requantized: quantization
//! tables and every coefficient are carried over unchanged (the Huffman tables are re-optimized,
//! which changes the bytes but not the decoded pixels).
//!
//! All libjpeg calls, including error handling via `setjmp`/`longjmp`, live in `csrc/direct_sort.c`;
//! no unwinding or `longjmp` ever crosses the FFI boundary.
//!
//! # Supported input
//!
//! - 8-bit, baseline (SOF0), Huffman-coded, sequential JPEG.
//! - Grayscale, or 3-component YCbCr with 4:4:4 sampling (no chroma subsampling).
//! - Metadata limited to one JFIF APP0 without thumbnail and a complete ICC profile (APP2).
//!   Both are preserved. Any other segment (EXIF, XMP, comments, Adobe, thumbnails, ...) is
//!   rejected as [`DirectSortErrorKind::Unsupported`] rather than copied, because stale
//!   orientation or thumbnails would be wrong after the tile swap.
//! - No bytes after EOI.
//!
//! # Behaviour
//!
//! - Tile size matches `Solver`: `dim / 32` blocks per cell. Blocks outside the 4x4 grid, such as a
//!   partial right or bottom block, are left untouched.
//! - Any libjpeg warning (for example truncation) is an error: [`DirectSortErrorKind::Malformed`].
//! - Representational limits, libjpeg constraints and allocation failures are reported as
//!   [`DirectSortErrorKind::LimitExceeded`].
//!
//! Every failure, [`DirectSortErrorKind::Unsupported`] included, is returned as an error. Nothing
//! is converted or re-encoded here; callers must hold and report failures.

use std::{
    ffi::{c_char, c_int},
    fmt,
};

// Forces linking of the bundled libjpeg that the C shim calls into.
extern crate mozjpeg_sys;

const DSJ_OK: c_int = 0;
const DSJ_UNSUPPORTED: c_int = 1;
const DSJ_MALFORMED: c_int = 2;
const DSJ_LIMIT: c_int = 3;
const DSJ_INVALID_ARG: c_int = 5;

unsafe extern "C" {
    fn dsj_sort_jpeg(
        input: *const u8,
        input_len: usize,
        output: *mut *mut u8,
        output_len: *mut usize,
        msg: *mut c_char,
        msg_cap: usize,
    ) -> c_int;
    fn dsj_permute_jpeg(
        input: *const u8,
        input_len: usize,
        tile_width: u32,
        tile_height: u32,
        permutation: *const u8,
        permutation_len: usize,
        output: *mut *mut u8,
        output_len: *mut usize,
        msg: *mut c_char,
        msg_cap: usize,
    ) -> c_int;
    fn dsj_free(ptr: *mut u8);
}

/// Why [`sort_jpeg`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectSortErrorKind {
    /// Not corrupt, but outside the supported subset. Reported like any other error.
    Unsupported,
    /// Corrupt, truncated, empty or otherwise invalid JPEG.
    Malformed,
    /// A representation or libjpeg limit was exceeded, or an allocation failed.
    LimitExceeded,
    /// Unexpected failure inside the shim.
    Internal,
    /// Caller error in [`permute_jpeg`]: zero tile size, bad permutation, or a grid larger than
    /// the image.
    InvalidArgument,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectSortError {
    kind: DirectSortErrorKind,
    message: String,
}

impl DirectSortError {
    pub fn kind(&self) -> DirectSortErrorKind {
        self.kind
    }

    /// True when the input is valid-looking but outside the supported subset.
    pub fn is_unsupported(&self) -> bool {
        self.kind == DirectSortErrorKind::Unsupported
    }
}

impl fmt::Display for DirectSortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.kind {
            DirectSortErrorKind::Unsupported => "unsupported JPEG",
            DirectSortErrorKind::Malformed => "malformed JPEG",
            DirectSortErrorKind::LimitExceeded => "JPEG limit exceeded",
            DirectSortErrorKind::Internal => "internal JPEG error",
            DirectSortErrorKind::InvalidArgument => "invalid argument",
        };
        write!(f, "{label}: {}", self.message)
    }
}

impl std::error::Error for DirectSortError {}

/// Runs `call` with fresh output slots and converts the shim result into bytes or an error.
fn run_shim(
    call: impl FnOnce(*mut *mut u8, *mut usize, *mut c_char, usize) -> c_int,
) -> Result<Vec<u8>, DirectSortError> {
    let mut out: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let mut msg = [0 as c_char; 256];

    let status = call(&mut out, &mut out_len, msg.as_mut_ptr(), msg.len());

    if status == DSJ_OK {
        if out.is_null() {
            return Err(error(
                DirectSortErrorKind::Internal,
                "shim returned no output",
            ));
        }
        if out_len > isize::MAX as usize {
            unsafe { dsj_free(out) };
            return Err(error(
                DirectSortErrorKind::LimitExceeded,
                "output exceeds addressable size",
            ));
        }
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(out_len).is_err() {
            unsafe { dsj_free(out) };
            return Err(error(DirectSortErrorKind::LimitExceeded, "out of memory"));
        }
        // SAFETY: on success `out` points to `out_len` initialized bytes allocated by the shim;
        // the length fits isize and its allocation is released exactly once below.
        bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(out, out_len) });
        unsafe { dsj_free(out) };
        return Ok(bytes);
    }

    let text: Vec<u8> = msg
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    let kind = match status {
        DSJ_UNSUPPORTED => DirectSortErrorKind::Unsupported,
        DSJ_MALFORMED => DirectSortErrorKind::Malformed,
        DSJ_LIMIT => DirectSortErrorKind::LimitExceeded,
        DSJ_INVALID_ARG => DirectSortErrorKind::InvalidArgument,
        _ => DirectSortErrorKind::Internal,
    };
    Err(error(kind, &String::from_utf8_lossy(&text)))
}

/// Unscrambles a GigaViewer JPEG without requantizing. See the module docs for limitations.
pub fn sort_jpeg(input: &[u8]) -> Result<Vec<u8>, DirectSortError> {
    // SAFETY: `input` is valid for `input_len` bytes; the out pointers and `msg` come from
    // `run_shim` and are valid for writes. The shim never retains these pointers.
    run_shim(|out, out_len, msg, msg_cap| unsafe {
        dsj_sort_jpeg(input.as_ptr(), input.len(), out, out_len, msg, msg_cap)
    })
}

/// Permutes the 4x4 tile grid of a JPEG without requantizing, under the same input policy as
/// [`sort_jpeg`].
///
/// Tiles are `tile_width` x `tile_height` pixels, anchored at the top-left corner; pixels right of
/// or below the grid are left untouched. `source_by_destination[d]` is the row-major source tile
/// copied into row-major destination tile `d`; it must be a bijection on `0..16`.
///
/// Errors: [`DirectSortErrorKind::InvalidArgument`] for zero tile sizes, a permutation that is not
/// 16 distinct entries below 16, or a grid larger than the image;
/// [`DirectSortErrorKind::Unsupported`] when a tile size is not a multiple of 8.
pub fn permute_jpeg(
    input: &[u8],
    tile_width: u32,
    tile_height: u32,
    source_by_destination: &[usize],
) -> Result<Vec<u8>, DirectSortError> {
    let mut perm = [0u8; 16];
    if source_by_destination.len() != perm.len() {
        return Err(error(
            DirectSortErrorKind::InvalidArgument,
            "permutation must have exactly 16 entries",
        ));
    }
    for (slot, &source) in perm.iter_mut().zip(source_by_destination) {
        *slot = u8::try_from(source).map_err(|_| {
            error(
                DirectSortErrorKind::InvalidArgument,
                "permutation entry out of range",
            )
        })?;
    }

    // SAFETY: as in `sort_jpeg`; `perm` is valid for `perm.len()` bytes.
    run_shim(|out, out_len, msg, msg_cap| unsafe {
        dsj_permute_jpeg(
            input.as_ptr(),
            input.len(),
            tile_width,
            tile_height,
            perm.as_ptr(),
            perm.len(),
            out,
            out_len,
            msg,
            msg_cap,
        )
    })
}

fn error(kind: DirectSortErrorKind, message: &str) -> DirectSortError {
    DirectSortError {
        kind,
        message: message.to_owned(),
    }
}
