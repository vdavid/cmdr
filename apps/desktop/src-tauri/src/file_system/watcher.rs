//! File system watcher with debouncing, incremental processing, and diff computation.
//!
//! Watches directories for changes and emits `directory-diff` events to frontend.
//! Uses the unified LISTING_CACHE from operations.rs (no duplicate cache).
//! Two processing paths: incremental (stat + classify individual events, patch cache
//! in-place via cache helpers) and full re-read fallback (> 500 events or unknown
//! event kinds).

use crate::ignore_poison::RwLockIgnorePoison;
use notify_debouncer_full::{
    DebounceEventResult, DebouncedEvent, Debouncer, NoCache, new_debouncer_opt,
    notify::{
        self, RecommendedWatcher, RecursiveMode,
        event::{EventKind, ModifyKind},
    },
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::file_system::listing::{
    DiffChange, FileEntry, ModifyResult, compute_diff, get_listing_entries, get_listing_volume_id_and_path,
    get_single_entry, has_entry, insert_entry_sorted, list_directory_core, remove_entries_by_paths,
    OverlayRows, update_entry_sorted, update_listing_entries,
};
use crate::index_host::index;
use cmdr_fs::firmlinks;
use cmdr_fs::volume::WatchCoverage;

/// Default debounce duration in milliseconds (used if not configured)
const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Configured debounce duration in milliseconds (set by frontend via update_debounce_ms)
static DEBOUNCE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(DEFAULT_DEBOUNCE_MS);

/// Updates the file watcher debounce duration.
/// This affects newly started watchers; existing watchers keep their original duration.
pub fn update_debounce_ms(ms: u64) {
    DEBOUNCE_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
    log::debug!("File watcher debounce updated to {} ms", ms);
}

/// Gets the current debounce duration in milliseconds.
fn get_debounce_ms() -> u64 {
    DEBOUNCE_MS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Global watcher manager
pub(crate) static WATCHER_MANAGER: LazyLock<RwLock<WatcherManager>> =
    LazyLock::new(|| RwLock::new(WatcherManager::new()));

/// `directory-deleted` event: the watched directory itself was deleted.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "directory-deleted")]
pub struct DirectoryDeletedEvent {
    pub listing_id: String,
    pub path: String,
}

/// State for a watched directory.
/// NOTE: No `entries` field - we use the unified LISTING_CACHE instead.
pub(crate) struct WatchedDirectory {
    #[allow(dead_code, reason = "Debouncer must be held to keep watching")]
    debouncer: Debouncer<RecommendedWatcher, NoCache>,
    /// What this particular watch observes, decided once when it was armed.
    ///
    /// Resolved here rather than in `Volume::listing_watch_coverage` because the
    /// answer needs a `statfs` on the path, and the oracle that asks for it runs
    /// inside sync recursive scan walkers, once per directory. Deciding at arm
    /// time costs one syscall per open listing, on a path whose `read_dir` has
    /// just succeeded, and leaves the read side a pure in-memory lookup.
    ///
    /// Private on purpose: `coverage_for_listings` is the only reader, so the
    /// backend asks a function rather than reaching into the manager's map.
    coverage: WatchCoverage,
}

/// Manages file watchers for directories
pub(crate) struct WatcherManager {
    pub(crate) watches: HashMap<String, WatchedDirectory>,
    pub(crate) app_handle: Option<AppHandle>,
}

impl WatcherManager {
    fn new() -> Self {
        Self {
            watches: HashMap::new(),
            app_handle: None,
        }
    }
}

/// Initialize the watcher manager with the app handle.
/// Must be called during app setup.
pub fn init_watcher_manager(app: AppHandle) {
    WATCHER_MANAGER.write_ignore_poison().app_handle = Some(app);
}

/// Whether the app handle is registered yet (set once during app setup).
///
/// A watcher whose only job is to refresh open frontend listings has nothing to
/// do before this is true: the diff emit is a no-op with no app handle. The
/// archive content watch uses this to skip starting an OS watch in headless /
/// pre-init contexts (unit tests), which also keeps the test suite from
/// oversubscribing FSEvents. Production sets the handle at startup, before any
/// browsing, so archive watches always start when a user opens a `.zip`.
pub fn app_handle_present() -> bool {
    WATCHER_MANAGER.read_ignore_poison().app_handle.is_some()
}

/// Start watching a directory for a given listing.
///
/// # Arguments
/// * `listing_id` - The listing ID from list_directory_start
/// * `path` - The directory path to watch
///
/// Note: Initial entries are read from LISTING_CACHE when needed.
pub fn start_watching(listing_id: &str, path: &Path) -> Result<(), String> {
    log::debug!("start_watching: listing_id={}, path={}", listing_id, path.display());
    let listing_id_owned = listing_id.to_string();
    let listing_for_closure = listing_id_owned.clone();

    // Create the debouncer with a callback that handles changes
    let debounce_duration = Duration::from_millis(get_debounce_ms());
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        debounce_duration,
        None, // No tick rate limit
        move |result: DebounceEventResult| {
            match result {
                Ok(events) => {
                    handle_directory_change_incremental(&listing_for_closure, events);
                }
                Err(_errors) => {
                    // Watcher errors often mean the watched directory was deleted.
                    // Try to re-read; if it fails with NotFound, we'll emit directory-deleted.
                    let lid = listing_for_closure.clone();
                    tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
                }
            }
        },
        // `NoCache`, not the platform default `RecommendedCache` (a `FileIdMap` on
        // macOS). The map exists to pair a rename's `From` with its `To` by file id, and
        // it pays for that by walking the watched directory and `stat`ing every entry at
        // arm time, then re-`stat`ing on every create, rename, and remove.
        //
        // We get nothing for it: `handle_directory_change_incremental` collects the
        // unique paths out of a batch and re-stats each one, so a rename classifies
        // identically whether it arrives as one paired event carrying both paths or as a
        // separate `From` and `To`. Root-rename detection is unaffected too, since
        // `watch_root_identity_changed` matches on `Modify(Name(_))`, which the debouncer
        // emits either way. Linux already runs this path with `NoCache`.
        NoCache,
        notify::Config::default(),
    )
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Start watching the path (Debouncer implements Watcher trait)
    debouncer
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    // Store in manager (no entries - we use LISTING_CACHE)
    let coverage = coverage_for_watched_path(path);
    let mut manager = WATCHER_MANAGER.write().map_err(|_| "Failed to acquire watcher lock")?;

    manager
        .watches
        .insert(listing_id_owned, WatchedDirectory { debouncer, coverage });

    Ok(())
}

/// What an FSEvents watch on `path` can actually see.
///
/// A network mount answers [`WatchCoverage::ThisMachineOnly`]: FSEvents is a
/// local-VFS notifier, so it reports this machine's writes through the mount and
/// nothing another client does to the share. The pane still updates from the
/// user's own work, which is why the watch is worth arming at all; the oracle
/// just can't treat that cache as a substitute for reading the directory.
fn coverage_for_watched_path(path: &Path) -> WatchCoverage {
    if crate::file_system::index_provider::path_is_on_network_mount(path) {
        WatchCoverage::ThisMachineOnly
    } else {
        WatchCoverage::EveryWriter
    }
}

/// The coverage recorded for any live watch on `listing_ids`, best answer first.
///
/// Backs `LocalPosixVolume::listing_watch_coverage`, so it must stay a pure
/// in-memory read: it runs once per directory inside recursive scan walkers.
pub(crate) fn coverage_for_listings(listing_ids: &[String]) -> WatchCoverage {
    let manager = WATCHER_MANAGER.read_ignore_poison();
    let mut best = WatchCoverage::None;
    for id in listing_ids {
        match manager.watches.get(id.as_str()).map(|w| w.coverage) {
            // Nothing beats full coverage, so stop at the first one.
            Some(WatchCoverage::EveryWriter) => return WatchCoverage::EveryWriter,
            Some(WatchCoverage::ThisMachineOnly) => best = WatchCoverage::ThisMachineOnly,
            Some(WatchCoverage::None) | None => {}
        }
    }
    best
}

/// Arms the listing watcher without making the caller wait for it.
///
/// ❌ Don't call [`start_watching`] from the listing pipeline instead. Arming is slow
/// and, worse, slow by an amount that has nothing to do with the directory being
/// listed: it waits on an `FSEventStreamStart` handshake with `fseventsd`, on a
/// CFRunLoop thread bootstrap, and on [`stop_watching`] finishing the PREVIOUS
/// listing's teardown. The pipeline arms the watch before it emits `listing-complete`
/// and the pane renders nothing until that event, so every one of those waits used to
/// be dead time the user saw as a stalled "Sorting your files, preparing view…"
/// (measured p50 88 ms, p90 653 ms, max 1.5 s while navigating a warm `~/Downloads`,
/// macOS 26.5.2, 2026-08-11; a 3-entry folder hit 723 ms and a 265-entry folder 57 ms,
/// which is what "not about the directory" looks like in the data).
///
/// The listing doesn't depend on the watch: entries are read, sorted, and cached
/// before this is called. The watch only has to be in place before the user notices a
/// change on disk, and the window between the read and the arm is unchanged in
/// duration by detaching it, since it's the same work either way.
pub fn start_watching_detached(listing_id: &str, path: &Path) {
    let listing_id = listing_id.to_string();
    let path = path.to_path_buf();
    // `spawn_blocking`, not `spawn`: arming is a chain of blocking syscalls, so it must
    // not sit on a runtime worker. `tauri::async_runtime` rather than `tokio` because
    // the sync `list_directory_start` path calls this from the IPC handler thread,
    // which has no Tokio runtime context.
    tauri::async_runtime::spawn_blocking(move || arm_and_reconcile(&listing_id, &path));
}

/// Arms the watch, then hands it back if the listing ended while we were arming.
///
/// The reconcile half is not optional. `list_directory_end` removes the listing from
/// `LISTING_CACHE` and then removes a watch that a detached arm may not have inserted
/// yet, so the arm can land on a listing nobody will ever close again. Left alone that
/// strands an FSEvents stream, its CFRunLoop thread, and a manager entry for the life
/// of the process, each still costing `fseventsd` fan-out.
///
/// `LISTING_CACHE` membership is the liveness signal because `list_directory_end` is
/// what clears it, and reading it is a plain lookup that doesn't bump `last_accessed_ms`
/// (this is background work, not user activity — see `listing/CLAUDE.md`).
pub(super) fn arm_and_reconcile(listing_id: &str, path: &Path) {
    if let Err(e) = start_watching(listing_id, path) {
        log::warn!("Failed to start watcher: {}", e);
        return;
    }

    if get_listing_volume_id_and_path(listing_id).is_none() {
        log::debug!(
            "start_watching_detached: listing {} ended while arming, dropping the watch",
            listing_id
        );
        stop_watching(listing_id);
    }
}

/// Stop watching a directory for a given listing.
pub fn stop_watching(listing_id: &str) {
    let removed = WATCHER_MANAGER.write_ignore_poison().watches.remove(listing_id);

    // Drop OUTSIDE the lock. Dropping a `WatchedDirectory` tears an FSEvents run loop
    // down, and notify's teardown busy-spins on `CFRunLoopIsWaiting` before it joins the
    // stream's thread. Holding the manager's write lock across that made every
    // navigation's arm queue behind the previous listing's teardown, since the frontend
    // fires `listDirectoryEnd(old)` immediately before loading the new directory.
    drop(removed);
}

/// Maps an FSEvents/inotify path to the watched listing's path space, returning the
/// rebased path when the event is for a direct child of the watched directory.
///
/// Two path-form mismatches make a raw `parent == dir_path` comparison silently drop
/// events, leaving the pane stale until the user re-navigates:
///
/// 1. **Firmlinks / well-known `/private` symlinks.** FSEvents reports canonical paths
///    (`/private/tmp/…`) while the listing cache holds the user-navigated form
///    (`/tmp/…`). `firmlinks::normalize_path` (the same canonicalization the index
///    uses) aligns both.
/// 2. **A symlinked watch root.** Google Drive exposes "My Drive" as a symlink
///    (`…/CloudStorage/GoogleDrive-…/My Drive` → `~/My Drive`), and FSEvents resolves
///    the watched symlink and reports events under the real target. `canonical_dir` is
///    `dir_path` with its symlinks resolved (see the caller); matching against it
///    catches this case. iCloud and Dropbox mount real directories, so they hit the
///    firmlink path above; only Google Drive needs this branch.
///
/// The returned path is always rebased onto `dir_path` so cache lookups (`has_entry`)
/// and diff entries stay consistent with the listing's own path space.
pub(super) fn rebase_event_path(event_path: &Path, dir_path: &Path, canonical_dir: &Path) -> Option<PathBuf> {
    let parent = event_path.parent()?;
    if parent == dir_path {
        return Some(event_path.to_path_buf());
    }
    let parent_normalized = firmlinks::normalize_path(&parent.to_string_lossy());
    let dir_normalized = firmlinks::normalize_path(&dir_path.to_string_lossy());
    let canonical_normalized = firmlinks::normalize_path(&canonical_dir.to_string_lossy());
    if parent_normalized == dir_normalized || parent_normalized == canonical_normalized {
        event_path.file_name().map(|name| dir_path.join(name))
    } else {
        None
    }
}

/// Whether an FSEvents/inotify path IS the watched directory itself rather than
/// something inside it, in either path form (see `rebase_event_path` for why a
/// listing's path and the OS's path can differ).
pub(super) fn event_targets_watch_root(event_path: &Path, dir_path: &Path, canonical_dir: &Path) -> bool {
    if event_path == dir_path || event_path == canonical_dir {
        return true;
    }
    let event_normalized = firmlinks::normalize_path(&event_path.to_string_lossy());
    event_normalized == firmlinks::normalize_path(&dir_path.to_string_lossy())
        || event_normalized == firmlinks::normalize_path(&canonical_dir.to_string_lossy())
}

/// Whether this batch says the watched directory itself was replaced, removed, or
/// renamed, so the pane's entries can no longer be trusted.
///
/// `Modify(Metadata(_))` on the root is deliberately NOT a trigger: every ordinary
/// child create or remove bumps the directory's own mtime, so counting it would send
/// every change down the full re-read path and cost the incremental path its point.
fn watch_root_identity_changed(events: &[DebouncedEvent], dir_path: &Path, canonical_dir: &Path) -> bool {
    events.iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_))
        ) && event
            .paths
            .iter()
            .any(|path| event_targets_watch_root(path, dir_path, canonical_dir))
    })
}

