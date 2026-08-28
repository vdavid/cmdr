//! The conflict-dispatch mutex that serializes the human across concurrent and
//! nested merges. Two deep clashes on the concurrent path each get answered in
//! turn through the single oneshot slot, a top-level clash and a deep one share
//! that same mutex so neither clobbers the other's slot, and a cancel landing
//! while one task is parked on it unblocks both without a hang.
//!
//! These drive the real `copy_volumes_with_progress` pipeline against
//! `InMemoryVolume` pairs + `CollectorEventSink`, so the whole stack (preflight,
//! the serial/concurrent split, `copy_directory_streaming`, the resolver) runs
//! exactly as in production. Shared fixtures `make_state` / `make_volumes` live in
//! `volume/copy_tests.rs` (`super::tests`). Per-file conflict resolution inside a
//! merge is `volume/merge_tests.rs`.

use super::super::super::conflict_responder_test_support::{ConflictResponderSink, file_conflict_count};
use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::ConflictResolution;

// ============================================================================
// Helpers
// ============================================================================
/// Reads a whole file from a volume into a `Vec<u8>`.
async fn read_all(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.unwrap();
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    out
}

// ============================================================================
// Conflict-dispatch mutex
// ============================================================================

/// Concurrent merge with two deep clashes (across two top-level sources, taking
/// the FuturesUnordered concurrent path) under Stop: the dispatch mutex
/// serializes the prompts so each one gets answered through the single oneshot
/// slot. Both clashes resolve, both dest-only files survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_merge_with_two_deep_clashes_serializes_prompts() {
    let (source, dest) = make_volumes();

    // 3 top-level sources ⇒ concurrent path (>=3 and InMemory max_concurrent=32).
    // Two of them are merging dirs that each hide a deep file clash; the third is
    // a plain fresh file so the batch is unambiguously concurrent.
    for d in ["one", "two"] {
        source.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        source
            .create_file(Path::new(&format!("/{d}/clash.bin")), &vec![1u8; 50_000])
            .await
            .unwrap();
        dest.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        dest.create_file(Path::new(&format!("/{d}/clash.bin")), &vec![9u8; 50_000])
            .await
            .unwrap();
        dest.create_file(Path::new(&format!("/{d}/keep.txt")), b"KEEP")
            .await
            .unwrap();
    }
    source.create_file(Path::new("/three.txt"), b"THREE").await.unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-concurrent-two-clashes",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/one"),
            PathBuf::from("/two"),
            PathBuf::from("/three.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // Sink-derived: both deep clashes prompted (the dispatch mutex serialized
    // them through the single oneshot slot, each answered in turn).
    let n = file_conflict_count(&events.inner);
    assert_eq!(n, 2, "both deep clashes should prompt and be answered, got {n}");

    // Both clashes overwritten (50_000 bytes of 1u8), both dest-only files kept.
    for d in ["one", "two"] {
        assert_eq!(read_all(&dest, &format!("/{d}/clash.bin")).await, vec![1u8; 50_000]);
        assert_eq!(read_all(&dest, &format!("/{d}/keep.txt")).await, b"KEEP");
    }
    assert!(dest.exists(Path::new("/three.txt")).await);
}

