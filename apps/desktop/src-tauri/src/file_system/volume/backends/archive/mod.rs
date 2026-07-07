//! The archive backend: presents a zip / tar / 7z file as a browsable, read-only
//! folder (zip is also writable), minted on demand when a path crosses a `.zip`
//! (or other supported archive) boundary.
//!
//! Two layers, split so the reading engine is decoupled from Tauri and the
//! `Volume` trait:
//!
//! - [`read`] — the reading core: parse an archive's directory into a synthetic
//!   tree and stream-decompress entries, in archive-native types
//!   ([`ArchiveIndex`], [`ArchiveNode`], [`ArchiveError`]). Serves all formats.
//! - [`volume`] — [`ArchiveVolume`], the one file that maps the core onto
//!   `FileEntry` / `VolumeError` / a `VolumeReadStream` and holds the parent seam.
//!
//! Around them: [`boundary`] (the routing detector `VolumeManager::resolve` uses
//! to decide when to mint an `ArchiveVolume`), [`watch`] (the live content watch
//! on the backing `.zip`), and [`mutation`] (the zip-only temp+rename write side).
//!
//! See [`CLAUDE.md`](CLAUDE.md) for the must-knows and [`DETAILS.md`](DETAILS.md)
//! for the `ArchiveVolume` layer, routing, and remote-backed archives; each
//! subfolder carries its own `CLAUDE.md` / `DETAILS.md`.

// This facade re-exports the reading core's full public surface for a uniform
// `archive::` path, but some names are consumed only inside a subfolder or by
// `#[cfg(test)]` code (e.g. `active_watch_count`, the Zip Slip sanitizer surface),
// so `#![deny(unused)]` at the crate root would flag those re-exports.
#![allow(
    unused_imports,
    reason = "facade re-exports the reading core's full surface; some names are consumed only within a subfolder or in tests"
)]

mod boundary;
mod mutation;
mod read;
mod volume;
mod watch;

// Shared zip-fixture builders for the subfolders' tests (read, mutation, watch,
// and the `ArchiveVolume` layer). Lives at the archive root so every descendant
// test module can reach it.
#[cfg(test)]
mod test_fixtures;

// `mutator` presents at this level (`archive::mutator`) — the write-ops
// archive-edit driver reaches it there — while it lives under `mutation/`.
pub(crate) use mutation::mutator;

pub use boundary::{
    ARCHIVE_MAGIC_PREFIX_LEN, archive_boundary_candidate, bytes_match_archive_magic, confirm_archive_boundary,
    has_supported_archive_extension, path_crosses_archive_boundary, path_is_inside_archive, path_targets_archive_file,
};
pub use read::{
    ArchiveByteSource, ArchiveEntryReader, ArchiveError, ArchiveFormat, ArchiveIndex, ArchiveIndexCache, ArchiveNode,
    BytesSource, DEFAULT_TAIL_CACHE_LEN, LocalFileSource, QuarantineReason, SanitizedName, TailCachedSource, TarCodec,
    format_for_name, format_for_path, sanitize_entry_name,
};
pub use volume::ArchiveVolume;
pub use watch::active_watch_count;
