use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU8;
use uuid::Uuid;

use super::super::super::operation_intent::OperationIntent;
use crate::test_support::TestDir;

/// A fixture directory in `$TMPDIR`.
///
/// ❌ Never move these fixtures under the repo tree. `rust-tests-linux` runs the suite in
/// Docker with the repo bind-mounted from the macOS host, so a fixture there sits on
/// case-INSENSITIVE APFS *even inside Linux*, while `normalize_for_comparison` (which
/// picks the case-only rename strategy) is compiled for Linux and treats the filesystem
/// as case-sensitive. The two disagree, and `screenshot.png` → `Screenshot.png` reports
/// `Skipped` because the destination "already exists" — while every content assertion
/// still passes, since a case-insensitive lookup finds the file. That reads as a mystery
/// failure reproducing only in the container. `$TMPDIR` is container-local, so the
/// fixture's case sensitivity matches the OS the code was compiled for.
fn create_test_dir(name: &str) -> TestDir {
    TestDir::new(&format!("bulk_rename_test_{name}_{}", Uuid::new_v4()))
}

fn local_row(row_id: &str, source: PathBuf, destination: PathBuf) -> BulkRenameRow {
    let expected_fingerprint = SourceFingerprint::capture_local(&source).expect("fingerprint fixture source");
    BulkRenameRow {
        row_id: row_id.to_string(),
        source,
        destination,
        expected_fingerprint,
    }
}

/// A recorder for tests that don't assert on the journal. Journaling no-ops when no
/// journal is installed, so this is inert unless a `TestJournalGuard` is in scope.
fn test_recorder() -> BulkRenameRecorder {
    BulkRenameRecorder::new("op-test".to_string(), "root".to_string())
}

fn assert_no_staging_paths(dir: &Path) {
    let staging_paths: Vec<_> = fs::read_dir(dir)
        .expect("read fixture directory")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".cmdr-bulk-rename-"))
        .collect();
    assert!(staging_paths.is_empty(), "unexpected staging paths: {staging_paths:?}");
}

fn planning_row(source: &str, destination: &str) -> BulkRenameRow {
    BulkRenameRow {
        row_id: source.to_string(),
        source: PathBuf::from(source),
        destination: PathBuf::from(destination),
        expected_fingerprint: SourceFingerprint::Local {
            device: 0,
            inode: 0,
            content: super::super::super::source_binding::LocalContent::File {
                size: 0,
                modified_nanos: None,
            },
        },
    }
}

/// An applied rename must journal the mtime undo verifies on. Recording `None`
/// here left `verify_snapshot` comparing size and nothing else, so a same-size
/// replacement file was renamed back in place of the original.
#[test]
fn an_applied_bulk_rename_journals_the_mtime_undo_verifies_on() {
    use crate::operation_log::TestJournalGuard;
    use crate::operation_log::capture::WriterJournal;
    use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation_items};
    use crate::operation_log::writer::OperationLogWriter;

    let journal_dir = tempfile::tempdir().expect("journal tempdir");
    let db = operation_log_db_path(journal_dir.path());
    let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
    let _journal = TestJournalGuard::install(Arc::new(WriterJournal::new(writer)));

    let tmp = create_test_dir("journaled_mtime");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    let row = local_row("journaled", source.clone(), destination.clone());
    let live = crate::file_system::listing::get_single_entry(&source).expect("read the live entry");

    let op_id = "op-bulk-rename-mtime";
    super::super::super::journal::open_local_op(op_id, OpKind::Rename, Initiator::User, 1, Some("root"));
    BulkRenameRecorder::new(op_id.to_string(), "root".to_string()).record_hop(&row, &source, &destination);
    super::super::super::journal::finalize_op(op_id, OpKind::Rename, ExecutionStatus::Done);

    let conn = open_read_connection(&db).expect("read conn");
    let items = read_operation_items(&conn, op_id, 10).expect("items");
    assert_eq!(items.len(), 1, "one row per applied rename");
    assert_eq!(
        items[0].mtime,
        live.modified_at.map(|secs| secs as i64),
        "the journaled mtime must be the one undo rechecks against"
    );
    assert_eq!(items[0].size, live.size.map(|size| size as i64));
    let _ = fs::remove_dir_all(&tmp);
}

