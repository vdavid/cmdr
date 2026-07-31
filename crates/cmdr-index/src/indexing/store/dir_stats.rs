//! `IndexStore` recursive directory-stats reads and writes (`dir_stats` table)
//! plus subtree-epoch recompute. Pure code movement from the former monolithic
//! `store.rs`.

use super::*;

impl IndexStore {
    /// Look up dir_stats for a single entry by ID.
    pub fn get_dir_stats_by_id(conn: &Connection, entry_id: i64) -> Result<Option<DirStatsById>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, recursive_dir_count, recursive_has_symlinks, min_subtree_epoch
             FROM dir_stats WHERE entry_id = ?1",
        )?;
        let result = stmt
            .query_row(params![entry_id], |row| {
                Ok(DirStatsById {
                    entry_id: row.get(0)?,
                    recursive_logical_size: row.get(1)?,
                    recursive_physical_size: row.get(2)?,
                    recursive_file_count: row.get(3)?,
                    recursive_dir_count: row.get(4)?,
                    recursive_has_symlinks: row.get::<_, i32>(5)? != 0,
                    min_subtree_epoch: row.get(6)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Batch lookup of dir_stats for multiple entry IDs.
    ///
    /// Returns a `Vec` with the same length as `entry_ids`, where each element
    /// is `Some(DirStatsById)` if found or `None` otherwise.
    pub fn get_dir_stats_batch_by_ids(
        conn: &Connection,
        entry_ids: &[i64],
    ) -> Result<Vec<Option<DirStatsById>>, IndexStoreError> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }

        if entry_ids.len() <= 20 {
            return entry_ids
                .iter()
                .map(|id| Self::get_dir_stats_by_id(conn, *id))
                .collect();
        }

        let placeholders: String = (0..entry_ids.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, recursive_dir_count, recursive_has_symlinks, min_subtree_epoch
             FROM dir_stats WHERE entry_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let param_values: Vec<&dyn rusqlite::types::ToSql> =
            entry_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();

        let rows = stmt.query_map(&*param_values, |row| {
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

        let mut map = std::collections::HashMap::new();
        for row in rows {
            let stats = row?;
            map.insert(stats.entry_id, stats);
        }

        Ok(entry_ids.iter().map(|id| map.remove(id)).collect())
    }

    /// Batch upsert dir_stats by entry ID inside a savepoint.
    ///
    /// Uses a savepoint instead of `unchecked_transaction()` so it nests correctly
    /// inside explicit transactions (replay's `BEGIN IMMEDIATE`).
    pub fn upsert_dir_stats_by_id(conn: &Connection, stats: &[DirStatsById]) -> Result<(), IndexStoreError> {
        if stats.is_empty() {
            return Ok(());
        }
        with_savepoint(conn, "upsert_stats", |conn| {
            let mut stmt = conn.prepare_cached(
                "INSERT OR REPLACE INTO dir_stats
                     (entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, recursive_dir_count, recursive_has_symlinks, min_subtree_epoch)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for s in stats {
                stmt.execute(params![
                    s.entry_id,
                    s.recursive_logical_size,
                    s.recursive_physical_size,
                    s.recursive_file_count,
                    s.recursive_dir_count,
                    s.recursive_has_symlinks as i32,
                    s.min_subtree_epoch,
                ])?;
            }
            Ok(())
        })
    }

    /// Get aggregated child stats for a parent directory by entry ID.
    #[cfg(test)]
    pub fn get_children_stats_by_id(
        conn: &Connection,
        parent_id: i64,
    ) -> Result<(u64, u64, u64, u64), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN logical_size ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN physical_size ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 1 THEN 1 ELSE 0 END), 0)
             FROM entries WHERE parent_id = ?1",
        )?;
        let row = stmt.query_row(params![parent_id], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;
        Ok(row)
    }

    /// Get total logical size, physical size, file count, and directory count for a subtree.
    pub fn get_subtree_totals_by_id(conn: &Connection, root_id: i64) -> Result<(u64, u64, u64, u64), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM entries WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
            )
            SELECT
                COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN e.logical_size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN e.physical_size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN e.is_directory = 1 THEN 1 ELSE 0 END), 0)
            FROM entries e WHERE e.id IN (SELECT id FROM subtree)",
        )?;
        let row = stmt.query_row(params![root_id], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
            ))
        })?;
        Ok(row)
    }

    /// Count directories that have dir_stats rows.
    pub fn get_dirs_with_stats_count(conn: &Connection) -> Result<u64, IndexStoreError> {
        let count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM dir_stats ds JOIN entries e ON e.id = ds.entry_id WHERE e.is_directory = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Recompute a directory's `min_subtree_epoch` from its own `listed_epoch`
    /// and every child directory's stored `min_subtree_epoch`, taking the
    /// 0-absorbing `min` (any `0` ⇒ `0`).
    ///
    /// This is the per-dir step of `delta::propagate_min_subtree_epoch`, kept in
    /// the store as a single SQL pass so the walk does one round trip per
    /// ancestor instead of N+1. A dir with no `entries` row is treated as
    /// unlisted (`0`). The 0-absorbing semantics differ from a plain `MIN()`:
    /// SQL `MIN` over `{1, 0}` is `0` (correct here by luck), but `MIN` over an
    /// empty child set is `NULL`, and `MIN` ignoring NULLs could mask a missing
    /// `dir_stats` row — so the CTE coalesces a missing child stat to `0`.
    pub fn recompute_min_subtree_epoch(conn: &Connection, dir_id: i64) -> Result<u64, IndexStoreError> {
        // Own listed_epoch (0 if the row is gone).
        let own: u64 = conn
            .prepare_cached("SELECT listed_epoch FROM entries WHERE id = ?1")?
            .query_row(params![dir_id], |row| row.get::<_, u64>(0))
            .optional()?
            .unwrap_or(0);
        if own == 0 {
            return Ok(0);
        }

        // 0-absorbing min over child dirs' min_subtree_epoch. A child dir with no
        // dir_stats row counts as 0 (unknown coverage), so LEFT JOIN + COALESCE.
        // If ANY child is 0, MIN is 0. No child dirs ⇒ MIN over empty ⇒ NULL ⇒
        // keep `own` (a listed-but-childless dir is fully covered at its epoch).
        let child_min: Option<u64> = conn
            .prepare_cached(
                "SELECT MIN(COALESCE(ds.min_subtree_epoch, 0))
                 FROM entries c
                 LEFT JOIN dir_stats ds ON ds.entry_id = c.id
                 WHERE c.parent_id = ?1 AND c.is_directory = 1",
            )?
            .query_row(params![dir_id], |row| row.get::<_, Option<u64>>(0))?;

        Ok(match child_min {
            None => own,             // no child dirs: covered at own epoch
            Some(0) => 0,            // some child subtree is unlisted
            Some(cm) => own.min(cm), // weakest link across self + children
        })
    }
}
