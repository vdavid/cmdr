//! Tests for the per-listing `directory-diff` coalescer.
//!
//! These exercise the in-memory buffer behavior only. The Tauri `emit` side is
//! a no-op without an `AppHandle`, which suits these tests fine — we're proving
//! that producers coalesce into one logical flush, not that Tauri routing works
//! (that's covered by the existing watcher integration tests).

use std::path::PathBuf;

use super::caching::{CachedListing, LISTING_CACHE};
use super::diff_emitter::{drop_pending, enqueue_diff, flush_now_for_test, pending_count};
use super::metadata::FileEntry;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::watcher::DiffChange;

fn install_listing(id: &str) {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.insert(
        id.to_string(),
        CachedListing {
            volume_id: "root".to_string(),
            path: PathBuf::from("/test"),
            entries: Vec::new(),
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::default(),
            sequence: std::sync::atomic::AtomicU64::new(0),
            created_at: std::time::Instant::now(),
            last_accessed_ms: std::sync::atomic::AtomicU64::new(0),
        },
    );
}

fn drop_listing(id: &str) {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.remove(id);
}

fn make_change(name: &str, index: usize) -> DiffChange {
    DiffChange {
        change_type: "remove".to_string(),
        entry: FileEntry::new(name.to_string(), format!("/test/{}", name), false, false),
        index,
    }
}

#[test]
fn enqueue_accumulates_changes_in_buffer() {
    let id = "diff-emitter-accumulate";
    install_listing(id);

    enqueue_diff(id, vec![make_change("a", 0)]);
    enqueue_diff(id, vec![make_change("b", 1), make_change("c", 2)]);
    enqueue_diff(id, vec![make_change("d", 3)]);

    assert_eq!(pending_count(id), 4, "all changes should buffer until flush");

    drop_pending(id);
    drop_listing(id);
}

#[test]
fn empty_changes_is_noop() {
    let id = "diff-emitter-empty";
    install_listing(id);

    enqueue_diff(id, Vec::new());
    assert_eq!(pending_count(id), 0);

    drop_listing(id);
}

#[test]
fn drop_pending_clears_buffer() {
    let id = "diff-emitter-drop";
    install_listing(id);

    enqueue_diff(id, vec![make_change("x", 0), make_change("y", 1)]);
    assert_eq!(pending_count(id), 2);

    drop_pending(id);
    assert_eq!(pending_count(id), 0);

    drop_listing(id);
}

#[test]
fn buffers_are_isolated_per_listing() {
    let a = "diff-emitter-iso-a";
    let b = "diff-emitter-iso-b";
    install_listing(a);
    install_listing(b);

    enqueue_diff(a, vec![make_change("x", 0)]);
    enqueue_diff(b, vec![make_change("y", 0), make_change("z", 1)]);

    assert_eq!(pending_count(a), 1);
    assert_eq!(pending_count(b), 2);

    drop_pending(a);
    drop_pending(b);
    drop_listing(a);
    drop_listing(b);
}

#[test]
fn flush_empties_buffer_and_re_arms_for_next_burst() {
    let id = "diff-emitter-flush";
    install_listing(id);

    enqueue_diff(id, vec![make_change("a", 0), make_change("b", 1)]);
    assert_eq!(pending_count(id), 2);

    // Flush is best-effort: no AppHandle in unit tests, so the emit is a no-op,
    // but the buffer must still be drained and re-armed.
    flush_now_for_test(id);
    assert_eq!(pending_count(id), 0);

    // A new enqueue after flush should accumulate again.
    enqueue_diff(id, vec![make_change("c", 0)]);
    assert_eq!(pending_count(id), 1);

    drop_pending(id);
    drop_listing(id);
}
