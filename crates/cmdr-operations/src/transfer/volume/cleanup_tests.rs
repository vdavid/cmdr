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
use crate::event_sinks::CollectorEventSink;
use crate::in_flight_temps::{self, Ledger, RecordHome};
use crate::ledger::WrittenIdentity;
use crate::outage_slot::{CleanupPolicy, OutagePolicy};
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::types::{CancelRollback, CancelRollbackOutcome, StagedLeftoverFate, StagedLeftovers};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;

/// A stop's background cleanup at a test's pace: a dial that gives up fast, and
/// a wait long enough for a cell to bring the share back inside it.
const QUICK_CLEANUP: CleanupPolicy = CleanupPolicy {
    inline_budget: Duration::from_secs(5),
    wait: Duration::from_secs(10),
    retry_after: Duration::from_millis(20),
};
const QUICK_OUTAGE: OutagePolicy = OutagePolicy {
    budget: Duration::from_secs(30),
    cadence: &[Duration::from_millis(10)],
    poll_every: Duration::from_millis(5),
    attempt_timeout: Duration::from_millis(200),
};
const WAIT: Duration = Duration::from_secs(5);

fn make_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_cleanup_policy(QUICK_CLEANUP),
    )
}

/// A stopped operation writing to `volume_id`, recording into `ledger`, whose
/// stop has `temp` left in flight: what a task the drain's deadline dropped
/// mid-write leaves.
fn stopped_with_a_temp_on(
    engine: &TestOperations,
    ledger: &Ledger,
    volume_id: &str,
    temp: &Path,
) -> Arc<WriteOperationState> {
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("cleanup-test-source".to_string(), volume_id.to_string())
            .with_in_flight_ledger(ledger.clone())
            .with_outage_policy(QUICK_OUTAGE)
            .with_cleanup_policy(QUICK_CLEANUP),
    );
    in_flight_temps::track_temp(&state, Some(RecordHome::volume_space(volume_id)), temp);
    state
}

/// A NAS behind an [`OutageVolume`], named the way the leftover line names it.
fn nas(lane: &str) -> (Arc<InMemoryVolume>, Arc<OutageVolume>, Arc<dyn Volume>) {
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    let nas = OutageVolume::wrap(Arc::clone(&inner), lane);
    let volume: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    (inner, nas, volume)
}

fn leftover(fate: StagedLeftoverFate, example_name: &str, volume_name: &str) -> StagedLeftovers {
    StagedLeftovers {
        fate,
        count: 1,
        example_name: example_name.to_string(),
        volume_name: volume_name.to_string(),
    }
}

