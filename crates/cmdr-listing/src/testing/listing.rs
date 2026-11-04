//! A listing a test puts straight into an engine's cache, and the handle that
//! reads it back and ends it.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::testing::unique_test_id;

use crate::Listings;
use crate::cached_listing::CachedListing;
use crate::listing_id::ListingId;
use crate::pipeline::{ListedDates, OverlayRows, Prepared};
use cmdr_fs::volume::SupportedDates;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

/// A listing a test put into `engine`, ended on drop.
///
/// A test that builds its own engine shares nothing, so the end is tidiness. A
/// test on the host's ONE installed engine shares it with every other test in
/// the binary, so there the unique id ([`TestListing::insert`] derives one) and
/// the end on drop (which runs on unwind too) are what keep two tests from
/// seeing each other's listings. Keep it bound (`let listing = …`), never
/// `let _ = …`, which ends it at once.
pub struct TestListingGuard {
    engine: Listings,
    listing_id: String,
}

impl TestListingGuard {
    /// Takes ownership of `listing_id` in `engine`, a listing a read opened
    /// rather than one this fixture inserted, so the test doesn't end it by hand.
    pub fn adopt(engine: &Listings, listing_id: impl Into<String>) -> Self {
        Self {
            engine: engine.clone(),
            listing_id: listing_id.into(),
        }
    }

    /// The listing id. Pass it wherever a test would have used a literal.
    pub fn id(&self) -> &str {
        &self.listing_id
    }

    /// The listing id, as the public API takes it.
    pub fn listing_id(&self) -> ListingId {
        ListingId::new(&self.listing_id)
    }

    /// The engine holding this listing.
    pub fn engine(&self) -> &Listings {
        &self.engine
    }

    /// Runs `f` against this listing under the cache read lock. Panics if the
    /// listing is gone, which is the assertion a test wants anyway.
    pub(crate) fn with_listing<R>(&self, f: impl FnOnce(&CachedListing) -> R) -> R {
        let cache = self.engine.inner.cache.read_ignore_poison();
        let listing = cache
            .get(&self.listing_id)
            .unwrap_or_else(|| panic!("listing `{}` is no longer cached", self.listing_id));
        f(listing)
    }

    /// This listing's cached entries.
    pub fn entries(&self) -> Vec<FileEntry> {
        self.with_listing(|listing| listing.entries().to_vec())
    }

    /// This listing's cached entry names, in cache order. The common assertion.
    pub fn entry_names(&self) -> Vec<String> {
        self.with_listing(|listing| listing.entries().iter().map(|e| e.name.clone()).collect())
    }

    /// Whether an overlay contributed any of this listing's rows, which makes
    /// it a pane view the fresh-listing oracle declines.
    pub fn has_overlay_rows(&self) -> bool {
        self.with_listing(|listing| listing.has_overlay_rows())
    }

    /// Whether the engine still holds the listing. For tests that assert on
    /// teardown.
    pub fn is_cached(&self) -> bool {
        self.engine.inner.contains(&self.listing_id)
    }
}

impl Drop for TestListingGuard {
    fn drop(&mut self) {
        self.engine.inner.list_directory_end(&self.listing_id);
    }
}

/// Builder for a listing a test puts into an engine's cache. Defaults to an
/// empty listing on `root` at `/test`, sorted Name / Ascending / LikeFiles,
/// hidden files shown, read from a volume that records every date.
///
/// `last_accessed_ms` defaults to the engine's NOW, matching a listing a live
/// pane just touched, so an orphan sweep doesn't take it; the reaper's own
/// tests set it explicitly.
pub struct TestListing {
    volume_id: String,
    path: PathBuf,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: DirectorySortMode,
    supported_dates: SupportedDates,
    entries: Vec<FileEntry>,
    include_hidden: bool,
    sequence: u64,
    last_accessed_ms: Option<u64>,
    overlay_rows: usize,
}

impl Default for TestListing {
    fn default() -> Self {
        Self::new()
    }
}

impl TestListing {
    /// An empty listing of `/test` on `root`, hidden files shown, sorted
    /// Name / Ascending / LikeFiles. Set a field to change one.
    pub fn new() -> Self {
        Self {
            volume_id: "root".to_string(),
            path: PathBuf::from("/test"),
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            supported_dates: SupportedDates::ALL,
            entries: Vec::new(),
            include_hidden: true,
            sequence: 0,
            last_accessed_ms: None,
            overlay_rows: 0,
        }
    }

    /// The volume the listing is on.
    pub fn volume(mut self, volume_id: &str) -> Self {
        self.volume_id = volume_id.to_string();
        self
    }

    /// The directory the listing shows.
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = path.into();
        self
    }

    /// How the listing sorts.
    pub fn sort(mut self, by: SortColumn, order: SortOrder, mode: DirectorySortMode) -> Self {
        self.sort_by = by;
        self.sort_order = order;
        self.directory_sort_mode = mode;
        self
    }

    /// The dates the volume that read the listing records, which decide the
    /// column its rows are in (`EffectiveSort`).
    pub fn supported_dates(mut self, dates: SupportedDates) -> Self {
        self.supported_dates = dates;
        self
    }

    /// The entries the listing holds, already in its EFFECTIVE sort order.
    pub fn entries(mut self, entries: Vec<FileEntry>) -> Self {
        self.entries = entries;
        self
    }

    /// The pane's hidden-files setting, which picks the row space its diffs
    /// speak. Defaults to showing everything, where a row IS its entry index in
    /// a fixture with no in-flight scratch.
    pub fn include_hidden(mut self, include_hidden: bool) -> Self {
        self.include_hidden = include_hidden;
        self
    }

    /// The revision the listing starts at.
    pub fn sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    /// When the listing was last read, in the engine's epoch milliseconds
    /// (now, by default): what the orphan reaper keys on.
    pub fn last_accessed_ms(mut self, last_accessed_ms: u64) -> Self {
        self.last_accessed_ms = Some(last_accessed_ms);
        self
    }

    /// How many of the entries a listing overlay contributed. Nonzero makes this
    /// a PANE listing, which the fresh-listing oracle declines.
    pub fn overlay_rows(mut self, overlay_rows: usize) -> Self {
        self.overlay_rows = overlay_rows;
        self
    }

    /// Inserts the listing into `engine` under a unique id derived from `tag`
    /// and hands back its guard. Bind it (`let listing = …`), never
    /// `let _ = …`: a `_` binding ends the listing before the test runs.
    pub fn insert(self, engine: &Listings, tag: &str) -> TestListingGuard {
        let inner = &engine.inner;
        let listing_id = unique_test_id(tag);
        let listing = CachedListing::new(
            inner,
            self.volume_id,
            self.path,
            Prepared::for_test(self.entries, OverlayRows::Recounted(self.overlay_rows))
                .claiming(ListedDates::Claimed(self.supported_dates)),
            self.include_hidden,
            self.sort_by,
            self.sort_order,
            self.directory_sort_mode,
        );
        listing.sequence.store(self.sequence, Ordering::Relaxed);
        let last_accessed_ms = self.last_accessed_ms.unwrap_or_else(|| inner.epoch_millis_now());
        listing.last_accessed_ms.store(last_accessed_ms, Ordering::Relaxed);
        inner.cache.write_ignore_poison().insert(listing_id.clone(), listing);
        TestListingGuard::adopt(engine, listing_id)
    }
}
