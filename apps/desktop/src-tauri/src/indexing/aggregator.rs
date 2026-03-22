//! Dir stats computation: bottom-up aggregation and incremental delta propagation.
//!
//! Three modes:
//! - **Full aggregation**: after a full scan, compute `dir_stats` for every directory (deepest first).
//! - **Subtree aggregation**: after a subtree scan, compute `dir_stats` only under a given root.
//! - **Delta propagation**: after a watcher event, walk up the ancestor chain updating counts.
//!
//! All queries use the integer-keyed schema v2 (`id`, `parent_id`, `entry_id`).

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::indexing::store::{DirStatsById, IndexStore, IndexStoreError, resolve_path};

/// `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count)`.
type ChildrenStatsMap = HashMap<i64, (u64, u64, u64, u64)>;

/// Progress phases reported during full aggregation.
/// Wired up by the progress-reporting layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationPhase {
    /// Flushing remaining entry batches from the writer channel to DB.
    SavingEntries,
    /// Loading directory IDs from DB (fast).
    LoadingDirectories,
    /// Topological sort (fast, ~1s).
    Sorting,
    /// Bottom-up recursive computation.
    Computing,
    /// Batch-writing dir_stats rows to DB.
    Writing,
}

/// Progress update emitted during aggregation.
/// Wired up by the progress-reporting layer.
#[derive(Debug, Clone)]
pub struct AggregationProgress {
    pub phase: AggregationPhase,
    /// Current item within the phase (0-based).
    pub current: u64,
    /// Total items in the phase (0 if unknown).
    pub total: u64,
}

impl AggregationProgress {
    pub(crate) fn new(phase: AggregationPhase, current: u64, total: u64) -> Self {
        Self { phase, current, total }
    }
}

/// Convenience wrapper: compute all aggregates without progress reporting.
///
/// Used by tests and as a simple entry point. Delegates to `compute_all_aggregates_reported`
/// with a no-op callback.
#[cfg(test)]
pub fn compute_all_aggregates(conn: &Connection) -> Result<u64, IndexStoreError> {
    compute_all_aggregates_reported(conn, &mut |_| {})
}

/// Same as `compute_all_aggregates` but calls `on_progress` at each phase transition
/// and periodically during the compute/write loops.
pub fn compute_all_aggregates_reported(
    conn: &Connection,
    on_progress: &mut dyn FnMut(AggregationProgress),
) -> Result<u64, IndexStoreError> {
    let start = std::time::Instant::now();

    on_progress(AggregationProgress::new(AggregationPhase::LoadingDirectories, 0, 0));

    // Load all directory (id, parent_id) pairs including root sentinel
    let dir_entries = load_all_directory_ids(conn)?;
    if dir_entries.is_empty() {
        return Ok(0);
    }

    let dir_count = dir_entries.len();
    log::debug!("Aggregation: starting bottom-up computation for {dir_count} directories");

    // Bulk-load direct children stats for ALL parent IDs in two SQL queries
    log::debug!("Aggregation: loading direct children stats (bulk query)...");
    let direct_stats = bulk_get_children_stats_by_id(conn)?;
    log::debug!(
        "Aggregation: loaded stats for {} parent IDs in {:.1}s",
        direct_stats.len(),
        start.elapsed().as_secs_f64()
    );

    log::debug!("Aggregation: loading child directory relationships (bulk query)...");
    let child_dirs_map = bulk_get_child_dir_ids(conn)?;
    log::debug!(
        "Aggregation: loaded child dirs for {} parent IDs in {:.1}s",
        child_dirs_map.len(),
        start.elapsed().as_secs_f64()
    );

    compute_and_write(conn, &dir_entries, &direct_stats, &child_dirs_map, on_progress)
}

