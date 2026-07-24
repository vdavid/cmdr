//! Foreground auto-yield tests for `volume_strategy.rs`'s `copy_single_path`.
//!
//! A RUNNING (not paused) MTP→local copy must, when foreground work pends on the
//! source device, stop starting the next window, wait for foreground to drain
//! plus a debounce window, then resume from the current offset — byte-exact, op
//! stays Running, with NO session release/reopen (bounded windows hold nothing
//! between chunks). A min-progress floor keeps continuous foreground nav from
//! starving the copy to zero throughput, and a debounce collapses a burst of
//! listings into one park.
//!
//! The `YieldingSource` double opts into `supports_foreground_yield()` and
//! carries a controllable `foreground_pending` signal — the test-double of
//! `MtpVolume` + its device priority gate, so no real device is needed.
//! `AutoYieldTuningGuard` injects a near-zero debounce and a tiny floor so the
//! arm fires deterministically over the small synthetic file (production uses the
//! named constants, ~400 ms / 4 MiB). Two regression guards pin that a source
//! that does NOT opt into foreground-yield never yields, and that a yield-capable
//! source with nothing pending never self-yields.

use super::super::super::state::{OperationIntent, cancel_write_operation, load_intent};
use super::super::super::test_support::TestOperationGuard;
use super::test_support::{
    AutoYieldTuningGuard, NeverPendingYieldSource, PARK_WINDOW, REL_CHUNK, REL_TOTAL, RelLog, ReleasingSource,
    YieldingSource, make_state, park_holds_at, rel_expected_bytes,
};
use super::*;
use crate::test_support::wait_until_async;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};

#[tokio::test(flavor = "current_thread")]
async fn auto_yield_parks_before_next_window_then_resumes_byte_exact() {
    use std::fs;

    // Near-zero debounce, tiny floor (one chunk) so the arm fires within the
    // small synthetic file.
    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(0), REL_CHUNK as u64);

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let foreground = Arc::new(AtomicBool::new(false));
    let source: Arc<dyn Volume> = Arc::new(YieldingSource {
        log: Arc::clone(&log),
        foreground: Arc::clone(&foreground),
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_autoyield_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let state = make_state();
    let bytes_seen = Arc::new(AtomicU64::new(0));

    // A `LocalSet` so the copy runs on THIS thread (sharing the thread-local
    // tuning override) while the controller below drives `foreground`.
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

            // Stream past the min-progress floor, then raise foreground.
            wait_until_async(
                Duration::from_secs(10),
                "the copy to clear the min-progress floor",
                || bytes_seen.load(Ordering::SeqCst) >= REL_CHUNK as u64,
            )
            .await;
            foreground.store(true, Ordering::SeqCst);

            // The copy must NOT start the next window (freeze) while foreground is
            // held — but it must NOT release the source either (park-in-place).
            let frozen = park_holds_at(
                &bytes_seen,
                "a copy yielding to foreground must stop advancing while foreground is held",
            )
            .await;
            assert!(
                frozen > 0 && (frozen as usize) < REL_TOTAL,
                "must have yielded mid-file, short of completion"
            );
            assert_eq!(
                log.lock().unwrap().releases,
                0,
                "a foreground yield must NOT release the source — it parks before the next window"
            );
            assert!(
                !op.is_finished(),
                "the copy must still be yielding while foreground is held"
            );
            assert_eq!(
                load_intent(&state.intent),
                OperationIntent::Running,
                "an auto-yield must NOT touch OperationIntent — the op stays Running"
            );

            // Drop foreground: the copy resumes from the current offset and completes.
            foreground.store(false, Ordering::SeqCst);
            let bytes = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("copy must resume after foreground drains")
                .expect("copy task must not panic")
                .expect("resumed copy must succeed");

            assert_eq!(bytes, REL_TOTAL as u64, "resumed copy reports the full byte count");
            let written = fs::read(dst_dir.join("movie.bin")).unwrap();
            assert_eq!(
                written,
                rel_expected_bytes(),
                "assembled bytes across an auto-yield must equal a non-yielded copy exactly"
            );

            let opens = log.lock().unwrap().opens.clone();
            assert_eq!(opens, vec![0], "a single open at offset 0; no reopen across the yield");
        })
        .await;

    let _ = fs::remove_dir_all(&dst_dir);
}

