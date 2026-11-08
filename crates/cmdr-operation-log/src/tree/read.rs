//! Every read the tree view and the selection make, each served by an index: one
//! summary row by its key, a folder's child folders or files from position k, and a
//! position count. One [`OpReader`] per request caches every row it reads, so a
//! window of 200 rows sharing a few folders reads each folder once.
//!
//! ❗ No statement here may scan an op: the structural gate (`scale_tests.rs`) runs
//! each one under `EXPLAIN QUERY PLAN` and counts its VM steps at two journal sizes.
//! A listing from position k is O(k) inside ONE folder; past a folder's midpoint it
//! reads from the end instead, so the walk is capped at half the folder.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};

use super::TreeAxis;
use super::summary::{FACT_COLUMNS, OpTreeContext, RowFacts, SubtreeTotals, TreeDirRow, map_facts};
use crate::store::OperationLogStoreError;

// Every `operation_tree_dirs` read selects the same 15 columns, in `map_tree_dir`'s order.

/// One summary row by its key.
pub(super) const SQL_DIR: &str = "SELECT dir_id, parent_dir_id, name_folded, self_seq, child_dirs, direct_files, \
     files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, units_rolled_back, \
     units_held_back, units_unfinished, housekeeping_waiting \
     FROM operation_tree_dirs WHERE op_id = ?1 AND dir_id = ?2";

/// The op's volume roots, the top of its tree (several make a forest).
pub(super) const SQL_ROOTS: &str = "SELECT dir_id, parent_dir_id, name_folded, self_seq, child_dirs, \
     direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, \
     units_rolled_back, units_held_back, units_unfinished, housekeeping_waiting \
     FROM operation_tree_dirs WHERE op_id = ?1 AND parent_dir_id IS NULL ORDER BY name_folded, dir_id";

/// The key of the child folder at a listing offset. Index-only: an `OFFSET` over a
/// WITHOUT ROWID table's columns seeks the table for every row it skips (50 ms at the
/// middle of 100,000 child folders), over the index alone it doesn't.
pub(super) const SQL_CHILD_DIR_AT: &str = "SELECT name_folded, dir_id FROM operation_tree_dirs \
     WHERE op_id = ?1 AND parent_dir_id = ?2 ORDER BY name_folded, dir_id LIMIT 1 OFFSET ?3";

/// The same, counted from the end of the listing.
pub(super) const SQL_CHILD_DIR_AT_DESC: &str = "SELECT name_folded, dir_id FROM operation_tree_dirs \
     WHERE op_id = ?1 AND parent_dir_id = ?2 ORDER BY name_folded DESC, dir_id DESC LIMIT 1 OFFSET ?3";

/// A folder's child folders in listing order, from a key on.
pub(super) const SQL_CHILD_DIRS_FROM: &str = "SELECT dir_id, parent_dir_id, name_folded, self_seq, child_dirs, \
     direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, \
     units_rolled_back, units_held_back, units_unfinished, housekeeping_waiting \
     FROM operation_tree_dirs WHERE op_id = ?1 AND parent_dir_id = ?2 AND (name_folded, dir_id) >= (?3, ?4) \
     ORDER BY name_folded, dir_id LIMIT ?5";

/// The same, backwards from a key.
pub(super) const SQL_CHILD_DIRS_UPTO: &str = "SELECT dir_id, parent_dir_id, name_folded, self_seq, child_dirs, \
     direct_files, files, bytes, units_selectable, unit_files_selectable, unit_bytes_selectable, \
     units_rolled_back, units_held_back, units_unfinished, housekeeping_waiting \
     FROM operation_tree_dirs WHERE op_id = ?1 AND parent_dir_id = ?2 AND (name_folded, dir_id) <= (?3, ?4) \
     ORDER BY name_folded DESC, dir_id DESC LIMIT ?5";

/// How many child folders sort before one (capped, see [`OpReader::position_of_dir`]).
pub(super) const SQL_COUNT_DIRS_BEFORE: &str = "SELECT COUNT(*) FROM (SELECT 1 FROM operation_tree_dirs \
     WHERE op_id = ?1 AND parent_dir_id = ?2 AND (name_folded, dir_id) < (?3, ?4) LIMIT ?5)";

