//! Tests for `cleanup.rs`: the three ways this directory is allowed to remove
//! something, and the two that must never take more than one node.
//!
//! Rollback and the single-node prunes come first, `remove_tree` (the one that
//! recurses) last.
//!
//! A `#[path]` child of `cleanup`, so `super::` here is `cleanup` and
//! `super::super::` is `volume` (the one-level-shallower rule every `*_tests.rs`
//! in this directory follows).

use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::ledger::WrittenIdentity;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{CancelRollback, CancelRollbackOutcome};

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// Runs a rollback over the given ledger of complete volume files, and reports
/// what it managed.
///
/// Each entry is recorded with the size the backend reports right now, which is
/// what a copy that just wrote the file would have recorded — so the reversal's
/// recheck sees an undisturbed destination.
async fn roll_back(volume: &Arc<dyn Volume>, copied_paths: &[PathBuf], created_dirs: &[PathBuf]) -> CancelRollback {
    let mut ledger: Vec<WrittenFile> = Vec::new();
    for path in copied_paths {
        let size = volume.get_metadata(path).await.ok().and_then(|e| e.size).unwrap_or(0);
        ledger.push(WrittenFile::volume(path.clone(), size));
    }
    let events = CollectorEventSink::new();
    let state = make_state();
    volume_rollback_with_progress(
        volume,
        &mut ledger,
        created_dirs,
        &events,
        "cleanup-tests-op",
        &state,
        1,
        1,
        1,
        1,
    )
    .await
    .into_cancel_rollback()
}

/// **The destructive one.** A directory that reaches the partial sweep must
/// cost the user nothing.
///
/// `copy_serial.rs` parks every source's destination in `last_dest_path`,
/// directories included, and clears it in both arms of the transfer's result.
/// Whether a directory root can survive that window is a property of the
/// DRIVER (today it awaits the future, so it can't) — and the sweep is not
/// allowed to depend on it. A merged destination directory holds files the
/// user already had; removing it recursively is silent data loss.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_sweep_leaves_a_directory_and_its_contents_alone() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    clean_partial_writes(&volume, &[PathBuf::from("/album")], "cleanup-tests-op").await;

    assert!(
        vol.exists(Path::new("/album/keep-me.jpg")).await,
        "a directory in the partial sweep must not take the user's files with it"
    );
    assert!(
        vol.exists(Path::new("/album")).await,
        "the merged dest directory survives"
    );
}

/// The same leak through the OTHER feed: `copy.rs`'s RollingBack branch pushes
/// `last_dest_path` into `copied_paths`, and the rollback loop deletes each
/// entry. Same cell, same directory, same loss.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_leaves_a_directory_in_copied_paths_alone() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    let report = roll_back(&volume, &[PathBuf::from("/album")], &[]).await;

    assert_eq!(
        report.outcome,
        CancelRollbackOutcome::PartiallyRolledBack,
        "rollback runs to the end even when a path refuses, and says so"
    );
    assert!(
        vol.exists(Path::new("/album/keep-me.jpg")).await,
        "rollback must delete the files this op wrote, never a directory's contents"
    );
}

/// A file the copy wrote still goes, and one that's already gone is not a
/// failure worth logging: the sweep's job is "make sure this isn't there".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_removes_the_files_the_copy_wrote() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/ours.jpg"), b"we wrote this")
        .await
        .unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    roll_back(
        &volume,
        &[
            PathBuf::from("/album/ours.jpg"),
            PathBuf::from("/album/never-landed.jpg"),
        ],
        &[],
    )
    .await;

    assert!(!vol.exists(Path::new("/album/ours.jpg")).await);
    assert!(vol.exists(Path::new("/album/keep-me.jpg")).await);
}

