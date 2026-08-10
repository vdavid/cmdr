//! Crash-safety tests for the copy engines: the destination path must never
//! hold a partial, and the original must survive every failure short of the
//! final swap.
//!
//! Two suites live here. The cross-volume one covers the file→file Overwrite
//! safe-replace guarantee against a mid-stream read/write or finalize-rename
//! failure; its Volume test doubles (`FailAfterOneChunkStream`,
//! `FailingReadSourceVolume`, `RenameFailsDestVolume`) model those. The
//! local-FS one at the bottom covers the same promise for
//! `copy_strategy::copy_file_using` against a real tempdir.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume/copy_tests.rs`
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

/// Wraps an `InMemoryVolume` destination whose `rename` fails when it would land
/// something at `fails_onto`. Models a disconnect at the exact instant
/// `finalize_safe_replace` tries to swap the fully-written temp over the
/// original: `delete(orig)` succeeds, then `rename(temp, orig)` fails.
/// Everything else delegates to the inner volume.
///
/// Scoped to ONE path rather than failing every rename because every streaming
/// write now lands through a rename (`staged_write.rs`): a blanket failure would
/// break the copy of every unrelated file in the batch and the test would stop
/// measuring the finalize failure it is about.
struct RenameFailsDestVolume {
    inner: Arc<InMemoryVolume>,
    fails_onto: PathBuf,
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
    /// The whole point of this double: the finalize rename onto `fails_onto`
    /// fails. Every other rename (the staged landing of the batch's other files)
    /// goes through, so the test isolates the finalize failure.
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if to != self.fails_onto {
            return self.inner.rename(from, to, force);
        }
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
        fails_onto: PathBuf::from("/notes.txt"),
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
        fails_onto: PathBuf::from("/b.txt"),
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

// ========================================================================
// Local-FS staging: the destination NAME never holds a partial
// ========================================================================
//
// The same promise as the cross-volume suite above, on the most ordinary path
// in the app. A local copy writes to a `.cmdr-tmp-*` sibling and takes the real
// name by one same-directory rename, so a crash, a force-quit, or a worker
// thread abandoned in a syscall we can't interrupt leaves a recognizable
// leftover rather than a truncated file wearing the user's filename.
//
// These drive `copy_file_using` with an explicitly chosen `Chunked` strategy:
// on a Mac every tempdir pair is one APFS volume, so the selector always picks
// the clone branch and nothing would otherwise exercise the branch that runs
// for an external drive, an SD card, or a Finder-mounted NAS — the very case
// the 4 GB-copy-then-quit scenario describes.

mod local_staging {
    use crate::file_system::write_operations::state::{OperationIntent, WriteOperationState};
    use crate::file_system::write_operations::transfer::copy_strategy::{LocalCopyStrategy, copy_file_using};
    use crate::file_system::write_operations::types::WriteOperationError;
    use crate::ignore_poison::IgnorePoison;
    use crate::test_support::TestDir;
    use cmdr_fs::staging::STAGING_TEMP_MARKER;
    use std::cell::Cell;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    /// 3.5 chunks of the chunked copier's 1 MiB window, so the progress
    /// callback fires several times mid-write.
    const MULTI_CHUNK_BYTES: usize = 1024 * 1024 * 3 + 12_345;