/// Compute `dir_stats` for ALL directories using pre-built in-memory maps.
///
/// Called by the writer thread with maps accumulated during `InsertEntriesV2`
/// processing. Skips the two expensive bulk SQL queries (full-table scans)
/// that dominate aggregation time on large indexes.
/// Falls back to `compute_all_aggregates` if the maps are empty (edge case).
pub fn compute_all_aggregates_with_maps(
    conn: &Connection,
    direct_stats: &ChildrenStatsMap,
    child_dirs: &HashMap<i64, Vec<i64>>,
    on_progress: &mut dyn FnMut(AggregationProgress),
) -> Result<u64, IndexStoreError> {
    on_progress(AggregationProgress::new(AggregationPhase::LoadingDirectories, 0, 0));

    let dir_entries = load_all_directory_ids(conn)?;
    if dir_entries.is_empty() {
        return Ok(0);
    }

    log::debug!(
        "Aggregation (with maps): starting bottom-up computation for {} directories \
         (direct_stats={}, child_dirs={})",
        dir_entries.len(),
        direct_stats.len(),
        child_dirs.len(),
    );

    compute_and_write(conn, &dir_entries, direct_stats, child_dirs, on_progress)
}

/// Shared core: topological sort, bottom-up computation, batch write.
///
/// Calls `on_progress` at phase transitions and every ~1% during compute/write loops.
fn compute_and_write(
    conn: &Connection,
    dir_entries: &[(i64, i64)],
    direct_stats: &ChildrenStatsMap,
    child_dirs_map: &HashMap<i64, Vec<i64>>,
    on_progress: &mut dyn FnMut(AggregationProgress),
) -> Result<u64, IndexStoreError> {
    let start = std::time::Instant::now();
    let dir_count = dir_entries.len() as u64;

    on_progress(AggregationProgress::new(AggregationPhase::Sorting, 0, dir_count));
    let sorted = topological_sort_bottom_up(dir_entries);

    // Report every ~1% of progress, but at least every 1000 items
    let compute_report_interval = (dir_count / 100).max(1000).min(dir_count.max(1)) as usize;

    on_progress(AggregationProgress::new(AggregationPhase::Computing, 0, dir_count));
    let mut computed: HashMap<i64, DirStatsById> = HashMap::with_capacity(sorted.len());

    for (i, &dir_id) in sorted.iter().enumerate() {
        let (logical_size_sum, physical_size_sum, file_count, child_dir_count) =
            direct_stats.get(&dir_id).copied().unwrap_or((0, 0, 0, 0));

        let mut recursive_logical_size = logical_size_sum;
        let mut recursive_physical_size = physical_size_sum;
        let mut recursive_file_count = file_count;
        let mut recursive_dir_count = child_dir_count;

        // Add already-computed recursive stats from child directories
        if let Some(children) = child_dirs_map.get(&dir_id) {
            for &child_id in children {
                if let Some(child_stats) = computed.get(&child_id) {
                    recursive_logical_size += child_stats.recursive_logical_size;
                    recursive_physical_size += child_stats.recursive_physical_size;
                    recursive_file_count += child_stats.recursive_file_count;
                    recursive_dir_count += child_stats.recursive_dir_count;
                }
            }
        }

        computed.insert(
            dir_id,
            DirStatsById {
                entry_id: dir_id,
                recursive_logical_size,
                recursive_physical_size,
                recursive_file_count,
                recursive_dir_count,
            },
        );

        if (i + 1) % compute_report_interval == 0 {
            on_progress(AggregationProgress::new(
                AggregationPhase::Computing,
                (i + 1) as u64,
                dir_count,
            ));
            log::debug!(
                "Aggregation: processed {}/{dir_count} directories ({:.1}s)",
                i + 1,
                start.elapsed().as_secs_f64()
            );
        }
    }

    // Batch-write all computed stats in chunks of 1000
    log::debug!("Aggregation: writing {} dir_stats rows to DB...", computed.len());
    let all_stats: Vec<DirStatsById> = computed.into_values().collect();
    let count = all_stats.len() as u64;
    let total_chunks = count.div_ceil(1000);
    let write_report_interval = (total_chunks / 100).max(1) as usize;

    on_progress(AggregationProgress::new(AggregationPhase::Writing, 0, count));

    for (chunk_idx, chunk) in all_stats.chunks(1000).enumerate() {
        IndexStore::upsert_dir_stats_by_id(conn, chunk)?;
        if (chunk_idx + 1) % write_report_interval == 0 {
            let written = ((chunk_idx + 1) as u64 * 1000).min(count);
            on_progress(AggregationProgress::new(AggregationPhase::Writing, written, count));
        }
    }

    log::debug!(
        "Aggregation: complete. {count} directories processed in {:.1}s",
        start.elapsed().as_secs_f64()
    );

    Ok(count)
}