/// A destination the backend now reports at a different size is left where it
/// is, while its unchanged neighbour still goes. On a volume the size IS the
/// identity: no backend but the local filesystem offers a node id.
///
/// The wrong size comes from `InMemoryVolume::set_reported_size`, the named
/// fixture for exactly this, ❌ never a hand-rolled forwarder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_leaves_a_file_the_backend_now_reports_at_another_size() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/ours.jpg"), b"we wrote this")
        .await
        .unwrap();
    vol.create_file(Path::new("/album/theirs.jpg"), b"we wrote this too")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    // The ledger as the copy left it: each file with the size it landed with.
    let mut ledger = Vec::new();
    for name in ["ours.jpg", "theirs.jpg"] {
        let path = PathBuf::from(format!("/album/{name}"));
        let size = volume.get_metadata(&path).await.unwrap().size.unwrap();
        ledger.push(WrittenFile::volume(path, size));
    }
    // Something else changes one of them while the copy was running.
    vol.set_reported_size(Path::new("/album/theirs.jpg"), 4096);

    let events = CollectorEventSink::new();
    let state = make_state();
    let report = volume_rollback_with_progress(
        &volume,
        &mut ledger,
        &[],
        &events,
        "cleanup-tests-op",
        &state,
        2,
        30,
        2,
        30,
    )
    .await
    .into_cancel_rollback();

    assert!(
        vol.exists(Path::new("/album/theirs.jpg")).await,
        "a file the backend reports differently must survive the reversal"
    );
    assert!(
        !vol.exists(Path::new("/album/ours.jpg")).await,
        "one changed file must not stop the reversal removing its neighbours"
    );
    assert_eq!(report.outcome, CancelRollbackOutcome::PartiallyRolledBack);
    assert_eq!(report.reversed, 1);
    assert_eq!(report.skips.len(), 1);
    assert_eq!(report.skips[0].reason, SkipReason::Drift);
    assert_eq!(report.skips[0].example_name, "theirs.jpg");
}

/// A write that was still in flight goes even though nothing about it can be
/// verified, while a complete file that changed stays. The two must never fold
/// together: a partial left at the destination is a truncated file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_partial_still_goes_while_a_changed_file_stays() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/half.mov"), b"the first chunk")
        .await
        .unwrap();
    vol.create_file(Path::new("/album/done.jpg"), b"complete")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    let done = PathBuf::from("/album/done.jpg");
    let size = volume.get_metadata(&done).await.unwrap().size.unwrap();
    let mut ledger = vec![WrittenFile::volume(done, size)];
    append_own_partials(&mut ledger, Some(PathBuf::from("/album/half.mov")), &[]);
    vol.set_reported_size(Path::new("/album/done.jpg"), 999_999);

    let events = CollectorEventSink::new();
    let state = make_state();
    volume_rollback_with_progress(
        &volume,
        &mut ledger,
        &[],
        &events,
        "cleanup-tests-op",
        &state,
        2,
        30,
        2,
        30,
    )
    .await;

    assert!(
        !vol.exists(Path::new("/album/half.mov")).await,
        "a partial must never be left at the destination"
    );
    assert!(
        vol.exists(Path::new("/album/done.jpg")).await,
        "a complete file that changed must never be deleted"
    );
}

/// **The second destructive one, against a backend that lies.** The created-dirs
/// prune has to establish emptiness ITSELF.
///
/// Every shipping backend refuses to delete a non-empty directory, and a
/// conformance assertion keeps them honest — but a guard that survives only
/// because a promise held breaks the day someone writes a new `Volume`. The
/// user's file here got into a created dir through the one window that allows
/// it (a TOCTOU race against another writer), and it must survive regardless of
/// what `delete` would have done.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_dir_prune_checks_emptiness_itself_even_on_a_recursive_backend() {
    use super::super::strategy::test_support::RecursiveDeleteVolume;

    let inner = Arc::new(InMemoryVolume::new("Dest"));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner
        .create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    roll_back(&volume, &[], &[PathBuf::from("/album")]).await;

    assert!(
        inner.exists(Path::new("/album/keep-me.jpg")).await,
        "the prune must list the directory before deleting it, not trust the backend to refuse"
    );
}

/// A created dir that really is empty still goes, on that same lying backend:
/// the emptiness check must not overshoot into "never prune anything".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_dir_prune_still_removes_an_empty_directory() {
    use super::super::strategy::test_support::RecursiveDeleteVolume;

    let inner = Arc::new(InMemoryVolume::new("Dest"));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner.create_directory(Path::new("/album/raw")).await.unwrap();
    let volume: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    // Creation order is shallowest-first; the prune walks it in reverse, so the
    // leaf empties before its parent is tried.
    roll_back(&volume, &[], &[PathBuf::from("/album"), PathBuf::from("/album/raw")]).await;

    assert!(!inner.exists(Path::new("/album/raw")).await);
    assert!(!inner.exists(Path::new("/album")).await);
}

// ── The in-flight partials this operation owns ─────────────────────────────

