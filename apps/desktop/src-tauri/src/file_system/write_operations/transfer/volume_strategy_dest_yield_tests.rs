//! DESTINATION-side foreground auto-yield tests for `volume_strategy.rs`'s
//! `copy_single_path` (the UPLOAD path: local → SMB).
//!
//! A RUNNING (not paused) copy WRITING to a share the user is browsing must,
//! between write chunks, stand aside for that share's foreground navigation, but
//! in SHORT, HARD-CAPPED slices, never the unbounded park the SOURCE arm
//! uses. The upload holds an OPEN SMB write handle across the pause, so a long
//! stall risks the server reaping the handle. The cap guarantees the write
//! resumes (and lands a chunk, keeping the handle warm) at least once per cap,
//! even under continuous browsing. Resuming leaves the source offset untouched,
//! so the assembled bytes match a non-yielded upload exactly.
//!
//! `ForegroundBusyDest` is the test-double of an `SmbVolume` upload target: it
//! opts into `supports_foreground_yield_as_destination()`, serves a controllable
//! `foreground_pending`, and drains the source stream chunk-by-chunk (like
//! `SmbVolume::write_from_stream`'s streaming loop) into an in-memory buffer so a
//! test can assert byte-exactness across a park. The SOURCE is a `ReleasingSource`,
//! which does NOT opt into the source-side yield, so the source arm is inert and
//! only the destination arm can fire. `AutoYieldTuningGuard::with_dest_cap`
//! injects a tiny floor and a short cap so the arm is deterministic over the small
//! synthetic file (production uses ~4 MiB / 1 s).
//!
//! The pure park decision (`checkpoint_stream::dest_park_continues`) is unit-tested
//! against a fake clock in `checkpoint_stream.rs`; these tests drive the whole
//! `copy_single_path` → `CheckpointStream` → `write_from_stream` path so the arm's
//! WIRING (opt-in gate, floor, cap, cancel-awareness, byte-exact resume) is
//! covered end to end.

use super::super::super::state::cancel_write_operation;
use super::super::super::test_support::TestOperationGuard;
use super::test_support::{
    AutoYieldTuningGuard, ForegroundBusyDest, PanicIfProbedDest, REL_CHUNK, REL_TOTAL, RelLog, ReleasingSource,
    make_state, park_holds_at, rel_expected_bytes,
};
use super::*;
use crate::test_support::wait_until_async;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use crate::file_system::volume::{Volume, VolumeError};

/// A near-instant SMB-shaped source so the destination arm (not source latency)
/// governs the copy's pacing. `ReleasingSource` streams the offset pattern and
/// does NOT opt into the SOURCE-side yield, so the source arm is a no-op.
fn releasing_source() -> (Arc<dyn Volume>, Arc<StdMutex<RelLog>>) {
    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource {
        log: Arc::clone(&log),
        gate: None,
    });
    (source, log)
}