/// The full loop for one applied batch rename: a real local rename, journaled by
/// the capture layer, then reversed by the real rollback engine over the rows
/// capture actually wrote. Hand-seeded journal rows (as `rollback/tests.rs` uses)
/// can't catch a capture-side snapshot defect, so this bed exists to.
struct UndoLoop {
    _journal: crate::operation_log::TestJournalGuard,
    writer_journal: Arc<crate::operation_log::capture::WriterJournal>,
    vm: crate::file_system::VolumeManager,
    /// Owns the scratch directory: holding the guard (rather than a bare path
    /// copied out of it) is what keeps the directory alive for the fixture's
    /// lifetime and removes it afterwards. Same for `_journal_dir`, which holds
    /// the operation-log database the writer above reads and writes throughout.
    dir: TestDir,
    _journal_dir: TestDir,
    op_id: String,
}

impl UndoLoop {
    fn new(name: &str) -> Self {
        use crate::file_system::volume::{LocalPosixVolume, Volume};
        use crate::operation_log::capture::WriterJournal;
        use crate::operation_log::store::operation_log_db_path;
        use crate::operation_log::writer::OperationLogWriter;

        // Held in the struct below, not just here: the writer keeps using this
        // database for the fixture's whole life, so dropping the handle at the end
        // of `new` would delete `operation-log.db` out from under it.
        let journal_dir = create_test_dir(&format!("{name}_journal"));
        let writer = OperationLogWriter::spawn(&operation_log_db_path(&journal_dir)).expect("spawn writer");
        let writer_journal = Arc::new(WriterJournal::new(writer));
        let journal = crate::operation_log::TestJournalGuard::install(writer_journal.clone());

        let vm = crate::file_system::VolumeManager::new();
        // Rooted at `/` so the fixture's absolute paths resolve, matching how the
        // real `root` volume is registered.
        vm.register(
            "root",
            Arc::new(LocalPosixVolume::new("Test root", "/")) as Arc<dyn Volume>,
        );
        UndoLoop {
            _journal: journal,
            writer_journal,
            vm,
            dir: create_test_dir(name),
            _journal_dir: journal_dir,
            op_id: format!("op-{name}"),
        }
    }

    /// Apply `source` → `destination` for real and journal it exactly as the
    /// managed driver does.
    fn apply(&self, source: &Path, destination: &Path) {
        let row = local_row("undo", source.to_path_buf(), destination.to_path_buf());
        // Open BEFORE the run: the run journals each landing as it happens, so the
        // op has to exist first, exactly as the managed driver arranges it.
        super::super::super::journal::open_local_op(&self.op_id, OpKind::Rename, Initiator::User, 1, Some("root"));
        let run = bulk_rename_local(
            std::slice::from_ref(&row),
            &AtomicU8::new(OperationIntent::Running as u8),
            &BulkRenameRecorder::new(self.op_id.clone(), "root".to_string()),
        );
        assert_eq!(
            run.outcomes,
            vec![BulkRenameOutcome::Done],
            "the fixture rename applied"
        );
        super::super::super::journal::finalize_op(&self.op_id, OpKind::Rename, ExecutionStatus::Done);
    }

    async fn undo(&self) -> crate::operation_log::rollback::RollbackReport {
        use crate::operation_log::store::{open_read_connection, read_operation};
        let writer = self.writer_journal.writer();
        let conn = open_read_connection(writer.db_path()).expect("read conn");
        let original = read_operation(&conn, &self.op_id)
            .expect("read op")
            .expect("the applied batch is journaled");
        assert_eq!(
            original.rollback_state,
            crate::operation_log::types::RollbackState::Rollbackable,
            "an applied batch rename must be rollbackable"
        );
        drop(conn);
        crate::operation_log::rollback::execute_rollback(
            &self.vm,
            writer,
            &original,
            "inv-undo",
            Initiator::User,
            &|| false,
        )
        .await
    }
}