/// How many sort after it.
pub(super) const SQL_COUNT_DIRS_AFTER: &str = "SELECT COUNT(*) FROM (SELECT 1 FROM operation_tree_dirs \
     WHERE op_id = ?1 AND parent_dir_id = ?2 AND (name_folded, dir_id) > (?3, ?4) LIMIT ?5)";

/// One item row by its `seq`.
pub(super) fn sql_item() -> String {
    format!("SELECT {FACT_COLUMNS}, item_id FROM operation_items WHERE op_id = ?1 AND seq = ?2")
}

/// One dir's exact name and parent.
pub(super) const SQL_DIR_NAME: &str = "SELECT name, parent_dir_id FROM dirs WHERE dir_id = ?1";

/// The op's first few rows, for the other side's root.
pub(super) fn sql_first_rows() -> String {
    format!("SELECT {FACT_COLUMNS}, item_id FROM operation_items WHERE op_id = ?1 ORDER BY seq LIMIT ?2")
}

/// Which way a listing reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

/// The file-listing statements for one axis. A dest-axis folder also holds the rows
/// that fell back to their source side (no dest), so its listing merges two index
/// ranges (`UNION ALL` with an `ORDER BY` SQLite answers by merging, never sorting).
pub(super) struct FileSql {
    pub(super) asc: String,
    pub(super) desc: String,
    pub(super) before: String,
    pub(super) after: String,
}

impl FileSql {
    pub(super) fn for_axis(axis: TreeAxis) -> FileSql {
        // (the name the folder sorts by, which rows sit in folder ?2)
        let arms: &[(&str, &str)] = match axis {
            TreeAxis::Source => &[("source_name_folded", "source_dir_id = ?2")],
            TreeAxis::Dest => &[
                ("dest_name_folded", "dest_dir_id = ?2"),
                ("source_name_folded", "source_dir_id = ?2 AND dest_dir_id IS NULL"),
            ],
        };
        let listing = |order: &str| {
            let selects: Vec<String> = arms
                .iter()
                .map(|(key, place)| {
                    format!(
                        "SELECT {FACT_COLUMNS}, item_id, {key} AS k FROM operation_items \
                         WHERE op_id = ?1 AND {place} AND entry_type = 'file'"
                    )
                })
                .collect();
            format!(
                "{} ORDER BY k {order}, item_id {order} LIMIT ?3 OFFSET ?4",
                selects.join(" UNION ALL ")
            )
        };
        let count = |cmp: &str| {
            let counts: Vec<String> = arms
                .iter()
                .map(|(key, place)| {
                    format!(
                        "(SELECT COUNT(*) FROM (SELECT 1 FROM operation_items WHERE op_id = ?1 AND {place} \
                         AND entry_type = 'file' AND ({key}, item_id) {cmp} (?3, ?4) LIMIT ?5))"
                    )
                })
                .collect();
            format!("SELECT {}", counts.join(" + "))
        };
        FileSql {
            asc: listing("ASC"),
            desc: listing("DESC"),
            before: count("<"),
            after: count(">"),
        }
    }
}

/// A file row as a listing reads it: its facts plus its tie-breaking `item_id`.
#[derive(Debug, Clone)]
pub(crate) struct FileRow {
    pub(crate) facts: RowFacts,
    pub(crate) item_id: i64,
}

fn map_tree_dir(row: &rusqlite::Row<'_>) -> rusqlite::Result<TreeDirRow> {
    Ok(TreeDirRow {
        dir_id: row.get(0)?,
        parent_dir_id: row.get(1)?,
        name_folded: row.get(2)?,
        self_seq: row.get(3)?,
        child_dirs: row.get(4)?,
        direct_files: row.get(5)?,
        totals: SubtreeTotals {
            files: row.get(6)?,
            bytes: row.get(7)?,
            units_selectable: row.get(8)?,
            unit_files_selectable: row.get(9)?,
            unit_bytes_selectable: row.get(10)?,
            units_rolled_back: row.get(11)?,
            units_held_back: row.get(12)?,
            units_unfinished: row.get(13)?,
            housekeeping_waiting: row.get(14)?,
        },
    })
}

