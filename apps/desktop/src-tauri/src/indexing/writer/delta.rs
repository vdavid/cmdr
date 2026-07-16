//! Recursive `dir_stats` propagation up the `parent_id` chain.
//!
//! These helpers keep ancestor aggregates consistent after a single-entry
//! mutation: `propagate_delta_by_id` walks size/count deltas upward, and
//! `propagate_recursive_has_symlinks` recomputes the OR-aggregated symlink flag.
//! All run on the writer thread, inside whatever transaction the caller holds.
//!
//! When an exact delta can't be trusted — a subtraction that would drive a field
//! below zero (arithmetic proof the stored balance drifted), or a debit against
//! a missing row — the walk escalates to [`super::repair::repair_dir_stats_upward`]
//! rather than clamping the lie into place. See the ledger design in
//! `indexing/DETAILS.md` § "The dir_stats ledger".

use crate::indexing::store::IndexStore;

use super::repair::{recompute_recursive_has_symlinks, repair_dir_stats_upward};

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

        // A size/count delta never changes coverage, so `recursive_has_symlinks`
        // and `min_subtree_epoch` are carried through unchanged from the existing
        // row. Resetting `min_subtree_epoch` here would flip an exact dir to "≥"
        // on every live file write — the lie this milestone prevents.
        let (new_logical, new_physical, new_files, new_dirs, has_symlinks, min_subtree_epoch) = match existing {
            Some(s) => {
                let new_logical = s.recursive_logical_size as i64 + logical_size_delta;
                let new_physical = s.recursive_physical_size as i64 + physical_size_delta;
                let new_files = s.recursive_file_count as i64 + i64::from(file_delta);
                let new_dirs = s.recursive_dir_count as i64 + i64::from(dir_delta);
                // A field going negative is arithmetic PROOF this ancestor's stored
                // balance drifted low (an exact debit exceeded it). Don't clamp the
                // lie into place — escalate: recompute this dir and the rest of the
                // chain from committed children, and log it once as drift telemetry.
                // It should stay silent; a steadily-firing warn means a new leak.
                if new_logical < 0 || new_physical < 0 || new_files < 0 || new_dirs < 0 {
                    log::warn!(
                        target: "indexing::writer",
                        "dir_stats drift at id={current_id} in {db}: stored ({}, {}, {}, {}) + delta (logical={logical_size_delta}, physical={physical_size_delta}, files={file_delta}, dirs={dir_delta}) would go negative; repairing from children",
                        s.recursive_logical_size,
                        s.recursive_physical_size,
                        s.recursive_file_count,
                        s.recursive_dir_count,
                        db = conn.path().unwrap_or("<unknown db>"),
                    );
                    repair_dir_stats_upward(conn, current_id);
                    return;
                }
                (
                    new_logical as u64,
                    new_physical as u64,
                    new_files as u64,
                    new_dirs as u64,
                    s.recursive_has_symlinks,
                    s.min_subtree_epoch,
                )
            }
            None => {
                // No row here. A NEGATIVE component means we're debiting a dir whose
                // aggregate row is missing (Leak C by construction) — materializing a
                // zeroed row would bake in a lie. Repair this dir from its committed
                // children and let the recompute walk up instead.
                if logical_size_delta < 0 || physical_size_delta < 0 || file_delta < 0 || dir_delta < 0 {
                    repair_dir_stats_upward(conn, current_id);
                    return;
                }
                // Pure-positive delta to a missing row: create it (load-bearing for
                // live-created dirs; coverage unknown ⇒ epoch 0). A `MarkDirsListed`
                // + `propagate_min_subtree_epoch` from the shape-change handler sets
                // the real epoch; this default never claims coverage.
                (
                    logical_size_delta as u64,
                    physical_size_delta as u64,
                    i64::from(file_delta) as u64,
                    i64::from(dir_delta) as u64,
                    false,
                    0,
                )
            }
        };

        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO dir_stats
                 (entry_id, recursive_logical_size, recursive_physical_size, recursive_file_count, recursive_dir_count, recursive_has_symlinks, min_subtree_epoch)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![current_id, new_logical, new_physical, new_files, new_dirs, has_symlinks as i32, min_subtree_epoch],
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

