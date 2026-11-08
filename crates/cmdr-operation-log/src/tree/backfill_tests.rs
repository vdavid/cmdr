//! The backfill of an op journaled before the tree summary existed: what it builds
//! equals what the writer would have kept, and every race (a cancel, a rollback, a
//! prune, a second ask) ends with an exact summary or none at all.

use std::sync::Arc;

use super::backfill::{TreeBackfill, TreeBackfillEnd, TreeBackfillStart, TreeBackfillStep};
use super::status::{BackfillProgress, BackfillRegistry, UnbuiltReason};
use super::summary::{TreeDirRow, read_op_tree_context, read_tree_dirs};
use super::test_support::{Journal, created_dir, item, journal};
use crate::types::{EntryType, ItemOutcome, OpKind, RollbackState, RowRole, SkipReason};
use crate::writer::{JournalItem, PruneRequest};

fn file(seq: i64, src: (&str, &str), dest: Option<(&str, &str)>, size: i64) -> JournalItem {
    item(
        seq,
        EntryType::File,
        RowRole::RollbackUnit,
        ("v", src.0, src.1),
        dest.map(|(dir, name)| ("v", dir, name)),
        Some(size),
    )
}

/// A folder copy `/src/A` → `/dst/A` with a subfolder and the created folders.
fn record_copy(j: &Journal, op_id: &str) {
    j.open(op_id, OpKind::Copy);
    j.record(
        op_id,
        vec![
            file(0, ("/src/A", "a"), Some(("/dst/A", "a")), 1),
            file(1, ("/src/A/B", "b"), Some(("/dst/A/B", "b")), 2),
            created_dir(2, ("v", "/dst", "A")),
            created_dir(3, ("v", "/dst/A", "B")),
            // An empty created folder: interned only as its own folder.
            created_dir(4, ("v", "/dst/A", "Empty")),
        ],
    );
}

/// A trash of `/home/T` (one folder unit) with a file and a folder leaf, plus a
/// capped folder with no leaves.
fn record_trash(j: &Journal, op_id: &str) {
    j.open(op_id, OpKind::Trash);
    let leaf = |seq, entry_type, dir: &str, name, size| {
        let trashed = dir.replacen("/home", "/.Trash", 1);
        item(
            seq,
            entry_type,
            RowRole::SearchOnly,
            ("v", dir, name),
            Some(("v", &trashed, name)),
            size,
        )
    };
    j.record(
        op_id,
        vec![
            item(
                0,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("v", "/home", "T"),
                Some(("v", "/.Trash", "T")),
                None,
            ),
            leaf(1, EntryType::File, "/home/T", "p", Some(3)),
            leaf(2, EntryType::Dir, "/home/T", "Sub", None),
            leaf(3, EntryType::File, "/home/T/Sub", "q", Some(5)),
            item(
                4,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("v", "/home", "Capped"),
                Some(("v", "/.Trash", "Capped")),
                None,
            ),
        ],
    );
}

/// A same-FS move of `/a/M` to `/b/M`: its unit owns its LANDED folder.
fn record_move(j: &Journal, op_id: &str) {
    j.open(op_id, OpKind::Move);
    j.record(
        op_id,
        vec![
            item(
                0,
                EntryType::Dir,
                RowRole::RollbackUnit,
                ("v", "/a", "M"),
                Some(("v", "/b", "M")),
                None,
            ),
            item(
                1,
                EntryType::File,
                RowRole::SearchOnly,
                ("v", "/a/M", "x"),
                Some(("v", "/b/M", "x")),
                Some(7),
            ),
        ],
    );
}

/// `count` files in one folder of a delete, recorded in batches.
fn record_many(j: &Journal, op_id: &str, count: i64) {
    j.open(op_id, OpKind::Delete);
    for start in (0..count).step_by(1_000) {
        let batch = (start..(start + 1_000).min(count))
            .map(|seq| file(seq, ("/many", &format!("f{seq}")), None, seq))
            .collect();
        j.record(op_id, batch);
    }
}

/// Run `statement` on a connection of the test's own, once the writer has drained.
fn sql(j: &Journal, statement: &str) {
    j.writer.flush_blocking().expect("flush");
    let conn = cmdr_fs::sqlite_util::open(&j.path).expect("write connection");
    conn.execute_batch(&format!("PRAGMA busy_timeout = 5000; {statement}"))
        .expect("statement");
}

/// Turn `op_id` into what a pre-summary journal holds: no summary rows, no own
/// folders, not built. Answers the rows the writer had kept, for comparison.
fn make_pre_summary(j: &Journal, op_id: &str) -> Vec<TreeDirRow> {
    let kept = read_tree_dirs(&j.read(), op_id).expect("summary rows");
    sql(
        j,
        &format!(
            "DELETE FROM operation_tree_dirs WHERE op_id = '{op_id}';
             UPDATE operation_items SET self_dir_id = NULL WHERE op_id = '{op_id}';
             UPDATE operations SET tree_summary = NULL WHERE op_id = '{op_id}';"
        ),
    );
    kept
}

