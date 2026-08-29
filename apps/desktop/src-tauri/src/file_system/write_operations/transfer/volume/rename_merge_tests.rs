//! Merge semantics of the same-volume rename-merge fast path
//! (`move_within_same_volume_with_progress` + `rename_merge_directory`): what
//! prompts, what the file policy does inside a merge, which source directories
//! survive, the dest-inside-source guard, symlinks, and why a merged folder
//! can't be reversed.
//!
//! These drive the real `move_within_same_volume_with_progress` pipeline so the
//! whole stack — top-level hints, the driver's top-level conflict detection, the
//! resolver short-circuit for dir-vs-dir, and the recursive rename-merge — runs
//! exactly as in production. The fixtures (and why they run on a real
//! `LocalPosixVolume`) are in `rename_merge_test_support.rs`.
//!
//! Five sibling suites take the families that carry a backend or a rig of their
//! own:
//! `rename_merge_cancel_tests.rs` (cancel mid-merge),
//! `rename_merge_case_fold_tests.rs` (case-insensitive backends and the
//! late-detected collision), `rename_merge_walk_tests.rs` (the no-subtree-walk
//! perf pin), `rename_merge_stat_tests.rs` (a stat that refuses to answer), and
//! `rename_merge_mtp_tests.rs` (the whole path against a virtual MTP device).

use super::super::conflict_responder_test_support::{
    ConflictResponderSink, file_conflict_count, folder_conflict_count_any_dir,
};
use super::move_same::move_within_same_volume_with_progress;
use super::rename_merge_test_support::{exists, local_volume, make_state, mkdir, read, write_file};
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::WriteOperationState;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig, WriteOperationError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Merge with zero folder prompts
// ============================================================================

/// A top-level folder collision merges with NO folder-level prompt. Dest-only
/// files survive, source-only files arrive, and a non-clashing nested subtree
/// rides across on one rename.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_no_folder_prompt_dest_only_survives() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    // Source /album: a fresh file + a nested subtree with no dest clash.
    write_file(root, "src/album/fresh.txt", b"SRC-fresh");
    write_file(root, "src/album/sub/deep.txt", b"SRC-deep");
    // Dest /album: a dest-only file that must survive the merge.
    write_file(root, "dst/album/keep.txt", b"DEST-keep");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Stop policy: a folder-level prompt would BLOCK forever (no responder).
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-no-prompt",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    assert_eq!(
        folder_conflict_count_any_dir(&events),
        0,
        "a folder merge must never prompt"
    );
    // Dest-only file preserved.
    assert_eq!(read(root, "dst/album/keep.txt"), b"DEST-keep");
    // Source-only file + nested subtree arrived.
    assert_eq!(read(root, "dst/album/fresh.txt"), b"SRC-fresh");
    assert_eq!(read(root, "dst/album/sub/deep.txt"), b"SRC-deep");
    // Whole source spine deleted (all moved).
    assert!(!exists(root, "src/album"), "fully-moved source spine must be gone");
}

// ============================================================================
// File policy inside the merge
// ============================================================================

/// Inside a merge, a clashing FILE follows the Skip policy: dest keeps its copy,
/// source keeps its original, and the source DIR survives (it still holds the
/// skipped child).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_skip_child_leaves_source_dir_and_ancestors() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC-clash");
    write_file(root, "src/album/sub/deeper/clash2.txt", b"SRC-deep-clash");
    write_file(root, "src/album/sub/deeper/fresh.txt", b"SRC-fresh");
    write_file(root, "dst/album/clash.txt", b"DEST-clash");
    write_file(root, "dst/album/sub/deeper/clash2.txt", b"DEST-deep-clash");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-skip",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Dest keeps both clashing copies; both sources survive (skip = keep both).
    assert_eq!(read(root, "dst/album/clash.txt"), b"DEST-clash");
    assert_eq!(read(root, "dst/album/sub/deeper/clash2.txt"), b"DEST-deep-clash");
    assert!(exists(root, "src/album/clash.txt"), "skipped source file must survive");
    assert!(
        exists(root, "src/album/sub/deeper/clash2.txt"),
        "skipped deep source file must survive"
    );
    // The non-clashing fresh file still moved.
    assert_eq!(read(root, "dst/album/sub/deeper/fresh.txt"), b"SRC-fresh");
    assert!(!exists(root, "src/album/sub/deeper/fresh.txt"));

    // Source dir + ALL its ancestors survive because they still hold skipped
    // children. Inside-out empty-only cleanup never deletes a dir with content.
    assert!(exists(root, "src/album"), "source dir holding a skipped child survives");
    assert!(
        exists(root, "src/album/sub/deeper"),
        "deepest source dir holding a skipped child survives"
    );
}

