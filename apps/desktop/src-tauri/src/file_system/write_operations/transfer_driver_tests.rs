//! Tests for `transfer_driver.rs`.
//!
//! Coverage map:
//!
//! - **Data-safety properties** (the most critical class):
//!   - pre-skip sources never see `transfer_one`
//!   - conflict-resolved-as-Skip never sees `transfer_one`
//!   - no closure invocations after cancellation
//!   - cancellation check is BEFORE pre-skip check is BEFORE conflict resolve
//! - **Progress accounting**:
//!   - pre-skip + per-iter skip + completed sources sum correctly
//!   - total emitted bytes = sum of `Transferred.bytes`
//!   - final progress event reflects the right totals
//! - **Conflict resolution** (async driver):
//!   - skip decision skips the closure
//!   - proceed-with-rewritten-path delivers the rewritten path to closure
//!   - resolver error propagates as `PostLoopIntent::Failed`
//! - **Cancellation**:
//!   - cancellation between sources is honored
//!   - cancellation reflected in `TransferLoopOutcome.intent`
//!   - post-loop intent check catches mid-arm cancellation race
//! - **Status cache parity**: every emitted progress event has a matching
//!   `update_operation_status` call (verified via `get_operation_status`).
//! - **Sync driver only**: `&mut`-state captures work (the prototype's whole
//!   point) — exercise a captured `tracker`-like counter.

use super::super::state::{
    OperationIntent, WRITE_OPERATION_STATE, WriteOperationState, register_operation_status, unregister_operation_status,
};
use super::super::types::{
    CollectorEventSink, ConflictResolution, WriteOperationError, WriteOperationPhase, WriteOperationType,
};
use super::{
    ConflictDecision, ConflictDecisionInput, DriverConfig, PostLoopIntent, TransferContext, TransferOutcome,
    build_pre_skip_set, drive_transfer_serial_async, drive_transfer_serial_sync,
};
use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn unique_op_id(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("test-driver-{label}-{n}-{:?}", std::thread::current().id())
}

