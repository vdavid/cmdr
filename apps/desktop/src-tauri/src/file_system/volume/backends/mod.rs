//! Per-backend `Volume` implementations.
//!
//! Each submodule wraps a different storage system behind the `Volume` trait
//! defined in [`super`]. The trait lives in `volume/mod.rs`; the implementations
//! live here. New backends slot in alongside these without touching the trait.
//!
//! See [`super::CLAUDE.md`](../CLAUDE.md) for the trait shape and capability
//! matrix, and `backends/CLAUDE.md` for the per-backend decisions and gotchas
//! that drive each implementation here.

// `InMemoryVolume` and its read stream are test-only scaffolding, and the archive
// reading core carries a few accessors (`ArchiveIndex::has_encrypted_entries`,
// `ArchiveEntryReader::bytes_read`, `BytesSource`, …) that only its own tests call.
// `#![deny(unused)]` at the crate root would flag both against a non-test build.
// Scoped to `backends`: the trait, its types, the manager, eject, and
// `friendly_error` are all fully live, so don't widen this back up the tree.
#![allow(dead_code, reason = "Test-only backends and archive-core accessors")]

// Archive reading core (zip). Cross-platform (pure Rust), so it isn't gated
// like the mtp/smb backends. The `ArchiveVolume` `Volume` impl is built on top
// of this.
pub mod archive;
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
pub(crate) use local_posix::rename_local_exclusive;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use mtp::MtpVolume;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use smb::SmbVolume;

// Re-export shared `volume/` types so each backend submodule can keep using
// `super::Volume`, `super::VolumeError`, `super::MutationEvent`, etc. without
// having to spell `crate::file_system::volume::...` everywhere.
pub(crate) use super::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SmbConnectionState, SourceItemInfo,
    SpaceInfo, Volume, VolumeError, VolumeReadStream,
};

#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod local_posix_test;
// `mtp_test` is gated on the same platforms as the `mtp` module it tests (the
// other two backends are cross-platform, so their test mods aren't gated).
// `mtp_archive_test` also needs the `virtual-mtp` feature (every test in it runs
// against a virtual MTP device), so it carries that gate on top.
#[cfg(all(test, any(target_os = "macos", target_os = "linux"), feature = "virtual-mtp"))]
mod mtp_archive_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod mtp_read_bench;
// `mtp_read_range_test` drives every test against a virtual MTP device, so it
// carries the `virtual-mtp` gate like `mtp_archive_test`.
#[cfg(all(test, any(target_os = "macos", target_os = "linux"), feature = "virtual-mtp"))]
mod mtp_read_range_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod mtp_test;
