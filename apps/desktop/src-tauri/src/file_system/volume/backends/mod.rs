//! Per-backend `Volume` implementations.
//!
//! Each submodule wraps a different storage system behind the `Volume` trait,
//! which lives in `cmdr_fs::volume` and is re-exported by [`super`]. Only one
//! backend still lives IN the app: `local_posix`. The others are crates of their
//! own (`cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`,
//! `cmdr-mtp`) and every call site imports them by crate name.
//!
//! See [`super::CLAUDE.md`](../CLAUDE.md) for the trait shape and capability
//! matrix, and `backends/CLAUDE.md` for the per-backend decisions and gotchas
//! that drive each implementation here.

mod local_posix;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use cmdr_mtp::MtpVolume;
pub use local_posix::LocalPosixVolume;
/// Cross-platform volume used-bytes helper (NSURL purgeable-aware on macOS,
/// `statvfs` on Linux). Re-exported so the indexing module can read the scanned
/// volume's used bytes for tier-2 scan progress without re-implementing statfs.
pub(crate) use local_posix::get_space_info_for_path;
pub(crate) use local_posix::rename_local_exclusive;
pub(crate) use local_posix::rename_volume_error;

// Re-export shared `volume/` types so each backend submodule can keep using
// `super::Volume`, `super::VolumeError`, `super::MutationEvent`, etc. without
// having to spell `crate::file_system::volume::...` everywhere.
pub(crate) use super::{
    CopyScanResult, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream,
    WatchCoverage,
};

#[cfg(test)]
mod local_posix_test;
// The shared `volume::conformance` assertions LocalPosix runs, split out the way
// every other backend keeps its own (SMB's, SFTP's, and MTP's
// `volume::conformance_test`).
#[cfg(test)]
mod local_posix_conformance_test;
