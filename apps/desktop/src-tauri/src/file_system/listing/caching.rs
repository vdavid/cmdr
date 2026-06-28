//! Directory listing cache for on-demand virtual scrolling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};

use crate::file_system::listing::metadata::{FileEntry, TagRef};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, entry_comparator};

/// Describes a change to a directory's contents on a specific volume.
///
/// Used by `notify_directory_changed` to apply targeted cache updates
/// and emit `directory-diff` events to the frontend.
///
/// `Clone` so the SMB watch→index translator (`indexing::smb_watch`) can stash a
/// change in its mid-scan replay buffer without taking ownership away from the
/// pane-update path.
#[derive(Clone)]
pub enum DirectoryChange {
    /// A single entry was added. Includes the full `FileEntry` to insert.
    Added(FileEntry),
    /// A single entry was removed by name.
    Removed(String),
    /// A single entry was modified. Includes the updated `FileEntry`.
    Modified(FileEntry),
    /// An entry was renamed within the same directory.
    Renamed { old_name: String, new_entry: FileEntry },
    /// Unknown or bulk change: trigger a full re-read via the Volume trait.
    FullRefresh,
}

/// Result of updating an entry in-place or moving it to a new sorted position.
pub enum ModifyResult {
    /// Entry was updated without changing its sorted position.
    UpdatedInPlace { index: usize },
    /// Entry was removed from `old_index` and re-inserted at `new_index` because sort-relevant
    /// fields changed.
    Moved { old_index: usize, new_index: usize },
}

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

/// Idle window after which an untouched listing is treated as orphaned and reaped.
///
/// **Deliberately generous (six hours).** A listing legitimately lives for the entire
/// time a pane shows its directory, which can be the whole multi-day session. The
/// primary, fast eviction path is the explicit `list_directory_end` IPC; this backstop
/// only catches listings that genuinely leaked (a thrown FE handler skipped the close
/// IPC, an `$effect` teardown that threw, a future code path that forgot the call).
/// Every read accessor that proves a pane is still alive (`get_file_range`,
/// `get_total_count`, `get_file_at`, `get_listing_stats`, resort, watcher-diff patches,
/// …) refreshes `last_accessed_ms`, so a pane the user is interacting with — or that is
/// receiving FS-change diffs — is never six continuous hours idle. We err strongly
/// toward NOT evicting: six hours of zero interaction AND zero FS activity on a path is
/// overwhelmingly a leak, not a pane the user is actively using.
pub(crate) const ORPHAN_IDLE_WINDOW: Duration = Duration::from_secs(6 * 60 * 60);

/// How often the backstop reaper task wakes up to scan for orphaned listings.
///
/// Coarse on purpose: the reaper is defense-in-depth for a multi-day session, not a
/// hot-path reclaimer, so a 30-minute cadence keeps it effectively free while still
/// bounding orphan accumulation well under a day.
pub(crate) const REAPER_SWEEP_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Cached directory listing for on-demand virtual scrolling.
pub(crate) struct CachedListing {
    /// Volume ID this listing belongs to (like "root", "dropbox")
    pub volume_id: String,
    /// Path within the volume (absolute path for now)
    pub path: PathBuf,
    /// Cached file entries
    pub entries: Vec<FileEntry>,
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
}

impl CachedListing {
    /// Refreshes `last_accessed_ms` to now. Cheap, lock-free; safe to call under a shared
    /// `LISTING_CACHE.read()` lock. Every accessor that proves a live pane calls this so
    /// the orphan reaper never evicts a listing in active use.
    pub(crate) fn touch(&self) {
        self.last_accessed_ms.store(epoch_millis_now(), Ordering::Relaxed);
    }
}

