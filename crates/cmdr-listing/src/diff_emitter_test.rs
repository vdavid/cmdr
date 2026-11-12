//! Tests for the per-listing `directory-diff` coalescer.
//!
//! These exercise the in-memory buffer only: a flush hands the batches to the
//! test engine's recorder, and what matters here is that producers coalesce
//! into one logical flush, not where the flush goes.

use super::testing::{TestEngine, TestListing, TestListingGuard, pending_changes};
use crate::DiffChange;
use cmdr_fs::entry::FileEntry;

/// A cached listing in an engine of its own, its diffs held until the test
/// flushes them.
fn install_listing(tag: &str) -> TestListingGuard {
    let listing = TestListing::new().insert(&TestEngine::new(), tag);
    listing.engine().inner.diffs.hold(listing.id());
    listing
}

fn enqueue_diff(listing: &TestListingGuard, changes: Vec<DiffChange>) {
    listing.engine().inner.write_cache().publish().enqueue(
        listing.id(),
        super::diff::DirectoryDiffBatch {
            from_sequence: 0,
            sequence: 1,
            total_count: 0,
            hidden_count: 0,
            changes,
            effective_sort_by: None,
        },
    );
}

fn make_change(name: &str, index: usize) -> DiffChange {
    DiffChange::removed(
        FileEntry::new(name.to_string(), format!("/test/{}", name), false, false),
        index,
    )
}

#[test]
fn enqueue_accumulates_changes_in_buffer() {
    let listing = install_listing("diff-emitter-accumulate");

    enqueue_diff(&listing, vec![make_change("a", 0)]);
    enqueue_diff(&listing, vec![make_change("b", 1), make_change("c", 2)]);
    enqueue_diff(&listing, vec![make_change("d", 3)]);

    assert_eq!(
        pending_changes(listing.engine(), listing.id()).len(),
        4,
        "all changes should buffer until flush"
    );
}

#[test]
fn empty_changes_is_noop() {
    let listing = install_listing("diff-emitter-empty");

    enqueue_diff(&listing, Vec::new());
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 0);
}

#[test]
fn drop_pending_clears_buffer() {
    let listing = install_listing("diff-emitter-drop");

    enqueue_diff(&listing, vec![make_change("x", 0), make_change("y", 1)]);
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 2);

    listing
        .engine()
        .inner
        .write_cache()
        .publish()
        .drop_pending(listing.id());
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 0);
}

#[test]
fn buffers_are_isolated_per_listing() {
    let a = install_listing("diff-emitter-iso-a");
    let b = install_listing("diff-emitter-iso-b");

    enqueue_diff(&a, vec![make_change("x", 0)]);
    enqueue_diff(&b, vec![make_change("y", 0), make_change("z", 1)]);

    assert_eq!(pending_changes(a.engine(), a.id()).len(), 1);
    assert_eq!(pending_changes(b.engine(), b.id()).len(), 2);
}

#[test]
fn flush_empties_buffer_and_re_arms_for_next_burst() {
    let listing = install_listing("diff-emitter-flush");

    enqueue_diff(&listing, vec![make_change("a", 0), make_change("b", 1)]);
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 2);

    // The flush drains the buffer and re-arms it.
    listing.engine().inner.diffs.flush(listing.id());
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 0);

    // A new enqueue after flush should accumulate again.
    listing.engine().inner.diffs.hold(listing.id());
    enqueue_diff(&listing, vec![make_change("c", 0)]);
    assert_eq!(pending_changes(listing.engine(), listing.id()).len(), 1);
}
