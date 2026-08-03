// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These two can't go with them: `unused_crate_dependencies` is judged per
// compilation unit (as a package-wide flag every test target would report unused
// externs for deps only the lib uses), and `missing_docs` is this crate's own
// contract — its API is a deliverable rather than a side effect.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]
// This facade re-exports the reading core's full public surface for a uniform
// `cmdr_archive::` path, but some names are consumed only inside a submodule or by
// test code (the Zip Slip sanitizer surface, the in-memory `BytesSource`), so the
// workspace lint set's `deny(unused)` would flag those re-exports.
#![allow(
    unused_imports,
    reason = "facade re-exports the reading core's full surface; some names are consumed only within a submodule or in tests"
)]

//! The archive backend: presents a zip / tar / 7z file as a browsable, read-only
//! folder (zip is also writable).
//!
//! A storage backend as its own crate. [`ArchiveVolume`] implements
//! [`Volume`](cmdr_fs::volume::Volume) over an archive file that physically lives
//! on ANOTHER volume, and everything it can't answer itself — which panes are
//! open, which runtime to spawn a watcher task onto — arrives through the
//! [`VolumeHost`](cmdr_fs::volume::host::VolumeHost) it's constructed with. It
//! never registers itself, never names the application around it, and never
//! produces a word a human reads.
//!
//! Three layers, split so the reading engine is decoupled from the `Volume` trait:
//!
//! - [`read`] — the reading core: parse an archive's directory into a synthetic
//!   tree and stream-decompress entries, in archive-native types
//!   ([`ArchiveIndex`], [`ArchiveNode`], [`ArchiveError`]). Serves all formats.
//! - [`volume`] — [`ArchiveVolume`], the one module that maps the core onto
//!   `FileEntry` / `VolumeError` / a `VolumeReadStream` and holds the parent seam.
//! - [`mutator`] — the zip-only temp+rename write side, driven by the host's own
//!   write-operations layer rather than through `ArchiveVolume`'s mutation
//!   methods (which stay `NotSupported`).
//!
//! Around them: [`boundary`] (the routing detector a host uses to decide when to
//! mint an `ArchiveVolume`) and [`watch`] (the live content watch on the backing
//! file).
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the `ArchiveVolume`
//! layer, routing, and remote-backed archives; each submodule folder carries its
//! own `CLAUDE.md` / `DETAILS.md`.

//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the lib (see `Cargo.toml`). That makes `cmdr_archive` an extern crate of
// its own test target, which `unused_crate_dependencies` reports.
#[cfg(test)]
use cmdr_archive as _;
//noinspection RsUnusedImport
// The `.tar.zst` fixture encoder, used from `read/multiformat_test.rs`, which a
// partial test build may not compile; the marker keeps the lint quiet either way.
#[cfg(test)]
use zstd as _;

pub mod boundary;
pub mod read;
pub mod volume;
pub mod watch;

mod mutation;

/// Fixture builders for archive tests: clean zips, hostile byte-patched variants,
/// and encrypted zip / 7z archives, all built in memory with no blobs checked in.
///
/// Published under the `testing` feature so a HOST's own archive tests build
/// fixtures the way this crate's do, instead of growing a second set that drifts.
#[cfg(any(test, feature = "testing"))]
pub mod test_fixtures;

// `mutator` presents at the crate root — a host's archive-edit driver reaches it
// there — while it lives under `mutation/`.
pub use mutation::mutator;

pub use boundary::{
    ARCHIVE_MAGIC_PREFIX_LEN, archive_boundary_candidate, bytes_match_archive_magic, bytes_start_with_zip_signature,
    confirm_archive_boundary, has_supported_archive_extension, path_crosses_archive_boundary, path_is_inside_archive,
    path_targets_archive_file,
};
pub use read::{
    ArchiveByteSource, ArchiveEntryReader, ArchiveError, ArchiveFormat, ArchiveIndex, ArchiveIndexCache, ArchiveNode,
    BytesSource, DEFAULT_TAIL_CACHE_LEN, LocalFileSource, QuarantineReason, SanitizedName, SubtreeExtractReader,
    SubtreeMember, TailCachedSource, TarCodec, format_for_name, format_for_path, sanitize_entry_name,
};
pub use volume::ArchiveVolume;
pub use watch::active_watch_count;