// The clock is PAUSED for this test: tokio auto-advances virtual time only when
// every task is idle, so each `sleep` below is an exact, instant jump to a point in
// the burst rather than a wall-clock bet. That's what makes "the second listing
// landed inside the quiet window" a fact instead of a race, and it runs in ~30 ms.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn auto_yield_debounces_a_burst_into_one_park() {
    use std::fs;

    // A real (long) debounce window so a brief gap between two listings is
    // collapsed into ONE park, not two. Floor = one chunk so the arm fires early.
    // The debounce here (120 ms) is wide enough that the second listing lands
    // inside the quiet window after the first drains.
    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(120), REL_CHUNK as u64);

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let foreground = Arc::new(AtomicBool::new(false));
    let source: Arc<dyn Volume> = Arc::new(YieldingSource {
        log: Arc::clone(&log),
        foreground: Arc::clone(&foreground),
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_autoyield_burst_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

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

            // First listing: stream past the floor, raise, then drop. The copy
            // parks (doesn't start the next window) and enters the debounce window.
            // A virtual-clock advance to "several windows in" on the paused runtime; it IS the
            // burst's shape, which is this test's whole subject.
            // allowed-test-sleep: virtual-time advance on a start_paused runtime; the burst shape under test.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(true, Ordering::SeqCst);
            // allowed-test-sleep: how long the first listing holds the device, in virtual time.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(false, Ordering::SeqCst);
            // Sample the parked offset: the copy has been parked since ~the raise
            // and is now in the quiet window.
            let parked = bytes_seen.load(Ordering::SeqCst);
            assert!(
                parked > 0 && (parked as usize) < REL_TOTAL,
                "must be parked mid-file before the burst's second listing"
            );

            // Second listing arrives BEFORE the quiet window elapses. If the
            // debounce collapses the burst, the copy stays parked the whole time
            // (no next window between the two listings); without it, the copy would
            // have resumed and advanced in the gap.
            // THE subject: this gap must be shorter than the 120 ms debounce, so the second listing
            // lands inside the quiet window. Virtual time makes it exact.
            // allowed-test-sleep: virtual-time advance on a start_paused runtime; the debounce window under test.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(true, Ordering::SeqCst);
            // allowed-test-sleep: how long the second listing holds the device, in virtual time.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(false, Ordering::SeqCst);
            assert_eq!(
                bytes_seen.load(Ordering::SeqCst),
                parked,
                "a burst within the debounce window must collapse into ONE park — the copy must not start a window between the two listings"
            );

            let bytes = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("copy must finish")
                .expect("copy task must not panic")
                .expect("copy must succeed");

            assert_eq!(bytes, REL_TOTAL as u64);
            assert_eq!(fs::read(dst_dir.join("movie.bin")).unwrap(), rel_expected_bytes());
            assert_eq!(
                log.lock().unwrap().releases,
                0,
                "the bounded-window model parks; it never releases the source"
            );
        })
        .await;

    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "current_thread")]
async fn auto_yield_min_progress_floor_prevents_starvation() {
    use std::fs;

    // Continuous foreground pressure (never drops) with a near-zero debounce.
    // The min-progress floor must still let the copy advance by >= floor between
    // yields, so it makes forward progress instead of starving to zero.
    let floor = REL_CHUNK as u64; // one chunk per cycle
    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(0), floor);

    let log = Arc::new(StdMutex::new(RelLog::default()));
    // Foreground stays pending the WHOLE copy.
    let foreground = Arc::new(AtomicBool::new(true));
    let source: Arc<dyn Volume> = Arc::new(YieldingSource {
        log: Arc::clone(&log),
        foreground: Arc::clone(&foreground),
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_autoyield_floor_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

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

            // The wait_until_foreground_idle never returns while foreground is held, so
            // to let the copy progress we must release foreground briefly each cycle —
            // BUT the floor guarantees that between two yields the copy advances >= floor.
            // Drive several cycles: confirm steady forward progress, never frozen.
            let mut last = 0u64;
            let mut advanced_cycles = 0;
            for _ in 0..8 {
                // Allow a drain so a parked yield can resume.
                foreground.store(false, Ordering::SeqCst);
                // The drain half of a browse cycle. Waiting on progress here would assume the very
                // forward progress the floor is supposed to guarantee, which is what this test measures.
                // allowed-test-sleep: the drain half of the browse cycle; the floor's forward progress is under test.
                tokio::time::sleep(PARK_WINDOW / 2).await;
                foreground.store(true, Ordering::SeqCst);
                // The busy half of the cycle, held long enough for the arm to park again before the sample.
                // allowed-test-sleep: the busy half of the browse cycle; holds until the arm re-parks.
                tokio::time::sleep(PARK_WINDOW / 2).await;
                let now = bytes_seen.load(Ordering::SeqCst);
                if now >= last + floor {
                    advanced_cycles += 1;
                }
                last = now;
                if op.is_finished() {
                    break;
                }
            }
            assert!(
                advanced_cycles >= 2,
                "the min-progress floor must let the copy advance by >= floor across cycles (no zero-throughput starvation); advanced_cycles={advanced_cycles}"
            );

            // Let it finish.
            foreground.store(false, Ordering::SeqCst);
            let bytes = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("copy must finish")
                .expect("copy task must not panic")
                .expect("copy must succeed");
            assert_eq!(bytes, REL_TOTAL as u64);
            assert_eq!(fs::read(dst_dir.join("movie.bin")).unwrap(), rel_expected_bytes());
        })
        .await;

    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "current_thread")]