/// Top-level vs deep race: a top-level file clash and a deep file clash (inside a
/// merging dir) both surface under Stop on the concurrent path. The SAME dispatch
/// mutex guards both, so neither clobbers the other's oneshot. We answer both
/// with "…all" Overwrite; everything overwrites, dest-only file survives.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn top_level_and_deep_clash_share_the_dispatch_mutex() {
    let (source, dest) = make_volumes();

    // Top-level file clash.
    source.create_file(Path::new("/top.txt"), b"SRC-top").await.unwrap();
    dest.create_file(Path::new("/top.txt"), b"DEST-top").await.unwrap();
    // A merging dir with a deep file clash + dest-only file.
    source.create_directory(Path::new("/dir")).await.unwrap();
    source
        .create_file(Path::new("/dir/clash.txt"), b"SRC-deep")
        .await
        .unwrap();
    dest.create_directory(Path::new("/dir")).await.unwrap();
    dest.create_file(Path::new("/dir/clash.txt"), b"DEST-deep")
        .await
        .unwrap();
    dest.create_file(Path::new("/dir/keep.txt"), b"KEEP").await.unwrap();
    // Third source to force the concurrent path.
    source.create_file(Path::new("/extra.txt"), b"EXTRA").await.unwrap();

    let state = make_state();
    // One "…all" answer collapses any queued prompt via the latch double-check,
    // so at most 2 prompts ever emit (top + deep), often just 1.
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, true));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-top-vs-deep",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/top.txt"),
            PathBuf::from("/dir"),
            PathBuf::from("/extra.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Both clashes overwritten by the "…all" choice; dest-only file survives.
    assert_eq!(read_all(&dest, "/top.txt").await, b"SRC-top");
    assert_eq!(read_all(&dest, "/dir/clash.txt").await, b"SRC-deep");
    assert_eq!(read_all(&dest, "/dir/keep.txt").await, b"KEEP");
}

/// Cancel-while-queued: task A is awaiting a Stop prompt while task B is parked
/// on the dispatch mutex. Cancel must unblock BOTH and return without a hang
/// (task B's cancel-check inside the resolver bails before emitting a prompt no
/// one would answer). The op terminates (the test simply completing is the
/// no-hang assertion).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_while_queued_unblocks_both_no_hang() {
    let (source, dest) = make_volumes();

    // Two merging dirs that each hide a deep file clash, plus a third source for
    // the concurrent path. Both deep clashes hit Stop; one task ends up parked on
    // the dispatch mutex behind the other.
    for d in ["one", "two"] {
        source.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        source
            .create_file(Path::new(&format!("/{d}/clash.bin")), &vec![1u8; 50_000])
            .await
            .unwrap();
        dest.create_directory(Path::new(&format!("/{d}"))).await.unwrap();
        dest.create_file(Path::new(&format!("/{d}/clash.bin")), &vec![9u8; 50_000])
            .await
            .unwrap();
    }
    source.create_file(Path::new("/three.txt"), b"THREE").await.unwrap();

    let state = make_state();
    let op = TestOperationGuard::register_state("cancel-while-queued", Arc::clone(&state));
    let op_id = op.id();

    // Wait until the FIRST prompt is installed (proving task A is awaiting and
    // task B is queued on the mutex), then cancel via the public path. Cancelling
    // drops the oneshot sender (unblocking A) and flips intent (so B's in-resolver
    // cancel-check bails). Neither re-emits.
    let state_for_cancel = Arc::clone(&state);
    let op_id_for_cancel = op_id.to_string();
    let canceller = tokio::spawn(async move {
        // A sender is installed while task A awaits the prompt (task B is then
        // queued behind it on the dispatch mutex).
        crate::test_support::wait_until_async(Duration::from_secs(5), "the Stop prompt to be installed", || {
            state_for_cancel.conflict_slot.is_awaiting()
        })
        .await;
        // Task B parking on the dispatch mutex is not observable from here, and this test's whole
        // point is the cancel landing WHILE B is queued, so we give B a beat to reach the mutex.
        // allowed-test-sleep: no signal for "B is queued on the dispatch mutex"; the window creates the race.
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel_write_operation(&op_id_for_cancel, false);
    });

    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    // The whole op must terminate. A 20 s timeout converts a hang into a test
    // failure instead of wedging the suite.
    let driven = tokio::time::timeout(
        Duration::from_secs(20),
        copy_volumes_with_progress(
            events.clone(),
            op_id,
            &state,
            Arc::clone(&source),
            &[
                PathBuf::from("/one"),
                PathBuf::from("/two"),
                PathBuf::from("/three.txt"),
            ],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        ),
    )
    .await;

    canceller.await.unwrap();
    assert!(
        driven.is_ok(),
        "operation hung after cancel-while-queued (dispatch-mutex deadlock)"
    );
    // Cancelled → the op returns Err(Cancelled). Dest-only nothing here; the
    // no-hang completion is the assertion that matters.
    let result = driven.unwrap();
    assert!(result.is_err(), "cancelled op should return Err, got {result:?}");
}