/// Inside a merge, a clashing FILE under Overwrite-all replaces the dest copy
/// (delete-then-rename), and the fully-emptied source spine is deleted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_overwrite_replaces_and_deletes_source_spine() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC-NEW");
    write_file(root, "src/album/sub/clash2.txt", b"SRC-NEW-2");
    write_file(root, "dst/album/clash.txt", b"DEST-OLD");
    write_file(root, "dst/album/sub/clash2.txt", b"DEST-OLD-2");
    write_file(root, "dst/album/sub/keep.txt", b"DEST-keep");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-overwrite",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert_eq!(folder_conflict_count_any_dir(&events), 0);

    // Clashing files replaced with the source bytes.
    assert_eq!(read(root, "dst/album/clash.txt"), b"SRC-NEW");
    assert_eq!(read(root, "dst/album/sub/clash2.txt"), b"SRC-NEW-2");
    // Dest-only file untouched (merge invariant).
    assert_eq!(read(root, "dst/album/sub/keep.txt"), b"DEST-keep");
    // Everything moved → source spine gone, deepest-first.
    assert!(!exists(root, "src/album"), "fully-moved source spine must be deleted");
}

/// Inside a merge, a clashing FILE under Stop emits a per-file `write-conflict`
/// (NOT a folder one), and resumes on the scripted answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_stop_file_clash_prompts_and_resumes() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC-NEW");
    write_file(root, "dst/album/clash.txt", b"DEST-OLD");

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-stop",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Exactly one FILE prompt, zero FOLDER prompts — sink-derived, race-free.
    assert_eq!(file_conflict_count(&events.inner), 1, "exactly one file clash prompted");
    assert_eq!(
        folder_conflict_count_any_dir(&events.inner),
        0,
        "the folder itself never prompts"
    );
    // The Overwrite answer landed the source bytes.
    assert_eq!(read(root, "dst/album/clash.txt"), b"SRC-NEW");
}

// ============================================================================
// Source-dir cleanup matrix
// ============================================================================

/// All-Rename: every clashing child resolves to Rename (lands as `name (1)`), so
/// every source child moves out and the spine deletes inside-out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_all_rename_deletes_source_spine() {
    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/clash.txt", b"SRC");
    write_file(root, "src/album/sub/clash2.txt", b"SRC2");
    write_file(root, "dst/album/clash.txt", b"DEST");
    write_file(root, "dst/album/sub/clash2.txt", b"DEST2");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Rename,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-rename",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // Originals preserved at dest; renamed copies landed beside them.
    assert_eq!(read(root, "dst/album/clash.txt"), b"DEST");
    assert_eq!(read(root, "dst/album/clash (1).txt"), b"SRC");
    assert_eq!(read(root, "dst/album/sub/clash2.txt"), b"DEST2");
    assert_eq!(read(root, "dst/album/sub/clash2 (1).txt"), b"SRC2");
    // All children moved → source spine deleted inside-out.
    assert!(
        !exists(root, "src/album"),
        "all-Rename empties and deletes the source spine"
    );
}

/// An errored deep child preserves the source dir and its ancestors. A read-only
/// nested dest subdir makes the child rename fail; the source must survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_errored_child_preserves_source_spine() {
    // Root bypasses POSIX permission bits, so the read-only dest subdir below
    // wouldn't block the rename and the error path wouldn't trigger. The Linux
    // CI Rust suite runs as root in Docker; skip there (mirrors the geteuid==0
    // guards in the permission-dependent integration tests).
    #[cfg(unix)]
    // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail — it just
    // returns the caller's effective uid. We compare the returned integer to 0 to detect root.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let (volume, dir) = local_volume();
    let root = dir.path();

    write_file(root, "src/album/ok.txt", b"OK");
    write_file(root, "src/album/sub/blocked.txt", b"SRC");
    // Dest has the same subtree; make the dest subdir read-only so renaming a
    // child INTO it fails (POSIX requires write on the target directory).
    write_file(root, "dst/album/sub/other.txt", b"DEST");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let sub = root.join("dst/album/sub");
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
    }

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-error",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;

    // Restore permissions so the TempDir can clean up.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(root.join("dst/album/sub"), std::fs::Permissions::from_mode(0o755));
    }

    // On Unix the blocked rename errors out; the source spine must survive.
    #[cfg(unix)]
    {
        assert!(result.is_err(), "a blocked child rename must surface as an error");
        assert!(
            exists(root, "src/album/sub/blocked.txt"),
            "errored child must leave the source in place"
        );
        assert!(exists(root, "src/album"), "errored child preserves the source spine");
    }
    #[cfg(not(unix))]
    {
        let _ = result;
    }
}

// ============================================================================
// Dest-inside-source guard
// ============================================================================

/// Moving `/A` into `/A/sub` (its own descendant) on the same volume is
/// rejected before any rename runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_into_own_descendant_is_rejected() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "A/file.txt", b"x");
    mkdir(root, "A/sub");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-dest-inside",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("A")],
        Path::new("A/sub"),
        &config,
    )
    .await;

    assert!(
        matches!(result, Err(WriteOperationError::DestinationInsideSource { .. })),
        "moving a dir into its own descendant must be rejected, got {:?}",
        result
    );
    // Nothing was moved.
    assert!(exists(root, "A/file.txt"), "source untouched on a rejected move");
}

// ============================================================================
// Symlinks moved as opaque entries
// ============================================================================

