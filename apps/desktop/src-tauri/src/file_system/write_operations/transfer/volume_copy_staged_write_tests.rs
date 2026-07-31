//! A destination file must never carry its FINAL name until its last byte has
//! landed.
//!
//! The 2026-07-31 wedge was force-quit mid-transfer and left
//! `sms-20260726002817.xml` at zero bytes and `sms-20260725002819.xml` truncated
//! at 4 MiB, both at their final names, indistinguishable from complete files
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Neither had a
//! conflict, so neither took the safe-replace temp; they streamed straight to the
//! destination path.
//!
//! These suites reproduce that by ABANDONING the copy future mid-stream — the
//! in-process equivalent of a force-quit, since no error path, no cleanup, and no
//! `Drop` on the backend writer gets to run. The destination doubles here write
//! incrementally (like `LocalPosixVolume` and `SmbVolume` do, and unlike
//! `InMemoryVolume`, which buffers the whole file and creates it at the end), so
//! "bytes are visible at the write path mid-stream" is modeled faithfully.

use super::tests::make_state;
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, InMemoryVolume, ListingProgress, ScanConflict, SourceItemInfo, SpaceInfo, VolumeReadStream,
};
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};
use cmdr_fs::testing::wait_until_async;
use std::future::Future;
use std::pin::Pin as StdPin;
use std::sync::atomic::AtomicU64;

/// How long a wait-for-the-copy-to-reach-a-known-point may take before the test
/// gives up. Generous: these run on a loaded CI box.
const WAIT: Duration = Duration::from_secs(5);

const CHUNK: usize = 4096;

// ============================================================================
// Test doubles
// ============================================================================

/// A read stream that hands out `total / CHUNK` chunks, one per permit from a
/// test-owned semaphore. Granting exactly one permit parks the copy at a known
/// byte offset instead of racing a timer, and never granting another leaves the
/// task wedged for as long as the test needs.
struct GatedChunkStream {
    total: u64,
    emitted: u64,
    gate: Arc<tokio::sync::Semaphore>,
}

impl VolumeReadStream for GatedChunkStream {
    fn next_chunk(&mut self) -> StdPin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.emitted >= self.total {
                return None;
            }
            match self.gate.acquire().await {
                Ok(permit) => permit.forget(),
                Err(_) => return None,
            }
            let len = CHUNK.min((self.total - self.emitted) as usize);
            self.emitted += len as u64;
            Some(Ok(vec![0xAB; len]))
        })
    }

    fn total_size(&self) -> u64 {
        self.total
    }

    fn bytes_read(&self) -> u64 {
        self.emitted
    }
}

/// Source volume: metadata / listing / scan delegate to an `InMemoryVolume`, but
/// every read stream is gated so the test controls exactly how far the copy gets.
struct GatedSource {
    inner: Arc<InMemoryVolume>,
    gate: Arc<tokio::sync::Semaphore>,
    file_size: u64,
}

impl Volume for GatedSource {
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
        8
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
        self.inner.scan_for_copy(path)
    }
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let total = self.file_size;
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            let stream: Box<dyn VolumeReadStream> = Box::new(GatedChunkStream {
                total,
                emitted: 0,
                gate,
            });
            Ok(stream)
        })
    }
}

/// Destination volume that publishes bytes AT THE WRITE PATH as they arrive,
/// the way a real file handle does. `InMemoryVolume::write_from_stream` buffers
/// the whole file and creates it only at the end, which would hide exactly the
/// defect these tests are about.
struct IncrementalDest {
    inner: Arc<InMemoryVolume>,
    /// Bytes published so far, so a test can wait on "the write is under way"
    /// with a plain synchronous condition.
    written: Arc<AtomicU64>,
}

