//! The scan-wait, end to end: a confirmed transfer gets its operation record
//! before its preview has finished walking, waits for it inside its own task,
//! and consumes the result instead of re-walking.
//!
//! Every test drives the real `*_start` entry points through the real manager,
//! because the whole point of this seam is what the manager, the preview map,
//! and the operation task agree on. The manager is a process-global singleton,
//! so ids and lane keys are unique per test.

use crate::testing::TestOperations;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::OperationEventSink;
use super::event_sinks::CollectorEventSink;
use super::manager::PauseOutcome;
use super::scan_bridge::observed_scan_ticks;
use super::scan_cache::{CachedScanResult, ScanOutcome, ScanPreviewState};
use super::types::{LifecycleStatus, WriteOperationConfig, WriteOperationError, WriteOperationType};
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::CopyScanResult;
use cmdr_operation_log::types::{Initiator, OperationId};

/// Deadline for every wait here. Generous on purpose: each wait has a real
/// condition, so a healthy run satisfies it in microseconds.
const WAIT: Duration = Duration::from_secs(5);

fn unique(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("scanwait-{label}-{n}-{:?}", std::thread::current().id())
}

/// Registers a preview on `engine` that is still walking, the state a
/// `TransferDialog` leaves behind when the user confirms early.
fn register_in_flight(engine: &TestOperations, preview_id: &str) {
    engine.stores().previews().register_preview(
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
                    top_level_modified_at: None,
                    top_level_created_at: None,
                },
            )
        })
        .collect();
    CachedScanResult::from_local_walk(
        crate::types::ScanPreviewPurpose::Transfer,
        sources,
        infos,
        Vec::new(),
        total,
        total,
        per_path,
        None,
    )
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
    let engine = TestOperations::new();
    let dir = TestDir::new("scanwait-registers");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    let file = src.join("a.bin");
    std::fs::write(&file, b"a").expect("write a");

    let preview_id = unique("preview");
    register_in_flight(&engine, &preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = engine
        .copy_files_start(
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
            None,
        )
        .await
        .expect("copy starts");

    let op_id = OperationId::from(start.operation_id.clone());
    let snapshot = engine.list_operations();
    let row = snapshot
        .iter()
        .find(|op| op.operation_id == op_id.as_str())
        .expect("the confirmed transfer must be a queue row while its preview still walks");
    assert_eq!(
        row.status,
        LifecycleStatus::Running,
        "a scanning transfer reuses Running; a status of its own would degrade silently everywhere"
    );
    assert!(
        engine.manager().is_in_scan_wait(&op_id),
        "the record must know it is parked on its preview"
    );

    // Let it finish so the lane frees for the rest of the suite.
    engine.stores().previews().settle_preview(
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
    let engine = TestOperations::new();
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
    register_in_flight(&engine, &preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = engine
        .copy_files_start(
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
            None,
        )
        .await
        .expect("copy starts");
    let op_id = OperationId::from(start.operation_id.clone());

    // The operation is parked: nothing copied yet, because nothing has told it
    // what to copy.
    assert!(
        engine.manager().is_in_scan_wait(&op_id),
        "the operation must be waiting, not walking"
    );

    engine.stores().previews().settle_preview(
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

/// A preview that errored fails its operation with the walk's own failure,
/// unchanged. Unimplementable against a bare "the preview finished" pulse,
/// which is how this pins the outcome contract rather than a completion signal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_preview_fails_its_operation() {
    let engine = TestOperations::new();
    let (events, op_id, preview_id, _dir) = start_copy_awaiting_preview(&engine, "scanwait-error").await;

    engine.stores().previews().settle_preview(
        &preview_id,
        ScanOutcome::Error(WriteOperationError::IoError {
            path: "/src/locked.bin".to_string(),
            message: "Permission denied".into(),
        }),
        None,
    );

    wait_until_async(WAIT, "the write-error event", || {
        !events.errors.lock().expect("collector mutex").is_empty()
    })
    .await;
    let errors = events.errors.lock().expect("collector mutex");
    let error = errors.first().expect("one error");
    assert_eq!(error.operation_id, op_id.as_str());
    assert!(
        matches!(&error.error, WriteOperationError::IoError { path, message }
            if path == "/src/locked.bin" && message == "Permission denied"),
        "the walk's own failure must reach the operation, got {:?}",
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
    let engine = TestOperations::new();
    let (events, op_id, preview_id, _dir) = start_copy_awaiting_preview(&engine, "scanwait-cancel").await;

    engine
        .stores()
        .previews()
        .settle_preview(&preview_id, ScanOutcome::Cancelled, None);

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
        Some(op_id.to_string()),
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
    let engine = TestOperations::new();
    let dir = TestDir::new("scanwait-unknown");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    std::fs::write(src.join("a.bin"), b"a").expect("write a");

    let events = Arc::new(CollectorEventSink::new());
    let start = engine
        .copy_files_start(
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
            None,
        )
        .await
        .expect("copy starts");

    assert!(
        !engine
            .manager()
            .is_in_scan_wait(&OperationId::from(start.operation_id.as_str())),
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
    let engine = TestOperations::new();
    let preview_id = unique("shared-preview");
    register_in_flight(&engine, &preview_id);

    let first = OperationId::from(unique("owner"));
    let second = OperationId::from(unique("interloper"));
    assert!(
        matches!(
            engine.stores().previews().claim_preview(&preview_id, &first),
            super::scan_cache::PreviewClaim::Waiting
        ),
        "the first claimant owns it"
    );
    assert!(
        matches!(
            engine.stores().previews().claim_preview(&preview_id, &second),
            super::scan_cache::PreviewClaim::Refused
        ),
        "a second operation must be refused, never share"
    );

    engine.stores().previews().abandon_claim(&preview_id);
}

/// A claimed result survives a TTL sweep triggered by a later settle. With
/// `LANE_BUDGET = 1` an operation can sit Queued well past the five-minute TTL,
/// so evicting its result would silently downgrade the ordinary busy-lane case
/// to a re-walk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claimed_result_survives_a_ttl_sweep() {
    let engine = TestOperations::new();
    let dir = TestDir::new("scanwait-ttl");
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("create src");
    let file = src.join("a.bin");
    std::fs::write(&file, b"a").expect("write a");

    let claimed = unique("claimed");
    let owner = OperationId::from(unique("owner"));
    register_in_flight(&engine, &claimed);
    engine.stores().previews().claim_preview(&claimed, &owner);
    engine.stores().previews().settle_preview(
        &claimed,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], std::slice::from_ref(&file), &src)),
    );

    // Age it past the TTL, then trigger a sweep by settling an unrelated
    // preview. `SCAN_RESULT_TTL` is measured from the settle, so the sweep is
    // driven by a stub whose age the test controls.
    engine
        .stores()
        .previews()
        .age_settled_entry_for_test(&claimed, super::scan_cache::SCAN_RESULT_TTL + Duration::from_secs(1));
    let other = unique("other");
    register_in_flight(&engine, &other);
    engine
        .stores()
        .previews()
        .settle_preview(&other, ScanOutcome::Cancelled, None);

    assert!(
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&claimed, &[src])
            .is_some(),
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
    let engine = TestOperations::new();
    let emits_before_registration = engine.manager().emit_count();
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview(&engine, "scanwait-bridge").await;

    let ticks = observed_scan_ticks(op_id.as_str());
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
        &engine,
        &preview_id,
        super::scan_bridge::ScanCounts {
            files_found: 12,
            dirs_found: 3,
            bytes_found: 4_096,
            ..Default::default()
        },
    );
    let ticks = observed_scan_ticks(op_id.as_str());
    assert_eq!(
        ticks.last().map(|t| (t.files_done, t.bytes_done)),
        Some((12, 4_096)),
        "a claimed preview's counts must ride the operation's own progress stream"
    );

    let src = dir.join("src");
    let file = src.join("a.bin");
    engine.stores().previews().settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], &[file], &src)),
    );
    wait_until_async(WAIT, "the copy to complete", || {
        !events.complete.lock().expect("collector mutex").is_empty()
    })
    .await;

    // The claim is gone, so the bridge has nothing to forward to.
    let before = observed_scan_ticks(op_id.as_str()).len();
    super::scan_bridge::forward_scan_progress(
        &engine,
        &preview_id,
        super::scan_bridge::ScanCounts {
            files_found: 99,
            ..Default::default()
        },
    );
    assert_eq!(
        observed_scan_ticks(op_id.as_str()).len(),
        before,
        "the bridge must go quiet once the wait is over"
    );
}

