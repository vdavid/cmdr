//! The archive backend, as this app sees it.
//!
//! The backend itself is the `cmdr-archive` crate: a `Volume` over a zip / tar /
//! 7z file, with no `tauri` anywhere in its dependency tree (enforced by the
//! `index-crate-isolation` check). This module re-exports it under its original
//! `crate::file_system::volume::backends::archive::…` path, the way
//! `file_system::volume` re-exports `cmdr_fs::volume`, so every call site here
//! reads the same as before the crate boundary existed.
//!
//! What stays on this side, and why it isn't in the crate:
//!
//! - **Routing and lifecycle.** `manager/archive_routing.rs` decides when to mint
//!   an `ArchiveVolume`, hands it the app's `VolumeHost`, and LRU-caps the
//!   registry. A backend never registers itself.
//! - **Driving edits.** `write_operations/archive_edit/` builds a `Changeset` and
//!   runs `mutator` with the real event sink, pause gate, and cancel intent.
//! - **The listing seam's app side.** `listing/streaming.rs` and `caching.rs`
//!   special-case archive listings; the backend only ever says a path changed.
//!
//! Editing the backend? Everything is in `crates/cmdr-archive/` — start at its
//! `CLAUDE.md`. `cargo check -p cmdr-archive` is a complete verification loop
//! there, with none of this app in it.

pub use cmdr_archive::*;

// The app's half of the archive live-content watch: what a refresh DOES to the
// listing cache. The backend's half (an on-disk edit reaching the refresh seam at
// all) lives with the watch itself, in `cmdr-archive`'s `watch/host_seam_test.rs`.
#[cfg(test)]
#[path = "archive_watch_integration_test.rs"]
mod watch_integration_test;
