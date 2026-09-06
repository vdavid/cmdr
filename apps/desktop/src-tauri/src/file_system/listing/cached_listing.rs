//! The cached listing record and the process-global map it lives in.
//!
//! One `CachedListing` is one directory as a PANE sees it: what the volume
//! read, plus whatever a listing overlay contributed, plus the two lazily
//! rebuilt maps (row numbers and paths) that let an accessor index instead of
//! walk. The helpers that patch these records, the diff they queue, and the
//! orphan reaper are `caching.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

use crate::file_system::listing::metadata::{FileEntry, TagRef};
use crate::file_system::listing::path_index::PathIndexCache;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::visible_rows::{VisibleRows, VisibleRowsCache};

/// Cache for directory listings (on-demand virtual scrolling).
/// Key: listing_id, Value: cached listing with all entries.
pub(crate) static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Process-start reference point for the `last_accessed_ms` field on `CachedListing`.
///
/// `Instant` isn't an integer, so we can't store it in an `AtomicU64` for lock-free
/// touch-on-read. Instead we store milliseconds elapsed since this epoch. Monotonic,
/// never affected by wall-clock jumps, and good for ~584 million years before the
/// `u64` overflows.
static LISTING_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Milliseconds elapsed since `LISTING_EPOCH`. Used to stamp `last_accessed_ms`.
pub(crate) fn epoch_millis_now() -> u64 {
    LISTING_EPOCH.elapsed().as_millis() as u64
}

/// Cached directory listing for on-demand virtual scrolling.
pub(crate) struct CachedListing {
    /// Volume ID this listing belongs to (like "root", "dropbox")
    pub volume_id: String,
    /// Path within the volume (absolute path for now)
    pub path: PathBuf,
    /// Cached file entries, exactly what's on disk. What the PANE shows is a
    /// subset of this (`visible_rows`), so ❗ reach for [`Self::rows`] to answer
    /// anything index-shaped. Private so `entries_mut` is the only way to change
    /// it, which is what keeps the row map from ever going stale.
    entries: Vec<FileEntry>,
    /// Row numbers over the visible subset of `entries`, per `include_hidden`.
    /// Rebuilt lazily after any mutation; see `visible_rows.rs`.
    visible_rows: VisibleRowsCache,
    /// Where each entry sits, by path, for the callers that know a path and need
    /// an index. Rebuilt lazily after any mutation; see `path_index.rs`.
    path_index: PathIndexCache,
    /// Current sort column
    pub sort_by: SortColumn,
    /// Current sort order
    pub sort_order: SortOrder,
    /// How directories are sorted relative to the current sort column
    pub directory_sort_mode: DirectorySortMode,
    /// Monotonic sequence number for `directory-diff` events. Incremented each time
    /// the cache is patched (by watcher, notify_mutation, or manual refresh).
    /// Lives on the listing so it works for all volume types, including SMB/MTP
    /// which don't use the FSEvents-based `WatchedDirectory`.
    pub sequence: AtomicU64,
    /// When this listing was created. Used by `snapshot_listings` for triage,
    /// surfacing orphan listings (e.g., volume dropdown previews) in error reports.
    pub created_at: Instant,
    /// Milliseconds since `LISTING_EPOCH` at the last access (read accessor, resort, or
    /// watcher/notify cache patch).
    ///
    /// **Decision**: track last-access for the orphan reaper, NOT `created_at`.
    /// **Why**: `created_at` is stamped once at creation and never refreshed, so an
    /// age-based reaper keyed on it would wrongly evict a long-open pane (a pane
    /// legitimately backs the same listing for the whole session). `last_accessed_ms` is
    /// bumped on every operation that proves the listing is still backing a live pane —
    /// `get_file_range`, `get_total_count`, `get_file_at`, `get_listing_stats`, resort,
    /// and every watcher/notify diff that patches the cache. So the reaper only ever
    /// sees a stale timestamp on a listing nobody has touched for hours: a genuine leak.
    /// `AtomicU64` (not `Mutex<Instant>`) so the read accessors, which already hold a
    /// shared `LISTING_CACHE.read()` lock, can stamp it lock-free.
    pub last_accessed_ms: AtomicU64,
    /// How many of `entries` a [`ListingOverlay`](crate::listing_overlays::ListingOverlay)
    /// contributed: rows the PANE shows that no volume holds.
    ///
    /// Nonzero makes this listing a pane view rather than a picture of a
    /// directory, so [`try_get_authoritative_listing`](crate::file_system::listing::caching::try_get_authoritative_listing) declines it and a delete
    /// walker or a copy scan pays the re-read. Without that, a `.git/` listing a
    /// watch keeps fresh would hand six virtual folders straight to a walker
    /// that then tries to remove them.
    overlay_rows: usize,
}

/// What a cache update knows about the [`ListingOverlay`](crate::listing_overlays::ListingOverlay)
/// rows inside the entries it is about to write.
///
/// The two travel together because they must be written under ONE lock
/// acquisition: a walker asking the fresh-listing oracle in between would see
/// decorated entries described by the previous count, and six rows with no inode
/// behind them would go to a delete walker.
pub(crate) enum OverlayRows {
    /// The overlays ran again for this write, and this is what they contributed.
    Recounted(usize),
    /// The overlays did not run: this write patches entries a previous read
    /// already decorated, so the stored count still describes them.
    Unchanged,
}

