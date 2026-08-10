//! Shared doubles for the suites that KILL or CANCEL a cross-volume copy
//! mid-stream: a source whose chunks are handed out one test-granted permit at a
//! time, and a destination that publishes bytes at the write path as they arrive.
//!
//! The gate is what makes "the copy is parked exactly here" a fact rather than a
//! timing bet: grant one permit and the transfer advances one chunk, grant none
//! and every in-flight task stays wedged for as long as the test needs — which is
//! how a transfer that will not wind down is modeled.
//!
//! The destination has to publish incrementally to be worth anything here.
//! `LocalPosixVolume` and `SmbVolume` do (an open file handle is visible the
//! moment it is created); `InMemoryVolume` buffers the whole file and creates it
//! at the end, which would hide every defect these suites are about.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    CopyScanResult, DirectoryCreation, InMemoryVolume, ListingProgress, ScanConflict, SourceItemInfo, SpaceInfo,
    VolumeReadStream,
};
use std::future::Future;
use std::pin::Pin as StdPin;
use std::sync::atomic::AtomicU64;

/// How long a wait-for-the-copy-to-reach-a-known-point may take before the test
/// gives up. Generous: these run on a loaded CI box.
pub(super) const WAIT: Duration = Duration::from_secs(5);

pub(super) const CHUNK: usize = 4096;

// ============================================================================
// Test doubles
// ============================================================================

