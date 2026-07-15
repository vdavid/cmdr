//! Lock-free enrichment of file listings with index data.
//!
//! Provides `enrich_entries_with_index` (called on every `get_file_range`)
//! and the `ReadPool` for thread-local SQLite read connections.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;

use super::firmlinks;
use super::state::ROOT_VOLUME_ID;
use super::store::{self, DirStatsById, IndexStore, IndexStoreError};
use crate::file_system::listing::FileEntry;
use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;

// ── Read pool (lock-free enrichment reads) ──────────────────────────

pub(crate) struct ReadPool {
    db_path: PathBuf,
    /// Incremented on shutdown/clear. Thread-local connections check this to detect staleness.
    generation: AtomicU64,
}

thread_local! {
    pub(super) static THREAD_CONN: RefCell<Option<(PathBuf, u64, Connection)>> = const { RefCell::new(None) };
}

impl ReadPool {
    pub(crate) fn new(db_path: PathBuf) -> Result<Self, IndexStoreError> {
        let _ = IndexStore::open_read_connection(&db_path)?; // Validate openable
        Ok(Self {
            db_path,
            generation: AtomicU64::new(0),
        })
    }

    /// Invalidate all thread-local connections. Next `with_conn` call reopens.
    pub(super) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Run `f` with a thread-local read connection.
    ///
    /// Thread-local safety: the `&Connection` can't escape the closure because
    /// `T` is lifetime-independent of the `&Connection` borrow. This means
    /// callers can't hold the connection across `.await` points (the compiler
    /// rejects it), so async task migration can't break thread affinity.
    pub(crate) fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T) -> Result<T, String> {
        let current_gen = self.generation.load(Ordering::Acquire);
        THREAD_CONN.with(|cell| {
            let mut slot = cell.borrow_mut();
            // Reuse if same path + same generation; otherwise reopen
            let needs_reopen = match slot.as_ref() {
                Some((p, g, _)) => p != &self.db_path || *g != current_gen,
                None => true,
            };
            if needs_reopen {
                let conn = IndexStore::open_read_connection(&self.db_path).map_err(|e| format!("{e}"))?;
                *slot = Some((self.db_path.clone(), current_gen, conn));
            }
            Ok(f(&slot
                .as_ref()
                .expect("slot is Some: needs_reopen is true whenever it was None, and we just set it")
                .2))
        })
    }
}

/// The root volume's read pool. The fast handle for local-disk enrichment,
/// search (D7 keeps search local-only), and IPC dir-stats. The root
/// `IndexInstance` shares this same `Arc`, so registry and read path can't drift.
///
/// Root is special-cased to this global (rather than read from the registry)
/// for two reasons: search reads it on the hot path, and the indexing tests
/// install it directly. Non-root volumes' pools live in their `IndexInstance`
/// (see `super::state::get_instance_read_pool`).
pub(super) static READ_POOL: LazyLock<std::sync::Mutex<Option<Arc<ReadPool>>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

/// Tests that touch `READ_POOL` must hold this lock to avoid races with parallel test threads.
#[cfg(test)]
pub(super) static READ_POOL_TEST_MUTEX: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));

/// Clone the root volume's pool Arc. Lock held for nanoseconds (just an Arc
/// clone). Kept for the search module (local-disk-only by D7) and the root
/// read-path callers.
pub(crate) fn get_read_pool() -> Option<Arc<ReadPool>> {
    READ_POOL.lock().ok()?.as_ref().cloned()
}

/// Clone a specific volume's read pool. Routes root to `READ_POOL`, every other
/// volume to its `IndexInstance` in the registry. `None` means "no index
/// registered for this volume" — the read path skips before any DB work.
pub(crate) fn get_read_pool_for(volume_id: &str) -> Option<Arc<ReadPool>> {
    if volume_id == ROOT_VOLUME_ID {
        get_read_pool()
    } else {
        super::state::get_instance_read_pool(volume_id)
    }
}

/// Install the root volume's read pool into the global fast handle. No-op for
/// non-root volumes: their pool is owned by the `IndexInstance` directly.
pub(super) fn install_read_pool(volume_id: &str, pool: Arc<ReadPool>) {
    if volume_id == ROOT_VOLUME_ID {
        *READ_POOL.lock_ignore_poison() = Some(pool);
    }
}