/// Pure helper: given the current time, the idle window, and an iterator of
/// `(listing_id, last_accessed_ms)`, returns the IDs whose idle time meets or exceeds
/// `window_ms`.
///
/// Split out from the cache walk so the reaper logic is deterministically testable
/// without sleeping or touching the real (process-start-relative) clock: feed it a
/// synthetic `now_ms`, `window_ms`, and a list of stamps.
pub(crate) fn orphan_ids<'a>(
    now_ms: u64,
    window_ms: u64,
    listings: impl Iterator<Item = (&'a str, u64)>,
) -> Vec<String> {
    listings
        .filter(|(_, last_ms)| now_ms.saturating_sub(*last_ms) >= window_ms)
        .map(|(id, _)| id.to_string())
        .collect()
}

/// Scans the cache for orphaned listings (idle past `ORPHAN_IDLE_WINDOW`) and tears each
/// down via the same path as the explicit `list_directory_end` IPC: cache entry removed,
/// `WATCHER_MANAGER` watcher dropped, pending coalesced diff dropped.
///
/// This is the backstop reaper. The fast, primary eviction is still the FE-fired
/// `list_directory_end`; this only catches listings whose close IPC was never delivered.
/// Returns the IDs it reaped (empty in the common case), for logging/tests.
pub(crate) fn reap_orphaned_listings() -> Vec<String> {
    reap_orphaned_listings_at(epoch_millis_now(), ORPHAN_IDLE_WINDOW.as_millis() as u64)
}

/// `reap_orphaned_listings` with the clock and idle window injected, so tests can
/// simulate "6 hours idle" deterministically (the real epoch clock starts at process
/// launch, so a real 6 h gap can't be produced in a unit test without sleeping).
pub(crate) fn reap_orphaned_listings_at(now_ms: u64, window_ms: u64) -> Vec<String> {
    let ids = {
        let cache = match LISTING_CACHE.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        orphan_ids(
            now_ms,
            window_ms,
            cache
                .iter()
                .map(|(id, listing)| (id.as_str(), listing.last_accessed_ms.load(Ordering::Relaxed))),
        )
    };

    for id in &ids {
        // Reuse the exact teardown the explicit close IPC uses, so the cache entry AND
        // its watcher (and any pending diff) are released together.
        crate::file_system::listing::operations::list_directory_end(id);
        log::warn!(
            target: "listing_cache",
            "Reaped orphaned listing `{id}`: no access for >= {} min. Its `list_directory_end` IPC was likely never delivered (a skipped FE cleanup).",
            ORPHAN_IDLE_WINDOW.as_secs() / 60,
        );
    }

    ids
}

/// Spawns the periodic backstop reaper task. Call once during app setup.
///
/// Wakes every `REAPER_SWEEP_INTERVAL` and calls `reap_orphaned_listings`. Runs on
/// Tauri's async runtime so it survives for the process lifetime; the task ends only
/// when the runtime shuts down at app exit.
pub(crate) fn start_orphan_listing_reaper() {
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(REAPER_SWEEP_INTERVAL).await;
            let reaped = reap_orphaned_listings();
            if !reaped.is_empty() {
                log::info!(
                    target: "listing_cache",
                    "Orphan-listing reaper swept {} leaked listing(s)",
                    reaped.len(),
                );
            }
        }
    });
}

/// Lightweight summary of one cached listing, for `snapshot_listings`.
pub struct ListingSummary {
    pub listing_id: String,
    pub volume_id: String,
    pub path: PathBuf,
    pub entry_count: usize,
    pub age_ms: u128,
}

/// Returns a snapshot of every active listing in the cache. Used by `cmdr://state`
/// so triagers can spot orphan listings (started but never bound to a pane,
/// for example when a volume dropdown commits a navigation that the user then
/// abandons or that surfaces an error).
pub fn snapshot_listings() -> Vec<ListingSummary> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let now = Instant::now();
    let mut out: Vec<ListingSummary> = cache
        .iter()
        .map(|(id, listing)| ListingSummary {
            listing_id: id.clone(),
            volume_id: listing.volume_id.clone(),
            path: listing.path.clone(),
            entry_count: listing.entries.len(),
            age_ms: now.saturating_duration_since(listing.created_at).as_millis(),
        })
        .collect();
    out.sort_by_key(|a| a.age_ms);
    out
}