/// Runs a rollback over the given ledger of complete volume files, and reports
/// what it managed.
///
/// Each entry is recorded with the size the backend reports right now, which is
/// what a copy that just wrote the file would have recorded — so the reversal's
/// recheck sees an undisturbed destination.
async fn roll_back(
    engine: &TestOperations,
    volume: &Arc<dyn Volume>,
    copied_paths: &[PathBuf],
    created_dirs: &[PathBuf],
) -> CancelRollback {
    let mut ledger: Vec<WrittenFile> = Vec::new();
    for path in copied_paths {
        let size = volume.get_metadata(path).await.ok().and_then(|e| e.size).unwrap_or(0);
        ledger.push(WrittenFile::volume(path.clone(), size));
    }
    let events = CollectorEventSink::new();
    let state = make_state(engine);
    volume_rollback_with_progress(
        volume,
        &mut ledger,
        created_dirs,
        &events,
        "cleanup-tests-op",
        &WindDown::of(&state),
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
    let engine = TestOperations::new();
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    let report = roll_back(&engine, &volume, &[PathBuf::from("/album")], &[]).await;

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
    let engine = TestOperations::new();
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
        &engine,
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
    let engine = TestOperations::new();
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
    let state = make_state(&engine);
    let report = volume_rollback_with_progress(
        &volume,
        &mut ledger,
        &[],
        &events,
        "cleanup-tests-op",
        &WindDown::of(&state),
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
    let engine = TestOperations::new();
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
    let state = make_state(&engine);
    volume_rollback_with_progress(
        &volume,
        &mut ledger,
        &[],
        &events,
        "cleanup-tests-op",
        &WindDown::of(&state),
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

    let engine = TestOperations::new();

    let inner = Arc::new(InMemoryVolume::new("Dest"));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner
        .create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    roll_back(&engine, &volume, &[], &[PathBuf::from("/album")]).await;

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

    let engine = TestOperations::new();

    let inner = Arc::new(InMemoryVolume::new("Dest"));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner.create_directory(Path::new("/album/raw")).await.unwrap();
    let volume: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    // Creation order is shallowest-first; the prune walks it in reverse, so the
    // leaf empties before its parent is tried.
    roll_back(
        &engine,
        &volume,
        &[],
        &[PathBuf::from("/album"), PathBuf::from("/album/raw")],
    )
    .await;

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
    let engine = TestOperations::new();
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    let mut ledger = Vec::new();
    for i in 0..4 {
        let path = PathBuf::from(format!("/album/file{i}.bin"));
        vol.create_file(&path, b"payload").await.unwrap();
        ledger.push(WrittenFile::volume(path, 7));
    }
    let volume: Arc<dyn Volume> = vol.clone();

    let state = make_state(&engine);
    let guard = engine.register_state("volume-ledger-stop", Arc::clone(&state));
    // Rolling back, and the user stops the reversal before it starts deleting.
    engine.cancel_write_operation(guard.id(), true);
    engine.cancel_write_operation(guard.id(), false);
    let events = CollectorEventSink::new();
    let report = volume_rollback_with_progress(
        &volume,
        &mut ledger,
        &[],
        &events,
        guard.id().as_str(),
        &WindDown::of(&state),
        4,
        28,
        4,
        28,
    )
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
        TreeRemoval::UserChoseOverwriteAcrossTypes,
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
        TreeRemoval::UserChoseOverwriteAcrossTypes,
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

    let failure = remove_tree(&volume, Path::new("/tree"), TreeRemoval::UserChoseOverwriteAcrossTypes)
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
        TreeRemoval::UserChoseOverwriteAcrossTypes,
    )
    .await;
    assert!(r.is_ok(), "expected Ok, got {r:?}");
}

/// A staged partial the destination refuses to remove **comes back from the
/// sweep**, typed as refused, so the summary the user reads can account for it.
///
/// This is the honesty half of the abandoned-write path. A dropped write task
/// can leave its SMB2 handle open, the server answers the delete with a sharing
/// violation for as long as that session lives, and a user who chose Rollback
/// was being told the destination was clear while gigabytes of scratch sat on
/// their NAS.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_staged_partial_the_destination_keeps_comes_back_from_the_sweep() {
    let engine = TestOperations::new();
    let vol = Arc::new(InMemoryVolume::new("Dest").with_delete_failing());
    let volume: Arc<dyn Volume> = vol.clone();
    let state = make_state(&engine);
    let temp = PathBuf::from("/album/holiday.jpg.cmdr-tmp-4d1f9c");
    state.in_flight_temps.lock_ignore_poison().push(temp.clone());

    let leftovers = clean_abandoned_staged_writes(&volume, &state).await;

    assert_eq!(
        leftovers,
        vec![leftover(
            StagedLeftoverFate::Refused,
            "holiday.jpg.cmdr-tmp-4d1f9c",
            "Dest"
        )],
        "a temp the destination wouldn't take back has to reach the caller, named by what it's called there, \
         which is what the user would look for"
    );
    assert_eq!(
        CancelRollback::none()
            .with_staged_leftovers(leftovers.clone())
            .staged_leftovers,
        leftovers,
        "and the summary carries it"
    );
}

/// A sweep that got everything reports nothing, so the ordinary cancel stays
/// silent about scratch nobody has to think about. A temp that had already gone
/// counts as removed rather than sending the user hunting for a file that isn't
/// there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sweep_that_cleared_the_destination_reports_no_leftovers() {
    let engine = TestOperations::new();
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    let present = PathBuf::from("/album/holiday.jpg.cmdr-tmp-4d1f9c");
    vol.create_file(&present, b"half a photo").await.unwrap();
    let volume: Arc<dyn Volume> = vol.clone();
    let state = make_state(&engine);
    state.in_flight_temps.lock_ignore_poison().extend([
        present.clone(),
        PathBuf::from("/album/already-landed.jpg.cmdr-tmp-77aa10"),
    ]);

    let leftovers = clean_abandoned_staged_writes(&volume, &state).await;

    assert!(leftovers.is_empty(), "nothing was left, so there is nothing to report");
    assert!(!vol.exists(&present).await, "the staged partial itself still goes");
}