/// Clear the root volume's global read pool and return it (for invalidation on
/// stop/clear). Non-root volumes' pools are dropped with their `IndexInstance`;
/// this returns the instance's pool so the caller can `invalidate()` it.
pub(super) fn uninstall_read_pool(volume_id: &str) -> Option<Arc<ReadPool>> {
    if volume_id == ROOT_VOLUME_ID {
        READ_POOL.lock_ignore_poison().take()
    } else {
        super::state::get_instance_read_pool(volume_id)
    }
}

/// Test-only: install a root read pool over `db_path` (an existing index DB) so a
/// consumer in a sibling module (media-index scheduler tests) can drive a pass through
/// `get_read_pool_for("root")`. Callers MUST hold [`test_read_pool_lock`] for the
/// duration, since the root pool is a process-global shared across parallel tests.
#[cfg(test)]
pub(crate) fn test_install_root_read_pool(db_path: PathBuf) -> Result<(), IndexStoreError> {
    install_read_pool(ROOT_VOLUME_ID, Arc::new(ReadPool::new(db_path)?));
    Ok(())
}

/// Test-only: drop the root read pool installed by [`test_install_root_read_pool`].
#[cfg(test)]
pub(crate) fn test_uninstall_root_read_pool() {
    uninstall_read_pool(ROOT_VOLUME_ID);
}

/// Test-only: the shared lock serializing access to the process-global root read pool
/// across parallel test threads. A sibling module's test acquires this before
/// installing the root pool (mirrors `READ_POOL_TEST_MUTEX`'s use inside this module).
#[cfg(test)]
pub(crate) fn test_read_pool_lock() -> std::sync::MutexGuard<'static, ()> {
    READ_POOL_TEST_MUTEX.lock_ignore_poison()
}

/// The common parent directory of a sibling listing (all entries in a listing share one
/// parent). Returns `None` when the listing has no enrichable directory entry or the
/// first such entry's path is malformed (no `/`). Firmlink-normalized so it matches the
/// index's canonical paths.
fn listing_parent_path(entries: &[FileEntry]) -> Option<String> {
    let first_dir = entries.iter().find(|e| e.is_directory && !e.is_symlink)?;
    let normalized = firmlinks::normalize_path(&first_dir.path);
    match normalized.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(pos) => Some(normalized[..pos].to_string()),
        None => None,
    }
}

/// Enrich directory entries with recursive size data from the local (`root`)
/// index. Convenience wrapper for call sites without a `volume_id` in scope
/// (most local-listing paths) and for the read-path tests.
///
/// When only `root` is registered, routing every caller through root is
/// byte-identical to single-volume behaviour. Call sites that know the
/// listing's volume call `enrich_entries_with_index_on_volume` directly.
pub fn enrich_entries_with_index(entries: &mut [FileEntry]) {
    enrich_entries_with_index_on_volume(ROOT_VOLUME_ID, entries);
}