/// Finds all cached listings whose directory path matches `parent_path`.
///
/// When `volume_id` is `Some`, also filters by volume. This prevents false matches
/// when two volumes serve overlapping paths.
///
/// Returns `(listing_id, sort_by, sort_order, directory_sort_mode)` for each match.
/// Typically 0 (no pane showing that dir), 1, or 2 (both panes showing the same dir).
pub fn find_listings_for_path(parent_path: &Path) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)> {
    find_listings_for_path_on_volume(None, parent_path)
}

/// Like `find_listings_for_path`, but also filters by `volume_id`.
pub fn find_listings_for_path_on_volume(
    volume_id: Option<&str>,
    parent_path: &Path,
) -> Vec<(String, SortColumn, SortOrder, DirectorySortMode)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .filter(|(_, listing)| listing.path == parent_path && volume_id.is_none_or(|vid| listing.volume_id == vid))
        .map(|(id, listing)| {
            (
                id.clone(),
                listing.sort_by,
                listing.sort_order,
                listing.directory_sort_mode,
            )
        })
        .collect()
}

/// Finds all cached listings belonging to a volume, regardless of path.
///
/// Used by `FullRefresh` when the SMB watcher emits `STATUS_NOTIFY_ENUM_DIR` for
/// the share root but no listing matches that exact path (the user may be browsing
/// a subdirectory).
pub(crate) fn find_listings_on_volume(
    volume_id: &str,
) -> Vec<(String, PathBuf, SortColumn, SortOrder, DirectorySortMode)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    cache
        .iter()
        .filter(|(_, listing)| listing.volume_id == volume_id)
        .map(|(id, listing)| {
            (
                id.clone(),
                listing.path.clone(),
                listing.sort_by,
                listing.sort_order,
                listing.directory_sort_mode,
            )
        })
        .collect()
}

/// Inserts a `FileEntry` into a cached listing at the correct sorted position.
///
/// Uses `partition_point` with the listing's sort comparator to find the insertion index.
/// Returns the insertion index, or `None` if the listing wasn't found or the entry
/// already exists (checked by path).
pub fn insert_entry_sorted(listing_id: &str, entry: FileEntry) -> Option<usize> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;
    listing.touch();

    // Don't insert if an entry with this path already exists
    if listing.entries.iter().any(|e| e.path == entry.path) {
        return None;
    }

    let cmp = entry_comparator(listing.sort_by, listing.sort_order, listing.directory_sort_mode);
    let pos = listing
        .entries
        .partition_point(|existing| cmp(existing, &entry).is_lt());
    listing.entries.insert(pos, entry);
    Some(pos)
}

/// Returns the directory path for a cached listing, without cloning entries.
pub fn get_listing_path(listing_id: &str) -> Option<PathBuf> {
    let cache = LISTING_CACHE.read().ok()?;
    cache.get(listing_id).map(|listing| listing.path.clone())
}

/// Returns `(volume_id, path)` for a cached listing in one read-lock acquisition.
///
/// Used by `refresh_listing` so the short-circuit check can ask the volume
/// `listing_is_watched(path)` without two separate cache reads.
pub fn get_listing_volume_id_and_path(listing_id: &str) -> Option<(String, PathBuf)> {
    let cache = LISTING_CACHE.read().ok()?;
    cache
        .get(listing_id)
        .map(|listing| (listing.volume_id.clone(), listing.path.clone()))
}

/// Removes an entry by its path from the cached listing.
///
/// Returns `(old_index, removed_entry)` or `None` if the listing or entry wasn't found.
pub fn remove_entry_by_path(listing_id: &str, path: &Path) -> Option<(usize, FileEntry)> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;
    listing.touch();
    let path_str = path.to_string_lossy();

    let idx = listing.entries.iter().position(|e| e.path == *path_str)?;
    let entry = listing.entries.remove(idx);
    Some((idx, entry))
}

