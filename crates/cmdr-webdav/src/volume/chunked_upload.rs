//! A big file onto a Nextcloud, in chunks: `MKCOL` an uploads collection,
//! `PUT` each chunk into it, then `MOVE` its `.file` onto the target, which
//! assembles them server-side. Where it lives and how a file splits:
//! `crate::chunking`. Why: `DETAILS.md` § "Chunked uploads (Nextcloud)".
//!
//! Each chunk is one of `writes.rs`'s streaming `PUT`s over a window of the
//! one source stream, so the size guard, the read-ahead, progress, and cancel
//! hold per chunk exactly as they do for a whole file. ❗ The target sees no
//! byte until the assembling `MOVE`, and every failure before it leaves the
//! target untouched and DELETEs the uploads collection (which cancels a v2
//! upload's backend write too).

use std::ops::ControlFlow;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ScanStop, StreamLength, StreamWritten, VolumeError, VolumeReadStream};
use log::debug;
use reqwest::Method;

use super::WebdavVolume;
use super::writes::{UploadDates, size_mismatch};
use crate::chunking::{ChunkedUploads, chunks};
use crate::errors::Attempted;
use crate::transport::{MUTATION_BUDGET, WebdavClient, method};

/// The header naming the whole file's length on every chunk request, so v2
/// checks the account's quota up front (a 507 on the first chunk) rather than
/// at the assembling `MOVE`.
const OC_TOTAL_LENGTH: &str = "OC-Total-Length";

/// The slowest assembly the `MOVE`'s budget allows for, in bytes per second.
/// Assembling copies every chunk into the file, so a big file on a slow disk
/// can outlast [`MUTATION_BUDGET`]; past that, the budget grows with the size.
const SLOWEST_ASSEMBLY: u64 = 10 * 1024 * 1024;

/// Every uploads collection this process started, for a cell to ask the
/// server whether each is gone: Nextcloud refuses to list the uploads home
/// ("Listing members of this collection is disabled"), so naming them is the
/// only way to look. Process-wide is enough: nextest runs each cell alone.
#[cfg(test)]
pub(super) static STARTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The assembling `MOVE`'s budget for a file of `size` bytes.
fn assembly_budget(size: u64) -> Duration {
    MUTATION_BUDGET.max(Duration::from_secs(size / SLOWEST_ASSEMBLY))
}

impl WebdavVolume {
    /// Uploads `stream` (exactly `size` bytes) to `target` through `uploads`.
    /// Returns what `put_streaming` does, and like it removes whatever a
    /// failure left behind.
    #[allow(
        clippy::too_many_arguments,
        reason = "put_streaming's arguments, plus where and which dates"
    )]
    pub(super) async fn upload_in_chunks(
        &self,
        client: &Arc<WebdavClient>,
        uploads: &ChunkedUploads,
        target: &str,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        dates: &UploadDates,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        let id = format!("cmdr-{}", uuid::Uuid::new_v4().simple());
        #[cfg(test)]
        STARTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(id.clone());
        let destination = uploads.destination(target);
        debug!(
            "WebdavVolume::write_from_stream: {target} in chunks via uploads/{id}",
            target = LogPrivate::file_path(&target)
        );
        let mkcol = client
            .request(method("MKCOL"), uploads.session(&id))
            .header("Destination", destination.as_str())
            .timeout(MUTATION_BUDGET);
        self.send(client, mkcol, target, Attempted::Reaching).await?;

        let source = SharedSource::new(stream);
        let mut sent = 0;
        for (number, len) in chunks(size, self.upload_chunk_size()) {
            let request = client
                .request(Method::PUT, uploads.chunk(&id, number))
                .header("Destination", destination.as_str())
                .header(OC_TOTAL_LENGTH, size);
            let window = Box::new(source.window(len));
            let progress = |done: u64, _: u64| on_progress(sent + done, size);
            if let Err(failure) = self.stream_put(client, request, target, len, window, &progress).await {
                self.abandon(client, uploads, &id).await;
                // A source that ran dry is the WHOLE file's short count, not
                // this chunk's.
                return Err(match source.ran_dry().await {
                    Some(pulled) => size_mismatch(target, pulled, size),
                    None => failure.settle(target, self.volume_id()).await,
                });
            }
            sent += len;
        }
        // ❗ The windows took exactly `size`; a source with bytes left over
        // promised less than it holds, and assembling would keep a prefix.
        if let Err(e) = source.ensure_drained(target, size).await {
            self.abandon(client, uploads, &id).await;
            return Err(e);
        }

        let assemble = dates.on(client
            .request(method("MOVE"), uploads.assembled(&id))
            .header("Destination", destination.as_str())
            .header(OC_TOTAL_LENGTH, size)
            .timeout(assembly_budget(size)));
        match self.send(client, assemble, target, Attempted::Reaching).await {
            // The server deletes the uploads collection itself once it's in place.
            Ok(response) => Ok(dates.written(size, dates.judge(&response, target))),
            Err(e) => {
                self.abandon(client, uploads, &id).await;
                // Whatever a half-done assembly may have left at the target.
                self.remove_best_effort(target).await;
                Err(e)
            }
        }
    }

    /// DELETEs upload `id`'s collection, chunks and all, saying nothing if
    /// that fails: the error that got us here is the one worth reporting, and
    /// the server expires an abandoned upload on its own after a day.
    async fn abandon(&self, client: &WebdavClient, uploads: &ChunkedUploads, id: &str) {
        let request = client
            .request(Method::DELETE, uploads.session(id))
            .timeout(MUTATION_BUDGET);
        let _ = self.send(client, request, id, Attempted::Reaching).await;
    }
}

