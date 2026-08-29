//! The scan-wait, end to end: a confirmed transfer gets its operation record
//! before its preview has finished walking, waits for it inside its own task,
//! and consumes the result instead of re-walking.
//!
//! Every test drives the real `*_start` entry points through the real manager,
//! because the whole point of this seam is what the manager, the preview map,
//! and the operation task agree on. The manager is a process-global singleton,
//! so ids and lane keys are unique per test.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::CollectorEventSink;
use super::manager::{PauseOutcome, list_operations, manager, pause_operation, resume_operation};
use super::scan_bridge::observed_scan_ticks;
use super::scan_cache::{
    CachedScanResult, ScanOutcome, ScanPreviewState, claim_preview, register_preview, settle_preview,
    take_cached_scan_result,
};
use super::types::{LifecycleStatus, WriteOperationConfig, WriteOperationError, WriteOperationType};
use super::{OperationEventSink, copy_files_start};
use crate::file_system::volume::CopyScanResult;
use crate::operation_log::types::Initiator;
use crate::test_support::{TestDir, wait_until_async};

/// Deadline for every wait here. Generous on purpose: each wait has a real
/// condition, so a healthy run satisfies it in microseconds.
const WAIT: Duration = Duration::from_secs(5);

fn unique(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("scanwait-{label}-{n}-{:?}", std::thread::current().id())
}

/// Registers a preview that is still walking, the state a `TransferDialog`
/// leaves behind when the user confirms early.
fn register_in_flight(preview_id: &str) {
    register_preview(
        preview_id.to_string(),
        Arc::new(ScanPreviewState {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            progress_interval: Duration::from_millis(50),
        }),
    );
}

/// A completed local-walk result over exactly `files`, in the shape
/// `run_scan_preview` publishes.
fn completed_result(sources: Vec<PathBuf>, files: &[PathBuf], source_root: &std::path::Path) -> CachedScanResult {
    let mut infos = Vec::new();
    let mut total = 0u64;
    for file in files {
        let metadata = std::fs::symlink_metadata(file).expect("fixture file exists");
        total += metadata.len();
        infos.push(super::state::FileInfo::new(
            file.clone(),
            source_root.to_path_buf(),
            &metadata,
        ));
    }
    let per_path = sources
        .iter()
        .map(|source| {
            (
                source.clone(),
                CopyScanResult {
                    file_count: files.len(),
                    dir_count: 0,
                    total_bytes: total,
                    dedup_bytes: total,
                    top_level_is_directory: false,
                },
            )
        })
        .collect();
    CachedScanResult::from_local_walk(sources, infos, Vec::new(), total, total, per_path, None)
}

// ============================================================================
// The wait, and what it buys
// ============================================================================

/// A transfer confirmed while its preview is still walking is a real operation
/// from the first frame: `list_operations` names it before the preview settles.
/// This is the user-visible bug — without a record there is no queue row, no
/// Pause, no Background, and ⌘Q walks straight past it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transfer_confirmed_mid_scan_is_registered_before_its_preview_settles() {
    let dir = TestDir::new("scanwait-registers");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    let file = src.join("a.bin");
    std::fs::write(&file, b"a").expect("write a");

    let preview_id = unique("preview");
    register_in_flight(&preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = copy_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![file.clone()],
        dst.clone(),
        WriteOperationConfig {
            preview_id: Some(preview_id.clone()),
            ..WriteOperationConfig::default()
        },
        vec![unique("vol")],
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("copy starts");

    let op_id = start.operation_id.clone();
    let snapshot = list_operations();
    let row = snapshot
        .iter()
        .find(|op| op.operation_id == op_id)
        .expect("the confirmed transfer must be a queue row while its preview still walks");
    assert_eq!(
        row.status,
        LifecycleStatus::Running,
        "a scanning transfer reuses Running; a status of its own would degrade silently everywhere"
    );
    assert!(
        manager().is_in_scan_wait(&op_id),
        "the record must know it is parked on its preview"
    );

    // Let it finish so the lane frees for the rest of the suite.
    settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![file.clone()], &[file], &src)),
    );
    wait_until_async(WAIT, "the copy to settle", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;
}

