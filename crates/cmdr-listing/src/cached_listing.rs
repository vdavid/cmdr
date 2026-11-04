//! The cached listing record, which lives in its engine's cache (`Inner::cache`, `lib.rs`).
//!
//! One `CachedListing` is one directory as a PANE sees it: what the volume
//! read, plus whatever a listing overlay contributed, plus the two lazily
//! rebuilt maps (row numbers and paths) that let an accessor index instead of
//! walk. The helpers that patch these records are `mutate.rs`, the diff they
//! queue is `diff_emitter.rs`, and the orphan reaper is `orphan_reaper.rs`.

use crate::rt::Instant;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLockReadGuard};

use crate::Inner;
use crate::diff_emitter::{CacheWrite, Publish};
use crate::host::{ListingVolumes, ScratchVisibility};
use crate::name_filter::NameFilter;
use crate::path_index::PathIndexCache;
pub(crate) use crate::pipeline::OverlayRows;
use crate::pipeline::{ListedDates, Prepared};
use crate::reread::ReadGeneration;
use crate::sorting::{EffectiveSort, sort_entries};
use crate::visible_rows::{ScratchProjection, VisibleRows, VisibleRowsCache};
use cmdr_collation::NameCollator;
use cmdr_fs::entry::{FileEntry, TagRef};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::SupportedDates;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

impl Inner {
    /// The cache under its read lock, with every listing in `ids` reconciled to
    /// the scratch settings first.
    ///
    /// Normal directories keep the shared-lock path and do not scan or clone entries.
    /// A drifted projection is rechecked and published under the mutation lock.
    pub(crate) fn reconciled_cache(&self, ids: &[&str]) -> RwLockReadGuard<'_, HashMap<String, CachedListing>> {
        // Read once, before the lock: the settings are engine state, and every
        // listing in this pass reconciles to the same answer.
        let visibility = self.scratch_visibility();
        loop {
            let cache = self.cache.read_ignore_poison();
            if !ids.iter().any(|id| {
                cache
                    .get(*id)
                    .is_some_and(|listing| listing.scratch_has_drift(visibility))
            }) {
                return cache;
            }
            drop(cache);
            let mut cache = self.write_cache();
            for id in ids {
                if let Some((listing, publish)) = cache.get_mut(id) {
                    listing.reconcile_scratch(id, visibility, &publish);
                }
            }
        }
    }

    /// The cache under its write lock, the only place a listing's changes are
    /// published from (`diff_emitter.rs`).
    pub(crate) fn write_cache(&self) -> CacheWrite<'_, CachedListing> {
        CacheWrite::lock(&self.cache, &self.diffs)
    }

    /// Whether the cache holds `listing_id`.
    pub(crate) fn contains(&self, listing_id: &str) -> bool {
        self.cache.read_ignore_poison().contains_key(listing_id)
    }
}

/// The directory a cached listing shows, spelled the ONE way its volume keys it.
///
/// A backend can spell one directory two ways: an MTP storage reports changes at
/// its URL (`mtp://{device}/{storage}/DCIM`) while its rows, and so a pane entered
/// with Enter, carry the inner `/DCIM`. Compared verbatim, a Cmdr-made delete
/// never reached that pane and the user acted on files already gone (ERR-QW42X,
/// ERR-46A6B). So every stored listing and every lookup builds its path through
/// [`ListingPath::on_volume`], which asks the volume (`Volume::listing_path`).
/// ❌ No `From<PathBuf>` and no `PartialEq<Path>`: a raw path can't be stored or
/// compared without passing through it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListingPath(PathBuf);

impl ListingPath {
    /// `path` in the spelling `volume_id`'s volume keys listings under; verbatim
    /// when `volumes` serves nothing under that id.
    ///
    /// ❗ Call it before taking the cache: it asks the host's volume seam.
    pub(crate) fn on_volume(volumes: &dyn ListingVolumes, volume_id: &str, path: &Path) -> Self {
        let volume = volumes.get(volume_id);
        Self(volume.map_or_else(|| path.to_path_buf(), |volume| volume.listing_path(path)))
    }

