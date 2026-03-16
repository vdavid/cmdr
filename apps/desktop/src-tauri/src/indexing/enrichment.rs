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
use super::store::{self, DirStatsById, IndexStore, IndexStoreError};
use crate::file_system::listing::FileEntry;

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
            Ok(f(&slot.as_ref().unwrap().2))
        })
    }
}

pub(super) static READ_POOL: LazyLock<std::sync::Mutex<Option<Arc<ReadPool>>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

/// Tests that touch `READ_POOL` must hold this lock to avoid races with parallel test threads.
#[cfg(test)]
pub(super) static READ_POOL_TEST_MUTEX: LazyLock<std::sync::Mutex<()>> = LazyLock::new(|| std::sync::Mutex::new(()));

/// Clone the pool Arc. Lock held for nanoseconds — just an Arc clone.
pub(crate) fn get_read_pool() -> Option<Arc<ReadPool>> {
    READ_POOL.lock().ok()?.as_ref().cloned()
}

/// Enrich directory entries with recursive size data from the index.
///
/// Called from `get_file_range` on every page fetch. Does nothing if
/// indexing is not initialized. Uses a `ReadPool` for lock-free DB reads,
/// so enrichment never blocks on the `INDEXING` state-machine mutex.
///
/// **Integer-keyed optimization**: Instead of resolving each directory path
/// individually, resolves the common parent directory once, gets all child
/// dir `(id, name)` pairs via `idx_parent`, then batch-fetches their
/// `dir_stats` by integer IDs. Two indexed queries total.
pub fn enrich_entries_with_index(entries: &mut [FileEntry]) {
    let pool = match get_read_pool() {
        Some(p) => p,
        None => {
            log::debug!("enrich: no read pool");
            return;
        }
    };

    // Find directory entries that need enrichment
    let has_dirs = entries.iter().any(|e| e.is_directory && !e.is_symlink);
    if !has_dirs {
        return;
    }

    let dir_count = entries.iter().filter(|e| e.is_directory && !e.is_symlink).count();

    // Determine the common parent directory from the first directory entry.
    // All entries in a listing share the same parent (they're siblings).
    let parent_path = match entries.iter().find(|e| e.is_directory && !e.is_symlink) {
        Some(e) => {
            let normalized = firmlinks::normalize_path(&e.path);
            // Parent = path without the last component
            match normalized.rfind('/') {
                Some(0) => "/".to_string(),
                Some(pos) => normalized[..pos].to_string(),
                None => return, // Malformed path, skip
            }
        }
        None => return,
    };

    log::debug!("enrich: {dir_count} dirs under {parent_path}");

    // Use the integer-keyed fast path: resolve parent once, batch-fetch child stats
    if let Err(e) = pool
        .with_conn(|conn| enrich_via_parent_id_on(entries, conn, &parent_path))
        .and_then(|r| r)
    {
        log::debug!("Enrichment fast path failed: {e}, trying fallback");
        // Fallback: resolve each path individually (handles mixed-parent edge cases)
        let _ = pool.with_conn(|conn| enrich_via_individual_paths_on(entries, conn));
    }

    let enriched = entries
        .iter()
        .filter(|e| e.is_directory && !e.is_symlink && e.recursive_size.is_some())
        .count();
    log::debug!("enrich: {enriched}/{dir_count} dirs got sizes");
}

/// Fast path: resolve parent dir → id, get child dir IDs, batch-fetch stats.
pub(super) fn enrich_via_parent_id_on(
    entries: &mut [FileEntry],
    conn: &Connection,
    parent_path: &str,
) -> Result<(), String> {
    // Resolve parent directory path → entry ID (one tree walk, almost always cached)
    let parent_id = match store::resolve_path(conn, parent_path).map_err(|e| format!("{e}"))? {
        Some(id) => id,
        None => return Err(format!("Parent path not found in index: {parent_path}")),
    };

    // Get all child directory (id, name) pairs
    let child_dirs = IndexStore::list_child_dir_ids_and_names(conn, parent_id).map_err(|e| format!("{e}"))?;

    if child_dirs.is_empty() {
        return Ok(());
    }

    // Batch-fetch dir_stats by integer IDs
    let child_ids: Vec<i64> = child_dirs.iter().map(|(id, _)| *id).collect();
    let stats_batch = IndexStore::get_dir_stats_batch_by_ids(conn, &child_ids).map_err(|e| format!("{e}"))?;

    // Build name → DirStatsById map (using normalized names for matching)
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
            entry.recursive_size = Some(stats.recursive_size);
            entry.recursive_file_count = Some(stats.recursive_file_count);
            entry.recursive_dir_count = Some(stats.recursive_dir_count);
        }
    }
    Ok(())
}

/// Fallback: resolve each directory path individually (handles mixed-parent entries).
pub(super) fn enrich_via_individual_paths_on(entries: &mut [FileEntry], conn: &Connection) {
    // Resolve each dir path → entry_id, then batch-fetch stats
    let mut id_to_path: Vec<(i64, String)> = Vec::new();
    for entry in entries.iter().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        if let Ok(Some(id)) = store::resolve_path(conn, &normalized) {
            id_to_path.push((id, normalized));
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

    // Apply to entries
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        let normalized = firmlinks::normalize_path(&entry.path);
        if let Some(stats) = stats_map.get(&normalized) {
            entry.recursive_size = Some(stats.recursive_size);
            entry.recursive_file_count = Some(stats.recursive_file_count);
            entry.recursive_dir_count = Some(stats.recursive_dir_count);
        }
    }
}
