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
//! `volume/copy_tests/` (`super::tests`). Per-file conflict resolution inside a
//! merge is `volume/merge_tests.rs`.

use super::super::super::conflict_responder_test_support::{
    ConflictResponderSink, await_prompted_clash, file_conflict_count,
};
use super::super::faulty_volume::forward_volume_methods;
use super::tests::{make_state, make_volumes};
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::state::ConflictResolutionResponse;
use crate::testing::TestOperations;

use crate::types::ConflictResolution;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeReadStream};

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

/// A source whose `held` files can't be read until their `release` says so.
/// That ordering is what makes the driver queue on the dispatch mutex BEHIND a
/// deep merge's prompt rather than ahead of it: the merge walker reaches its
/// clash while the held files still fill the window's other slots, and only
/// once the prompt is up does a slot free and the driver move on to prepare the
/// next source. `width` is the window.
struct HeldReadSource {
    inner: Arc<InMemoryVolume>,
    width: usize,
    held: Vec<HeldRead>,
}

/// One file [`HeldReadSource`] holds back.
struct HeldRead {
    path: PathBuf,
    release: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Once released, the read fails instead of streaming. An `IoError` with no
    /// errno isn't retryable, so the task fails on its first attempt.
    then_fail: bool,
}

impl Volume for HeldReadSource {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file, create_directory,
        create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        operations_are_local, scan_for_copy, supports_unknown_length_writes, write_from_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn max_concurrent_ops(&self) -> usize {
        self.width
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(held) = self.held.iter().find(|held| held.path == path) {
                let release = Arc::clone(&held.release);
                cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the held read's release", move || {
                    release()
                })
                .await;
                if held.then_fail {
                    return Err(VolumeError::IoError {
                        message: "simulated source read failure".into(),
                        raw_os_error: None,
                    });
                }
            }
            self.inner.open_read_stream(path).await
        })
    }
}

/// The ERR-8MEX6 shape, on a two-wide window: `/album` merges into an existing
/// folder and clashes on a child, `/sib.bin` copies fresh, and `/top.txt`
/// clashes at the top level. Hands back the source and the destination;
/// `deep_prompt_sources` is the selection, in order.
async fn deep_prompt_then_top_level_clash(state: &Arc<WriteOperationState>) -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let (inner, dest) = deep_prompt_fixture().await;
    let prompted = Arc::clone(state);
    let source: Arc<dyn Volume> = Arc::new(HeldReadSource {
        inner,
        width: 2,
        held: vec![HeldRead {
            path: PathBuf::from("/sib.bin"),
            // The deep prompt is up, so the walker holds the dispatch mutex.
            release: Arc::new(move || prompted.conflict_slot.is_awaiting()),
            then_fail: false,
        }],
    });
    (source, dest)
}

/// The volumes behind [`deep_prompt_then_top_level_clash`], before any source
/// holds anything back.
async fn deep_prompt_fixture() -> (Arc<InMemoryVolume>, Arc<dyn Volume>) {
    let inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    inner.create_directory(Path::new("/album")).await.unwrap();
    inner
        .create_file(Path::new("/album/clash.txt"), b"SRC-deep")
        .await
        .unwrap();
    inner.create_file(Path::new("/sib.bin"), b"SIB").await.unwrap();
    inner.create_file(Path::new("/top.txt"), b"SRC-top").await.unwrap();

    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/clash.txt"), b"DEST-deep")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/keep.txt"), b"KEEP").await.unwrap();
    dest.create_file(Path::new("/top.txt"), b"DEST-top").await.unwrap();
    (inner, dest)
}

fn deep_prompt_sources() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/album"),
        PathBuf::from("/sib.bin"),
        PathBuf::from("/top.txt"),
    ]
}

/// Waits until `/sib.bin` has landed, which frees the window's slot and sends
/// the driver on to `/top.txt`, then gives it a beat to queue on the dispatch
/// mutex behind the deep prompt.
async fn until_the_driver_queues_behind_the_deep_prompt(events: &CollectorEventSink) {
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "/sib.bin to land", || {
        events
            .source_items_done
            .lock_ignore_poison()
            .iter()
            .any(|done| done.source_path == "/sib.bin")
    })
    .await;
    // allowed-test-sleep: "the driver is queued on the dispatch mutex" has no
    // observable signal; the pre-check and the latch lookup ahead of it take
    // microseconds on in-memory volumes, so this is generous.
    tokio::time::sleep(Duration::from_millis(150)).await;
}