fn start(j: &Journal, registry: &Arc<BackfillRegistry>, op_id: &str) -> (TreeBackfillStart, Option<TreeBackfill>) {
    j.writer.flush_blocking().expect("flush");
    TreeBackfill::start_on(&j.writer, registry, op_id).expect("start")
}

fn started(j: &Journal, registry: &Arc<BackfillRegistry>, op_id: &str) -> TreeBackfill {
    match start(j, registry, op_id) {
        (TreeBackfillStart::Started, Some(job)) => job,
        (other, _) => panic!("{op_id}: expected a fresh backfill, got {other:?}"),
    }
}

/// Step `job` to its end, answering it and every progress it reported.
fn drive(job: &mut TreeBackfill) -> (TreeBackfillEnd, Vec<BackfillProgress>) {
    let mut progress = Vec::new();
    loop {
        match job.step() {
            TreeBackfillStep::Progress(p) => progress.push(p),
            TreeBackfillStep::Finished(end) => return (end, progress),
        }
    }
}

#[test]
fn a_backfill_builds_exactly_what_the_writer_would_have_kept() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    for (op_id, record) in [
        ("copy", record_copy as fn(&Journal, &str)),
        ("trash", record_trash),
        ("move", record_move),
    ] {
        record(&j, op_id);
        j.finalize(op_id, RollbackState::Rollbackable, 5);
        let kept = make_pre_summary(&j, op_id);
        assert!(!kept.is_empty());

        let (end, progress) = drive(&mut started(&j, &registry, op_id));
        assert_eq!(end, TreeBackfillEnd::Built, "{op_id}");
        let last = progress.last().expect("progress");
        assert!(last.total > 0 && last.done == last.total, "{op_id}: read every row");
        assert_eq!(read_tree_dirs(&j.read(), op_id).expect("rows"), kept, "{op_id}");
        j.assert_exact(op_id);
        assert_eq!(registry.status(op_id), None, "{op_id}: the claim is gone");
    }
}

/// Pre-summary DIR rows, `search_only` folders included, get their AXIS-side own
/// folder: the original for a trash, the landed one for a move.
#[test]
fn pre_summary_folder_rows_get_their_axis_side_own_folder() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_trash(&j, "trash");
    record_move(&j, "move");
    for op_id in ["trash", "move"] {
        j.finalize(op_id, RollbackState::Rollbackable, 5);
        make_pre_summary(&j, op_id);
        drive(&mut started(&j, &registry, op_id));
    }
    assert_eq!(j.self_dir_path("trash", 0), "v:/home/T");
    assert_eq!(j.self_dir_path("trash", 2), "v:/home/T/Sub", "a search-only folder too");
    assert_eq!(j.self_dir_path("trash", 4), "v:/home/Capped");
    assert_eq!(j.self_dir_path("move", 0), "v:/b/M");
}

/// A folder row whose own folder won't intern leaves the op unbuilt, as the writer's
/// insert does. Regression: the backfill built anyway, counting that unit in its
/// parent's "N of M" with no row of its own to see or uncheck.
#[test]
fn a_folder_row_that_cant_get_its_own_folder_leaves_the_op_unbuilt() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_move(&j, "move");
    j.finalize("move", RollbackState::Rollbackable, 5);
    make_pre_summary(&j, "move");
    // Stands in for the DB error that makes `intern_child_dir` fail.
    sql(
        &j,
        "CREATE TRIGGER refuse_own_folder BEFORE UPDATE OF self_dir_id ON operation_items \
         BEGIN SELECT RAISE(ABORT, 'refused'); END;",
    );

    let (end, _) = drive(&mut started(&j, &registry, "move"));

    assert_eq!(end, TreeBackfillEnd::Unreadable);
    let context = read_op_tree_context(&j.read(), "move").expect("context").expect("op");
    assert!(!context.built, "nothing is stamped, so the next open retries");
}