/// Checks whether a cached listing contains an entry with the given path.
pub fn has_entry(listing_id: &str, path: &str) -> bool {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return false,
    };
    cache
        .get(listing_id)
        .is_some_and(|listing| listing.entries.iter().any(|e| e.path == path))
}

/// Updates an existing entry in the cached listing.
///
/// If sort-relevant fields changed (size, modified_at, is_directory), removes the old entry
/// and re-inserts at the correct sorted position. Otherwise updates in place.
/// Returns `None` if the listing or entry wasn't found.
pub fn update_entry_sorted(listing_id: &str, new_entry: FileEntry) -> Option<ModifyResult> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;
    listing.touch();

    let idx = listing.entries.iter().position(|e| e.path == new_entry.path)?;
    let old = &listing.entries[idx];

    let sort_relevant_changed = old.size != new_entry.size
        || old.modified_at != new_entry.modified_at
        || old.is_directory != new_entry.is_directory;

    if sort_relevant_changed {
        listing.entries.remove(idx);
        let cmp = entry_comparator(listing.sort_by, listing.sort_order, listing.directory_sort_mode);
        let new_pos = listing
            .entries
            .partition_point(|existing| cmp(existing, &new_entry).is_lt());
        listing.entries.insert(new_pos, new_entry);
        Some(ModifyResult::Moved {
            old_index: idx,
            new_index: new_pos,
        })
    } else {
        listing.entries[idx] = new_entry;
        Some(ModifyResult::UpdatedInPlace { index: idx })
    }
}

/// Fills `entry.tags` from the cached entry of the same path when `entry` carries
/// none. A watcher re-stat builds entries via `get_single_entry`, which reads no
/// xattr and so always yields empty tags; without this, any unrelated Modify
/// event (a content edit, an mtime touch) would blank a file's tag dots until the
/// next `enrich_tags` pass. Call this on a re-stat'd entry BEFORE it's stored and
/// emitted, so the cache and the `directory-diff` payload stay consistent.
///
/// No-op when the incoming entry already has tags — the enrich path sets tags
/// explicitly (including clearing to empty on an external removal), so it must
/// never route through here.
pub fn carry_forward_tags(listing_id: &str, entry: &mut FileEntry) {
    if !entry.tags.is_empty() {
        return;
    }
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Some(listing) = cache.get(listing_id)
        && let Some(old) = listing.entries.iter().find(|e| e.path == entry.path)
        && !old.tags.is_empty()
    {
        entry.tags = old.tags.clone();
    }
}

/// Applies freshly-read Finder tags to cached entries by path and enqueues ONE
/// coalesced `modify` diff for the rows that actually changed. Drives the deferred
/// `enrich_tags` pass.
///
/// Replaces tags **unconditionally** (including to empty), so an external removal
/// (a user clearing all tags in Finder) propagates and clears the dots — this is
/// the deliberate counterpart to `carry_forward_tags`, which only ever restores.
/// Tags are sort-irrelevant, so entries are mutated in place (no reorder). Paths
/// not present in the listing are skipped (scrolled away, or already removed).
/// Emits a diff only for rows whose tags genuinely changed, so re-enriching an
/// unchanged visible range is silent (no diff storm on every scroll).
pub fn apply_tags_to_listing(listing_id: &str, updates: Vec<(String, Vec<TagRef>)>) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    let mut changes: Vec<DiffChange> = Vec::new();
    {
        let mut cache = match LISTING_CACHE.write() {
            Ok(c) => c,
            Err(_) => return,
        };
        let Some(listing) = cache.get_mut(listing_id) else {
            return;
        };
        listing.touch();
        for (path, tags) in updates {
            if let Some(idx) = listing.entries.iter().position(|e| e.path == path)
                && listing.entries[idx].tags != tags
            {
                listing.entries[idx].tags = tags;
                changes.push(DiffChange {
                    change_type: "modify".to_string(),
                    entry: listing.entries[idx].clone(),
                    index: idx,
                });
            }
        }
    }
    if !changes.is_empty() {
        enqueue_diff(listing_id, changes);
    }
}