impl CachedListing {
    /// A listing freshly filled from a volume read: sequence 0, created and
    /// accessed now.
    pub(crate) fn new(
        volume_id: String,
        path: PathBuf,
        entries: Vec<FileEntry>,
        sort_by: SortColumn,
        sort_order: SortOrder,
        directory_sort_mode: DirectorySortMode,
    ) -> Self {
        Self {
            volume_id,
            path,
            entries,
            visible_rows: VisibleRowsCache::new(),
            path_index: PathIndexCache::new(),
            sort_by,
            sort_order,
            directory_sort_mode,
            sequence: AtomicU64::new(0),
            created_at: Instant::now(),
            last_accessed_ms: AtomicU64::new(epoch_millis_now()),
            overlay_rows: 0,
        }
    }

    /// Records that `count` of the entries came from a listing overlay. See
    /// [`Self::has_overlay_rows`].
    pub(crate) fn with_overlay_rows(mut self, count: usize) -> Self {
        self.overlay_rows = count;
        self
    }

    /// Replaces the overlay-row count, for a re-read that ran the overlays
    /// again. Written in the same lock acquisition as
    /// [`set_entries`](Self::set_entries), which
    /// [`OverlayRows`] makes every
    /// caller decide about: a listing that gained contributed rows while this
    /// still said zero would be handed to a walker as if it were a picture of
    /// the directory.
    pub(crate) fn set_overlay_rows(&mut self, count: usize) {
        self.overlay_rows = count;
    }

    /// Whether a listing overlay contributed any of these rows.
    pub(crate) fn has_overlay_rows(&self) -> bool {
        self.overlay_rows > 0
    }

    /// Refreshes `last_accessed_ms` to now. Cheap, lock-free; safe to call under a shared
    /// `LISTING_CACHE.read()` lock. Every accessor that proves a live pane calls this so
    /// the orphan reaper never evicts a listing in active use.
    pub(crate) fn touch(&self) {
        self.last_accessed_ms.store(epoch_millis_now(), Ordering::Relaxed);
    }

    /// Everything on disk, in sort order. For an index a PANE gave you, ask
    /// [`Self::rows`] instead: a pane's row numbers skip the entries it isn't
    /// showing, so `entries()[7]` and row 7 are different files.
    pub(crate) fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// The entries, to change. Drops the row map and the path map on the way out,
    /// so no caller can leave a stale one behind by forgetting a step.
    pub(crate) fn entries_mut(&mut self) -> &mut Vec<FileEntry> {
        self.visible_rows.invalidate();
        self.path_index.invalidate();
        &mut self.entries
    }

    /// Where `path` sits in `entries`, or `None` when the listing doesn't hold it.
    ///
    /// ❗ A caller that then MUTATES must ask before it takes
    /// [`Self::entries_mut`]: that drops the path map, so a lookup after it can
    /// only walk. A single path never builds a map; see
    /// `PathIndexCache::resolve_one`.
    pub(crate) fn index_of_path(&self, path: &str) -> Option<usize> {
        self.path_index.resolve_one(&self.entries, path)
    }

    /// Where each of `paths` sits in `entries`, in the order given, `None` for the
    /// ones it doesn't hold. ONE build decision for the whole batch, so a caller
    /// with a path list reaches for this rather than looping
    /// [`Self::index_of_path`]. See `path_index.rs`.
    pub(crate) fn indices_of_paths<'a>(&self, paths: impl ExactSizeIterator<Item = &'a str>) -> Vec<Option<usize>> {
        self.path_index.resolve(&self.entries, paths)
    }

    /// Replaces the tags on the entries `updates` names, in place, and reports the
    /// index of every row whose tags actually changed.
    ///
    /// **Deliberately not routed through [`Self::entries_mut`].** A tag is not part
    /// of a name, a sort key, or a path, so neither map can go stale from a tag
    /// write, and dropping them here would make a whole-listing rebuild the price
    /// of every enrichment chunk — the frontend sends one per 500 rows. ❗ If a tag
    /// ever becomes a sort column or a visibility input, this has to go back
    /// through `entries_mut`.
    pub(crate) fn set_tags_by_path(&mut self, updates: Vec<(String, Vec<TagRef>)>) -> Vec<usize> {
        let found = self.indices_of_paths(updates.iter().map(|(path, _)| path.as_str()));

        let mut changed = Vec::new();
        for (index, (_, tags)) in found.into_iter().zip(updates) {
            // A path the listing doesn't hold is skipped, not an error: it
            // scrolled away, or the enrich batch outlived the row.
            let Some(index) = index else { continue };
            if self.entries[index].tags != tags {
                self.entries[index].tags = tags;
                changed.push(index);
            }
        }
        changed
    }

    /// Replaces the entries wholesale (a re-read, a re-sort).
    pub(crate) fn set_entries(&mut self, entries: Vec<FileEntry>) {
        *self.entries_mut() = entries;
    }

    /// The rows a pane with this `include_hidden` is showing, indexed in constant
    /// time. THE single filter point every read accessor goes through, so counts,
    /// ranges, stats, selection indices, and type-to-jump can never disagree
    /// about what the pane is showing.
    pub(crate) fn rows(&self, include_hidden: bool) -> VisibleRows<'_> {
        self.visible_rows.rows(&self.entries, include_hidden)
    }
}