impl Drop for UndoLoop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Set a file's mtime to a fixed, distant second, so "the mtime differs" doesn't
/// depend on the test running across a second boundary.
fn pin_mtime(path: &Path, unix_seconds: u64) {
    let file = fs::File::options().write(true).open(path).expect("open to set mtime");
    file.set_modified(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(unix_seconds))
        .expect("set mtime");
}

/// The half that's easy to break while fixing the other half: undo of an
/// untouched batch must still restore. A snapshot recorded in the wrong unit
/// would land here as drift, silently disabling undo.
#[tokio::test]
async fn undo_of_an_untouched_batch_rename_restores_the_original_name() {
    let undo_loop = UndoLoop::new("undo_restores");
    let source = undo_loop.dir.join("before.txt");
    let destination = undo_loop.dir.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    undo_loop.apply(&source, &destination);

    let report = undo_loop.undo().await;

    assert_eq!(report.reversed, 1, "an untouched rename reverses");
    assert_eq!(report.skipped, 0);
    assert_eq!(
        report.final_state,
        crate::operation_log::types::RollbackState::RolledBack
    );
    assert_eq!(fs::read_to_string(&source).expect("read restored source"), "reviewed");
    assert!(!destination.exists(), "the renamed name is released");
}

/// The defect this milestone fixes: with no mtime journaled, identity rested on
/// size alone, so a same-size replacement file was renamed back in place of the
/// original — data loss by undo.
#[tokio::test]
async fn undo_refuses_a_same_size_replacement_instead_of_renaming_it_back() {
    let undo_loop = UndoLoop::new("undo_drift");
    let source = undo_loop.dir.join("before.txt");
    let destination = undo_loop.dir.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    undo_loop.apply(&source, &destination);
    // A DIFFERENT file, byte-for-byte the same length, at the renamed name.
    fs::write(&destination, "replaced").expect("replace the renamed file");
    pin_mtime(&destination, 1_700_000_000);

    let report = undo_loop.undo().await;

    assert_eq!(report.reversed, 0, "a drifted target is never touched");
    assert_eq!(report.skipped, 1);
    assert_eq!(
        report.final_state,
        crate::operation_log::types::RollbackState::PartiallyRolledBack,
        "a skip is reported as a partial undo, never as a clean one"
    );
    assert_eq!(
        fs::read_to_string(&destination).expect("read the replacement"),
        "replaced",
        "the replacement stays where the user put it"
    );
    assert!(
        !source.exists(),
        "the impostor is NOT renamed back to the original name"
    );
}

#[test]
fn execution_plan_renames_independent_rows_directly_without_temporaries() {
    let rows = vec![planning_row("a", "renamed-a"), planning_row("b", "renamed-b")];

    assert_eq!(
        build_execution_plan(&rows, &[true, true]),
        vec![RenamePlanStep::Direct(0), RenamePlanStep::Direct(1)]
    );
}

#[test]
fn execution_plan_orders_chains_from_the_free_destination_without_temporaries() {
    let rows = vec![planning_row("a", "b"), planning_row("b", "c"), planning_row("c", "d")];

    assert_eq!(
        build_execution_plan(&rows, &[true, true, true]),
        vec![
            RenamePlanStep::Direct(2),
            RenamePlanStep::Direct(1),
            RenamePlanStep::Direct(0),
        ]
    );
}

