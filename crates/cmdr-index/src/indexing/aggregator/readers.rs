//! Bulk and scoped SQL readers for aggregation: the index queries that load the
//! directory tree, per-dir children stats, child-dir relationships, and listed
//! epochs that the bottom-up compute pass in `super` consumes. Pure code movement
//! from the former monolithic `aggregator.rs`.

use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::ChildrenStatsMap;
use crate::indexing::store::{DirStatsById, IndexStoreError};

/// Load directory IDs that have entries but no `dir_stats` row.
pub(super) fn load_dirs_missing_stats(conn: &Connection) -> Result<Vec<i64>, IndexStoreError> {
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
pub(super) fn load_all_directory_ids(conn: &Connection) -> Result<Vec<(i64, i64)>, IndexStoreError> {
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
pub(super) fn bulk_get_listed_epochs(conn: &Connection) -> Result<HashMap<i64, u64>, IndexStoreError> {
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
pub(super) fn scoped_get_listed_epochs(conn: &Connection, root_id: i64) -> Result<HashMap<i64, u64>, IndexStoreError> {
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
pub(super) fn get_listed_epochs_for_ids(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, u64>, IndexStoreError> {
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
pub(super) fn load_subtree_directory_ids(conn: &Connection, root_id: i64) -> Result<Vec<(i64, i64)>, IndexStoreError> {
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

/// Bulk-load direct children stats for ALL parent IDs in a single SQL query.
///
/// Returns a map: `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
/// has_symlinks_direct)`.
pub(super) fn bulk_get_children_stats_by_id(conn: &Connection) -> Result<ChildrenStatsMap, IndexStoreError> {
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
pub(super) fn bulk_get_child_dir_ids(conn: &Connection) -> Result<HashMap<i64, Vec<i64>>, IndexStoreError> {
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
pub(super) fn bulk_get_all_dir_stats(conn: &Connection) -> Result<HashMap<i64, DirStatsById>, IndexStoreError> {
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
pub(super) fn scoped_get_children_stats_by_id(
    conn: &Connection,
    root_id: i64,
) -> Result<ChildrenStatsMap, IndexStoreError> {
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
pub(super) fn scoped_get_child_dir_ids(
    conn: &Connection,
    root_id: i64,
) -> Result<HashMap<i64, Vec<i64>>, IndexStoreError> {
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
