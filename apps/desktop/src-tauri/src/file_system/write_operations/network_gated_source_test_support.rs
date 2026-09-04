//! The source a cancel cell holds still: one file, handed out a chunk at a time.
//!
//! ❗ This, not a big file and a stopwatch, is what makes "the cancel landed
//! while the upload was still running" a FACT. Grant one permit and the transfer
//! is parked at a known byte offset, with the destination holding an open,
//! incomplete staging sibling, for as long as the cell needs. It also keeps the
//! cell inside the workspace-wide 8 s nextest cap, which a payload big enough to
//! outrun a stopwatch would not.
//!
//! Same shape as `transfer/volume/copy_wedge_test_support.rs`'s `GatedChunkStream`,
//! which is private to that module and so can't be reused from out here. It lives
//! apart from `network_transfer_test_support.rs` because the `Volume` impl below
//! is mostly trait signatures, and folding them in put that file over the
//! `file-length` warn threshold.

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cmdr_fs::volume::Volume;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, ListingProgress, VolumeError, VolumeReadStream};

/// How big the file a cancel interrupts is, and how much of it one permit buys.
///
/// Small on purpose: the gate is what makes the cancel land mid-file, so nothing
/// here has to be big enough to outrun a timer.
pub(super) const CANCEL_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
const CANCEL_CHUNK_BYTES: usize = 64 * 1024;

/// A read stream that hands out one chunk per permit from a test-owned
/// semaphore.
struct GatedChunks {
    bytes: Arc<Vec<u8>>,
    emitted: usize,
    gate: Arc<tokio::sync::Semaphore>,
    handed_out: Arc<AtomicU64>,
}

impl VolumeReadStream for GatedChunks {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.emitted >= self.bytes.len() {
                return None;
            }
            match self.gate.acquire().await {
                Ok(permit) => permit.forget(),
                Err(_) => return None,
            }
            let end = (self.emitted + CANCEL_CHUNK_BYTES).min(self.bytes.len());
            let chunk = self.bytes[self.emitted..end].to_vec();
            self.emitted = end;
            self.handed_out.fetch_add(1, Ordering::SeqCst);
            Some(Ok(chunk))
        })
    }

    fn total_size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn bytes_read(&self) -> u64 {
        self.emitted as u64
    }
}

/// A source volume holding one file, whose bytes only move when the cell says
/// so. Everything but the read stream is an `InMemoryVolume`'s answer.
struct GatedUploadSource {
    inner: InMemoryVolume,
    bytes: Arc<Vec<u8>>,
    gate: Arc<tokio::sync::Semaphore>,
    handed_out: Arc<AtomicU64>,
}

impl Volume for GatedUploadSource {
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }
    fn open_read_stream<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let bytes = Arc::clone(&self.bytes);
        let gate = Arc::clone(&self.gate);
        let handed_out = Arc::clone(&self.handed_out);
        Box::pin(async move {
            let stream: Box<dyn VolumeReadStream> = Box::new(GatedChunks {
                bytes,
                emitted: 0,
                gate,
                handed_out,
            });
            Ok(stream)
        })
    }
}

/// The handles a cancel cell drives the source through.
pub(super) struct GatedUpload {
    /// The source to copy FROM. Holds one file at `/big.bin`.
    pub(super) volume: Arc<dyn Volume>,
    /// One permit buys one chunk.
    pub(super) gate: Arc<tokio::sync::Semaphore>,
    /// Chunks handed to the destination so far, so a cell can wait on "the
    /// upload has actually started" with a plain synchronous condition.
    pub(super) handed_out: Arc<AtomicU64>,
}

/// Builds the source with its gate closed, so nothing moves until the cell says
/// so.
pub(super) async fn gated_upload(bytes: Vec<u8>) -> GatedUpload {
    let bytes = Arc::new(bytes);
    let inner = InMemoryVolume::new("Gated");
    inner
        .create_file(Path::new("/big.bin"), &bytes)
        .await
        .expect("seeding the gated source");
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let handed_out = Arc::new(AtomicU64::new(0));
    GatedUpload {
        volume: Arc::new(GatedUploadSource {
            inner,
            bytes,
            gate: Arc::clone(&gate),
            handed_out: Arc::clone(&handed_out),
        }),
        gate,
        handed_out,
    }
}
