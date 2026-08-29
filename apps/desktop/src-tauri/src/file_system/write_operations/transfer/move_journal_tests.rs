//! Full-loop tests over the MOVE record points: run a real local move through
//! the pipeline with a real journal installed, then try to reverse it exactly as
//! production does (the eligibility gate first, then the engine), and assert on
//! the files that are left on disk.
//!
//! Why full-loop rather than row assertions alone: each defect these pin is only
//! visible where journaling and reversal meet. A row that names the wrong
//! destination reads as a perfectly good row; it's the reversal acting on it that
//! moves a file the operation never touched.
//!
//! The same-volume (`Volume::rename`) half of the merge-move story lives in
//! `volume/rename_merge_tests.rs`, next to that path's rig.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::{move_files_with_progress_inner, move_with_staging};

use crate::file_system::VolumeManager;
use crate::file_system::volume::{LocalPosixVolume, Volume};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::rollback::Reversal;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, WriteOperationConfig};
use crate::file_system::write_operations::{journal, journal_search};
use crate::operation_log::TestJournalGuard;
use crate::operation_log::capture::WriterJournal;
use crate::operation_log::query::{OperationItemView, get_operation};
use crate::operation_log::rollback::{RollbackRefusal, RollbackReport, execute_rollback, rollback_operation};
use crate::operation_log::store::{OperationRow, open_read_connection, operation_log_db_path, read_operation};
use crate::operation_log::types::{
    EntryType, ExecutionStatus, Initiator, NotRollbackableReason, OpKind, RollbackState, RowRole, SearchCoverage,
};
use crate::operation_log::writer::OperationLogWriter;

/// A real move, journaled to a real DB, reversible through the real gate.
struct MoveLoop {
    _journal: TestJournalGuard,
    writer_journal: Arc<WriterJournal>,
    vm: VolumeManager,
    /// The work tree the move runs in. Held so it outlives the run.
    work: tempfile::TempDir,
    /// The journal DB's own directory, held for the same reason: dropping it
    /// would delete `operation-log.db` out from under the writer thread.
    _journal_dir: tempfile::TempDir,
    op_id: String,
}