#[test]
fn execution_plan_uses_one_temporary_step_per_cycle() {
    let rows = vec![
        planning_row("a", "b"),
        planning_row("b", "c"),
        planning_row("c", "a"),
        planning_row("x", "y"),
        planning_row("y", "x"),
    ];

    assert_eq!(
        build_execution_plan(&rows, &[true, true, true, true, true]),
        vec![RenamePlanStep::Cycle(vec![0, 1, 2]), RenamePlanStep::Cycle(vec![3, 4]),]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn execution_plan_keeps_a_temporary_step_for_case_only_renames() {
    let rows = vec![planning_row("screenshot.png", "Screenshot.png")];

    assert_eq!(build_execution_plan(&rows, &[true]), vec![RenamePlanStep::CaseOnly(0)]);
}

#[test]
fn bulk_local_rename_preserves_chains_and_cycles() {
    let tmp = create_test_dir("chain_cycle");
    let chain_a = tmp.join("chain-a.txt");
    let chain_b = tmp.join("chain-b.txt");
    let chain_c = tmp.join("chain-c.txt");
    let chain_d = tmp.join("chain-d.txt");
    let cycle_a = tmp.join("cycle-a.txt");
    let cycle_b = tmp.join("cycle-b.txt");
    let cycle_c = tmp.join("cycle-c.txt");
    for (path, contents) in [
        (&chain_a, "chain a"),
        (&chain_b, "chain b"),
        (&chain_c, "chain c"),
        (&cycle_a, "cycle a"),
        (&cycle_b, "cycle b"),
        (&cycle_c, "cycle c"),
    ] {
        fs::write(path, contents).expect("write fixture");
    }

    let rows = vec![
        local_row("chain-a", chain_a.clone(), chain_b.clone()),
        local_row("chain-b", chain_b.clone(), chain_c.clone()),
        local_row("chain-c", chain_c.clone(), chain_d.clone()),
        local_row("cycle-a", cycle_a.clone(), cycle_b.clone()),
        local_row("cycle-b", cycle_b.clone(), cycle_c.clone()),
        local_row("cycle-c", cycle_c.clone(), cycle_a.clone()),
    ];

    let run = bulk_rename_local(&rows, &AtomicU8::new(OperationIntent::Running as u8), &test_recorder());

    assert_eq!(fs::read_to_string(&chain_b).expect("read chain b"), "chain a");
    assert_eq!(fs::read_to_string(&chain_c).expect("read chain c"), "chain b");
    assert_eq!(fs::read_to_string(&chain_d).expect("read chain d"), "chain c");
    assert_eq!(fs::read_to_string(&cycle_a).expect("read cycle a"), "cycle c");
    assert_eq!(fs::read_to_string(&cycle_b).expect("read cycle b"), "cycle a");
    assert_eq!(fs::read_to_string(&cycle_c).expect("read cycle c"), "cycle b");
    assert!(
        run.outcomes.iter().all(|outcome| outcome.is_done()),
        "unexpected outcomes: {:?}",
        run.outcomes
    );
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_preserves_swaps_and_case_only_names() {
    let tmp = create_test_dir("swap_case_only");
    let first = tmp.join("first.txt");
    let second = tmp.join("second.txt");
    let case_source = tmp.join("screenshot.png");
    fs::write(&first, "first").expect("write first fixture");
    fs::write(&second, "second").expect("write second fixture");
    fs::write(&case_source, "image").expect("write case fixture");

    let rows = vec![
        local_row("first", first.clone(), second.clone()),
        local_row("second", second.clone(), first.clone()),
        local_row("case", case_source.clone(), tmp.join("Screenshot.png")),
    ];

    let run = bulk_rename_local(&rows, &AtomicU8::new(OperationIntent::Running as u8), &test_recorder());

    assert_eq!(fs::read_to_string(&first).expect("read swapped first"), "second");
    assert_eq!(fs::read_to_string(&second).expect("read swapped second"), "first");
    assert_eq!(
        fs::read_to_string(tmp.join("Screenshot.png")).expect("read case-only rename"),
        "image"
    );
    assert!(
        run.outcomes.iter().all(|outcome| outcome.is_done()),
        "unexpected outcomes: {:?}",
        run.outcomes
    );
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_skips_a_source_that_changed_after_preflight() {
    let tmp = create_test_dir("changed_source");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    let row = local_row("changed", source.clone(), destination.clone());
    fs::write(&source, "changed after review").expect("change fixture after fingerprint");

    let run = bulk_rename_local(&[row], &AtomicU8::new(OperationIntent::Running as u8), &test_recorder());

    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Skipped]);
    assert_eq!(
        fs::read_to_string(&source).expect("read changed source"),
        "changed after review"
    );
    assert!(!destination.exists(), "a changed source must not be renamed");
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_honours_cancel_before_staging() {
    let tmp = create_test_dir("cancel_before_staging");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    let row = local_row("cancelled", source.clone(), destination.clone());

    let run = bulk_rename_local(&[row], &AtomicU8::new(OperationIntent::Stopped as u8), &test_recorder());

    assert!(run.cancelled, "cancel must stop the batch driver");
    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Skipped]);
    assert_eq!(fs::read_to_string(&source).expect("read preserved source"), "reviewed");
    assert!(!destination.exists(), "cancel must not apply a final rename");
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn case_only_staging_recovers_the_source_when_the_destination_is_occupied() {
    let tmp = create_test_dir("case_restore");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    fs::write(&destination, "external").expect("write occupied destination");
    let row = local_row("restore", source.clone(), destination.clone());
    let mut outcome = BulkRenameOutcome::Skipped;

    rename_local_case_only(&row, &mut outcome, &test_recorder());

    assert_eq!(outcome, BulkRenameOutcome::Skipped);
    assert_eq!(fs::read_to_string(&source).expect("read restored source"), "reviewed");
    assert_eq!(fs::read_to_string(&destination).expect("read destination"), "external");
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn exclusive_local_rename_moves_into_an_empty_name() {
    let tmp = create_test_dir("exclusive_empty");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");

    rename_local_exclusive(&source, &destination).expect("exclusive rename into empty name");

    assert!(!source.exists(), "the source name must be released");
    assert_eq!(fs::read_to_string(&destination).expect("read destination"), "reviewed");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn exclusive_local_rename_never_replaces_an_existing_destination() {
    let tmp = create_test_dir("exclusive_conflict");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write source fixture");
    fs::write(&destination, "appeared after preflight").expect("write destination fixture");

    let result = rename_local_exclusive(&source, &destination);

    assert!(result.is_err(), "an occupied destination must reject the rename");
    assert_eq!(fs::read_to_string(&source).expect("read preserved source"), "reviewed");
    assert_eq!(
        fs::read_to_string(&destination).expect("read preserved destination"),
        "appeared after preflight"
    );
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_preserves_a_destination_created_after_review() {
    let tmp = create_test_dir("late_destination");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write source fixture");
    let row = local_row("late-conflict", source.clone(), destination.clone());
    fs::write(&destination, "appeared after review").expect("write late destination");

    let run = bulk_rename_local(&[row], &AtomicU8::new(OperationIntent::Running as u8), &test_recorder());

    assert_ne!(run.outcomes, vec![BulkRenameOutcome::Done]);
    assert_eq!(fs::read_to_string(&source).expect("read preserved source"), "reviewed");
    assert_eq!(
        fs::read_to_string(&destination).expect("read preserved destination"),
        "appeared after review"
    );
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

/// A journal bed for the as-it-lands recorder: an installed writer journal, an
/// open op, and a reader for the rows the run wrote. Deliberately does NOT call
/// `record_bulk_rename_outcomes` — these tests exist to prove the RUN journals,
/// not a later pass.
struct LandingJournal {
    _journal: crate::operation_log::TestJournalGuard,
    _journal_dir: TestDir,
    db: PathBuf,
    op_id: String,
}

impl LandingJournal {
    fn new(name: &str) -> Self {
        use crate::operation_log::capture::WriterJournal;
        use crate::operation_log::store::operation_log_db_path;
        use crate::operation_log::writer::OperationLogWriter;

        let journal_dir = create_test_dir(&format!("{name}_journal"));
        let db = operation_log_db_path(&journal_dir);
        let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
        let journal = crate::operation_log::TestJournalGuard::install(Arc::new(WriterJournal::new(writer)));
        let op_id = format!("op-{name}");
        super::super::super::journal::open_local_op(&op_id, OpKind::Rename, Initiator::Agent, 1, Some("root"));
        Self {
            _journal: journal,
            _journal_dir: journal_dir,
            db,
            op_id,
        }
    }

    fn recorder(&self) -> BulkRenameRecorder {
        BulkRenameRecorder::new(self.op_id.clone(), "root".to_string())
    }

    /// The rows as the journal holds them, after finalizing so the writer flushes.
    fn rows(&self) -> Vec<crate::operation_log::store::OperationItemRow> {
        use crate::operation_log::store::{open_read_connection, read_operation_items};
        super::super::super::journal::finalize_op(&self.op_id, OpKind::Rename, ExecutionStatus::Done);
        let conn = open_read_connection(&self.db).expect("read conn");
        read_operation_items(&conn, &self.op_id, 100).expect("items")
    }
}

/// The bug this catches: journaling used to be one pass AFTER the whole batch, so
/// a crash or force-quit mid-batch left the renames done on disk, `operation_items`
/// empty, and nothing for undo to reverse. The run itself must journal each landing.
#[test]
fn a_landed_rename_is_journaled_by_the_run_itself_not_by_a_later_pass() {
    let bed = LandingJournal::new("landed_in_run");
    let tmp = create_test_dir("landed_in_run_files");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "payload").expect("write fixture");
    let row = local_row("landed", source.clone(), destination.clone());

    let run = bulk_rename_local(
        std::slice::from_ref(&row),
        &AtomicU8::new(OperationIntent::Running as u8),
        &bed.recorder(),
    );
    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Done]);

    let rows = bed.rows();
    assert_eq!(rows.len(), 1, "the run journaled the landing on its own");
    assert_eq!(rows[0].outcome, ItemOutcome::Done);
}

/// Finding: `record_bulk_rename_outcomes` opened with `if outcome != Done { continue }`,
/// so a skipped row was journaled NOWHERE and the operation log silently claimed a
/// smaller batch than the user approved.
#[test]
fn a_skipped_row_reaches_the_journal_instead_of_vanishing() {
    let bed = LandingJournal::new("skipped_row");
    let tmp = create_test_dir("skipped_row_files");
    let source = tmp.join("keep.txt");
    let destination = tmp.join("taken.txt");
    fs::write(&source, "payload").expect("write fixture");
    fs::write(&destination, "occupied").expect("write blocker");
    let row = local_row("skipped", source.clone(), destination.clone());

    let run = bulk_rename_local(
        std::slice::from_ref(&row),
        &AtomicU8::new(OperationIntent::Running as u8),
        &bed.recorder(),
    );
    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Skipped]);

    let rows = bed.rows();
    assert_eq!(rows.len(), 1, "the skipped row is recorded, not dropped");
    assert_eq!(rows[0].outcome, ItemOutcome::Skipped);
}

/// A case-only rename goes through a private temp name. If the temp hop isn't
/// journaled, a crash between the two renames leaves the file at a
/// `.cmdr-bulk-rename-*` name that no journal, no ledger, and no sweep knows about,
/// so nothing can ever find it again.
#[test]
fn a_case_only_rename_journals_its_temp_hop_so_a_crash_leaves_it_findable() {
    let bed = LandingJournal::new("case_only_temp");
    let tmp = create_test_dir("case_only_temp_files");
    let source = tmp.join("screenshot.png");
    let destination = tmp.join("Screenshot.png");
    fs::write(&source, "payload").expect("write fixture");
    let row = local_row("caseonly", source.clone(), destination.clone());

    let run = bulk_rename_local(
        std::slice::from_ref(&row),
        &AtomicU8::new(OperationIntent::Running as u8),
        &bed.recorder(),
    );
    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Done]);

    let rows = bed.rows();
    assert_eq!(
        rows.len(),
        2,
        "the temp hop and the landing are both hops that happened"
    );
    assert!(
        rows.iter().any(|row| row
            .dest_name
            .as_deref()
            .is_some_and(|name| name.starts_with(".cmdr-bulk-rename-"))),
        "the temp hop names the temp, so recovery can find the file: {rows:?}"
    );
}