/// Processes individual file-system events incrementally instead of re-reading the whole directory.
///
/// Falls back to `handle_directory_change` when events are too numerous or ambiguous.
fn handle_directory_change_incremental(listing_id: &str, events: Vec<DebouncedEvent>) {
    // Fallback: too many events or ambiguous event kinds
    if events.len() > 500
        || events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Any | EventKind::Other))
    {
        let lid = listing_id.to_string();
        // `tauri::async_runtime::spawn` instead of `tokio::spawn` because this
        // closure runs on the notify-rs debouncer thread, which has no Tokio
        // runtime context. Tauri's async runtime works from any thread.
        tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
        return;
    }

    // Get watched directory path + volume from the cache (without cloning all entries)
    let Some((volume_id, dir_path)) = get_listing_volume_id_and_path(listing_id) else {
        return;
    };

    // Resolve the watched dir's symlinks once per batch, so events FSEvents reports
    // under a symlinked root's real target (Google Drive's "My Drive" → `~/My Drive`)
    // still match. This is a `realpath` syscall like the per-event `get_single_entry`
    // stats below, so it adds no new blocking class here; falls back to `dir_path` if
    // the dir vanished mid-batch (the re-read path handles a deleted watch root).
    let canonical_dir = std::fs::canonicalize(&dir_path).unwrap_or_else(|_| dir_path.clone());

    // A watch root that was itself removed, created, or renamed is a DIFFERENT
    // directory now, so classifying this batch's child events against the old entries
    // would keep everything the replacement took away. macOS reports a wholesale
    // replacement (a `git checkout` across branches, `rsync --delete`, unzipping over a
    // folder, a build regenerating its output dir) as Remove(Folder) + Create(Folder)
    // on the ROOT plus one Create per NEW child, and never a remove for the old ones.
    // Re-read instead: that diffs against disk, replaces the listing, and emits
    // `directory-deleted` plus stops the watch when the directory is really gone.
    if watch_root_identity_changed(&events, &dir_path, &canonical_dir) {
        let lid = listing_id.to_string();
        // `tauri::async_runtime::spawn`, not `tokio::spawn`: see the fallback above.
        tauri::async_runtime::spawn(async move { handle_directory_change(&lid).await });
        return;
    }

    // Collect unique direct-child paths, skipping access events. Event paths are
    // rebased into the listing's path space (see `rebase_event_path`).
    let mut unique_paths: HashSet<PathBuf> = HashSet::new();
    for event in &events {
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        for path in &event.paths {
            if let Some(rebased) = rebase_event_path(path, &dir_path, &canonical_dir) {
                unique_paths.insert(rebased);
            }
        }
    }

    if unique_paths.is_empty() {
        return;
    }

    // Stat all paths BEFORE acquiring any locks
    let mut stat_results: HashMap<PathBuf, Option<FileEntry>> = HashMap::new();
    for path in &unique_paths {
        let entry = get_single_entry(path).ok();
        stat_results.insert(path.clone(), entry);
    }

    // Classify changes against the cache
    let mut adds: Vec<FileEntry> = Vec::new();
    let mut removes: Vec<PathBuf> = Vec::new();
    let mut modifies: Vec<FileEntry> = Vec::new();

    for (path, stat_entry) in &stat_results {
        let path_str = path.to_string_lossy();
        let in_cache = has_entry(listing_id, &path_str);
        match (in_cache, stat_entry) {
            (true, Some(entry)) => modifies.push(entry.clone()),
            (true, None) => removes.push(path.clone()),
            (false, Some(entry)) => adds.push(entry.clone()),
            (false, None) => {} // Not in cache and gone from disk: ignore
        }
    }

    if adds.is_empty() && removes.is_empty() && modifies.is_empty() {
        return;
    }

    // Enrich new/modified entries with index data
    for entry in &mut adds {
        index().enrich(&volume_id, std::slice::from_mut(entry));
    }
    for entry in &mut modifies {
        index().enrich(&volume_id, std::slice::from_mut(entry));
    }

    // Apply changes: removes first (their indices are the OLD listing's), then adds, then
    // modifies. `remove_entries_by_paths` resolves every index against the pre-removal
    // listing and drops the rows highest-index-first, so no removal shifts the next one's
    // row, and the whole batch costs one lookup pass.
    let mut changes: Vec<DiffChange> = Vec::new();

    for (original_index, removed_entry) in remove_entries_by_paths(listing_id, &removes) {
        changes.push(DiffChange::removed(removed_entry, original_index));
    }

    for entry in adds {
        if let Some(new_index) = insert_entry_sorted(listing_id, entry.clone()) {
            changes.push(DiffChange::added(entry, new_index));
        }
    }

    for mut entry in modifies {
        // Preserve already-loaded Finder tags across this re-stat: `get_single_entry`
        // reads no xattr, so a bare modify would otherwise blank the file's dots.
        crate::file_system::listing::caching::carry_forward_tags(listing_id, &mut entry);
        match update_entry_sorted(listing_id, entry.clone()) {
            Some(ModifyResult::UpdatedInPlace { index }) => {
                changes.push(DiffChange::modified(entry, index));
            }
            Some(ModifyResult::Moved { old_index, new_index }) => {
                changes.push(DiffChange::moved(entry, old_index, new_index));
            }
            None => {}
        }
    }

    if changes.is_empty() {
        return;
    }

    crate::file_system::listing::diff_emitter::enqueue_diff(listing_id, changes);
}