async fn auto_yield_cancel_while_yielding_keeps_no_partial() {
    use std::fs;

    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(400), REL_CHUNK as u64);

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let foreground = Arc::new(AtomicBool::new(false));
    let source: Arc<dyn Volume> = Arc::new(YieldingSource {
        log: Arc::clone(&log),
        foreground: Arc::clone(&foreground),
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_autoyield_cancel_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let op = TestOperationGuard::register_state("test-autoyield-cancel", make_state());
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

            // Let it stream past the floor, then hold foreground so it yields and parks
            // in the (long) debounce window.
            wait_until_async(
                Duration::from_secs(10),
                "the copy to clear the min-progress floor",
                || bytes_seen.load(Ordering::SeqCst) >= REL_CHUNK as u64,
            )
            .await;
            foreground.store(true, Ordering::SeqCst);
            let parked_at =
                park_holds_at(&bytes_seen, "the copy must park, not advance, while foreground is held").await;
            assert!(
                parked_at >= REL_CHUNK as u64 && (parked_at as usize) < REL_TOTAL,
                "the cancel must land on a copy parked MID-FILE, past the floor and short of the end; parked_at={parked_at}"
            );
            assert_eq!(log.lock().unwrap().releases, 0, "parked, not released");
            assert!(!op.is_finished(), "parked in the debounce window");

            // Cancel (keep partials) WHILE yielding. The cancel-aware debounce must bail
            // promptly (the `select!` races the foreground-idle park against cancel);
            // the next chunk then flows to on_progress, which breaks on cancel and the
            // local sink removes its in-flight temp. The cancel must not hang.
            cancel_write_operation(&op_id, false);

            let result = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("cancel during an auto-yield must unblock the parked copy (no hang)")
                .expect("copy task must not panic");
            assert!(
                matches!(result, Err(VolumeError::Cancelled(_))),
                "cancel wins over an auto-yield: the copy ends Cancelled, got {result:?}"
            );
            assert!(
                !dest.exists(Path::new("movie.bin")).await,
                "a cancelled auto-yielding copy leaves no partial dest file"
            );
        })
        .await;
    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "current_thread")]
async fn non_mtp_source_never_auto_yields_for_foreground() {
    // Regression guard: a source that does NOT support foreground yield
    // (`supports_foreground_yield() == false`, the trait default — local FS and
    // in-memory; MTP and SMB opt in) must never auto-yield, even with a tiny
    // floor. `ReleasingSource` is MTP-shaped but does NOT opt into
    // foreground-yield, so it's the right double — it parks in place for pause but
    // never yields for foreground.
    use std::fs;

    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(0), REL_CHUNK as u64);

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource {
        log: Arc::clone(&log),
        gate: None,
    });
    assert!(
        !source.supports_foreground_yield(),
        "the double must NOT support foreground yield"
    );

    let dst_dir = std::env::temp_dir().join(format!("cmdr_autoyield_nonmtp_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

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
    assert_eq!(fs::read(dst_dir.join("movie.bin")).unwrap(), rel_expected_bytes());
    let l = log.lock().unwrap();
    assert_eq!(l.releases, 0, "no foreground yield ⇒ no release");
    assert_eq!(l.opens, vec![0], "no release ⇒ a single open at offset 0");

    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "current_thread")]
async fn yield_capable_source_with_no_foreground_pending_never_self_yields() {
    // A copy from a yield-capable source with NOTHING pending in foreground must
    // run to completion at full speed and NEVER park itself. The tiny floor means
    // the floor gate is satisfied early, so the ONLY thing keeping the arm from
    // parking is `foreground_pending()` being false — exactly the property a
    // self-yield livelock (a window read raising its own foreground_pending, or
    // the enable-switch regressing) would violate. The source panics if the arm
    // ever parks, so this hard-fails on regression instead of silently freezing.
    use std::fs;

    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(0), REL_CHUNK as u64);

    let opens = Arc::new(StdMutex::new(Vec::<u64>::new()));
    let source: Arc<dyn Volume> = Arc::new(NeverPendingYieldSource {
        opens: Arc::clone(&opens),
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_no_self_yield_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let state = make_state();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let source_drv = Arc::clone(&source);
            let dest_drv = Arc::clone(&dest);
            let state_drv = Arc::clone(&state);
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
                .expect("a copy with no foreground pending must complete, never self-park")
                .expect("copy task must not panic (a panic = self-yield livelock)")
                .expect("copy must succeed");

            assert_eq!(bytes, REL_TOTAL as u64);
            assert_eq!(fs::read(dst_dir.join("movie.bin")).unwrap(), rel_expected_bytes());
            assert_eq!(*opens.lock().unwrap(), vec![0], "a single open at offset 0; no reopen");
        })
        .await;

    let _ = fs::remove_dir_all(&dst_dir);
}
