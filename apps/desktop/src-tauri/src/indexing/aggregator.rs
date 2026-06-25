//! Dir stats computation: bottom-up aggregation and incremental delta propagation.
//!
//! Three modes:
//! - **Full aggregation**: after a full scan, compute `dir_stats` for every directory (deepest
//!   first).
//! - **Subtree aggregation**: after a subtree scan, compute `dir_stats` only under a given root.
//! - **Delta propagation**: after a watcher event, walk up the ancestor chain updating counts.
//!
//! All queries use the integer-keyed schema v2 (`id`, `parent_id`, `entry_id`).

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::indexing::store::{DirStatsById, IndexStore, IndexStoreError, resolve_path};
use crate::pluralize::pluralize_with;

/// `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
/// has_symlinks_direct)`.
///
/// `has_symlinks_direct` is `true` if any direct child of `parent_id` is a symlink.
/// OR-aggregated with descendant directories' `recursive_has_symlinks` during the
/// bottom-up pass to compute each directory's `recursive_has_symlinks`.
type ChildrenStatsMap = HashMap<i64, (u64, u64, u64, u64, bool)>;

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
    log::debug!(
        "Aggregation: starting bottom-up computation for {}",
        pluralize_with(dir_count as u64, "directory", "directories")
    );

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

    let listed_epochs = bulk_get_listed_epochs(conn)?;

    compute_and_write(
        conn,
        &dir_entries,
        &direct_stats,
        &child_dirs_map,
        &listed_epochs,
        on_progress,
    )
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

    // `listed_epoch` is NOT in the accumulator maps (those are keyed by `parent_id`
    // and never see a dir's own epoch — the mark arrives via a separate
    // `MarkDirsListed` message). Read it from `entries` here, in the same scan that
    // loaded the dir list above.
    let listed_epochs = bulk_get_listed_epochs(conn)?;

    compute_and_write(
        conn,
        &dir_entries,
        direct_stats,
        child_dirs,
        &listed_epochs,
        on_progress,
    )
}

/// Shared core: topological sort, bottom-up computation, batch write.
///
/// Calls `on_progress` at phase transitions and every ~1% during compute/write loops.
fn compute_and_write(
    conn: &Connection,
    dir_entries: &[(i64, i64)],
    direct_stats: &ChildrenStatsMap,
    child_dirs_map: &HashMap<i64, Vec<i64>>,
    listed_epochs: &HashMap<i64, u64>,
    on_progress: &mut dyn FnMut(AggregationProgress),
) -> Result<u64, IndexStoreError> {
    let start = std::time::Instant::now();
    let dir_count = dir_entries.len() as u64;

    on_progress(AggregationProgress::new(AggregationPhase::Sorting, 0, dir_count));
    let sorted = topological_sort_bottom_up(dir_entries);

    // Report every ~1% of progress, but at least every 1000 items
    let compute_report_interval = (dir_count / 100).max(1000).min(dir_count.max(1)) as usize;

    on_progress(AggregationProgress::new(AggregationPhase::Computing, 0, dir_count));
    let computed = compute_bottom_up(&sorted, direct_stats, child_dirs_map, listed_epochs, None, |i| {
        if (i + 1) % compute_report_interval == 0 {
            on_progress(AggregationProgress::new(
                AggregationPhase::Computing,
                (i + 1) as u64,
                dir_count,
            ));
            log::debug!(
                "Aggregation: processed {}/{} ({:.1}s)",
                i + 1,
                pluralize_with(dir_count, "directory", "directories"),
                start.elapsed().as_secs_f64()
            );
        }
    });

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
        "Aggregation: complete. {} processed in {:.1}s",
        pluralize_with(count, "directory", "directories"),
        start.elapsed().as_secs_f64()
    );

    Ok(count)
}

/// 0-absorbing minimum of two epochs: `0` (an unlisted dir) ABSORBS, so any `0`
/// in the rolled-up set drags the result to `0`. For two non-zero epochs it's the
/// ordinary `min`. This is what makes a single unlisted descendant anywhere in a
/// subtree pull the whole subtree's `min_subtree_epoch` to `0` (incomplete).
fn absorbing_min_epoch(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 { 0 } else { a.min(b) }
}