impl MoveLoop {
    fn new(op_id: &str) -> Self {
        let journal_dir = tempfile::tempdir().expect("journal dir");
        let writer = OperationLogWriter::spawn(&operation_log_db_path(journal_dir.path())).expect("spawn writer");
        let writer_journal = Arc::new(WriterJournal::new(writer));
        let journal = TestJournalGuard::install(writer_journal.clone());

        let vm = VolumeManager::new();
        // Rooted at `/` so the fixture's absolute paths resolve, matching how the
        // real `root` volume is registered.
        vm.register(
            "root",
            Arc::new(LocalPosixVolume::new("Test root", "/")) as Arc<dyn Volume>,
        );

        MoveLoop {
            _journal: journal,
            writer_journal,
            vm,
            work: tempfile::tempdir().expect("work dir"),
            _journal_dir: journal_dir,
            op_id: op_id.to_string(),
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.work.path().join(rel)
    }

    /// Write a file, creating its parents. Every fixture file gets the same fixed
    /// mtime, so a rename-aside case can hand the operation a genuine duplicate
    /// (same size AND same mtime) — the shape where a wrong journal row is
    /// indistinguishable from a right one at reversal time.
    fn write(&self, rel: &str, contents: &[u8]) -> PathBuf {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mk parents");
        std::fs::write(&path, contents).expect("write fixture");
        filetime::set_file_mtime(&path, filetime::FileTime::from_unix_time(1_700_000_000, 0)).expect("pin mtime");
        path
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    }

    fn exists(&self, rel: &str) -> bool {
        self.path(rel).exists()
    }

    /// The names directly inside `rel`, sorted — for asserting where a
    /// rename-aside actually landed without hard-coding the reserved name.
    fn names_in(&self, rel: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.path(rel))
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Run a same-FS move, bracketed by the journal open/finalize the managed
    /// driver arranges.
    fn move_same_fs(&self, sources: &[PathBuf], destination: &Path, config: &WriteOperationConfig) {
        journal::open_local_op(
            &self.op_id,
            OpKind::Move,
            Initiator::User,
            sources.len() as u64,
            Some("root"),
        );
        let events = CollectorEventSink::new();
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        let result = move_files_with_progress_inner(&events, &self.op_id, &state, sources, destination, config);
        journal::finalize_op(
            &self.op_id,
            OpKind::Move,
            journal::execution_status_from_error(result.as_ref().err()),
        );
        result.expect("the fixture move ran");
    }

    /// Run the cross-FS (staging) move body directly. Two real filesystems aren't
    /// available in a unit test, so this calls the path the dispatcher would pick
    /// for them — the same trick `move_op_tests.rs` uses.
    fn move_cross_fs(&self, sources: &[PathBuf], destination: &Path, config: &WriteOperationConfig) {
        journal::open_local_op(
            &self.op_id,
            OpKind::Move,
            Initiator::User,
            sources.len() as u64,
            Some("root"),
        );
        let events = CollectorEventSink::new();
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        let result = move_with_staging(&events, &self.op_id, &state, sources, destination, config, 0);
        journal::finalize_op(
            &self.op_id,
            OpKind::Move,
            journal::execution_status_from_error(result.as_ref().err()),
        );
        result.expect("the fixture move ran");
    }

    fn op_row(&self) -> OperationRow {
        let conn = open_read_connection(self.writer_journal.writer().db_path()).expect("read conn");
        read_operation(&conn, &self.op_id)
            .expect("read op")
            .expect("the move is journaled")
    }

    /// The op's item rows as the history dialog sees them: interned dirs resolved
    /// to full paths.
    fn items(&self) -> Vec<OperationItemView> {
        let conn = open_read_connection(self.writer_journal.writer().db_path()).expect("read conn");
        get_operation(&conn, &self.op_id, 1_000, 0)
            .expect("read op detail")
            .expect("the move is journaled")
            .items
    }

    /// Ask for the reversal the way the app does: the eligibility gate decides,
    /// and only an accepted request reaches the engine. A refused one leaves the
    /// files alone, which is exactly what these tests check.
    async fn attempt_rollback(&self) -> Result<RollbackReport, RollbackRefusal> {
        let writer = self.writer_journal.writer();
        let plan = rollback_operation(&self.vm, writer, &self.op_id, |_plan| Ok(()))?;
        let reversal = Reversal::new("move-journal-test");
        Ok(execute_rollback(
            &self.vm,
            writer,
            &plan.original,
            &plan.inverse_op_id,
            Initiator::User,
            reversal.runner(),
        )
        .await)
    }
}

/// Canned drive-index leaves for the next subtree enumeration, so a unit test can
/// exercise the `search_only` rebase without a running index.
fn install_canned_leaves(names: &[&str]) {
    let leaves: Vec<_> = names
        .iter()
        .map(|n| journal_search::Leaf {
            rel: PathBuf::from(n),
            entry_type: EntryType::File,
            size: Some(1),
            mtime: None,
        })
        .collect();
    journal_search::test_hook::install(move |_path| {
        Some(journal_search::BufferedLeaves {
            coverage: SearchCoverage::Full,
            reason: None,
            leaves: leaves.clone(),
        })
    });
}

/// The destination path a moved item's row names.
fn dest_of(item: &OperationItemView) -> &str {
    item.dest_path.as_deref().expect("a moved item names a destination")
}

// ============================================================================
// A directory merge is not reversible at directory granularity
// ============================================================================

/// The worst of the three: `move A/ → B/` where `B/A` already exists merges, and
/// the journal's ONE directory row names `B/A` — a folder that also holds files
/// this operation never touched. Reversing that row renames the whole merged
/// folder to `A/`, carrying the destination's own files away with it.
///
/// This is the shape that survives the tempting fix (flagging merges that
/// overwrote something): nothing here is overwritten. The destination keeps
/// `keep.txt`, the source brings `fresh.txt`, the emptied source folder is
/// removed, and reversal would still take `keep.txt` away.
#[tokio::test]
async fn a_merge_move_that_overwrote_nothing_is_never_offered_for_reversal() {
    let fixture = MoveLoop::new("op-merge-clean");
    fixture.write("src/album/fresh.txt", b"SRC-fresh");
    fixture.write("dst/album/keep.txt", b"DEST-keep");

    fixture.move_same_fs(
        &[fixture.path("src/album")],
        &fixture.path("dst"),
        &WriteOperationConfig::default(),
    );
    assert_eq!(fixture.read("dst/album/fresh.txt"), "SRC-fresh", "the merge ran");

    // The claim first, whatever the gate answered: the destination's own file is
    // still in the destination.
    let outcome = fixture.attempt_rollback().await;
    assert_eq!(
        fixture.read("dst/album/keep.txt"),
        "DEST-keep",
        "a reversal must never relocate a file the operation didn't move"
    );
    assert_eq!(fixture.read("dst/album/fresh.txt"), "SRC-fresh");
    assert!(!fixture.exists("src/album"), "and nothing is put back at the source");
    assert_eq!(
        outcome.expect_err("a merge can't be reversed"),
        RollbackRefusal::NotRollbackable(NotRollbackableReason::DirectoryMerge)
    );
}

/// The same rule, for the merge that DID replace a destination file. The local
/// path never sets `overwrote` on a merge child, so this one also finalized
/// rollbackable — reversing it would have carried `keep.txt` away too.
#[tokio::test]
async fn a_merge_move_that_replaced_a_destination_file_is_never_offered_for_reversal() {
    let fixture = MoveLoop::new("op-merge-overwrite");
    fixture.write("src/album/shared.txt", b"SRC-shared");
    fixture.write("dst/album/shared.txt", b"DEST-shared");
    fixture.write("dst/album/keep.txt", b"DEST-keep");

    fixture.move_same_fs(
        &[fixture.path("src/album")],
        &fixture.path("dst"),
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            ..Default::default()
        },
    );
    assert_eq!(fixture.read("dst/album/shared.txt"), "SRC-shared", "the merge ran");

