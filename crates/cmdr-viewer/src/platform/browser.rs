//! The browser: no local files. A session there reads through its volume, and `File::open`
//! already refuses, so this is never reached with a real handle.

use std::fs::File;

/// A typed refusal, never a panic.
pub(crate) fn positioned_read(_file: &File, _buf: &mut [u8], _offset: u64) -> std::io::Result<usize> {
    Err(std::io::ErrorKind::Unsupported.into())
}
