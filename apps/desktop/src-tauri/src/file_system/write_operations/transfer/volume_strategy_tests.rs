use super::super::super::state::OperationIntent;
use super::*;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{ListingProgress, LocalPosixVolume, Volume, VolumeError, VolumeReadStream};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_copy_single_path_local_to_local() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_copy_single_src");
    let dst_dir = std::env::temp_dir().join("cmdr_copy_single_dst");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    fs::write(src_dir.join("source.txt"), "Source content").unwrap();

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    let bytes = copy_single_path(
        &source,
        Path::new("source.txt"),
        false,
        None,
        &dest,
        Path::new("dest.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {},
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 14); // "Source content"
    assert_eq!(fs::read_to_string(dst_dir.join("dest.txt")).unwrap(), "Source content");

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_copy_single_path_cancelled() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_copy_cancel_src");
    let dst_dir = std::env::temp_dir().join("cmdr_copy_cancel_dst");
    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    fs::write(src_dir.join("source.txt"), "Content").unwrap();

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);

    let result = copy_single_path(
        &source,
        Path::new("source.txt"),
        false,
        None,
        &dest,
        Path::new("dest.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {},
        None,
    )
    .await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), VolumeError::Cancelled(_)));

    let _ = fs::remove_dir_all(&src_dir);
    let _ = fs::remove_dir_all(&dst_dir);
}

// ========================================================================
// Cross-volume streaming copy tests (InMemoryVolume pairs)
// ========================================================================