/// Compute `dir_stats` for directories under `root` only (bottom-up).
///
/// Called after a subtree scan completes. Resolves the root path to an entry ID,
/// uses a recursive CTE to collect subtree directory IDs, then computes stats
/// bottom-up. Returns the number of directories processed.
pub fn compute_subtree_aggregates(conn: &Connection, root: &str) -> Result<u64, IndexStoreError> {
    let root_id = match resolve_path(conn, root)? {
        Some(id) => id,
        None => return Ok(0),
    };

    let dir_entries = load_subtree_directory_ids(conn, root_id)?;
    if dir_entries.is_empty() {
        return Ok(0);
    }

    let start = std::time::Instant::now();
    let dir_count = dir_entries.len();
    log::debug!("Subtree aggregation: starting bottom-up computation for {dir_count} directories under {root}");

    // Load direct children stats scoped to this subtree via recursive CTE
    let direct_stats = scoped_get_children_stats_by_id(conn, root_id)?;
    log::debug!(
        "Subtree aggregation: loaded stats for {} parent IDs in {:.1}ms",
        direct_stats.len(),
        start.elapsed().as_secs_f64() * 1000.0,
    );

    let child_dirs_map = scoped_get_child_dir_ids(conn, root_id)?;
    log::debug!(
        "Subtree aggregation: loaded child dirs for {} parent IDs in {:.1}ms",
        child_dirs_map.len(),
        start.elapsed().as_secs_f64() * 1000.0,
    );

    // Topological sort: leaves first
    let sorted = topological_sort_bottom_up(&dir_entries);

    let mut computed: HashMap<i64, DirStatsById> = HashMap::with_capacity(sorted.len());

    for &dir_id in &sorted {
        let (logical_size_sum, physical_size_sum, file_count, child_dir_count) =
            direct_stats.get(&dir_id).copied().unwrap_or((0, 0, 0, 0));

        let mut recursive_logical_size = logical_size_sum;
        let mut recursive_physical_size = physical_size_sum;
        let mut recursive_file_count = file_count;
        let mut recursive_dir_count = child_dir_count;

        if let Some(children) = child_dirs_map.get(&dir_id) {
            for &child_id in children {
                if let Some(child_stats) = computed.get(&child_id) {
                    recursive_logical_size += child_stats.recursive_logical_size;
                    recursive_physical_size += child_stats.recursive_physical_size;
                    recursive_file_count += child_stats.recursive_file_count;
                    recursive_dir_count += child_stats.recursive_dir_count;
                }
            }
        }

        computed.insert(
            dir_id,
            DirStatsById {
                entry_id: dir_id,
                recursive_logical_size,
                recursive_physical_size,
                recursive_file_count,
                recursive_dir_count,
            },
        );
    }

    // Batch-write all computed stats
    log::debug!(
        "Subtree aggregation: writing {} dir_stats rows to DB...",
        computed.len()
    );
    let all_stats: Vec<DirStatsById> = computed.into_values().collect();
    let count = all_stats.len() as u64;

    for chunk in all_stats.chunks(1000) {
        IndexStore::upsert_dir_stats_by_id(conn, chunk)?;
    }

    log::debug!(
        "Subtree aggregation: complete. {count} directories processed in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0,
    );

    Ok(count)
}

