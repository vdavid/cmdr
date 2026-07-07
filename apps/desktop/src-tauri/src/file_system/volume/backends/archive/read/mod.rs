//! Archive reading core: parse an archive's directory into a synthetic tree, and
//! stream-decompress individual entries — the read-only foundation the
//! `ArchiveVolume` backend is built on. Serves zip, tar (+ gzip/xz/bzip2/zstd),
//! and 7z behind one tree + query surface; only parsing and the per-entry read
//! handle differ per format.
//!
//! This module is deliberately decoupled from the `Volume` trait: it deals in
//! archive-native types ([`ArchiveIndex`], [`ArchiveNode`], [`ArchiveError`]),
//! and the volume layer ([`super::volume`]) maps those onto `FileEntry` /
//! `VolumeError` / a `VolumeReadStream`. That keeps the reader unit-testable
//! without any Tauri or volume machinery.
//!
//! ## Shape
//!
//! - [`ArchiveByteSource`] is the byte-supply seam (sans-IO): [`LocalFileSource`]
//!   reads a local file; a remote parent volume's ranged read implements the same
//!   trait (remote-backed archives) with no change here.
//! - [`ArchiveIndex::parse`] drives each format's parser over a source, sanitizes
//!   every entry name ([Zip Slip](name) defense), and builds the tree.
//! - [`ArchiveIndex::open_read`] returns an [`ArchiveEntryReader`] that
//!   decompresses one entry chunk-by-chunk off the async executor.
//! - [`ArchiveIndexCache`] caches parsed indexes keyed by `(path, size, mtime)`.
//!
//! See [`CLAUDE.md`](CLAUDE.md) for the must-knows and [`DETAILS.md`](DETAILS.md)
//! for the design rationale (why rc-zip's sans-IO fsm and not `rc-zip-tokio`, the
//! Zip Slip guarantee, the cache key, off-executor decompression, the multi-format
//! seam and codecs).

// A few of this module's public items are part of the reading core's API but
// aren't referenced outside their own submodule or the tests yet (the Zip Slip
// sanitizer surface, the in-memory `BytesSource`). `#![deny(unused)]` at the
// crate root would flag those re-exports; relax `unused_imports` here.
#![allow(
    unused_imports,
    reason = "Zip Slip sanitizer surface and the in-memory BytesSource are public API not yet consumed outside this module"
)]

mod cache;
mod error;
mod format;
mod index;
mod name;
mod reader;
mod sevenz;
mod source;
mod tar;
mod zip;

#[cfg(test)]
mod archive_test;
#[cfg(test)]
mod multiformat_test;

pub use cache::ArchiveIndexCache;
pub use error::ArchiveError;
pub use format::{ArchiveFormat, TarCodec, format_for_name, format_for_path};
pub use index::{ArchiveIndex, ArchiveNode};
pub use name::{QuarantineReason, SanitizedName, sanitize_entry_name};
pub use reader::ArchiveEntryReader;
pub use source::{ArchiveByteSource, BytesSource, DEFAULT_TAIL_CACHE_LEN, LocalFileSource, TailCachedSource};