    let outcome = fixture.attempt_rollback().await;
    assert_eq!(
        fixture.read("dst/album/keep.txt"),
        "DEST-keep",
        "a reversal must never relocate a file the operation didn't move"
    );
    assert_eq!(
        outcome.expect_err("a merge can't be reversed"),
        RollbackRefusal::NotRollbackable(NotRollbackableReason::DirectoryMerge)
    );
}

// ============================================================================
// A rename-aside move journals where the file actually landed
// ============================================================================

/// A move onto an occupied name, resolved as Rename, lands the file at a fresh
/// `name (N)` and leaves the existing file untouched. The journal has to record
/// the landed path: recording the pre-existing one aims the reversal at a file
/// the operation never touched, and the fixture makes that file a genuine
/// duplicate (same size, same mtime) so the snapshot recheck can't save us.
#[tokio::test]
async fn a_move_that_landed_aside_reverses_its_own_file_and_leaves_the_other_alone() {
    let fixture = MoveLoop::new("op-rename-aside");
    let source = fixture.write("src/report.txt", b"SRCX");
    fixture.write("dst/report.txt", b"DEST");

    fixture.move_same_fs(
        std::slice::from_ref(&source),
        &fixture.path("dst"),
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Rename,
            ..Default::default()
        },
    );

    let landed = fixture
        .names_in("dst")
        .into_iter()
        .find(|n| n != "report.txt")
        .expect("the move landed aside under a fresh name");
    assert_eq!(fixture.read(&format!("dst/{landed}")), "SRCX");

    let items = fixture.items();
    assert_eq!(items.len(), 1, "one top-level row, got {items:?}");
    assert_eq!(
        dest_of(&items[0]),
        fixture.path("dst").join(&landed).to_string_lossy(),
        "the row names where the file landed, not the name that was taken"
    );

    let report = fixture.attempt_rollback().await.expect("a plain move is reversible");

    assert_eq!(report.reversed, 1);
    assert_eq!(report.skipped, 0);
    assert_eq!(fixture.read("src/report.txt"), "SRCX", "the moved file came home");
    assert_eq!(
        fixture.read("dst/report.txt"),
        "DEST",
        "the file that was already there is untouched"
    );
    assert!(!fixture.exists(&format!("dst/{landed}")), "the aside name is released");
}

/// The other half of the same fix: the subtree's `search_only` leaves are rebased
/// onto the top-level item's landed path too. Fixing only the rollback unit
/// leaves every search row for a landed-aside move pointing at the pre-existing
/// item's location, so "where did `deep.txt` go" answers with a stranger's
/// folder.
///
/// A directory source lands aside when a FILE holds its name at the destination.
#[tokio::test]
async fn a_move_that_landed_aside_rebases_its_search_leaves_onto_the_landed_path() {
    let fixture = MoveLoop::new("op-rename-aside-leaves");
    fixture.write("src/album/deep.txt", b"SRC-deep");
    // A file holding the folder's name at the destination: a type-mismatch
    // conflict, resolved by landing the folder aside.
    fixture.write("dst/album", b"DEST-file");

    // The drive index isn't running in a unit test, so the leaf enumeration is
    // canned — the rebase under test is the caller's, not the index's.
    install_canned_leaves(&["deep.txt"]);
    fixture.move_same_fs(
        &[fixture.path("src/album")],
        &fixture.path("dst"),
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Rename,
            ..Default::default()
        },
    );
    journal_search::test_hook::clear();

    let landed = fixture
        .names_in("dst")
        .into_iter()
        .find(|n| n != "album")
        .expect("the folder landed aside under a fresh name");
    assert_eq!(fixture.read("dst/album"), "DEST-file", "the file kept its name");

    let items = fixture.items();
    let leaf = items
        .iter()
        .find(|i| i.row_role == RowRole::SearchOnly)
        .expect("the subtree's leaf is recorded for search");
    assert_eq!(
        dest_of(leaf),
        fixture.path("dst").join(&landed).join("deep.txt").to_string_lossy(),
        "a search leaf points at where its file actually is"
    );
}

