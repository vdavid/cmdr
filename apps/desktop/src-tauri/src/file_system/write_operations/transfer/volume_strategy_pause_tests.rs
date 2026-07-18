//! Pause/resume tests for `volume_strategy.rs`'s `copy_single_path`.
//!
//! Two pause shapes:
//!
//! - **Mid-file (between-chunk) pause** for a generic streaming source: a
//!   multi-chunk copy must STOP advancing while paused, then complete on resume
//!   / unblock on cancel. The cross-volume streaming path's per-chunk progress
//!   callback is sync, so `stream_pipe_file` wraps the source stream in a
//!   `CheckpointStream` whose `next_chunk()` parks while paused once per chunk.
//!   The `SlowSource` double sleeps per chunk so "pause lands mid-file" is
//!   deterministic.
//! - **Bounded-window park-in-place** for an MTP-shaped source: an MTP read is
//!   bounded windows that hold nothing between chunks, so a paused MTP→local copy
//!   parks IN PLACE — it stops starting the next window and resumes from the
//!   current offset, byte-exact, with NO release/reopen. The `ReleasingSource`
//!   double records every open and counts any (now-unexpected) `cancel_and_release`,
//!   so no real device is needed. Includes a no-pause sanity case.

use super::super::super::state::{OperationIntent, WRITE_OPERATION_STATE, cancel_write_operation, load_intent};
use super::test_support::{
    REL_CHUNK, REL_TOTAL, RelLog, ReleasingSource, SLOW_CHUNK_COUNT, SLOW_CHUNK_SIZE, SlowSource, make_state,
    rel_expected_bytes,
};
use super::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume, Volume, VolumeError};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn streaming_copy_parks_mid_file_while_paused_then_resumes() {
    let source: Arc<dyn Volume> = Arc::new(SlowSource);
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    let total = (SLOW_CHUNK_COUNT * SLOW_CHUNK_SIZE) as u64;

    let state = make_state();
    let bytes_seen = Arc::new(AtomicU64::new(0));

    let source_drv = Arc::clone(&source);
    let dest_drv = Arc::clone(&dest);
    let state_drv = Arc::clone(&state);
    let bytes_seen_drv = Arc::clone(&bytes_seen);
    let op = tokio::spawn(async move {
        let bytes_ref = &bytes_seen_drv;
        copy_single_path(
            &source_drv,
            Path::new("/big.bin"),
            false,
            None,
            &dest_drv,
            Path::new("/big.bin"),
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

    // Let a few chunks stream, then pause MID-FILE.
    tokio::time::sleep(Duration::from_millis(30)).await;
    state.pause_gate.pause();

    // Sample twice across several chunk intervals: the byte count must freeze
    // (the wrapped stream parks before reading the next chunk) and the op must
    // not finish.
    tokio::time::sleep(Duration::from_millis(40)).await;
    let frozen = bytes_seen.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(
        bytes_seen.load(Ordering::SeqCst),
        frozen,
        "a paused multi-chunk copy must stop advancing mid-file"
    );
    assert!(
        frozen < total,
        "the copy must be parked short of completion while paused"
    );
    assert!(frozen > 0, "at least one chunk should have streamed before the pause");
    assert!(!op.is_finished(), "the copy task must still be parked while paused");
    assert_eq!(
        load_intent(&state.intent),
        OperationIntent::Running,
        "pause must not touch OperationIntent"
    );

    // Resume → completes with the full byte count.
    state.pause_gate.resume();
    let bytes = tokio::time::timeout(Duration::from_secs(10), op)
        .await
        .expect("resumed copy must complete")
        .expect("copy task must not panic")
        .expect("resumed copy must succeed");
    assert_eq!(bytes, total, "resumed copy reports the full byte count");
    assert_eq!(
        dest.get_metadata(Path::new("/big.bin")).await.unwrap().size,
        Some(total),
        "resumed copy lands the full file at the destination"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn streaming_copy_cancel_while_paused_mid_file_unblocks() {
    use std::fs;

    let source: Arc<dyn Volume> = Arc::new(SlowSource);
    // Local-FS destination — the user's real case is MTP→local, whose dest is
    // `LocalPosixVolume`. On cancel its `write_from_stream` returns typed
    // `VolumeError::Cancelled` and removes the in-flight partial.
    let dst_dir = std::env::temp_dir().join(format!("cmdr_midchunk_cancel_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    // Install into the global state cache so the production cancel API reaches it.
    let op_id = format!("test-midchunk-cancel-{:?}", std::thread::current().id());
    let state = make_state();
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    let bytes_seen = Arc::new(AtomicU64::new(0));
    let source_drv = Arc::clone(&source);
    let dest_drv = Arc::clone(&dest);
    let state_drv = Arc::clone(&state);
    let bytes_seen_drv = Arc::clone(&bytes_seen);
    let op = tokio::spawn(async move {
        let bytes_ref = &bytes_seen_drv;
        let state_ref = &state_drv;
        copy_single_path(
            &source_drv,
            Path::new("/big.bin"),
            false,
            None,
            &dest_drv,
            Path::new("big.bin"),
            state_ref,
            &CreatedPaths::default(),
            // Mirror the production per-file callback (`SerialLeafProgress::on_chunk`):
            // break on cancel so the backend's chunk loop tears down the partial.
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

    tokio::time::sleep(Duration::from_millis(30)).await;
    state.pause_gate.pause();
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(!op.is_finished(), "parked while paused");

    // Cancel (keep partials) while paused: the production cancel path flips
    // intent AND wakes the gate, so the parked stream unblocks and the backend's
    // on_progress cancel check breaks + cleans up the partial.
    cancel_write_operation(&op_id, false);

    let result = tokio::time::timeout(Duration::from_secs(10), op)
        .await
        .expect("cancel-while-paused must unblock the parked copy")
        .expect("copy task must not panic");
    assert!(
        matches!(result, Err(VolumeError::Cancelled(_))),
        "cancel wins over pause: the copy ends Cancelled, got {result:?}"
    );
    assert_eq!(
        load_intent(&state.intent),
        OperationIntent::Stopped,
        "keep-partials cancel lands on Stopped"
    );
    // Keep-partials: the local sink removes its in-flight file on the cancel
    // break, so no torn target is left behind.
    assert!(
        !dest.exists(Path::new("big.bin")).await,
        "a cancelled mid-file copy leaves no partial dest file"
    );

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
    let _ = fs::remove_dir_all(&dst_dir);
}

/// Poll `seen` until it reaches at least `target` bytes, then return; panic after a
/// generous budget. Lets the pause test wait on real copy progress instead of a
/// fixed sleep, so a descheduled runtime can't race the assertion.
async fn wait_for_bytes(seen: &AtomicU64, target: u64) {
    for _ in 0..3_000 {
        if seen.load(Ordering::SeqCst) >= target {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!(
        "bytes_seen never reached {target}; last = {}",
        seen.load(Ordering::SeqCst)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paused_mtp_copy_parks_in_place_then_resumes_byte_exact() {
    use std::fs;

    let log = Arc::new(StdMutex::new(RelLog::default()));
    // A chunk-budget gate lets us hold the source at an exact byte offset, so the
    // pause lands deterministically mid-file instead of racing a wall-clock timer.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource {
        log: Arc::clone(&log),
        gate: Some(Arc::clone(&gate)),
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_relpause_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let state = make_state();
    let bytes_seen = Arc::new(AtomicU64::new(0));

    let source_drv = Arc::clone(&source);
    let dest_drv = Arc::clone(&dest);
    let state_drv = Arc::clone(&state);
    let bytes_seen_drv = Arc::clone(&bytes_seen);
    let op = tokio::spawn(async move {
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

    // Release exactly one window, then wait until it lands. The source now blocks
    // on the next permit, so the copy is provably mid-stream (one window written).
    gate.add_permits(1);
    wait_for_bytes(&bytes_seen, REL_CHUNK as u64).await;

    // Pause MID-FILE, then lift the budget entirely. The source can now serve every
    // remaining window, so if the copy still stops it's because the PAUSE parked it,
    // not because it ran out of chunks. The window already past its checkpoint
    // completes, then the copy parks at the next one: it settles at exactly two
    // windows. NOTHING is released — the bounded-window read holds no session
    // between windows, so there's nothing to free.
    state.pause_gate.pause();
    gate.add_permits(REL_TOTAL / REL_CHUNK + 4);
    wait_for_bytes(&bytes_seen, 2 * REL_CHUNK as u64).await;
    let frozen = bytes_seen.load(Ordering::SeqCst);

    assert!(
        frozen > 0 && (frozen as usize) < REL_TOTAL,
        "must be parked short of completion"
    );
    assert!(!op.is_finished(), "the copy task must still be parked while paused");
    assert_eq!(
        log.lock().unwrap().releases,
        0,
        "pause must NOT release the source stream — bounded windows hold nothing to free (park-in-place)"
    );

    // The park must HOLD: with the budget wide open, an UNpaused copy would race to
    // completion, so a stable offset across this grace window proves the pause (not
    // a starved source) is what's holding it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        bytes_seen.load(Ordering::SeqCst),
        frozen,
        "a paused copy must stop starting the next window mid-file"
    );
    assert!(!op.is_finished(), "the copy task must still be parked while paused");

    // Resume → keeps reading from the current offset and completes.
    state.pause_gate.resume();
    let bytes = tokio::time::timeout(Duration::from_secs(10), op)
        .await
        .expect("resumed copy must complete")
        .expect("copy task must not panic")
        .expect("resumed copy must succeed");

    assert_eq!(bytes, REL_TOTAL as u64, "resumed copy reports the full byte count");

    // The destination must hold EXACTLY the full file, byte-for-byte equal to a
    // non-paused copy: parking left the offset alone, so the next window read
    // `[offset, …)` with no gap or overlap.
    let written = fs::read(dst_dir.join("movie.bin")).unwrap();
    assert_eq!(
        written,
        rel_expected_bytes(),
        "assembled bytes must equal the source exactly"
    );

    // The stream was opened ONCE at offset 0 and never reopened — no release, no
    // reopen-at-offset in the bounded-window model.
    let opens = log.lock().unwrap().opens.clone();
    assert_eq!(opens, vec![0], "a single open at offset 0; no reopen while paused");

    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paused_mtp_copy_cancel_while_paused_keeps_no_partial() {
    use std::fs;

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource {
        log: Arc::clone(&log),
        gate: None,
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_relpause_cancel_dst_{:?}", std::thread::current().id()));
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&dst_dir).unwrap();
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let op_id = format!("test-relpause-cancel-{:?}", std::thread::current().id());
    let state = make_state();
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    let source_drv = Arc::clone(&source);
    let dest_drv = Arc::clone(&dest);
    let state_drv = Arc::clone(&state);
    let op = tokio::spawn(async move {
        let state_ref = &state_drv;
        copy_single_path(
            &source_drv,
            Path::new("/movie.bin"),
            false,
            None,
            &dest_drv,
            Path::new("movie.bin"),
            state_ref,
            &CreatedPaths::default(),
            // Mirror the production per-file callback: break on cancel so the
            // backend's chunk loop tears down the partial.
            &|_bytes_done, _total| {
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

    tokio::time::sleep(Duration::from_millis(30)).await;
    state.pause_gate.pause();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(!op.is_finished(), "parked while paused");
    assert_eq!(
        log.lock().unwrap().releases,
        0,
        "pause parks in place — no release in the bounded-window model"
    );

    // Cancel (keep partials) while paused. The parked stream unblocks and the
    // next chunk flows through to on_progress, which breaks on cancel and the
    // local sink removes its in-flight temp.
    cancel_write_operation(&op_id, false);

    let result = tokio::time::timeout(Duration::from_secs(10), op)
        .await
        .expect("cancel-while-paused must unblock the parked copy")
        .expect("copy task must not panic");
    assert!(
        matches!(result, Err(VolumeError::Cancelled(_))),
        "cancel wins over pause: the copy ends Cancelled, got {result:?}"
    );
    assert!(
        !dest.exists(Path::new("movie.bin")).await,
        "a cancelled mid-file copy leaves no partial dest file"
    );

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unpaused_mtp_copy_streams_straight_through() {
    use std::fs;

    // Sanity: with no pause, an MTP-shaped source streams straight through with a
    // single open and no release — the pause/yield machinery stays dormant.
    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource {
        log: Arc::clone(&log),
        gate: None,
    });

    let dst_dir = std::env::temp_dir().join(format!("cmdr_relpause_nopause_dst_{:?}", std::thread::current().id()));
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
    .expect("unpaused copy must succeed");

    assert_eq!(bytes, REL_TOTAL as u64);
    assert_eq!(fs::read(dst_dir.join("movie.bin")).unwrap(), rel_expected_bytes());
    let l = log.lock().unwrap();
    assert_eq!(l.releases, 0, "no pause ⇒ no release");
    assert_eq!(l.opens, vec![0], "no pause ⇒ a single open at offset 0");

    let _ = fs::remove_dir_all(&dst_dir);
}
