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

// ========================================================================
// Release-on-pause (MTP "navigate while paused"): a paused MTP→local copy
// must CLOSE the source stream (freeing the device session), then on resume
// REOPEN at the kept offset and append the rest, reconstructing the file
// exactly. These mirror the production wiring with a fake source that records
// open/close + supports an offset open, so no real device is needed.
// ========================================================================

use std::sync::Mutex as StdMutex;

const REL_TOTAL: usize = 200 * 1024; // 200 KiB, well over one chunk
const REL_CHUNK: usize = 16 * 1024;
const REL_CHUNK_DELAY: Duration = Duration::from_millis(4);

/// Records what a `ReleasingSource` did, so a test can assert the stream was
/// released on pause and reopened at the right offset.
#[derive(Default)]
struct RelLog {
    /// Offsets at which a stream was opened (0 for the initial open, then the
    /// kept offset for each resume).
    opens: Vec<u64>,
    /// Number of times `cancel_and_release` ran (one per pause that released).
    releases: usize,
}

/// A stream over the synthetic `[offset, REL_TOTAL)` byte range. The byte at
/// absolute position `p` is `(p % 256) as u8`, so the assembled destination can
/// be checked against that pattern regardless of where reopens happened.
struct ReleasingStream {
    log: Arc<StdMutex<RelLog>>,
    pos: u64, // absolute position of the next byte to emit
    emitted_here: u64,
    released: bool,
}

impl VolumeReadStream for ReleasingStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.pos >= REL_TOTAL as u64 {
                return None;
            }
            tokio::time::sleep(REL_CHUNK_DELAY).await;
            let start = self.pos;
            let end = (start + REL_CHUNK as u64).min(REL_TOTAL as u64);
            let chunk: Vec<u8> = (start..end).map(|p| (p % 256) as u8).collect();
            self.pos = end;
            self.emitted_here += chunk.len() as u64;
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        REL_TOTAL as u64
    }

    fn bytes_read(&self) -> u64 {
        self.emitted_here
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if !self.released {
                self.released = true;
                self.log.lock().unwrap().releases += 1;
            }
        })
    }
}

/// A source volume that opts into release-on-pause and serves an offset-aware
/// stream — the test-double of `MtpVolume`.
struct ReleasingSource {
    log: Arc<StdMutex<RelLog>>,
}