/// Notifies the listing system that a directory's contents changed on a volume.
///
/// Finds all active listings matching `volume_id` and `parent_path`, applies the
/// change to the cache, and emits `directory-diff` events to the frontend.
///
/// For single-entry changes (add/remove/modify/rename), patches the cache in-place.
/// For `FullRefresh`, re-reads the directory via the Volume trait and computes a diff.
pub fn notify_directory_changed(volume_id: &str, parent_path: &Path, change: DirectoryChange) {
    use crate::file_system::watcher::WATCHER_MANAGER;

    // Index sync FIRST, before any pane-listing work (plan Architecture §3).
    // The SMB watcher runs for the WHOLE volume's lifetime, not just while a pane
    // shows the share, so the index must update even when no listing matches this
    // path — hence this sits ahead of the "no listing, bail" early-return below.
    // It's a no-op for `root` and any non-indexed volume. Sequencing the index
    // write before the pane enrich means the enrich (and the `index-dir-updated`
    // the writer emits) reflect the just-written sizes, not the pre-event ones.
    // The coupling is one-directional: listing → indexer, never the reverse.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::indexing::apply_smb_change(volume_id, parent_path, &change);

    let listings = find_listings_for_path_on_volume(Some(volume_id), parent_path);

    // For non-FullRefresh changes, bail early if no listing matches this path.
    // FullRefresh has a volume-wide fallback below (for STATUS_NOTIFY_ENUM_DIR).
    if listings.is_empty() && !matches!(change, DirectoryChange::FullRefresh) {
        return;
    }

    // Skip if no AppHandle is registered yet (test or pre-init context).
    // Without it the `tokio::spawn` in FullRefresh has no runtime, and the
    // coalesced flush would never emit anyway.
    let has_app = WATCHER_MANAGER.read().ok().and_then(|m| m.app_handle.clone()).is_some();
    if !has_app {
        return;
    }

    match change {
        DirectoryChange::Added(entry) => {
            let mut entry = entry;
            crate::indexing::enrich_entries_with_index_on_volume(volume_id, std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_added(listing_id, entry.clone());
            }
        }
        DirectoryChange::Removed(name) => {
            let full_path = parent_path.join(&name);
            for (listing_id, ..) in &listings {
                notify_removed(listing_id, &full_path);
            }
        }
        DirectoryChange::Modified(entry) => {
            let mut entry = entry;
            crate::indexing::enrich_entries_with_index_on_volume(volume_id, std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_modified(listing_id, entry.clone());
            }
        }
        DirectoryChange::Renamed { old_name, new_entry } => {
            let mut new_entry = new_entry;
            crate::indexing::enrich_entries_with_index_on_volume(volume_id, std::slice::from_mut(&mut new_entry));
            let old_path = parent_path.join(&old_name);
            for (listing_id, ..) in &listings {
                notify_removed(listing_id, &old_path);
                notify_added(listing_id, new_entry.clone());
            }
        }
        DirectoryChange::FullRefresh => {
            if listings.is_empty() {
                // No listing matches this exact path. For STATUS_NOTIFY_ENUM_DIR the
                // path is the share root, but the user may be browsing a subdirectory.
                // Refresh all listings on this volume instead.
                let volume_listings = find_listings_on_volume(volume_id);
                for (lid, path, sort_by, sort_order, dir_sort_mode) in volume_listings {
                    let volume_id = volume_id.to_string();
                    tokio::spawn(notify_full_refresh(
                        volume_id,
                        path,
                        vec![(lid, sort_by, sort_order, dir_sort_mode)],
                    ));
                }
            } else {
                let volume_id = volume_id.to_string();
                let parent_path = parent_path.to_path_buf();
                tokio::spawn(notify_full_refresh(volume_id, parent_path, listings));
            }
        }
    }
}

