//! Directory listing cache for on-demand virtual scrolling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::WatchCoverage;

use crate::file_system::listing::cached_listing::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::{FileEntry, TagRef};
use crate::file_system::listing::operations::OverlayRows;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder, entry_comparator};
use crate::file_system::volume::manager::RoutedKind;
pub use cmdr_fs::volume::DirectoryChange;

/// Result of updating an entry in-place or moving it to a new sorted position.
#[derive(Debug)]
pub enum ModifyResult {
    /// Entry was updated without changing its sorted position.
    UpdatedInPlace { index: usize },
    /// Entry was removed from `old_index` and re-inserted at `new_index` because sort-relevant
    /// fields changed.
    Moved { old_index: usize, new_index: usize },
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
            entry_count: listing.entries().len(),
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

/// Returns the newest cached pane listing for a `(volume_id, path)` pair.
///
/// Unlike [`try_get_authoritative_listing`], this doesn't require a watcher: SMB, MTP, and
/// other virtual panes still have a UI listing cache. No filesystem call happens here.
pub(crate) fn get_cached_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
    let cache = LISTING_CACHE.read().ok()?;
    let listing = cache
        .values()
        .filter(|listing| listing.volume_id == volume_id && listing.path == path)
        .max_by_key(|listing| (listing.sequence.load(Ordering::Relaxed), listing.created_at))?;
    listing.touch();
    Some(listing.entries().to_vec())
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

    // Don't insert if an entry with this path already exists. Asked BEFORE
    // `entries_mut` drops the path map, so the guard rides one when it's there.
    if listing.index_of_path(&entry.path).is_some() {
        return None;
    }