/// Bottom-up aggregation over a topologically sorted list of directory IDs.
///
/// For each directory (leaves first), sums direct children stats from `direct_stats`,
/// then adds recursive stats from already-computed child directories. When
/// `existing_stats` is provided, falls back to it for children not yet in the
/// computed map (used by `backfill_missing_dir_stats` where some children already
/// have DB rows). Calls `on_iter(index)` after each directory for progress reporting.
///
/// `listed_epochs` maps each dir id to its own `entries.listed_epoch` (`0` = never
/// listed). Each dir's `min_subtree_epoch` is the 0-absorbing `min` of its own
/// `listed_epoch` and every child dir's computed `min_subtree_epoch`, so an
/// unlisted dir anywhere in the subtree drags the whole subtree to `0`
/// (incomplete). A dir absent from `listed_epochs` is treated as unlisted (`0`).
fn compute_bottom_up(
    sorted_ids: &[i64],
    direct_stats: &ChildrenStatsMap,
    child_dirs: &HashMap<i64, Vec<i64>>,
    listed_epochs: &HashMap<i64, u64>,
    existing_stats: Option<&HashMap<i64, DirStatsById>>,
    mut on_iter: impl FnMut(usize),
) -> HashMap<i64, DirStatsById> {
    let mut computed: HashMap<i64, DirStatsById> = HashMap::with_capacity(sorted_ids.len());

    for (i, &dir_id) in sorted_ids.iter().enumerate() {
        let (logical_size_sum, physical_size_sum, file_count, child_dir_count, has_symlinks_direct) =
            direct_stats.get(&dir_id).copied().unwrap_or((0, 0, 0, 0, false));

        let mut recursive_logical_size = logical_size_sum;
        let mut recursive_physical_size = physical_size_sum;
        let mut recursive_file_count = file_count;
        let mut recursive_dir_count = child_dir_count;
        let mut recursive_has_symlinks = has_symlinks_direct;
        // Coverage rollup: start from the dir's own listed_epoch, then 0-absorbing-
        // min in each child dir's subtree epoch.
        let mut min_subtree_epoch = listed_epochs.get(&dir_id).copied().unwrap_or(0);

        if let Some(children) = child_dirs.get(&dir_id) {
            for &child_id in children {
                let child_stats = computed
                    .get(&child_id)
                    .or_else(|| existing_stats.and_then(|m| m.get(&child_id)));
                if let Some(cs) = child_stats {
                    recursive_logical_size += cs.recursive_logical_size;
                    recursive_physical_size += cs.recursive_physical_size;
                    recursive_file_count += cs.recursive_file_count;
                    recursive_dir_count += cs.recursive_dir_count;
                    recursive_has_symlinks = recursive_has_symlinks || cs.recursive_has_symlinks;
                    min_subtree_epoch = absorbing_min_epoch(min_subtree_epoch, cs.min_subtree_epoch);
                } else {
                    // A child dir we know exists but have no stats for is unknown ⇒
                    // its subtree epoch is `0`, which absorbs the parent's to `0`.
                    min_subtree_epoch = 0;
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
                recursive_has_symlinks,
                min_subtree_epoch,
            },
        );

        on_iter(i);
    }

    computed
}

/// Outcome of a single partial-aggregation pass, for logging.
pub struct PartialAggStats {
    /// Number of directories the bottom-up compute covered (all known dirs).
    pub dirs_computed: u64,
    /// Number of `dir_stats` rows actually written this pass.
    pub rows_written: u64,
    /// Number of `hot_paths` that resolved to a directory entry.
    pub hot_paths_resolved: u64,
}

/// Mid-scan partial aggregation: compute partial recursive sizes from the
/// in-memory accumulator maps (as they stand) and write a bounded subset of
/// `dir_stats` rows so visible listings can show growing sizes during the scan.
///
/// Borrows `direct_stats` and `child_dirs` read-only and never mutates them.
/// The compute runs over every known directory (cheap, pure in-memory), but the
/// write set is bounded: dirs at `depth <= max_depth` from the scan root, plus
/// any `hot_paths` directory and its direct children (so a pane currently
/// showing a deep dir gets live sizes regardless of depth).
///
/// `hot_paths` resolve via `resolve_path` on the writer's connection (fine here:
/// partial passes run between committed batches, so there's no open
/// transaction). A hot path that doesn't resolve, or resolves to a
/// non-directory, is skipped silently.
pub fn compute_partial_aggregates(
    conn: &Connection,
    direct_stats: &ChildrenStatsMap,
    child_dirs: &HashMap<i64, Vec<i64>>,
    hot_paths: &[String],
    max_depth: usize,
) -> Result<PartialAggStats, IndexStoreError> {
    use std::collections::HashSet;

    use crate::indexing::store::ROOT_ID;

    // Derive the directory list from the maps. Every scanned directory appears
    // exactly once as a value in `child_dirs` (pushed when its row was inserted
    // under its parent). The scan root (ROOT_ID) has no parent in any map, so
    // add it explicitly with the `0` sentinel parent.
    let mut dir_entries: Vec<(i64, i64)> = Vec::new();
    dir_entries.push((ROOT_ID, 0));
    for (&parent_id, children) in child_dirs {
        for &child_id in children {
            dir_entries.push((child_id, parent_id));
        }
    }

    // Compute each dir's depth from the scan root via a memoized walk.
    // `depth(ROOT_ID) = 0` is the explicit base case: ROOT_ID's parent is the
    // `0` sentinel (in no map), so a naive walk would assign it `usize::MAX` and
    // the most visible row (the `/` pane's `..`) would never get a partial
    // total. Any dir whose chain can't reach ROOT_ID (shouldn't happen — jwalk
    // inserts parents before children — but cheap to guard) gets `usize::MAX`,
    // so it's never written by the depth cap (it stays a placeholder until the
    // final pass).
    let parent_of: HashMap<i64, i64> = dir_entries.iter().map(|&(id, pid)| (id, pid)).collect();
    let mut depths: HashMap<i64, usize> = HashMap::with_capacity(dir_entries.len());
    depths.insert(ROOT_ID, 0);
    for &(id, _) in &dir_entries {
        depth_of(id, &parent_of, &mut depths);
    }

    // Read each known dir's `listed_epoch` in ONE batched `WHERE id IN (...)` over
    // exactly the ids the borrowed maps describe — never a full-table scan and
    // never per-dir N+1 (this runs every ~5 s mid-scan). Mid-scan the marks land
    // only at scan end, so these all read `0` and the partial sizes are honest
    // lower bounds (`min_subtree_epoch = 0`); the final aggregate stamps them
    // exact. No SQL fallback: the dir list still comes from the borrowed maps.
    let dir_ids: Vec<i64> = dir_entries.iter().map(|&(id, _)| id).collect();
    let listed_epochs = get_listed_epochs_for_ids(conn, &dir_ids)?;

    // Topological sort + bottom-up compute over ALL dirs (borrowed maps; the
    // cheap part — pure in-memory iteration). Every dir gets a correct subtree
    // total regardless of its depth-to-root, so hot-path writes are always safe.
    let sorted = topological_sort_bottom_up(&dir_entries);
    let dirs_computed = sorted.len() as u64;
    let computed = compute_bottom_up(&sorted, direct_stats, child_dirs, &listed_epochs, None, |_| {});

    // Write set: dirs at depth ≤ max_depth, plus each hot-path dir and its direct
    // children (independent of the depth cap).
    let mut write_set: HashSet<i64> = depths
        .iter()
        .filter(|&(_, &depth)| depth <= max_depth)
        .map(|(&id, _)| id)
        .collect();

    let mut hot_paths_resolved: u64 = 0;
    for path in hot_paths {
        let Some(dir_id) = resolve_path(conn, path)? else {
            continue; // Not scanned yet (or excluded): skip silently, retry next pass.
        };
        // `computed` holds exactly the known directories (derived from the maps),
        // so a hot path resolving to a non-directory (a symlink/file leaf) is
        // absent and skipped. An empty directory is still present — it's a value
        // in `child_dirs`, hence in the compute — so it punches through fine.
        if !computed.contains_key(&dir_id) {
            continue;
        }
        hot_paths_resolved += 1;
        write_set.insert(dir_id);
        if let Some(children) = child_dirs.get(&dir_id) {
            write_set.extend(children.iter().copied());
        }
    }

    // Collect the rows to write (value columns only; partial passes overwrite
    // whatever the final pass will fully recompute).
    let rows: Vec<DirStatsById> = write_set.iter().filter_map(|id| computed.get(id).cloned()).collect();
    let rows_written = rows.len() as u64;

    for chunk in rows.chunks(1000) {
        IndexStore::upsert_dir_stats_by_id(conn, chunk)?;
    }

    Ok(PartialAggStats {
        dirs_computed,
        rows_written,
        hot_paths_resolved,
    })
}

/// Memoized depth from the scan root for the partial-aggregation depth cap.
///
/// `depth(ROOT_ID) = 0` is seeded by the caller. A dir whose parent chain can't
/// reach ROOT_ID gets `usize::MAX` (never written by the depth cap). Iterative
/// to avoid deep recursion on long chains.
fn depth_of(start: i64, parent_of: &HashMap<i64, i64>, depths: &mut HashMap<i64, usize>) -> usize {
    if let Some(&d) = depths.get(&start) {
        return d;
    }
    // Walk up, collecting the unresolved chain, until we hit a memoized node or
    // a dead end (a parent not in the map).
    let mut chain: Vec<i64> = Vec::new();
    let mut current = start;
    let base = loop {
        if let Some(&d) = depths.get(&current) {
            break d;
        }
        chain.push(current);
        match parent_of.get(&current) {
            Some(&pid) => current = pid,
            None => {
                // Chain can't reach ROOT_ID: every node on it is detached.
                for id in &chain {
                    depths.insert(*id, usize::MAX);
                }
                return usize::MAX;
            }
        }
    };
    // `base` is a finite depth at `current`; assign increasing depths back down
    // the chain. If `base` is `usize::MAX` the whole chain is detached too.
    if base == usize::MAX {
        for id in &chain {
            depths.insert(*id, usize::MAX);
        }
        return usize::MAX;
    }
    let mut d = base;
    for &id in chain.iter().rev() {
        d += 1;
        depths.insert(id, d);
    }
    *depths.get(&start).unwrap_or(&usize::MAX)
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
    log::debug!(
        "Subtree aggregation: starting bottom-up computation for {} under {root}",
        pluralize_with(dir_count as u64, "directory", "directories")
    );

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

    let listed_epochs = scoped_get_listed_epochs(conn, root_id)?;

    // Topological sort: leaves first
    let sorted = topological_sort_bottom_up(&dir_entries);
    let computed = compute_bottom_up(&sorted, &direct_stats, &child_dirs_map, &listed_epochs, None, |_| {});

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
        "Subtree aggregation: complete. {} processed in {:.1}ms",
        pluralize_with(count, "directory", "directories"),
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
    log::debug!(
        "Backfill: {} missing dir_stats, computing...",
        pluralize_with(count as u64, "directory", "directories")
    );

    // Load ALL directory entries for the topological sort (we need the full
    // tree structure to compute bottom-up correctly, since a missing dir's
    // children may also be missing).
    let all_dir_entries = load_all_directory_ids(conn)?;

    // Build direct_stats and child_dirs maps scoped to the missing dirs
    // and their descendants. We use the full-table bulk queries since the
    // missing dirs can be scattered across the tree.
    let direct_stats = bulk_get_children_stats_by_id(conn)?;
    let child_dirs_map = bulk_get_child_dir_ids(conn)?;

    // Bulk-load existing dir_stats so the bottom-up pass can use them as
    // fallback for children that already have stats (avoids N+1 queries).
    let existing_stats = bulk_get_all_dir_stats(conn)?;

    let listed_epochs = bulk_get_listed_epochs(conn)?;

    // Topological sort all dirs (we need correct ordering)
    let sorted = topological_sort_bottom_up(&all_dir_entries);

    // Build set of missing IDs for fast lookup
    let missing_set: std::collections::HashSet<i64> = missing_ids.into_iter().collect();

    // Compute stats bottom-up for ALL dirs, but only write the missing ones.
    // We need to compute all because a missing dir's stats depend on its
    // children (which might have existing stats in the DB or might also be
    // missing).
    let computed = compute_bottom_up(
        &sorted,
        &direct_stats,
        &child_dirs_map,
        &listed_epochs,
        Some(&existing_stats),
        |_| {},
    );
    let to_write: Vec<DirStatsById> = computed
        .into_values()
        .filter(|s| missing_set.contains(&s.entry_id))
        .collect();

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

/// Load `dir_id -> listed_epoch` for ALL directories in a single SQL query.
///
/// Feeds the `min_subtree_epoch` rollup in `compute_bottom_up`. Read in the same
/// pass that loads the dir list (no extra full scan beyond this one query). A dir
/// absent from the map (impossible here — every dir row has a `NOT NULL DEFAULT 0`
/// `listed_epoch`) would be treated as unlisted.
fn bulk_get_listed_epochs(conn: &Connection) -> Result<HashMap<i64, u64>, IndexStoreError> {
    let mut stmt = conn.prepare("SELECT id, listed_epoch FROM entries WHERE is_directory = 1")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?)))?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, epoch) = row?;
        map.insert(id, epoch);
    }
    Ok(map)
}