/// Walk the parent chain, recomputing `min_subtree_epoch` for each ancestor as
/// the 0-absorbing `min` of the dir's own `listed_epoch` and every child dir's
/// stored `min_subtree_epoch`.
///
/// Structurally mirrors `propagate_recursive_has_symlinks`, with one crucial
/// difference: the per-dir recompute depends on the dir's OWN `listed_epoch`
/// (from `entries`), not only on its children. The OR-aggregate precedent is
/// children-only; coverage is self-and-children. The short-circuit-on-stable
/// still holds: `min` is monotone-down on coverage loss and monotone-up on
/// coverage gain, so once a recomputed ancestor matches its stored value, no
/// further ancestor can change.
///
/// Fire it from the handlers where TREE SHAPE changes (new dir created, delete,
/// subtree delete, move) — never on a pure size/count delta, where coverage is
/// unchanged and `propagate_delta_by_id` carries `min_subtree_epoch` through.
pub(super) fn propagate_min_subtree_epoch(conn: &rusqlite::Connection, start_id: i64) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        let new_value = match IndexStore::recompute_min_subtree_epoch(conn, current_id) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("propagate_min_subtree_epoch: recompute failed for id={current_id}: {e}");
                break;
            }
        };
        let old_value = IndexStore::get_dir_stats_by_id(conn, current_id)
            .ok()
            .flatten()
            .map(|s| s.min_subtree_epoch);

        if old_value == Some(new_value) {
            // No change at this ancestor: the rest of the chain can't change either.
            break;
        }

        // Update only the min_subtree_epoch column, preserving other stats.
        if let Err(e) = conn.execute(
            "UPDATE dir_stats SET min_subtree_epoch = ?1 WHERE entry_id = ?2",
            rusqlite::params![new_value, current_id],
        ) {
            log::warn!("propagate_min_subtree_epoch: update failed for id={current_id}: {e}");
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
                    min_subtree_epoch: 0,
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

    /// REGRESSION (the exact-dir-flips-to-≥ bug): a pure size/count delta must
    /// carry the existing `min_subtree_epoch` through unchanged. A complete dir
    /// (`min_subtree_epoch == current_epoch`) that gets a live file write must
    /// NOT reset to `0` (which would flip it from exact to "≥" on every write —
    /// the lie this milestone prevents).
    #[test]
    fn propagate_delta_by_id_preserves_min_subtree_epoch() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

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

        // The dir is fully covered at epoch 7 (a clean scan stamped it).
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 1000,
                    recursive_physical_size: 1000,
                    recursive_file_count: 5,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 7,
                }],
            )
            .unwrap();
        }

        // A live file write propagates a pure size/count delta.
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
        assert_eq!(result.recursive_logical_size, 1250, "size delta applied");
        assert_eq!(
            result.min_subtree_epoch, 7,
            "a pure size/count delta must NOT reset min_subtree_epoch (exact dir stays exact)"
        );

        writer.shutdown();
    }

    /// Build ROOT → home(10, listed) with a complete subtree at epoch 5, then a
    /// live `UpsertEntryV2` creates a new unlisted dir under home. Home and ROOT
    /// must drop to `min_subtree_epoch = 0` (a new incomplete subtree exists).
    #[test]
    fn live_new_dir_drops_ancestor_min_subtree_epoch_to_zero() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // home(10), listed at epoch 5, with complete dir_stats.
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
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10], 5).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: ROOT_ID,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 1,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 5,
                    },
                    DirStatsById {
                        entry_id: 10,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 5,
                    },
                ],
            )
            .unwrap();
        }

        // A live new dir appears under home.
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "newproj".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let newproj_id = IndexStore::resolve_component(&conn, 10, "newproj").unwrap().unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, newproj_id)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "the new unlisted dir is incomplete"
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "home must drop to incomplete: it now has an unlisted child"
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "ROOT must drop to incomplete too"
        );

        writer.shutdown();
    }

    /// After a new unlisted dir drops coverage, marking it listed and recomputing
    /// up (the reconcile/scan fill path) lifts ancestors back to the epoch.
    #[test]
    fn marking_a_filled_dir_lifts_ancestor_coverage() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10], 5).unwrap();
            IndexStore::update_meta(&conn, "current_epoch", "5").unwrap();
        }

        // New dir appears (drops ancestors to 0).
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: 10,
                name: "newproj".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        let newproj_id = {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::resolve_component(&conn, 10, "newproj").unwrap().unwrap()
        };

        // The fill path: mark the new dir listed at epoch 5, recompute up.
        writer
            .send(WriteMessage::MarkDirsListed {
                ids: vec![newproj_id],
                epoch: 5,
            })
            .unwrap();
        writer.send(WriteMessage::PropagateMinSubtreeEpoch(newproj_id)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, newproj_id)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            5,
            "the now-listed dir is complete at epoch 5"
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            5,
            "home lifts back to 5"
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            5,
            "ROOT lifts back to 5"
        );

        writer.shutdown();
    }

    /// Deleting an incomplete child subtree RAISES the parent's coverage: ROOT
    /// has a complete child (epoch 5) and an incomplete one (epoch 0) → ROOT is
    /// 0. Delete the incomplete one → ROOT rises to 5.
    #[test]
    fn subtree_delete_of_incomplete_child_raises_parent() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "complete".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 20,
                parent_id: ROOT_ID,
                name: "incomplete".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10], 5).unwrap();
            // dir 20 stays listed_epoch=0 (incomplete).
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: ROOT_ID,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 2,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 0,
                    },
                    DirStatsById {
                        entry_id: 10,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 5,
                    },
                    DirStatsById {
                        entry_id: 20,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 0,
                    },
                ],
            )
            .unwrap();
        }

        // Delete the incomplete subtree.
        writer.send(WriteMessage::DeleteSubtreeById(20)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            5,
            "ROOT must rise to 5 once its only incomplete child is gone"
        );

        writer.shutdown();
    }

    /// A cross-parent move recomputes coverage on BOTH ancestor chains: moving an
    /// incomplete subtree from src to dst raises src and drops dst.
    #[test]
    fn move_recomputes_both_chains_min_subtree_epoch() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT → src(10) → incomplete(30); ROOT → dst(20). src holds an
        // incomplete child so src is 0; dst is complete at 5.
        let entries = vec![
            EntryRow {
                id: 10,
                parent_id: ROOT_ID,
                name: "src".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 20,
                parent_id: ROOT_ID,
                name: "dst".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 30,
                parent_id: 10,
                name: "moving".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10, 20], 5).unwrap();
            // dir 30 stays listed_epoch=0 (incomplete).
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: ROOT_ID,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 3,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 0,
                    },
                    DirStatsById {
                        entry_id: 10,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 1,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 0,
                    },
                    DirStatsById {
                        entry_id: 20,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 5,
                    },
                    DirStatsById {
                        entry_id: 30,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 0,
                    },
                ],
            )
            .unwrap();
        }

        // Move incomplete(30) from src(10) to dst(20).
        writer
            .send(WriteMessage::MoveEntryV2 {
                entry_id: 30,
                new_parent_id: 20,
                new_name: "moving".into(),
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            5,
            "src rises to 5: its incomplete child left"
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, 20)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "dst drops to 0: it gained the incomplete child"
        );
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, 30)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            0,
            "the moved subtree's own min is unchanged (it moved intact)"
        );

        writer.shutdown();
    }
}
