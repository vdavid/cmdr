//! The `dir_stats` repair primitive: recompute-from-children escalation.
//!
//! When a mutation knows *exactly* what changed it applies a delta ([`super::delta`]);
//! when it knows something changed but not exactly what, it escalates here.
//! [`repair_dir_stats_upward`] walks the `parent_id` chain rewriting each level's
//! aggregate from its committed children, short-circuiting once a level already
//! matches. It's the universal escalation behind the negative-delta detector, the
//! subtree-scan ancestor fix, and the backfill ancestor fix — see the ledger
//! design in `indexing/DETAILS.md` § "The dir_stats ledger".
//!
//! Everything here runs on the writer thread, inside whatever transaction the
//! caller holds.

use crate::indexing::store::{DirStatsById, IndexStore, IndexStoreError};

/// Recompute a directory's aggregate from its committed children and walk that
/// recompute up the `parent_id` chain, rewriting each level, until a level's
/// recompute already equals its stored row.
///
/// Each level is one indexed `SUM` over the dir's direct children — file
/// children read from `entries`, child dirs from their stored `dir_stats` rows
/// — so repairing near the root costs `~depth` aggregate queries, not a
/// recursive-CTE rescan. It trusts the already-stored child rows: the result is
/// self-consistent with current state, and any error below heals the next time
/// anything below repairs (monotone convergence toward truth, never preserved
/// corruption).
///
/// The short-circuit compares the FULL row (sizes, counts, `recursive_has_symlinks`,
/// AND `min_subtree_epoch`): a size-only OR an epoch-only OR a symlink-only
/// difference all keep the walk going, mirroring the two per-field up-walkers in
/// [`super::delta`]. Coverage restoration depends on this — an epoch change that
/// doesn't move any size must still propagate.
///
/// Missing-child-row semantics (a child dir with no `dir_stats` row): it
/// contributes **0 to sizes and counts** (LEFT JOIN + COALESCE 0), **absorbs
/// `min_subtree_epoch` to 0** (matching `store::recompute_min_subtree_epoch`),
/// and **false for symlinks**. The resulting under-count is the accepted
/// convergence state — coverage reads honestly incomplete (epoch 0) until
/// backfill heals the missing row and repairs upward.
///
/// Idempotent and order-independent: two callers produce the same rows and a
/// duplicate call is a cheap no-op after the short-circuit, so it's safe to fire
/// from every escalation site without coordination. Writer-thread only; don't
/// add a `WriteMessage::RepairDirStats` until a real off-thread caller exists.
pub(super) fn repair_dir_stats_upward(conn: &rusqlite::Connection, start_id: i64) {
    use crate::indexing::store::ROOT_ID;

    let mut current_id = start_id;
    while current_id != 0 {
        let fresh = match recompute_dir_stats_from_children(conn, current_id) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("repair_dir_stats_upward: recompute failed for id={current_id}: {e}");
                break;
            }
        };

        let stored = IndexStore::get_dir_stats_by_id(conn, current_id).ok().flatten();
        if stored.as_ref() == Some(&fresh) {
            // This ancestor already agrees with its children, so the change below
            // it never reached here and nothing above can change either.
            break;
        }

        if let Err(e) = IndexStore::upsert_dir_stats_by_id(conn, std::slice::from_ref(&fresh)) {
            log::warn!("repair_dir_stats_upward: upsert failed for id={current_id}: {e}");
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

/// Recompute one directory's `dir_stats` from its committed children: file
/// children summed from `entries`, child dirs from their stored `dir_stats`
/// rows. Mirrors the aggregator's `compute_bottom_up` per-level rollup. See
/// [`repair_dir_stats_upward`] for the missing-child-row semantics.
fn recompute_dir_stats_from_children(
    conn: &rusqlite::Connection,
    dir_id: i64,
) -> Result<DirStatsById, IndexStoreError> {
    let (logical, physical, files, dirs) = conn.query_row(
        "SELECT
             COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN COALESCE(e.logical_size, 0)
                               ELSE COALESCE(ds.recursive_logical_size, 0) END), 0),
             COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN COALESCE(e.physical_size, 0)
                               ELSE COALESCE(ds.recursive_physical_size, 0) END), 0),
             COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN 1
                               ELSE COALESCE(ds.recursive_file_count, 0) END), 0),
             COALESCE(SUM(CASE WHEN e.is_directory = 1 THEN 1 + COALESCE(ds.recursive_dir_count, 0)
                               ELSE 0 END), 0)
         FROM entries e
         LEFT JOIN dir_stats ds ON ds.entry_id = e.id
         WHERE e.parent_id = ?1",
        rusqlite::params![dir_id],
        |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
            ))
        },
    )?;

    Ok(DirStatsById {
        entry_id: dir_id,
        recursive_logical_size: logical,
        recursive_physical_size: physical,
        recursive_file_count: files,
        recursive_dir_count: dirs,
        recursive_has_symlinks: recompute_recursive_has_symlinks(conn, dir_id),
        min_subtree_epoch: IndexStore::recompute_min_subtree_epoch(conn, dir_id)?,
    })
}