// ============================================================================
// Pause during the scan-wait
// ============================================================================

/// An operation waiting on its preview pauses like any other: the walk it is
/// waiting on parks on the same gate (`scan_pause_tests`), so the record can say
/// `Paused` and mean it. Nothing here is a special case any more, which is the
/// point — the special case is what told users a pause was coming and then
/// let an 80,000-file move run to completion.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_applies_during_the_scan_wait() {
    let engine = TestOperations::new();
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview(&engine, "scanwait-pause").await;

    assert_eq!(
        engine.pause_operation(&op_id),
        PauseOutcome::Applied,
        "a scan-waiting op parks, so its pause is an ordinary one"
    );

    assert_eq!(
        engine.manager().lifecycle_status(&op_id),
        Some(LifecycleStatus::Paused),
        "and the record says so"
    );

    engine.resume_operation(&op_id);
    assert_eq!(
        engine.manager().lifecycle_status(&op_id),
        Some(LifecycleStatus::Running),
        "resume puts it back, with no latch left behind to fire later"
    );

    let src = dir.join("src");
    let file = src.join("a.bin");
    engine.stores().previews().settle_preview(
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

/// The one that matters: a "Pause all" issued mid-scan is still holding when
/// the scan ends, so the operation does not start writing at full speed while
/// every other one is parked and the user believes the device is free. That is
/// precisely the scenario pause exists for, which makes losing it worse than
/// never offering it. The gate is what carries it across the handover: the walk
/// parks on it, and the driver that takes over parks on the same one.
///
/// ⚠️ It drives `pause_all`'s two halves rather than `pause_all` itself: the
/// manager is a process-global singleton, so a real `pause_all` here would park
/// every other test's operation too. The halves are the whole of it —
/// `running_ids()` decides who is asked, `pause_operation` per id is the ask —
/// so asserting membership plus the per-id behavior covers the same ground
/// without reaching across the suite.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pause_during_a_scan_wait_still_holds_when_the_scan_ends() {
    let engine = TestOperations::new();
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview(&engine, "scanwait-pauselatch").await;

    assert!(
        engine.manager().running_ids().contains(&op_id),
        "`pause_all` walks the running set, so a scan-waiting op has to be in it"
    );

    // The ask `pause_all` makes, per id.
    assert_eq!(engine.pause_operation(&op_id), PauseOutcome::Applied);

    let src = dir.join("src");
    let file = src.join("a.bin");
    engine.stores().previews().settle_preview(
        &preview_id,
        ScanOutcome::Complete,
        Some(completed_result(vec![src.clone()], &[file], &src)),
    );

    // allowed-test-sleep: the claim is that the operation does NOT write across
    // a span in which an unpaused one would have finished several times over.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        events.complete.lock().expect("collector mutex").is_empty(),
        "a paused operation must not copy anything once its scan lands"
    );
    assert_eq!(
        engine.manager().lifecycle_status(&op_id),
        Some(LifecycleStatus::Paused),
        "and it is still parked, not quietly running again"
    );

    // Release it so the lane frees for the rest of the suite.
    engine.resume_operation(&op_id);
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
///
/// The source doesn't exist, so the operation runs its whole shape and settles
/// on its per-item verdict without handing anything to the real Trash. What's
/// under test is the preview, and a real `trashItemAtURL` in a bare test binary
/// has DesktopServices list `target/debug/deps` to find the main bundle, which
/// alone outlasted the settle wait under a full `rust-tests` run
/// (`docs/testing.md` § "The host machine is not a fixture").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trash_frees_its_preview_instead_of_waiting_on_it() {
    let engine = TestOperations::new();
    let dir = TestDir::new("scanwait-trash");
    let never_created = dir.join("never-created.bin");

    let preview_id = unique("preview");
    register_in_flight(&engine, &preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = engine
        .trash_files_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            vec![never_created],
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
        !engine
            .manager()
            .is_in_scan_wait(&OperationId::from(start.operation_id.as_str())),
        "a trash must not park on a scan it will never read"
    );
    assert!(
        matches!(
            engine
                .stores()
                .previews()
                .claim_preview(&preview_id, &OperationId::from(unique("anyone"))),
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
    use cmdr_fs::volume::Volume;
    use cmdr_local::LocalPosixVolume;

    let engine = TestOperations::new();
    let dir = TestDir::new("scanwait-compress");
    let src_root = dir.join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "src",
        src_root.clone(),
        crate::host::testing::pane_order_host(),
    ));
    let dest = dir.join("bundle.zip");

    let preview_id = unique("preview");
    register_in_flight(&engine, &preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = engine
        .compress_start(
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
        engine
            .manager()
            .is_in_scan_wait(&OperationId::from(start.operation_id.as_str())),
        "the compress must be parked on its preview, not planning its changeset alongside it"
    );

    engine
        .stores()
        .previews()
        .settle_preview(&preview_id, ScanOutcome::Complete, None);
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
    let engine = TestOperations::new();
    let lane = unique("busy-lane");
    let preview_id = unique("preview");
    register_in_flight(&engine, &preview_id);

    // Occupy the lane so the op under test is admitted as Queued and its
    // deferred start never runs.
    let (blocker_started, blocker_started_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = tokio::sync::oneshot::channel();
    let blocker_id = OperationId::from(unique("blocker"));
    engine.manager().spawn_managed(
        descriptor_on_lane(&blocker_id, &lane, None),
        Arc::new(engine.new_operation_state(Duration::from_millis(10))),
        Box::new({
            let blocker_id = blocker_id.clone();
            move |ops: crate::Operations| {
                Box::pin(async move {
                    let guard = super::manager::ManagedTaskGuard::new(ops.manager(), blocker_id.clone());
                    let _ = blocker_started.send(());
                    let _ = release_rx.await;
                    guard.disarm();
                    ops.manager().on_settled(&blocker_id);
                })
            }
        }),
    );
    blocker_started_rx.await.expect("the blocker holds the lane");

    let queued_id = OperationId::from(unique("queued"));
    engine.manager().spawn_managed(
        descriptor_on_lane(&queued_id, &lane, Some(preview_id.clone())),
        Arc::new(engine.new_operation_state(Duration::from_millis(10))),
        Box::new(|_: crate::Operations| Box::pin(async {})),
    );
    assert_eq!(
        engine.manager().lifecycle_status(&queued_id),
        Some(LifecycleStatus::Queued),
        "the second op must be waiting on the busy lane"
    );
    assert!(
        matches!(
            engine
                .stores()
                .previews()
                .claim_preview(&preview_id, &OperationId::from("someone-else")),
            super::scan_cache::PreviewClaim::Refused
        ),
        "the queued op owns the preview while it waits"
    );

    engine.cancel_operation(&queued_id);

    assert!(
        matches!(
            engine
                .stores()
                .previews()
                .claim_preview(&preview_id, &OperationId::from("someone-else")),
            super::scan_cache::PreviewClaim::Unknown
        ),
        "cancelling a queued op must free its preview, not leave the walk running for nobody"
    );

    let _ = release.send(());
    wait_until_async(WAIT, "the blocker to settle", || {
        engine
            .list_operations()
            .iter()
            .all(|op| op.operation_id != blocker_id.as_str())
    })
    .await;
}

// ============================================================================
// The quit gate
// ============================================================================

/// ⌘Q during a scan holds, where today it walks straight past. A scan-waiting
/// transfer is an ordinary `Running` `Copy` row, which is exactly why reusing
/// `Running` (rather than minting a status of its own) makes the quit gate
/// correct with no edit at all. The gate reads exactly those two fields, and
/// `quit::tests::a_running_copy_holds_the_quit_and_announces_it` pins that a
/// `Running` `Copy` holds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scanning_transfer_holds_a_quit() {
    let engine = TestOperations::new();
    let (events, op_id, preview_id, dir) = start_copy_awaiting_preview(&engine, "scanwait-quit").await;

    let snapshot = engine.list_operations();
    let row = snapshot
        .iter()
        .find(|op| op.operation_id == op_id.as_str())
        .expect("the row");
    assert!(
        row.status == LifecycleStatus::Running && row.operation_type == WriteOperationType::Copy,
        "confirmed work a user is watching must hold a quit, even while it is only counting: \
         the gate holds for a Running Copy, got {:?} {:?}",
        row.status,
        row.operation_type
    );

    let src = dir.join("src");
    let file = src.join("a.bin");
    engine.stores().previews().settle_preview(
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
fn descriptor_on_lane(
    op_id: &OperationId,
    lane: &str,
    preview_id: Option<String>,
) -> super::manager::OperationDescriptor {
    super::manager::OperationDescriptor {
        operation_id: op_id.clone(),
        operation_type: WriteOperationType::Copy,
        lanes: vec![cmdr_fs::volume::LaneKey::new(lane)],
        volume_ids: vec![],
        summary: super::manager::OperationSummaryText::default(),
        rollback_support: crate::types::RollbackSupport::Supported,
        preview_id,
        reverses: None,
    }
}

/// Starts a copy on `engine` that is parked on an in-flight preview, returning
/// its sink, id, preview id, and the fixture dir (which must outlive the operation).
pub(super) async fn start_copy_awaiting_preview(
    engine: &TestOperations,
    label: &str,
) -> (Arc<CollectorEventSink>, OperationId, String, TestDir) {
    let dir = TestDir::new(label);
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&dst).expect("create dst");
    std::fs::write(src.join("a.bin"), b"a").expect("write a");

    let preview_id = unique("preview");
    register_in_flight(engine, &preview_id);

    let events = Arc::new(CollectorEventSink::new());
    let start = engine
        .copy_files_start(
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
            None,
        )
        .await
        .expect("copy starts");

    let op_id = OperationId::from(start.operation_id.clone());
    assert_eq!(start.operation_type, WriteOperationType::Copy);
    assert!(
        engine.manager().is_in_scan_wait(&op_id),
        "the copy must be parked on its scan"
    );
    (events, op_id, preview_id, dir)
}