/// Enrich directory entries with recursive size data from a volume's index.
///
/// Called when entries land in the listing cache. Uses a per-volume `ReadPool`
/// for lock-free DB reads, so enrichment never blocks on the lifecycle mutex.
///
/// **Skip-vs-route gate**: if no index is registered for `volume_id`, return
/// before any DB work. This replaces the path-based `should_exclude(parent_path)`
/// early-return: when only `root` is registered, every non-root listing (SMB,
/// MTP, network mounts under their own volume ids) skips here, the same as
/// before. For the `root` volume we ALSO keep the path-based exclusion check
/// below, so a listing navigated to an excluded local path (`/Volumes/...`,
/// `/proc/...`) under the root volume id still skips — those paths aren't in
/// root's index, and enrichment would miss the parent lookup and log "Parent
/// path not found" on every refresh.
///
/// **Integer-keyed optimization**: Instead of resolving each directory path
/// individually, resolves the common parent directory once, gets all child
/// dir `(id, name)` pairs via `idx_parent`, then batch-fetches their
/// `dir_stats` by integer IDs. Two indexed queries total.
pub fn enrich_entries_with_index_on_volume(volume_id: &str, entries: &mut [FileEntry]) {
    // Skip if no index is registered for this volume: `get_read_pool_for`
    // returns `None`, which IS the "no index registered" signal (root's pool
    // lives in `READ_POOL`; a non-root volume's pool lives in its registry
    // instance). When only root is registered, this fires for every volume
    // except root, preserving the network-mount fast skip. A single lock-free
    // Arc-clone check.
    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => {
            log::debug!("enrich: no index registered for volume '{volume_id}'");
            return;
        }
    };

    // Find directory entries that need enrichment
    let has_dirs = entries.iter().any(|e| e.is_directory && !e.is_symlink);
    if !has_dirs {
        return;
    }

    let dir_count = entries.iter().filter(|e| e.is_directory && !e.is_symlink).count();

    let parent_path = match listing_parent_path(entries) {
        Some(p) => p,
        None => return,
    };

    // Skip parent paths the scan never indexed, so enrichment doesn't miss the
    // parent lookup and log "Parent path not found" on every listing refresh.
    // The `root` volume is the boot disk, so it excludes the whole boot-disk set
    // (network mounts under /Volumes, /mnt, /proc, system paths — none of which
    // are in root's index). Every other registered volume is mount-rooted, so it
    // applies only the per-volume junk tier: it must NOT exclude its own
    // `/Volumes/X/...` paths (those ARE its index), only junk like a
    // `.Spotlight-V100` a user navigated into.
    let scope = if volume_id == ROOT_VOLUME_ID {
        super::scanner::ExclusionScope::BootDisk
    } else {
        super::scanner::ExclusionScope::MountRooted
    };
    if super::scanner::should_exclude(&parent_path, scope) {
        return;
    }

    // Map the mount-absolute parent into the volume's index path space: a no-op
    // for `root`, mount-relative for an SMB volume (its index `ROOT_ID` is the
    // mount root, so a mount-absolute parent would resolve to nothing). `None`
    // means the parent isn't under this volume's mount root — skip.
    let index_parent_path = match super::routing::index_read_path(volume_id, &parent_path) {
        Some(p) => p,
        None => {
            log::debug!("enrich: parent {parent_path} not under volume '{volume_id}' mount root, skipping");
            return;
        }
    };

    log::debug!("enrich: {} under {parent_path}", pluralize(dir_count as u64, "dir"));

    // Read the volume's `current_epoch` once for this enrichment pass, on the
    // same connection that does the stats lookup. Absent (older / first-run DB)
    // reads as 1 (`read_current_epoch`), so a volume with no recorded epoch
    // behaves as "all current". This is what turns the stored `min_subtree_epoch`
    // into the FE-facing `recursive_size_complete` / `recursive_size_stale`
    // booleans — the FE never learns the epoch scheme.
    //
    // Use the integer-keyed fast path: resolve parent once, batch-fetch child stats
    if let Err(e) = pool
        .with_conn(|conn| {
            let current_epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
            enrich_via_parent_id_on(entries, conn, &index_parent_path, current_epoch)
        })
        .and_then(|r| r)
    {
        log::debug!("Enrichment fast path failed: {e}, trying fallback");
        // Fallback: resolve each path individually (handles mixed-parent edge cases)
        let _ = pool.with_conn(|conn| {
            let current_epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
            enrich_via_individual_paths_on(volume_id, entries, conn, current_epoch)
        });
    }

    let enriched = entries
        .iter()
        .filter(|e| e.is_directory && !e.is_symlink && e.recursive_size.is_some())
        .count();
    log::debug!("enrich: {enriched}/{} got sizes", pluralize(dir_count as u64, "dir"));
}

/// Copy a directory's aggregated stats onto its `FileEntry`, deriving the
/// FE-facing honest-size booleans from `min_subtree_epoch` vs `current_epoch`.
///
/// - `recursive_size_complete = min_subtree_epoch > 0` (subtree fully covered ⇒
///   the size is exact, not a lower bound).
/// - `recursive_size_stale = 0 < min_subtree_epoch < current_epoch` (exact, but
///   computed at an older epoch than the current one).
///
/// The FE never sees raw epochs; it renders from `{recursive_size, complete,
/// stale}` alone. See the "Honest sizes" model in DETAILS.
fn apply_dir_stats(entry: &mut FileEntry, stats: &DirStatsById, current_epoch: u64) {
    entry.recursive_size = Some(stats.recursive_logical_size);
    entry.recursive_physical_size = Some(stats.recursive_physical_size);
    entry.recursive_file_count = Some(stats.recursive_file_count);
    entry.recursive_dir_count = Some(stats.recursive_dir_count);
    entry.recursive_has_symlinks = Some(stats.recursive_has_symlinks);
    let complete = stats.min_subtree_epoch > 0;
    entry.recursive_size_complete = Some(complete);
    entry.recursive_size_stale = Some(complete && stats.min_subtree_epoch < current_epoch);
}