/// The guard against the frontend half shipping alone: with the scan-wait gone
/// from the dialog, an operation that does NOT wait dispatches immediately,
/// finds nothing in the cache, and silently re-walks the whole tree. Nothing
/// else catches that, so this asserts on what the operation acted on.
///
/// The preview describes ONE of the two files on disk. Consuming it copies one
/// file; re-walking copies both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_operation_consumes_its_previews_result_rather_than_re_walking() {
    let dir = TestDir::new("scanwait-consumes");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    let named = src.join("named.bin");
    let extra = src.join("extra.bin");
    std::fs::write(&named, b"named").expect("write named");
    std::fs::write(&extra, b"extra").expect("write extra");

    let preview_id = unique("preview");
    register_in_flight(&preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = copy_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![src.clone()],
        dst.clone(),
        WriteOperationConfig {
            preview_id: Some(preview_id.clone()),
            ..WriteOperationConfig::default()
        },
        vec![unique("vol")],
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("copy starts");
    let op_id = start.operation_id.clone();

    // The operation is parked: nothing copied yet, because nothing has told it
    // what to copy.
    assert!(
        manager().is_in_scan_wait(&op_id),
        "the operation must be waiting, not walking"
    );

    settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], std::slice::from_ref(&named), &dir)),
    );

    wait_until_async(WAIT, "the copy to complete", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;

    assert!(
        dst.join("src/named.bin").exists(),
        "the file the preview listed must be copied"
    );
    assert!(
        !dst.join("src/extra.bin").exists(),
        "a second walk is the regression this test exists to catch: the operation must act on the preview's result"
    );
}

// ============================================================================
// The terminal-outcome contract
// ============================================================================

/// A preview that errored fails its operation, carrying the walk's own message.
/// Unimplementable against a bare "the preview finished" pulse, which is how
/// this pins the outcome contract rather than a completion signal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_preview_fails_its_operation() {
    let (events, op_id, preview_id, _dir) = start_copy_awaiting_preview("scanwait-error").await;

    settle_preview(&preview_id, ScanOutcome::Error("Permission denied".to_string()), None);

    wait_until_async(WAIT, "the write-error event", || {
        !events.errors.lock().expect("collector mutex").is_empty()
    })
    .await;
    let errors = events.errors.lock().expect("collector mutex");
    let error = errors.first().expect("one error");
    assert_eq!(error.operation_id, op_id);
    assert!(
        matches!(&error.error, WriteOperationError::IoError { message, .. } if message == "Permission denied"),
        "the walk's own message must reach the operation, got {:?}",
        error.error
    );
}

/// A cancelled preview reaches its operation as `write-cancelled`.
///
/// ⚠️ The distinction is not cosmetic. Both scan workers unwind a cancelled walk
/// through their ERROR arm (the local walk's `on_cancelled` string, the volume
/// path's `"Scan failed: {VolumeError::Cancelled}"`), so an implementation that
/// classifies on the event instead of the worker's cancel flag reports a user's
/// cancel as a failure, and recovering the truth would mean matching on the
/// message.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_preview_cancels_its_operation_rather_than_failing_it() {
    let (events, op_id, preview_id, _dir) = start_copy_awaiting_preview("scanwait-cancel").await;

    settle_preview(&preview_id, ScanOutcome::Cancelled, None);

    wait_until_async(WAIT, "the write-cancelled event", || {
        !events.cancelled.lock().expect("collector mutex").is_empty()
    })
    .await;
    assert_eq!(
        events
            .cancelled
            .lock()
            .expect("collector mutex")
            .first()
            .map(|e| e.operation_id.clone()),
        Some(op_id),
    );
    assert!(
        events.errors.lock().expect("collector mutex").is_empty(),
        "a cancel must never surface as a failure"
    );
}

