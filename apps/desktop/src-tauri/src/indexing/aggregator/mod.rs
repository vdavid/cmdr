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

use rusqlite::Connection;

use crate::indexing::store::{DirStatsById, IndexStore, IndexStoreError, ROOT_ID, resolve_path, resolve_path_under};
use crate::pluralize::pluralize_with;

mod readers;
use readers::*;
#[cfg(test)]
mod tests;

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
    // total. Any dir whose chain can't reach ROOT_ID (shouldn't happen — the guarded
    // walker inserts parents before children — but cheap to guard) gets `usize::MAX`,
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

/// Bottom-up compute over the subtree rooted at `root_id`, returning the computed
/// `dir_stats` per directory WITHOUT writing anything.
///
/// The reusable scoped primitive shared by `compute_subtree_aggregates` (which
/// writes the whole result) and `compute_partial_aggregates_sql` (which writes
/// only a bounded slice of it). Uses the scoped recursive-CTE readers, so it's
/// O(subtree_size) regardless of total DB size. An empty subtree yields an empty
/// map. `min_subtree_epoch` falls out of `compute_bottom_up` naturally: the
/// scoped `listed_epochs` are read for exactly the dirs in the subtree, and the
/// 0-absorbing rollup makes a never-listed dir drag its ancestors to `0`.
fn compute_subtree_map(conn: &Connection, root_id: i64) -> Result<HashMap<i64, DirStatsById>, IndexStoreError> {
    let dir_entries = load_subtree_directory_ids(conn, root_id)?;
    if dir_entries.is_empty() {
        return Ok(HashMap::new());
    }

    let direct_stats = scoped_get_children_stats_by_id(conn, root_id)?;
    let child_dirs_map = scoped_get_child_dir_ids(conn, root_id)?;
    let listed_epochs = scoped_get_listed_epochs(conn, root_id)?;

    let sorted = topological_sort_bottom_up(&dir_entries);
    Ok(compute_bottom_up(
        &sorted,
        &direct_stats,
        &child_dirs_map,
        &listed_epochs,
        None,
        |_| {},
    ))
}

