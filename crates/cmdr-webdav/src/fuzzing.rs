//! Entry points for the fuzz targets in `fuzz/` (repo root).
//!
//! A WebDAV server writes every byte of a PROPFIND answer, and a listing hands
//! it to [`parse_multistatus`] (the transport decodes it lossily first, as here).
//! The fuzzer is looking for panics, runaway allocations, and hangs.

use crate::propfind::parse_multistatus;

/// Parses `body` as a PROPFIND `multistatus` answer.
pub fn propfind(body: &[u8]) {
    let _ = parse_multistatus(&String::from_utf8_lossy(body));
}