/// A `previewId` naming nothing (evicted, or a stale id from a reloaded window)
/// falls through to the operation's own walk. Never a hang.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_preview_id_falls_back_to_the_operations_own_walk() {
    let dir = TestDir::new("scanwait-unknown");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    std::fs::write(src.join("a.bin"), b"a").expect("write a");

    let events = Arc::new(CollectorEventSink::new());
    let start = copy_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![src.clone()],
        dst.clone(),
        WriteOperationConfig {
            preview_id: Some(unique("never-existed")),
            ..WriteOperationConfig::default()
        },
        vec![unique("vol")],
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("copy starts");

    assert!(
        !manager().is_in_scan_wait(&start.operation_id),
        "there is nothing to wait for"
    );
    wait_until_async(WAIT, "the copy to complete on its own walk", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;
    assert!(dst.join("src/a.bin").exists(), "the foolproof re-scan must still copy");
}

/// Two operations naming one preview: the second is refused the claim and walks
/// for itself. Sharing would race two consumers for a result
/// `take_cached_scan_result` hands out exactly once, and the loser would
/// silently get nothing. The archive-password retry re-dispatches over the same
/// sources, so this is a real path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_operation_naming_a_claimed_preview_is_refused() {
    let preview_id = unique("shared-preview");
    register_in_flight(&preview_id);

    let first = unique("owner");
    let second = unique("interloper");
    assert!(
        matches!(
            claim_preview(&preview_id, &first),
            super::scan_cache::PreviewClaim::Waiting
        ),
        "the first claimant owns it"
    );
    assert!(
        matches!(
            claim_preview(&preview_id, &second),
            super::scan_cache::PreviewClaim::Refused
        ),
        "a second operation must be refused, never share"
    );

    super::scan_cache::abandon_claim(&preview_id);
}

/// A claimed result survives a TTL sweep triggered by a later settle. With
/// `LANE_BUDGET = 1` an operation can sit Queued well past the five-minute TTL,
/// so evicting its result would silently downgrade the ordinary busy-lane case
/// to a re-walk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claimed_result_survives_a_ttl_sweep() {
    let dir = TestDir::new("scanwait-ttl");
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("create src");
    let file = src.join("a.bin");
    std::fs::write(&file, b"a").expect("write a");

    let claimed = unique("claimed");
    let owner = unique("owner");
    register_in_flight(&claimed);
    claim_preview(&claimed, &owner);
    settle_preview(
        &claimed,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], std::slice::from_ref(&file), &src)),
    );

    // Age it past the TTL, then trigger a sweep by settling an unrelated
    // preview. `SCAN_RESULT_TTL` is measured from the settle, so the sweep is
    // driven by a stub whose age the test controls.
    super::scan_cache::age_settled_entry_for_test(
        &claimed,
        super::scan_cache::SCAN_RESULT_TTL + Duration::from_secs(1),
    );
    let other = unique("other");
    register_in_flight(&other);
    settle_preview(&other, ScanOutcome::Cancelled, None);

    assert!(
        take_cached_scan_result(&claimed, &[src]).is_some(),
        "a claimed result must outlive the sweep: its owner may still be waiting on a lane"
    );
}

// ============================================================================
// The progress bridge
// ============================================================================

/// The bridge republishes the walk's counts under the owner's id, and stops
/// once the wait is over. Without it every scan-phase surface renders zeros:
/// `scan-preview-progress` carries no `operationId`, and nothing else emits for
/// an operation that is only waiting.
///
/// The ORDERING is the load-bearing half. `spawn_managed` inserts the record,
/// admits, and only THEN broadcasts `operations-changed`; the frontend store
/// drops progress for an id it has no row for. A tick that beats its own
/// snapshot is indistinguishable from no tick at all, so the assertion is on
/// the broadcast counter, not on the tick's mere existence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bridge_ticks_after_the_snapshot_that_first_carries_the_row() {
    let emits_before_registration = manager().emit_count();
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview("scanwait-bridge").await;

    let ticks = observed_scan_ticks(&op_id);
    let first = ticks.first().expect("the row must get its opening tick");
    assert!(
        first.emits_before > emits_before_registration,
        "the tick must land after the operations-changed that first carries the row \
         (tick saw {} broadcasts, registration started at {})",
        first.emits_before,
        emits_before_registration
    );

    // A live preview tick reaches the operation too.
    super::scan_bridge::forward_scan_progress(
        &preview_id,
        super::scan_bridge::ScanCounts {
            files_found: 12,
            dirs_found: 3,
            bytes_found: 4_096,
            ..Default::default()
        },
    );
    let ticks = observed_scan_ticks(&op_id);
    assert_eq!(
        ticks.last().map(|t| (t.files_done, t.bytes_done)),
        Some((12, 4_096)),
        "a claimed preview's counts must ride the operation's own progress stream"
    );

    let src = dir.join("src");
    let file = src.join("a.bin");
    settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], &[file], &src)),
    );
    wait_until_async(WAIT, "the copy to complete", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;

    // The claim is gone, so the bridge has nothing to forward to.
    let before = observed_scan_ticks(&op_id).len();
    super::scan_bridge::forward_scan_progress(
        &preview_id,
        super::scan_bridge::ScanCounts {
            files_found: 99,
            ..Default::default()
        },
    );
    assert_eq!(
        observed_scan_ticks(&op_id).len(),
        before,
        "the bridge must go quiet once the wait is over"
    );
}