fn make_state() -> Arc<WriteOperationState> {
    // Zero progress interval so throttled emits ALWAYS fire — tests that
    // count emits would otherwise be flaky.
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

fn install_state(op_id: &str, state: Arc<WriteOperationState>) {
    WRITE_OPERATION_STATE.write().unwrap().insert(op_id.to_string(), state);
}

fn uninstall_state(op_id: &str) {
    WRITE_OPERATION_STATE.write().unwrap().remove(op_id);
}

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

fn copy_config() -> DriverConfig {
    DriverConfig {
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        conflict_resolution: ConflictResolution::Stop,
        pre_known_conflicts: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Closure-shape type aliases
// ---------------------------------------------------------------------------
//
// The async driver bounds its closures as
// `for<'a> FnMut(...) -> Pin<Box<dyn Future<...> + Send + 'a>>` rather than
// `AsyncFnMut(...)` so the driver future is `Send` across `tokio::spawn`
// (see `transfer_driver.rs` § "Closure-bound shape"). That shape doesn't
// compose with `async ||` literals, so tests construct each per-call future
// via `Box::pin(async move { ... })`. The type aliases below abbreviate the
// return types so the test closures stay readable; the call sites still spell
// out the lifetimes explicitly.

/// Per-call future shape for `dest_meta_fetcher`.
type FetchFut<'a> = Pin<Box<dyn Future<Output = Option<u64>> + Send + 'a>>;

/// Per-call future shape for `conflict_resolver`.
type ResolveFut<'a> = Pin<Box<dyn Future<Output = Result<ConflictDecision, WriteOperationError>> + Send + 'a>>;

/// Per-call future shape for `transfer_one`.
type TransferFut<'a> = Pin<Box<dyn Future<Output = Result<TransferOutcome, WriteOperationError>> + Send + 'a>>;

/// Tiny in-memory "call log" the closures dump into so tests can assert
/// invocation order and counts.
#[derive(Default)]
struct CallLog {
    invoked_for: Mutex<Vec<PathBuf>>,
    invoked_dests: Mutex<Vec<Option<PathBuf>>>,
}

impl CallLog {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn record(&self, src: &Path, dest: Option<&Path>) {
        self.invoked_for.lock().unwrap().push(src.to_path_buf());
        self.invoked_dests.lock().unwrap().push(dest.map(|p| p.to_path_buf()));
    }
    fn sources(&self) -> Vec<PathBuf> {
        self.invoked_for.lock().unwrap().clone()
    }
    fn dests(&self) -> Vec<Option<PathBuf>> {
        self.invoked_dests.lock().unwrap().clone()
    }
}

// ===========================================================================
// build_pre_skip_set
// ===========================================================================

#[test]
fn build_pre_skip_set_empty_when_not_skip() {
    let sources = paths(&["/a.txt", "/b.txt"]);
    let empty_dirs = HashSet::new();
    for resolution in [
        ConflictResolution::Stop,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
    ] {
        let set = build_pre_skip_set(&sources, resolution, &["a.txt".into()], &empty_dirs);
        assert!(
            set.is_empty(),
            "non-Skip resolution {resolution:?} must not populate pre-skip set"
        );
    }
}

#[test]
fn build_pre_skip_set_empty_when_pre_known_list_empty() {
    let sources = paths(&["/a.txt", "/b.txt"]);
    let empty_dirs = HashSet::new();
    let set = build_pre_skip_set(&sources, ConflictResolution::Skip, &[], &empty_dirs);
    assert!(set.is_empty());
}

#[test]
fn build_pre_skip_set_matches_by_filename_only() {
    // Pre-known list contains FILE NAMES (the FE only knows leaf names from
    // the conflict scan). The driver must match by `file_name()`, not full
    // path.
    let sources = paths(&["/photos/a.txt", "/docs/b.txt", "/docs/c.txt"]);
    let empty_dirs = HashSet::new();
    let set = build_pre_skip_set(
        &sources,
        ConflictResolution::Skip,
        &["a.txt".into(), "c.txt".into()],
        &empty_dirs,
    );
    assert_eq!(set.len(), 2);
    assert!(set.contains(&PathBuf::from("/photos/a.txt")));
    assert!(set.contains(&PathBuf::from("/docs/c.txt")));
    assert!(!set.contains(&PathBuf::from("/docs/b.txt")));
}

/// Directory-typed top-level sources are excluded from the bulk-skip set
/// even when their filenames match a pre-known conflict. Bulk-skip would
/// drop the whole subtree; for directories the right behavior is to fall
/// through to per-iter conflict resolution so the conflicting children get
/// skipped individually while the non-conflicting ones still copy.
#[test]
fn build_pre_skip_set_excludes_known_directory_paths() {
    let sources = paths(&["/photos/a.txt", "/docs", "/notes/c.txt"]);
    let mut known_dirs = HashSet::new();
    known_dirs.insert(PathBuf::from("/docs"));
    let set = build_pre_skip_set(
        &sources,
        ConflictResolution::Skip,
        &["a.txt".into(), "docs".into(), "c.txt".into()],
        &known_dirs,
    );
    assert_eq!(set.len(), 2);
    assert!(set.contains(&PathBuf::from("/photos/a.txt")));
    assert!(set.contains(&PathBuf::from("/notes/c.txt")));
    assert!(
        !set.contains(&PathBuf::from("/docs")),
        "known-directory path /docs must be excluded from bulk-skip"
    );
}

// ===========================================================================
// Sync driver: data-safety
// ===========================================================================

#[test]
fn sync_driver_does_not_invoke_closure_for_pre_skipped_sources() {
    // The most important data-safety property: pre-known-conflict sources
    // MUST NOT reach the closure. A regression would mean the closure runs
    // for a source the user already opted to skip.
    let op_id = unique_op_id("sync-pre-skip-no-closure");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    let sources = paths(&["/a.txt", "/b.txt", "/c.txt"]);
    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/a.txt"));
    pre_skip.insert(PathBuf::from("/c.txt"));

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &sources,
        3,
        300,
        2,
        200, // bulk-skip 2 files / 200 bytes
        &pre_skip,
        &copy_config(),
        |ctx| {
            log_clone.record(ctx.source_path, ctx.dest_path);
            Ok(TransferOutcome::Transferred { bytes: 100 })
        },
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 3, "2 bulk-skipped + 1 transferred");
    assert_eq!(outcome.bytes_done, 300, "200 bulk-skipped bytes + 100 transferred");
    assert_eq!(
        log.sources(),
        vec![PathBuf::from("/b.txt")],
        "closure must NEVER fire for pre-skipped sources"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_honors_cancellation_at_start_of_iteration() {
    // Pre-cancel the state. The driver must NOT invoke the closure even once.
    let op_id = unique_op_id("sync-pre-cancel");
    let state = make_state();
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        2,
        200,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |ctx| {
            log_clone.record(ctx.source_path, ctx.dest_path);
            Ok(TransferOutcome::Transferred { bytes: 100 })
        },
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Cancelled));
    assert!(log.sources().is_empty(), "no closure invocations after cancellation");
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_stops_invoking_closure_after_mid_loop_cancel() {
    // Cancel after the first iteration. The closure should run exactly once;
    // the second iteration's cancel-check intercepts.
    let op_id = unique_op_id("sync-mid-cancel");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);
    let state_clone = Arc::clone(&state);

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        3,
        300,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |ctx| {
            log_clone.record(ctx.source_path, ctx.dest_path);
            // Cancel after we've recorded /a.
            state_clone
                .intent
                .store(OperationIntent::Stopped as u8, Ordering::Relaxed);
            Ok(TransferOutcome::Transferred { bytes: 50 })
        },
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Cancelled));
    assert_eq!(
        log.sources(),
        vec![PathBuf::from("/a")],
        "only /a should have been invoked; /b and /c blocked by cancel check"
    );
    assert_eq!(outcome.files_done, 1);
    assert_eq!(outcome.bytes_done, 50);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_propagates_closure_failure_as_failed_intent() {
    let op_id = unique_op_id("sync-fail");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        2,
        0,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_| {
            Err(WriteOperationError::IoError {
                path: "/a".into(),
                message: "synthetic boom".into(),
            })
        },
    );

    assert!(
        matches!(
            outcome.intent,
            PostLoopIntent::Failed(WriteOperationError::IoError { .. })
        ),
        "got {:?}",
        outcome.intent
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_closure_can_capture_mut_state() {
    // The whole point of the sync driver's `FnMut` bound: closures need to
    // capture `&mut` references to per-iteration state (CopyTransaction,
    // SourceItemTracker, etc.). Exercise a captured counter to pin this.
    let op_id = unique_op_id("sync-mut-capture");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    // `tracker_counter` stands in for `SourceItemTracker`; the closure mutates
    // it on every invocation. If the `FnMut` bound regressed to `Fn`, this
    // wouldn't compile.
    let mut tracker_counter: usize = 0;
    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        3,
        300,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_ctx| {
            tracker_counter += 1;
            Ok(TransferOutcome::Transferred { bytes: 100 })
        },
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(tracker_counter, 3);
    assert_eq!(outcome.files_done, 3);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Sync driver: progress accounting
// ===========================================================================

#[test]
fn sync_driver_skipped_outcome_bumps_counters_and_emits_progress() {
    let op_id = unique_op_id("sync-skip-outcome");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        2,
        200,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |ctx| {
            if ctx.source_path == Path::new("/a") {
                Ok(TransferOutcome::Skipped { bytes_accounted: 100 })
            } else {
                Ok(TransferOutcome::Transferred { bytes: 100 })
            }
        },
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 2);
    assert_eq!(outcome.bytes_done, 200);
    // The Skipped arm should emit a progress event so the bar reflects the
    // skip immediately.
    let progress = sink.progress.lock().unwrap();
    let has_a_skip = progress
        .iter()
        .any(|e| e.current_file.as_deref() == Some("a") && e.files_done == 1);
    assert!(
        has_a_skip,
        "expected a progress event for skipped /a with files_done=1; got: {:?}",
        *progress
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_bulk_skip_emits_one_progress_event() {
    let op_id = unique_op_id("sync-bulk-skip-event");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/a"));
    pre_skip.insert(PathBuf::from("/b"));

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        2,
        200,
        2,
        200,
        &pre_skip,
        &copy_config(),
        |_| panic!("closure must NEVER fire for all-bulk-skipped batch"),
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 2);
    assert_eq!(outcome.bytes_done, 200);
    let progress = sink.progress.lock().unwrap();
    let bulk_events: Vec<_> = progress.iter().filter(|e| e.files_done == 2).collect();
    assert_eq!(
        bulk_events.len(),
        1,
        "exactly one progress event should reflect the bulk skip; got: {:?}",
        *progress
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_bulk_skip_zero_does_not_emit_extra_event() {
    let op_id = unique_op_id("sync-bulk-skip-zero");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let _ = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&[]),
        0,
        0,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_| Ok(TransferOutcome::Transferred { bytes: 0 }),
    );

    assert!(
        sink.progress.lock().unwrap().is_empty(),
        "no sources + no bulk-skip should mean no progress events from the driver"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_threads_running_totals_through_transfer_context() {
    // `files_done_so_far` and `bytes_done_so_far` snapshot pre-iter totals.
    // The closure needs these for intra-file progress callbacks.
    let op_id = unique_op_id("sync-totals-context");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let seen_totals: Arc<Mutex<Vec<(usize, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = Arc::clone(&seen_totals);

    let _ = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        3,
        300,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |ctx| {
            seen_clone
                .lock()
                .unwrap()
                .push((ctx.files_done_so_far, ctx.bytes_done_so_far));
            Ok(TransferOutcome::Transferred { bytes: 100 })
        },
    );

    let seen = seen_totals.lock().unwrap();
    assert_eq!(seen.as_slice(), &[(0, 0), (1, 100), (2, 200)]);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Sync driver: status-cache parity
// ===========================================================================

#[test]
fn sync_driver_status_cache_matches_emitted_progress() {
    // Every progress event the driver emits must come paired with an
    // `update_operation_status` call so query APIs (the menu-bar overlay,
    // the global progress list) see the same numbers as the dialog.
    let op_id = unique_op_id("sync-status-cache");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/a"));
    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        2,
        200,
        1,
        100, // bulk-skip 1 file
        &pre_skip,
        &copy_config(),
        |_| Ok(TransferOutcome::Transferred { bytes: 100 }),
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    let status = super::super::state::get_operation_status(&op_id).expect("status entry present");
    // After bulk-skip /a (1 file, 100 bytes) and transfer /b (100 bytes), the
    // status cache reflects the bulk-skip emit; the per-Transferred arm
    // doesn't emit (the sync closure owns its own intra-file emits in
    // production). So the cache sees the bulk-skip numbers.
    assert_eq!(status.files_done, 1);
    assert_eq!(status.bytes_done, 100);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Async driver: data-safety
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_does_not_invoke_closure_for_pre_skipped_sources() {
    let op_id = unique_op_id("async-pre-skip-no-closure");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    let sources = paths(&["/a.txt", "/b.txt"]);
    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/a.txt"));

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &sources,
        Path::new("/dest"),
        2,
        200,
        1,
        100,
        &pre_skip,
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) }, // no conflicts
        |_input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async { panic!("conflict resolver must NEVER be called when there's no conflict") })
        },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                Ok(TransferOutcome::Transferred { bytes: 100 })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 2);
    assert_eq!(outcome.bytes_done, 200);
    assert_eq!(
        log.sources(),
        vec![PathBuf::from("/b.txt")],
        "closure must NEVER fire for pre-skipped /a.txt"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_does_not_invoke_closure_when_conflict_resolved_as_skip() {
    let op_id = unique_op_id("async-conflict-skip-no-closure");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    // Source /a.txt has a dest conflict; resolver chooses Skip.
    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a.txt", "/b.txt"]),
        Path::new("/dest"),
        2,
        200,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |p: &Path| -> FetchFut<'_> {
            let conflict = p == Path::new("/dest/a.txt");
            Box::pin(async move { if conflict { Some(50) } else { None } })
        },
        |input: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async move {
                assert_eq!(input.source_path, Path::new("/a.txt"));
                assert_eq!(input.initial_dest_path, Path::new("/dest/a.txt"));
                assert_eq!(input.dest_size_hint, Some(50));
                Ok(ConflictDecision::Skip)
            })
        },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                Ok(TransferOutcome::Transferred { bytes: 100 })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 2, "1 skipped + 1 transferred");
    assert_eq!(outcome.bytes_done, 100, "only /b.txt's 100 bytes");
    assert_eq!(
        log.sources(),
        vec![PathBuf::from("/b.txt")],
        "closure must NEVER fire when resolver returned Skip"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_pre_cancel_does_not_invoke_closure_or_resolver() {
    let op_id = unique_op_id("async-pre-cancel");
    let state = make_state();
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        Path::new("/dest"),
        2,
        0,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> {
            Box::pin(async { panic!("dest_meta_fetcher must NEVER be called after pre-cancel") })
        },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async { panic!("conflict resolver must NEVER be called after pre-cancel") })
        },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                Ok(TransferOutcome::Transferred { bytes: 0 })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Cancelled));
    assert!(log.sources().is_empty());
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_cancel_after_first_blocks_second() {
    let op_id = unique_op_id("async-mid-cancel");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);
    let state_for_closure = Arc::clone(&state);

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        Path::new("/dest"),
        3,
        300,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async { unreachable!("no conflicts in this test") })
        },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            let state_for_closure = Arc::clone(&state_for_closure);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                state_for_closure
                    .intent
                    .store(OperationIntent::Stopped as u8, Ordering::Relaxed);
                Ok(TransferOutcome::Transferred { bytes: 50 })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Cancelled));
    assert_eq!(log.sources(), vec![PathBuf::from("/a")]);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_post_loop_intent_catches_late_cancel_race() {
    // Cancel AFTER the closure for the only source has completed but BEFORE
    // the driver returns. The post-loop intent check should observe this and
    // return Cancelled instead of Completed.
    let op_id = unique_op_id("async-late-cancel-race");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let state_for_closure = Arc::clone(&state);

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/only"]),
        Path::new("/dest"),
        1,
        100,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            let state_for_closure = Arc::clone(&state_for_closure);
            Box::pin(async move {
                // Closure finishes successfully, then user clicks Rollback.
                let bytes = 100;
                state_for_closure
                    .intent
                    .store(OperationIntent::RollingBack as u8, Ordering::Relaxed);
                Ok(TransferOutcome::Transferred { bytes })
            })
        },
    )
    .await;

    assert!(
        matches!(outcome.intent, PostLoopIntent::Cancelled),
        "late cancel after the only iteration must be observed by the post-loop check; \
         this is the `1de4255d`-class race"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Async driver: conflict resolution
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_proceed_with_rewritten_dest_reaches_closure() {
    let op_id = unique_op_id("async-conflict-rename");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a.txt"]),
        Path::new("/dest"),
        1,
        100,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { Some(50) }) }, // always a conflict
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async {
                Ok(ConflictDecision::Proceed {
                    dest_path: PathBuf::from("/dest/a (1).txt"),
                })
            })
        },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                Ok(TransferOutcome::Transferred { bytes: 100 })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(
        log.dests(),
        vec![Some(PathBuf::from("/dest/a (1).txt"))],
        "rewritten dest must be threaded through TransferContext.dest_path"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_resolver_error_propagates_as_failed_intent() {
    let op_id = unique_op_id("async-resolver-err");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a.txt"]),
        Path::new("/dest"),
        1,
        0,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { Some(0) }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async {
                Err(WriteOperationError::IoError {
                    path: "/a.txt".into(),
                    message: "resolver boom".into(),
                })
            })
        },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { panic!("closure must NEVER fire when resolver errored") })
        },
    )
    .await;

    assert!(matches!(
        outcome.intent,
        PostLoopIntent::Failed(WriteOperationError::IoError { .. })
    ));
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_no_conflict_skips_resolver_entirely() {
    let op_id = unique_op_id("async-no-conflict-no-resolver");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let resolver_count: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let r = Arc::clone(&resolver_count);

    let _outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b"]),
        Path::new("/dest"),
        2,
        200,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            let r = Arc::clone(&r);
            Box::pin(async move {
                r.fetch_add(1, Ordering::SeqCst);
                Ok(ConflictDecision::Proceed {
                    dest_path: PathBuf::new(),
                })
            })
        },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { Ok(TransferOutcome::Transferred { bytes: 100 }) })
        },
    )
    .await;

    assert_eq!(resolver_count.load(Ordering::SeqCst), 0);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_apply_to_all_resolver_decision_persists_across_sources() {
    // The driver passes through `&mut apply_to_all_resolution` via the
    // resolver closure's own captures (the resolver IS the
    // resolve_volume_conflict caller). The driver itself doesn't latch the
    // resolution — but the resolver can. Exercise that the resolver runs
    // for every conflict source and CAN latch behaviour across iterations.
    let op_id = unique_op_id("async-apply-to-all-resolver");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    // The resolver "latches Skip-all" after the first call. We assert the
    // driver invokes the resolver for /a, then again for /b (so latching
    // works), and never invokes the closure (everything skipped).
    let resolver_calls: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let r = Arc::clone(&resolver_calls);
    let latched: Arc<Mutex<Option<ConflictDecision>>> = Arc::new(Mutex::new(None));
    let latched_for_resolver = Arc::clone(&latched);

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        Path::new("/dest"),
        3,
        300,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { Some(0) }) }, // every source conflicts
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            let r = Arc::clone(&r);
            let latched = Arc::clone(&latched_for_resolver);
            Box::pin(async move {
                r.fetch_add(1, Ordering::SeqCst);
                // Closure latches Skip on first call; from then on, returns Skip.
                let mut guard = latched.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(ConflictDecision::Skip);
                }
                Ok(ConflictDecision::Skip)
            })
        },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                Ok(TransferOutcome::Transferred { bytes: 999 })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(resolver_calls.load(Ordering::SeqCst), 3);
    assert!(log.sources().is_empty(), "closure must NEVER fire under all-Skip");
    assert_eq!(outcome.files_done, 3);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Async driver: progress accounting & status parity
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_progress_accounting_sums_correctly() {
    // 2 bulk-skip + 1 per-iter Skip + 1 Transferred = 4 files_done.
    let op_id = unique_op_id("async-progress-sum");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/x1"));
    pre_skip.insert(PathBuf::from("/x2"));

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/x1", "/x2", "/conflict", "/clean"]),
        Path::new("/dest"),
        4,
        400,
        2,
        200, // 2 bulk-skipped, 100 bytes each
        &pre_skip,
        &copy_config(),
        |p: &Path| -> FetchFut<'_> {
            let conflict = p == Path::new("/dest/conflict");
            Box::pin(async move { if conflict { Some(50) } else { None } })
        },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { Ok(ConflictDecision::Skip) }) },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { Ok(TransferOutcome::Transferred { bytes: 100 }) })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 4, "2 bulk + 1 conflict-skip + 1 transferred");
    assert_eq!(
        outcome.bytes_done, 300,
        "200 bulk-skipped + 0 (conflict-skip has no bytes) + 100 transferred"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_status_cache_matches_emitted_progress() {
    let op_id = unique_op_id("async-status-cache");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a"]),
        Path::new("/dest"),
        1,
        100,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { Some(50) }) }, // conflict
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { Ok(ConflictDecision::Skip) }) },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { panic!("closure should not fire under Skip") })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    let status = super::super::state::get_operation_status(&op_id).expect("status entry present");
    // The per-iter Skip emit pairs with `update_operation_status` so the cache
    // must mirror the same numbers.
    assert_eq!(status.files_done, 1);
    assert_eq!(status.bytes_done, 0);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_emitted_bytes_equal_sum_of_transferred() {
    // No conflicts, no skips, three pure transfers: outcome.bytes_done must
    // equal the sum.
    let op_id = unique_op_id("async-bytes-sum");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let bytes_seq = Arc::new(Mutex::new(vec![10u64, 20, 30].into_iter()));
    let b = Arc::clone(&bytes_seq);

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        Path::new("/dest"),
        3,
        60,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            let b = Arc::clone(&b);
            Box::pin(async move {
                let n = b.lock().unwrap().next().unwrap();
                Ok(TransferOutcome::Transferred { bytes: n })
            })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.bytes_done, 60);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_threads_running_totals_through_context() {
    let op_id = unique_op_id("async-totals-context");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let seen: Arc<Mutex<Vec<(usize, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let s = Arc::clone(&seen);

    let _ = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a", "/b", "/c"]),
        Path::new("/dest"),
        3,
        300,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let s = Arc::clone(&s);
            Box::pin(async move {
                s.lock().unwrap().push((ctx.files_done_so_far, ctx.bytes_done_so_far));
                Ok(TransferOutcome::Transferred { bytes: 100 })
            })
        },
    )
    .await;

    let seen = seen.lock().unwrap();
    assert_eq!(seen.as_slice(), &[(0, 0), (1, 100), (2, 200)]);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Async driver: dest path derivation
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_default_dest_joins_source_basename() {
    let op_id = unique_op_id("async-default-dest");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();
    let log = CallLog::new();
    let log_clone = Arc::clone(&log);

    let _ = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/foo/bar.txt"]),
        Path::new("/dest/root"),
        1,
        100,
        0,
        0,
        &HashSet::new(),
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
        |ctx: TransferContext<'_>| -> TransferFut<'_> {
            let log_clone = Arc::clone(&log_clone);
            Box::pin(async move {
                log_clone.record(ctx.source_path, ctx.dest_path);
                Ok(TransferOutcome::Transferred { bytes: 100 })
            })
        },
    )
    .await;

    assert_eq!(
        log.dests(),
        vec![Some(PathBuf::from("/dest/root/bar.txt"))],
        "default dest is dest_root.join(source.file_name())"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_dest_meta_fetcher_polled_exactly_once_per_non_skipped_source() {
    // The fetcher is the cross-volume `dest.get_metadata` stat. It MUST run
    // for every non-pre-skipped source, and it must NOT run for pre-skipped
    // ones (those are the bulk-skip-data-safety guarantee).
    let op_id = unique_op_id("async-fetcher-cardinality");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let probed: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let p = Arc::clone(&probed);

    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/skip"));

    let _ = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/skip", "/a", "/b"]),
        Path::new("/dest"),
        3,
        300,
        1,
        100,
        &pre_skip,
        &copy_config(),
        |path: &Path| -> FetchFut<'_> {
            let p = Arc::clone(&p);
            let path = path.to_path_buf();
            Box::pin(async move {
                p.lock().unwrap().push(path);
                None
            })
        },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { Ok(TransferOutcome::Transferred { bytes: 100 }) })
        },
    )
    .await;

    let probed = probed.lock().unwrap();
    assert_eq!(
        probed.as_slice(),
        &[PathBuf::from("/dest/a"), PathBuf::from("/dest/b")],
        "fetcher must skip pre-skipped sources and run exactly once for each remaining one"
    );
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Shared loop ordering invariant
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_cancel_check_precedes_pre_skip_check_precedes_conflict_resolve() {
    // If cancellation is observed, NEITHER the pre-skip check matters NOR
    // does the resolver run. Verify by cancelling on iter-start for a source
    // that's BOTH pre-skipped AND would have conflicted — resolver must not
    // be called, fetcher must not be called.
    let op_id = unique_op_id("async-ordering");
    let state = make_state();
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);
    let sink = CollectorEventSink::new();

    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/a"));

    let outcome = drive_transfer_serial_async(
        &sink,
        &state,
        &op_id,
        &paths(&["/a"]),
        Path::new("/dest"),
        1,
        100,
        0,
        0,
        &pre_skip,
        &copy_config(),
        |_p: &Path| -> FetchFut<'_> {
            Box::pin(async { panic!("cancel check must short-circuit BEFORE any fetcher call") })
        },
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async { panic!("cancel check must short-circuit BEFORE any resolver call") })
        },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { panic!("cancel check must short-circuit BEFORE any closure call") })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Cancelled));
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Send-bound regression guard
// ===========================================================================