    fn running_state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState::new(Duration::from_millis(50)))
    }

    /// The names in `dir` that carry Cmdr's incoming-scratch marker.
    fn staging_temps(dir: &Path) -> Vec<String> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(STAGING_TEMP_MARKER))
            .collect()
    }

    /// While the bytes are still arriving, the destination name must not exist
    /// and a recognizable temp must. That snapshot IS what a force-quit or an
    /// abandoned worker thread leaves on disk, so asserting it mid-write is the
    /// in-process equivalent of pulling the plug.
    #[test]
    fn an_abandoned_local_copy_leaves_a_cmdr_temp_and_no_file_at_the_real_name() {
        let dir = TestDir::new("local_staging_abandoned");
        let src = dir.join("holiday.raw");
        let dest = dir.join("dst").join("holiday.raw");
        fs::create_dir_all(dest.parent().expect("dest has a parent")).unwrap();
        fs::write(&src, vec![0x5A_u8; MULTI_CHUNK_BYTES]).unwrap();

        let state = running_state();
        let dest_dir = dest.parent().expect("dest has a parent").to_path_buf();
        let saw_real_name = Cell::new(false);
        let saw_temp = Cell::new(false);
        let tracked_mid_write = Cell::new(false);
        let observe = |_done: u64, _total: u64| {
            if dest.exists() {
                saw_real_name.set(true);
            }
            if !staging_temps(&dest_dir).is_empty() {
                saw_temp.set(true);
            }
            if !state.in_flight_temps.lock_ignore_poison().is_empty() {
                tracked_mid_write.set(true);
            }
        };

        copy_file_using(LocalCopyStrategy::Chunked, &state, &src, &dest, false, Some(&observe))
            .expect("the copy itself must succeed");

        assert!(
            !saw_real_name.get(),
            "the destination name held a partially-written file: a crash there leaves the user a truncated file that looks complete"
        );
        assert!(
            saw_temp.get(),
            "mid-write there must be a recognizable `.cmdr-tmp-*` sibling holding the incoming bytes"
        );
        assert!(
            tracked_mid_write.get(),
            "the partial must be listed in the operation's in-flight temps so an abandoned copy's litter can be found"
        );

        // And when it does finish, the file is at its real name, complete, with
        // nothing left over.
        assert_eq!(fs::metadata(&dest).unwrap().len(), MULTI_CHUNK_BYTES as u64);
        assert!(
            staging_temps(&dest_dir).is_empty(),
            "a completed copy leaves no scratch behind: {:?}",
            staging_temps(&dest_dir)
        );
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
    }

    /// Cancelling mid-copy must leave nothing at the destination name — not a
    /// zero-byte file, not a truncated one, not briefly. The "not briefly"
    /// matters: cleanup after the fact is a race a force-quit wins, so the
    /// observation is taken from inside the write rather than after it.
    #[test]
    fn a_cancelled_local_chunked_copy_leaves_no_file_at_the_destination_name() {
        let dir = TestDir::new("local_staging_cancelled");
        let src = dir.join("holiday.raw");
        let dest = dir.join("dst").join("holiday.raw");
        let dest_dir = dest.parent().expect("dest has a parent").to_path_buf();
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(&src, vec![0x5A_u8; MULTI_CHUNK_BYTES]).unwrap();

        let state = running_state();
        let saw_real_name = Cell::new(false);
        let cancel_after_first_chunk = |_done: u64, _total: u64| {
            if dest.exists() {
                saw_real_name.set(true);
            }
            state.intent.store(OperationIntent::Stopped as u8, Ordering::SeqCst);
        };

        let result = copy_file_using(
            LocalCopyStrategy::Chunked,
            &state,
            &src,
            &dest,
            false,
            Some(&cancel_after_first_chunk),
        );

        assert!(
            matches!(result, Err(WriteOperationError::Cancelled { .. })),
            "expected a Cancelled outcome, got {result:?}"
        );
        assert!(
            !saw_real_name.get(),
            "the destination name held a partial while the copy was still running"
        );
        assert!(
            !dest.exists(),
            "a cancelled copy must leave nothing at the destination name"
        );
        assert!(
            state.in_flight_temps.lock_ignore_poison().is_empty(),
            "a cancelled copy must stop tracking its partial"
        );
    }

    /// Staging moved the create off the destination name, and a plain POSIX
    /// rename replaces silently — so the landing has to refuse a destination
    /// that appeared underneath a non-overwrite copy, the way the direct
    /// `O_EXCL` create used to.
    #[test]
    fn a_non_overwrite_local_copy_refuses_to_clobber_a_destination_that_appeared() {
        let dir = TestDir::new("local_staging_no_clobber");
        let src = dir.join("incoming.txt");
        let dest = dir.join("occupied.txt");
        fs::write(&src, "new bytes").unwrap();
        fs::write(&dest, "the user's own file").unwrap();

        let result = copy_file_using(
            LocalCopyStrategy::Chunked,
            &running_state(),
            &src,
            &dest,
            // Not an overwrite: nobody resolved a conflict for this path.
            false,
            None,
        );

        assert!(
            matches!(result, Err(WriteOperationError::DestinationExists { .. })),
            "expected DestinationExists, got {result:?}"
        );
        assert_eq!(
            fs::read_to_string(&dest).unwrap(),
            "the user's own file",
            "the file that was there must be untouched"
        );
        assert!(
            staging_temps(&dir).is_empty(),
            "the refused copy's temp must be cleaned up: {:?}",
            staging_temps(&dir)
        );
    }

    /// An overwrite still replaces, and still leaves no scratch behind.
    #[test]
    fn a_local_overwrite_replaces_the_destination_and_cleans_up_both_scratch_files() {
        let dir = TestDir::new("local_staging_overwrite");
        let src = dir.join("incoming.txt");
        let dest = dir.join("target.txt");
        fs::write(&src, "new bytes").unwrap();
        fs::write(&dest, "old bytes").unwrap();

        copy_file_using(LocalCopyStrategy::Chunked, &running_state(), &src, &dest, true, None)
            .expect("the overwrite must succeed");

        assert_eq!(fs::read_to_string(&dest).unwrap(), "new bytes");
        let leftovers: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
                cmdr_fs::staging::is_staging_temp_name(&name)
            })
            .collect();
        assert!(leftovers.is_empty(), "no temp or aside may survive: {leftovers:?}");
    }
}
