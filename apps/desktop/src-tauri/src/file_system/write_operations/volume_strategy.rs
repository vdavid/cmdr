//! Copy strategy routing for volume-to-volume operations.
//!
//! Since Phase 4, every cross-volume copy either (a) uses the APFS clonefile
//! fast path when both sides are `LocalPosixVolume` on the same APFS volume, or
//! (b) pipes bytes through `open_read_stream` + `write_from_stream`. The old
//! `export_to_local` / `import_from_local` short-circuits are gone.
//!
//! Directories are walked here (recursively) so the user can cancel between
//! files. Per-file transfers use the destination's `write_from_stream`.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::state::WriteOperationState;
use crate::file_system::volume::{Volume, VolumeError};

/// Copies a single path from source volume to destination volume.
///
/// Dispatches on two cases:
/// - Both volumes are `LocalPosixVolume` and the source/destination are on the
///   same APFS volume → delegate to the native `copy_files_start` path
///   upstream (handled in `copy_between_volumes`; this function isn't called
///   for that case).
/// - Otherwise → generic streaming pipe via `open_read_stream` +
///   `write_from_stream`, walking directories recursively so the user can
///   cancel between files.
#[allow(
    clippy::too_many_arguments,
    reason = "Cross-volume copy needs source/dest volumes, paths, the source type hint, shared state, and two progress callbacks. Bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: bool,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn() + Sync),
) -> Result<u64, VolumeError> {
    // Check cancellation up front.
    if super::state::is_cancelled(&state.intent) {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
    }

    if source_is_directory {
        Box::pin(copy_directory_streaming(
            source_volume,
            source_path,
            dest_volume,
            dest_path,
            state,
            on_file_progress,
            on_file_complete,
        ))
        .await
    } else {
        let bytes = stream_pipe_file(source_volume, source_path, dest_volume, dest_path, on_file_progress).await?;
        on_file_complete();
        Ok(bytes)
    }
}

/// Streams one file from source to destination via `open_read_stream` /
/// `write_from_stream`. Per-chunk progress and cancellation are enforced by
/// the destination's `write_from_stream` implementation — it calls
/// `on_progress` between chunks and returns `VolumeError::Cancelled` on
/// `ControlFlow::Break(())`.
async fn stream_pipe_file(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
) -> Result<u64, VolumeError> {
    log::debug!("stream_pipe_file: {} -> {}", source_path.display(), dest_path.display());

    let stream = source_volume.open_read_stream(source_path).await?;
    let size = stream.total_size();
    dest_volume
        .write_from_stream(dest_path, size, stream, on_file_progress)
        .await
}

/// Recursively copies a directory tree from source to destination, streaming
/// each file through `write_from_stream`. Checks cancellation between entries.
async fn copy_directory_streaming(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn() + Sync),
) -> Result<u64, VolumeError> {
    // Ensure the destination directory exists. The create_directory call is
    // idempotent for in-memory/SMB/MTP (they treat "already exists" as a
    // merge), but we catch `AlreadyExists` just in case for LocalPosix.
    match dest_volume.create_directory(dest_path).await {
        Ok(()) => {}
        Err(VolumeError::AlreadyExists(_)) => {}
        Err(e) => {
            // LocalPosix uses std::fs::create_dir which fails if the parent
            // doesn't exist; retry with create_dir_all semantics via the
            // default trait — but most backends implement this fine. Treat
            // NotSupported as a signal to skip the explicit mkdir and hope
            // write_from_stream creates parents as needed.
            if !matches!(e, VolumeError::NotSupported) {
                return Err(e);
            }
        }
    }

    let entries = source_volume.list_directory(source_path, None).await?;
    let mut total_bytes = 0u64;

    for entry in &entries {
        if super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
        }

        let child_source = PathBuf::from(&entry.path);
        let child_dest = dest_path.join(&entry.name);

        if entry.is_directory {
            total_bytes += Box::pin(copy_directory_streaming(
                source_volume,
                &child_source,
                dest_volume,
                &child_dest,
                state,
                on_file_progress,
                on_file_complete,
            ))
            .await?;
        } else {
            let bytes =
                stream_pipe_file(source_volume, &child_source, dest_volume, &child_dest, on_file_progress).await?;
            total_bytes += bytes;
            on_file_complete();
        }
    }

    Ok(total_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU8;
    use std::time::Duration;

    use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};

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

        let state = Arc::new(WriteOperationState {
            intent: Arc::new(AtomicU8::new(0)),
            progress_interval: Duration::from_millis(200),
            conflict_resolution_tx: std::sync::Mutex::new(None),
        });

        let bytes = copy_single_path(
            &source,
            Path::new("source.txt"),
            false,
            &dest,
            Path::new("dest.txt"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {},
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

        let state = Arc::new(WriteOperationState {
            intent: Arc::new(AtomicU8::new(2)), // Already cancelled (Stopped)
            progress_interval: Duration::from_millis(200),
            conflict_resolution_tx: std::sync::Mutex::new(None),
        });

        let result = copy_single_path(
            &source,
            Path::new("source.txt"),
            false,
            &dest,
            Path::new("dest.txt"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {},
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
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    fn make_state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState {
            intent: Arc::new(AtomicU8::new(0)),
            progress_interval: Duration::from_millis(200),
            conflict_resolution_tx: std::sync::Mutex::new(None),
        })
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
            &dest,
            Path::new("/photo.jpg"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {},
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
            &dest,
            Path::new("/big.bin"),
            &state,
            &|bytes_done, total| {
                progress_calls.fetch_add(1, Ordering::Relaxed);
                total_bytes_reported.store(bytes_done, Ordering::Relaxed);
                assert_eq!(total, 200_000);
                ControlFlow::Continue(())
            },
            &|| {
                file_complete_calls.fetch_add(1, Ordering::Relaxed);
            },
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
            &dest,
            Path::new("/big.bin"),
            &state,
            &|_, _| {
                let n = call_count.fetch_add(1, Ordering::Relaxed);
                if n >= 1 {
                    ControlFlow::Break(()) // Cancel after second chunk
                } else {
                    ControlFlow::Continue(())
                }
            },
            &|| {},
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
            &dest,
            Path::new("/empty.txt"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {},
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
            &dest,
            Path::new("/nope.txt"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {},
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
            &dest,
            Path::new("/test.txt"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {
                file_complete.fetch_add(1, Ordering::Relaxed);
            },
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
            &dest,
            Path::new("/docs"),
            &state,
            &|_, _| ControlFlow::Continue(()),
            &|| {
                file_complete.fetch_add(1, Ordering::Relaxed);
            },
        )
        .await
        .unwrap();

        assert_eq!(bytes, 17); // 7 + 10
        assert_eq!(file_complete.load(Ordering::Relaxed), 2);

        assert!(dest.exists(Path::new("/docs/readme.txt")).await);
        assert!(dest.exists(Path::new("/docs/notes.txt")).await);
    }
}
