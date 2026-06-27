//! Per-backend `Volume` implementations.
//!
//! Each submodule wraps a different storage system behind the `Volume` trait
//! defined in [`super`]. The trait lives in `volume/mod.rs`; the implementations
//! live here. New backends slot in alongside these without touching the trait.
//!
//! See [`super::CLAUDE.md`](../CLAUDE.md) for the trait shape and capability
//! matrix, and `backends/CLAUDE.md` for the per-backend decisions and gotchas
//! that drive each implementation here.

mod in_memory;
mod local_posix;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod mtp;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod smb;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_watcher;

pub use in_memory::InMemoryVolume;
pub use local_posix::LocalPosixVolume;
/// Cross-platform volume used-bytes helper (NSURL purgeable-aware on macOS,
/// `statvfs` on Linux). Re-exported so the indexing module can read the scanned
/// volume's used bytes for tier-2 scan progress without re-implementing statfs.
pub(crate) use local_posix::get_space_info_for_path;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use mtp::MtpVolume;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use smb::SmbVolume;

// Re-export shared `volume/` types so each backend submodule can keep using
// `super::Volume`, `super::VolumeError`, `super::MutationEvent`, etc. without
// having to spell `crate::file_system::volume::...` everywhere.
pub(crate) use super::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SmbConnectionState, SourceItemInfo,
    SpaceInfo, Volume, VolumeError, VolumeReadStream, VolumeScanner, VolumeWatcher,
};

#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod local_posix_test;
// `mtp_test` is gated on the same platforms as the `mtp` module it tests (the
// other two backends are cross-platform, so their test mods aren't gated).
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod mtp_test;