/// Backfill `dir_stats` for directories that have entries but no stats row.
///
/// Finds all directories missing a `dir_stats` row and computes their stats
/// bottom-up. This catches directories created by reconciler/live events
/// after the last full aggregation. Returns the number of dirs backfilled.
pub fn backfill_missing_dir_stats(conn: &Connection) -> Result<u64, IndexStoreError> {
    // Find directories without dir_stats
    let missing_ids = load_dirs_missing_stats(conn)?;
    if missing_ids.is_empty() {
        return Ok(0);
    }

    let start = std::time::Instant::now();
    let count = missing_ids.len();
    log::debug!("Backfill: {count} directories missing dir_stats, computing...");

    // Load ALL directory entries for the topological sort (we need the full
    // tree structure to compute bottom-up correctly, since a missing dir's
    // children may also be missing).
    let all_dir_entries = load_all_directory_ids(conn)?;

    // Build direct_stats and child_dirs maps scoped to the missing dirs
    // and their descendants. We use the full-table bulk queries since the
    // missing dirs can be scattered across the tree.
    let direct_stats = bulk_get_children_stats_by_id(conn)?;
    let child_dirs_map = bulk_get_child_dir_ids(conn)?;

    // Topological sort all dirs (we need correct ordering)
    let sorted = topological_sort_bottom_up(&all_dir_entries);

    // Build set of missing IDs for fast lookup
    let missing_set: std::collections::HashSet<i64> = missing_ids.into_iter().collect();

    // Compute stats bottom-up for ALL dirs, but only write the missing ones.
    // We need to compute all because a missing dir's stats depend on its
    // children (which might have existing stats in the DB or might also be
    // missing).
    let mut computed: HashMap<i64, DirStatsById> = HashMap::with_capacity(sorted.len());
    let mut to_write: Vec<DirStatsById> = Vec::with_capacity(count);

    for &dir_id in &sorted {
        let (logical_size_sum, physical_size_sum, file_count, child_dir_count) =
            direct_stats.get(&dir_id).copied().unwrap_or((0, 0, 0, 0));

        let mut recursive_logical_size = logical_size_sum;
        let mut recursive_physical_size = physical_size_sum;
        let mut recursive_file_count = file_count;
        let mut recursive_dir_count = child_dir_count;

        if let Some(children) = child_dirs_map.get(&dir_id) {
            for &child_id in children {
                // Prefer freshly computed stats, fall back to existing DB stats
                if let Some(child_stats) = computed.get(&child_id) {
                    recursive_logical_size += child_stats.recursive_logical_size;
                    recursive_physical_size += child_stats.recursive_physical_size;
                    recursive_file_count += child_stats.recursive_file_count;
                    recursive_dir_count += child_stats.recursive_dir_count;
                } else if let Ok(Some(db_stats)) = IndexStore::get_dir_stats_by_id(conn, child_id) {
                    recursive_logical_size += db_stats.recursive_logical_size;
                    recursive_physical_size += db_stats.recursive_physical_size;
                    recursive_file_count += db_stats.recursive_file_count;
                    recursive_dir_count += db_stats.recursive_dir_count;
                }
            }
        }

        let stats = DirStatsById {
            entry_id: dir_id,
            recursive_logical_size,
            recursive_physical_size,
            recursive_file_count,
            recursive_dir_count,
        };

        if missing_set.contains(&dir_id) {
            to_write.push(stats.clone());
        }
        computed.insert(dir_id, stats);
    }

    // Batch-write only the missing stats
    for chunk in to_write.chunks(1000) {
        IndexStore::upsert_dir_stats_by_id(conn, chunk)?;
    }

    log::debug!(
        "Backfill: wrote {} dir_stats rows in {:.1}s",
        to_write.len(),
        start.elapsed().as_secs_f64(),
    );

    Ok(to_write.len() as u64)
}

