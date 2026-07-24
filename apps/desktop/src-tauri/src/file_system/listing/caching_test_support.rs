//! Test isolation for the process-global `LISTING_CACHE`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::caching::{CachedListing, LISTING_CACHE, epoch_millis_now};
use super::metadata::FileEntry;
use super::operations::list_directory_end;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::ignore_poison::RwLockIgnorePoison;

/// A `LISTING_CACHE` entry owned by ONE test, torn down on drop.
///
/// **Why this exists.** `cargo test` runs a crate's tests as threads in ONE
/// process, so `LISTING_CACHE` is shared by every listing test at once. Three
/// things go wrong without a guard: two tests that pick the same literal listing
/// id clobber each other's entries; a test whose assertion fails before its
/// manual `cache.remove(...)` leaks the entry into every later test's view of the
/// cache; and any test that asserts on cache-WIDE state (`find_listings_for_path`
/// counts, an orphan sweep) sees foreign listings. A UNIQUE id, teardown from
/// `Drop` (which runs on unwind, so a panicking test cleans up too), and a
/// unique path per cache-wide assertion fix all three.
///
/// Mirrors `indexing::tests::stress_test_helpers::TestInstanceGuard`, the same
/// pattern over `INDEX_REGISTRY`. Keep the guard on the stack: a `std::mem::forget`
/// or a clone that outlives the test defeats the whole thing.
///
/// Teardown goes through the production `list_directory_end`, so the entry, its
/// watcher, and any pending coalesced diff are released together.
pub(crate) struct TestListingGuard {
    listing_id: String,
}

impl TestListingGuard {
    /// Takes ownership of a listing id that production code created (a
    /// `list_directory_start_*` call), so the test doesn't hand-roll teardown.
    pub(crate) fn adopt(listing_id: impl Into<String>) -> Self {
        Self {
            listing_id: listing_id.into(),
        }
    }

    /// The unique listing id. Pass it wherever a test would have used a literal.
    pub(crate) fn id(&self) -> &str {
        &self.listing_id
    }

    /// Runs `f` against this test's `CachedListing` under the cache read lock.
    /// Panics if the entry is gone, which is the assertion a test wants anyway.
    pub(crate) fn with_listing<R>(&self, f: impl FnOnce(&CachedListing) -> R) -> R {
        let cache = LISTING_CACHE.read_ignore_poison();
        let listing = cache
            .get(&self.listing_id)
            .unwrap_or_else(|| panic!("listing `{}` is no longer cached", self.listing_id));
        f(listing)
    }

    /// This listing's cached entries.
    pub(crate) fn entries(&self) -> Vec<FileEntry> {
        self.with_listing(|listing| listing.entries.clone())
    }

    /// This listing's cached entry names, in cache order. The common assertion.
    pub(crate) fn entry_names(&self) -> Vec<String> {
        self.with_listing(|listing| listing.entries.iter().map(|e| e.name.clone()).collect())
    }

    /// Whether the entry is still in the cache. For tests that assert on teardown.
    pub(crate) fn is_cached(&self) -> bool {
        LISTING_CACHE.read_ignore_poison().contains_key(&self.listing_id)
    }
}

impl Drop for TestListingGuard {
    fn drop(&mut self) {
        list_directory_end(&self.listing_id);
    }
}

/// Builder for a test-owned `LISTING_CACHE` entry. Defaults to an empty listing
/// on `root` at `/test`, sorted Name / Ascending / LikeFiles.
///
/// `last_accessed_ms` defaults to NOW, matching a listing that a live pane just
/// touched. A stamp of 0 would make the fixture orphan-eligible under any other
/// test's `reap_orphaned_listings_at` sweep, which is exactly the cross-test
/// eviction this module exists to stop; the reaper's own tests set it explicitly.
pub(crate) struct TestListing {
    volume_id: String,
    path: PathBuf,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: DirectorySortMode,
    entries: Vec<FileEntry>,
    sequence: u64,
    last_accessed_ms: u64,
}

impl TestListing {
    pub(crate) fn new() -> Self {
        Self {
            volume_id: "root".to_string(),
            path: PathBuf::from("/test"),
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            entries: Vec::new(),
            sequence: 0,
            last_accessed_ms: epoch_millis_now(),
        }
    }

    pub(crate) fn volume(mut self, volume_id: &str) -> Self {
        self.volume_id = volume_id.to_string();
        self
    }

    pub(crate) fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = path.into();
        self
    }

    pub(crate) fn sort(mut self, by: SortColumn, order: SortOrder, mode: DirectorySortMode) -> Self {
        self.sort_by = by;
        self.sort_order = order;
        self.directory_sort_mode = mode;
        self
    }

    pub(crate) fn entries(mut self, entries: Vec<FileEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub(crate) fn sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    pub(crate) fn last_accessed_ms(mut self, last_accessed_ms: u64) -> Self {
        self.last_accessed_ms = last_accessed_ms;
        self
    }

    /// Inserts the listing under a unique id derived from `tag` and hands back the
    /// RAII guard. Bind it (`let listing = …`), never `let _ = …`: a `_` binding
    /// drops immediately and the entry is gone before the test runs.
    pub(crate) fn insert(self, tag: &str) -> TestListingGuard {
        let listing_id = unique_test_id(tag);
        LISTING_CACHE.write_ignore_poison().insert(
            listing_id.clone(),
            CachedListing {
                volume_id: self.volume_id,
                path: self.path,
                entries: self.entries,
                sort_by: self.sort_by,
                sort_order: self.sort_order,
                directory_sort_mode: self.directory_sort_mode,
                sequence: AtomicU64::new(self.sequence),
                created_at: Instant::now(),
                last_accessed_ms: AtomicU64::new(self.last_accessed_ms),
            },
        );
        TestListingGuard { listing_id }
    }
}

/// A process-unique key for a test-owned entry in any global map (a listing id, a
/// font id, an operation id). The counter alone would collide across
/// concurrently-running test binaries, so the pid goes in too.
pub(crate) fn unique_test_id(tag: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "test-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}
