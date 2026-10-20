//! A delete on a share that drops mid-delete waits for it to come back and
//! finishes, through the real volume walker (`delete_volume_files_with_progress_inner`).
//!
//! The wait itself is pinned in `transfer/volume_outage_tests.rs`. These cells
//! pin what the delete does around it: a re-run re-checks the entry against
//! what the walk listed first, a `NotFound` after a wait counts as deleted (and
//! only after one), and an outage that ends the delete names the server.

use cmdr_operation_log::types::OperationId;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::volume_walker::delete_volume_files_with_progress_inner;
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::transfer::volume::{FaultyOp, FaultyVolume};
use crate::types::{WriteOperationConfig, WriteOperationError};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{BackendKind, InMemoryVolume, Volume, VolumeError};

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

fn outage_state(engine: &TestOperations, budget: Duration) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    )
}

/// A share holding `/dir/a.txt` ("aaaa") and `/dir/b.txt` ("bbbb"), with its
/// inner volume, so a cell can change the share behind the session's back.
async fn nas(lane: &str) -> (Arc<InMemoryVolume>, Arc<OutageVolume>) {
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    inner.create_directory(Path::new("/dir")).await.unwrap();
    inner.create_file(Path::new("/dir/a.txt"), b"aaaa").await.unwrap();
    inner.create_file(Path::new("/dir/b.txt"), b"bbbb").await.unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&inner), lane);
    (inner, nas)
}

/// Runs `then` once the delete has started waiting for the share.
fn when_waited_for(state: &Arc<WriteOperationState>, then: impl Future<Output = ()> + Send + 'static) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the delete to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        then.await;
    });
}

struct Run {
    result: Result<(), WriteOperationError>,
    sink: CollectorEventSink,
}

impl Run {
    fn files_deleted(&self) -> usize {
        let complete = self.sink.complete.lock_ignore_poison();
        assert_eq!(complete.len(), 1, "one completion: {:?}", self.result);
        complete[0].files_processed
    }
}

async fn delete(
    engine: &TestOperations,
    op_id: &str,
    state: &Arc<WriteOperationState>,
    volume: Arc<dyn Volume>,
    sources: &[&str],
) -> Run {
    let sink = CollectorEventSink::new();
    let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let _op = engine.register_state(op_id, Arc::clone(state));
    let result = delete_volume_files_with_progress_inner(
        volume,
        op_id,
        &sink,
        op_id,
        state,
        &sources,
        &WriteOperationConfig::default(),
    )
    .await;
    Run { result, sink }
}