/// ❗ **A stop never waits on a share that's out** (the lead's decision: a
/// Cancel is reported the moment it happens, and a failure takes the same
/// shape). The cleanup returns at once, while the background pass's one dial is
/// still hanging, and says the temp goes once the share is back; then it does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_with_the_destination_out_reports_at_once_and_the_temp_goes_once_the_share_is_back() {
    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let (inner, nas, volume) = nas("cleanup-test-out-at-stop");
    let temp = PathBuf::from("/holiday.jpg.cmdr-tmp-5d2e");
    inner.create_file(&temp, b"half a photo").await.unwrap();
    let state = stopped_with_a_temp_on(&engine, &ledger, "cleanup-test-nas-out", &temp);
    nas.go_away();
    nas.hang_reconnects();

    let leftovers = tokio::time::timeout(Duration::from_secs(2), clean_abandoned_staged_writes(&volume, &state))
        .await
        .expect("the stop's cleanup never waits on a share that's out");

    assert_eq!(
        leftovers,
        vec![leftover(
            StagedLeftoverFate::RemovedWhenBack,
            "holiday.jpg.cmdr-tmp-5d2e",
            "Naspolya"
        )]
    );
    wait_until_async(WAIT, "the background pass to dial the share", || {
        nas.reconnect_attempts() >= 1
    })
    .await;
    assert!(inner.exists(&temp).await, "nothing is deleted while the share is out");

    nas.reconnected_elsewhere();
    engine.leftover_cleanups_settled().await;

    assert!(
        !inner.exists(&temp).await,
        "the pass removes the temp once the share is back"
    );
    assert!(!ledger.live_paths().contains(&temp), "and retires its record");
}

/// A temp the destination keeps refusing (smb2#24's handle, held until the
/// session ends) is asked again by the background pass, and then waits in
/// the ledger's pending set for that volume's next arrival or reconnect, by
/// which time the session that held it is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_the_destination_still_refuses_waits_in_the_ledger_for_its_next_reconnect() {
    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let vol = Arc::new(InMemoryVolume::new("Naspolya").with_delete_failing());
    let temp = PathBuf::from("/holiday.jpg.cmdr-tmp-6e3f");
    vol.create_file(&temp, b"half a photo").await.unwrap();
    let volume: Arc<dyn Volume> = vol.clone();
    let state = stopped_with_a_temp_on(&engine, &ledger, "cleanup-test-nas-refuses", &temp);

    let leftovers = clean_abandoned_staged_writes(&volume, &state).await;
    engine.leftover_cleanups_settled().await;

    assert_eq!(
        leftovers,
        vec![leftover(
            StagedLeftoverFate::Refused,
            "holiday.jpg.cmdr-tmp-6e3f",
            "Naspolya"
        )]
    );
    assert!(vol.exists(&temp).await, "a refused temp stays where it is");
    assert_eq!(
        ledger.pending_paths(),
        vec![temp],
        "and waits in the ledger for the volume's next arrival or reconnect"
    );
}