/// Load directory IDs that have entries but no `dir_stats` row.
fn load_dirs_missing_stats(conn: &Connection) -> Result<Vec<i64>, IndexStoreError> {
    let mut stmt = conn.prepare(
        "SELECT e.id FROM entries e
         LEFT JOIN dir_stats ds ON ds.entry_id = e.id
         WHERE e.is_directory = 1 AND ds.entry_id IS NULL",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ── Internal helpers ─────────────────────────────────────────────────

/// Load all directory `(id, parent_id)` pairs from the entries table.
fn load_all_directory_ids(conn: &Connection) -> Result<Vec<(i64, i64)>, IndexStoreError> {
    let mut stmt = conn.prepare("SELECT id, parent_id FROM entries WHERE is_directory = 1")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Load directory `(id, parent_id)` pairs for a subtree rooted at `root_id`.
///
/// Uses a recursive CTE to collect all entries under the root, then filters
/// for directories only.
fn load_subtree_directory_ids(conn: &Connection, root_id: i64) -> Result<Vec<(i64, i64)>, IndexStoreError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM entries WHERE id = ?1
            UNION ALL
            SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
        )
        SELECT e.id, e.parent_id FROM entries e
        WHERE e.id IN (SELECT id FROM subtree) AND e.is_directory = 1",
    )?;
    let rows = stmt.query_map(params![root_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Topological sort: returns directory IDs in bottom-up order (leaves first).
///
/// Builds a children map from `(id, parent_id)` pairs, then iterates from leaves
/// to root. This is equivalent to sorting by depth descending but works correctly
/// with integer IDs (no path depth counting needed).
fn topological_sort_bottom_up(entries: &[(i64, i64)]) -> Vec<i64> {
    if entries.is_empty() {
        return Vec::new();
    }

    let id_set: std::collections::HashSet<i64> = entries.iter().map(|&(id, _)| id).collect();

    // Build a map from child_id -> parent_id (within the set)
    let parent_of: HashMap<i64, i64> = entries
        .iter()
        .filter(|&&(_, pid)| id_set.contains(&pid))
        .map(|&(id, pid)| (id, pid))
        .collect();

    // Count how many children each node has within the set (in-degree for reverse topo)
    let mut child_count: HashMap<i64, usize> = entries.iter().map(|&(id, _)| (id, 0)).collect();
    for &parent_id in parent_of.values() {
        *child_count.entry(parent_id).or_insert(0) += 1;
    }

    // Start from leaves (nodes with no children in the set)
    let mut queue: Vec<i64> = child_count
        .iter()
        .filter(|&(_, &count)| count == 0)
        .map(|(&id, _)| id)
        .collect();
    queue.sort_unstable(); // Deterministic output

    let mut result = Vec::with_capacity(entries.len());
    let mut processed = std::collections::HashSet::new();

    while let Some(id) = queue.pop() {
        if !processed.insert(id) {
            continue;
        }
        result.push(id);

        // Decrement parent's child count; enqueue parent when it becomes a leaf
        if let Some(&parent_id) = parent_of.get(&id)
            && let Some(count) = child_count.get_mut(&parent_id)
        {
            *count = count.saturating_sub(1);
            if *count == 0 && !processed.contains(&parent_id) {
                queue.push(parent_id);
            }
        }
    }

    result
}

/// Bulk-load direct children stats for ALL parent IDs in a single SQL query.
///
/// Returns a map: `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count)`.
fn bulk_get_children_stats_by_id(conn: &Connection) -> Result<ChildrenStatsMap, IndexStoreError> {
    let mut stmt = conn.prepare(
        "SELECT parent_id,
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN logical_size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN physical_size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 1 THEN 1 ELSE 0 END), 0)
         FROM entries
         GROUP BY parent_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, u64>(1)?,
            row.get::<_, u64>(2)?,
            row.get::<_, u64>(3)?,
            row.get::<_, u64>(4)?,
        ))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (parent_id, logical_size, physical_size, files, dirs) = row?;
        map.insert(parent_id, (logical_size, physical_size, files, dirs));
    }
    Ok(map)
}