// ============================================================================
// Conflict-dispatch mutex
// ============================================================================

/// Concurrent merge with two deep clashes (across two top-level sources, taking
/// the concurrent path) under Stop: the dispatch mutex
/// serializes the prompts so each one gets answered through the single oneshot
/// slot. Both clashes resolve, both dest-only files survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_merge_with_two_deep_clashes_serializes_prompts() {
    let engine = TestOperations::new();
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

    let state = make_state(&engine);
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
    let engine = TestOperations::new();
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

    let state = make_state(&engine);
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
    let engine = TestOperations::new();
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

    let state = make_state(&engine);
    let op = engine.register_state("cancel-while-queued", Arc::clone(&state));
    let op_id = op.id();

    // Wait until the FIRST prompt is installed (proving task A is awaiting and
    // task B is queued on the mutex), then cancel via the public path. Cancelling
    // drops the oneshot sender (unblocking A) and flips intent (so B's in-resolver
    // cancel-check bails). Neither re-emits.
    let state_for_cancel = Arc::clone(&state);
    let op_id_for_cancel = op_id.clone();
    let canceller = tokio::spawn(async move {
        // A sender is installed while task A awaits the prompt (task B is then
        // queued behind it on the dispatch mutex).
        cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the Stop prompt to be installed", || {
            state_for_cancel.conflict_slot.is_awaiting()
        })
        .await;
        // Task B parking on the dispatch mutex is not observable from here, and this test's whole
        // point is the cancel landing WHILE B is queued, so we give B a beat to reach the mutex.
        // allowed-test-sleep: no signal for "B is queued on the dispatch mutex"; the window creates the race.
        tokio::time::sleep(Duration::from_millis(20)).await;
        engine.cancel_write_operation(&op_id_for_cancel, false);
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
            op_id.as_str(),
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

/// The driver queued behind a DEEP prompt, and the person answers it.
///
/// Regression anchor for ERR-8MEX6 (2026-10): a folder merge asked about a
/// child, the driver moved on to a top-level clash and waited for the dispatch
/// mutex the merge held, and the person answered "Skip all". The answer landed,
/// but the merge only ever moved when the driver polled its window, and the
/// driver was waiting on the merge: no I/O for 21 minutes, and Cancel couldn't
/// end it either. The prompts in the older tests here are answered inside
/// `emit_conflict`, so a merge never parked holding the mutex and the
/// interleaving never formed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn answering_a_deep_prompt_frees_a_driver_queued_behind_it() {
    let engine = TestOperations::new();
    let state = make_state(&engine);
    let (source, dest) = deep_prompt_then_top_level_clash(&state).await;
    let events = Arc::new(CollectorEventSink::new());

    let responder = tokio::spawn({
        let events = Arc::clone(&events);
        let state = Arc::clone(&state);
        async move {
            let clash = await_prompted_clash(&events).await;
            until_the_driver_queues_behind_the_deep_prompt(&events).await;
            let _ = state.conflict_slot.answer(
                clash,
                ConflictResolutionResponse {
                    resolution: ConflictResolution::Skip,
                    apply_to_all: true,
                },
            );
        }
    });

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let driven = tokio::time::timeout(
        Duration::from_secs(5),
        copy_volumes_with_progress(
            events.clone(),
            "op-deep-prompt-then-top-level",
            &state,
            Arc::clone(&source),
            &deep_prompt_sources(),
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        ),
    )
    .await;
    responder.await.unwrap();

    let result = driven.expect("the copy hung after the deep prompt was answered (dispatch-mutex self-deadlock)");
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // "Skip all" answered the deep clash and, through the latch, the queued
    // top-level one: one prompt, both destinations untouched.
    assert_eq!(file_conflict_count(&events), 1);
    assert_eq!(read_all(&dest, "/album/clash.txt").await, b"DEST-deep");
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"KEEP");
    assert_eq!(read_all(&dest, "/top.txt").await, b"DEST-top");
    assert_eq!(read_all(&dest, "/sib.bin").await, b"SIB");
}

