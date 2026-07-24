//! Tests for the per-listing `directory-diff` coalescer.
//!
//! These exercise the in-memory buffer behavior only. The Tauri `emit` side is
//! a no-op without an `AppHandle`, which suits these tests fine — we're proving
//! that producers coalesce into one logical flush, not that Tauri routing works
//! (that's covered by the existing watcher integration tests).

use super::caching_test_support::{TestListing, TestListingGuard};
use super::diff_emitter::{drop_pending, enqueue_diff, flush_now_for_test, pending_count};
use super::metadata::FileEntry;
use crate::file_system::watcher::DiffChange;

/// A cached listing under a unique id. Dropping the guard runs the production
/// teardown, which also drops the listing's pending diff buffer.
fn install_listing(tag: &str) -> TestListingGuard {
    TestListing::new().insert(tag)
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
    let listing = install_listing("diff-emitter-accumulate");

    enqueue_diff(listing.id(), vec![make_change("a", 0)]);
    enqueue_diff(listing.id(), vec![make_change("b", 1), make_change("c", 2)]);
    enqueue_diff(listing.id(), vec![make_change("d", 3)]);

    assert_eq!(pending_count(listing.id()), 4, "all changes should buffer until flush");
}

#[test]
fn empty_changes_is_noop() {
    let listing = install_listing("diff-emitter-empty");

    enqueue_diff(listing.id(), Vec::new());
    assert_eq!(pending_count(listing.id()), 0);
}

#[test]
fn drop_pending_clears_buffer() {
    let listing = install_listing("diff-emitter-drop");

    enqueue_diff(listing.id(), vec![make_change("x", 0), make_change("y", 1)]);
    assert_eq!(pending_count(listing.id()), 2);

    drop_pending(listing.id());
    assert_eq!(pending_count(listing.id()), 0);
}

#[test]
fn buffers_are_isolated_per_listing() {
    let a = install_listing("diff-emitter-iso-a");
    let b = install_listing("diff-emitter-iso-b");

    enqueue_diff(a.id(), vec![make_change("x", 0)]);
    enqueue_diff(b.id(), vec![make_change("y", 0), make_change("z", 1)]);

    assert_eq!(pending_count(a.id()), 1);
    assert_eq!(pending_count(b.id()), 2);
}

#[test]
fn flush_empties_buffer_and_re_arms_for_next_burst() {
    let listing = install_listing("diff-emitter-flush");

    enqueue_diff(listing.id(), vec![make_change("a", 0), make_change("b", 1)]);
    assert_eq!(pending_count(listing.id()), 2);

    // Flush is best-effort: no AppHandle in unit tests, so the emit is a no-op,
    // but the buffer must still be drained and re-armed.
    flush_now_for_test(listing.id());
    assert_eq!(pending_count(listing.id()), 0);

    // A new enqueue after flush should accumulate again.
    enqueue_diff(listing.id(), vec![make_change("c", 0)]);
    assert_eq!(pending_count(listing.id()), 1);
}
