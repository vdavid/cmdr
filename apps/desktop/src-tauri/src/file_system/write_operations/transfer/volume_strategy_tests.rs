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