/// Load `dir_id -> listed_epoch` for directories within the subtree rooted at
/// `root_id` (mirrors the scoped CTE child queries).
fn scoped_get_listed_epochs(conn: &Connection, root_id: i64) -> Result<HashMap<i64, u64>, IndexStoreError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM entries WHERE id = ?1
            UNION ALL
            SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
        )
        SELECT e.id, e.listed_epoch FROM entries e
        WHERE e.id IN (SELECT id FROM subtree) AND e.is_directory = 1",
    )?;
    let rows = stmt.query_map(params![root_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (id, epoch) = row?;
        map.insert(id, epoch);
    }
    Ok(map)
}

/// Load `dir_id -> listed_epoch` for a specific set of ids via batched
/// `WHERE id IN (...)` queries (chunked to stay under SQLite's parameter ceiling).
///
/// Used by the mid-scan partial path: it already knows the dir ids from the
/// borrowed accumulator maps, so this is a targeted batched read, never a
/// full-table scan and never per-dir N+1.
fn get_listed_epochs_for_ids(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, u64>, IndexStoreError> {
    // Stay well under SQLite's default 999-parameter ceiling.
    const CHUNK: usize = 900;
    let mut map = HashMap::with_capacity(ids.len());
    for chunk in ids.chunks(CHUNK) {
        let placeholders: String = (0..chunk.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT id, listed_epoch FROM entries WHERE id IN ({placeholders})");
        let mut stmt = conn.prepare_cached(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            chunk.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt.query_map(&*params, |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u64>(1)?)))?;
        for row in rows {
            let (id, epoch) = row?;
            map.insert(id, epoch);
        }
    }
    Ok(map)
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
/// Returns a map: `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
/// has_symlinks_direct)`.
fn bulk_get_children_stats_by_id(conn: &Connection) -> Result<ChildrenStatsMap, IndexStoreError> {
    let mut stmt = conn.prepare(
        "SELECT parent_id,
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN logical_size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN physical_size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_directory = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(MAX(is_symlink), 0)
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
            row.get::<_, i32>(5)? != 0,
        ))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (parent_id, logical_size, physical_size, files, dirs, has_symlinks) = row?;
        map.insert(parent_id, (logical_size, physical_size, files, dirs, has_symlinks));
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

/// Bulk-load all existing `dir_stats` rows into a map keyed by `entry_id`.
///
/// Used by `backfill_missing_dir_stats` so the bottom-up pass can fall back to
/// existing stats for children that already have rows (avoiding N+1 queries).
fn bulk_get_all_dir_stats(conn: &Connection) -> Result<HashMap<i64, DirStatsById>, IndexStoreError> {
    let mut stmt = conn.prepare(
        "SELECT entry_id, recursive_logical_size, recursive_physical_size,
                recursive_file_count, recursive_dir_count, recursive_has_symlinks, min_subtree_epoch
         FROM dir_stats",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DirStatsById {
            entry_id: row.get(0)?,
            recursive_logical_size: row.get(1)?,
            recursive_physical_size: row.get(2)?,
            recursive_file_count: row.get(3)?,
            recursive_dir_count: row.get(4)?,
            recursive_has_symlinks: row.get::<_, i32>(5)? != 0,
            min_subtree_epoch: row.get(6)?,
        })
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let stats = row?;
        map.insert(stats.entry_id, stats);
    }
    Ok(map)
}

