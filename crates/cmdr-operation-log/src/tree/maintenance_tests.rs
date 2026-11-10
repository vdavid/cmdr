//! The summary stays exact as the journal moves: batches, outcome changes, finalize,
//! prune, and a summary write that fails. Plus the reconcile's summary shortcut and
//! the status the tree view reads.

use super::status::{BackfillProgress, BackfillRegistry, TreeSummaryStatus, UnbuiltReason, read_tree_summary_status};
use super::summary::read_tree_dirs;
use super::test_support::{Journal, created_dir, item, journal};
use crate::rollback::reconcile_rolling_back_on_open;
use crate::store::{read_operation, read_operation_items};
use crate::types::{EntryType, ItemOutcome, OpKind, RollbackState, RowRole, SkipReason};
use crate::writer::{JournalItem, PruneRequest};

fn copied_file(seq: i64, dir: &str, name: &str, size: i64) -> JournalItem {
    let src = format!("/src{}", dir.trim_start_matches("/dst"));
    item(
        seq,
        EntryType::File,
        RowRole::RollbackUnit,
        ("v", &src, name),
        Some(("v", dir, name)),
        Some(size),
    )
}

/// A folder copy, journaled in two batches the way the capture layer flushes:
/// `/dst/A/{a,b}`, `/dst/A/B/c`, then the created folders.
fn record_copy(j: &Journal, op_id: &str) {
    j.open(op_id, OpKind::Copy);
    j.record(
        op_id,
        vec![copied_file(0, "/dst/A", "a", 1), copied_file(1, "/dst/A", "b", 2)],
    );
    j.record(
        op_id,
        vec![
            copied_file(2, "/dst/A/B", "c", 4),
            created_dir(3, ("v", "/dst", "A")),
            created_dir(4, ("v", "/dst/A", "B")),
        ],
    );
}

/// Run `statement` on a connection of the test's own, once the writer has drained
/// (a `record_items` is fire-and-forget, so its batch may still hold the write lock).
fn sql(j: &Journal, statement: &str) {
    j.writer.flush_blocking().expect("flush");
    let conn = cmdr_fs::sqlite_util::open(&j.path).expect("write connection");
    conn.execute_batch(&format!("PRAGMA busy_timeout = 5000; {statement}"))
        .expect("statement");
}

#[test]
fn every_kind_of_batch_keeps_the_summary_equal_to_the_aggregate() {
    let j = journal();
    record_copy(&j, "copy");
    j.assert_exact("copy");

    j.open("trash", OpKind::Trash);
    j.record(
        "trash",
        vec![
            item(
                0,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("v", "/home", "A"),
                Some(("v", "/.Trash", "A")),
                None,
            ),
            item(
                1,
                EntryType::File,
                RowRole::SearchOnly,
                ("v", "/home/A", "p"),
                Some(("v", "/.Trash/A", "p")),
                Some(3),
            ),
        ],
    );
    j.assert_exact("trash");

    j.open("delete", OpKind::Delete);
    j.record(
        "delete",
        (0..5)
            .map(|seq| {
                item(
                    seq,
                    EntryType::File,
                    RowRole::RollbackUnit,
                    ("v", "/gone", "f"),
                    None,
                    Some(seq),
                )
            })
            .collect(),
    );
    j.assert_exact("delete");
    assert!(
        read_tree_dirs(&j.read(), "delete")
            .expect("rows")
            .iter()
            .all(|row| row.totals.units_selectable == 0),
        "a permanent delete has nothing selectable"
    );
}

#[test]
fn reversing_holding_back_and_finishing_keep_the_summary_exact() {
    let j = journal();
    record_copy(&j, "op");
    // A first pass: one file back, one held back by drift, the folders held back.
    j.outcomes(
        "op",
        &[
            (2, ItemOutcome::RolledBack, None),
            (1, ItemOutcome::Skipped, Some(SkipReason::Drift)),
            (4, ItemOutcome::RolledBack, None),
            (3, ItemOutcome::Skipped, Some(SkipReason::DirNotEmpty)),
        ],
    );
    j.assert_exact("op");
    // Roll back the rest: the held-back file comes back, then the folder.
    j.outcomes("op", &[(1, ItemOutcome::RolledBack, None)]);
    j.outcomes("op", &[(3, ItemOutcome::RolledBack, None)]);
    j.assert_exact("op");
    let root = read_tree_dirs(&j.read(), "op")
        .expect("rows")
        .into_iter()
        .find(|row| row.parent_dir_id.is_none())
        .expect("a root");
    assert_eq!(root.totals.units_selectable, 1, "only `a` still waits");
    assert_eq!(root.totals.units_rolled_back, 4);
    assert_eq!(root.totals.units_held_back, 0);
    assert_eq!(root.totals.housekeeping_waiting, 0);
}