// ============================================================================
// Pause during the scan-wait
// ============================================================================

/// Pause is refused while an operation is parked on its preview: it holds its
/// lane and writes nothing, so flipping it to `Paused` would say the walk had
/// stopped when it had not. The refusal reports itself as `Deferred`, which is
/// what lets a surface say "not yet, but it's coming" rather than either lie.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_is_declined_during_the_scan_wait() {
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview("scanwait-pause").await;

    assert_eq!(
        pause_operation(&op_id),
        PauseOutcome::Deferred,
        "a scan-waiting op can't park, and the request is latched rather than lost"
    );

    assert_eq!(
        manager().lifecycle_status(&op_id),
        Some(LifecycleStatus::Running),
        "the record must stay Running: nothing has parked"
    );

    // Withdrawing before the wait ends clears the latch, so the operation runs
    // on rather than parking the moment its scan lands.
    resume_operation(&op_id);

    let src = dir.join("src");
    let file = src.join("a.bin");
    settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], &[file], &src)),
    );
    wait_until_async(WAIT, "the copy to settle", || {
        !events.complete.lock().expect("collector mutex").is_empty()
            || !events.cancelled.lock().expect("collector mutex").is_empty()
    })
    .await;
}

/// The one that matters: a "Pause all" issued mid-scan is LATCHED and applied
/// the moment the wait ends. A bare refusal looks correct and silently loses
/// the request, and that one operation then writes at full speed while every
/// other one is paused and the user believes the device is free. That is
/// precisely the scenario pause exists for, which makes losing it worse than
/// never offering it.
///
/// ⚠️ It drives `pause_all`'s two halves rather than `pause_all` itself: the
/// manager is a process-global singleton, so a real `pause_all` here would park
/// every other test's operation too. The halves are the whole of it —
/// `running_ids()` decides who is asked, `pause_operation` per id is the ask —
/// so asserting membership plus the per-id behavior covers the same ground
/// without reaching across the suite.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pause_during_a_scan_wait_latches_and_lands_before_the_first_write() {
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview("scanwait-pauselatch").await;

    assert!(
        manager().running_ids().contains(&op_id),
        "`pause_all` walks the running set, so a scan-waiting op has to be in it"
    );

    // The ask `pause_all` makes, per id. Nothing parks: a scan-wait has nothing
    // to park, and flipping the record would claim the walk had stopped.
    assert_eq!(pause_operation(&op_id), PauseOutcome::Deferred);
    assert_eq!(
        manager().lifecycle_status(&op_id),
        Some(LifecycleStatus::Running),
        "a scan-wait can't park, so the refusal is honest"
    );

    let src = dir.join("src");
    let file = src.join("a.bin");
    settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], &[file], &src)),
    );

    wait_until_async(WAIT, "the latched pause to land", || {
        manager().lifecycle_status(&op_id) == Some(LifecycleStatus::Paused)
            || !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;
    assert_eq!(
        manager().lifecycle_status(&op_id),
        Some(LifecycleStatus::Paused),
        "the deferred pause must land before the operation writes a byte, \
         not be dropped on the floor by the refusal"
    );

    // Release it so the lane frees for the rest of the suite.
    resume_operation(&op_id);
    wait_until_async(WAIT, "the copy to settle", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;
}

/// Trash is the ONE operation that doesn't wait, and it frees the preview
/// instead. `trashItemAtURL` is atomic per top-level item, so a trash walks
/// nothing: there is no second walk to serialize against and no cached result
/// to consume, and waiting would be pure delay — a long one on a big tree.
/// Leaving the preview alone isn't an option either, because the dialog
/// deliberately skips its own cleanup after a confirm (on the DELETE path the
/// operation does consume it), so an ownerless walk would run for nobody.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trash_frees_its_preview_instead_of_waiting_on_it() {
    use super::trash_files_start;

    let dir = TestDir::new("scanwait-trash");
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("create src");
    let file = src.join("a.bin");
    std::fs::write(&file, b"a").expect("write a");

    let preview_id = unique("preview");
    register_in_flight(&preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = trash_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![file.clone()],
        None,
        WriteOperationConfig {
            preview_id: Some(preview_id.clone()),
            ..WriteOperationConfig::default()
        },
        Initiator::User,
        None,
    )
    .await
    .expect("trash starts");

    assert!(
        !manager().is_in_scan_wait(&start.operation_id),
        "a trash must not park on a scan it will never read"
    );
    assert!(
        matches!(
            claim_preview(&preview_id, &unique("anyone")),
            super::scan_cache::PreviewClaim::Unknown
        ),
        "the preview must be freed, not left walking for an operation that ignores it"
    );

    wait_until_async(WAIT, "the trash to settle", || {
        !events.complete.lock().expect("collector mutex").is_empty()
            || !events.errors.lock().expect("collector mutex").is_empty()
    })
    .await;
}

// ============================================================================
// The archive routes
// ============================================================================

/// A Compress confirmed mid-scan awaits its preview instead of walking
/// concurrently with it.
///
/// The archive changeset planner runs its own `WalkDir` and never consumes a
/// cached result, so awaiting buys serialization, not reuse: today the
/// frontend's scan-wait provides that serialization, and deleting it without
/// threading `preview_id` through this route would put ⌥F5's sampling preview
/// and the planner's walk down the same tree at once. The copy-path tests all
/// pass without the threading, so this is the one that catches it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_compress_confirmed_mid_scan_awaits_its_preview() {
    use super::archive_edit::compress_start;
    use crate::file_system::volume::Volume;
    use crate::file_system::volume::backends::LocalPosixVolume;

    let dir = TestDir::new("scanwait-compress");
    let src_root = dir.join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let dest = dir.join("bundle.zip");

    let preview_id = unique("preview");
    register_in_flight(&preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt")],
        dest.clone(),
        unique("lane"),
        super::types::ConflictResolution::Overwrite,
        0,
        None,
        Some(preview_id.clone()),
        Initiator::User,
    )
    .await
    .expect("start compress");

    assert!(
        manager().is_in_scan_wait(&start.operation_id),
        "the compress must be parked on its preview, not planning its changeset alongside it"
    );

    settle_preview(&preview_id, ScanOutcome::Complete, None);
    wait_until_async(WAIT, "the compress to complete", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;
}

// ============================================================================
// Leaks
// ============================================================================

/// A queued operation cancelled before admission frees its preview.
///
/// `cancel_if_queued` drops the record without ever running its
/// `DeferredStart`, which is where the wait (and its cleanup) lives — so
/// without an explicit hook the walk keeps going for an operation that no
/// longer exists, and its result (tens of thousands of `FileInfo`) sits until a
/// TTL sweep. On a busy lane, cancelling a queued op is the ordinary case.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_queued_operation_frees_the_preview_it_claimed() {
    use super::manager::cancel_operation;

    let lane = unique("busy-lane");
    let preview_id = unique("preview");
    register_in_flight(&preview_id);

    // Occupy the lane so the op under test is admitted as Queued and its
    // deferred start never runs.
    let (blocker_started, blocker_started_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = tokio::sync::oneshot::channel();
    let blocker_id = unique("blocker");
    manager().spawn_managed(
        descriptor_on_lane(&blocker_id, &lane, None),
        Arc::new(super::state::WriteOperationState::new(Duration::from_millis(10))),
        Box::new({
            let blocker_id = blocker_id.clone();
            move || {
                Box::pin(async move {
                    let guard = super::manager::ManagedTaskGuard::new(blocker_id.clone());
                    let _ = blocker_started.send(());
                    let _ = release_rx.await;
                    guard.disarm();
                    manager().on_settled(&blocker_id);
                })
            }
        }),
    );
    blocker_started_rx.await.expect("the blocker holds the lane");

    let queued_id = unique("queued");
    manager().spawn_managed(
        descriptor_on_lane(&queued_id, &lane, Some(preview_id.clone())),
        Arc::new(super::state::WriteOperationState::new(Duration::from_millis(10))),
        Box::new(|| Box::pin(async {})),
    );
    assert_eq!(
        manager().lifecycle_status(&queued_id),
        Some(LifecycleStatus::Queued),
        "the second op must be waiting on the busy lane"
    );
    assert!(
        matches!(
            claim_preview(&preview_id, "someone-else"),
            super::scan_cache::PreviewClaim::Refused
        ),
        "the queued op owns the preview while it waits"
    );

    cancel_operation(&queued_id);

    assert!(
        matches!(
            claim_preview(&preview_id, "someone-else"),
            super::scan_cache::PreviewClaim::Unknown
        ),
        "cancelling a queued op must free its preview, not leave the walk running for nobody"
    );

    let _ = release.send(());
    wait_until_async(WAIT, "the blocker to settle", || {
        list_operations().iter().all(|op| op.operation_id != blocker_id)
    })
    .await;
}

// ============================================================================
// The quit gate
// ============================================================================

/// ⌘Q during a scan holds, where today it walks straight past. A scan-waiting
/// transfer is an ordinary `Running` `Copy` row, which is exactly why reusing
/// `Running` (rather than minting a status of its own) makes the quit gate
/// correct with no edit at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scanning_transfer_holds_a_quit() {
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview("scanwait-quit").await;

    let snapshot = list_operations();
    let row = snapshot.iter().find(|op| op.operation_id == op_id).expect("the row");
    assert!(
        crate::quit::blocks_quit(row),
        "confirmed work a user is watching must hold a quit, even while it is only counting"
    );

    let src = dir.join("src");
    let file = src.join("a.bin");
    settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], &[file], &src)),
    );
    wait_until_async(WAIT, "the copy to settle", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;
}