/// A write that was still in flight goes into the ledger as this operation's own
/// partial, which is a different thing from a file whose identity is unknown.
///
/// A partial has no size and no complete file to recognize, by construction, so
/// a reversal that skipped whatever it couldn't verify would leave a truncated
/// file at the destination. Nothing else can own a destination path that never
/// held a complete file, so these are removed on sight.
#[test]
fn in_flight_writes_join_the_ledger_as_this_operation_s_own_partials() {
    let mut ledger = vec![WrittenFile::volume(PathBuf::from("/album/done.jpg"), 4096)];

    append_own_partials(
        &mut ledger,
        Some(PathBuf::from("/album/half.jpg")),
        &[PathBuf::from("/album/also-half.jpg")],
    );

    assert_eq!(ledger[0].identity, WrittenIdentity::VolumeFile { size: 4096 });
    for partial in &ledger[1..] {
        assert_eq!(
            partial.identity,
            WrittenIdentity::OwnPartial,
            "{} was still being written",
            partial.path.display()
        );
        assert_ne!(
            partial.identity,
            WrittenIdentity::Unverifiable,
            "a partial that reads as merely unverifiable gets left at the destination"
        );
    }
    assert_eq!(ledger.len(), 3);
}

/// A path the ledger already carries isn't added twice: the completed write is
/// the better record, and a second entry would walk the same path twice.
#[test]
fn a_partial_the_ledger_already_carries_is_not_added_twice() {
    let mut ledger = vec![WrittenFile::volume(PathBuf::from("/album/one.jpg"), 10)];

    append_own_partials(
        &mut ledger,
        Some(PathBuf::from("/album/one.jpg")),
        &[PathBuf::from("/album/two.jpg"), PathBuf::from("/album/two.jpg")],
    );

    let paths: Vec<&PathBuf> = ledger.iter().map(|entry| &entry.path).collect();
    assert_eq!(
        paths,
        vec![&PathBuf::from("/album/one.jpg"), &PathBuf::from("/album/two.jpg")]
    );
    assert_eq!(ledger[0].identity, WrittenIdentity::VolumeFile { size: 10 });
}

/// The volume ledger is a stack too: a reversal the user stops halfway leaves it
/// claiming exactly what's still on the volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_volume_reversal_leaves_the_ledger_claiming_what_is_still_there() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    let mut ledger = Vec::new();
    for i in 0..4 {
        let path = PathBuf::from(format!("/album/file{i}.bin"));
        vol.create_file(&path, b"payload").await.unwrap();
        ledger.push(WrittenFile::volume(path, 7));
    }
    let volume: Arc<dyn Volume> = vol.clone();

    let state = make_state();
    let guard = TestOperationGuard::register_state("volume-ledger-stop", Arc::clone(&state));
    // Rolling back, and the user stops the reversal before it starts deleting.
    cancel_write_operation(guard.id(), true);
    cancel_write_operation(guard.id(), false);
    let events = CollectorEventSink::new();
    let report = volume_rollback_with_progress(&volume, &mut ledger, &[], &events, guard.id(), &state, 4, 28, 4, 28)
        .await
        .into_cancel_rollback();

    assert_eq!(
        report.outcome,
        CancelRollbackOutcome::NotRolledBack,
        "the user stopped it before it reached an item"
    );
    assert_eq!(ledger.len(), 4, "nothing was reversed, so nothing left the ledger");
    for entry in &ledger {
        assert!(
            vol.exists(&entry.path).await,
            "{} is still claimed, so it must still be on the volume",
            entry.path.display()
        );
    }
}