fn map_file(row: &rusqlite::Row<'_>) -> Result<FileRow, OperationLogStoreError> {
    Ok(FileRow {
        facts: map_facts(row)?,
        item_id: row.get(11)?,
    })
}

/// The summary rows an [`OpReader`] has read, by `dir_id` (`None`: the op has no row
/// there). Its own type so a reader can be handed one that outlives it: the rollback
/// filter lends its run-long cache to each page's reader.
pub(crate) type DirCache = HashMap<i64, Option<TreeDirRow>>;

/// One request's reads of one op, each row cached on first read.
pub(crate) struct OpReader<'c> {
    conn: &'c Connection,
    op_id: String,
    pub(crate) context: OpTreeContext,
    dirs: DirCache,
    /// Summary point reads this reader made (cache misses), for the read-count gates.
    dir_reads: u64,
    items: HashMap<i64, Option<FileRow>>,
    names: HashMap<i64, (String, Option<i64>)>,
    paths: HashMap<i64, String>,
    files: FileSql,
}

impl<'c> OpReader<'c> {
    pub(crate) fn new(conn: &'c Connection, op_id: &str, context: OpTreeContext) -> Self {
        Self::with_dirs(conn, op_id, context, DirCache::new())
    }

    /// A reader that starts from `dirs`, rows an earlier reader of the same op read.
    pub(crate) fn with_dirs(conn: &'c Connection, op_id: &str, context: OpTreeContext, dirs: DirCache) -> Self {
        OpReader {
            conn,
            op_id: op_id.to_string(),
            context,
            dirs,
            dir_reads: 0,
            items: HashMap::new(),
            names: HashMap::new(),
            paths: HashMap::new(),
            files: FileSql::for_axis(context.axis),
        }
    }

    /// The summary row of `dir_id` in this op, `None` when the op has none there.
    pub(crate) fn dir(&mut self, dir_id: i64) -> Result<Option<&TreeDirRow>, OperationLogStoreError> {
        if !self.dirs.contains_key(&dir_id) {
            let row = self
                .conn
                .prepare_cached(SQL_DIR)?
                .query_row(rusqlite::params![self.op_id, dir_id], map_tree_dir)
                .optional()?;
            self.dirs.insert(dir_id, row);
            self.dir_reads += 1;
        }
        Ok(self.dirs.get(&dir_id).and_then(Option::as_ref))
    }

    /// Hand the summary rows read so far to the next reader, leaving this one empty.
    pub(crate) fn take_dirs(&mut self) -> DirCache {
        std::mem::take(&mut self.dirs)
    }

    /// Summary point reads this reader made, cache hits not counted.
    pub(crate) fn dir_reads(&self) -> u64 {
        self.dir_reads
    }

    /// The summary row of a dir this op is known to have (a parent of one it has).
    pub(crate) fn known_dir(&mut self, dir_id: i64) -> Result<TreeDirRow, OperationLogStoreError> {
        self.dir(dir_id)?.cloned().ok_or(OperationLogStoreError::Decode {
            column: "operation_tree_dirs",
            value: format!("no summary row for dir {dir_id}"),
        })
    }

    /// The item row at `seq`, `None` when this op has none.
    pub(crate) fn item(&mut self, seq: i64) -> Result<Option<&FileRow>, OperationLogStoreError> {
        if !self.items.contains_key(&seq) {
            let mut stmt = self.conn.prepare_cached(&sql_item())?;
            let mut rows = stmt.query(rusqlite::params![self.op_id, seq])?;
            let row = match rows.next()? {
                Some(row) => Some(map_file(row)?),
                None => None,
            };
            self.items.insert(seq, row);
        }
        Ok(self.items.get(&seq).and_then(Option::as_ref))
    }

