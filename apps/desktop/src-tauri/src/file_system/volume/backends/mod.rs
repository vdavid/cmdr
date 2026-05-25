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
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use mtp::MtpVolume;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use smb::SmbVolume;

// Re-export shared `volume/` types so each backend submodule can keep using
// `super::Volume`, `super::VolumeError`, `super::MutationEvent`, etc. without
// having to spell `crate::file_system::volume::...` everywhere.
pub(crate) use super::{
    BatchScanResult, CopyScanResult, MutationEvent, ScanConflict, SmbConnectionState, SourceItemInfo, SpaceInfo,
    Volume, VolumeError, VolumeReadStream, VolumeScanner, VolumeWatcher,
};

#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod local_posix_test;