// ============================================================================
// A cross-FS move journals final paths, and says so when it can't
// ============================================================================

/// A cross-FS move copies into `.cmdr-staging-<op>/` and renames the tree into
/// place afterwards. Journaling the staging path leaves history and name search
/// pointing into a directory that no longer exists — and makes the reversal read
/// "already gone" and report a phantom success while the file sits at the
/// destination.
#[tokio::test]
async fn a_cross_fs_move_records_final_paths_and_really_puts_the_file_back() {
    let fixture = MoveLoop::new("op-cross-fs");
    let source = fixture.write("src/notes.txt", b"NOTES");
    std::fs::create_dir_all(fixture.path("dst")).expect("mk dst");

    fixture.move_cross_fs(
        std::slice::from_ref(&source),
        &fixture.path("dst"),
        &WriteOperationConfig::default(),
    );
    assert_eq!(fixture.read("dst/notes.txt"), "NOTES", "the move ran");

    let items = fixture.items();
    assert_eq!(items.len(), 1, "one leaf row, got {items:?}");
    assert_eq!(
        dest_of(&items[0]),
        fixture.path("dst/notes.txt").to_string_lossy(),
        "the row names the final destination, never the staging directory"
    );

    let report = fixture
        .attempt_rollback()
        .await
        .expect("a conflict-free cross-FS move is reversible");

    assert_eq!(report.reversed, 1);
    assert_eq!(report.skipped, 0);
    assert_eq!(
        fixture.read("src/notes.txt"),
        "NOTES",
        "the file is actually restored, not merely reported as reversed"
    );
    assert!(!fixture.exists("dst/notes.txt"));
}

/// Phase 2 journals against the staging area; phase 3 resolves conflicts at the
/// real destination. When phase 3 lands a file at `name (N)` instead, every row
/// this operation wrote names a path holding a STRANGER's file — and a move's
/// inverse is a restore-move, so a duplicate at that path would be carried off to
/// the source. The operation says it can't be reversed instead.
#[tokio::test]
async fn a_cross_fs_move_that_landed_aside_at_the_destination_is_not_reversible() {
    let fixture = MoveLoop::new("op-cross-fs-aside");
    let source = fixture.write("src/notes.txt", b"SRCX");
    fixture.write("dst/notes.txt", b"DEST");

    fixture.move_cross_fs(
        std::slice::from_ref(&source),
        &fixture.path("dst"),
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Rename,
            ..Default::default()
        },
    );

    assert_eq!(
        fixture.op_row().rollback_state,
        RollbackState::NotRollbackable,
        "the journaled destinations aren't where the files landed"
    );
    let outcome = fixture.attempt_rollback().await;
    assert_eq!(
        fixture.read("dst/notes.txt"),
        "DEST",
        "the file that was already there is never carried off to the source"
    );
    assert_eq!(
        outcome.expect_err("refused at the gate"),
        RollbackRefusal::NotRollbackable(NotRollbackableReason::StagedConflictResolved)
    );
}

/// The overwrite half of the same gap: phase 3 replaces a file at the final
/// destination, and phase 2's rows carry `overwrote = false` because nothing was
/// overwritten in the staging area. The operation must still refuse — the
/// replaced original is gone.
#[tokio::test]
async fn a_cross_fs_move_that_overwrote_at_the_destination_is_not_reversible() {
    let fixture = MoveLoop::new("op-cross-fs-overwrite");
    let source = fixture.write("src/notes.txt", b"SRCX");
    fixture.write("dst/notes.txt", b"DEST");

    fixture.move_cross_fs(
        std::slice::from_ref(&source),
        &fixture.path("dst"),
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            ..Default::default()
        },
    );
    assert_eq!(fixture.read("dst/notes.txt"), "SRCX", "the move ran");

    let refusal = fixture.attempt_rollback().await.expect_err("refused at the gate");
    assert_eq!(
        refusal,
        RollbackRefusal::NotRollbackable(NotRollbackableReason::Overwrote)
    );
}