/// A refusal that clears a moment later (a dropped write's CLOSE going out late,
/// with no credit on hand) is taken by the background pass's first retry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_refused_once_goes_on_the_background_retry() {
    use super::super::faulty_volume::{FaultyOp, FaultyVolume};

    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    let temp = PathBuf::from("/holiday.jpg.cmdr-tmp-7f40");
    inner.create_file(&temp, b"half a photo").await.unwrap();
    let faulty = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(
            FaultyOp::Delete,
            1,
            VolumeError::IoError {
                message: "sharing violation".to_string(),
                raw_os_error: None,
            },
        )
        .arc();
    let volume: Arc<dyn Volume> = Arc::clone(&faulty) as Arc<dyn Volume>;
    let state = stopped_with_a_temp_on(&engine, &ledger, "cleanup-test-nas-refuses-once", &temp);

    let leftovers = clean_abandoned_staged_writes(&volume, &state).await;
    engine.leftover_cleanups_settled().await;

    assert!(faulty.fault_fired(FaultyOp::Delete), "the first delete was refused");
    assert_eq!(leftovers.len(), 1, "reported as refused at the stop: {leftovers:?}");
    assert!(!inner.exists(&temp).await, "and gone after the retry");
    assert!(!ledger.live_paths().contains(&temp), "its record retired");
}

/// ❗ Tier 2 (the quit deadline) ends the background pass where it is: nothing
/// goes back through the connection, and the record stays registered and on
/// disk for the startup sweep, ❌ not handed to the reconnect sweep.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_two_during_the_background_pass_leaves_the_record_for_the_startup_sweep() {
    let engine = TestOperations::new();
    let ledger = Ledger::for_test();
    let (inner, nas, volume) = nas("cleanup-test-tier-two");
    let temp = PathBuf::from("/holiday.jpg.cmdr-tmp-8a51");
    inner.create_file(&temp, b"half a photo").await.unwrap();
    let state = stopped_with_a_temp_on(&engine, &ledger, "cleanup-test-nas-tier-two", &temp);
    nas.go_away();
    nas.hang_reconnects();

    clean_abandoned_staged_writes(&volume, &state).await;
    wait_until_async(WAIT, "the background pass to dial the share", || {
        nas.reconnect_attempts() >= 1
    })
    .await;
    engine.abort_all_write_operations();
    engine.leftover_cleanups_settled().await;

    assert!(inner.exists(&temp).await, "nothing is deleted after tier 2");
    assert!(
        ledger.live_paths().contains(&temp),
        "the record stays for the startup sweep"
    );
    assert!(
        ledger.pending_paths().is_empty(),
        "and isn't handed to the reconnect sweep"
    );
}

/// A landing a stop left recorded (a complete file whose way may have been
/// cleared) is named by the file it's on its way to, as one that gets its name
/// once the share is back: ❌ never as scratch to remove.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kept_landing_is_reported_as_a_file_that_lands_once_the_share_is_back() {
    let engine = TestOperations::new();
    let (_inner, nas, volume) = nas("cleanup-test-kept-landing");
    nas.go_away();
    let state = make_state(&engine);
    state
        .kept_landings
        .lock_ignore_poison()
        .push(PathBuf::from("/album/holiday.jpg"));

    let leftovers = clean_abandoned_staged_writes(&volume, &state).await;

    assert_eq!(
        leftovers,
        vec![leftover(StagedLeftoverFate::LandsWhenBack, "holiday.jpg", "Naspolya")]
    );
}

