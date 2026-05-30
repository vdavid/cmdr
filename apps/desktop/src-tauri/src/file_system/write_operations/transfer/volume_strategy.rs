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
use std::sync::Mutex;

use super::super::state::WriteOperationState;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// Records exactly what a single `copy_single_path` call wrote to the
/// destination, so rollback can remove only what this operation created — never
/// dest-only files that pre-existed a merged destination directory.
///
/// A directory source merges into an existing dest directory ("Overwrite means
/// merge for dirs"), so recording the top-level dest directory and recursively
/// deleting it on rollback would destroy the user's untouched files. Instead we
/// record:
/// - `files`: every destination FILE path the copy streamed, in write order.
///   Rollback deletes these individually.
/// - `dirs`: every destination DIRECTORY this copy newly created (i.e. the
///   `create_directory` call returned `Ok`, not `AlreadyExists`), in
///   creation order (shallowest first). Rollback removes these with a
///   non-recursive delete (empty-only on real backends), deepest first, so a
///   directory that still holds a pre-existing sibling survives.
#[derive(Default)]
pub(super) struct CreatedPaths {
    pub files: Mutex<Vec<PathBuf>>,
    pub dirs: Mutex<Vec<PathBuf>>,
}

impl CreatedPaths {
    fn record_file(&self, path: PathBuf) {
        self.files.lock_ignore_poison().push(path);
    }

    fn record_dir(&self, path: PathBuf) {
        self.dirs.lock_ignore_poison().push(path);
    }
}

/// Copies a single path from source volume to destination volume.
///
/// Dispatches on two cases:
/// - Both volumes are `LocalPosixVolume` and the source/destination are on the same APFS volume →
///   delegate to the native `copy_files_start` path upstream (handled in `copy_between_volumes`;
///   this function isn't called for that case).
/// - Otherwise → generic streaming pipe via `open_read_stream` + `write_from_stream`, walking
///   directories recursively so the user can cancel between files.
#[allow(
    clippy::too_many_arguments,
    reason = "Cross-volume copy needs source/dest volumes, paths, the source type hint, the size hint, shared state, the rollback ledger, and two progress callbacks. Bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: bool,
    source_size_hint: Option<u64>,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn() + Sync),
) -> Result<u64, VolumeError> {
    // Check cancellation up front.
    if super::super::state::is_cancelled(&state.intent) {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
    }

    if source_is_directory {
        Box::pin(copy_directory_streaming(
            source_volume,
            source_path,
            dest_volume,
            dest_path,
            state,
            created,
            on_file_progress,
            on_file_complete,
        ))
        .await
    } else {
        // A top-level FILE source records nothing into `created` here: the
        // caller owns that path's rollback bookkeeping because it may be a
        // safe-replace temp sibling (`write_path`) that gets renamed onto the
        // original after the write lands — the caller records the ORIGINAL, not
        // the temp. `created` is for the directory-merge case, where the
        // recursive copy below is the only place that knows which files and
        // newly-created subdirs landed inside a (possibly pre-existing) dest
        // directory.
        let bytes = stream_pipe_file(
            source_volume,
            source_path,
            source_size_hint,
            dest_volume,
            dest_path,
            on_file_progress,
        )
        .await?;
        on_file_complete();
        Ok(bytes)
    }
}

/// Streams one file from source to destination via `open_read_stream` /
/// `write_from_stream`. Per-chunk progress and cancellation are enforced by
/// the destination's `write_from_stream` implementation, which calls
/// `on_progress` between chunks and returns `VolumeError::Cancelled` on
/// `ControlFlow::Break(())`.
async fn stream_pipe_file(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_size_hint: Option<u64>,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
) -> Result<u64, VolumeError> {
    log::debug!("stream_pipe_file: {} -> {}", source_path.display(), dest_path.display());

    // Register the destination with the downloads watcher's ignore set
    // when the destination is local-FS-backed (the only case where the
    // watcher could otherwise fire). Covers MTP→Local and SMB→Local
    // imports that land in ~/Downloads.
    note_pending_for_local_dest(dest_volume, dest_path);

    let stream = source_volume
        .open_read_stream_with_hint(source_path, source_size_hint)
        .await?;
    let size = stream.total_size();
    dest_volume
        .write_from_stream(dest_path, size, stream, on_file_progress)
        .await
}