/// Recompute `recursive_has_symlinks` for a directory from its direct children
/// (`is_symlink`) plus its subdirectories' stored `recursive_has_symlinks`.
///
/// Returns the recomputed value, without writing it. Returns `false` if the
/// directory has no children or the queries fail. Consumed by both the repair
/// recompute above and [`super::delta::propagate_recursive_has_symlinks`].
pub(super) fn recompute_recursive_has_symlinks(conn: &rusqlite::Connection, dir_id: i64) -> bool {
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{DirStatsById, EntryRow, ROOT_ID};
    use crate::indexing::stress_test_helpers::check_db_consistency;
    use crate::indexing::writer::tests::setup_db;
    use crate::indexing::writer::{IndexWriter, WriteMessage};

    // ── The incident fingerprint: negative-delta escalation ──────────

    /// Seed ROOT → A → {big, keep}, aggregate correctly, then artificially drift
    /// A's `dir_stats` LOW (simulating leaked ancestor credits), and
    /// `DeleteSubtreeById(big)`. The exact debit (2000 bytes / 2 files / 1 dir,
    /// read from `entries`) exceeds A's drifted balance, so the ancestor walk
    /// goes negative. Pre-fix it floored to a lie; post-fix it must escalate to a
    /// recompute and leave A EXACT versus the recompute-from-`entries` oracle.
    #[test]
    fn delete_subtree_repairs_drifted_ancestor_via_oracle() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → A(10) → big(20) → f21(1000), f22(1000)
        //                 → keep(30) → f31(500)
        let entries = vec![
            dir_entry(10, ROOT_ID, "A"),
            dir_entry(20, 10, "big"),
            file_entry(21, 20, "f21", 1000),
            file_entry(22, 20, "f22", 1000),
            dir_entry(30, 10, "keep"),
            file_entry(31, 30, "f31", 500),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Drift A low: under-credited (true A = 2500 bytes / 3 files / 2 dirs).
        drift_dir_stats_low(&db_path, 10, 1500, 2, 2);

        // Delete the large subtree. The exact debit exceeds A's drifted balance.
        writer.send(WriteMessage::DeleteSubtreeById(20)).unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        // Pre-fix this stored 0/0/1492 for a 1.21 GB subtree.
        let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
        assert_eq!(
            (a.recursive_logical_size, a.recursive_file_count, a.recursive_dir_count),
            (500, 1, 1),
            "A must reflect the surviving `keep` subtree exactly, not a clamped lie"
        );
        let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(
            (
                root.recursive_logical_size,
                root.recursive_file_count,
                root.recursive_dir_count
            ),
            (500, 1, 2),
        );
        // The strong oracle: the whole tree agrees with a recompute from `entries`.
        check_db_consistency(&conn);

        writer.shutdown();
    }

    /// A NEGATIVE `PropagateDeltaById` to a dir whose `dir_stats` row is MISSING
    /// (Leak C by construction) must NOT materialize a zeroed row — it must
    /// repair from the dir's committed children instead.
    #[test]
    fn propagate_delta_none_branch_negative_repairs_not_zeroes() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → X(40) → f41(300)
        let entries = vec![dir_entry(40, ROOT_ID, "X"), file_entry(41, 40, "f41", 300)];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Simulate the missing row: drop X's dir_stats (a dir that has entries but
        // no aggregate row — exactly what backfill would later heal).
        delete_dir_stats_row(&db_path, 40);

        // A negative delta lands on the missing row.
        writer
            .send(WriteMessage::PropagateDeltaById {
                entry_id: 40,
                logical_size_delta: -100,
                physical_size_delta: -100,
                file_count_delta: -1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let x = IndexStore::get_dir_stats_by_id(&conn, 40).unwrap().unwrap();
        assert_eq!(
            (x.recursive_logical_size, x.recursive_file_count, x.recursive_dir_count),
            (300, 1, 0),
            "missing row + negative delta must recompute from children, not zero out"
        );

        writer.shutdown();
    }

    /// Twin of the above pinning the KEPT behavior: a PURE-POSITIVE delta to a
    /// missing row still CREATES the row from the delta (load-bearing for
    /// live-created dirs; coverage unknown ⇒ epoch 0).
    #[test]
    fn propagate_delta_none_branch_positive_creates_row() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        let entries = vec![dir_entry(50, ROOT_ID, "Y")];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();
        // Y has no dir_stats row at all (never aggregated).
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            assert!(IndexStore::get_dir_stats_by_id(&conn, 50).unwrap().is_none());
        }

        writer
            .send(WriteMessage::PropagateDeltaById {
                entry_id: 50,
                logical_size_delta: 200,
                physical_size_delta: 200,
                file_count_delta: 1,
                dir_count_delta: 0,
            })
            .unwrap();
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let y = IndexStore::get_dir_stats_by_id(&conn, 50).unwrap().unwrap();
        assert_eq!(
            (
                y.recursive_logical_size,
                y.recursive_file_count,
                y.recursive_dir_count,
                y.min_subtree_epoch
            ),
            (200, 1, 0, 0),
            "pure-positive delta to a missing row creates it from the delta, epoch 0"
        );

        writer.shutdown();
    }

    // ── repair_dir_stats_upward unit tests (contract) ────────────────

    /// Repairs a wrong middle row and every ancestor above it.
    #[test]
    fn repair_fixes_wrong_middle_row_and_above() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → A(10) → B(20) → f(21, 700)
        let entries = vec![
            dir_entry(10, ROOT_ID, "A"),
            dir_entry(20, 10, "B"),
            file_entry(21, 20, "f", 700),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Drift the MIDDLE row B and (independently) A, leaving ROOT stale too.
        drift_dir_stats_low(&db_path, 20, 111, 0, 0);
        drift_dir_stats_low(&db_path, 10, 222, 0, 0);

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            repair_dir_stats_upward(&conn, 20);
            check_db_consistency(&conn);
        }

        writer.shutdown();
    }

    /// Short-circuits: once a level already matches its children, the walk stops.
    /// A deliberately-poisoned row ABOVE the correct level must stay poisoned —
    /// proof the walk didn't reach it.
    #[test]
    fn repair_short_circuits_leaving_poisoned_ancestor_untouched() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → A(10) → B(20) → f(21, 700). Everything correct after aggregate.
        let entries = vec![
            dir_entry(10, ROOT_ID, "A"),
            dir_entry(20, 10, "B"),
            file_entry(21, 20, "f", 700),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        // Poison A (a level ABOVE B) with a sentinel. B is already correct, so a
        // repair starting at B recomputes B (unchanged ⇒ short-circuit) and never
        // touches A.
        let poison = DirStatsById {
            entry_id: 10,
            recursive_logical_size: 999_999,
            recursive_physical_size: 999_999,
            recursive_file_count: 42,
            recursive_dir_count: 7,
            recursive_has_symlinks: true,
            min_subtree_epoch: 0,
        };
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::upsert_dir_stats_by_id(&conn, std::slice::from_ref(&poison)).unwrap();

            repair_dir_stats_upward(&conn, 20);

            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(
                a, poison,
                "the short-circuit at B must leave the poisoned ancestor A untouched"
            );
        }

        writer.shutdown();
    }

    /// The short-circuit compares the FULL row: an EPOCH-only difference (sizes
    /// identical) must NOT short-circuit — the walk must keep going so coverage
    /// changes propagate.
    #[test]
    fn repair_keeps_walking_on_epoch_only_difference() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → A(10) → B(20). B is an empty, listed, complete dir at epoch 9;
        // A and ROOT are complete at 9 too. Sizes are all zero (empty dirs).
        let entries = vec![dir_entry(10, ROOT_ID, "A"), dir_entry(20, 10, "B")];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10, 20], 9).unwrap();
            // dir_count: ROOT has A + B in its subtree (2), A has B (1), B empty (0).
            for (id, dir_count) in [(ROOT_ID, 2), (10, 1), (20, 0)] {
                IndexStore::upsert_dir_stats_by_id(
                    &conn,
                    &[DirStatsById {
                        entry_id: id,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: dir_count,
                        recursive_has_symlinks: false,
                        min_subtree_epoch: 9,
                    }],
                )
                .unwrap();
            }
            // Now poison ONLY A's epoch to a wrong value (sizes/counts stay right).
            // A repair from A must detect the epoch-only mismatch, rewrite A back to
            // 9, and keep walking to ROOT (which is unaffected here).
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 0,
                    recursive_physical_size: 0,
                    recursive_file_count: 0,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 3, // wrong; truth is 9
                }],
            )
            .unwrap();

            repair_dir_stats_upward(&conn, 10);

            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(
                a.min_subtree_epoch, 9,
                "epoch-only drift must be repaired, not short-circuited past"
            );
            check_db_consistency(&conn);
        }

        writer.shutdown();
    }

    /// A MISSING `dir_stats` row mid-chain: repair creates it with the exact
    /// missing-child semantics — 0 sizes/counts contribution absorbs the epoch to
    /// 0, symlinks false — while still crediting the surviving children it has.
    #[test]
    fn repair_handles_missing_row_midchain() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → A(10) → B(20, listed) with a file, and C(30, MISSING row).
        let entries = vec![
            dir_entry(10, ROOT_ID, "A"),
            dir_entry(20, 10, "B"),
            file_entry(21, 20, "f", 400),
            dir_entry(30, 10, "C"),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            // B is listed+complete at 5; C never got a dir_stats row (Leak C).
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10, 20], 5).unwrap();
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[DirStatsById {
                    entry_id: 20,
                    recursive_logical_size: 400,
                    recursive_physical_size: 400,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 5,
                }],
            )
            .unwrap();

            repair_dir_stats_upward(&conn, 10);

            // A rolls up B (400/1) + counts B and C as dirs (2). C contributes 0 to
            // sizes/counts (missing row) and absorbs A's epoch to 0.
            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert_eq!(
                (
                    a.recursive_logical_size,
                    a.recursive_file_count,
                    a.recursive_dir_count,
                    a.min_subtree_epoch,
                    a.recursive_has_symlinks,
                ),
                (400, 1, 2, 0, false),
                "missing child C: 0 sizes/counts, epoch absorbed to 0, symlinks false"
            );
        }

        writer.shutdown();
    }

    /// Repair recomputes `has_symlinks` and `min_subtree_epoch` consistently with
    /// the existing walkers: a symlink deep in the subtree flips the ancestor's
    /// flag true, and an unlisted descendant drags the ancestor's epoch to 0.
    #[test]
    fn repair_recomputes_symlinks_and_epoch() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn(&db_path, None).unwrap();

        // ROOT(1) → A(10) → B(20, listed@5) with a symlink child, and
        //                 → C(30, UNLISTED — epoch 0).
        let entries = vec![
            dir_entry(10, ROOT_ID, "A"),
            dir_entry(20, 10, "B"),
            symlink_entry(21, 20, "link"),
            dir_entry(30, 10, "C"),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.flush_blocking().unwrap();
        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, 10, 20], 5).unwrap();
            // B has a symlink child; C is unlisted with a bare (epoch-0) row.
            IndexStore::upsert_dir_stats_by_id(
                &conn,
                &[
                    DirStatsById {
                        entry_id: 20,
                        recursive_logical_size: 0,
                        recursive_physical_size: 0,
                        recursive_file_count: 0,
                        recursive_dir_count: 0,
                        recursive_has_symlinks: true,
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

            repair_dir_stats_upward(&conn, 10);

            let a = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
            assert!(a.recursive_has_symlinks, "A must inherit the symlink from B's subtree");
            assert_eq!(a.min_subtree_epoch, 0, "unlisted C drags A's coverage to 0");
        }

        writer.shutdown();
    }

    /// Volume-agnostic by construction: the same repair on a NON-root (SMB-style)
    /// volume DB. Structurally identical — repair only walks entry ids — but this
    /// pins the claim cheaply.
    #[test]
    fn repair_on_mount_rooted_volume_db() {
        let (db_path, _dir) = setup_db();
        let writer = IndexWriter::spawn_for(&db_path, None, false, "smb-share-42".to_string()).unwrap();

        // ROOT(1) → share(10) → sub(20) → f(21, 900)
        let entries = vec![
            dir_entry(10, ROOT_ID, "share"),
            dir_entry(20, 10, "sub"),
            file_entry(21, 20, "f", 900),
        ];
        writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
        writer.send(WriteMessage::ComputeAllAggregates).unwrap();
        writer.flush_blocking().unwrap();

        drift_dir_stats_low(&db_path, 10, 100, 0, 0);

        {
            let conn = IndexStore::open_write_connection(&db_path).unwrap();
            repair_dir_stats_upward(&conn, 10);
            check_db_consistency(&conn);
        }

        writer.shutdown();
    }

    // ── test helpers ─────────────────────────────────────────────────

    fn dir_entry(id: i64, parent_id: i64, name: &str) -> EntryRow {
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

    fn file_entry(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
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

    fn symlink_entry(id: i64, parent_id: i64, name: &str) -> EntryRow {
        EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: false,
            is_symlink: true,
            logical_size: Some(0),
            physical_size: Some(0),
            modified_at: None,
            inode: None,
        }
    }

    /// Overwrite a dir's `dir_stats` sizes/counts LOW to simulate leaked ancestor
    /// credits, preserving symlink/epoch fields as a clean baseline.
    fn drift_dir_stats_low(db_path: &std::path::Path, entry_id: i64, logical: u64, files: u64, dirs: u64) {
        let conn = IndexStore::open_write_connection(db_path).expect("open write conn");
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id,
                recursive_logical_size: logical,
                recursive_physical_size: logical,
                recursive_file_count: files,
                recursive_dir_count: dirs,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .expect("drift upsert");
    }

    fn delete_dir_stats_row(db_path: &std::path::Path, entry_id: i64) {
        let conn = IndexStore::open_write_connection(db_path).expect("open write conn");
        conn.execute("DELETE FROM dir_stats WHERE entry_id = ?1", rusqlite::params![entry_id])
            .expect("delete dir_stats row");
    }
}