    /// The op's volume roots.
    pub(crate) fn roots(&mut self) -> Result<Vec<TreeDirRow>, OperationLogStoreError> {
        let rows: Vec<TreeDirRow> = self
            .conn
            .prepare_cached(SQL_ROOTS)?
            .query_map(rusqlite::params![self.op_id], map_tree_dir)?
            .collect::<Result<_, _>>()?;
        for row in &rows {
            self.dirs.insert(row.dir_id, Some(row.clone()));
        }
        Ok(rows)
    }

    /// Up to `count` child folders of `parent` from listing position `from`, of `total`.
    pub(crate) fn child_dirs(
        &mut self,
        parent: i64,
        from: u64,
        count: u64,
        total: u64,
    ) -> Result<Vec<TreeDirRow>, OperationLogStoreError> {
        let (direction, offset, take) = plan_range(from, count, total);
        if take == 0 {
            return Ok(Vec::new());
        }
        let (at, run) = match direction {
            Direction::Up => (SQL_CHILD_DIR_AT, SQL_CHILD_DIRS_FROM),
            Direction::Down => (SQL_CHILD_DIR_AT_DESC, SQL_CHILD_DIRS_UPTO),
        };
        let anchor: Option<(String, i64)> = self
            .conn
            .prepare_cached(at)?
            .query_row(rusqlite::params![self.op_id, parent, offset], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let Some((name, dir)) = anchor else {
            return Ok(Vec::new());
        };
        let mut rows: Vec<TreeDirRow> = self
            .conn
            .prepare_cached(run)?
            .query_map(rusqlite::params![self.op_id, parent, name, dir, take], map_tree_dir)?
            .collect::<Result<_, _>>()?;
        if direction == Direction::Down {
            rows.reverse();
        }
        for row in &rows {
            self.dirs.insert(row.dir_id, Some(row.clone()));
        }
        Ok(rows)
    }

    /// Up to `count` files directly in `dir` from listing position `from`, of `total`.
    pub(crate) fn files(
        &mut self,
        dir: i64,
        from: u64,
        count: u64,
        total: u64,
    ) -> Result<Vec<FileRow>, OperationLogStoreError> {
        let (direction, offset, take) = plan_range(from, count, total);
        let sql = match direction {
            Direction::Up => &self.files.asc,
            Direction::Down => &self.files.desc,
        };
        let mut out = Vec::new();
        {
            let mut stmt = self.conn.prepare_cached(sql)?;
            let mut rows = stmt.query(rusqlite::params![self.op_id, dir, take, offset])?;
            while let Some(row) = rows.next()? {
                out.push(map_file(row)?);
            }
        }
        if direction == Direction::Down {
            out.reverse();
        }
        for row in &out {
            self.items.insert(row.facts.seq, Some(row.clone()));
        }
        Ok(out)
    }

    /// Where child folder `dir` sits among `parent`'s `total` child folders. Counts
    /// from whichever end is nearer, so it walks at most half the listing.
    pub(crate) fn position_of_dir(
        &mut self,
        parent: i64,
        dir: &TreeDirRow,
        total: u64,
    ) -> Result<u64, OperationLogStoreError> {
        let key = (dir.name_folded.as_str(), dir.dir_id);
        self.position(SQL_COUNT_DIRS_BEFORE, SQL_COUNT_DIRS_AFTER, parent, key, total)
    }

    /// Where file `file` sits among the `total` files directly in `dir`.
    pub(crate) fn position_of_file(
        &mut self,
        dir: i64,
        name_folded: &str,
        item_id: i64,
        total: u64,
    ) -> Result<u64, OperationLogStoreError> {
        let (before, after) = (self.files.before.clone(), self.files.after.clone());
        self.position(&before, &after, dir, (name_folded, item_id), total)
    }

    fn position(
        &mut self,
        before_sql: &str,
        after_sql: &str,
        container: i64,
        key: (&str, i64),
        total: u64,
    ) -> Result<u64, OperationLogStoreError> {
        let half = total / 2 + 1;
        let count = |sql: &str| -> Result<u64, OperationLogStoreError> {
            let n: i64 = self.conn.prepare_cached(sql)?.query_row(
                rusqlite::params![self.op_id, container, key.0, key.1, half as i64],
                |row| row.get(0),
            )?;
            Ok(n.max(0) as u64)
        };
        let before = count(before_sql)?;
        if before < half {
            return Ok(before);
        }
        let after = count(after_sql)?;
        Ok(total.saturating_sub(1).saturating_sub(after))
    }

    /// A dir's exact name and parent (the `dirs` row, which every volume shares).
    pub(crate) fn dir_name(&mut self, dir_id: i64) -> Result<(String, Option<i64>), OperationLogStoreError> {
        if let Some(known) = self.names.get(&dir_id) {
            return Ok(known.clone());
        }
        let read: (String, Option<i64>) = self
            .conn
            .prepare_cached(SQL_DIR_NAME)?
            .query_row(rusqlite::params![dir_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        self.names.insert(dir_id, read.clone());
        Ok(read)
    }

    /// A dir's full path, as `store::reconstruct_dir_path` renders it, each distinct
    /// dir resolved once per request.
    pub(crate) fn path(&mut self, dir_id: i64) -> Result<String, OperationLogStoreError> {
        if let Some(known) = self.paths.get(&dir_id) {
            return Ok(known.clone());
        }
        let (name, parent) = self.dir_name(dir_id)?;
        let path = match parent {
            None => "/".to_string(),
            Some(parent) => {
                let parent_path = self.path(parent)?;
                join_path(&parent_path, &name)
            }
        };
        self.paths.insert(dir_id, path.clone());
        Ok(path)
    }

    /// The op's first `n` rows by `seq`, for working out where the other side starts.
    pub(crate) fn first_rows(&mut self, n: i64) -> Result<Vec<FileRow>, OperationLogStoreError> {
        let mut out = Vec::new();
        let mut stmt = self.conn.prepare_cached(&sql_first_rows())?;
        let mut rows = stmt.query(rusqlite::params![self.op_id, n])?;
        while let Some(row) = rows.next()? {
            out.push(map_file(row)?);
        }
        Ok(out)
    }
}

/// `name` inside a folder at `parent_path`, as `reconstruct_dir_path` joins: the
/// volume root renders `/`, and a `scheme://authority` root stands alone.
pub(crate) fn join_path(parent_path: &str, name: &str) -> String {
    if parent_path == "/" {
        if name.contains("://") {
            return name.to_string();
        }
        return format!("/{name}");
    }
    format!("{parent_path}/{name}")
}

/// Which end to read `count` entries at listing position `from` of `total` from, the
/// SQL offset that lands there, and how many to take.
fn plan_range(from: u64, count: u64, total: u64) -> (Direction, i64, i64) {
    let take = count.min(total.saturating_sub(from));
    if from + take / 2 <= total / 2 {
        (Direction::Up, from as i64, take as i64)
    } else {
        let offset = total.saturating_sub(from).saturating_sub(take);
        (Direction::Down, offset as i64, take as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::{Direction, join_path, plan_range};

    #[test]
    fn a_range_past_the_middle_reads_from_the_end() {
        assert_eq!(plan_range(0, 10, 100), (Direction::Up, 0, 10));
        assert_eq!(plan_range(40, 10, 100), (Direction::Up, 40, 10));
        assert_eq!(plan_range(90, 10, 100), (Direction::Down, 0, 10));
        assert_eq!(plan_range(60, 10, 100), (Direction::Down, 30, 10));
        assert_eq!(plan_range(95, 10, 100), (Direction::Down, 0, 5), "clipped at the end");
    }

    #[test]
    fn paths_join_as_the_store_renders_them() {
        assert_eq!(join_path("/", "a"), "/a");
        assert_eq!(join_path("/a", "b"), "/a/b");
        assert_eq!(join_path("/", "smb://nas"), "smb://nas");
        assert_eq!(join_path("smb://nas", "share"), "smb://nas/share");
    }
}