/// A name swap rotates through one temp. Every hop it makes is a real filesystem
/// change, so every hop needs a row; undo replays them in reverse.
#[test]
fn a_name_swap_journals_every_hop_of_its_rotation() {
    let bed = LandingJournal::new("swap_hops");
    let tmp = create_test_dir("swap_hops_files");
    let left = tmp.join("a.txt");
    let right = tmp.join("b.txt");
    fs::write(&left, "left").expect("write left");
    fs::write(&right, "right").expect("write right");
    let rows_in = vec![
        local_row("l", left.clone(), right.clone()),
        local_row("r", right.clone(), left.clone()),
    ];

    let run = bulk_rename_local(
        &rows_in,
        &AtomicU8::new(OperationIntent::Running as u8),
        &bed.recorder(),
    );
    assert_eq!(
        run.outcomes,
        vec![BulkRenameOutcome::Done, BulkRenameOutcome::Done],
        "the swap applied"
    );

    let rows = bed.rows();
    assert_eq!(
        rows.len(),
        3,
        "two files swapping is three hops: one out to the temp, one direct, one back in: {rows:?}"
    );
    assert_eq!(fs::read_to_string(&left).expect("read left"), "right");
    assert_eq!(fs::read_to_string(&right).expect("read right"), "left");
    assert_no_staging_paths(&tmp);
}