/// Bulk-load child directory IDs for ALL parent IDs in a single SQL query.
///
/// Returns a map: `parent_id -> Vec<child_dir_id>`.
fn bulk_get_child_dir_ids(conn: &Connection) -> Result<HashMap<i64, Vec<i64>>, IndexStoreError> {
    let mut stmt = conn.prepare("SELECT parent_id, id FROM entries WHERE is_directory = 1")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;

    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    for row in rows {
        let (parent_id, child_id) = row?;
        map.entry(parent_id).or_default().push(child_id);
    }
    Ok(map)
}

/// Load direct children stats scoped to a subtree via recursive CTE.
///
/// Returns a map: `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count)`.
/// Only includes entries whose parent is within the subtree rooted at `root_id`.
fn scoped_get_children_stats_by_id(conn: &Connection, root_id: i64) -> Result<ChildrenStatsMap, IndexStoreError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM entries WHERE id = ?1
            UNION ALL
            SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
        )
        SELECT e.parent_id,
               COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN e.logical_size ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN e.physical_size ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN e.is_directory = 1 THEN 1 ELSE 0 END), 0)
        FROM entries e
        WHERE e.parent_id IN (SELECT id FROM subtree)
        GROUP BY e.parent_id",
    )?;
    let rows = stmt.query_map(params![root_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, u64>(1)?,
            row.get::<_, u64>(2)?,
            row.get::<_, u64>(3)?,
            row.get::<_, u64>(4)?,
        ))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (parent_id, logical_size, physical_size, files, dirs) = row?;
        map.insert(parent_id, (logical_size, physical_size, files, dirs));
    }
    Ok(map)
}