/// The async driver's returned future must be `Send` so production callers can
/// `tokio::spawn` it. `#[tokio::test]` alone doesn't enforce this (it runs on a
/// single-thread runtime by default and `spawn`s the body itself, but doesn't
/// require the body to outlive the spawn caller's borrows). Routing the call
/// through an inner `tokio::spawn` forces the Send check.
///
/// Before the bound switched from `AsyncFnMut` to
/// `for<'a> FnMut(...) -> Pin<Box<dyn Future + Send + 'a>>`, this test failed
/// to compile with "future cannot be sent between threads safely" because
/// `AsyncFnMut`'s HRTB-bound `CallRefFuture<'a>` isn't provably `Send` for all
/// `'a` (rust-lang/rust#100013-class).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn driver_future_is_send_across_spawn() {
    let op_id = unique_op_id("async-send-across-spawn");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy);

    let op_id_clone = op_id.clone();
    let state_clone = Arc::clone(&state);
    // Mimic production: the closures capture a reference to an Arc that lives
    // in the outer spawn'd future scope. This matches volume_copy's pattern of
    // closing over `&dest_volume` (an `Arc<dyn Volume>`).
    let shared: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let task = tokio::spawn(async move {
        let sink = CollectorEventSink::new();
        let shared_ref = &shared;
        drive_transfer_serial_async(
            &sink,
            &state_clone,
            &op_id_clone,
            &paths(&["/a"]),
            Path::new("/dest"),
            1,
            100,
            0,
            0,
            &HashSet::new(),
            &copy_config(),
            |_p: &Path| -> FetchFut<'_> {
                let r = Arc::clone(shared_ref);
                Box::pin(async move {
                    r.fetch_add(1, Ordering::SeqCst);
                    None
                })
            },
            |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
            |_ctx: TransferContext<'_>| -> TransferFut<'_> {
                let r = Arc::clone(shared_ref);
                Box::pin(async move {
                    r.fetch_add(1, Ordering::SeqCst);
                    Ok(TransferOutcome::Transferred { bytes: 100 })
                })
            },
        )
        .await
    });

    let outcome = task.await.expect("driver future must be Send");
    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}