/// Inserts an entry into the cache and queues a single-add change for the next
/// coalesced `directory-diff` flush.
///
/// **Upsert semantics**: if a cached entry with the same path already exists,
/// delegates to `notify_modified` so the cache reflects the latest observation
/// instead of dropping it. This matters when the SMB / MTP watcher fires an
/// Add event mid-write (the watcher's stat catches a partial file size), then
/// `Volume::write_from_stream` fires its own Add post-close with the final
/// size. Without upsert, the partial size from the watcher sticks and the FE
/// shows a wrong size until the next manual refresh. Concretely seen on
/// MTP→SMB copies: 9 files copied, 3 stuck at half size (watcher stat'd
/// mid-write, self-notify lost the race against `insert_entry_sorted`'s
/// duplicate guard).
pub(super) fn notify_added(listing_id: &str, entry: FileEntry) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    if has_entry(listing_id, &entry.path) {
        notify_modified(listing_id, entry);
        return;
    }

    let Some(index) = insert_entry_sorted(listing_id, entry.clone()) else {
        return; // Listing gone (or, harmless: lost a TOCTOU race against another add — Modified would no-op).
    };

    enqueue_diff(
        listing_id,
        vec![DiffChange {
            change_type: "add".to_string(),
            entry,
            index,
        }],
    );
}

/// Removes an entry from the cache and queues a single-remove change.
fn notify_removed(listing_id: &str, full_path: &Path) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    let Some((index, removed_entry)) = remove_entry_by_path(listing_id, full_path) else {
        return; // Not in cache or listing gone
    };

    enqueue_diff(
        listing_id,
        vec![DiffChange {
            change_type: "remove".to_string(),
            entry: removed_entry,
            index,
        }],
    );
}

/// Updates an entry in the cache and queues a modify (or remove+add) change.
fn notify_modified(listing_id: &str, mut entry: FileEntry) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::watcher::DiffChange;

    // Preserve already-loaded Finder tags across this re-stat (see `carry_forward_tags`).
    carry_forward_tags(listing_id, &mut entry);

    let result = match update_entry_sorted(listing_id, entry.clone()) {
        Some(r) => r,
        None => return,
    };

    let changes = match result {
        ModifyResult::UpdatedInPlace { index } => {
            vec![DiffChange {
                change_type: "modify".to_string(),
                entry,
                index,
            }]
        }
        ModifyResult::Moved { old_index, new_index } => {
            vec![
                DiffChange {
                    change_type: "remove".to_string(),
                    entry: entry.clone(),
                    index: old_index,
                },
                DiffChange {
                    change_type: "add".to_string(),
                    entry,
                    index: new_index,
                },
            ]
        }
    };

    enqueue_diff(listing_id, changes);
}

/// Re-reads a directory via the Volume trait, computes a diff, and queues it.
async fn notify_full_refresh(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::listing::sorting::sort_entries;
    use crate::file_system::watcher::compute_diff;

    let vol = match crate::file_system::get_volume_manager().get(&volume_id) {
        Some(v) => v,
        None => {
            log::warn!("notify_directory_changed: volume `{}` not found", volume_id);
            return;
        }
    };

    let mut new_entries = match vol.list_directory(&parent_path, None).await {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!(
                "notify_directory_changed: failed to re-read {}: {}",
                parent_path.display(),
                e
            );
            return;
        }
    };

    crate::indexing::enrich_entries_with_index_on_volume(&volume_id, &mut new_entries);

    for (listing_id, sort_by, sort_order, dir_sort_mode) in &listings {
        // Re-sort to match this listing's sort params
        let mut sorted = new_entries.clone();
        sort_entries(&mut sorted, *sort_by, *sort_order, *dir_sort_mode);

        // Get old entries for diff computation
        let old_entries = {
            let cache = match LISTING_CACHE.read() {
                Ok(c) => c,
                Err(_) => continue,
            };
            match cache.get(listing_id.as_str()) {
                Some(listing) => listing.entries.clone(),
                None => continue,
            }
        };

        let changes = compute_diff(&old_entries, &sorted);
        if changes.is_empty() {
            continue;
        }

        // Update cache
        crate::file_system::listing::operations::update_listing_entries(listing_id, sorted);

        enqueue_diff(listing_id, changes);
    }
}