/// Load child directory IDs scoped to a subtree via recursive CTE.
///
/// Returns a map: `parent_id -> Vec<child_dir_id>`.
/// Only includes entries whose parent is within the subtree rooted at `root_id`.
fn scoped_get_child_dir_ids(conn: &Connection, root_id: i64) -> Result<HashMap<i64, Vec<i64>>, IndexStoreError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM entries WHERE id = ?1
            UNION ALL
            SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
        )
        SELECT e.parent_id, e.id FROM entries e
        WHERE e.parent_id IN (SELECT id FROM subtree) AND e.is_directory = 1",
    )?;
    let rows = stmt.query_map(params![root_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    for row in rows {
        let (parent_id, child_id) = row?;
        map.entry(parent_id).or_default().push(child_id);
    }
    Ok(map)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};

    /// Open a write connection to a temp DB with schema initialized.
    fn open_temp_conn() -> (Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(store.db_path()).expect("failed to open write conn");
        // Drop store so the read connection is closed; we only need the write conn for tests
        drop(store);
        (conn, dir)
    }

    /// Insert a batch of test entries using the v2 integer-keyed API.
    fn insert_entries(conn: &Connection, entries: &[EntryRow]) {
        IndexStore::insert_entries_v2_batch(conn, entries).expect("insert failed");
    }

    fn make_dir(id: i64, parent_id: i64, name: &str) -> EntryRow {
        EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }
    }

    fn make_file(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
        EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(size),
            physical_size: Some(size),
            modified_at: None,
            inode: None,
        }
    }

    /// Get dir_stats by entry ID.
    fn get_stats(conn: &Connection, entry_id: i64) -> Option<DirStatsById> {
        IndexStore::get_dir_stats_by_id(conn, entry_id).unwrap()
    }

    // ── compute_all_aggregates tests ─────────────────────────────────

    #[test]
    fn aggregate_simple_tree() {
        let (conn, _dir) = open_temp_conn();

        // Tree structure (root sentinel id=1 already exists):
        //   /root (id=2)
        //   /root/a.txt (id=3, 100 bytes)
        //   /root/b.txt (id=4, 200 bytes)
        //   /root/sub/ (id=5)
        //   /root/sub/c.txt (id=6, 50 bytes)
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "root"),
                make_file(3, 2, "a.txt", 100),
                make_file(4, 2, "b.txt", 200),
                make_dir(5, 2, "sub"),
                make_file(6, 5, "c.txt", 50),
            ],
        );

        let count = compute_all_aggregates(&conn).unwrap();
        assert_eq!(count, 3); // root sentinel + /root + /root/sub

        let sub_stats = get_stats(&conn, 5).unwrap();
        assert_eq!(sub_stats.recursive_logical_size, 50);
        assert_eq!(sub_stats.recursive_file_count, 1);
        assert_eq!(sub_stats.recursive_dir_count, 0);

        let root_dir_stats = get_stats(&conn, 2).unwrap();
        assert_eq!(root_dir_stats.recursive_logical_size, 350); // 100 + 200 + 50
        assert_eq!(root_dir_stats.recursive_file_count, 3);
        assert_eq!(root_dir_stats.recursive_dir_count, 1);

        // Root sentinel (id=1) should have stats summing all top-level entries
        let sentinel_stats = get_stats(&conn, ROOT_ID).unwrap();
        assert_eq!(sentinel_stats.recursive_logical_size, 350);
        assert_eq!(sentinel_stats.recursive_file_count, 3);
        assert_eq!(sentinel_stats.recursive_dir_count, 2); // /root + /root/sub
    }

    #[test]
    fn aggregate_deep_tree() {
        let (conn, _dir) = open_temp_conn();

        // Tree: /a/b/c/d/file.txt (1000 bytes)
        // id=2: /a, id=3: /a/b, id=4: /a/b/c, id=5: /a/b/c/d, id=6: file.txt
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "a"),
                make_dir(3, 2, "b"),
                make_dir(4, 3, "c"),
                make_dir(5, 4, "d"),
                make_file(6, 5, "file.txt", 1000),
            ],
        );

        compute_all_aggregates(&conn).unwrap();

        // Each ancestor should have the file's size propagated up
        for &dir_id in &[5, 4, 3, 2] {
            let stats = get_stats(&conn, dir_id).unwrap();
            assert_eq!(stats.recursive_logical_size, 1000, "wrong size for id={dir_id}");
            assert_eq!(stats.recursive_file_count, 1, "wrong file count for id={dir_id}");
        }

        // Dir counts should increase as we go up
        assert_eq!(get_stats(&conn, 5).unwrap().recursive_dir_count, 0); // /a/b/c/d
        assert_eq!(get_stats(&conn, 4).unwrap().recursive_dir_count, 1); // /a/b/c
        assert_eq!(get_stats(&conn, 3).unwrap().recursive_dir_count, 2); // /a/b
        assert_eq!(get_stats(&conn, 2).unwrap().recursive_dir_count, 3); // /a
    }

    #[test]
    fn aggregate_empty_db() {
        let (conn, _dir) = open_temp_conn();
        let count = compute_all_aggregates(&conn).unwrap();
        // Root sentinel exists but has no children, so it may or may not be counted.
        // With the integer-keyed schema, root sentinel is a real directory entry.
        // If no other entries exist, the root sentinel has 0 children -> count is 1 (just root).
        assert!(count <= 1);
    }

    #[test]
    fn aggregate_dir_with_no_files() {
        let (conn, _dir) = open_temp_conn();

        insert_entries(&conn, &[make_dir(2, ROOT_ID, "empty")]);

        compute_all_aggregates(&conn).unwrap();

        let stats = get_stats(&conn, 2).unwrap();
        assert_eq!(stats.recursive_logical_size, 0);
        assert_eq!(stats.recursive_file_count, 0);
        assert_eq!(stats.recursive_dir_count, 0);
    }

    // ── compute_subtree_aggregates tests ─────────────────────────────

    #[test]
    fn subtree_aggregation() {
        let (conn, _dir) = open_temp_conn();

        // Two separate subtrees under root:
        //   /a (id=2) with /a/f.txt (id=3, 100 bytes)
        //   /b (id=4) with /b/sub (id=5) with /b/sub/g.txt (id=6, 200 bytes)
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "a"),
                make_file(3, 2, "f.txt", 100),
                make_dir(4, ROOT_ID, "b"),
                make_dir(5, 4, "sub"),
                make_file(6, 5, "g.txt", 200),
            ],
        );

        // Only aggregate /b subtree
        let count = compute_subtree_aggregates(&conn, "/b").unwrap();
        assert_eq!(count, 2); // /b and /b/sub

        // /b/sub should have stats
        let sub_stats = get_stats(&conn, 5).unwrap();
        assert_eq!(sub_stats.recursive_logical_size, 200);

        // /b should have stats
        let b_stats = get_stats(&conn, 4).unwrap();
        assert_eq!(b_stats.recursive_logical_size, 200);
        assert_eq!(b_stats.recursive_file_count, 1);
        assert_eq!(b_stats.recursive_dir_count, 1);

        // /a should NOT have stats (not in subtree)
        assert!(get_stats(&conn, 2).is_none());
    }

    #[test]
    fn subtree_aggregation_nonexistent_root() {
        let (conn, _dir) = open_temp_conn();
        let count = compute_subtree_aggregates(&conn, "/nonexistent").unwrap();
        assert_eq!(count, 0);
    }

    // ── backfill_missing_dir_stats tests ─────────────────────────────

    #[test]
    fn backfill_fills_missing_stats() {
        let (conn, _dir) = open_temp_conn();

        // Tree: /a (id=2) with /a/f.txt (id=3, 100 bytes), /a/sub (id=4), /a/sub/g.txt (id=5, 200)
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "a"),
                make_file(3, 2, "f.txt", 100),
                make_dir(4, 2, "sub"),
                make_file(5, 4, "g.txt", 200),
            ],
        );

        // Only compute stats for /a/sub (id=4) — leave /a (id=2) and root (id=1) missing
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: 4,
                recursive_logical_size: 200,
                recursive_physical_size: 200,
                recursive_file_count: 1,
                recursive_dir_count: 0,
            }],
        )
        .unwrap();

        // Backfill should fill in root sentinel (id=1) and /a (id=2)
        let count = backfill_missing_dir_stats(&conn).unwrap();
        assert_eq!(count, 2); // root sentinel + /a

        // /a should now have correct recursive stats
        let a_stats = get_stats(&conn, 2).unwrap();
        assert_eq!(a_stats.recursive_logical_size, 300); // 100 + 200
        assert_eq!(a_stats.recursive_file_count, 2);
        assert_eq!(a_stats.recursive_dir_count, 1);

        // Root sentinel should also be correct
        let root_stats = get_stats(&conn, ROOT_ID).unwrap();
        assert_eq!(root_stats.recursive_logical_size, 300);
    }

    #[test]
    fn backfill_noop_when_all_stats_present() {
        let (conn, _dir) = open_temp_conn();

        insert_entries(&conn, &[make_dir(2, ROOT_ID, "a"), make_file(3, 2, "f.txt", 100)]);

        // Compute all stats first
        compute_all_aggregates(&conn).unwrap();

        // Backfill should find nothing to do
        let count = backfill_missing_dir_stats(&conn).unwrap();
        assert_eq!(count, 0);
    }

    // ── topological sort test ────────────────────────────────────────

    #[test]
    fn topological_sort_produces_bottom_up_order() {
        // Tree: 1 -> 2 -> 3 -> 4 (root -> a -> b -> c)
        let entries = vec![(1, 0), (2, 1), (3, 2), (4, 3)];
        let sorted = topological_sort_bottom_up(&entries);
        // Leaf (4) should come before its ancestors
        let pos_4 = sorted.iter().position(|&id| id == 4).unwrap();
        let pos_3 = sorted.iter().position(|&id| id == 3).unwrap();
        let pos_2 = sorted.iter().position(|&id| id == 2).unwrap();
        let pos_1 = sorted.iter().position(|&id| id == 1).unwrap();
        assert!(pos_4 < pos_3);
        assert!(pos_3 < pos_2);
        assert!(pos_2 < pos_1);
    }
}