/// Resolve `dest_path` against `dest_volume.local_path()` and register it
/// with the downloads watcher's ignore set. Skips silently when
/// `dest_volume` isn't local-FS-backed (MTP, SMB, in-memory): those paths
/// would never trigger the watcher anyway, and synthesizing a non-local
/// path into the ignore set would just churn the map for no benefit.
fn note_pending_for_local_dest(dest_volume: &Arc<dyn Volume>, dest_path: &Path) {
    let Some(root) = dest_volume.local_path() else {
        return;
    };
    // Mirror `LocalPosixVolume::resolve`'s absolute-path handling so the
    // path we register matches the one `write_from_stream` will hit.
    let absolute = if dest_path.as_os_str().is_empty() || dest_path == Path::new(".") {
        root
    } else if dest_path.is_absolute() {
        if dest_path.starts_with(&root) || root == Path::new("/") {
            dest_path.to_path_buf()
        } else {
            root.join(dest_path.strip_prefix("/").unwrap_or(dest_path))
        }
    } else {
        root.join(dest_path)
    };
    crate::downloads::note_pending_write_for_cmdr(&absolute);
}

/// Recursively copies a directory tree from source to destination, streaming
/// each file through `write_from_stream`. Checks cancellation between entries.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors copy_single_path's argument list plus the rollback ledger; bundling into a struct adds ceremony without cleaning anything up."
)]
async fn copy_directory_streaming(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn() + Sync),
) -> Result<u64, VolumeError> {
    // Ensure the destination directory exists. Every backend is expected to
    // surface "already exists" as `VolumeError::AlreadyExists`; we swallow it
    // because that's the merge-into-existing-directory signal, not a failure.
    // (SMB needed smb2 ≥ 0.8.0 to typed-classify STATUS_OBJECT_NAME_COLLISION;
    // older versions leaked it as IoError and the merge path blew up.)
    //
    // `Ok(())` means WE created this directory, so rollback may remove it (once
    // empty). `AlreadyExists` means we merged into the user's pre-existing
    // directory, so rollback must NOT remove it — only the files we wrote into
    // it. This distinction is what keeps rollback from destroying dest-only
    // files that legitimately coexist in a merged directory.
    note_pending_for_local_dest(dest_volume, dest_path);
    match dest_volume.create_directory(dest_path).await {
        Ok(()) => created.record_dir(dest_path.to_path_buf()),
        Err(VolumeError::AlreadyExists(_)) => {}
        Err(VolumeError::NotSupported) => {
            // Backend can't create directories at all; assume `write_from_stream`
            // will materialize parents on demand (LocalPosix does this via the
            // default `create_dir_all` semantics).
        }
        Err(e) => return Err(e),
    }

    let entries = source_volume.list_directory(source_path, None).await?;
    let mut total_bytes = 0u64;

    for entry in &entries {
        if super::super::state::is_cancelled(&state.intent) {
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
                created,
                on_file_progress,
                on_file_complete,
            ))
            .await?;
        } else {
            let bytes = stream_pipe_file(
                source_volume,
                &child_source,
                entry.size,
                dest_volume,
                &child_dest,
                on_file_progress,
            )
            .await?;
            created.record_file(child_dest);
            total_bytes += bytes;
            on_file_complete();
        }
    }

    Ok(total_bytes)
}

#[cfg(test)]
mod tests {
    use super::super::super::state::OperationIntent;
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
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
        )
        .await
        .unwrap();

        assert_eq!(bytes, 17); // 7 + 10
        assert_eq!(file_complete.load(Ordering::Relaxed), 2);

        assert!(dest.exists(Path::new("/docs/readme.txt")).await);
        assert!(dest.exists(Path::new("/docs/notes.txt")).await);
    }
}