#[test]
fn a_cancelled_backfill_stamps_nothing_and_a_rerun_completes() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_many(&j, "op", 5_000);
    j.finalize("op", RollbackState::NotRollbackable, 5);
    let kept = make_pre_summary(&j, "op");

    let mut job = started(&j, &registry, "op");
    assert!(matches!(
        job.step(),
        TreeBackfillStep::Progress(BackfillProgress { done: 0, total: 5_000 })
    ));
    assert!(matches!(
        job.step(),
        TreeBackfillStep::Progress(BackfillProgress { done: 4_096, .. })
    ));
    assert_eq!(
        registry.status("op"),
        Some(BackfillProgress {
            done: 4_096,
            total: 5_000
        })
    );
    registry.cancel("op");
    assert_eq!(job.step(), TreeBackfillStep::Finished(TreeBackfillEnd::Cancelled));
    assert_eq!(
        job.step(),
        TreeBackfillStep::Finished(TreeBackfillEnd::Cancelled),
        "it stays over"
    );
    assert_eq!(j.summary_state("op"), None, "nothing stamped");
    assert!(read_tree_dirs(&j.read(), "op").expect("rows").is_empty());
    assert_eq!(registry.status("op"), None);

    let (end, _) = drive(&mut started(&j, &registry, "op"));
    assert_eq!(end, TreeBackfillEnd::Built);
    assert_eq!(read_tree_dirs(&j.read(), "op").expect("rows"), kept);
}

/// One backfill per op: a second ask joins it, and a join takes back a cancel the
/// dialog asked for as it closed, so reopening it doesn't lose the work.
#[test]
fn a_second_ask_joins_and_takes_back_a_cancel() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    make_pre_summary(&j, "op");

    let mut job = started(&j, &registry, "op");
    job.step();
    registry.cancel("op");
    assert!(matches!(start(&j, &registry, "op"), (TreeBackfillStart::Joined, None)));
    assert_eq!(drive(&mut job).0, TreeBackfillEnd::Built);
    assert!(
        matches!(start(&j, &registry, "op"), (TreeBackfillStart::Built, None)),
        "never twice"
    );
}

/// A job dropped mid-way (its host task stopped) frees its op.
#[test]
fn a_dropped_job_frees_its_op() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    make_pre_summary(&j, "op");
    drop(started(&j, &registry, "op"));
    assert_eq!(registry.status("op"), None);
    drive(&mut started(&j, &registry, "op"));
    j.assert_exact("op");
}

/// A rollback pass that lands outcomes while the backfill reads makes its result
/// stale: the writer refuses it, the job reads again, and the summary it writes
/// counts the new outcomes.
#[test]
fn outcomes_that_land_mid_backfill_make_it_start_over() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    make_pre_summary(&j, "op");

    let mut job = started(&j, &registry, "op");
    job.step();
    job.step();
    j.outcomes(
        "op",
        &[
            (0, ItemOutcome::RolledBack, None),
            (1, ItemOutcome::Skipped, Some(SkipReason::Drift)),
        ],
    );
    let (end, progress) = drive(&mut job);
    assert_eq!(end, TreeBackfillEnd::Built);
    assert!(
        progress.contains(&BackfillProgress { done: 0, total: 5 }),
        "it read again"
    );
    j.assert_exact("op");
    let root = read_tree_dirs(&j.read(), "op")
        .expect("rows")
        .into_iter()
        .find(|row| row.parent_dir_id.is_none())
        .expect("a root");
    assert_eq!((root.totals.units_rolled_back, root.totals.units_held_back), (1, 1));
}

/// A rollback that STARTED while the backfill read stops it at the restart: the tree
/// says why, and nothing is stamped.
#[test]
fn a_rollback_that_starts_mid_backfill_stops_it() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    make_pre_summary(&j, "op");

    let mut job = started(&j, &registry, "op");
    job.step();
    j.writer
        .set_rollback_state("op", RollbackState::RollingBack, None)
        .expect("rolling back");
    j.outcomes("op", &[(0, ItemOutcome::RolledBack, None)]);
    assert_eq!(drive(&mut job).0, TreeBackfillEnd::Unbuilt(UnbuiltReason::RollingBack));
    assert_eq!(j.summary_state("op"), None);
    assert!(matches!(
        start(&j, &registry, "op"),
        (TreeBackfillStart::Unbuilt(UnbuiltReason::RollingBack), None)
    ));
}

/// Retention prunes the op while the backfill reads: nothing is written, so no
/// summary row (and no dir it would keep alive) outlives the op.
#[test]
fn a_prune_mid_backfill_leaves_no_summary_and_no_dirs() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_trash(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 1);
    make_pre_summary(&j, "op");

    let mut job = started(&j, &registry, "op");
    job.step();
    job.step(); // The chunk: own folders interned by the writer.
    j.writer
        .prune(PruneRequest {
            max_age_secs: Some(10),
            max_size_bytes: None,
            now_secs: 1_000,
            vacuum: false,
        })
        .expect("prune");
    j.writer.flush_blocking().expect("flush");
    assert_eq!(drive(&mut job).0, TreeBackfillEnd::Gone);
    let left: (i64, i64) = j
        .read()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM operation_tree_dirs), (SELECT COUNT(*) FROM dirs)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("counts");
    assert_eq!(left, (0, 0));
    assert!(matches!(
        start(&j, &registry, "op"),
        (TreeBackfillStart::UnknownOperation, None)
    ));
}

