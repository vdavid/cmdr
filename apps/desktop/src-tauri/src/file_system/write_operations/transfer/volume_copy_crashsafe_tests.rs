//! Crash-safety / safe-replace tests for `volume_copy`, split out of
//! `volume_copy_tests.rs` to keep each suite focused. These cover the
//! cross-volume file→file Overwrite safe-replace guarantee: the original
//! destination must survive a mid-stream read/write or finalize-rename
//! failure. The Volume test doubles below (`FailAfterOneChunkStream`,
//! `FailingReadSourceVolume`, `RenameFailsDestVolume`) model those failures.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume_copy_tests.rs`
//! (`super::tests`) so they aren't duplicated.

use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};

// ========================================================================
// Cross-volume file→file Overwrite safe-replace (data-loss regression)
// ========================================================================
//
// On a cross-volume file Overwrite, the original destination MUST survive a
// mid-stream read/write failure. The fix streams into a temp sibling and only
// swaps it over the original after the write fully lands. These tests pin both
// halves: data survives a failure, and a success replaces the content cleanly
// with no temp left behind.

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{CopyScanResult, ListingProgress, SpaceInfo, VolumeReadStream};
use std::pin::Pin as StdPin;

/// A `VolumeReadStream` that yields exactly one chunk, then fails. Models a
/// network drop / USB yank partway through reading the source file.
struct FailAfterOneChunkStream {
    total: u64,
    chunk: Option<Vec<u8>>,
}

impl VolumeReadStream for FailAfterOneChunkStream {
    fn next_chunk(&mut self) -> StdPin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if let Some(c) = self.chunk.take() {
                Some(Ok(c))
            } else {
                Some(Err(VolumeError::IoError {
                    message: "simulated mid-stream read failure".to_string(),
                    raw_os_error: None,
                }))
            }
        })
    }
    fn total_size(&self) -> u64 {
        self.total
    }
    fn bytes_read(&self) -> u64 {
        // Best-effort: 4 once the single chunk has been handed out, else 0.
        if self.chunk.is_some() { 0 } else { 4 }
    }
}

/// Wraps an `InMemoryVolume` source but returns a stream that fails partway
/// through. Everything else (listing, metadata, scan) delegates to the inner
/// volume so conflict detection and preflight behave normally.
struct FailingReadSourceVolume {
    inner: Arc<InMemoryVolume>,
    file_size: u64,
}

impl Volume for FailingReadSourceVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_export(&self) -> bool {
        true
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> StdPin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        // Delegate so the preflight scan succeeds and the copy reaches the
        // streaming read (where our failure is injected). Without this the
        // default `scan_for_copy` returns NotSupported and the copy bails
        // before conflict resolution — masking the bug under test.
        self.inner.scan_for_copy(path)
    }
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let total = self.file_size;
        Box::pin(async move {
            let stream: Box<dyn VolumeReadStream> = Box::new(FailAfterOneChunkStream {
                total,
                chunk: Some(vec![0xAB; 4]),
            });
            Ok(stream)
        })
    }
}