/// Journaling skips made a worry reachable-looking: `restore_move` answers any
/// non-`Done` unit with `SkipReason::Failed`, so a skipped row could report as a
/// failed undo. It can't, and this pins why: the rollback engine's query binds
/// `ItemOutcome::Done`, so a skipped row is a log entry and never a rollback unit.
/// Undo a batch that skipped a row and only the landed row is offered for reversal.
#[test]
fn a_skipped_row_is_logged_but_never_offered_to_undo_as_a_rollback_unit() {
    use crate::operation_log::store::{open_read_connection, read_rollback_units_page};

    let bed = LandingJournal::new("skip_not_a_unit");
    let tmp = create_test_dir("skip_not_a_unit_files");
    let landed_source = tmp.join("lands.txt");
    let landed_destination = tmp.join("landed.txt");
    let blocked_source = tmp.join("blocked.txt");
    let blocked_destination = tmp.join("occupied.txt");
    fs::write(&landed_source, "lands").expect("write lands");
    fs::write(&blocked_source, "blocked").expect("write blocked");
    fs::write(&blocked_destination, "already here").expect("write blocker");
    let rows_in = vec![
        local_row("lands", landed_source.clone(), landed_destination.clone()),
        local_row("blocked", blocked_source.clone(), blocked_destination.clone()),
    ];

    let run = bulk_rename_local(
        &rows_in,
        &AtomicU8::new(OperationIntent::Running as u8),
        &bed.recorder(),
    );
    assert_eq!(
        run.outcomes,
        vec![BulkRenameOutcome::Done, BulkRenameOutcome::Skipped],
        "one landed, one was blocked by an occupied destination"
    );

    let rows = bed.rows();
    assert_eq!(rows.len(), 2, "both rows are in the log: {rows:?}");

    let conn = open_read_connection(&bed.db).expect("read conn");
    let units = read_rollback_units_page(&conn, &bed.op_id, i64::MAX, 100).expect("rollback units");
    assert_eq!(
        units.len(),
        1,
        "only the row that actually moved is reversible: {units:?}"
    );
}