#[test]
fn every_outcome_batch_bumps_the_generation_and_inserts_dont() {
    let j = journal();
    record_copy(&j, "op");
    let generation = || -> i64 {
        j.read()
            .query_row(
                "SELECT outcome_generation FROM operations WHERE op_id = 'op'",
                [],
                |row| row.get(0),
            )
            .expect("op")
    };
    assert_eq!(generation(), 0);
    j.outcomes("op", &[(0, ItemOutcome::RolledBack, None)]);
    j.outcomes("op", &[(1, ItemOutcome::RolledBack, None)]);
    assert_eq!(generation(), 2);
    j.record("op", vec![copied_file(9, "/dst/A", "late", 1)]);
    assert_eq!(generation(), 2);
}

#[test]
fn finalizing_not_rollbackable_leaves_the_summary_unchanged() {
    let j = journal();
    record_copy(&j, "op");
    let before = j.golden("op");
    j.finalize("op", RollbackState::NotRollbackable, 5);
    assert_eq!(j.golden("op"), before, "every column is a pure function of the rows");
    j.assert_exact("op");
}

/// The writer drops a row it can't insert and keeps going; the summary counts only
/// the rows that landed.
#[test]
fn a_dropped_row_never_counts() {
    let j = journal();
    j.open("op", OpKind::Copy);
    sql(
        &j,
        "CREATE TRIGGER drop_one BEFORE INSERT ON operation_items WHEN NEW.seq = 1 BEGIN SELECT RAISE(ABORT, 'x'); END;",
    );
    j.record(
        "op",
        vec![copied_file(0, "/dst", "a", 1), copied_file(1, "/dst", "b", 2)],
    );
    assert_eq!(read_operation_items(&j.read(), "op", 10).expect("items").len(), 1);
    j.assert_exact("op");
}

#[test]
fn a_summary_write_that_fails_marks_the_op_unbuilt_and_keeps_every_row() {
    let j = journal();
    j.open("op", OpKind::Copy);
    sql(
        &j,
        "CREATE TRIGGER no_summary BEFORE INSERT ON operation_tree_dirs BEGIN SELECT RAISE(ABORT, 'x'); END;",
    );
    j.record(
        "op",
        vec![copied_file(0, "/dst", "a", 1), copied_file(1, "/dst", "b", 2)],
    );
    assert_eq!(
        read_operation_items(&j.read(), "op", 10).expect("items").len(),
        2,
        "no row dropped"
    );
    assert_eq!(j.summary_state("op"), None, "the next tree open rebuilds it");
    assert!(
        read_tree_dirs(&j.read(), "op").expect("rows").is_empty(),
        "its partial write rolled back"
    );

    // From here the writer maintains nothing for it, so nothing half-written appears.
    sql(&j, "DROP TRIGGER no_summary;");
    j.record("op", vec![copied_file(2, "/dst", "c", 4)]);
    j.outcomes("op", &[(0, ItemOutcome::RolledBack, None)]);
    assert_eq!(j.summary_state("op"), None);
    assert!(read_tree_dirs(&j.read(), "op").expect("rows").is_empty());
    let items = read_operation_items(&j.read(), "op", 10).expect("items");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].outcome, ItemOutcome::RolledBack, "the outcome landed anyway");
}

#[test]
fn an_outcome_batch_whose_summary_fails_still_lands_its_outcomes() {
    let j = journal();
    record_copy(&j, "op");
    sql(
        &j,
        "CREATE TRIGGER no_summary BEFORE UPDATE ON operation_tree_dirs BEGIN SELECT RAISE(ABORT, 'x'); END;",
    );
    j.outcomes("op", &[(0, ItemOutcome::RolledBack, None)]);
    assert_eq!(j.summary_state("op"), None);
    let items = read_operation_items(&j.read(), "op", 10).expect("items");
    assert_eq!(items[0].outcome, ItemOutcome::RolledBack);
}

fn prune_older_than(j: &Journal, now: u64, max_age: u64) {
    j.writer
        .prune(PruneRequest {
            max_age_secs: Some(max_age),
            max_size_bytes: None,
            now_secs: now,
            vacuum: false,
        })
        .expect("prune");
}

