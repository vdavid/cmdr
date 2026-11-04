//! The bytes of a picture or PDF that live on a volume: no local path to open, so the
//! `cmdr-media://` handler reads them by position through the session's
//! [`SharedSource`]. `DETAILS.md` § "Media on a volume".
//!
//! Tauri's scheme responder takes one finished body, so a whole-file request (every
//! `<img>`) is read in full before the webview sees a byte. That's why a read here has no
//! total deadline but a stall rule, stops between chunks, and reports how far it got.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use cmdr_fs::deadline::StallWatch;
use cmdr_fs::rt::{self, Instant};
use serde::{Deserialize, Serialize};

use super::ViewerError;
use super::source::SharedSource;
use cmdr_fs::ignore_poison::IgnorePoison;

/// Past this, a file on a volume doesn't render: one whole-file request would hold it all
/// in memory, here and again in the webview. A held reader falls through to the text and
/// hex views, and any other volume to the pull, which answers `TooLargeToPreview`.
pub(super) const MEDIA_ON_VOLUME_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// How much one read asks the volume for. Sets how fast a close or a stall stops the read
/// and how often progress moves. Four of the source's blocks, so it's one round trip.
const READ_CHUNK_BYTES: usize = 1024 * 1024;

/// How a volume-served picture's or PDF's request is going, for the window that asked,
/// so it can show progress over an `<img>` that won't say, and tell a dead server from a
/// damaged file (an `<embed>` says neither). The host carries it to the window: the app
/// as the `viewer-media-load` event (`file_viewer/events.rs`, the same JSON as this).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLoad {
    /// The token the request came in on, so a window ignores news of a closed session.
    pub token: String,
    pub stage: MediaLoadStage,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MediaLoadStage {
    Reading,
    /// The body is on its way to the webview.
    Done,
    /// The volume stopped answering or refused the read.
    Failed,
}

/// Why a read produced no body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MediaReadFailure {
    /// The session closed. Nobody is waiting, so nobody hears of it.
    Closed,
    /// No bytes for the stall limit.
    Stalled,
    /// The volume refused the read (after `VolumeSource`'s one reopen).
    Unreadable,
}

/// A token's bytes on a volume.
pub struct VolumeMedia {
    source: SharedSource,
    total_bytes: u64,
    mime: String,
    /// Set when the token drops: reads in flight stop at their next chunk.
    closed: AtomicBool,
}

impl VolumeMedia {
    pub(super) fn new(source: SharedSource, total_bytes: u64, mime: String) -> Self {
        Self {
            source,
            total_bytes,
            mime,
            closed: AtomicBool::new(false),
        }
    }

    pub(super) fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub(super) fn mime(&self) -> &str {
        &self.mime
    }

    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Blocking. Up to `len` bytes at `start`, a chunk at a time. Fewer only when the file
    /// shrank under the session, like a local read: the caller serves what there is.
    pub(super) fn read(&self, start: u64, len: u64, progress: &ReadProgress) -> Result<Vec<u8>, MediaReadFailure> {
        let len = len as usize;
        let mut body = Vec::with_capacity(len);
        while body.len() < len {
            if self.closed.load(Ordering::SeqCst) {
                return Err(MediaReadFailure::Closed);
            }
            if progress.given_up.load(Ordering::SeqCst) {
                return Err(MediaReadFailure::Stalled);
            }
            let filled = body.len();
            body.resize(filled + (len - filled).min(READ_CHUNK_BYTES), 0);
            let read = self
                .source
                .read_at(start + filled as u64, &mut body[filled..])
                .map_err(|e: ViewerError| {
                    log::debug!("viewer media: a read from the volume failed: {e:?}");
                    MediaReadFailure::Unreadable
                })?;
            body.truncate(filled + read);
            if read == 0 {
                break;
            }
            progress.note(read as u64);
        }
        Ok(body)
    }
}

/// One request's read, shared between the blocking task and its watch.
pub(super) struct ReadProgress {
    done: AtomicU64,
    last_news: Mutex<Instant>,
    given_up: AtomicBool,
}

impl ReadProgress {
    pub(super) fn new() -> Self {
        Self {
            done: AtomicU64::new(0),
            last_news: Mutex::new(rt::now()),
            given_up: AtomicBool::new(false),
        }
    }

    fn note(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::SeqCst);
        *self.last_news.lock_ignore_poison() = rt::now();
    }

    pub(super) fn done(&self) -> u64 {
        self.done.load(Ordering::SeqCst)
    }
}

/// Watches one read for `blocking_typed_result_until_stalled`, reporting through `emit`.
pub(super) struct ReadWatch<F> {
    pub(super) progress: Arc<ReadProgress>,
    pub(super) bytes_total: u64,
    pub(super) token: String,
    pub(super) emit: F,
    pub(super) last_emitted: Option<u64>,
}

impl<F: FnMut(MediaLoad)> StallWatch<MediaReadFailure> for ReadWatch<F> {
    fn idle_for(&self) -> Duration {
        rt::since(*self.progress.last_news.lock_ignore_poison())
    }

    fn on_poll(&mut self) {
        let done = self.progress.done();
        if self.last_emitted != Some(done) {
            (self.emit)(MediaLoad {
                token: self.token.clone(),
                stage: MediaLoadStage::Reading,
                bytes_done: done,
                bytes_total: self.bytes_total,
            });
            self.last_emitted = Some(done);
        }
    }

    fn give_up(&mut self) -> Option<MediaReadFailure> {
        self.progress.given_up.store(true, Ordering::SeqCst);
        Some(MediaReadFailure::Stalled)
    }
}