use crate::file_system::volume::InMemoryVolume;
use std::sync::atomic::{AtomicU64, AtomicUsize};

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(200)))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_single_file() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source
        .create_file(Path::new("/photo.jpg"), b"JPEG data here")
        .await
        .unwrap();

    let state = make_state();
    let bytes = copy_single_path(
        &source,
        Path::new("/photo.jpg"),
        false,
        None,
        &dest,
        Path::new("/photo.jpg"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {},
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 14);
    // Verify content
    let mut stream = dest.open_read_stream(Path::new("/photo.jpg")).await.unwrap();
    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"JPEG data here");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_large_file_with_progress() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    let data: Vec<u8> = (0..=255).cycle().take(200_000).collect();
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let state = make_state();
    let progress_calls = Arc::new(AtomicUsize::new(0));
    let total_bytes_reported = Arc::new(AtomicU64::new(0));
    let file_complete_calls = Arc::new(AtomicUsize::new(0));

    let bytes = copy_single_path(
        &source,
        Path::new("/big.bin"),
        false,
        None,
        &dest,
        Path::new("/big.bin"),
        &state,
        &CreatedPaths::default(),
        &|bytes_done, total| {
            progress_calls.fetch_add(1, Ordering::Relaxed);
            total_bytes_reported.store(bytes_done, Ordering::Relaxed);
            assert_eq!(total, 200_000);
            ControlFlow::Continue(())
        },
        &|| {
            file_complete_calls.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 200_000);
    assert!(
        progress_calls.load(Ordering::Relaxed) >= 2,
        "expected progress calls for multi-chunk file"
    );
    assert_eq!(total_bytes_reported.load(Ordering::Relaxed), 200_000);
    assert_eq!(file_complete_calls.load(Ordering::Relaxed), 1);

    // Verify content integrity
    let mut stream = dest.open_read_stream(Path::new("/big.bin")).await.unwrap();
    let mut reassembled = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        reassembled.extend_from_slice(&chunk);
    }
    assert_eq!(reassembled, data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_cancel_mid_file() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    let data = vec![0xAB; 200_000];
    source.create_file(Path::new("/big.bin"), &data).await.unwrap();

    let state = make_state();
    let call_count = Arc::new(AtomicUsize::new(0));

    let result = copy_single_path(
        &source,
        Path::new("/big.bin"),
        false,
        None,
        &dest,
        Path::new("/big.bin"),
        &state,
        &CreatedPaths::default(),
        &|_, _| {
            let n = call_count.fetch_add(1, Ordering::Relaxed);
            if n >= 1 {
                ControlFlow::Break(()) // Cancel after second chunk
            } else {
                ControlFlow::Continue(())
            }
        },
        &|| {},
        None,
    )
    .await;

    assert!(result.is_err());
    // File should not exist at dest (cancelled before completion)
    assert!(!dest.exists(Path::new("/big.bin")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_empty_file() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source.create_file(Path::new("/empty.txt"), b"").await.unwrap();

    let state = make_state();
    let bytes = copy_single_path(
        &source,
        Path::new("/empty.txt"),
        false,
        None,
        &dest,
        Path::new("/empty.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {},
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 0);
    assert!(dest.exists(Path::new("/empty.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_nonexistent_source_fails() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    let state = make_state();
    let result = copy_single_path(
        &source,
        Path::new("/nope.txt"),
        false,
        None,
        &dest,
        Path::new("/nope.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {},
        None,
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_uses_streaming_for_non_local_volumes() {
    // InMemoryVolume has local_path() = None and supports_streaming() = true.
    // Verify that copy_single_path routes through the streaming path.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    // Verify the routing assumptions
    assert!(source.local_path().is_none(), "InMemoryVolume should not be local");
    assert!(source.supports_streaming(), "InMemoryVolume should support streaming");
    assert!(dest.supports_streaming(), "InMemoryVolume should support streaming");

    source
        .create_file(Path::new("/test.txt"), b"routed correctly")
        .await
        .unwrap();

    let state = make_state();
    let file_complete = Arc::new(AtomicUsize::new(0));
    let bytes = copy_single_path(
        &source,
        Path::new("/test.txt"),
        false,
        None,
        &dest,
        Path::new("/test.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {
            file_complete.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 16);
    assert_eq!(file_complete.load(Ordering::Relaxed), 1, "on_file_complete should fire");

    let mut stream = dest.open_read_stream(Path::new("/test.txt")).await.unwrap();
    let chunk = stream.next_chunk().await.unwrap().unwrap();
    assert_eq!(chunk, b"routed correctly");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_streaming_copy_directory_recursive() {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source"));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));

    source.create_directory(Path::new("/docs")).await.unwrap();
    source
        .create_file(Path::new("/docs/readme.txt"), b"Read me")
        .await
        .unwrap();
    source
        .create_file(Path::new("/docs/notes.txt"), b"Notes here")
        .await
        .unwrap();

    let state = make_state();
    let file_complete = Arc::new(AtomicUsize::new(0));
    let bytes = copy_single_path(
        &source,
        Path::new("/docs"),
        true,
        None,
        &dest,
        Path::new("/docs"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {
            file_complete.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(bytes, 17); // 7 + 10
    assert_eq!(file_complete.load(Ordering::Relaxed), 2);

    assert!(dest.exists(Path::new("/docs/readme.txt")).await);
    assert!(dest.exists(Path::new("/docs/notes.txt")).await);
}

// ========================================================================
// Mid-file (between-chunk) pause: a multi-chunk volume copy must STOP
// advancing while paused, then complete on resume / unblock on cancel.
//
// The cross-volume streaming path's per-chunk progress callback is sync, so
// it can't `.await` to park. `stream_pipe_file` wraps the source stream in a
// `CheckpointStream` whose `next_chunk()` parks while paused (and yields) once
// per chunk. These tests pin that the gate now reaches MID-FILE — pre-fix the
// gate lived only at the per-source loop top, so a single large file streamed
// to completion while "Paused" was showing.
//
// The source is a `SlowSource`: its read stream sleeps a few ms per chunk so
// the transfer spans a real wall-clock window. That makes "pause lands
// mid-file" deterministic — an instant in-memory copy would otherwise finish
// inside the controlling task's first sleep before any pause could land.
// ========================================================================

use super::super::super::state::{WRITE_OPERATION_STATE, cancel_write_operation, load_intent};

const SLOW_CHUNK_SIZE: usize = 64 * 1024;
const SLOW_CHUNK_COUNT: usize = 30;
/// Per-chunk delay so the whole transfer spans ~120 ms — wide enough that a
/// pause from the controlling task reliably lands between two chunks, short
/// enough to keep the test from lingering across other globally-stateful tests.
const SLOW_CHUNK_DELAY: Duration = Duration::from_millis(4);

/// A read stream that yields `SLOW_CHUNK_COUNT` chunks of `SLOW_CHUNK_SIZE`
/// bytes, sleeping `SLOW_CHUNK_DELAY` before each, so a multi-chunk copy spans a
/// real wall-clock window for pause/cancel to land mid-stream.
struct SlowChunkedStream {
    chunks_left: usize,
    fill: u8,
    total: u64,
    emitted: u64,
}

impl VolumeReadStream for SlowChunkedStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.chunks_left == 0 {
                return None;
            }
            tokio::time::sleep(SLOW_CHUNK_DELAY).await;
            self.chunks_left -= 1;
            self.emitted += SLOW_CHUNK_SIZE as u64;
            Some(Ok(vec![self.fill; SLOW_CHUNK_SIZE]))
        })
    }

    fn total_size(&self) -> u64 {
        self.total
    }

    fn bytes_read(&self) -> u64 {
        self.emitted
    }
}

/// Minimal source volume whose `open_read_stream` returns a `SlowChunkedStream`.
/// Non-local + streaming so `copy_single_path` routes through the streaming
/// pipe (and thus the `CheckpointStream` wrapper).
struct SlowSource;

impl Volume for SlowSource {
    fn name(&self) -> &str {
        "slow-source"
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }
    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async {
            Ok(Box::new(SlowChunkedStream {
                chunks_left: SLOW_CHUNK_COUNT,
                fill: 0xCD,
                total: (SLOW_CHUNK_COUNT * SLOW_CHUNK_SIZE) as u64,
                emitted: 0,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

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
            &|| {},
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
            // Mirror the production per-file callback (`make_serial_per_file_progress`):
            // break on cancel so the backend's chunk loop tears down the partial.
            &|bytes_done, _total| {
                bytes_ref.store(bytes_done, Ordering::SeqCst);
                if crate::file_system::write_operations::state::is_cancelled(&state_ref.intent) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
            &|| {},
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

/// Destination volume that rejects the first `write_from_stream` with
/// `StaleDestinationHandle` (a re-keyed MTP folder handle) and accepts the
/// second. Proves the transfer engine re-opens the source and retries once
/// rather than surfacing the stale-handle error to the user.
struct FailOnceStaleDest {
    calls: AtomicUsize,
}

impl Volume for FailOnceStaleDest {
    fn name(&self) -> &str {
        "fail-once-stale-dest"
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn write_from_stream<'a>(
        &'a self,
        _dest: &'a Path,
        size: u64,
        _stream: Box<dyn VolumeReadStream>,
        _on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if attempt == 0 {
                Err(VolumeError::StaleDestinationHandle("/Documents".to_string()))
            } else {
                Ok(size)
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_pipe_file_retries_once_on_stale_destination_handle() {
    use std::fs;

    let src_dir = std::env::temp_dir().join("cmdr_retry_stale_src");
    let _ = fs::remove_dir_all(&src_dir);
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("a.txt"), "payload-bytes").unwrap(); // 13 bytes

    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
    let dest: Arc<dyn Volume> = Arc::new(FailOnceStaleDest {
        calls: AtomicUsize::new(0),
    });

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    let bytes = copy_single_path(
        &source,
        Path::new("a.txt"),
        false,
        None,
        &dest,
        Path::new("a.txt"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|| {},
        None,
    )
    .await
    .expect("a stale destination handle must be retried, not surfaced as a copy failure");

    assert_eq!(bytes, 13, "the retried copy reports the full byte count");
    let dest = dest.as_any().downcast_ref::<FailOnceStaleDest>().unwrap();
    assert_eq!(
        dest.calls.load(Ordering::SeqCst),
        2,
        "write_from_stream must be called exactly twice: the stale-handle rejection, then the retry"
    );

    let _ = fs::remove_dir_all(&src_dir);
}