    let cmp = entry_comparator(listing.sort_by, listing.sort_order, listing.directory_sort_mode);
    let entries = listing.entries_mut();
    let pos = entries.partition_point(|existing| cmp(existing, &entry).is_lt());
    entries.insert(pos, entry);
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
/// `listing_watch_coverage(path)` without two separate cache reads.
pub fn get_listing_volume_id_and_path(listing_id: &str) -> Option<(String, PathBuf)> {
    let cache = LISTING_CACHE.read().ok()?;
    cache
        .get(listing_id)
        .map(|listing| (listing.volume_id.clone(), listing.path.clone()))
}

/// Removes every entry `paths` names, returning `(pre-removal index, entry)` for
/// the ones the listing held, HIGHEST INDEX FIRST.
///
/// **The batch form is the only by-path removal**, because its caller is always a
/// batch: one coalesced watcher event carries up to 500 paths. Why that matters,
/// and what removing them one at a time cost: `DETAILS.md` § "Entries by path".
///
/// ❗ Indices are the PRE-removal listing's, which is the space a `directory-diff`
/// payload speaks, and resolving plus removing under ONE write lock is what keeps
/// them true — no other writer can move a row in between. Highest-first is the
/// order that stops each removal shifting a row a later one still points at.
pub fn remove_entries_by_paths(listing_id: &str, paths: &[PathBuf]) -> Vec<(usize, FileEntry)> {
    let Ok(mut cache) = LISTING_CACHE.write() else {
        return Vec::new();
    };
    let Some(listing) = cache.get_mut(listing_id) else {
        return Vec::new();
    };
    listing.touch();

    let path_strings: Vec<String> = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let mut doomed: Vec<usize> = listing
        .indices_of_paths(path_strings.iter().map(String::as_str))
        .into_iter()
        .flatten()
        .collect();
    // Highest first, and each row at most once: a repeated path in `paths` must
    // not take a second, innocent row down with it.
    doomed.sort_unstable_by(|a, b| b.cmp(a));
    doomed.dedup();
    // ❗ Return before `entries_mut`, which drops both maps. A watcher event whose
    // removals all landed in another listing must not cost this one its maps, and
    // an add-only event calls straight through here with an empty `paths`.
    if doomed.is_empty() {
        return Vec::new();
    }

    let entries = listing.entries_mut();
    doomed.into_iter().map(|index| (index, entries.remove(index))).collect()
}

/// Removes the entry whose file name equals `name` from a listing, returning its
/// index and value.
///
/// A cached listing is exactly one directory, so entry names are unique — matching
/// by name (not full path) makes the `Removed` patch robust to the path-space the
/// notifier resolves the parent into. This matters for MTP: `MtpVolume` stores each
/// entry's `path` as the storage-relative inner form (`/Documents/notes.txt`), while
/// `notify_mutation` resolves the parent to the absolute `mtp://…` URL to match the
/// listing itself. Comparing full paths never matched, so `notify_mutation(Deleted)`
/// silently no-oped and a moved/deleted MTP file lingered in the source pane until a
/// manual refresh. Local/SMB entries store the same path space the notifier builds,
/// so name matching is equivalent there (a directory has no duplicate names).
pub fn remove_entry_by_name(listing_id: &str, name: &std::ffi::OsStr) -> Option<(usize, FileEntry)> {
    let mut cache = LISTING_CACHE.write().ok()?;
    let listing = cache.get_mut(listing_id)?;
    listing.touch();
    let entries = listing.entries_mut();
    let idx = entries
        .iter()
        .position(|e| Path::new(&e.path).file_name() == Some(name))?;
    let entry = entries.remove(idx);
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
        .is_some_and(|listing| listing.index_of_path(path).is_some())
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

    let cmp = entry_comparator(listing.sort_by, listing.sort_order, listing.directory_sort_mode);
    // ❗ Before `entries_mut`, which drops the path map: after it, this could only
    // walk. Still true when the mutation lands, because the `LISTING_CACHE` write
    // lock is held across both, and a modify that finds nothing now leaves both
    // maps standing rather than dropping them for a listing it never touched.
    let idx = listing.index_of_path(&new_entry.path)?;
    let entries = listing.entries_mut();
    let old = &entries[idx];

    let sort_relevant_changed = old.size != new_entry.size
        || old.modified_at != new_entry.modified_at
        || old.is_directory != new_entry.is_directory;

    if sort_relevant_changed {
        entries.remove(idx);
        let new_pos = entries.partition_point(|existing| cmp(existing, &new_entry).is_lt());
        entries.insert(new_pos, new_entry);
        Some(ModifyResult::Moved {
            old_index: idx,
            new_index: new_pos,
        })
    } else {
        entries[idx] = new_entry;
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
        && let Some(index) = listing.index_of_path(&entry.path)
        && !listing.entries()[index].tags.is_empty()
    {
        entry.tags = listing.entries()[index].tags.clone();
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
///
/// **The whole batch is one pass.** Rows are found through the listing's path map
/// (`path_index.rs`), so a 500-path enrichment chunk holds the write lock for the
/// length of the chunk rather than 500 walks of the listing.
pub fn apply_tags_to_listing(listing_id: &str, updates: Vec<(String, Vec<TagRef>)>) {
    use crate::file_system::listing::diff::DiffChange;
    use crate::file_system::listing::diff_emitter::enqueue_diff;

    let changes: Vec<DiffChange> = {
        let mut cache = match LISTING_CACHE.write() {
            Ok(c) => c,
            Err(_) => return,
        };
        let Some(listing) = cache.get_mut(listing_id) else {
            return;
        };
        listing.touch();
        listing
            .set_tags_by_path(updates)
            .into_iter()
            .map(|index| DiffChange::modified(listing.entries()[index].clone(), index))
            .collect()
    };
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
    crate::index_host::index().apply_directory_change(volume_id, parent_path, &change);

    // The cloud badge caches per directory, so this is how it learns that a file it
    // has an answer for moved on. Cheap: one hash lookup, hit or miss.
    #[cfg(target_os = "macos")]
    crate::file_system::sync_status::invalidate_dir(parent_path);

    let listings = find_listings_for_path_on_volume(Some(volume_id), parent_path);

    // For non-FullRefresh changes, bail early if no listing matches this path.
    // FullRefresh has a volume-wide fallback below (for STATUS_NOTIFY_ENUM_DIR).
    if listings.is_empty() && !matches!(change, DirectoryChange::FullRefresh) {
        return;
    }

    // Skip if no AppHandle is registered yet (test or pre-init context): every arm
    // below ends in `enqueue_diff`, whose coalesced flush needs the handle to emit,
    // so the whole re-read would be wasted work. This says nothing about async
    // context — see `spawn_full_refresh` for why the runtime is resolved globally.
    let has_app = WATCHER_MANAGER.read().ok().and_then(|m| m.app_handle.clone()).is_some();
    if !has_app {
        return;
    }

    match change {
        DirectoryChange::Added(entry) => {
            let mut entry = entry;
            crate::index_host::index().enrich(volume_id, std::slice::from_mut(&mut entry));
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
            crate::index_host::index().enrich(volume_id, std::slice::from_mut(&mut entry));
            for (listing_id, ..) in &listings {
                notify_modified(listing_id, entry.clone());
            }
        }
        DirectoryChange::Renamed { old_name, new_entry } => {
            let mut new_entry = new_entry;
            crate::index_host::index().enrich(volume_id, std::slice::from_mut(&mut new_entry));
            let old_path = parent_path.join(&old_name);
            for (listing_id, ..) in &listings {
                notify_removed(listing_id, &old_path);
                notify_added(listing_id, new_entry.clone());
            }
        }
        DirectoryChange::Replaced(entries) => {
            // The backend already re-read the directory, so there's nothing to ask
            // it for: enrich once for the volume, then publish per listing. This is
            // where the diffing a device backend used to do for itself lives now.
            let mut entries = entries;
            crate::index_host::index().enrich(volume_id, &mut entries);
            for (listing_id, ..) in &listings {
                // Zero overlay rows: this arm is a device backend handing over
                // its own re-read (SMB, MTP), and no overlay contributes to one.
                publish_replacement(listing_id, entries.clone(), 0);
            }
        }
        DirectoryChange::FullRefresh => {
            if listings.is_empty() {
                // No listing matches this exact path. For STATUS_NOTIFY_ENUM_DIR the
                // path is the share root, but the user may be browsing a subdirectory.
                // Refresh all listings on this volume instead.
                let volume_listings = find_listings_on_volume(volume_id);
                for (lid, path, sort_by, sort_order, dir_sort_mode) in volume_listings {
                    spawn_full_refresh(
                        volume_id.to_string(),
                        path,
                        vec![(lid, sort_by, sort_order, dir_sort_mode)],
                    );
                }
            } else {
                spawn_full_refresh(volume_id.to_string(), parent_path.to_path_buf(), listings);
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
    use crate::file_system::listing::diff::DiffChange;
    use crate::file_system::listing::diff_emitter::enqueue_diff;

    if has_entry(listing_id, &entry.path) {
        notify_modified(listing_id, entry);
        return;
    }

    let Some(index) = insert_entry_sorted(listing_id, entry.clone()) else {
        return; // Listing gone (or, harmless: lost a TOCTOU race against another add — Modified would no-op).
    };

    enqueue_diff(listing_id, vec![DiffChange::added(entry, index)]);
}

/// Removes an entry from the cache and queues a single-remove change.
///
/// Matches by file name within the listing (its directory), NOT the full path, so
/// it works when the listing's stored entry paths use a different path space than
/// the notifier's resolved parent (MTP: inner `/Dir/file` entries vs `mtp://…`
/// parent). See `remove_entry_by_name`.
pub(super) fn notify_removed(listing_id: &str, full_path: &Path) {
    use crate::file_system::listing::diff::DiffChange;
    use crate::file_system::listing::diff_emitter::enqueue_diff;

    let Some(name) = full_path.file_name() else {
        return;
    };
    let Some((index, removed_entry)) = remove_entry_by_name(listing_id, name) else {
        return; // Not in cache or listing gone
    };

    enqueue_diff(listing_id, vec![DiffChange::removed(removed_entry, index)]);
}

/// Updates an entry in the cache and queues a modify (or, when its sort key changed, a move) change.
fn notify_modified(listing_id: &str, mut entry: FileEntry) {
    use crate::file_system::listing::diff::DiffChange;
    use crate::file_system::listing::diff_emitter::enqueue_diff;

    // Preserve already-loaded Finder tags across this re-stat (see `carry_forward_tags`).
    carry_forward_tags(listing_id, &mut entry);

    let result = match update_entry_sorted(listing_id, entry.clone()) {
        Some(r) => r,
        None => return,
    };

    let changes = match result {
        ModifyResult::UpdatedInPlace { index } => vec![DiffChange::modified(entry, index)],
        ModifyResult::Moved { old_index, new_index } => vec![DiffChange::moved(entry, old_index, new_index)],
    };

    enqueue_diff(listing_id, changes);
}

/// Dispatches a `FullRefresh` re-read onto Tauri's global async runtime.
///
/// Every `FullRefresh` producer (the notify-rs debouncer, the git watcher, the SMB
/// and MTP watcher threads) runs on a plain OS thread with no Tokio runtime in
/// thread-local context, so a bare `tokio::spawn` panics with "there is no reactor
/// running" and takes the app down. `tauri::async_runtime::spawn` resolves the
/// process-global runtime instead of a thread-local one, so it works from any
/// thread. Same rule as `file_system::watcher` and `cmdr_archive::watch`.
pub(super) fn spawn_full_refresh(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    tauri::async_runtime::spawn(notify_full_refresh(volume_id, parent_path, listings));
}

/// One directory's refresh turnstile: an async lock plus a flag saying whether a
/// refresh is already queued behind the running one.
struct RefreshSlot {
    lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    queued: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Per-directory turnstiles for [`notify_full_refresh`], keyed by the pair that
/// identifies the directory a refresh re-reads.
static REFRESH_SLOTS: LazyLock<RwLock<HashMap<(String, PathBuf), RefreshSlot>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Hands out (creating on first use) the turnstile for one directory.
fn refresh_slot(volume_id: &str, parent_path: &Path) -> RefreshSlot {
    let key = (volume_id.to_string(), parent_path.to_path_buf());
    if let Some(slot) = REFRESH_SLOTS.read_ignore_poison().get(&key) {
        return RefreshSlot {
            lock: slot.lock.clone(),
            queued: slot.queued.clone(),
        };
    }
    let mut slots = REFRESH_SLOTS.write_ignore_poison();
    // Drop turnstiles nobody holds any more. A slot is one `String` + `PathBuf` + two
    // `Arc`s, but there is one per directory that ever saw a refresh, so without this a
    // long session browsing a big tree accumulates them for directories it left hours
    // ago. `strong_count == 1` means this map holds the only reference: no refresh is
    // running or queued there, so removing it can't strand a waiter.
    slots.retain(|_, slot| std::sync::Arc::strong_count(&slot.lock) > 1);
    let slot = slots.entry(key).or_insert_with(|| RefreshSlot {
        lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        queued: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    });
    RefreshSlot {
        lock: slot.lock.clone(),
        queued: slot.queued.clone(),
    }
}

/// Re-reads a directory via the Volume trait, computes a diff, and queues it.
///
/// ❗ Refreshes of the SAME directory are serialized, and read-then-write is atomic
/// against each other because of it. Without that, a burst of writes fires several
/// refreshes at once and each does its own read before replacing the cached listing
/// wholesale, so a read that STARTED earlier can LAND later and reinstate a directory
/// state that has already been superseded. Nothing re-reads afterwards, so the pane
/// keeps showing the older truth until an unrelated event corrects it: files that are
/// on disk read as missing for as long as the folder stays quiet. That is reachable by
/// any heavy external burst — an unzip, a `git checkout`, an rsync into a watched
/// folder — and it cost an E2E flake before it was understood
/// (`a_slow_refresh_cannot_overwrite_the_listing_a_newer_one_already_wrote`).
///
/// At most one refresh runs and one waits per directory: a third arrival returns
/// immediately, because the one already queued will start its read after the running
/// one finishes and so cannot answer with anything staler. That bounds a storm to two
/// reads instead of one per event, which is also why this is cheaper than what it
/// replaces, not just safer.
///
/// `pub(super)` so a test can await one directly rather than through the spawn.
pub(super) async fn notify_full_refresh(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    let slot = refresh_slot(&volume_id, &parent_path);
    // Claim the single waiting berth. Losing the race means somebody is already queued
    // to read AFTER the current refresh, which covers this request too.
    if slot.queued.swap(true, Ordering::AcqRel) {
        return;
    }
    let _turn = slot.lock.lock().await;
    // The berth is free again the moment this refresh OWNS the turn: its own read is
    // still ahead, so a request arriving now must be allowed to queue behind it.
    slot.queued.store(false, Ordering::Release);
    notify_full_refresh_locked(volume_id, parent_path, listings).await;
}

/// Makes `entries` the contents of `listing_id` and queues what changed for the
/// next coalesced flush.
///
/// Sorts the way that listing sorts BEFORE diffing, which is the whole reason a
/// backend hands entries over instead of patching the cache itself: a protocol
/// answers in its own order (MTP by object handle), and a diff computed against
/// that order carries indices that point at the wrong rows in a pane sorted any
/// other way.
///
/// `entries` must already be enriched with index data, so the rows the frontend
/// receives carry the recursive sizes the cache is about to hold. Callers enrich
/// ONCE per directory rather than once per listing.
///
/// `overlay_rows` is how many of `entries` a listing overlay contributed, which
/// the listing has to keep recorded: a `.git/` listing that regained its six
/// virtual rows in this refresh must not read as authoritative to a walker
/// afterwards (`crate::listing_overlays`).
pub(super) fn publish_replacement(listing_id: &str, entries: Vec<FileEntry>, overlay_rows: usize) {
    use crate::file_system::listing::diff::compute_diff;
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::listing::sorting::sort_entries;

    let mut sorted = entries;
    let old_entries = {
        let cache = match LISTING_CACHE.read() {
            Ok(c) => c,
            Err(_) => return,
        };
        // The listing closed between the backend's re-read and this call, which is
        // the ordinary race on a device that unplugs mid-refresh.
        let Some(listing) = cache.get(listing_id) else {
            return;
        };
        sort_entries(
            &mut sorted,
            listing.sort_by,
            listing.sort_order,
            listing.directory_sort_mode,
        );
        listing.entries().to_vec()
    };

    let changes = compute_diff(&old_entries, &sorted);
    if changes.is_empty() {
        return;
    }

    // Entries and count under ONE lock acquisition: a walker asking the
    // fresh-listing oracle between the two writes would see six contributed
    // rows described by a count that still said zero, and a delete walker would
    // be handed a path with no inode behind it.
    crate::file_system::listing::operations::update_listing_entries(
        listing_id,
        sorted,
        OverlayRows::Recounted(overlay_rows),
    );
    enqueue_diff(listing_id, changes);
}

/// The body of one refresh, with the directory's turn already held.
async fn notify_full_refresh_locked(
    volume_id: String,
    parent_path: PathBuf,
    listings: Vec<(String, SortColumn, SortOrder, DirectorySortMode)>,
) {
    // Re-resolve from `(volume_id, parent_path)` so a `.zip`-crossing listing hits
    // the same `ArchiveVolume` the read used (the cache keys on the parent drive
    // id, and a re-resolve re-registers a lazily-evicted archive).
    let resolved = crate::file_system::volume::manager::get_volume_manager()
        .resolve(&volume_id, &parent_path)
        .await;
    let is_routed = resolved.is_routed();
    let vol = match resolved.volume {
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

    // A routed volume has no drive index, so enrich is a no-op — skip it.
    if !is_routed {
        crate::index_host::index().enrich(&volume_id, &mut new_entries);
    }

    // Re-run the overlays, in the same place in the pipeline the first read ran
    // them. Without this a watcher-driven refresh of a repo's `.git/` would
    // replace the listing with the real entries alone and the six portal rows
    // would vanish from an open pane.
    let overlay_rows = crate::listing_overlays::decorate(&vol, &parent_path, &mut new_entries).await;

    for (listing_id, ..) in &listings {
        publish_replacement(listing_id, new_entries.clone(), overlay_rows);
    }
}

/// Finds every cached listing at or inside `root` on `volume_id`.
///
/// `Path::starts_with` is component-wise, so `/a/foo.zip` matches the archive
/// root listing (`/a/foo.zip`) and any inner listing (`/a/foo.zip/sub`) but never
/// a prefix-similar sibling (`/a/foo.zipper`) or the containing directory (`/a`).
fn find_listings_under_path_on_volume(
    volume_id: &str,
    root: &Path,
) -> Vec<(String, PathBuf, SortColumn, SortOrder, DirectorySortMode)> {
    let cache = match LISTING_CACHE.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    cache
        .iter()
        .filter(|(_, listing)| listing.volume_id == volume_id && listing.path.starts_with(root))
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

/// Refreshes every open listing at or inside the archive at `archive_path` on its
/// parent drive `volume_id`, re-reading each through the freshly re-resolved
/// `ArchiveVolume`.
///
/// Two callers fire this when the backing `.zip` changes: the local archive
/// content watch (`archive::watch`, a `notify` watch on a LOCAL parent) and the
/// SMB share watcher (`smb_watcher`, for a REMOTE parent that has no local
/// `notify` transport — see `crates/cmdr-smb/DETAILS.md` § "SMB archive push-refresh").
/// It deliberately does NOT go through [`notify_directory_changed`]: that
/// function runs the drive-index sync (`apply_smb_change`) up front, and an
/// archive-inner path (`/…/foo.zip/dir`) isn't a real filesystem path, so feeding
/// it to the index would be meaningless. Here we only refresh the pane listings.
///
/// A listing whose re-read fails (a mid-write, truncated central directory) is
/// left untouched by [`notify_full_refresh`] — it keeps its previous entries
/// rather than blanking the pane, and the next change event retries. The FE only
/// surfaces the damaged-archive banner at navigation time, never from this
/// refresh path.
pub async fn refresh_archive_listings(volume_id: &str, archive_path: &Path) {
    let listings = find_listings_under_path_on_volume(volume_id, archive_path);
    for (listing_id, path, sort_by, sort_order, dir_sort_mode) in listings {
        // Each inner listing lives at its own path, so refresh per listing path
        // (two panes on the same inner dir share a path and coalesce naturally).
        notify_full_refresh(
            volume_id.to_string(),
            path,
            vec![(listing_id, sort_by, sort_order, dir_sort_mode)],
        )
        .await;
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

/// Returns cached entries for `(volume_id, path)` when the volume reports
/// [`WatchCoverage::EveryWriter`] for this listing and no listing overlay
/// decorated it. Otherwise `None`.
///
/// **Freshness contract (read carefully)**: a `Some(_)` result means the volume has
/// a change-notification channel that every writer's changes reach, and the cache
/// reflects the volume's most recently observed state. It does NOT mean the cache is
/// byte-perfect with the device right now: every backend has a debounce or settling
/// window between a real change and the cache reflecting it.
///
/// - Local FS: FSEvents coalesce window (~10 ms). An OS-mounted network share is
///   served by the same backend but never qualifies: FSEvents can't see other
///   clients there, so it reports `ThisMachineOnly` and this returns `None`.
/// - SMB: 200 ms watcher debounce; > 50 events per directory triggers a `FullRefresh` which arrives
///   via a real re-read.
/// - MTP: 500 ms event debouncer plus per-device polling. Many MTP devices (cameras especially)
///   never emit per-object events, so "watched" there means only "the device is reachable and would
///   forward changes if it sent any."
/// - Archive (`.zip`): 200 ms debounce on a watch of the backing file's parent directory, plus a
///   re-parse of the central directory. A mid-write (truncated) archive keeps the previous listing
///   until a clean re-read, so "watched" here means "the last clean parse," never a half-written one.
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
pub fn try_get_authoritative_listing(volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
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
        // A listing carrying overlay rows is what a PANE sees, not what the
        // directory holds: handing it to a delete walker or a copy scan would
        // send one at a path with no inode behind it. See
        // `crate::listing_overlays`.
        if listing.has_overlay_rows() {
            return None;
        }
        listing.entries().to_vec()
    };

    // Step 2: ask the volume what a watch on this listing actually covers.
    // `resolve_local_only` (the sync sibling of `resolve`) routes a LOCAL `.zip`
    // listing to its ArchiveVolume, whose live content watch answers
    // `listing_watch_coverage` honestly (covered once established, `None` if it
    // couldn't start). This oracle runs on sync recursive scan walkers, so it can't
    // `.await` the async remote confirm — hence the local-only variant.
    let resolved = crate::file_system::volume::manager::get_volume_manager().resolve_local_only(volume_id, path);
    let volume = resolved.volume?;

    // Honesty guard for a REMOTE archive-inner path. `resolve_local_only` can't
    // confirm a remote boundary, so such a path stays a passthrough to its parent
    // (a direct-SMB / MTP volume). That parent's `listing_watch_coverage` is
    // VOLUME-level ("device reachable"), which would falsely claim freshness for an
    // archive whose content watch is local-only and never established. Decline, so
    // the write-op pre-flight rescans through the ArchiveVolume rather than reusing
    // a possibly-stale cached inner listing. A local dir merely NAMED `foo.zip`
    // isn't affected (its parent is local), nor is a genuine local archive (routed
    // to `RoutedKind::Archive`). Matching the ARCHIVE kind rather than "routed at
    // all" is deliberate: the doubt this guards is specifically an unconfirmed
    // archive boundary.
    if resolved.routed != Some(RoutedKind::Archive)
        && !volume.supports_local_fs_access()
        && cmdr_archive::archive_boundary_candidate(path).is_some()
    {
        return None;
    }

    // Only full coverage substitutes for a read. `ThisMachineOnly` means a live
    // watch that can't see other writers (an OS-mounted share), so the cache may
    // be missing entries nobody told us about — exactly the case where handing
    // these to a delete walker or a copy scan does damage. Pay the re-read.
    match volume.listing_watch_coverage(path) {
        WatchCoverage::EveryWriter => Some(entries),
        WatchCoverage::ThisMachineOnly | WatchCoverage::None => None,
    }
}