/// Compute `dir_stats` for directories under the subtree rooted at `root_id`
/// only (bottom-up).
///
/// Called after a subtree scan completes. Keyed by entry id (not path) so a
/// rename between the scan and this recompute can't miss the subtree. Computes
/// stats bottom-up over the scoped subtree, then writes EVERY computed row
/// (including the subtree root's own fresh totals). Returns the number of
/// directories processed (`0` if the id no longer resolves to a directory).
pub fn compute_subtree_aggregates(conn: &Connection, root_id: i64) -> Result<u64, IndexStoreError> {
    let start = std::time::Instant::now();
    let computed = compute_subtree_map(conn, root_id)?;
    if computed.is_empty() {
        return Ok(0);
    }

    let all_stats: Vec<DirStatsById> = computed.into_values().collect();
    let count = all_stats.len() as u64;
    log::debug!("Subtree aggregation: writing {count} dir_stats rows under id={root_id}...");

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

/// SQL-sourced mid-scan partial aggregation: recompute partial recursive sizes
/// for the directories a pane is currently showing (`hot_paths`) straight from
/// the committed `entries` / `dir_stats` rows, and write a bounded slice of
/// `dir_stats` so those listings show growing sizes during the scan.
///
/// This is the unified-partials counterpart to [`compute_partial_aggregates`]
/// (which reads the writer's in-memory accumulator maps). The maps are only
/// populated by `InsertEntriesV2` (fresh guarded-walker scans), so the maps path is silent
/// on the reconcile / network paths (`UpsertEntryV2`, maps empty). This path
/// reads committed SQL instead, so it works for ALL write paths.
///
/// `hot_paths` are index-relative (volume-root-stripped) paths: for a `root`
/// (local-disk) index that's just the absolute path, for a network/MTP index
/// they're the mount-relative form produced by
/// [`crate::indexing::routing::index_read_path`]. Each resolves to a dir id via
/// [`resolve_path_under`] from `ROOT_ID`.
///
/// For each resolved hot dir it runs a SCOPED bottom-up aggregate over the hot
/// dir's subtree (reusing [`compute_subtree_map`]) and writes ONLY the hot dir
/// plus its DIRECT CHILDREN (the rows actually on screen) — never the whole
/// scoped subtree. This mirrors the maps path's hot-path write set.
///
/// **Conservative cap (the stability guard):** before scoping a hot dir, it
/// cheaply reads that dir's CURRENT `dir_stats` recursive counts; if the subtree
/// already exceeds `cap`, the dir is SKIPPED (the final `ComputeAllAggregates`
/// fills it). This keeps a near-volume-root hot path from triggering a
/// near-whole-tree recursive CTE that would stall the single writer thread. A hot
/// dir with no `dir_stats` row yet (freshly created, hence tiny) is not skipped.
///
/// **Late-race safe / idempotent:** every value is recomputed from committed
/// rows, and `upsert_dir_stats_by_id` REPLACES rows, so a pass that lands after
/// the final `ComputeAllAggregates` recomputes the SAME exact values from the
/// SAME committed rows (no double count, no zeroed subset). Unlike the maps path
/// there's no empty-maps no-op to lean on, but none is needed: a late SQL pass is
/// a self-consistent recompute, not a depth-capped subset of stale maps.
///
/// `min_subtree_epoch` is left to fall out of `compute_bottom_up` +
/// `absorbing_min_epoch` (a never-listed subtree reads as "≥ X"); it is not
/// special-cased.
pub fn compute_partial_aggregates_sql(
    conn: &Connection,
    hot_paths: &[String],
    cap: u64,
) -> Result<PartialAggStats, IndexStoreError> {
    use std::collections::HashSet;

    // Collapse a pane's parent+child to the DEEPEST hot path: if `/a` and
    // `/a/b/c` are both visible, scoping `/a` would compute the whole `/a`
    // subtree (expensive, and likely cap-skipped) while only `/a/b/c`'s own
    // scope serves the deep pane. Dropping the ancestor minimizes the scoped CTE
    // cost — the stability lever. Pure string work (component-aware prefix), done
    // before any DB hit.
    let deepest: Vec<&String> = hot_paths
        .iter()
        .filter(|candidate| !hot_paths.iter().any(|other| is_proper_path_ancestor(candidate, other)))
        .collect();

    // Resolve survivors to dir ids and dedup (two paths can resolve to the same
    // id via a symlink/firmlink alias).
    let mut hot_dir_ids: Vec<i64> = Vec::new();
    let mut seen_ids: HashSet<i64> = HashSet::new();
    let mut hot_paths_resolved: u64 = 0;
    for path in deepest {
        let Some(dir_id) = resolve_path_under(conn, ROOT_ID, path)? else {
            continue; // Not scanned yet (or excluded): skip silently, retry next pass.
        };
        hot_paths_resolved += 1;
        if seen_ids.insert(dir_id) {
            hot_dir_ids.push(dir_id);
        }
    }

    let mut dirs_computed: u64 = 0;
    let mut write_set: HashSet<i64> = HashSet::new();
    let mut rows: Vec<DirStatsById> = Vec::new();

    for hot_dir_id in hot_dir_ids {
        // Cheap conservative cap: skip a hot dir whose committed subtree already
        // exceeds `cap` (final pass will fill it). A dir without a `dir_stats` row
        // is freshly created and tiny — proceed.
        if let Some(existing) = IndexStore::get_dir_stats_by_id(conn, hot_dir_id)? {
            let subtree_size = existing
                .recursive_file_count
                .saturating_add(existing.recursive_dir_count);
            if subtree_size > cap {
                log::debug!(
                    "compute_partial_aggregates_sql: skipping hot dir id={hot_dir_id} \
                     (subtree ~{subtree_size} > cap {cap}); final aggregate will fill it"
                );
                continue;
            }
        }

        let computed = compute_subtree_map(conn, hot_dir_id)?;
        dirs_computed += computed.len() as u64;

        // Write set for this hot dir: the dir itself + its DIRECT CHILDREN only
        // (the rows actually on screen). Direct children come from a cheap direct
        // query; their recursive sizes are already correct in `computed`.
        let mut to_write: HashSet<i64> = HashSet::new();
        to_write.insert(hot_dir_id);
        for (child_id, _name) in IndexStore::list_child_dir_ids_and_names(conn, hot_dir_id)? {
            to_write.insert(child_id);
        }
        for id in to_write {
            if write_set.insert(id)
                && let Some(stats) = computed.get(&id)
            {
                rows.push(stats.clone());
            }
        }
    }

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

/// Component-aware proper-ancestor test for two index-relative paths.
///
/// `true` iff `ancestor` is a strict path-prefix of `descendant` at a component
/// boundary (`/a` is a proper ancestor of `/a/b`, but not of `/ab` or of itself).
/// `/` (or `""`) is a proper ancestor of every non-root path. Pure string work.
fn is_proper_path_ancestor(ancestor: &str, descendant: &str) -> bool {
    if ancestor == descendant {
        return false;
    }
    let a = ancestor.strip_suffix('/').unwrap_or(ancestor);
    match descendant.strip_prefix(a) {
        Some(rest) => rest.starts_with('/'),
        None => false,
    }
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
