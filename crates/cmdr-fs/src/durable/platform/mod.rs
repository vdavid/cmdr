//! Picks this OS's file and device syncs: macOS splits "hand it to the drive"
//! (`fsync`) from "empty the drive's cache" (`F_FULLFSYNC`); everywhere else one
//! sync does both.

use std::fs::File;
use std::io;

#[cfg(any(target_os = "linux", target_os = "windows", target_family = "wasm"))]
mod one_sync;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_family = "wasm", path = "wasm.rs")]
mod imp;

const _: fn(&File) -> io::Result<()> = sync_file_data;
const _: fn(&File) -> io::Result<()> = sync_to_drive;
const _: fn(&File) -> io::Result<()> = sync_dir_to_drive;
const _: fn(&File) -> io::Result<()> = device_barrier;
const _: bool = NEEDS_DEVICE_BARRIER;
const _: fn(&File) -> bool = defers_data_sync;
const _: fn(&File) -> io::Result<()> = sync_volume;

/// One file's data, all the way to the platter: the full sync for a writer of
/// one file.
pub(super) use imp::sync_file_data;

/// One file's bytes handed to its drive, which may still hold them in its cache.
pub(super) use imp::sync_to_drive;

/// [`sync_to_drive`] for an open directory's entries.
pub(super) use imp::sync_dir_to_drive;

/// The drive holding `file` empties its write cache; `Ok` where the filesystem
/// has none to offer.
pub(super) use imp::device_barrier;

/// Whether [`sync_to_drive`] stops short of the drive's cache, so a barrier is
/// owed after it.
pub(super) use imp::NEEDS_DEVICE_BARRIER;

/// Whether a written file's [`sync_to_drive`] can wait for the next pass: only
/// where a barrier follows it anyway, and only on a local volume.
pub(super) use imp::defers_data_sync;

/// Every write on the volume holding `file` handed to its drive, waited for.
/// Reports no write error (macOS discards it): a catch-all, never the answer.
pub(super) use imp::sync_volume;