/// Fast path: resolve parent dir → id, get child dir IDs, batch-fetch stats.
pub(super) fn enrich_via_parent_id_on(
    entries: &mut [FileEntry],
    conn: &Connection,
    parent_path: &str,
    current_epoch: u64,
) -> Result<(), String> {
    let t0 = std::time::Instant::now();

    // Resolve parent directory path → entry ID (one tree walk, almost always cached)
    let parent_id = match store::resolve_path(conn, parent_path).map_err(|e| format!("{e}"))? {
        Some(id) => id,
        None => return Err(format!("Parent path not found in index: {parent_path}")),
    };
    let resolve_parent_ms = t0.elapsed().as_millis();

    // Get all child directory (id, name) pairs
    let t1 = std::time::Instant::now();
    let child_dirs = IndexStore::list_child_dir_ids_and_names(conn, parent_id).map_err(|e| format!("{e}"))?;
    let list_children_ms = t1.elapsed().as_millis();

    if child_dirs.is_empty() {
        return Ok(());
    }

    // Batch-fetch dir_stats by integer IDs
    let t2 = std::time::Instant::now();
    let child_ids: Vec<i64> = child_dirs.iter().map(|(id, _)| *id).collect();
    let stats_batch = IndexStore::get_dir_stats_batch_by_ids(conn, &child_ids).map_err(|e| format!("{e}"))?;
    let batch_stats_ms = t2.elapsed().as_millis();

    // Build name → DirStatsById map (using normalized names for matching)
    let t3 = std::time::Instant::now();
    let mut name_to_stats: std::collections::HashMap<String, DirStatsById> =
        std::collections::HashMap::with_capacity(child_dirs.len());
    for ((_, name), stats_opt) in child_dirs.into_iter().zip(stats_batch) {
        if let Some(stats) = stats_opt {
            name_to_stats.insert(store::normalize_for_comparison(&name), stats);
        }
    }

    // Apply stats to entries by matching normalized basenames
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        let basename = match entry.path.rfind('/') {
            Some(pos) => &entry.path[pos + 1..],
            None => &entry.path,
        };
        let normalized_name = store::normalize_for_comparison(basename);
        if let Some(stats) = name_to_stats.get(&normalized_name) {
            apply_dir_stats(entry, stats, current_epoch);
        }
    }
    let match_ms = t3.elapsed().as_millis();
    let total_ms = t0.elapsed().as_millis();

    // Phase 1: only log when slow (>50ms) to keep noise low.
    if total_ms > 50 {
        log::debug!(
            target: "stall_probe::enrich",
            "enrich_slow parent={} entries={} resolve_parent_ms={} list_children_ms={} batch_stats_ms={} match_ms={} total_ms={}",
            parent_path,
            entries.len(),
            resolve_parent_ms,
            list_children_ms,
            batch_stats_ms,
            match_ms,
            total_ms,
        );
    }
    Ok(())
}