/// A symlink child is renamed as an opaque entry — never descended.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_merge_moves_symlink_as_opaque_entry() {
    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "src/album/real.txt", b"REAL");
    std::os::unix::fs::symlink("real.txt", root.join("src/album/link.txt")).unwrap();
    mkdir(root, "dst/album");

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_within_same_volume_with_progress(
        events.clone(),
        "op-merge-symlink",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    // The symlink moved as a symlink (not dereferenced into a copy of the file).
    let link = root.join("dst/album/link.txt");
    let meta = std::fs::symlink_metadata(&link).unwrap();
    assert!(meta.file_type().is_symlink(), "symlink must move as a symlink");
    assert_eq!(read(root, "dst/album/real.txt"), b"REAL");
    assert!(!exists(root, "src/album"), "fully-moved source spine deleted");
}

// ============================================================================
// A merged folder is not reversible
// ============================================================================

/// Run a same-volume move against a fresh journal DB and hand back the operation
/// row it finalized. The guard and the DB's own tempdir ride along in the tuple
/// so the caller keeps them alive: dropping the journal dir would delete
/// `operation-log.db` out from under the writer thread.
async fn journaled_same_volume_move(
    op_id: &str,
    volume: &Arc<dyn Volume>,
    sources: &[PathBuf],
    destination: &Path,
    config: &VolumeCopyConfig,
) -> (
    crate::operation_log::store::OperationRow,
    crate::operation_log::TestJournalGuard,
    TempDir,
) {
    use crate::file_system::write_operations::journal;
    use crate::operation_log::capture::WriterJournal;
    use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation};
    use crate::operation_log::types::{ExecutionStatus, Initiator, OpKind};
    use crate::operation_log::writer::OperationLogWriter;

    let journal_dir = TempDir::new().unwrap();
    let db = operation_log_db_path(journal_dir.path());
    let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
    let guard = crate::operation_log::TestJournalGuard::install(Arc::new(WriterJournal::new(writer)));

    // A same-volume move journals under the REAL volume id on both sides.
    let state =
        Arc::new(WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("v".into(), "v".into()));
    journal::open_volume_op(
        op_id,
        OpKind::Move,
        Initiator::User,
        "v",
        Some("v"),
        sources.len() as u64,
    );
    let events = Arc::new(CollectorEventSink::new());
    let result =
        move_within_same_volume_with_progress(events, op_id, &state, Arc::clone(volume), sources, destination, config)
            .await;
    journal::finalize_op(op_id, OpKind::Move, ExecutionStatus::Done);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let conn = open_read_connection(&db).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    (row, guard, journal_dir)
}

/// The same-volume twin of the local merge rule: a folder collision merges, and
/// the ONE row it journals names the pre-existing destination folder — which also
/// holds files this operation never touched. Reversing that row would rename the
/// merged folder back to the source and carry them along.
///
/// Nothing is overwritten here, which is the point: the disqualifying condition
/// is "merged", not "overwrote something".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_merge_that_overwrote_nothing_is_not_rollbackable() {
    use crate::operation_log::types::{NotRollbackableReason, RollbackState};

    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "src/album/fresh.txt", b"SRC-fresh");
    write_file(root, "dst/album/keep.txt", b"DEST-keep");

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let (row, _journal, _journal_dir) = journaled_same_volume_move(
        "op-merge-journal-clean",
        &volume,
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;

    assert_eq!(read(root, "dst/album/keep.txt"), b"DEST-keep", "the merge ran");
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    assert_eq!(row.not_rollbackable_reason, Some(NotRollbackableReason::DirectoryMerge));
}

/// The same verdict when a merge child DID replace a destination file. The
/// overwrite alone already ruled this one out; the merge is why the reason names
/// the merge.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_merge_that_replaced_a_destination_file_is_not_rollbackable() {
    use crate::operation_log::types::RollbackState;

    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "src/album/shared.txt", b"SRC-shared");
    write_file(root, "dst/album/shared.txt", b"DEST-shared");
    write_file(root, "dst/album/keep.txt", b"DEST-keep");

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let (row, _journal, _journal_dir) = journaled_same_volume_move(
        "op-merge-journal-overwrite",
        &volume,
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;

    assert_eq!(read(root, "dst/album/shared.txt"), b"SRC-shared", "the merge ran");
    assert_eq!(read(root, "dst/album/keep.txt"), b"DEST-keep");
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
}

/// The guard rail on the rule above: a same-volume move with NO folder collision
/// stays reversible, so the merge verdict can't quietly swallow every move.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_same_volume_move_with_no_folder_collision_stays_rollbackable() {
    use crate::operation_log::types::RollbackState;

    let (volume, dir) = local_volume();
    let root = dir.path();
    write_file(root, "src/album/fresh.txt", b"SRC-fresh");
    mkdir(root, "dst");

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let (row, _journal, _journal_dir) = journaled_same_volume_move(
        "op-move-journal-plain",
        &volume,
        &[PathBuf::from("src/album")],
        Path::new("dst"),
        &config,
    )
    .await;

    assert_eq!(read(root, "dst/album/fresh.txt"), b"SRC-fresh");
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);
    assert_eq!(row.not_rollbackable_reason, None);
}