/// Data survives a mid-stream failure on a cross-volume file Overwrite.
///
/// The source read fails partway through; the original destination bytes MUST
/// be unchanged afterward. Pre-fix the resolver deleted the destination before
/// the streaming write, so this failure left the user with neither the old nor
/// a complete new file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_overwrite_preserves_dest_on_midstream_failure() {
    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    // 100 bytes: bigger than the one 4-byte chunk the stream yields, so the
    // dest write loop pulls a second chunk and hits the failure.
    source_inner
        .create_file(Path::new("/notes.txt"), &[0xAB; 100])
        .await
        .unwrap();
    let source: Arc<dyn Volume> = Arc::new(FailingReadSourceVolume {
        inner: Arc::clone(&source_inner),
        file_size: 100,
    });

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner
        .create_file(Path::new("/notes.txt"), b"ORIGINAL DEST DATA")
        .await
        .unwrap();
    let dest: Arc<dyn Volume> = Arc::clone(&dest_inner) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        // < 3 sources → serial path.
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-midstream-fail",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/notes.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "the mid-stream read failure must surface as an error");

    // The original destination data MUST be intact.
    let mut stream = dest_inner.open_read_stream(Path::new("/notes.txt")).await.unwrap();
    assert_eq!(
        stream.next_chunk().await.unwrap().unwrap(),
        b"ORIGINAL DEST DATA",
        "a mid-stream failure must not destroy the existing destination file"
    );

    // No temp sibling should be left behind in the dest root.
    let entries = dest_inner.list_directory(Path::new("/"), None).await.unwrap();
    assert!(
        !entries.iter().any(|e| e.name.contains(".cmdr-tmp-")),
        "partial cleanup must remove the temp sibling on failure: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

/// A successful cross-volume file Overwrite replaces the destination content
/// and leaves no temp sibling behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_overwrite_success_replaces_and_cleans_temp() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/file.txt"), b"NEW").await.unwrap();
    let source: Arc<dyn Volume> = source;

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_file(Path::new("/file.txt"), b"OLD").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::clone(&dest_inner) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-overwrite-success",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/file.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "overwrite copy should succeed: {:?}", result);

    let mut stream = dest_inner.open_read_stream(Path::new("/file.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"NEW");

    let entries = dest_inner.list_directory(Path::new("/"), None).await.unwrap();
    assert!(
        !entries.iter().any(|e| e.name.contains(".cmdr-tmp-")),
        "no temp sibling should remain after a successful overwrite: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    // Exactly the one final file.
    assert_eq!(entries.iter().filter(|e| e.name == "file.txt").count(), 1);
}

/// Concurrent path (≥3 sources, InMemory `max_concurrent_ops` = 32) exercises
/// the inline `FuturesUnordered` safe-replace finalize: a mix of fresh and
/// conflicting files all land correctly with no temp siblings left behind, and
/// the conflicting one ends up with the source content.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_overwrite_concurrent_replaces_and_cleans_temp() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"AAA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBB-new").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"CCC").await.unwrap();
    // Pre-existing dest file for /b.txt → file→file overwrite on the concurrent path.
    dest.create_file(Path::new("/b.txt"), b"BBB-old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-overwrite-concurrent",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "concurrent overwrite copy should succeed: {:?}", result);

    let mut sb = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
    assert_eq!(sb.next_chunk().await.unwrap().unwrap(), b"BBB-new");

    let entries = dest.list_directory(Path::new("/"), None).await.unwrap();
    assert!(
        !entries.iter().any(|e| e.name.contains(".cmdr-tmp-")),
        "no temp sibling should remain after a successful concurrent overwrite: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(entries.iter().filter(|e| e.name == "b.txt").count(), 1);
}

/// Wraps an `InMemoryVolume` destination whose `rename` ALWAYS fails. Models a
/// disconnect at the exact instant `finalize_safe_replace` tries to swap the
/// fully-written temp over the original: `delete(orig)` succeeds, then
/// `rename(temp, orig)` fails. Everything else delegates to the inner volume.
struct RenameFailsDestVolume {
    inner: Arc<InMemoryVolume>,
}

impl Volume for RenameFailsDestVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_export(&self) -> bool {
        true
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        // Let the concurrent test exercise the FuturesUnordered path.
        32
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> StdPin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_file(path, content)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn get_space_info<'a>(&'a self) -> StdPin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> StdPin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.inner.write_from_stream(dest, size, stream, on_progress)
    }
    /// The whole point of this double: the finalize rename always fails.
    fn rename<'a>(
        &'a self,
        _from: &'a Path,
        _to: &'a Path,
        _force: bool,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async {
            Err(VolumeError::IoError {
                message: "simulated disconnect during finalize rename".to_string(),
                raw_os_error: None,
            })
        })
    }
}

/// Assert the new data survives somewhere on `dest` after a failed finalize:
/// either still at `/notes.txt` (rename never happened) OR in a surviving
/// `*.cmdr-tmp-*` sibling (the committed-but-not-yet-renamed temp). It must NOT
/// be the case that both the original and the temp are gone — that's total data
/// loss, the defect under test.
async fn assert_new_data_survives(dest_inner: &Arc<InMemoryVolume>, expected_new: &[u8]) {
    let entries = dest_inner.list_directory(Path::new("/"), None).await.unwrap();
    // Find any path whose content equals the new bytes.
    let mut found = false;
    for e in &entries {
        let p = PathBuf::from(&e.path);
        if let Ok(mut stream) = dest_inner.open_read_stream(&p).await {
            let mut buf = Vec::new();
            while let Some(Ok(chunk)) = stream.next_chunk().await {
                buf.extend_from_slice(&chunk);
            }
            if buf == expected_new {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "after a finalize failure the NEW data must survive somewhere on dest \
         (orig slot or a .cmdr-tmp-* sibling); both gone = total data loss. Entries: {:?}",
        entries.iter().map(|e| (&e.name, e.size)).collect::<Vec<_>>()
    );
}

/// SERIAL path: streaming write SUCCEEDS but finalize (rename) FAILS. The temp
/// holds the only complete copy of the new data; the cleanup path must NOT
/// delete it. RED today: the serial closure leaves the temp in `last_dest_cell`
/// and the post-loop "Stopped or error" branch deletes it — after finalize
/// already deleted the original. Net: both gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_overwrite_serial_preserves_new_data_on_finalize_failure() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/notes.txt"), b"NEW").await.unwrap();
    let source: Arc<dyn Volume> = source;

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(RenameFailsDestVolume {
        inner: Arc::clone(&dest_inner),
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        // 1 source → serial path.
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-finalize-fail-serial",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/notes.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "a finalize-rename failure must surface as an error");
    assert_new_data_survives(&dest_inner, b"NEW").await;
}

/// CONCURRENT path: same finalize-failure scenario, ≥3 sources so the
/// FuturesUnordered path runs. RED today: the failing task returns
/// `Err((temp, e))`, the result handler sets `last_dest_path = Some(temp)`, and
/// the post-loop deletes it — after finalize already deleted the original.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_overwrite_concurrent_preserves_new_data_on_finalize_failure() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/a.txt"), b"AAA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBB-new").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"CCC").await.unwrap();
    let source: Arc<dyn Volume> = source;

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    // Conflict on /b.txt → file→file overwrite → safe-replace finalize fails.
    dest_inner.create_file(Path::new("/b.txt"), b"BBB-old").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(RenameFailsDestVolume {
        inner: Arc::clone(&dest_inner),
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-finalize-fail-concurrent",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "a finalize-rename failure must surface as an error");
    // The /b.txt new content must survive (orig slot or a temp sibling).
    assert_new_data_survives(&dest_inner, b"BBB-new").await;
}