// ============================================================================
// Helpers
// ============================================================================

/// A minimal copy descriptor on one lane, for the admission-level tests that
/// don't need a real transfer behind them.
fn descriptor_on_lane(op_id: &str, lane: &str, preview_id: Option<String>) -> super::manager::OperationDescriptor {
    super::manager::OperationDescriptor {
        operation_id: op_id.to_string(),
        operation_type: WriteOperationType::Copy,
        lanes: vec![crate::file_system::volume::LaneKey::new(lane)],
        volume_ids: vec![],
        summary: super::manager::OperationSummaryText::default(),
        supports_rollback: true,
        preview_id,
        reverses: None,
    }
}

/// Starts a copy that is parked on an in-flight preview, returning its sink,
/// id, preview id, and the fixture dir (which must outlive the operation).
async fn start_copy_awaiting_preview(label: &str) -> (Arc<CollectorEventSink>, String, String, TestDir) {
    let dir = TestDir::new(label);
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    std::fs::write(src.join("a.bin"), b"a").expect("write a");

    let preview_id = unique("preview");
    register_in_flight(&preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = copy_files_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        vec![src.clone()],
        dst,
        WriteOperationConfig {
            preview_id: Some(preview_id.clone()),
            ..WriteOperationConfig::default()
        },
        vec![unique("vol")],
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("copy starts");

    let op_id = start.operation_id.clone();
    assert_eq!(start.operation_type, WriteOperationType::Copy);
    assert!(manager().is_in_scan_wait(&op_id), "the copy must be parked on its scan");
    (events, op_id, preview_id, dir)
}
