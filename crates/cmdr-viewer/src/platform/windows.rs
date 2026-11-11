//! Windows: `ReadFile` with the offset in its `OVERLAPPED`, through std's `seek_read`.

use std::fs::File;

/// Unlike `pread`, this also moves the handle's cursor, which nothing here reads: every read
/// of a viewer handle names its own offset, so concurrent readers still never disturb each other.
pub(crate) fn positioned_read(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}
