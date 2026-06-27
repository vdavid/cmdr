//! Tests for `transfer_driver.rs`'s synchronous driver
//! (`drive_transfer_serial_sync`).
//!
//! Coverage map:
//!
//! - **Data-safety properties** (the most critical class): pre-skip sources
//!   never see the closure; cancellation before/mid-loop stops the closure; a
//!   closure failure propagates as `PostLoopIntent::Failed`.
//! - **`&mut`-state capture**: the sync driver's `FnMut` bound lets closures
//!   capture `&mut` per-iteration state (a `tracker`-like counter exercises it).
//! - **Progress accounting**: pre-skip + per-iter skip + completed sources sum
//!   correctly; bulk-skip emits exactly one progress event; skip counters track
//!   the bulk + per-iter subset; running totals thread through `TransferContext`.
//! - **Status-cache parity**: every emitted progress event pairs with an
//!   `update_operation_status` call (verified via `get_operation_status`).
//! - **Pause gate**: the sync driver parks between files while paused (its
//!   condvar runs on a real OS thread under `spawn_blocking`), resumes to
//!   completion, and a cancel while paused unblocks and ends as Cancelled.

use super::super::super::state::{OperationIntent, register_operation_status, unregister_operation_status};
use super::super::super::types::{CollectorEventSink, WriteOperationError, WriteOperationType};
use super::test_support::{CallLog, copy_config, install_state, make_state, paths, uninstall_state, unique_op_id};
use super::{PostLoopIntent, TransferContext, TransferOutcome, drive_transfer_serial_sync};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
fn sync_driver_tracks_files_skipped_across_bulk_and_per_iter() {
    // Bulk-skip 2 files + per-iter Skip 1 file + transfer 1 file → 3 skipped
    // (2 bulk + 1 per-iter), 1 transferred. `files_skipped` / `bytes_skipped`
    // is the subset of `files_done` / `bytes_done` that came from any Skip
    // path — used by the volume-copy completion log annotation.
    let op_id = unique_op_id("sync-skip-counters");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
    let sink = CollectorEventSink::new();

    let mut pre_skip = HashSet::new();
    pre_skip.insert(PathBuf::from("/bulk1"));
    pre_skip.insert(PathBuf::from("/bulk2"));

    let outcome = drive_transfer_serial_sync(
        &sink,
        &state,
        &op_id,
        &paths(&["/bulk1", "/bulk2", "/per-iter", "/keep"]),
        4,
        400,
        2,
        200,
        &pre_skip,
        &copy_config(),
        |ctx| {
            if ctx.source_path == Path::new("/per-iter") {
                Ok(TransferOutcome::Skipped { bytes_accounted: 50 })
            } else {
                Ok(TransferOutcome::Transferred { bytes: 100 })
            }
        },
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 4, "2 bulk + 1 per-iter skip + 1 transferred");
    assert_eq!(outcome.bytes_done, 350, "200 bulk + 50 skip + 100 transferred");
    assert_eq!(outcome.files_skipped, 3, "2 bulk + 1 per-iter");
    assert_eq!(outcome.bytes_skipped, 250, "200 bulk + 50 per-iter");
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_skip_counters_zero_when_nothing_skipped() {
    let op_id = unique_op_id("sync-skip-counters-zero");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
        |_| Ok(TransferOutcome::Transferred { bytes: 100 }),
    );

    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(outcome.files_done, 2);
    assert_eq!(outcome.bytes_done, 200);
    assert_eq!(outcome.files_skipped, 0);
    assert_eq!(outcome.bytes_skipped, 0);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[test]
fn sync_driver_bulk_skip_zero_does_not_emit_extra_event() {
    let op_id = unique_op_id("sync-bulk-skip-zero");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);
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
    let status = super::super::super::state::get_operation_status(&op_id).expect("status entry present");
    // After bulk-skip /a (1 file, 100 bytes) the cache reflects the bulk-skip
    // emit. The Transferred arm doesn't emit (sync per-file closures —
    // `copy_single_item` in production — own their own per-file milestone),
    // so the cache stays at the bulk-skip numbers. This synthetic closure
    // is a bare `|_| Ok(Transferred)` that doesn't emit, which matches what
    // the driver's contract guarantees: the closure is responsible for any
    // mid-iteration emits.
    assert_eq!(status.files_done, 1);
    assert_eq!(status.bytes_done, 100);
    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

// ===========================================================================
// Pause gate: the sync driver parks between files while paused
// ===========================================================================
//
// The gate sits immediately AFTER the loop-top `is_cancelled` check. These
// tests prove the sync driver parks on its condvar (running on a real OS thread
// under `spawn_blocking`, as in production), resumes to completion, and that a
// cancel while paused unblocks the condvar and ends the loop as Cancelled. Each
// source closure throttles (an artificial per-file sleep) so a pause from the
// controlling task lands mid-loop deterministically.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sync_driver_parks_while_paused_then_resumes_to_completion() {
    // The sync driver runs inside spawn_blocking in production; mirror that here
    // so its condvar park happens on a real OS thread.
    let op_id = unique_op_id("sync-pause-resume");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);

    let sources = paths(&["/a", "/b", "/c", "/d"]);
    let transferred = Arc::new(AtomicUsize::new(0));

    let state_drv = Arc::clone(&state);
    let op = op_id.clone();
    let transferred_drv = Arc::clone(&transferred);
    let driver = tokio::task::spawn_blocking(move || {
        let sink = CollectorEventSink::new();
        drive_transfer_serial_sync(
            &sink,
            &state_drv,
            &op,
            &sources,
            4,
            0,
            0,
            0,
            &HashSet::new(),
            &copy_config(),
            |_ctx: TransferContext<'_>| {
                // Synchronous artificial throttle.
                std::thread::sleep(Duration::from_millis(40));
                transferred_drv.fetch_add(1, Ordering::SeqCst);
                Ok(TransferOutcome::Transferred { bytes: 0 })
            },
        )
    });

    tokio::time::sleep(Duration::from_millis(60)).await;
    state.pause_gate.pause();
    let done_at_pause = transferred.load(Ordering::SeqCst);
    assert!((1..4).contains(&done_at_pause));

    // Let the in-flight file drain (mid-file pause is v2), then assert steady.
    tokio::time::sleep(Duration::from_millis(120)).await;
    let steady = transferred.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        transferred.load(Ordering::SeqCst),
        steady,
        "sync driver must not advance once the in-flight file drains and the gate parks"
    );
    assert!(steady < 4, "sync driver parked short of completion while paused");
    assert!(!driver.is_finished(), "sync driver parked on the condvar while paused");

    state.pause_gate.resume();
    let outcome = tokio::time::timeout(Duration::from_secs(5), driver)
        .await
        .expect("resumed sync op completes")
        .expect("spawn_blocking joins");
    assert!(matches!(outcome.intent, PostLoopIntent::Completed));
    assert_eq!(transferred.load(Ordering::SeqCst), 4);

    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sync_driver_cancel_while_paused_unblocks_and_cancels() {
    let op_id = unique_op_id("sync-pause-cancel");
    let state = make_state();
    install_state(&op_id, Arc::clone(&state));
    register_operation_status(&op_id, WriteOperationType::Copy, vec![]);

    let sources = paths(&["/a", "/b", "/c", "/d"]);
    let transferred = Arc::new(AtomicUsize::new(0));

    let state_drv = Arc::clone(&state);
    let op = op_id.clone();
    let transferred_drv = Arc::clone(&transferred);
    let driver = tokio::task::spawn_blocking(move || {
        let sink = CollectorEventSink::new();
        drive_transfer_serial_sync(
            &sink,
            &state_drv,
            &op,
            &sources,
            4,
            0,
            0,
            0,
            &HashSet::new(),
            &copy_config(),
            |_ctx: TransferContext<'_>| {
                std::thread::sleep(Duration::from_millis(40));
                transferred_drv.fetch_add(1, Ordering::SeqCst);
                Ok(TransferOutcome::Transferred { bytes: 0 })
            },
        )
    });

    tokio::time::sleep(Duration::from_millis(60)).await;
    state.pause_gate.pause();
    tokio::time::sleep(Duration::from_millis(80)).await;

    super::super::super::state::cancel_write_operation(&op_id, false);

    let outcome = tokio::time::timeout(Duration::from_secs(5), driver)
        .await
        .expect("cancel-while-paused unblocks the condvar")
        .expect("spawn_blocking joins");
    assert!(matches!(outcome.intent, PostLoopIntent::Cancelled));
    assert_eq!(
        super::super::super::state::load_intent(&state.intent),
        OperationIntent::Stopped
    );

    uninstall_state(&op_id);
    unregister_operation_status(&op_id);
}
