//! Tests for `transfer_driver.rs`'s asynchronous driver
//! (`drive_transfer_serial_async`).
//!
//! Coverage map:
//!
//! - **Data-safety properties** (the most critical class): pre-skip sources
//!   never see the closure; a conflict resolved as Skip never reaches the
//!   closure; no closure/resolver invocations after cancellation; the loop
//!   ordering is cancel-check → pre-skip-check → conflict-resolve; a late cancel
//!   after the only iteration is caught by the post-loop intent check.
//! - **Conflict resolution**: a Skip decision skips the closure; a
//!   proceed-with-rewritten-path delivers the rewritten dest; a resolver error
//!   propagates as `PostLoopIntent::Failed`; no conflict skips the resolver
//!   entirely; an apply-to-all decision can latch across sources.
//! - **Progress accounting & status parity**: pre-skip + per-iter skip +
//!   completed sources sum; total emitted bytes = sum of `Transferred.bytes`;
//!   skip counters track the skipped subset; running totals thread through
//!   `TransferContext`; every emit pairs with `update_operation_status`.
//! - **Dest path derivation**: the default dest is `dest_root.join(basename)`;
//!   the dest-meta fetcher runs exactly once per non-pre-skipped source.
//! - **Send-bound regression guard**: the returned future stays `Send` across a
//!   `tokio::spawn` (the `for<'a> FnMut -> Pin<Box<dyn Future + Send>>` shape).
//! - **Pause gate**: the async driver parks between files while paused, resumes
//!   to completion, and a cancel while paused unblocks and ends as Cancelled.

use super::super::super::state::{OperationIntent, register_operation_status, unregister_operation_status};
use super::super::super::types::{CollectorEventSink, WriteOperationError, WriteOperationType};
use super::test_support::{CallLog, copy_config, install_state, make_state, paths, uninstall_state, unique_op_id};
use super::{
    ConflictDecision, ConflictDecisionInput, PostLoopIntent, TransferContext, TransferOutcome,
    drive_transfer_serial_async,
};
use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

// ===========================================================================
// Async driver: data-safety
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_does_not_invoke_closure_for_pre_skipped_sources() {
    let op_id = unique_op_id("async-pre-skip-no-closure");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
                Ok(ConflictDecision::Skip { bytes_accounted: 0 })
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
                    replace_after_write: None,
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
                    replace_after_write: None,
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
                    *guard = Some(ConflictDecision::Skip { bytes_accounted: 0 });
                }
                Ok(ConflictDecision::Skip { bytes_accounted: 0 })
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async { Ok(ConflictDecision::Skip { bytes_accounted: 50 }) })
        },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { Ok(TransferOutcome::Transferred { bytes: 100 }) })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 4, "2 bulk + 1 conflict-skip + 1 transferred");
    assert_eq!(
        outcome.bytes_done, 350,
        "200 bulk-skipped + 50 (conflict-skip's `bytes_accounted`) + 100 transferred"
    );
    assert_eq!(outcome.files_skipped, 3, "2 bulk + 1 conflict-skip");
    assert_eq!(outcome.bytes_skipped, 250, "200 bulk + 50 conflict-skip");
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_skip_counters_zero_when_nothing_skipped() {
    // No conflicts, no bulk-skip: skip counters stay at zero so the
    // volume_copy completion log keeps its terse form.
    let op_id = unique_op_id("async-skip-counters-zero");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
    let sink = CollectorEventSink::new();

    let outcome = drive_transfer_serial_async(
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
            Box::pin(async {
                Ok(ConflictDecision::Proceed {
                    dest_path: PathBuf::from("/never"),
                    replace_after_write: None,
                })
            })
        },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { Ok(TransferOutcome::Transferred { bytes: 100 }) })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 2);
    assert_eq!(outcome.bytes_done, 200);
    assert_eq!(outcome.files_skipped, 0);
    assert_eq!(outcome.bytes_skipped, 0);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_driver_status_cache_matches_emitted_progress() {
    let op_id = unique_op_id("async-status-cache");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
        |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> {
            Box::pin(async { Ok(ConflictDecision::Skip { bytes_accounted: 0 }) })
        },
        |_ctx: TransferContext<'_>| -> TransferFut<'_> {
            Box::pin(async { panic!("closure should not fire under Skip") })
        },
    )
    .await;

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    let status = super::super::super::state::get_operation_status(&op_id).expect("status entry present");
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);

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

