//! Recursive `dir_stats` propagation up the `parent_id` chain.
//!
//! These helpers keep ancestor aggregates consistent after a single-entry
//! mutation: `propagate_delta_by_id` walks size/count deltas upward, and
//! `propagate_recursive_has_symlinks` recomputes the OR-aggregated symlink flag.
//! All run on the writer thread, inside whatever transaction the caller holds.

use crate::indexing::store::IndexStore;

pub(super) fn propagate_delta_by_id(
    conn: &rusqlite::Connection,
    start_id: i64,
    logical_size_delta: i64,
    physical_size_delta: i64,
    file_delta: i32,
    dir_delta: i32,
) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        // Read existing stats
        let existing = IndexStore::get_dir_stats_by_id(conn, current_id).ok().flatten();

        let (new_logical, new_physical, new_files, new_dirs, has_symlinks) = match existing {
            Some(s) => (
                (s.recursive_logical_size as i64 + logical_size_delta).max(0) as u64,
                (s.recursive_physical_size as i64 + physical_size_delta).max(0) as u64,
                (s.recursive_file_count as i64 + i64::from(file_delta)).max(0) as u64,
                (s.recursive_dir_count as i64 + i64::from(dir_delta)).max(0) as u64,
                s.recursive_has_symlinks,
            ),
            None => (
                logical_size_delta.max(0) as u64,
                physical_size_delta.max(0) as u64,
                i64::from(file_delta).max(0) as u64,
                i64::from(dir_delta).max(0) as u64,
                false,
            ),
        };

        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO dir_stats
                 (entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, recursive_dir_count, recursive_has_symlinks)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![current_id, new_logical, new_physical, new_files, new_dirs, has_symlinks as i32],
        ) {
            log::warn!("propagate_delta_by_id: upsert failed for id={current_id}: {e}");
            break;
        }

        // Walk up to parent
        if current_id == ROOT_ID {
            break;
        }
        match IndexStore::get_parent_id(conn, current_id) {
            Ok(Some(pid)) if pid != 0 => current_id = pid,
            _ => break,
        }
    }
}

/// Recompute `recursive_has_symlinks` for a directory from its direct children
/// (`is_symlink`) plus its subdirectories' stored `recursive_has_symlinks`.
///
/// Returns the recomputed value, without writing it. Returns `false` if the
/// directory has no children or the queries fail.
fn recompute_recursive_has_symlinks(conn: &rusqlite::Connection, dir_id: i64) -> bool {
    // Direct symlink child?
    let direct: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE parent_id = ?1 AND is_symlink = 1)",
            rusqlite::params![dir_id],
            |row| row.get::<_, i32>(0).map(|n| n != 0),
        )
        .unwrap_or(false);
    if direct {
        return true;
    }
    // Any sub-directory with the flag set?
    let from_subdirs: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM entries e
                JOIN dir_stats ds ON ds.entry_id = e.id
                WHERE e.parent_id = ?1 AND e.is_directory = 1 AND ds.recursive_has_symlinks = 1
            )",
            rusqlite::params![dir_id],
            |row| row.get::<_, i32>(0).map(|n| n != 0),
        )
        .unwrap_or(false);
    from_subdirs
}

/// Walk the parent chain, recomputing `recursive_has_symlinks` for each ancestor
/// from its direct children + subdirs' stored flags.
///
/// Stops walking up as soon as an ancestor's recomputed value matches the value
/// already in the DB. The OR-aggregate is monotonic, so once the value stabilizes,
/// further ancestors won't change.
///
/// Used after symlink additions/removals (and subtree deletes that may have
/// removed all symlinks in a branch). For pure size/count deltas this is a no-op
/// and `propagate_delta_by_id` is enough.
pub(super) fn propagate_recursive_has_symlinks(conn: &rusqlite::Connection, start_id: i64) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        let new_value = recompute_recursive_has_symlinks(conn, current_id);
        let old_value = IndexStore::get_dir_stats_by_id(conn, current_id)
            .ok()
            .flatten()
            .map(|s| s.recursive_has_symlinks);

        if old_value == Some(new_value) {
            // No change: the rest of the chain can't change either.
            break;
        }

        // Update only the recursive_has_symlinks column, preserving other stats.
        if let Err(e) = conn.execute(
            "UPDATE dir_stats SET recursive_has_symlinks = ?1 WHERE entry_id = ?2",
            rusqlite::params![new_value as i32, current_id],
        ) {
            log::warn!("propagate_recursive_has_symlinks: update failed for id={current_id}: {e}");
            break;
        }

        if current_id == ROOT_ID {
            break;
        }
        match IndexStore::get_parent_id(conn, current_id) {
            Ok(Some(pid)) if pid != 0 => current_id = pid,
            _ => break,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{DirStatsById, EntryRow, ROOT_ID};
    use crate::indexing::writer::tests::setup_db;
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    #[test]
    fn propagate_delta_by_id_via_writer() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // Insert a directory to propagate to
        let entries = vec![EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();

        // Pre-populate dir_stats
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 1000,
                    recursive_physical_size: 1000,
                    recursive_file_count: 5,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                }],
            )
            .unwrap();
        }

        // Propagate a file addition starting from home's entry_id
        writer
            .send(WriteMessage::PropagateDeltaById {
                entry_id: 10,
                logical_size_delta: 250,
                physical_size_delta: 250,
                file_count_delta: 1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let result = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(result.recursive_logical_size, 1250);
        assert_eq!(result.recursive_file_count, 6);

        writer.shutdown();
    }
}