#[test]
fn prune_takes_an_ops_summary_and_gc_keeps_a_survivors_own_folders() {
    let j = journal();
    record_copy(&j, "old");
    j.finalize("old", RollbackState::Rollbackable, 1);
    // A capped trash: its folder is referenced by nothing but the unit's own folder
    // and its summary rows.
    j.open("kept", OpKind::Trash);
    j.record(
        "kept",
        vec![item(
            0,
            EntryType::Dir,
            RowRole::RollbackUnit,
            ("v", "/home", "Big"),
            Some(("v", "/.Trash", "Big")),
            None,
        )],
    );
    j.finalize("kept", RollbackState::Rollbackable, 995);

    prune_older_than(&j, 1_000, 10);
    let conn = j.read();
    assert!(read_operation(&conn, "old").expect("read").is_none());
    assert!(
        read_tree_dirs(&conn, "old").expect("rows").is_empty(),
        "no orphan summary rows"
    );
    assert_eq!(
        j.self_dir_path("kept", 0),
        "v:/home/Big",
        "the survivor's own folder stays"
    );
    j.assert_exact("kept");
    let dangling: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM operation_tree_dirs t LEFT JOIN dirs d ON d.dir_id = t.dir_id WHERE d.dir_id IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(dangling, 0);

    j.finalize("kept", RollbackState::Rollbackable, 1);
    prune_older_than(&j, 1_000, 10);
    let left: (i64, i64) = j
        .read()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM operation_tree_dirs), (SELECT COUNT(*) FROM dirs)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("counts");
    assert_eq!(left, (0, 0), "an empty journal keeps no summary rows and no dirs");
}

fn reconciled(j: &Journal, op_id: &str) -> RollbackState {
    j.writer
        .set_rollback_state(op_id, RollbackState::RollingBack, None)
        .expect("rolling back");
    reconcile_rolling_back_on_open(&j.writer);
    read_operation(&j.read(), op_id)
        .expect("read")
        .expect("op")
        .rollback_state
}

/// Guards the reconcile's summary clause: a built op answers "did a pass settle any
/// row?" from its root rows, so a summary that says so wins over rows that (here,
/// artificially) don't.
#[test]
fn reconcile_guard_a_built_summary_showing_a_reversal_resolves_partial() {
    let j = journal();
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    assert_eq!(reconciled(&j, "op"), RollbackState::Rollbackable, "nothing reversed");

    sql(
        &j,
        "UPDATE operation_tree_dirs SET units_held_back = 1 WHERE op_id = 'op' AND parent_dir_id IS NULL;",
    );
    assert_eq!(reconciled(&j, "op"), RollbackState::PartiallyRolledBack);
}

/// An unbuilt op has no summary to ask, so its own rows decide.
#[test]
fn reconcile_reads_an_unbuilt_ops_rows() {
    let j = journal();
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    j.outcomes("op", &[(0, ItemOutcome::RolledBack, None)]);
    sql(
        &j,
        "UPDATE operations SET tree_summary = NULL WHERE op_id = 'op'; DELETE FROM operation_tree_dirs;",
    );
    assert_eq!(reconciled(&j, "op"), RollbackState::PartiallyRolledBack);
}

#[test]
fn the_status_says_why_a_summary_cant_be_read_yet() {
    let j = journal();
    let registry = BackfillRegistry::default();
    let status = |op: &str| read_tree_summary_status(&j.read(), op, &registry).expect("status");

    assert_eq!(status("nope"), None);
    record_copy(&j, "op");
    assert_eq!(
        status("op"),
        Some(TreeSummaryStatus::Built),
        "built from its first row, even while running"
    );

    sql(&j, "UPDATE operations SET tree_summary = NULL WHERE op_id = 'op';");
    assert_eq!(
        status("op"),
        Some(TreeSummaryStatus::Unbuilt(UnbuiltReason::OperationRunning))
    );
    j.finalize("op", RollbackState::Rollbackable, 5);
    assert_eq!(status("op"), Some(TreeSummaryStatus::Unbuilt(UnbuiltReason::NotBuilt)));
    let progress = BackfillProgress { done: 2, total: 5 };
    registry.report("op", progress);
    assert_eq!(status("op"), Some(TreeSummaryStatus::Building(progress)));
    j.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("rolling back");
    assert_eq!(
        status("op"),
        Some(TreeSummaryStatus::Unbuilt(UnbuiltReason::RollingBack))
    );
    registry.remove("op");
    assert_eq!(registry.status("op"), None);

    sql(&j, "DROP INDEX operation_items_dest_dir;");
    assert_eq!(
        status("op"),
        Some(TreeSummaryStatus::IndexesBuilding),
        "no tree reads a folder before the writer built its indexes"
    );
}
