//! The OSes where one sync reaches the platter: Linux's `fdatasync` and `fsync`
//! flush the drive's cache, Windows' `FlushFileBuffers` does too, and the
//! browser build has no disk. So the per-item sync IS the barrier, and the
//! barrier itself has nothing left to do.

use std::fs::File;
use std::io;

pub(crate) const NEEDS_DEVICE_BARRIER: bool = false;

pub(crate) fn sync_file_data(file: &File) -> io::Result<()> {
    file.sync_data()
}

pub(crate) fn sync_to_drive(file: &File) -> io::Result<()> {
    file.sync_data()
}

pub(crate) fn sync_dir_to_drive(dir: &File) -> io::Result<()> {
    dir.sync_all()
}

pub(crate) fn device_barrier(_file: &File) -> io::Result<()> {
    Ok(())
}
