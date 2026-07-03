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
//!   same trait later (M5) with no change here.
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

// This module is the read-only archive core. Its public types and re-exports
// are the API surface the `ArchiveVolume` `Volume` impl is built on next, so
// not everything here is called from crate code yet. `#![deny(unused)]` at the
// crate root would flag the not-yet-consumed re-exports; relax it here (the
// parent `volume` module already relaxes `dead_code` the same way).
#![allow(
    unused_imports,
    reason = "Public API surface consumed by the ArchiveVolume backend built on top of this module"
)]

mod cache;
mod error;
mod index;
mod name;
mod read;
mod source;

#[cfg(test)]
mod archive_test;
#[cfg(test)]
mod test_fixtures;

pub use cache::ArchiveIndexCache;
pub use error::ArchiveError;
pub use index::{ArchiveIndex, ArchiveNode};
pub use name::{QuarantineReason, SanitizedName, sanitize_entry_name};
pub use read::ArchiveEntryReader;
pub use source::{ArchiveByteSource, BytesSource, LocalFileSource};