async fn exists(inner: &InMemoryVolume, path: &str) -> bool {
    inner.exists(Path::new(path)).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_cut_before_the_server_saw_it_waits_and_deletes() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-cut-before").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::BeforeTheServerSeesIt);
    let control = Arc::clone(&nas);
    when_waited_for(&state, async move { control.become_reachable() });

    let run = delete(&engine, "delete-cut-before", &state, nas.clone(), &["/dir"]).await;

    assert!(run.result.is_ok(), "the delete waits and finishes: {:?}", run.result);
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(!exists(&inner, "/dir/b.txt").await);
    assert!(!exists(&inner, "/dir").await, "the folder goes once it's empty");
    assert_eq!(run.files_deleted(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_file_the_walk_only_probed_is_still_proven_before_its_re_run() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-top-level-file").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::BeforeTheServerSeesIt);
    let control = Arc::clone(&nas);
    when_waited_for(&state, async move { control.become_reachable() });

    let run = delete(&engine, "delete-top-level-file", &state, nas.clone(), &["/dir/b.txt"]).await;

    assert!(
        run.result.is_ok(),
        "the walk learned its size, so the re-run can prove it's the same file: {:?}",
        run.result
    );
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(!exists(&inner, "/dir/b.txt").await);
    assert!(exists(&inner, "/dir/a.txt").await, "only the selected file goes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_whose_reply_was_lost_counts_the_file_once_the_share_is_back() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-reply-lost").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::AfterTheServerPerformsIt);
    let control = Arc::clone(&nas);
    when_waited_for(&state, async move { control.become_reachable() });

    let run = delete(&engine, "delete-reply-lost", &state, nas.clone(), &["/dir"]).await;

    assert!(
        run.result.is_ok(),
        "a NotFound after the wait is the delete that went through: {:?}",
        run.result
    );
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(!exists(&inner, "/dir").await);
    assert_eq!(run.files_deleted(), 2, "the file the lost reply was about counts once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_put_there_during_the_outage_is_left_and_reported() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-replaced").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::AfterTheServerPerformsIt);
    let (control, someone_else) = (Arc::clone(&nas), Arc::clone(&inner));
    when_waited_for(&state, async move {
        // Another client saves a different file under the same name while
        // Cmdr can't see the share.
        someone_else
            .create_file(Path::new("/dir/b.txt"), b"someone else's")
            .await
            .expect("the other client's save");
        control.become_reachable();
    });

    let run = delete(&engine, "delete-replaced", &state, nas.clone(), &["/dir"]).await;

    let Err(WriteOperationError::ChangedWhileDisconnected { path, volume_name }) = &run.result else {
        panic!("a changed file stops the delete and says so, got {:?}", run.result);
    };
    assert_eq!(path, "/dir/b.txt");
    assert_eq!(volume_name, "Naspolya");
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(exists(&inner, "/dir/b.txt").await, "the other client's file stays");
    assert_eq!(
        run.sink.errors.lock_ignore_poison().len(),
        0,
        "the walker returns it; the op body emits it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_became_a_folder_during_the_outage_is_left_with_everything_in_it() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-became-folder").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::AfterTheServerPerformsIt);
    let (control, someone_else) = (Arc::clone(&nas), Arc::clone(&inner));
    when_waited_for(&state, async move {
        someone_else
            .create_directory(Path::new("/dir/b.txt"))
            .await
            .expect("the other client's folder");
        someone_else
            .create_file(Path::new("/dir/b.txt/keep.txt"), b"keep")
            .await
            .expect("the other client's file in it");
        control.become_reachable();
    });

    let run = delete(&engine, "delete-became-folder", &state, nas.clone(), &["/dir"]).await;

    assert!(
        matches!(&run.result, Err(WriteOperationError::ChangedWhileDisconnected { path, .. }) if path == "/dir/b.txt"),
        "a different kind at the path is a changed entry, got {:?}",
        run.result
    );
    assert!(
        exists(&inner, "/dir/b.txt/keep.txt").await,
        "a re-run never escalates to a recursive delete"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_outage_that_outlasts_the_budget_names_the_server() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, Duration::from_millis(300));
    let (inner, nas) = nas("delete-gives-up").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::BeforeTheServerSeesIt);

    let run = delete(&engine, "delete-gives-up", &state, nas.clone(), &["/dir"]).await;

    let Err(WriteOperationError::ServerConnectionLost {
        path,
        volume_name,
        gave_up_after_secs,
    }) = &run.result
    else {
        panic!("a give-up reports the lost connection, got {:?}", run.result);
    };
    assert_eq!(path, "/dir/b.txt", "the file is context");
    assert_eq!(volume_name, "Naspolya");
    assert!(gave_up_after_secs.is_some(), "it says it waited");
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(exists(&inner, "/dir/b.txt").await, "the file the drop cut off stays");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_late_error_on_a_delete_re_checks_without_waiting() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-stale").await;
    nas.stale_error_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::AfterTheServerPerformsIt);

    let run = delete(&engine, "delete-stale", &state, nas.clone(), &["/dir"]).await;

    assert!(
        run.result.is_ok(),
        "the late error is the old session's: {:?}",
        run.result
    );
    assert_eq!(nas.reconnect_attempts(), 0, "someone else had already reconnected it");
    assert_eq!(nas.generation(), 1, "the late error really came across a reconnect");
    assert!(!exists(&inner, "/dir").await);
    assert_eq!(run.files_deleted(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_wait_ends_the_delete_at_once() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-cancel-wait").await;
    nas.drop_at_delete_of(Path::new("/dir/b.txt"), RenameDrop::BeforeTheServerSeesIt);
    let canceller = crate::Operations::clone(&engine);
    when_waited_for(&state, async move {
        canceller.cancel_write_operation(&OperationId::from("delete-cancel-wait"), false);
    });

    let run = delete(&engine, "delete-cancel-wait", &state, nas.clone(), &["/dir"]).await;

    assert!(
        matches!(run.result, Err(WriteOperationError::Cancelled { .. })),
        "got {:?}",
        run.result
    );
    assert_eq!(run.sink.cancelled.lock_ignore_poison().len(), 1, "one write-cancelled");
    assert!(exists(&inner, "/dir/b.txt").await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_delete_the_drop_cut_off_waits_and_removes_the_folder() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-folder-cut").await;
    nas.drop_at_delete_of(Path::new("/dir"), RenameDrop::BeforeTheServerSeesIt);
    let control = Arc::clone(&nas);
    when_waited_for(&state, async move { control.become_reachable() });

    let run = delete(&engine, "delete-folder-cut", &state, nas.clone(), &["/dir"]).await;

    assert!(run.result.is_ok(), "{:?}", run.result);
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(!exists(&inner, "/dir").await, "the folder doesn't stay behind silently");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_pass_the_outage_outlasts_is_reported_and_not_swallowed() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, Duration::from_millis(300));
    let (inner, nas) = nas("delete-folder-gives-up").await;
    nas.drop_at_delete_of(Path::new("/dir"), RenameDrop::BeforeTheServerSeesIt);

    let run = delete(&engine, "delete-folder-gives-up", &state, nas.clone(), &["/dir"]).await;

    assert!(
        matches!(&run.result, Err(WriteOperationError::ServerConnectionLost { path, .. }) if path == "/dir"),
        "got {:?}",
        run.result
    );
    assert!(exists(&inner, "/dir").await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_started_while_the_share_is_out_waits_for_it_before_the_walk() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-start-out").await;
    nas.go_away();
    let control = Arc::clone(&nas);
    when_waited_for(&state, async move { control.become_reachable() });

    let run = delete(&engine, "delete-start-out", &state, nas.clone(), &["/dir"]).await;

    assert!(
        run.result.is_ok(),
        "the walk's reads wait like any other: {:?}",
        run.result
    );
    assert!(nas.reconnect_attempts() > 0);
    assert!(!exists(&inner, "/dir").await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_delete_on_a_live_share_fails_at_once() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-refused").await;
    let faulty = FaultyVolume::wrapping(Arc::clone(&nas))
        .failing_call_at(
            FaultyOp::Delete,
            "/dir/b.txt",
            1,
            VolumeError::PermissionDenied {
                path: "/dir/b.txt".to_owned(),
                raw_os_error: Some(libc::EACCES),
            },
        )
        .arc();

    let run = delete(&engine, "delete-refused", &state, faulty.clone(), &["/dir"]).await;

    assert!(faulty.fault_fired(FaultyOp::Delete));
    assert!(
        matches!(&run.result, Err(WriteOperationError::PermissionDenied { path, .. }) if path == "/dir/b.txt"),
        "a refusal is reported as it always was, got {:?}",
        run.result
    );
    assert_eq!(nas.reconnect_attempts(), 0, "nothing to wait for");
    assert!(exists(&inner, "/dir/b.txt").await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_first_try_not_found_keeps_failing_the_delete() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (_inner, nas) = nas("delete-first-not-found").await;
    let faulty = FaultyVolume::wrapping(Arc::clone(&nas))
        .failing_call_at(
            FaultyOp::Delete,
            "/dir/b.txt",
            1,
            VolumeError::NotFound("/dir/b.txt".to_owned()),
        )
        .arc();

    let run = delete(&engine, "delete-first-not-found", &state, faulty.clone(), &["/dir"]).await;

    assert!(faulty.fault_fired(FaultyOp::Delete));
    assert!(
        run.result.is_err(),
        "without a wait, a NotFound means what it always did"
    );
    assert_eq!(nas.reconnect_attempts(), 0);
}

/// The share above, counting every stat-shaped request it answers: an object
/// store pays one HEAD for each (`cmdr-s3`'s cost estimate counts them).
struct CountingStats {
    inner: Arc<OutageVolume>,
    stats: std::sync::atomic::AtomicUsize,
}

impl Volume for CountingStats {
    crate::forward_volume_methods!(inner =>
        name, root, lane_key, connection_state, connection_generation, attempt_reconnect,
        list_directory, exists, delete,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn backend_kind(&self) -> BackendKind {
        BackendKind::S3
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<cmdr_fs::entry::FileEntry, VolumeError>> + Send + 'a>> {
        self.stats.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get_metadata(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.stats.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.is_directory(path)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_file_costs_one_stat_that_also_proves_it_for_a_re_run() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-one-stat").await;
    let counting = Arc::new(CountingStats {
        inner: Arc::clone(&nas),
        stats: std::sync::atomic::AtomicUsize::new(0),
    });

    let run = delete(&engine, "delete-one-stat", &state, counting.clone(), &["/dir/b.txt"]).await;

    assert!(run.result.is_ok(), "{:?}", run.result);
    assert!(!exists(&inner, "/dir/b.txt").await);
    assert_eq!(
        counting.stats.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the type probe's own stat carries the size: no second request per file"
    );
}

/// [`CountingStats`] with its listings watched, so a pane's listing answers.
struct WatchedCountingStats {
    inner: CountingStats,
}

impl Volume for WatchedCountingStats {
    crate::forward_volume_methods!(inner =>
        name, root, lane_key, connection_state, connection_generation, attempt_reconnect,
        list_directory, exists, delete, get_metadata, is_directory,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn backend_kind(&self) -> BackendKind {
        BackendKind::S3
    }
    fn listing_watch_coverage(&self, _path: &Path) -> cmdr_fs::volume::WatchCoverage {
        cmdr_fs::volume::WatchCoverage::EveryWriter
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_file_its_pane_listing_describes_costs_no_stat() {
    let panes = crate::host::testing::PaneListings::new();
    let engine = TestOperations::seeing(&panes);
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-pane-hint").await;
    let watched = Arc::new(WatchedCountingStats {
        inner: CountingStats {
            inner: Arc::clone(&nas),
            stats: std::sync::atomic::AtomicUsize::new(0),
        },
    });
    let volume_id = "delete-pane-hint";
    engine.volumes().register(volume_id, watched.clone() as Arc<dyn Volume>);
    panes.show_volume(volume_id, watched.clone() as Arc<dyn Volume>);
    let mut listed = cmdr_fs::entry::FileEntry::new("b.txt".to_string(), "/dir/b.txt".to_string(), false, false);
    listed.size = Some(4);
    let _listing = cmdr_listing::testing::TestListing::new()
        .volume(volume_id)
        .path("/dir")
        .entries(vec![listed])
        .sequence(1)
        .insert(panes.panes(), "delete-pane-hint");

    let run = delete(&engine, volume_id, &state, watched.clone(), &["/dir/b.txt"]).await;

    assert!(run.result.is_ok(), "{:?}", run.result);
    assert!(!exists(&inner, "/dir/b.txt").await);
    assert_eq!(
        watched.inner.stats.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "the pane's listing carries the kind AND the size: no request to learn either"
    );
}

/// An object store's double: the share above, deleting files in batches.
struct Batching {
    inner: Arc<OutageVolume>,
}

impl Volume for Batching {
    crate::forward_volume_methods!(inner =>
        name, root, lane_key, connection_state, connection_generation, attempt_reconnect,
        list_directory, get_metadata, exists, is_directory, delete,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn backend_kind(&self) -> BackendKind {
        BackendKind::S3
    }
    fn delete_batch_size(&self) -> Option<std::num::NonZeroUsize> {
        std::num::NonZeroUsize::new(10)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_batch_the_drop_cut_off_waits_and_deletes_what_it_missed() {
    let engine = TestOperations::new();
    let state = outage_state(&engine, WAIT);
    let (inner, nas) = nas("delete-batch-cut").await;
    nas.drop_at_delete_of(Path::new("/dir/a.txt"), RenameDrop::BeforeTheServerSeesIt);
    let control = Arc::clone(&nas);
    when_waited_for(&state, async move { control.become_reachable() });
    let batching: Arc<dyn Volume> = Arc::new(Batching {
        inner: Arc::clone(&nas),
    });

    let run = delete(&engine, "delete-batch-cut", &state, batching, &["/dir"]).await;

    assert!(run.result.is_ok(), "{:?}", run.result);
    assert!(nas.reconnect_attempts() > 0, "the drop happened and was waited out");
    assert!(!exists(&inner, "/dir").await);
    assert_eq!(run.files_deleted(), 2);
}