impl IncrementalDest {
    /// Builds the volume and hands back the byte counter alongside it, so a test
    /// can wait on "the write is under way" without reaching through the `dyn`.
    fn build(inner: Arc<InMemoryVolume>) -> (Arc<dyn Volume>, Arc<AtomicU64>) {
        let written = Arc::new(AtomicU64::new(0));
        let vol: Arc<dyn Volume> = Arc::new(Self {
            inner,
            written: Arc::clone(&written),
        });
        (vol, written)
    }
}

impl IncrementalDest {
    /// Replaces whatever is at `path` with `data`, so a growing write is visible
    /// at the write path chunk by chunk (`InMemoryVolume::create_file` refuses to
    /// overwrite).
    async fn publish(&self, path: &Path, data: &[u8]) -> Result<(), VolumeError> {
        let _ = self.inner.delete(path).await;
        self.inner.create_file(path, data).await
    }
}

impl Volume for IncrementalDest {
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
        8
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
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory_all(path)
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
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> StdPin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.rename(from, to, force)
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
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_conflicts(source_items, dest_path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> StdPin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut data: Vec<u8> = Vec::new();
            // Publish an empty file up front, like a `File::create` / SMB CREATE
            // does: this is the zero-byte artifact the incident left behind.
            self.publish(dest, &data).await?;
            while let Some(result) = stream.next_chunk().await {
                let chunk = result?;
                data.extend_from_slice(&chunk);
                self.publish(dest, &data).await?;
                self.written.store(data.len() as u64, Ordering::SeqCst);
                if on_progress(data.len() as u64, size).is_break() {
                    // What every real backend does on cancel: drop the handle and
                    // remove the partial at the WRITE path.
                    let _ = self.inner.delete(dest).await;
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                }
            }
            Ok(data.len() as u64)
        })
    }
}

/// Names of everything sitting in the destination root.
async fn dest_names(dest: &Arc<InMemoryVolume>) -> Vec<String> {
    dest.list_directory(Path::new("/"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

/// Full contents of a destination file, or `None` if it isn't there.
async fn read_dest(dest: &Arc<InMemoryVolume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = dest.open_read_stream(Path::new(path)).await.ok()?;
    let mut buf = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk);
    }
    Some(buf)
}

/// Everything a staged-write test drives: the gated source (plus the in-memory
/// volume its content lives in and the chunk gate), the incremental destination
/// (plus its inner volume and published-byte counter).
struct Fixture {
    source: Arc<dyn Volume>,
    source_inner: Arc<InMemoryVolume>,
    gate: Arc<tokio::sync::Semaphore>,
    dest: Arc<dyn Volume>,
    dest_inner: Arc<InMemoryVolume>,
    written: Arc<AtomicU64>,
}

/// Builds the standard fixture: a gated source whose files stream `size` bytes,
/// and a destination that publishes bytes as they arrive.
fn fixture(size: u64) -> Fixture {
    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let source: Arc<dyn Volume> = Arc::new(GatedSource {
        inner: Arc::clone(&source_inner),
        gate: Arc::clone(&gate),
        file_size: size,
    });
    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    let (dest, written) = IncrementalDest::build(Arc::clone(&dest_inner));
    Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
    }
}

// ============================================================================
// M2: a killed transfer leaves nothing at a final name
// ============================================================================

/// A FRESH copy (no conflict, so no safe-replace temp) abandoned mid-stream must
/// leave nothing at the destination's final name. Pre-staging the bytes streamed
/// straight to `/notes.txt`, so a force-quit left a truncated file there that
/// looked complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_fresh_copy_leaves_no_partial_at_the_final_name() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
    } = fixture(CHUNK as u64 * 4);
    source_inner
        .create_file(Path::new("/notes.txt"), &vec![0xAB; CHUNK * 4])
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    {
        let sources = [PathBuf::from("/notes.txt")];
        let copy = copy_volumes_with_progress(
            events.clone(),
            "test-op-abandon-fresh",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        );
        tokio::pin!(copy);

        // Let exactly one chunk through, then abandon the copy where it parks
        // waiting for the second. Dropping the future is the in-process
        // equivalent of the force-quit: no cleanup of any kind runs.
        gate.add_permits(1);
        tokio::select! {
            r = &mut copy => panic!("the gated copy must not run to completion: {r:?}"),
            () = wait_until_async(WAIT, "the first chunk to land at the destination", || {
                written.load(Ordering::SeqCst) > 0
            }) => {}
        }
    }

    let names = dest_names(&dest_inner).await;
    assert!(
        !names.iter().any(|n| n == "notes.txt"),
        "an abandoned transfer must leave NOTHING at the file's final name; found {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "the abandoned partial must survive under a recognizable .cmdr-tmp-* name; found {names:?}"
    );
}