#[tokio::test(flavor = "current_thread")]
async fn dest_yield_parks_before_next_write_then_resumes_byte_exact() {
    // Tiny floor (one chunk) so the arm fires early; a long cap so the park is
    // governed purely by the foreground flag for this test (the cap is exercised
    // by `dest_yield_hard_cap_bounds_the_park_under_continuous_browsing`).
    let _tuning =
        AutoYieldTuningGuard::with_dest_cap(Duration::from_millis(0), REL_CHUNK as u64, Duration::from_secs(10));

    let (source, source_log) = releasing_source();
    let foreground = Arc::new(AtomicBool::new(false));
    let written = Arc::new(StdMutex::new(Vec::<u8>::new()));
    let dest: Arc<dyn Volume> = Arc::new(ForegroundBusyDest {
        foreground: Arc::clone(&foreground),
        written: Arc::clone(&written),
    });

    let state = make_state();
    let bytes_seen = Arc::new(AtomicU64::new(0));

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let source_drv = Arc::clone(&source);
            let dest_drv = Arc::clone(&dest);
            let state_drv = Arc::clone(&state);
            let bytes_seen_drv = Arc::clone(&bytes_seen);
            let op = tokio::task::spawn_local(async move {
                let bytes_ref = &bytes_seen_drv;
                copy_single_path(
                    &source_drv,
                    Path::new("/movie.bin"),
                    false,
                    None,
                    &dest_drv,
                    Path::new("movie.bin"),
                    &state_drv,
                    &CreatedPaths::default(),
                    &|bytes_done, _total| {
                        bytes_ref.store(bytes_done, Ordering::SeqCst);
                        ControlFlow::Continue(())
                    },
                    &|_| {},
                    None,
                )
                .await
            });

            // Let the upload clear the floor, then mark the share busy. The in-memory
            // dest writes fast, so gate on the floor itself to be sure we catch it
            // mid-file.
            wait_until_async(
                Duration::from_secs(10),
                "the upload to clear the min-progress floor",
                || bytes_seen.load(Ordering::SeqCst) >= REL_CHUNK as u64,
            )
            .await;
            foreground.store(true, Ordering::SeqCst);

            // The upload must stop advancing (stand aside for the browsing) while
            // the share stays busy.
            let frozen = park_holds_at(
                &bytes_seen,
                "an upload yielding to the destination share must not advance while the share is busy",
            )
            .await;
            assert!(
                frozen > 0 && (frozen as usize) < REL_TOTAL,
                "the upload must have stood aside mid-file, short of completion; frozen={frozen}"
            );
            assert!(
                !op.is_finished(),
                "the upload must still be standing aside while the share is busy"
            );
            assert_eq!(
                source_log.lock().unwrap().releases,
                0,
                "a destination yield parks in place; it must NOT release the source"
            );

            // The share goes quiet: the upload resumes from the current offset and
            // completes byte-exact.
            foreground.store(false, Ordering::SeqCst);
            let bytes = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("the upload must resume once the share goes quiet")
                .expect("copy task must not panic")
                .expect("resumed upload must succeed");

            assert_eq!(bytes, REL_TOTAL as u64, "resumed upload reports the full byte count");
            assert_eq!(
                *written.lock().unwrap(),
                rel_expected_bytes(),
                "assembled bytes across a destination yield must equal a non-yielded upload exactly"
            );
            assert_eq!(
                source_log.lock().unwrap().opens,
                vec![0],
                "a single open at offset 0; the park never reopens or reseeks the source"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn dest_yield_hard_cap_bounds_the_park_under_continuous_browsing() {
    // THE data-safety test. The share stays busy for the WHOLE upload (the user
    // never stops browsing). With an UNBOUNDED park (the source-arm path) the
    // upload would stall forever and this test would hit its timeout. The hard cap
    // guarantees the park ends and the next chunk lands, so the upload completes,
    // keeping the open SMB write handle warm so the server can't reap it.
    let hard_cap = Duration::from_millis(40);
    let _tuning = AutoYieldTuningGuard::with_dest_cap(Duration::from_millis(0), REL_CHUNK as u64, hard_cap);

    let (source, _source_log) = releasing_source();
    // Foreground stays pending the WHOLE upload.
    let foreground = Arc::new(AtomicBool::new(true));
    let written = Arc::new(StdMutex::new(Vec::<u8>::new()));
    let dest: Arc<dyn Volume> = Arc::new(ForegroundBusyDest {
        foreground: Arc::clone(&foreground),
        written: Arc::clone(&written),
    });

    let state = make_state();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let source_drv = Arc::clone(&source);
            let dest_drv = Arc::clone(&dest);
            let state_drv = Arc::clone(&state);
            let started = Instant::now();
            let op = tokio::task::spawn_local(async move {
                copy_single_path(
                    &source_drv,
                    Path::new("/movie.bin"),
                    false,
                    None,
                    &dest_drv,
                    Path::new("movie.bin"),
                    &state_drv,
                    &CreatedPaths::default(),
                    &|_, _| ControlFlow::Continue(()),
                    &|_| {},
                    None,
                )
                .await
            });

            let bytes = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("under continuous browsing the bounded park must still let the upload COMPLETE (an unbounded park would hang here)")
                .expect("copy task must not panic")
                .expect("upload must succeed");
            let elapsed = started.elapsed();

            assert_eq!(bytes, REL_TOTAL as u64);
            assert_eq!(
                *written.lock().unwrap(),
                rel_expected_bytes(),
                "byte-exact even when every chunk was preceded by a capped park"
            );
            // It must actually have PARKED (repeatedly), not sailed through: an
            // unimpeded in-memory upload of `REL_TOTAL` is a few dozen ms, so a
            // multiple-hundred-ms floor proves the arm stood aside between writes.
            assert!(
                elapsed >= Duration::from_millis(200),
                "the upload must have repeatedly stood aside for the busy share; elapsed={elapsed:?}"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn dest_yield_cancel_while_parked_returns_cancelled_promptly() {
    // A cancel WHILE the upload is standing aside for the busy share must unblock
    // the park promptly (no hang): the park loop is cancel-aware, so it breaks, the
    // next chunk flows to `on_progress`, which returns `Break`, and the write ends
    // `Cancelled`. The in-memory buffer holds only a partial (the upload did not
    // finish), proving the cancel cut it off mid-transfer rather than racing to
    // completion. (Partial-FILE cleanup on cancel is the safe-replace machinery,
    // covered by the source-arm and rollback suites; this test owns the arm's
    // cancel-responsiveness.)
    let _tuning =
        AutoYieldTuningGuard::with_dest_cap(Duration::from_millis(0), REL_CHUNK as u64, Duration::from_secs(10));

    let (source, _source_log) = releasing_source();
    let foreground = Arc::new(AtomicBool::new(false));
    let written = Arc::new(StdMutex::new(Vec::<u8>::new()));
    let dest: Arc<dyn Volume> = Arc::new(ForegroundBusyDest {
        foreground: Arc::clone(&foreground),
        written: Arc::clone(&written),
    });

    let op = TestOperationGuard::register_state("test-dest-yield-cancel", make_state());
    let op_id = op.id().to_string();
    let state = Arc::clone(op.state());
    let bytes_seen = Arc::new(AtomicU64::new(0));

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let source_drv = Arc::clone(&source);
            let dest_drv = Arc::clone(&dest);
            let state_drv = Arc::clone(&state);
            let bytes_seen_drv = Arc::clone(&bytes_seen);
            let op = tokio::task::spawn_local(async move {
                let state_ref = &state_drv;
                let bytes_ref = &bytes_seen_drv;
                copy_single_path(
                    &source_drv,
                    Path::new("/movie.bin"),
                    false,
                    None,
                    &dest_drv,
                    Path::new("movie.bin"),
                    state_ref,
                    &CreatedPaths::default(),
                    &|bytes_done, _total| {
                        bytes_ref.store(bytes_done, Ordering::SeqCst);
                        if crate::file_system::write_operations::state::is_cancelled(&state_ref.intent) {
                            ControlFlow::Break(())
                        } else {
                            ControlFlow::Continue(())
                        }
                    },
                    &|_| {},
                    None,
                )
                .await
            });

            // Let it stream past the floor, then hold the share busy so it stands
            // aside (a long cap keeps it parked for the whole test window).
            wait_until_async(
                Duration::from_secs(10),
                "the upload to clear the min-progress floor",
                || bytes_seen.load(Ordering::SeqCst) >= REL_CHUNK as u64,
            )
            .await;
            foreground.store(true, Ordering::SeqCst);
            park_holds_at(
                &bytes_seen,
                "the upload must stand aside, not advance, while the share is busy",
            )
            .await;
            assert!(
                !op.is_finished(),
                "must be standing aside for the busy share before the cancel"
            );

            // Cancel (keep partials) WHILE parked. The cancel-aware park must bail
            // promptly instead of waiting out the cap.
            cancel_write_operation(&op_id, false);

            let result = tokio::time::timeout(Duration::from_secs(5), op)
                .await
                .expect("cancel during a destination yield must unblock the parked upload (no hang)")
                .expect("copy task must not panic");
            assert!(
                matches!(result, Err(VolumeError::Cancelled(_))),
                "cancel wins over a destination yield: the upload ends Cancelled, got {result:?}"
            );
            assert!(
                written.lock().unwrap().len() < REL_TOTAL,
                "a cancelled upload must not have written the whole file"
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn non_opting_dest_never_dest_yields() {
    // Regression guard on the enable-switch: a destination that does NOT opt into
    // `supports_foreground_yield_as_destination()` (the trait default: local FS,
    // in-memory, and MTP, whose one `SendObject` transaction can't pause mid-write)
    // must never reach the park. `PanicIfProbedDest` PANICS if its
    // `foreground_pending` is ever probed, so a regression that lets a non-opting
    // target park hard-fails here instead of silently pinning a write handle.
    let _tuning =
        AutoYieldTuningGuard::with_dest_cap(Duration::from_millis(0), REL_CHUNK as u64, Duration::from_secs(10));

    let (source, _source_log) = releasing_source();
    let written = Arc::new(StdMutex::new(Vec::<u8>::new()));
    let dest: Arc<dyn Volume> = Arc::new(PanicIfProbedDest {
        written: Arc::clone(&written),
    });
    assert!(
        !dest.supports_foreground_yield_as_destination(),
        "the double must NOT opt into the destination yield"
    );

    let state = make_state();
    let bytes = copy_single_path(
        &source,
        Path::new("/movie.bin"),
        false,
        None,
        &dest,
        Path::new("movie.bin"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|_| {},
        None,
    )
    .await
    .expect("copy must succeed");

    assert_eq!(bytes, REL_TOTAL as u64);
    assert_eq!(
        *written.lock().unwrap(),
        rel_expected_bytes(),
        "a non-opting destination uploads byte-exact and never parks"
    );
}