/// A cross-FS move whose phase 3 merges into an existing folder is the bug-1
/// shape again, one layer down: the destination folder holds files this operation
/// never touched.
#[tokio::test]
async fn a_cross_fs_move_that_merged_into_an_existing_folder_is_not_reversible() {
    let fixture = MoveLoop::new("op-cross-fs-merge");
    fixture.write("src/album/fresh.txt", b"SRC-fresh");
    fixture.write("dst/album/keep.txt", b"DEST-keep");

    fixture.move_cross_fs(
        &[fixture.path("src/album")],
        &fixture.path("dst"),
        &WriteOperationConfig::default(),
    );
    assert_eq!(fixture.read("dst/album/fresh.txt"), "SRC-fresh", "the merge ran");

    let outcome = fixture.attempt_rollback().await;
    assert_eq!(
        fixture.read("dst/album/keep.txt"),
        "DEST-keep",
        "a reversal must never relocate a file the operation didn't move"
    );
    assert_eq!(
        outcome.expect_err("refused at the gate"),
        RollbackRefusal::NotRollbackable(NotRollbackableReason::DirectoryMerge)
    );
}

/// The guard rail on the merge rule: a folder move with NO name clash at the
/// destination is a plain rename, and it stays reversible — the disqualification
/// must catch merges only, not every folder move.
#[tokio::test]
async fn a_folder_move_with_no_name_clash_reverses_the_whole_folder() {
    let fixture = MoveLoop::new("op-folder-plain");
    fixture.write("src/album/fresh.txt", b"SRC-fresh");
    std::fs::create_dir_all(fixture.path("dst")).expect("mk dst");

    fixture.move_same_fs(
        &[fixture.path("src/album")],
        &fixture.path("dst"),
        &WriteOperationConfig::default(),
    );

    let report = fixture.attempt_rollback().await.expect("a plain folder move reverses");

    assert_eq!(report.reversed, 1);
    assert_eq!(report.skipped, 0);
    assert_eq!(fixture.read("src/album/fresh.txt"), "SRC-fresh", "the folder came home");
    assert!(!fixture.exists("dst/album"));
}

/// A move the fixture drives to completion still journals a `Done` op, so a
/// refusal above is the gate talking, not a broken fixture.
#[tokio::test]
async fn the_fixture_journals_a_finished_move() {
    let fixture = MoveLoop::new("op-plain-move");
    let source = fixture.write("src/notes.txt", b"NOTES");
    std::fs::create_dir_all(fixture.path("dst")).expect("mk dst");

    fixture.move_same_fs(
        std::slice::from_ref(&source),
        &fixture.path("dst"),
        &WriteOperationConfig::default(),
    );

    let row = fixture.op_row();
    assert_eq!(row.kind, OpKind::Move);
    assert_eq!(row.execution_status, ExecutionStatus::Done);
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);
}

/// A cross-FS move stages a whole tree and renames it into place, creating
/// destination directories on the way. Those directories need `dir` rows like a
/// copy's do, or the reversal puts every file back and leaves the empty skeleton
/// of the moved folder sitting at the destination.
///
/// The rows must name where each directory LIVES, not the `.cmdr-staging-<op>/`
/// path it was created under — the same rule the leaf rows follow.
#[tokio::test]
async fn a_cross_fs_move_reverses_the_directories_it_created_too() {
    let fixture = MoveLoop::new("op-cross-fs-dirs");
    fixture.write("src/album/inner/song.txt", b"SONG");
    std::fs::create_dir_all(fixture.path("dst")).expect("mk dst");

    fixture.move_cross_fs(
        &[fixture.path("src/album")],
        &fixture.path("dst"),
        &WriteOperationConfig::default(),
    );
    assert_eq!(fixture.read("dst/album/inner/song.txt"), "SONG", "the move ran");

    let items = fixture.items();
    let dirs: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::Dir).collect();
    assert_eq!(
        dirs.len(),
        2,
        "the two created destination directories owe a row each, got {items:?}"
    );
    for dir in &dirs {
        assert!(
            !dest_of(dir).contains(".cmdr-staging-"),
            "a dir row must name where the directory lives, not the staging path: {dir:?}"
        );
    }

    let report = fixture
        .attempt_rollback()
        .await
        .expect("a conflict-free cross-FS move is reversible");
    assert_eq!(report.skipped, 0, "nothing should be skipped, got {report:?}");
    assert_eq!(fixture.read("src/album/inner/song.txt"), "SONG", "the file is back");
    assert!(
        !fixture.exists("dst/album"),
        "the reversal must not leave the moved folder's empty skeleton behind"
    );
}
