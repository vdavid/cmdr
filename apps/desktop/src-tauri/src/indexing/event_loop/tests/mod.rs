//! Tests for the event loop, clustered onto the production seams:
//! - `merge`: `merge_fs_events` dedup/flag-priority, buffer overflow/mode, and
//!   replay-dedup tests (the event-buffer behavior).
//! - `rename`: inode rename pre-pass, removal-storm coalescing, and the
//!   `process_live_batch` end-to-end rename, plus their shared fixtures.
//! - `split_parent`: the `split_parent_and_name` pure-helper tests.
//!
//! Production items resolve through `use super::*` (this module's `super` is
//! `event_loop`, so the root's re-exports and imports — `watcher`,
//! `merge_fs_events`, `process_live_batch`, `store`, `IndexStore`,
//! `IndexPathSpace`, `Path`, the `storm`/`live` submodules — are in scope and
//! chain into the cluster files via their own `use super::*`). Items that moved
//! into submodules (`detect_renames_by_inode`, `split_parent_and_name`) and
//! indexing-level types (`EventReconciler`, `IndexWriter`) are imported
//! explicitly where used.

use super::*;

mod merge;
mod rename;
mod split_parent;

/// Shared across the `merge` and `rename` clusters.
fn make_event(path: &str, event_id: u64, flags: watcher::FsEventFlags) -> watcher::FsChangeEvent {
    watcher::FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags,
    }
}