/// Force a re-read of a directory listing, computing and emitting any diff.
/// Called by the file watcher on change events, and also available as a Tauri
/// command for cases where the watcher doesn't fire (e.g. rename-move on Linux).
///
/// Works for all volume types: reads via the Volume trait's `list_directory`,
/// not via `std::fs`.
pub async fn handle_directory_change(listing_id: &str) {
    log::debug!("handle_directory_change: listing_id={}", listing_id);

    // Look up this listing's volume id so we can re-read through the Volume trait.
    let volume_id = {
        use crate::file_system::listing::cached_listing::LISTING_CACHE;
        let cache = match LISTING_CACHE.read() {
            Ok(c) => c,
            Err(_) => return,
        };
        match cache.get(listing_id) {
            Some(l) => l.volume_id.clone(),
            None => return,
        }
    };

    // Get old entries and path from the unified LISTING_CACHE
    let Some((path, old_entries)) = get_listing_entries(listing_id) else {
        return; // Listing no longer exists
    };

    // Resolve (not plain `get`) so a `.zip`-crossing listing re-reads through the
    // same ArchiveVolume the listing used, re-registering it if the LRU evicted
    // it. (Archives get no FSEvents watcher today, so this fires for them only
    // once live archive watching lands.)
    let volume = crate::file_system::volume::manager::get_volume_manager()
        .resolve(&volume_id, &path)
        .await
        .volume;

    // Get app handle for emitting events
    let app_handle = { WATCHER_MANAGER.read_ignore_poison().app_handle.clone() };

    // Re-read the directory via the Volume trait (works for all volume types).
    // Falls back to list_directory_core for listings whose volume was unregistered.
    let new_entries = if let Some(vol) = volume {
        match vol.list_directory(&path, None).await {
            Ok(entries) => entries,
            Err(crate::file_system::VolumeError::NotFound(_)) => {
                log::info!("Watcher: Directory deleted, notifying frontend: {}", path.display());
                if let Some(app) = &app_handle {
                    let event = DirectoryDeletedEvent {
                        listing_id: listing_id.to_string(),
                        path: path.to_string_lossy().to_string(),
                    };
                    if let Err(emit_err) = event.emit(app) {
                        log::warn!("Watcher: Failed to emit directory-deleted event: {}", emit_err);
                    }
                }
                stop_watching(listing_id);
                return;
            }
            Err(crate::file_system::VolumeError::PermissionDenied(_)) => return,
            Err(e) => {
                log::warn!("Watcher: Failed to re-read directory: {}", e);
                return;
            }
        }
    } else {
        // Volume unregistered: fall back to std::fs for local paths
        match list_directory_core(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("Watcher: Directory deleted, notifying frontend: {}", path.display());
                if let Some(app) = &app_handle {
                    let event = DirectoryDeletedEvent {
                        listing_id: listing_id.to_string(),
                        path: path.to_string_lossy().to_string(),
                    };
                    if let Err(emit_err) = event.emit(app) {
                        log::warn!("Watcher: Failed to emit directory-deleted event: {}", emit_err);
                    }
                }
                stop_watching(listing_id);
                return;
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::PermissionDenied {
                    log::warn!("Watcher: Failed to re-read directory: {}", e);
                }
                return;
            }
        }
    };

    // Re-sort new_entries by the listing's sort params so compute_diff compares
    // two lists in the same order (list_directory returns entries in Name/Asc).
    // Also enrich with index data so diff entries have recursive_size etc.
    let mut new_entries = new_entries;
    {
        use crate::file_system::listing::cached_listing::LISTING_CACHE;
        use crate::file_system::listing::sorting::sort_entries;

        if let Ok(cache) = LISTING_CACHE.read()
            && let Some(listing) = cache.get(listing_id)
        {
            index().enrich(&listing.volume_id, &mut new_entries);
            sort_entries(
                &mut new_entries,
                listing.sort_by,
                listing.sort_order,
                listing.directory_sort_mode,
            );
        }
    }

    // Compute diff
    let changes = compute_diff(&old_entries, &new_entries);

    if changes.is_empty() {
        return; // No actual changes
    }

    // Update the unified LISTING_CACHE with new entries. The overlays did NOT
    // re-run here: this is a diff against the entries a previous read already
    // decorated, so the stored contributed-row count still describes them.
    update_listing_entries(listing_id, new_entries, OverlayRows::Unchanged);

    crate::file_system::listing::diff_emitter::enqueue_diff(listing_id, changes);
}

/// Flushes pending watcher events by re-reading every active watch.
///
/// `notify-debouncer-full` doesn't expose a synchronous flush, and the
/// debouncer's window (plus FSEvents coalescing on macOS) adds 1–10 s of
/// latency per FS mutation under E2E. This helper sidesteps the debouncer:
/// it grabs every active listing_id, then `handle_directory_change` re-reads
/// each one via the Volume trait, computes the diff, updates LISTING_CACHE,
/// and emits a `directory-diff` event.
///
/// Feature-gated to `playwright-e2e` so production builds can't accidentally
/// bypass the debouncer (which exists to prevent thrash on bursts of events;
/// tests don't need that: they need determinism).
#[cfg(feature = "playwright-e2e")]
pub async fn flush_all_watchers() {
    let listing_ids: Vec<String> = WATCHER_MANAGER.read_ignore_poison().watches.keys().cloned().collect();
    log::debug!("flush_all_watchers: flushing {} watches", listing_ids.len());
    for id in listing_ids {
        handle_directory_change(&id).await;
    }
    // handle_directory_change now enqueues into the coalescer; flush so the
    // emit happens before this returns (E2E callers expect synchronous flush).
    crate::file_system::listing::diff_emitter::flush_all_pending();
}
