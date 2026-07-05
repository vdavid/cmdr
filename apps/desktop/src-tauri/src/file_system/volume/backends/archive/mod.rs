//! Archive reading core (zip): parse a zip's central directory into a synthetic
//! directory tree, and stream-decompress individual entries — the read-only
//! foundation the `ArchiveVolume` backend is built on.
//!
//! This module is deliberately decoupled from the `Volume` trait: it deals in
//! archive-native types ([`ArchiveIndex`], [`ArchiveNode`], [`ArchiveError`]),
//! and the volume layer maps those onto `FileEntry` / `VolumeError` / a
//! `VolumeReadStream`. That keeps the reader unit-testable without any Tauri or
//! volume machinery.
//!
//! ## Shape
//!
//! - [`ArchiveByteSource`] is the byte-supply seam (sans-IO): [`LocalFileSource`]
//!   reads a local file now; a remote parent volume's ranged read implements the
//!   same trait later (remote-backed archives) with no change here.
//! - [`ArchiveIndex::parse`] drives rc-zip's central-directory state machine over
//!   a source, sanitizes every entry name ([Zip Slip](name) defense), and builds
//!   the tree.
//! - [`ArchiveIndex::open_read`] returns an [`ArchiveEntryReader`] that
//!   decompresses one entry chunk-by-chunk off the async executor.
//! - [`ArchiveIndexCache`] caches parsed indexes keyed by `(path, size, mtime)`.
//!
//! See [`CLAUDE.md`](CLAUDE.md) for the must-knows and [`DETAILS.md`](DETAILS.md)
//! for the design rationale (why rc-zip's sans-IO fsm and not `rc-zip-tokio`,
//! the Zip Slip guarantee, the cache key, off-executor decompression).

// A few of this module's public items are part of the reading core's API but
// aren't referenced outside their own submodule or the tests yet (the Zip Slip
// sanitizer surface, the in-memory `BytesSource`). `#![deny(unused)]` at the
// crate root would flag those re-exports; relax `unused_imports` here. The
// `ArchiveVolume` impl in `volume` now consumes the index / reader / source /
// cache / error surface, so the allow no longer blankets the whole module.
#![allow(
    unused_imports,
    reason = "Zip Slip sanitizer surface and the in-memory BytesSource are public API not yet consumed outside this module"
)]

mod boundary;
mod cache;
mod error;
mod index;
mod mutator;
mod name;
mod read;
mod source;
mod volume;
mod watch;

#[cfg(test)]
mod archive_test;
#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
mod watch_integration_test;

pub use boundary::{
    SUPPORTED_ARCHIVE_EXTENSIONS, archive_boundary_candidate, confirm_archive_boundary,
    has_supported_archive_extension, path_crosses_archive_boundary, path_is_inside_archive, path_targets_archive_file,
};
pub use cache::ArchiveIndexCache;
pub use error::ArchiveError;
pub use index::{ArchiveIndex, ArchiveNode};
pub use name::{QuarantineReason, SanitizedName, sanitize_entry_name};
pub use read::ArchiveEntryReader;
pub use source::{ArchiveByteSource, BytesSource, LocalFileSource};
pub use volume::ArchiveVolume;
pub use watch::active_watch_count;