// ===========================================================================
// Pause gate: the async driver parks between files while paused
// ===========================================================================
//
// The gate sits immediately AFTER the loop-top `is_cancelled` check. These
// tests prove: (1) while paused, no further sources transfer and no further
// progress fires; (2) resume continues to completion; (3) a cancel while paused
// unblocks the gate and ends the loop as Cancelled; (4) pause never perturbs
// `OperationIntent` (it stays Running). Each source closure throttles (an
// artificial per-file sleep) so a pause from the controlling task lands mid-loop
// deterministically.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn async_driver_parks_while_paused_then_resumes_to_completion() {
    let op_id = unique_op_id("async-pause-resume");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);

    let sources = paths(&["/a", "/b", "/c", "/d"]);
    let transferred = Arc::new(AtomicUsize::new(0));

    let state_drv = Arc::clone(&state);
    let op = op_id.clone();
    let transferred_drv = Arc::clone(&transferred);
    let srcs = sources.clone();
    let driver = tokio::spawn(async move {
        let sink = CollectorEventSink::new();
        let transferred_ref = &transferred_drv;
        drive_transfer_serial_async(
            &sink,
            &state_drv,
            &op,
            &srcs,
            Path::new("/dest"),
            4,
            0,
            0,
            0,
            &HashSet::new(),
            &copy_config(),
            |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
            |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
            |_ctx: TransferContext<'_>| -> TransferFut<'_> {
                let t = Arc::clone(transferred_ref);
                Box::pin(async move {
                    // Artificial per-file throttle so the pause lands mid-loop.
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    t.fetch_add(1, Ordering::SeqCst);
                    Ok(TransferOutcome::Transferred { bytes: 0 })
                })
            },
        )
        .await
    });

    // Let one or two files transfer, then pause.
    tokio::time::sleep(Duration::from_millis(60)).await;
    state.pause_gate.pause();
    let done_at_pause = transferred.load(Ordering::SeqCst);
    assert!(
        done_at_pause >= 1,
        "at least one file should have transferred pre-pause"
    );
    assert!(done_at_pause < 4, "not all files should be done at pause time");

    // The gate parks BETWEEN files, so an already-in-flight file (mid-throttle
    // when pause landed) may still complete; mid-file pause is v2. After it
    // drains, the count must hold steady at the next-file boundary. Sample, wait
    // past several throttle intervals, sample again: the two must match (steady
    // state reached) and still be short of completion.
    tokio::time::sleep(Duration::from_millis(120)).await;
    let steady = transferred.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        transferred.load(Ordering::SeqCst),
        steady,
        "no further sources may transfer once the in-flight file drains and the gate parks"
    );
    assert!(steady < 4, "the op must be parked short of completion while paused");
    assert!(!driver.is_finished(), "the driver must still be parked while paused");
    // Pause is orthogonal to OperationIntent.
    assert_eq!(
        super::super::super::state::load_intent(&state.intent),
        OperationIntent::Running,
        "pause must not touch OperationIntent"
    );

    // Resume → runs to completion.
    state.pause_gate.resume();
    let outcome = tokio::time::timeout(Duration::from_secs(5), driver)
        .await
        .expect("resumed op must complete")
        .expect("driver future must be Send");
    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(
        transferred.load(Ordering::SeqCst),
        4,
        "all sources transfer after resume"
    );

    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn async_driver_cancel_while_paused_unblocks_and_cancels() {
    let op_id = unique_op_id("async-pause-cancel");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);

    let sources = paths(&["/a", "/b", "/c", "/d"]);
    let transferred = Arc::new(AtomicUsize::new(0));

    let state_drv = Arc::clone(&state);
    let op = op_id.clone();
    let transferred_drv = Arc::clone(&transferred);
    let srcs = sources.clone();
    let driver = tokio::spawn(async move {
        let sink = CollectorEventSink::new();
        let transferred_ref = &transferred_drv;
        drive_transfer_serial_async(
            &sink,
            &state_drv,
            &op,
            &srcs,
            Path::new("/dest"),
            4,
            0,
            0,
            0,
            &HashSet::new(),
            &copy_config(),
            |_p: &Path| -> FetchFut<'_> { Box::pin(async { None }) },
            |_i: ConflictDecisionInput<'_>| -> ResolveFut<'_> { Box::pin(async { unreachable!() }) },
            |_ctx: TransferContext<'_>| -> TransferFut<'_> {
                let t = Arc::clone(transferred_ref);
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    t.fetch_add(1, Ordering::SeqCst);
                    Ok(TransferOutcome::Transferred { bytes: 0 })
                })
            },
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(60)).await;
    state.pause_gate.pause();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let done_at_cancel = transferred.load(Ordering::SeqCst);
    assert!(done_at_cancel < 4, "not finished while paused");

    // Cancel while paused: the production cancel path flips intent AND wakes the
    // gate. Drive it through the public cancel API (never store intent directly).
    super::super::super::state::cancel_write_operation(&op_id, false);

    let outcome = tokio::time::timeout(Duration::from_secs(5), driver)
        .await
        .expect("cancel-while-paused must unblock the gate and end the loop")
        .expect("driver future must be Send");
    assert!(
        matches!(outcome.intent, PostLoopIntent::Cancelled),
        "cancel wins over pause"
    );
    // Already-copied files are kept (the driver returns the count it reached);
    // no further sources transferred after the cancel.
    assert!(transferred.load(Ordering::SeqCst) <= done_at_cancel + 1);
    assert_eq!(
        super::super::super::state::load_intent(&state.intent),
        OperationIntent::Stopped,
        "keep-partials cancel lands on Stopped"
    );

    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}