    /// The canonical spelling, for a re-read or a log line.
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Cached directory listing for on-demand virtual scrolling.
pub(crate) struct CachedListing {
    /// Volume ID this listing belongs to (like "root", "dropbox")
    pub(crate) volume_id: String,
    /// The directory, in its volume's one spelling. See [`ListingPath`].
    pub(crate) path: ListingPath,
    /// Cached file entries, exactly what's on disk. What the PANE shows is a
    /// subset of this (`visible_rows`), so ❗ reach for [`Self::rows`] to answer
    /// anything index-shaped. Private so `entries_mut` is the only way to change
    /// it, which is what keeps the row map from ever going stale.
    entries: Vec<FileEntry>,
    /// Whether the pane showing this listing shows hidden entries. It picks the
    /// row space every `directory-diff` for this listing speaks, and so which
    /// changes reach the pane at all. See [`Self::pane_rows`].
    include_hidden: bool,
    scratch_projection: ScratchProjection,
    /// The pane's quick filter, if the user is typing one. Like
    /// `include_hidden`, it picks the row space every reader and every
    /// `directory-diff` speaks. See `name_filter.rs`.
    name_filter: Option<NameFilter>,
    /// Row numbers over the visible subset of `entries`, per `include_hidden`.
    /// Rebuilt lazily after any mutation; see `visible_rows.rs`.
    visible_rows: VisibleRowsCache,
    /// Where each entry sits, by path, for the callers that know a path and need
    /// an index. Rebuilt lazily after any mutation; see `path_index.rs`.
    path_index: PathIndexCache,
    /// The column the pane ASKED to sort by, kept as asked: a fallback never
    /// writes it. The rows are in [`Self::effective`]'s order, which is the
    /// only thing a comparator may read.
    sort_by: SortColumn,
    /// The pane's sort order.
    sort_order: SortOrder,
    /// How directories are sorted relative to the sort column.
    directory_sort_mode: DirectorySortMode,
    /// Which dates the volume that last read this listing records, from the
    /// RESOLVED volume (an archive answers for itself). With the requested
    /// column it derives [`Self::effective`]; ❌ no stored effective column to
    /// drift from it.
    supported_dates: SupportedDates,
    /// Committed visible revision. Allocated under the cache write lock for a
    /// visible patch, sort, or visibility change, never by the transport emitter.
    /// Lives on the listing so it works for all volume types, including SMB/MTP
    /// which don't use the FSEvents-based `WatchedDirectory`.
    pub(crate) sequence: AtomicU64,
    /// When this listing was created. Used by `snapshot_listings` for triage,
    /// surfacing orphan listings (e.g., volume dropdown previews) in error reports.
    pub(crate) created_at: Instant,
    /// Milliseconds since [`Self::epoch`] at the last access (read accessor, resort, or
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
    /// shared cache read lock, can stamp it lock-free.
    pub(crate) last_accessed_ms: AtomicU64,
    /// The engine's epoch, which `last_accessed_ms` counts from. `Instant` isn't an
    /// integer, so it can't sit in an `AtomicU64` for lock-free touch-on-read;
    /// milliseconds since it can. Monotonic, never affected by wall-clock jumps.
    epoch: Instant,
    /// How many of `entries` a [`ListingOverlay`](crate::host::ListingOverlay)
    /// contributed: rows the PANE shows that no volume holds.
    ///
    /// Nonzero makes this listing a pane view rather than a picture of a
    /// directory, so `try_get_authoritative_listing` declines it and a delete
    /// walker or a copy scan pays the re-read. Without that, a `.git/` listing a
    /// watch keeps fresh would hand six virtual folders straight to a walker
    /// that then tries to remove them.
    overlay_rows: usize,
    /// The newest full re-read that landed here, by when it started reading.
    /// A re-read older than this is dropped (`reread.rs`).
    pub(crate) published_generation: ReadGeneration,
}

impl CachedListing {
    /// A listing freshly filled from a volume read, for `engine`: sequence 0,
    /// created and accessed now. `include_hidden` is the pane's setting at the
    /// time. The entries arrive [`Prepared`] (enriched and decorated, the read
    /// pipeline's only output) and sorted.
    ///
    /// ❗ Build it before taking the cache: it asks the volume seam for the
    /// listing's spelling. It reads its epoch and scratch settings off `engine`
    /// and keeps no reference to it.
    #[allow(
        clippy::too_many_arguments,
        reason = "a listing is born with its engine, its directory, its rows, and the pane's hidden-files and three sort settings"
    )]
    pub(crate) fn new(
        engine: &Inner,
        volume_id: String,
        path: PathBuf,
        prepared: Prepared,
        include_hidden: bool,
        sort_by: SortColumn,
        sort_order: SortOrder,
        directory_sort_mode: DirectorySortMode,
    ) -> Self {
        let (entries, overlay_rows, dates) = prepared.into_parts();
        // A new listing has no earlier count or dates for `Unchanged` to keep,
        // so it carries none: Modified is the date every volume records.
        let overlay_rows = match overlay_rows {
            OverlayRows::Recounted(count) => count,
            OverlayRows::Unchanged => 0,
        };
        let supported_dates = match dates {
            ListedDates::Claimed(dates) => dates,
            ListedDates::Unchanged => SupportedDates::NONE,
        };
        let scratch_projection =
            ScratchProjection::for_entries(&entries, &ScratchProjection::default(), engine.scratch_visibility());
        Self {
            path: engine.listing_path(&volume_id, &path),
            volume_id,
            entries,
            include_hidden,
            scratch_projection,
            name_filter: None,
            visible_rows: VisibleRowsCache::new(),
            path_index: PathIndexCache::new(),
            sort_by,
            sort_order,
            directory_sort_mode,
            supported_dates,
            sequence: AtomicU64::new(0),
            created_at: crate::rt::now(),
            last_accessed_ms: AtomicU64::new(engine.epoch_millis_now()),
            epoch: engine.epoch,
            overlay_rows,
            published_generation: ReadGeneration::INSERT,
        }
    }

    /// Replaces the overlay-row count, for a re-read that ran the overlays
    /// again. Written in the same lock acquisition as
    /// [`replace_entries`](Self::replace_entries), which
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

    /// The sort the rows are in: the requested column, resolved against the
    /// dates this listing's volume records. THE input to every comparator that
    /// orders this listing.
    pub(crate) fn effective(&self) -> EffectiveSort {
        EffectiveSort::resolve(
            self.sort_by,
            self.sort_order,
            self.directory_sort_mode,
            self.supported_dates,
        )
    }

    /// The column the pane asked for, which a fallback never changes.
    #[cfg(test)]
    pub(crate) fn requested_column(&self) -> SortColumn {
        self.sort_by
    }

    /// Records the pane's requested sort. The caller re-sorts with
    /// [`Self::sort_rows`].
    pub(crate) fn set_sort(
        &mut self,
        sort_by: SortColumn,
        sort_order: SortOrder,
        directory_sort_mode: DirectorySortMode,
    ) {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self.directory_sort_mode = directory_sort_mode;
    }

    /// Records the dates a re-read's volume records, unless nothing read through
    /// a volume (`ListedDates::Unchanged`). Answers the effective column when
    /// that moved, so the re-read can tell the pane its displayed date changed.
    pub(crate) fn set_listed_dates(&mut self, dates: ListedDates) -> Option<SortColumn> {
        let ListedDates::Claimed(dates) = dates else {
            return None;
        };
        let before = self.effective().column();
        self.supported_dates = dates;
        let after = self.effective().column();
        (before != after).then_some(after)
    }

    /// Puts the entries in [`Self::effective`]'s order.
    pub(crate) fn sort_rows(&mut self, collator: &Arc<NameCollator>) {
        let sort = self.effective();
        sort_entries(collator, self.entries_mut(), sort);
    }

    /// Refreshes `last_accessed_ms` to now. Cheap, lock-free; safe to call under a shared
    /// cache read lock. Every accessor that proves a live pane calls this so the
    /// orphan reaper never evicts a listing in active use.
    pub(crate) fn touch(&self) {
        self.last_accessed_ms
            .store(crate::rt::since(self.epoch).as_millis() as u64, Ordering::Relaxed);
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

    /// Rewrites the index-derived folder sizes of the entries at `paths`, in place: `update` gets
    /// each found entry with its position in `paths`, and says whether it wrote. A path the listing
    /// doesn't hold is skipped. Returns the entry index of every row `update` wrote.
    ///
    /// **Deliberately not routed through [`Self::entries_mut`]**, for the reason
    /// [`Self::set_tags_by_path`] gives: a recursive size is not a name or a visibility input, so
    /// neither map can go stale, and dropping them would rebuild a 74k-row map for every size
    /// refresh. It doesn't re-sort either, the same as enrichment always has: a size sort settles on
    /// the next re-sort.
    pub(crate) fn update_index_sizes_by_path(
        &mut self,
        paths: &[String],
        mut update: impl FnMut(usize, &mut FileEntry) -> bool,
    ) -> Vec<usize> {
        let found = self.indices_of_paths(paths.iter().map(String::as_str));
        let mut written = Vec::new();
        for (position, index) in found.into_iter().enumerate() {
            if let Some(index) = index
                && update(position, &mut self.entries[index])
            {
                written.push(index);
            }
        }
        written
    }

    /// The rows a pane with this `include_hidden` is showing, indexed in constant
    /// time. THE single filter point every read accessor goes through, so counts,
    /// ranges, stats, selection indices, and type-to-jump can never disagree
    /// about what the pane is showing.
    pub(crate) fn rows(&self, include_hidden: bool) -> VisibleRows<'_> {
        self.visible_rows.rows(
            &self.entries,
            include_hidden,
            self.name_filter.as_ref(),
            &self.scratch_projection,
        )
    }

    /// The pane's quick filter, if any.
    pub(crate) fn name_filter(&self) -> Option<&NameFilter> {
        self.name_filter.as_ref()
    }

    /// Records the pane's quick filter. Reports whether it changed. A change
    /// drops the row map: unlike `include_hidden`, the filter isn't a slot key.
    pub(crate) fn set_name_filter(&mut self, name_filter: Option<NameFilter>) -> bool {
        let changed = self.name_filter != name_filter;
        if changed {
            self.name_filter = name_filter;
            self.visible_rows.invalidate();
        }
        changed
    }

    /// Whether the pane showing this listing shows hidden entries.
    pub(crate) fn include_hidden(&self) -> bool {
        self.include_hidden
    }

    /// Records the pane's hidden-files setting. Reports whether it changed.
    pub(crate) fn set_include_hidden(&mut self, include_hidden: bool) -> bool {
        let changed = self.include_hidden != include_hidden;
        self.include_hidden = include_hidden;
        changed
    }

    /// The rows the pane showing this listing draws: [`Self::rows`] at the pane's
    /// own setting. What a `directory-diff` index means.
    pub(crate) fn pane_rows(&self) -> VisibleRows<'_> {
        self.rows(self.include_hidden)
    }

    /// Whether a scratch decision this listing committed differs from what
    /// `visibility` says now.
    pub(crate) fn scratch_has_drift(&self, visibility: ScratchVisibility) -> bool {
        self.scratch_projection.has_drift(visibility)
    }

    /// Pin a newly admitted entry before deriving its row or publication count.
    pub(crate) fn admit_entry(&mut self, entry: &FileEntry, visibility: ScratchVisibility) {
        self.scratch_projection.admit(entry, visibility);
    }

    pub(crate) fn forget_entry(&mut self, entry: &FileEntry) {
        self.scratch_projection.remove(&entry.path);
    }

    pub(crate) fn shows(&self, entry: &FileEntry) -> bool {
        self.shows_with_filter(entry, self.include_hidden, self.name_filter.as_ref())
    }

    pub(crate) fn shows_with_filter(
        &self,
        entry: &FileEntry,
        include_hidden: bool,
        filter: Option<&NameFilter>,
    ) -> bool {
        self.scratch_projection.shows(entry, include_hidden) && filter.is_none_or(|filter| filter.matches(&entry.name))
    }

    /// Commit projection drift BEFORE any consumer interprets old row indices.
    /// Both sides come from captured decisions, never a second ownership sample.
    pub(crate) fn reconcile_scratch(&mut self, listing_id: &str, visibility: ScratchVisibility, publish: &Publish<'_>) {
        let next = self.scratch_projection.live(visibility);
        if next == self.scratch_projection {
            return;
        }
        let changes = {
            let old: Vec<_> = self.pane_rows().iter().collect();
            let new: Vec<_> = self
                .entries
                .iter()
                .filter(|e| {
                    next.shows(e, self.include_hidden)
                        && self.name_filter.as_ref().is_none_or(|filter| filter.matches(&e.name))
                })
                .collect();
            super::diff::diff_rows(&old, &new)
        };
        self.scratch_projection = next;
        self.publish_transition(publish, listing_id, changes, None);
    }

    /// Replacement diffs compare projected rows, including newly admitted paths.
    pub(crate) fn replace_entries(
        &mut self,
        entries: Vec<FileEntry>,
        visibility: ScratchVisibility,
    ) -> Vec<super::diff::DiffChange> {
        let next = ScratchProjection::for_entries(&entries, &self.scratch_projection, visibility);
        let changes = {
            let old: Vec<_> = self.pane_rows().iter().collect();
            let new: Vec<_> = entries
                .iter()
                .filter(|e| {
                    next.shows(e, self.include_hidden)
                        && self.name_filter.as_ref().is_none_or(|filter| filter.matches(&e.name))
                })
                .collect();
            super::diff::diff_rows(&old, &new)
        };
        self.scratch_projection = next;
        *self.entries_mut() = entries;
        changes
    }

    /// Allocate only while the cache write lock protects the committed transition.
    pub(crate) fn advance_sequence(&mut self) -> u64 {
        let next = self.sequence.load(Ordering::Relaxed) + 1;
        self.sequence.store(next, Ordering::Release);
        next
    }

    /// Publication is part of the mutation, under the same cache write lock.
    pub(crate) fn publish_changes(
        &mut self,
        publish: &Publish<'_>,
        listing_id: &str,
        changes: Vec<super::diff::DiffChange>,
    ) {
        self.publish_changes_and_sort(publish, listing_id, changes, None);
    }

    /// [`Self::publish_changes`] for a re-read, which may also have moved the
    /// effective column (`effective_sort_by`). A moved column publishes even
    /// with no row changed: a one-row folder still has to hear that its
    /// displayed date changed.
    pub(crate) fn publish_changes_and_sort(
        &mut self,
        publish: &Publish<'_>,
        listing_id: &str,
        changes: Vec<super::diff::DiffChange>,
        effective_sort_by: Option<SortColumn>,
    ) {
        if changes.is_empty() && effective_sort_by.is_none() {
            return;
        }
        self.publish_transition(publish, listing_id, changes, effective_sort_by);
    }

    fn publish_transition(
        &mut self,
        publish: &Publish<'_>,
        listing_id: &str,
        changes: Vec<super::diff::DiffChange>,
        effective_sort_by: Option<SortColumn>,
    ) {
        let from_sequence = self.sequence.load(Ordering::Relaxed);
        let sequence = self.advance_sequence();
        publish.enqueue(
            listing_id,
            super::diff::DirectoryDiffBatch {
                from_sequence,
                sequence,
                total_count: self.pane_rows().len(),
                changes,
                effective_sort_by,
            },
        );
    }
}
