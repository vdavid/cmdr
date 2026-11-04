//! Two engines share nothing: each owns its listings, its diff queue, and its
//! background work, and dropping one lets it go.

use crate::RowSpan;
use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use super::cached_listing::CachedListing;
use super::engine::Listings;
use super::operations::ListingLookupError;
use super::pipeline::{OverlayRows, Prepared};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

const DIR: &str = "/two-engines";

fn entry(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
}

/// Puts a one-row listing under `listing_id` into `engine`'s cache.
fn insert_listing(engine: &Listings, listing_id: &str) {
    let inner = &engine.inner;
    let listing = CachedListing::new(
        inner,
        "root".to_string(),
        PathBuf::from(DIR),
        Prepared::for_test(vec![entry("a.txt")], OverlayRows::Recounted(0)),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    inner
        .cache
        .write_ignore_poison()
        .insert(listing_id.to_string(), listing);
}

#[test]
fn a_listing_in_one_engine_is_invisible_to_another() {
    let first = Listings::detached();
    let second = Listings::detached();
    insert_listing(&first, "shared-id");

    let rows = first
        .inner
        .get_file_range("shared-id", RowSpan { start: 0, count: 10 }, true)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        second
            .inner
            .get_file_range("shared-id", RowSpan { start: 0, count: 10 }, true),
        Err(ListingLookupError::Gone { .. })
    ));

    // A patch lands in the engine that holds the listing, and its diff queues
    // there: the other engine has neither. Held, so no flush races the read.
    first.inner.diffs.hold("shared-id");
    assert!(first.inner.insert_entry_sorted("shared-id", entry("b.txt")).is_some());
    assert!(second.inner.insert_entry_sorted("shared-id", entry("b.txt")).is_none());
    assert_eq!(first.inner.diffs.pending_batches("shared-id").len(), 1);
    assert!(second.inner.diffs.pending_batches("shared-id").is_empty());
}

#[test]
fn a_dropped_engine_is_gone_even_with_its_reaper_running() {
    let engine = Listings::detached();
    engine.start_orphan_reaper();
    let weak = Arc::downgrade(&engine.inner);

    drop(engine);

    // The reaper's loop holds the engine weakly, so nothing keeps it alive.
    assert!(weak.upgrade().is_none());
}