// ── remove_tree ───────────────────────────────────────────────────
//
// Regression coverage for the move-between-volumes recursive-delete fix.
// `Volume::delete` is contractually for files or *empty* directories
// (LocalPosix uses `std::fs::remove_dir`); cross-volume moves rely on
// this helper to clear out the source tree depth-first, which is why it
// carries a `TreeRemoval` naming who authorized the recursion.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_removes_nonempty_directory() {
    let vol = Arc::new(InMemoryVolume::new("V"));
    vol.create_directory(Path::new("/photos")).await.unwrap();
    vol.create_file(Path::new("/photos/a.jpg"), b"a").await.unwrap();
    vol.create_file(Path::new("/photos/b.jpg"), b"b").await.unwrap();
    vol.create_directory(Path::new("/photos/sub")).await.unwrap();
    vol.create_file(Path::new("/photos/sub/c.jpg"), b"c").await.unwrap();

    let result: Arc<dyn Volume> = vol.clone();
    remove_tree(
        &result,
        Path::new("/photos"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await
    .unwrap();

    assert!(!vol.exists(Path::new("/photos")).await);
    assert!(!vol.exists(Path::new("/photos/a.jpg")).await);
    assert!(!vol.exists(Path::new("/photos/sub/c.jpg")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_removes_single_file() {
    let vol = Arc::new(InMemoryVolume::new("V"));
    vol.create_file(Path::new("/file.txt"), b"hi").await.unwrap();

    let result: Arc<dyn Volume> = vol.clone();
    remove_tree(
        &result,
        Path::new("/file.txt"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await
    .unwrap();

    assert!(!vol.exists(Path::new("/file.txt")).await);
}

/// The whole tree can't come down because ONE leaf refuses. What comes back is
/// that leaf, not the root's own "directory not empty" — which names the folder
/// the user selected and tells them nothing they can act on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_reports_the_leaf_that_refused() {
    use super::super::strategy::test_support::UndeletableSource;

    let vol = UndeletableSource::new(
        "doomed.txt",
        VolumeError::IoError {
            message: "Resource busy".to_string(),
            raw_os_error: None,
        },
    );
    let volume: Arc<dyn Volume> = Arc::clone(&vol) as Arc<dyn Volume>;
    volume.create_directory(Path::new("/tree")).await.unwrap();
    volume.create_directory(Path::new("/tree/nested")).await.unwrap();
    volume.create_file(Path::new("/tree/fine.txt"), b"fine").await.unwrap();
    volume
        .create_file(Path::new("/tree/nested/doomed.txt"), b"doomed")
        .await
        .unwrap();

    let failure = remove_tree(
        &volume,
        Path::new("/tree"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await
    .expect_err("the leaf never deletes, so the sweep can't finish");
    assert_eq!(
        failure.path,
        Path::new("/tree/nested/doomed.txt"),
        "the failure must carry the leaf that refused, not the tree root"
    );

    // Best-effort still applies: everything that COULD go, went.
    assert!(!volume.exists(Path::new("/tree/fine.txt")).await);
    assert!(volume.exists(Path::new("/tree/nested/doomed.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_tree_missing_path_is_ok() {
    // Used during move cleanup where the path may already be gone (cancelled mid-op,
    // partial state). No error.
    let vol = Arc::new(InMemoryVolume::new("V"));
    let result: Arc<dyn Volume> = vol.clone();
    let r = remove_tree(
        &result,
        Path::new("/never-existed"),
        &HashSet::new(),
        TreeRemoval::MoveSourceAfterDestinationLanded,
    )
    .await;
    assert!(r.is_ok(), "expected Ok, got {r:?}");
}

/// A staged partial the destination refuses to remove **comes back from the
/// sweep**, so the summary the user reads can account for it.
///
/// This is the honesty half of the abandoned-write path. A dropped write task
/// leaves its SMB2 handle open, the server answers the delete with a sharing
/// violation for as long as that session lives, and a user who chose Rollback
/// was being told the destination was clear while gigabytes of scratch sat on
/// their NAS. Retrying can't help — the handle outlives the operation — so
/// reporting it is the whole remedy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_staged_partial_the_destination_keeps_comes_back_from_the_sweep() {
    let vol = Arc::new(InMemoryVolume::new("Dest").with_delete_failing());
    let volume: Arc<dyn Volume> = vol.clone();
    let state = make_state();
    let temp = PathBuf::from("/album/holiday.jpg.cmdr-tmp-4d1f9c");
    state.in_flight_temps.lock_ignore_poison().push(temp.clone());

    let unremoved = clean_abandoned_staged_writes(&volume, &state).await;

    assert_eq!(
        unremoved,
        vec![temp],
        "a temp the destination wouldn't take back has to reach the caller, not only the log"
    );
    let leftovers = CancelRollback::none()
        .with_staged_leftovers(&unremoved)
        .staged_leftovers
        .expect("a leftover the sweep reported must reach the summary");
    assert_eq!(leftovers.count, 1);
    assert_eq!(
        leftovers.example_name, "holiday.jpg.cmdr-tmp-4d1f9c",
        "named by what it is called at the destination, which is what the user would look for"
    );
}

/// A sweep that got everything reports nothing, so the ordinary cancel stays
/// silent about scratch nobody has to think about. A temp that had already gone
/// counts as removed rather than sending the user hunting for a file that isn't
/// there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sweep_that_cleared_the_destination_reports_no_leftovers() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    let present = PathBuf::from("/album/holiday.jpg.cmdr-tmp-4d1f9c");
    vol.create_file(&present, b"half a photo").await.unwrap();
    let volume: Arc<dyn Volume> = vol.clone();
    let state = make_state();
    state.in_flight_temps.lock_ignore_poison().extend([
        present.clone(),
        PathBuf::from("/album/already-landed.jpg.cmdr-tmp-77aa10"),
    ]);

    let unremoved = clean_abandoned_staged_writes(&volume, &state).await;

    assert!(unremoved.is_empty(), "nothing was left, so there is nothing to report");
    assert!(!vol.exists(&present).await, "the staged partial itself still goes");
    assert!(
        CancelRollback::none()
            .with_staged_leftovers(&unremoved)
            .staged_leftovers
            .is_none()
    );
}