/// The same queue, but the person presses Cancel instead of answering: the
/// operation must settle promptly rather than wait on a merge nobody polls.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_frees_a_driver_queued_behind_a_deep_prompt() {
    let engine = TestOperations::new();
    let state = make_state(&engine);
    let (source, dest) = deep_prompt_then_top_level_clash(&state).await;
    let events = Arc::new(CollectorEventSink::new());
    let op = engine.register_state("cancel-behind-deep-prompt", Arc::clone(&state));
    let op_id = op.id().to_string();

    let canceller = tokio::spawn({
        let events = Arc::clone(&events);
        let op_id = OperationId::from(op_id.clone());
        async move {
            await_prompted_clash(&events).await;
            until_the_driver_queues_behind_the_deep_prompt(&events).await;
            engine.cancel_write_operation(&op_id, false);
        }
    });

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let driven = tokio::time::timeout(
        Duration::from_secs(5),
        copy_volumes_with_progress(
            events.clone(),
            &op_id,
            &state,
            Arc::clone(&source),
            &deep_prompt_sources(),
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        ),
    )
    .await;
    canceller.await.unwrap();

    let result = driven.expect("the copy hung after Cancel (dispatch-mutex self-deadlock)");
    assert!(result.is_err(), "a cancelled copy returns Err, got {result:?}");
    // Nothing the person was asked about changed.
    assert_eq!(read_all(&dest, "/album/clash.txt").await, b"DEST-deep");
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"KEEP");
    assert_eq!(read_all(&dest, "/top.txt").await, b"DEST-top");
}

/// The same queue, with a task FAILING while the driver waits in it.
///
/// A failure ends the batch, but the merge holding the mutex still has to be
/// polled to read the person's answer and let go: a driver that stopped
/// draining at the first failure was back to waiting on itself (ERR-8MEX6's
/// shape, one step later). `/fail.bin` fails only once `/sib.bin` has landed,
/// which is the moment the driver moves on to `/top.txt` and queues.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_task_failing_while_the_driver_is_queued_behind_a_deep_prompt_still_frees_it() {
    let engine = TestOperations::new();
    let state = make_state(&engine);
    let events = Arc::new(CollectorEventSink::new());
    let (inner, dest) = deep_prompt_fixture().await;
    inner.create_file(Path::new("/fail.bin"), b"FAIL").await.unwrap();
    let prompted = Arc::clone(&state);
    let sib_landed = Arc::clone(&events);
    let source: Arc<dyn Volume> = Arc::new(HeldReadSource {
        inner,
        // `/album`, `/fail.bin`, and `/sib.bin` fill it; `/top.txt` waits.
        width: 3,
        held: vec![
            HeldRead {
                path: PathBuf::from("/sib.bin"),
                release: Arc::new(move || prompted.conflict_slot.is_awaiting()),
                then_fail: false,
            },
            HeldRead {
                path: PathBuf::from("/fail.bin"),
                release: Arc::new(move || {
                    sib_landed
                        .source_items_done
                        .lock_ignore_poison()
                        .iter()
                        .any(|done| done.source_path == "/sib.bin")
                }),
                then_fail: true,
            },
        ],
    });

    let responder = tokio::spawn({
        let events = Arc::clone(&events);
        let state = Arc::clone(&state);
        async move {
            let clash = await_prompted_clash(&events).await;
            until_the_driver_queues_behind_the_deep_prompt(&events).await;
            let _ = state.conflict_slot.answer(
                clash,
                ConflictResolutionResponse {
                    resolution: ConflictResolution::Skip,
                    apply_to_all: true,
                },
            );
        }
    });

    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let mut sources = deep_prompt_sources();
    sources.push(PathBuf::from("/fail.bin"));
    let driven = tokio::time::timeout(
        Duration::from_secs(5),
        copy_volumes_with_progress(
            events.clone(),
            "op-failure-behind-deep-prompt",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        ),
    )
    .await;
    responder.await.unwrap();

    let result = driven.expect("the copy hung after a task failed behind a deep prompt (dispatch-mutex self-deadlock)");
    assert!(
        result.is_err(),
        "the failed source ends the copy with an error, got {result:?}"
    );
    assert_eq!(read_all(&dest, "/album/clash.txt").await, b"DEST-deep");
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"KEEP");
    assert_eq!(read_all(&dest, "/top.txt").await, b"DEST-top");
}