/// Load direct children stats scoped to a subtree via recursive CTE.
///
/// Returns a map: `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
/// has_symlinks_direct)`. Only includes entries whose parent is within the subtree rooted at
/// `root_id`.
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
               COALESCE(SUM(CASE WHEN e.is_directory = 1 THEN 1 ELSE 0 END), 0),
               COALESCE(MAX(e.is_symlink), 0)
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
            row.get::<_, i32>(5)? != 0,
        ))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (parent_id, logical_size, physical_size, files, dirs, has_symlinks) = row?;
        map.insert(parent_id, (logical_size, physical_size, files, dirs, has_symlinks));
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

    fn make_symlink(id: i64, parent_id: i64, name: &str) -> EntryRow {
        EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
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

    // ── min_subtree_epoch rollup tests ───────────────────────────────

    /// The core honest-coverage rollup: a listed parent with a listed-EMPTY child
    /// and an UNlisted child. After aggregation:
    /// - the unlisted child rolls to `min_subtree_epoch == 0` (unknown),
    /// - the listed-empty child rolls to its own epoch `> 0` with size 0 (genuinely
    ///   empty, not unknown),
    /// - the parent absorbs the unlisted child to `min_subtree_epoch == 0`
    ///   (incomplete — its subtree has an unknown corner).
    #[test]
    fn aggregate_min_subtree_epoch_absorbs_unlisted() {
        let (conn, _dir) = open_temp_conn();

        // /parent (id=2): listed at epoch 5
        //   /parent/empty (id=3): listed at epoch 5, no children → genuinely empty
        //   /parent/unlisted (id=4): never listed (listed_epoch stays 0)
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "parent"),
                make_dir(3, 2, "empty"),
                make_dir(4, 2, "unlisted"),
            ],
        );
        // Stamp the parent and the empty child as listed at epoch 5; leave the
        // unlisted child (and root sentinel) at 0.
        IndexStore::mark_dirs_listed(&conn, &[2, 3], 5).unwrap();

        compute_all_aggregates(&conn).unwrap();

        let empty = get_stats(&conn, 3).unwrap();
        assert_eq!(
            empty.min_subtree_epoch, 5,
            "a listed-empty dir keeps its own epoch (>0)"
        );
        assert_eq!(empty.recursive_logical_size, 0, "and reports genuine 0 bytes");

        let unlisted = get_stats(&conn, 4).unwrap();
        assert_eq!(unlisted.min_subtree_epoch, 0, "an unlisted dir is unknown (0)");

        let parent = get_stats(&conn, 2).unwrap();
        assert_eq!(
            parent.min_subtree_epoch, 0,
            "a parent with any unlisted descendant is incomplete (0)"
        );
    }

    /// A fully-listed subtree (every dir marked at the same epoch) rolls every
    /// ancestor's `min_subtree_epoch` up to that epoch (`> 0`, exact).
    #[test]
    fn aggregate_min_subtree_epoch_all_listed_is_exact() {
        let (conn, _dir) = open_temp_conn();

        // /a/b/c with a file under c; all dirs listed at epoch 3.
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "a"),
                make_dir(3, 2, "b"),
                make_dir(4, 3, "c"),
                make_file(5, 4, "f.txt", 100),
            ],
        );
        IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 2, 3, 4], 3).unwrap();

        compute_all_aggregates(&conn).unwrap();

        for &dir_id in &[ROOT_ID, 2, 3, 4] {
            assert_eq!(
                get_stats(&conn, dir_id).unwrap().min_subtree_epoch,
                3,
                "fully-listed dir id={dir_id} should be exact at epoch 3"
            );
        }
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

    /// A subtree aggregate sets `min_subtree_epoch` from the scoped `listed_epoch`
    /// read (not left at the `0` default): a fully-listed subtree is exact, and an
    /// unlisted dir inside it drags its ancestors within the subtree to `0`.
    #[test]
    fn subtree_aggregation_sets_min_subtree_epoch() {
        let (conn, _dir) = open_temp_conn();

        // /b (id=2): listed at epoch 4
        //   /b/listed (id=3): listed at epoch 4 → exact
        //   /b/unlisted (id=4): never listed → unknown, drags /b to 0
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "b"),
                make_dir(3, 2, "listed"),
                make_dir(4, 2, "unlisted"),
            ],
        );
        IndexStore::mark_dirs_listed(&conn, &[2, 3], 4).unwrap();

        let count = compute_subtree_aggregates(&conn, "/b").unwrap();
        assert_eq!(count, 3); // /b, /b/listed, /b/unlisted

        assert_eq!(
            get_stats(&conn, 3).unwrap().min_subtree_epoch,
            4,
            "listed leaf is exact"
        );
        assert_eq!(
            get_stats(&conn, 4).unwrap().min_subtree_epoch,
            0,
            "unlisted leaf is unknown"
        );
        assert_eq!(
            get_stats(&conn, 2).unwrap().min_subtree_epoch,
            0,
            "subtree root absorbs the unlisted child"
        );
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

        // Only compute stats for /a/sub (id=4): leave /a (id=2) and root (id=1) missing
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: 4,
                recursive_logical_size: 200,
                recursive_physical_size: 200,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
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

    /// Backfill sets `min_subtree_epoch` on the dirs it fills (not left at the `0`
    /// default): a fully-listed subtree backfills to its epoch, exact.
    #[test]
    fn backfill_sets_min_subtree_epoch() {
        let (conn, _dir) = open_temp_conn();

        // /a (id=2) with /a/f.txt (id=3) and /a/sub (id=4) with /a/sub/g.txt (id=5).
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "a"),
                make_file(3, 2, "f.txt", 100),
                make_dir(4, 2, "sub"),
                make_file(5, 4, "g.txt", 200),
            ],
        );
        IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 2, 4], 6).unwrap();

        // Seed only /a/sub's stats (with its honest epoch); leave root + /a missing.
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: 4,
                recursive_logical_size: 200,
                recursive_physical_size: 200,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 6,
            }],
        )
        .unwrap();

        let count = backfill_missing_dir_stats(&conn).unwrap();
        assert_eq!(count, 2); // root sentinel + /a

        assert_eq!(
            get_stats(&conn, 2).unwrap().min_subtree_epoch,
            6,
            "/a backfills to its fully-listed epoch (exact)"
        );
        assert_eq!(get_stats(&conn, ROOT_ID).unwrap().min_subtree_epoch, 6);
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

    // ── recursive_has_symlinks tests ─────────────────────────────────

    #[test]
    fn aggregate_propagates_recursive_has_symlinks() {
        let (conn, _dir) = open_temp_conn();

        // Tree:
        //   /grand (id=2)
        //   /grand/parent (id=3)
        //   /grand/parent/leaf (id=4)
        //   /grand/parent/leaf/link (id=5, symlink)
        //   /grand/sibling (id=6), no symlinks
        //   /grand/sibling/file.txt (id=7, 100 bytes)
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "grand"),
                make_dir(3, 2, "parent"),
                make_dir(4, 3, "leaf"),
                make_symlink(5, 4, "link"),
                make_dir(6, 2, "sibling"),
                make_file(7, 6, "file.txt", 100),
            ],
        );

        compute_all_aggregates(&conn).unwrap();

        // The symlink leaf has the flag (direct child symlink)
        assert!(
            get_stats(&conn, 4).unwrap().recursive_has_symlinks,
            "leaf has direct symlink"
        );
        // Parent gets it via subdir aggregation
        assert!(
            get_stats(&conn, 3).unwrap().recursive_has_symlinks,
            "parent should propagate up"
        );
        // Grand gets it from /grand/parent
        assert!(
            get_stats(&conn, 2).unwrap().recursive_has_symlinks,
            "grand should propagate up"
        );
        // Sibling has no symlinks anywhere in its subtree
        assert!(
            !get_stats(&conn, 6).unwrap().recursive_has_symlinks,
            "sibling without symlinks should be false"
        );
        // Root sentinel inherits from /grand
        assert!(get_stats(&conn, ROOT_ID).unwrap().recursive_has_symlinks);
    }

    #[test]
    fn aggregate_no_symlinks_anywhere() {
        let (conn, _dir) = open_temp_conn();
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "a"),
                make_file(3, 2, "f.txt", 100),
                make_dir(4, 2, "b"),
                make_file(5, 4, "g.txt", 200),
            ],
        );
        compute_all_aggregates(&conn).unwrap();
        assert!(!get_stats(&conn, 2).unwrap().recursive_has_symlinks);
        assert!(!get_stats(&conn, 4).unwrap().recursive_has_symlinks);
        assert!(!get_stats(&conn, ROOT_ID).unwrap().recursive_has_symlinks);
    }

    #[test]
    fn aggregate_dir_with_only_symlinks_has_zero_size() {
        let (conn, _dir) = open_temp_conn();
        // /links contains only two symlinks: total size 0, but flag is true
        insert_entries(
            &conn,
            &[
                make_dir(2, ROOT_ID, "links"),
                make_symlink(3, 2, "a"),
                make_symlink(4, 2, "b"),
            ],
        );
        compute_all_aggregates(&conn).unwrap();
        let stats = get_stats(&conn, 2).unwrap();
        assert_eq!(stats.recursive_logical_size, 0, "symlink-only folder reports 0 bytes");
        assert_eq!(stats.recursive_file_count, 2, "symlinks count as files");
        assert!(stats.recursive_has_symlinks, "flag must be set");
    }

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

    // ── Property-based tests ─────────────────────────────────────────
    //
    // The function takes a slice of `(id, parent_id)` pairs and returns a
    // bottom-up ordering. The properties we pin here are the ones the callers
    // (`compute_all_aggregates`, the incremental aggregator paths) rely on:
    // each id appears at most once, descendants come before ancestors, and
    // pathological inputs (cycles, duplicates, large random forests) don't
    // panic or hang.

    mod proptests {
        use super::*;
        use proptest::prelude::*;
        use std::collections::HashSet;

        /// Generate an acyclic forest of `n` nodes where every node's parent
        /// is either `0` (forest root, treated as "out of set") or one of
        /// the already-emitted nodes. Returns `Vec<(id, parent_id)>` with
        /// ids in `1..=n`.
        fn forest_strategy(max_nodes: usize) -> impl Strategy<Value = Vec<(i64, i64)>> {
            (1usize..=max_nodes).prop_flat_map(|n| {
                // For each node i (1-indexed), pick a parent index in 0..i.
                // Index 0 maps to parent_id 0 (sentinel for "no parent in set").
                let parent_picks: Vec<_> = (0..n).map(|i| 0usize..=i).collect();
                parent_picks.prop_map(move |picks| {
                    picks
                        .into_iter()
                        .enumerate()
                        .map(|(i, pick)| {
                            let id = (i as i64) + 1;
                            let parent_id = pick as i64; // 0 means "no parent in set"
                            (id, parent_id)
                        })
                        .collect::<Vec<_>>()
                })
            })
        }

        proptest! {
            /// For any acyclic forest, the sort emits each node exactly once
            /// and places every descendant before its ancestor.
            #[test]
            fn forest_descendant_before_ancestor(entries in forest_strategy(40)) {
                let sorted = topological_sort_bottom_up(&entries);

                // Every id appears exactly once.
                let unique_ids: HashSet<i64> = entries.iter().map(|&(id, _)| id).collect();
                prop_assert_eq!(sorted.len(), unique_ids.len(), "output length must match unique input ids");
                let sorted_set: HashSet<i64> = sorted.iter().copied().collect();
                prop_assert_eq!(&sorted_set, &unique_ids, "output must be a permutation of the input ids");

                // Build position map and parent map.
                let pos: HashMap<i64, usize> =
                    sorted.iter().enumerate().map(|(i, &id)| (id, i)).collect();
                let parent_of: HashMap<i64, i64> =
                    entries.iter().copied().collect();

                // For every (child, parent_in_set) pair, child must come first.
                for &(id, pid) in &entries {
                    if pid != 0 && unique_ids.contains(&pid) {
                        let cp = pos[&id];
                        let pp = pos[&pid];
                        prop_assert!(
                            cp < pp,
                            "descendant {} at pos {} must come before ancestor {} at pos {}",
                            id, cp, pid, pp
                        );
                    }
                    // Transitively the same must hold for any ancestor,
                    // chain through `parent_of` to be sure.
                    let mut cursor = pid;
                    let mut hops = 0;
                    while cursor != 0 && unique_ids.contains(&cursor) && hops < entries.len() + 1 {
                        prop_assert!(
                            pos[&id] < pos[&cursor],
                            "descendant {} must come before transitive ancestor {}",
                            id, cursor
                        );
                        cursor = *parent_of.get(&cursor).unwrap_or(&0);
                        hops += 1;
                    }
                }
            }

            /// Robustness: the function must not panic and must produce a
            /// subset of unique input ids, even on arbitrary (possibly
            /// cyclic, duplicate, or detached) (id, parent_id) lists.
            #[test]
            fn arbitrary_input_is_panic_free_and_subset(
                entries in proptest::collection::vec((-50i64..50i64, -50i64..50i64), 0..30)
            ) {
                let sorted = topological_sort_bottom_up(&entries);
                let unique_ids: HashSet<i64> = entries.iter().map(|&(id, _)| id).collect();

                // No duplicates in output.
                let sorted_set: HashSet<i64> = sorted.iter().copied().collect();
                prop_assert_eq!(sorted.len(), sorted_set.len(), "output must have no duplicate ids");

                // Output is a subset of unique input ids.
                for id in &sorted_set {
                    prop_assert!(unique_ids.contains(id), "output id {} must come from input", id);
                }
            }
        }
    }
}