impl Volume for ReleasingSource {
    fn name(&self) -> &str {
        "releasing-source"
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
    fn pause_releases_read_stream(&self) -> bool {
        true
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }
    fn open_read_stream_at_offset<'a>(
        &'a self,
        _path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let log = Arc::clone(&self.log);
        Box::pin(async move {
            log.lock().unwrap().opens.push(offset);
            Ok(Box::new(ReleasingStream {
                log: Arc::clone(&log),
                pos: offset,
                emitted_here: 0,
                released: false,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

/// The reference bytes the destination must end up holding.
fn rel_expected_bytes() -> Vec<u8> {
    (0..REL_TOTAL as u64).map(|p| (p % 256) as u8).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_on_pause_copy_releases_source_then_resumes_from_offset() {
    use std::fs;

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource { log: Arc::clone(&log) });

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
            &|| {},
            None,
        )
        .await
    });

    // Let a few chunks stream, then pause MID-FILE.
    tokio::time::sleep(Duration::from_millis(30)).await;
    state.pause_gate.pause();

    // The byte count must freeze AND the source stream must be RELEASED (the
    // whole point: the device session is freed while paused).
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frozen = bytes_seen.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        bytes_seen.load(Ordering::SeqCst),
        frozen,
        "a paused copy must stop advancing mid-file"
    );
    assert!(
        frozen > 0 && (frozen as usize) < REL_TOTAL,
        "must be parked short of completion"
    );
    assert!(!op.is_finished(), "the copy task must still be parked while paused");
    assert_eq!(
        log.lock().unwrap().releases,
        1,
        "pause must have RELEASED the source stream exactly once (freeing the session)"
    );

    // Resume → reopens at the kept offset and completes.
    state.pause_gate.resume();
    let bytes = tokio::time::timeout(Duration::from_secs(10), op)
        .await
        .expect("resumed copy must complete")
        .expect("copy task must not panic")
        .expect("resumed copy must succeed");

    assert_eq!(bytes, REL_TOTAL as u64, "resumed copy reports the full byte count");

    // The destination must hold EXACTLY the full file (prefix + resumed tail),
    // byte-for-byte equal to a non-paused copy.
    let written = fs::read(dst_dir.join("movie.bin")).unwrap();
    assert_eq!(
        written,
        rel_expected_bytes(),
        "assembled bytes must equal the source exactly"
    );

    // The reopen happened at the kept offset (the frozen byte count), with no
    // gap or overlap — proven by the byte-exact assembly above plus the open log.
    let opens = log.lock().unwrap().opens.clone();
    assert_eq!(opens.len(), 2, "one initial open + one resume reopen");
    assert_eq!(opens[0], 0, "initial open is at offset 0");
    assert_eq!(
        opens[1], frozen,
        "resume must reopen at exactly the kept offset (= destination temp length)"
    );

    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn release_on_pause_cancel_while_paused_keeps_no_partial() {
    use std::fs;

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource { log: Arc::clone(&log) });

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
            &|| {},
            None,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    state.pause_gate.pause();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(!op.is_finished(), "parked while paused");
    assert_eq!(log.lock().unwrap().releases, 1, "pause released the source stream");

    // Cancel (keep partials) while paused. The parked stream unblocks, reopens,
    // and the next chunk flows through to on_progress, which breaks on cancel and
    // the local sink removes its in-flight temp.
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
async fn release_on_pause_unpaused_copy_never_releases() {
    use std::fs;

    // Sanity: with no pause, a release-on-pause source streams straight through
    // with a single open and no release — the resume machinery stays dormant.
    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource { log: Arc::clone(&log) });

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
        &|| {},
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

// ========================================================================
// Foreground auto-yield (M1, "navigate during transfers"): a RUNNING (not
// paused) MTP→local copy must, when foreground work pends on the source
// device, RELEASE the session, wait for foreground to drain plus a debounce
// window, then REOPEN at the kept offset and resume — byte-exact, op stays
// Running. A min-progress floor keeps continuous foreground nav from starving
// the copy to zero throughput, and a debounce collapses a burst of listings
// into one release/reopen.
//
// These mirror the production wiring with a fake source that (a) opts into
// release-on-pause offset reopen and (b) carries a controllable
// `foreground_pending` signal — the test-double of `MtpVolume` + its device
// priority gate. No real device or USB latency needed.
//
// `auto_yield_tuning_override` injects a near-zero debounce and a tiny floor so
// the arm fires deterministically over the small synthetic file these tests
// copy. Production uses the named constants (~400 ms, 4 MiB).
// ========================================================================

use std::sync::atomic::AtomicBool;

thread_local! {
    /// Per-test override of `(debounce, min_progress_floor)`. `None` ⇒ production
    /// constants. Set via [`AutoYieldTuningGuard`] and cleared on drop.
    static AUTO_YIELD_TUNING: std::cell::Cell<Option<(Duration, u64)>> = const { std::cell::Cell::new(None) };
}

/// Read by `super::auto_yield_tuning()` in test builds; production returns `None`.
pub(super) fn auto_yield_tuning_override() -> Option<(Duration, u64)> {
    AUTO_YIELD_TUNING.with(|c| c.get())
}

/// RAII guard that installs an auto-yield tuning override for the current thread
/// and restores the previous value on drop. The copy runs on a tokio task; these
/// tests use a CURRENT-THREAD runtime so the spawned copy shares this thread's
/// thread-local (a multi-thread runtime would not see it).
struct AutoYieldTuningGuard {
    prev: Option<(Duration, u64)>,
}

impl AutoYieldTuningGuard {
    fn new(debounce: Duration, floor: u64) -> Self {
        let prev = AUTO_YIELD_TUNING.with(|c| c.replace(Some((debounce, floor))));
        Self { prev }
    }
}

impl Drop for AutoYieldTuningGuard {
    fn drop(&mut self) {
        AUTO_YIELD_TUNING.with(|c| c.set(self.prev));
    }
}

/// A source volume that opts into release-on-pause AND foreground auto-yield,
/// serving an offset-aware stream — the test-double of `MtpVolume`. The
/// `foreground` flag is the controllable equivalent of the device priority
/// gate's `foreground_pending`.
struct YieldingSource {
    log: Arc<StdMutex<RelLog>>,
    /// When `true`, `foreground_pending()` reports a foreground op is waiting.
    foreground: Arc<AtomicBool>,
}

impl Volume for YieldingSource {
    fn name(&self) -> &str {
        "yielding-source"
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
    fn pause_releases_read_stream(&self) -> bool {
        true
    }
    fn supports_foreground_yield(&self) -> bool {
        true
    }
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let flag = Arc::clone(&self.foreground);
        Box::pin(async move { flag.load(Ordering::SeqCst) })
    }
    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        // The double's foreground signal is owned by the test, which clears it to
        // simulate the foreground op draining. Poll it the way the real per-device
        // gate parks until `foreground_pending == 0`.
        let flag = Arc::clone(&self.foreground);
        Box::pin(async move {
            while flag.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }
    fn open_read_stream_at_offset<'a>(
        &'a self,
        _path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let log = Arc::clone(&self.log);
        Box::pin(async move {
            log.lock().unwrap().opens.push(offset);
            Ok(Box::new(ReleasingStream {
                log: Arc::clone(&log),
                pos: offset,
                emitted_here: 0,
                released: false,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn auto_yield_releases_and_reopens_byte_exact() {
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
                    &|| {},
                    None,
                )
                .await
            });

            // Let a few chunks stream past the floor, then raise foreground.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(true, Ordering::SeqCst);

            // The copy must RELEASE the source (freeing the session) and NOT advance
            // while foreground is held.
            tokio::time::sleep(Duration::from_millis(40)).await;
            let frozen = bytes_seen.load(Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(40)).await;
            assert_eq!(
                bytes_seen.load(Ordering::SeqCst),
                frozen,
                "a copy yielding to foreground must stop advancing while foreground is held"
            );
            assert!(
                frozen > 0 && (frozen as usize) < REL_TOTAL,
                "must have yielded mid-file, short of completion"
            );
            assert_eq!(
                log.lock().unwrap().releases,
                1,
                "foreground-pending must RELEASE the source stream exactly once"
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

            // Drop foreground: the copy reopens at the kept offset and completes.
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
            assert_eq!(opens.len(), 2, "one initial open + one resume reopen");
            assert_eq!(opens[0], 0, "initial open at offset 0");
            assert_eq!(
                opens[1], frozen,
                "reopen at exactly the kept offset (= dest temp length)"
            );
        })
        .await;

    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "current_thread")]
async fn auto_yield_debounces_a_burst_into_one_release() {
    use std::fs;

    // A real debounce window so a brief gap between two listings is collapsed
    // into ONE suspension, not two. Floor = one chunk so the arm can fire early.
    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(60), REL_CHUNK as u64);

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
                    &|| {},
                    None,
                )
                .await
            });

            // First listing: raise, then drop. The copy releases and parks in the
            // debounce window.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            foreground.store(false, Ordering::SeqCst);
            // Second listing arrives BEFORE the 60 ms quiet window elapses — the copy
            // must stay parked (re-drain), not reopen-then-release again.
            tokio::time::sleep(Duration::from_millis(20)).await;
            foreground.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            foreground.store(false, Ordering::SeqCst);

            let bytes = tokio::time::timeout(Duration::from_secs(10), op)
                .await
                .expect("copy must finish")
                .expect("copy task must not panic")
                .expect("copy must succeed");

            assert_eq!(bytes, REL_TOTAL as u64);
            assert_eq!(fs::read(dst_dir.join("movie.bin")).unwrap(), rel_expected_bytes());
            assert_eq!(
                log.lock().unwrap().releases,
                1,
                "a burst of foreground ops within the debounce window must collapse into ONE release/reopen"
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
                    &|| {},
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
                tokio::time::sleep(Duration::from_millis(20)).await;
                foreground.store(true, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
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

    let op_id = format!("test-autoyield-cancel-{:?}", std::thread::current().id());
    let state = make_state();
    WRITE_OPERATION_STATE
        .write()
        .unwrap()
        .insert(op_id.clone(), Arc::clone(&state));

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let source_drv = Arc::clone(&source);
            let dest_drv = Arc::clone(&dest);
            let state_drv = Arc::clone(&state);
            let op = tokio::task::spawn_local(async move {
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
                    &|_bytes_done, _total| {
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

            // Let it stream past the floor, then hold foreground so it yields and parks
            // in the (long) debounce window.
            tokio::time::sleep(Duration::from_millis(40)).await;
            foreground.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(40)).await;
            assert_eq!(log.lock().unwrap().releases, 1, "yielded and parked");
            assert!(!op.is_finished(), "parked in the debounce window");

            // Cancel (keep partials) WHILE yielding. The cancel-aware debounce must bail
            // promptly; ensure_open reopens and the next chunk flows to on_progress,
            // which breaks on cancel and the local sink removes its in-flight temp.
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

    WRITE_OPERATION_STATE.write().unwrap().remove(&op_id);
    let _ = fs::remove_dir_all(&dst_dir);
}

#[tokio::test(flavor = "current_thread")]
async fn non_mtp_source_never_auto_yields_for_foreground() {
    // Regression guard: a source that does NOT support foreground yield
    // (`supports_foreground_yield() == false`, the default for local/SMB/
    // in-memory) must never release for foreground, even with a tiny floor and a
    // foreground signal that would trigger an MTP source. `ReleasingSource` opts
    // into release-on-pause but NOT foreground-yield, so it's the right double.
    use std::fs;

    let _tuning = AutoYieldTuningGuard::new(Duration::from_millis(0), REL_CHUNK as u64);

    let log = Arc::new(StdMutex::new(RelLog::default()));
    let source: Arc<dyn Volume> = Arc::new(ReleasingSource { log: Arc::clone(&log) });
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
        &|| {},
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