/// A v6 op whose summary write failed mid-copy, then took more rows: while it runs
/// no backfill can start (inserts don't bump the generation), and once it finishes
/// the backfill makes it exact.
#[test]
fn an_op_whose_summary_broke_mid_copy_backfills_once_it_finishes() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    j.open("op", OpKind::Copy);
    sql(
        &j,
        "CREATE TRIGGER no_summary BEFORE INSERT ON operation_tree_dirs BEGIN SELECT RAISE(ABORT, 'x'); END;",
    );
    j.record("op", vec![file(0, ("/src", "a"), Some(("/dst", "a")), 1)]);
    sql(&j, "DROP TRIGGER no_summary;");
    j.record(
        "op",
        vec![
            file(1, ("/src/S", "b"), Some(("/dst/S", "b")), 2),
            created_dir(2, ("v", "/dst", "S")),
        ],
    );
    assert_eq!(j.summary_state("op"), None);
    assert!(matches!(
        start(&j, &registry, "op"),
        (TreeBackfillStart::Unbuilt(UnbuiltReason::OperationRunning), None)
    ));

    j.finalize("op", RollbackState::Rollbackable, 5);
    assert_eq!(drive(&mut started(&j, &registry, "op")).0, TreeBackfillEnd::Built);
    j.assert_exact("op");
    assert_eq!(j.self_dir_path("op", 2), "v:/dst/S");
}

/// An op a crash left running (its summary broken mid-copy, so it needs a backfill)
/// backfills once the next launch has ended it.
#[test]
fn an_op_a_crash_left_running_backfills_once_the_next_launch_ends_it() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    j.open("op", OpKind::Copy);
    sql(
        &j,
        "CREATE TRIGGER no_summary BEFORE INSERT ON operation_tree_dirs BEGIN SELECT RAISE(ABORT, 'x'); END;",
    );
    j.record("op", vec![file(0, ("/src", "a"), Some(("/dst", "a")), 1)]);
    sql(&j, "DROP TRIGGER no_summary;");
    assert!(matches!(
        start(&j, &registry, "op"),
        (TreeBackfillStart::Unbuilt(UnbuiltReason::OperationRunning), None)
    ));

    crate::interrupted::end_interrupted_on_open(&j.writer);

    assert_eq!(drive(&mut started(&j, &registry, "op")).0, TreeBackfillEnd::Built);
    j.assert_exact("op");
}

/// The journal's folder indexes still building (the first launch after the upgrade)
/// don't hold a backfill back: it reads by `(op_id, seq)`, and its write queues
/// behind their build on the writer. A built op still answers built meanwhile.
#[test]
fn a_backfill_runs_while_the_folder_indexes_are_still_building() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_copy(&j, "old");
    record_copy(&j, "new");
    j.finalize("old", RollbackState::Rollbackable, 5);
    j.finalize("new", RollbackState::Rollbackable, 5);
    let kept = make_pre_summary(&j, "old");
    sql(&j, "DROP INDEX operation_items_source_dir;");

    assert!(matches!(start(&j, &registry, "new"), (TreeBackfillStart::Built, None)));
    assert_eq!(drive(&mut started(&j, &registry, "old")).0, TreeBackfillEnd::Built);
    assert_eq!(read_tree_dirs(&j.read(), "old").expect("rows"), kept);
}

#[test]
fn an_unknown_op_and_a_built_one_need_no_backfill() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_copy(&j, "op");
    j.finalize("op", RollbackState::Rollbackable, 5);
    assert!(matches!(
        start(&j, &registry, "nope"),
        (TreeBackfillStart::UnknownOperation, None)
    ));
    assert!(matches!(start(&j, &registry, "op"), (TreeBackfillStart::Built, None)));
}

/// The timing bar: a 20,000-row op, seeded and backfilled, runs well inside
/// `docs/testing.md`'s two seconds in the dev profile (0.77 s whole test on Apple
/// Silicon with the crate's suite in parallel, 2026-11-07); its duration is the
/// measurement, so it asserts no clock of its own.
#[test]
fn a_twenty_thousand_row_backfill_stays_quick() {
    let j = journal();
    let registry = Arc::new(BackfillRegistry::default());
    record_many(&j, "op", 20_000);
    j.finalize("op", RollbackState::NotRollbackable, 5);
    let kept = make_pre_summary(&j, "op");
    let (end, progress) = drive(&mut started(&j, &registry, "op"));
    assert_eq!(end, TreeBackfillEnd::Built);
    assert_eq!(progress.len(), 6, "a start and five chunks");
    assert_eq!(read_tree_dirs(&j.read(), "op").expect("rows"), kept);
}