/// Fallback: resolve each directory path individually (handles mixed-parent entries).
///
/// Each entry's mount-absolute path is mapped into the volume's index path space
/// (mount-relative for SMB) before `resolve_path`, mirroring the fast path.
pub(super) fn enrich_via_individual_paths_on(
    volume_id: &str,
    entries: &mut [FileEntry],
    conn: &Connection,
    current_epoch: u64,
) {
    // Resolve each dir path → entry_id, then batch-fetch stats. We key the stats
    // map on the index-rooted path (what resolved) so the apply loop below, which
    // recomputes the same index-rooted path per entry, matches.
    let mut id_to_path: Vec<(i64, String)> = Vec::new();
    for entry in entries.iter().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        let Some(index_path) = super::routing::index_read_path(volume_id, &normalized) else {
            continue;
        };
        if let Ok(Some(id)) = store::resolve_path(conn, &index_path) {
            id_to_path.push((id, index_path));
        }
    }

    if id_to_path.is_empty() {
        return;
    }

    let ids: Vec<i64> = id_to_path.iter().map(|(id, _)| *id).collect();
    let stats_batch = match IndexStore::get_dir_stats_batch_by_ids(conn, &ids) {
        Ok(s) => s,
        Err(e) => {
            log::debug!("Index enrichment fallback failed: {e}");
            return;
        }
    };

    // Map normalized path -> DirStatsById for lookup
    let mut stats_map: std::collections::HashMap<String, DirStatsById> =
        std::collections::HashMap::with_capacity(id_to_path.len());
    for ((_, path), stats_opt) in id_to_path.into_iter().zip(stats_batch) {
        if let Some(s) = stats_opt {
            stats_map.insert(path, s);
        }
    }

    // Apply to entries (key on the same index-rooted path the map was built with)
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        let Some(index_path) = super::routing::index_read_path(volume_id, &normalized) else {
            continue;
        };
        if let Some(stats) = stats_map.get(&index_path) {
            apply_dir_stats(entry, stats, current_epoch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(path: &str) -> FileEntry {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        FileEntry::new(name, path.to_string(), true, false)
    }

    fn stats_with_epoch(min_subtree_epoch: u64) -> DirStatsById {
        DirStatsById {
            entry_id: 1,
            recursive_logical_size: 1234,
            recursive_physical_size: 1234,
            recursive_file_count: 5,
            recursive_dir_count: 2,
            recursive_has_symlinks: false,
            min_subtree_epoch,
        }
    }

    /// `apply_dir_stats` derives the FE-facing honest-size booleans from
    /// `min_subtree_epoch` vs `current_epoch`. This is the read-side contract
    /// The FE never sees raw epochs.
    #[test]
    fn apply_dir_stats_derives_complete_and_stale() {
        let current_epoch = 5;

        // Incomplete subtree: `min_subtree_epoch == 0` ⇒ lower bound, not stale.
        let mut e = dir("/a");
        apply_dir_stats(&mut e, &stats_with_epoch(0), current_epoch);
        assert_eq!(e.recursive_size, Some(1234));
        assert_eq!(e.recursive_size_complete, Some(false));
        assert_eq!(e.recursive_size_stale, Some(false));

        // Complete and current: `min_subtree_epoch == current_epoch` ⇒ exact, fresh.
        let mut e = dir("/a");
        apply_dir_stats(&mut e, &stats_with_epoch(current_epoch), current_epoch);
        assert_eq!(e.recursive_size_complete, Some(true));
        assert_eq!(e.recursive_size_stale, Some(false));

        // Complete but older: `0 < min_subtree_epoch < current_epoch` ⇒ exact, stale.
        let mut e = dir("/a");
        apply_dir_stats(&mut e, &stats_with_epoch(3), current_epoch);
        assert_eq!(e.recursive_size_complete, Some(true));
        assert_eq!(e.recursive_size_stale, Some(true));
    }

    #[test]
    fn listing_parent_path_finds_common_parent() {
        let entries = [dir("/Users/veszelovszki/project"), dir("/Users/veszelovszki/other")];
        assert_eq!(listing_parent_path(&entries).as_deref(), Some("/Users/veszelovszki"));
    }

    #[test]
    fn listing_parent_path_handles_root_children() {
        assert_eq!(listing_parent_path(&[dir("/Users")]).as_deref(), Some("/"));
    }

    #[test]
    fn listing_parent_path_none_without_enrichable_dirs() {
        let files = [FileEntry::new("a.txt".into(), "/Users/a.txt".into(), false, false)];
        assert_eq!(listing_parent_path(&files), None);
    }

    /// A listing under an excluded prefix (network mount / external drive under /Volumes
    /// on macOS, /mnt on Linux, system virtual filesystems on both) must be skipped:
    /// those entries are never in the index, so enrichment would fail the parent lookup
    /// and log "Parent path not found" on every listing. `/proc/` is in the excluded set
    /// on both platforms, so it keeps this assertion platform-agnostic.
    #[test]
    fn excluded_listing_parents_are_skipped() {
        let parent = listing_parent_path(&[dir("/proc/123/fd")]).expect("has a parent");
        assert!(
            super::super::scanner::should_exclude(&parent, super::super::scanner::ExclusionScope::BootDisk),
            "an excluded system path must be skipped by enrichment"
        );
        // A boot-volume listing is NOT excluded — enrichment must still run there.
        let home = listing_parent_path(&[dir("/Users/veszelovszki/project")]).expect("has a parent");
        assert!(!super::super::scanner::should_exclude(
            &home,
            super::super::scanner::ExclusionScope::BootDisk
        ));
    }
}