/// Invariant 9: a rollback removes the Locked copy it made and the Locked folder
/// it made, though both refuse a plain removal, unlocking each only after
/// verifying it (`unblocked_removal.rs`). The file inside the Locked folder
/// needs the folder opened for its own removal.
#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_removes_a_locked_copy_and_a_locked_folder_it_made() {
    use cmdr_fs::testing::metadata_fixture::{LOCKED_FLAG, MetadataFixture, set_flags};

    let engine = TestOperations::new();

    let fixture = MetadataFixture::new("cleanup_locked_rollback");
    std::fs::write(fixture.join("loose.txt"), b"locked copy").unwrap();
    std::fs::create_dir(fixture.join("album")).unwrap();
    std::fs::write(fixture.join("album/photo.jpg"), b"inside a locked folder").unwrap();
    set_flags(&fixture.join("loose.txt"), LOCKED_FLAG);
    set_flags(&fixture.join("album"), LOCKED_FLAG);
    let volume: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "Local",
        &*fixture,
        crate::host::testing::pane_order_host(),
    ));

    roll_back(
        &engine,
        &volume,
        &[PathBuf::from("loose.txt"), PathBuf::from("album/photo.jpg")],
        &[PathBuf::from("album")],
    )
    .await;

    for name in ["loose.txt", "album/photo.jpg", "album"] {
        assert!(
            !fixture.join(name).exists(),
            "{name}: the rollback removes what the copy made, Locked or not"
        );
    }
}

/// ❗ Every staged temp wears the SOURCE's dates, so one holding committed data
/// (a landing a give-up or a crash ended after the way was cleared) reads as an
/// hour old the moment it's written. The reap goes by name and age, so the
/// ledger is what tells it apart: a path the ledger records, as a temp or a
/// landing, is a live write or committed data, and stays. An unrecorded one of
/// the same age is still reaped.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_stale_temp_reap_skips_what_the_ledger_records_however_old_it_looks() {
    use crate::in_flight_temps::{self, Ledger, RecordHome};
    const VOLUME_ID: &str = "cleanup-test-reap-nas";
    let engine = TestOperations::new();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("cleanup-test-source".to_string(), VOLUME_ID.to_string())
            .with_in_flight_ledger(Ledger::for_test()),
    );
    let volume = Arc::new(InMemoryVolume::new("nas"));
    let streaming = PathBuf::from("/streaming.mov.cmdr-tmp-1111");
    let landing = PathBuf::from("/landing.mov.cmdr-tmp-2222");
    let orphan = PathBuf::from("/orphan.mov.cmdr-tmp-3333");
    for temp in [&streaming, &landing, &orphan] {
        volume.create_file(temp, b"bytes").await.unwrap();
        // A year before the reap runs, the way a source's stamped date reads.
        volume.set_modified_at(temp, Some(1));
    }
    in_flight_temps::track_temp(&state, Some(RecordHome::volume_space(VOLUME_ID)), &streaming);
    in_flight_temps::track_temp(&state, Some(RecordHome::volume_space(VOLUME_ID)), &landing);
    in_flight_temps::hold_for_landing(
        &state,
        Some(RecordHome::volume_space(VOLUME_ID)),
        &landing,
        Path::new("/landing.mov"),
        5,
    )
    .await
    .expect("a ledger that keeps nothing on disk holds the landing in memory");
    let dest: Arc<dyn Volume> = Arc::clone(&volume) as Arc<dyn Volume>;

    let listing = reap_stale_transfer_temps(&state, &dest, Path::new("/"))
        .await
        .expect("the listing answers");

    assert!(
        volume.exists(&streaming).await,
        "a recorded temp is a live write, ❌ never reaped"
    );
    assert!(
        volume.exists(&landing).await,
        "a recorded landing may be the only copy, ❌ never reaped"
    );
    assert!(
        !volume.exists(&orphan).await,
        "an unrecorded temp that old is still a leftover, and goes"
    );
    let names: Vec<&str> = listing.iter().map(|entry| entry.name.as_str()).collect();
    assert!(
        names.contains(&"streaming.mov.cmdr-tmp-1111") && names.contains(&"landing.mov.cmdr-tmp-2222"),
        "and what the reap kept is still in the listing it hands back: {names:?}"
    );
}