/// The OVERWRITE path's counterpart: abandoning mid-stream must leave the
/// original destination file untouched and complete, and must not park partial
/// bytes at the final name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_overwrite_leaves_the_original_intact() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
    } = fixture(CHUNK as u64 * 4);
    source_inner
        .create_file(Path::new("/notes.txt"), &vec![0xAB; CHUNK * 4])
        .await
        .unwrap();
    dest_inner
        .create_file(Path::new("/notes.txt"), b"ORIGINAL DEST DATA")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    {
        let sources = [PathBuf::from("/notes.txt")];
        let copy = copy_volumes_with_progress(
            events.clone(),
            "test-op-abandon-overwrite",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        );
        tokio::pin!(copy);

        gate.add_permits(1);
        tokio::select! {
            r = &mut copy => panic!("the gated copy must not run to completion: {r:?}"),
            () = wait_until_async(WAIT, "the first chunk to land at the destination", || {
                written.load(Ordering::SeqCst) > 0
            }) => {}
        }
    }

    assert_eq!(
        read_dest(&dest_inner, "/notes.txt").await.as_deref(),
        Some(&b"ORIGINAL DEST DATA"[..]),
        "an abandoned overwrite must leave the original destination file complete"
    );
}

/// A file inside a DIRECTORY source gets the same guarantee. Directory sources
/// never entered the driver's in-flight partial ledger, so their children were
/// the least protected case of all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_directory_child_leaves_no_partial_at_the_final_name() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
    } = fixture(CHUNK as u64 * 4);
    source_inner.create_directory(Path::new("/folder")).await.unwrap();
    source_inner
        .create_file(Path::new("/folder/notes.txt"), &vec![0xAB; CHUNK * 4])
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    {
        let sources = [PathBuf::from("/folder")];
        let copy = copy_volumes_with_progress(
            events.clone(),
            "test-op-abandon-dir-child",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        );
        tokio::pin!(copy);

        gate.add_permits(1);
        tokio::select! {
            r = &mut copy => panic!("the gated copy must not run to completion: {r:?}"),
            () = wait_until_async(WAIT, "the first chunk to land at the destination", || {
                written.load(Ordering::SeqCst) > 0
            }) => {}
        }
    }

    let names: Vec<String> = dest_inner
        .list_directory(Path::new("/folder"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        !names.iter().any(|n| n == "notes.txt"),
        "an abandoned merge child must leave NOTHING at its final name; found {names:?}"
    );
}

/// The staging must be invisible on the happy path: a completed fresh copy lands
/// the full content at the final name and leaves no temp behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_completed_copy_lands_at_the_final_name_with_no_temp_left() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        ..
    } = fixture(CHUNK as u64 * 2);
    source_inner
        .create_file(Path::new("/notes.txt"), &vec![0xAB; CHUNK * 2])
        .await
        .unwrap();
    gate.add_permits(8); // enough for the whole file

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    copy_volumes_with_progress(
        events.clone(),
        "test-op-staged-happy",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/notes.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the copy should succeed");

    let names = dest_names(&dest_inner).await;
    assert_eq!(names, vec!["notes.txt".to_string()], "exactly the final file, no temp");
    assert_eq!(
        read_dest(&dest_inner, "/notes.txt").await.map(|b| b.len()),
        Some(CHUNK * 2),
        "the landed file must hold every byte"
    );
}