/// Increments and returns the sequence number for a cached listing.
///
/// Uses the `AtomicU64` on `CachedListing` so it works for all volume types,
/// including SMB/MTP which don't have a `WatchedDirectory` entry.
pub(crate) fn increment_sequence(listing_id: &str) -> Option<u64> {
    let cache = LISTING_CACHE.read().ok()?;
    let listing = cache.get(listing_id)?;
    let seq = listing.sequence.fetch_add(1, Ordering::Relaxed) + 1;
    Some(seq)
}

/// Returns cached entries for `(volume_id, path)` when the volume reports that this
/// listing is being kept in sync by an active watcher. Otherwise `None`.
///
/// **Freshness contract (read carefully)**: a `Some(_)` result means the volume has
/// an active change-notification channel and the cache reflects the volume's most
/// recently observed state. It does NOT mean the cache is byte-perfect with the
/// device right now: every backend has a debounce or settling window between a real
/// change and the cache reflecting it.
///
/// - Local FS: FSEvents coalesce window (~10 ms).
/// - SMB: 200 ms watcher debounce; > 50 events per directory triggers a `FullRefresh` which arrives
///   via a real re-read.
/// - MTP: 500 ms event debouncer plus per-device polling. Many MTP devices (cameras especially)
///   never emit per-object events, so "watched" there means only "the device is reachable and would
///   forward changes if it sent any."
///
/// Callers must treat the result as "fresh as our most recent observation," which
/// is the same guarantee a `list_directory` call gives: it sees the device's state
/// at the moment the call returned, not at the moment the caller reads its result.
/// The contract intentionally accepts this window; a tighter one would force us to
/// re-validate every walk, defeating the whole point of the oracle.
///
/// When multiple cached listings exist for the same `(volume_id, path)` pair (two
/// panes browsing the same directory), the picker is deterministic: highest
/// `sequence`, ties broken by the latest `created_at`. Both listings receive watcher
/// events, so they're equally fresh; the tiebreaker is just to keep the result
/// stable across calls.
#[allow(
    dead_code,
    reason = "Plumbing: callers (scan walker, scan-preview) consume this via the fresh-listing oracle path"
)]
pub fn try_get_watched_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
    // Step 1: find all listings on this (volume_id, path) and pick the most-recently-updated
    // one (highest sequence, ties broken by latest created_at). Read the entries out
    // under the cache lock and drop the lock before crossing any async / volume boundary.
    let entries: Vec<FileEntry> = {
        let cache = LISTING_CACHE.read().ok()?;
        let mut best: Option<(&String, &CachedListing, u64, Instant)> = None;
        for (id, listing) in cache.iter() {
            if listing.volume_id != volume_id || listing.path != path {
                continue;
            }
            let seq = listing.sequence.load(Ordering::Relaxed);
            let created = listing.created_at;
            best = match best {
                None => Some((id, listing, seq, created)),
                Some((_, _, best_seq, best_created))
                    if seq > best_seq || (seq == best_seq && created > best_created) =>
                {
                    Some((id, listing, seq, created))
                }
                Some(other) => Some(other),
            };
        }
        let (_, listing, ..) = best?;
        listing.entries.clone()
    };

    // Step 2: ask the volume whether this listing is being kept fresh by a watcher.
    // VolumeManager::get returns an Arc<dyn Volume> which we hold for the duration of
    // the sync `listing_is_watched` call. No await between this and the entries return.
    let volume = crate::file_system::get_volume_manager().get(volume_id)?;
    if volume.listing_is_watched(path) {
        Some(entries)
    } else {
        None
    }
}