/// A read stream that hands out `total / CHUNK` chunks, one per permit from a
/// test-owned semaphore. Granting exactly one permit parks the copy at a known
/// byte offset instead of racing a timer, and never granting another leaves the
/// task wedged for as long as the test needs.
pub(super) struct GatedChunkStream {
    total: u64,
    emitted: u64,
    pub(super) gate: Arc<tokio::sync::Semaphore>,
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
pub(super) struct GatedSource {
    inner: Arc<InMemoryVolume>,
    gate: Arc<tokio::sync::Semaphore>,
    /// How many bytes each gated file streams. A source file the inner volume
    /// reports as EMPTY streams nothing instead, so a fixture can mix a source
    /// that lands immediately with sources that wedge.
    file_size: u64,
    /// Read streams opened so far. Once this reaches the concurrency window, the
    /// driver has spawned everything it can and is parked awaiting tasks — which
    /// is the state these suites need to reach before they perturb it.
    pub(super) opened: Arc<AtomicU64>,
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
        path: &'a Path,
    ) -> StdPin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let file_size = self.file_size;
        let gate = Arc::clone(&self.gate);
        let opened = Arc::clone(&self.opened);
        Box::pin(async move {
            // An empty source file streams nothing and its copy lands at once;
            // anything else streams `file_size` gated bytes.
            let real_size = self.inner.get_metadata(path).await.ok().and_then(|m| m.size);
            let total = if real_size == Some(0) { 0 } else { file_size };
            opened.fetch_add(1, Ordering::SeqCst);
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
pub(super) struct IncrementalDest {
    inner: Arc<InMemoryVolume>,
    /// Bytes published so far, so a test can wait on "the write is under way"
    /// with a plain synchronous condition.
    pub(super) written: Arc<AtomicU64>,
    /// When set, `rename` never returns: the LANDING wedges instead of the write.
    ///
    /// That shape matters because it is out of the hard-abort tier's reach — tier
    /// 2 races the source open and the streaming write, not the landing round
    /// trip — so a task wedged here can only be ended by the driver's drain
    /// deadline. It is what lets a test measure that deadline and nothing else.
    wedge_rename: bool,
    /// How many renames have been entered, so a test can wait for the wedge
    /// rather than guessing at a duration.
    pub(super) renames_entered: Arc<AtomicU64>,
}

impl IncrementalDest {
    /// Builds the volume and hands back the byte counter alongside it, so a test
    /// can wait on "the write is under way" without reaching through the `dyn`.
    pub(super) fn build(inner: Arc<InMemoryVolume>) -> (Arc<dyn Volume>, Arc<AtomicU64>) {
        let written = Arc::new(AtomicU64::new(0));
        let vol: Arc<dyn Volume> = Arc::new(Self {
            inner,
            written: Arc::clone(&written),
            wedge_rename: false,
            renames_entered: Arc::new(AtomicU64::new(0)),
        });
        (vol, written)
    }

    /// Same destination, but every staged write's LANDING hangs forever. Hands
    /// back the written-bytes and entered-renames counters.
    pub(super) fn build_with_wedged_rename(
        inner: Arc<InMemoryVolume>,
    ) -> (Arc<dyn Volume>, Arc<AtomicU64>, Arc<AtomicU64>) {
        let written = Arc::new(AtomicU64::new(0));
        let renames_entered = Arc::new(AtomicU64::new(0));
        let vol: Arc<dyn Volume> = Arc::new(Self {
            inner,
            written: Arc::clone(&written),
            wedge_rename: true,
            renames_entered: Arc::clone(&renames_entered),
        });
        (vol, written, renames_entered)
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
    ) -> StdPin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
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
        if !self.wedge_rename {
            return self.inner.rename(from, to, force);
        }
        let entered = Arc::clone(&self.renames_entered);
        Box::pin(async move {
            entered.fetch_add(1, Ordering::SeqCst);
            // Never returns: the landing is outside the hard-abort tier's reach,
            // so only the driver's drain deadline can end this task.
            std::future::pending::<()>().await;
            unreachable!("a pending future never resolves")
        })
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
pub(super) async fn dest_names(dest: &Arc<InMemoryVolume>) -> Vec<String> {
    dest.list_directory(Path::new("/"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

/// Full contents of a destination file, or `None` if it isn't there.
pub(super) async fn read_dest(dest: &Arc<InMemoryVolume>, path: &str) -> Option<Vec<u8>> {
    let mut stream = dest.open_read_stream(Path::new(path)).await.ok()?;
    let mut buf = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        buf.extend_from_slice(&chunk);
    }
    Some(buf)
}

/// Everything one of these tests drives: the gated source (plus the in-memory
/// volume its content lives in, the chunk gate, and the streams-opened counter)
/// and the incremental destination (plus its inner volume and published-byte
/// counter).
pub(super) struct Fixture {
    pub(super) source: Arc<dyn Volume>,
    pub(super) source_inner: Arc<InMemoryVolume>,
    pub(super) gate: Arc<tokio::sync::Semaphore>,
    pub(super) opened: Arc<AtomicU64>,
    pub(super) dest: Arc<dyn Volume>,
    pub(super) dest_inner: Arc<InMemoryVolume>,
    pub(super) written: Arc<AtomicU64>,
}

/// Builds the standard fixture: a gated source whose non-empty files stream
/// `size` bytes, and a destination that publishes bytes as they arrive.
pub(super) fn fixture(size: u64) -> Fixture {
    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    let (dest, written) = IncrementalDest::build(Arc::clone(&dest_inner));
    assemble(size, dest, dest_inner, written)
}

/// The same fixture with a destination whose LANDING never returns, so its tasks
/// wedge somewhere the hard-abort tier deliberately doesn't reach. Also hands
/// back the entered-renames counter to wait on.
pub(super) fn fixture_with_wedged_landing(size: u64) -> (Fixture, Arc<AtomicU64>) {
    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    let (dest, written, renames) = IncrementalDest::build_with_wedged_rename(Arc::clone(&dest_inner));
    (assemble(size, dest, dest_inner, written), renames)
}

fn assemble(
    size: u64,
    dest: Arc<dyn Volume>,
    dest_inner: Arc<InMemoryVolume>,
    written: Arc<AtomicU64>,
) -> Fixture {
    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let opened = Arc::new(AtomicU64::new(0));
    let source: Arc<dyn Volume> = Arc::new(GatedSource {
        inner: Arc::clone(&source_inner),
        gate: Arc::clone(&gate),
        file_size: size,
        opened: Arc::clone(&opened),
    });
    Fixture {
        source,
        source_inner,
        gate,
        opened,
        dest,
        dest_inner,
        written,
    }
}

thread_local! {
    /// Per-test override of the drain deadlines, `(cooperative cancel, hard
    /// abort)`, read by `super::drain_deadline()` in test builds. `None` ⇒ the
    /// production constants. Set through [`CancelDrainGuard`].
    static DRAIN_OVERRIDE: std::cell::Cell<Option<(Duration, Duration)>> = const { std::cell::Cell::new(None) };
}

pub(super) fn drain_override(aborting: bool) -> Option<Duration> {
    DRAIN_OVERRIDE
        .with(std::cell::Cell::get)
        .map(|(cancel, abort)| if aborting { abort } else { cancel })
}

/// Shortens the drain deadlines for the current thread, restoring them on drop,
/// so a suite can watch the abandon path fire without waiting out the production
/// window.
///
/// Thread-local, like `volume::strategy`'s `AutoYieldTuningGuard`: the driver runs
/// inline on the test's own task (these tests `.await` it rather than spawning),
/// so it reads this thread's value.
pub(super) struct CancelDrainGuard {
    prev: Option<(Duration, Duration)>,
}

impl CancelDrainGuard {
    /// Overrides the cooperative-cancel deadline, leaving the abort one at a
    /// value so short it can't be what a test is measuring.
    pub(super) fn set(deadline: Duration) -> Self {
        Self::set_both(deadline, Duration::from_millis(1))
    }

    /// Overrides both, for the suite that has to tell the two apart.
    pub(super) fn set_both(cancel: Duration, abort: Duration) -> Self {
        Self {
            prev: DRAIN_OVERRIDE.with(|c| c.replace(Some((cancel, abort)))),
        }
    }
}

impl Drop for CancelDrainGuard {
    fn drop(&mut self) {
        DRAIN_OVERRIDE.with(|c| c.set(self.prev));
    }
}
