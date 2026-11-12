//! macOS: `fsync(2)` stops at the drive's cache, and `F_FULLFSYNC` (std's
//! `sync_data` and `sync_all`) is the device barrier past it, ~4 ms on an
//! internal SSD and tens of ms on a spinning disk.

use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd as _;

pub(crate) const NEEDS_DEVICE_BARRIER: bool = true;

/// `F_FULLFSYNC`, falling back to `fsync(2)` where the filesystem doesn't offer
/// it: an SMB mount of a Samba share answers `ENOTSUP` (verified against the
/// Docker guest share, macOS 27.0, 2026-10-11). SQLite falls back the same way.
pub(crate) fn sync_file_data(file: &File) -> io::Result<()> {
    match file.sync_data() {
        Err(e) if e.raw_os_error() == Some(libc::ENOTSUP) => plain_fsync(file),
        other => other,
    }
}

pub(crate) fn sync_to_drive(file: &File) -> io::Result<()> {
    plain_fsync(file)
}

pub(crate) fn sync_dir_to_drive(dir: &File) -> io::Result<()> {
    plain_fsync(dir)
}

/// `F_FULLFSYNC`, with the unsupported class (an SMB mount's `ENOTSUP`) as
/// `Ok`: each item already had its `fsync`, which on a network mount is the
/// server's flush.
pub(crate) fn device_barrier(file: &File) -> io::Result<()> {
    // SAFETY: `file` is open for the whole call, so its fd is valid;
    // `F_FULLFSYNC` takes no argument and touches nothing but that fd.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == 0 {
        return Ok(());
    }
    let e = io::Error::last_os_error();
    if super::super::is_sync_unsupported(&e) {
        Ok(())
    } else {
        Err(e)
    }
}

fn plain_fsync(file: &File) -> io::Result<()> {
    // SAFETY: `file` is open for the whole call, so its fd is valid; `fsync`
    // touches nothing but that fd.
    if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