/// The one source stream every chunk's window reads from, and what it has
/// yielded so far.
struct Source {
    stream: Box<dyn VolumeReadStream>,
    /// Bytes pulled out of the source that the window being read had no room
    /// for: a source piece rarely ends on a chunk boundary.
    carry: Vec<u8>,
    /// Bytes pulled out of the source so far.
    pulled: u64,
    /// Whether the source ended while a window still wanted bytes.
    ran_dry: bool,
}

/// The source behind every window. ❗ One window reads at a time (chunks go
/// up one after another), so the lock is never contended; it's what lets each
/// window own its reference for the `'static` body a request needs.
#[derive(Clone)]
struct SharedSource {
    source: Arc<tokio::sync::Mutex<Source>>,
    /// The source's Cancel and pause, which every window hands on (a wrapper
    /// stream forwards its inner stream's answer): the chunk's `PUT` notes a
    /// body the pause left silent by it.
    pause: ScanStop,
}

impl SharedSource {
    fn new(stream: Box<dyn VolumeReadStream>) -> Self {
        Self {
            pause: stream.stop_signal(),
            source: Arc::new(tokio::sync::Mutex::new(Source {
                stream,
                carry: Vec::new(),
                pulled: 0,
                ran_dry: false,
            })),
        }
    }

    /// The next `len` bytes of the source, as a stream of their own.
    fn window(&self, len: u64) -> Window {
        Window {
            source: self.clone(),
            left: len,
            read: 0,
        }
    }

    /// The bytes the source yielded in all, if it ended before a window was
    /// full.
    async fn ran_dry(&self) -> Option<u64> {
        let source = self.source.lock().await;
        source.ran_dry.then_some(source.pulled)
    }

    /// `Ok` when the source has nothing past the `size` bytes the windows took.
    async fn ensure_drained(&self, target: &str, size: u64) -> Result<(), VolumeError> {
        let mut source = self.source.lock().await;
        let mut extra = source.carry.len() as u64;
        while extra == 0 {
            match source.stream.next_chunk().await {
                None => return Ok(()),
                Some(Ok(piece)) => extra = piece.len() as u64,
                Some(Err(e)) => return Err(e),
            }
        }
        Err(size_mismatch(target, size + extra, size))
    }
}

/// A window onto the shared source: at most `left` more bytes, then the end.
struct Window {
    source: SharedSource,
    left: u64,
    read: u64,
}

impl VolumeReadStream for Window {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            // ❗ Checked before the lock: a full window answers the end without
            // touching the source, which the next window reads on from.
            if self.left == 0 {
                return None;
            }
            let mut source = self.source.source.lock().await;
            let mut piece = if source.carry.is_empty() {
                match source.stream.next_chunk().await {
                    None => {
                        source.ran_dry = true;
                        return None;
                    }
                    Some(Err(e)) => return Some(Err(e)),
                    Some(Ok(piece)) => {
                        source.pulled += piece.len() as u64;
                        piece
                    }
                }
            } else {
                std::mem::take(&mut source.carry)
            };
            let room = usize::try_from(self.left).unwrap_or(usize::MAX);
            if piece.len() > room {
                source.carry = piece.split_off(room);
            }
            self.left -= piece.len() as u64;
            self.read += piece.len() as u64;
            Some(Ok(piece))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.read + self.left)
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    /// None: the dates ride the assembling `MOVE`, never a chunk.
    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }

    fn stop_signal(&self) -> ScanStop {
        self.source.pause.clone()
    }
}

#[cfg(test)]
#[path = "chunked_upload_test.rs"]
mod chunked_upload_test;
