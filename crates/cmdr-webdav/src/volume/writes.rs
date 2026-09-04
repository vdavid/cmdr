//! The upload: one streaming PUT to a staging sibling, then a MOVE onto the
//! user's filename.
//!
//! ❗ `Content-Length` is set from `size` rather than sending a body of unknown
//! length. The size is always known here, so the header costs nothing and takes
//! away every disagreement a proxy or a server configuration could have about a
//! chunked request. (A real Nextcloud accepts a chunked PUT rather than
//! answering 411; `DETAILS.md` § "What a real server answers" carries the
//! observation and its date.) A source that yields a different byte count fails
//! the request, which is reported honestly and the temp removed; it is never
//! MOVEd into place.
//!
//! ❗ **The body reads one piece AHEAD**, and that is not a buffering trick: it
//! is what makes the sentence above true. hyper stops POLLING a body the moment
//! `Content-Length` is satisfied, so a source that still had bytes at that
//! moment is invisible unless it was asked before its answer was needed.
//! `DETAILS.md` § "Write staging" carries the two counters this costs and why
//! they mean different things.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::staging::STAGING_TEMP_MARKER;
use cmdr_fs::volume::{VolumeError, VolumeReadStream};
use log::debug;
use reqwest::Method;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio_util::sync::CancellationToken;

use super::WebdavVolume;
use crate::errors::Attempted;
use crate::transport::{MUTATION_BUDGET, method};

/// How often the upload reports progress while the body is on its way.
const PROGRESS_TICK: Duration = Duration::from_millis(200);

/// A staging name nothing else will pick: the destination plus the marker
/// every backend's leftover sweep recognizes, plus a per-process counter.
fn staging_sibling(remote: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(
        "{remote}{STAGING_TEMP_MARKER}{:x}{nanos:x}{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// What the body stream counts, and ❗ these are two different numbers on
/// purpose: the read-ahead means one piece can be out of the source but not yet
/// handed to hyper, and each question wants its own answer.
#[derive(Clone)]
struct BodyCounts {
    /// Bytes pulled OUT of the source. The size guard's number, and the one
    /// `write_from_stream` returns: only this one can see a piece hyper never
    /// asked for, which is the whole point of reading ahead.
    fetched: Arc<AtomicU64>,
    /// Bytes handed TO the body. What progress reports, because it is the same
    /// quantity the progress bar has always shown: a piece counted when it goes
    /// out, never when it is merely in hand. ❗ Reporting `fetched` instead would
    /// run the bar one piece ahead of the wire and invite a user to cancel a
    /// transfer that looked finished.
    handed: Arc<AtomicU64>,
    /// Whether the source reached its OWN end, which tells a short source (its
    /// fault) from a connection cut mid-body (the server's).
    ///
    /// ❗ With the read-ahead this can be true while a piece is still waiting to
    /// go out, which changes nothing: it is only ever read once the request is
    /// over.
    ended: Arc<AtomicBool>,
}

impl BodyCounts {
    fn new() -> Self {
        Self {
            fetched: Arc::new(AtomicU64::new(0)),
            handed: Arc::new(AtomicU64::new(0)),
            ended: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// The body stream's state: the source, the piece already pulled out of it, and
/// the counters both the guard and the progress bar read.
struct BodySource {
    stream: Box<dyn VolumeReadStream>,
    /// The next piece, already in hand. `None` means either "nothing pulled yet"
    /// (`primed` is false) or "the source is done" (it is true).
    pending: Option<Vec<u8>>,
    primed: bool,
    counts: BodyCounts,
    stop: CancellationToken,
    source_error: Arc<std::sync::Mutex<Option<VolumeError>>>,
}

impl BodySource {
    /// Pulls one piece into `pending`, counting it as fetched the moment it is
    /// out of the source rather than when it goes on the wire.
    ///
    /// A source failure is recorded for `write_from_stream` to report and
    /// handed back as the `io::Error` that poisons the body.
    async fn fetch(&mut self) -> Result<(), std::io::Error> {
        match self.stream.next_chunk().await {
            None => {
                self.counts.ended.store(true, Ordering::Relaxed);
                self.pending = None;
            }
            Some(Ok(chunk)) => {
                self.counts.fetched.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                self.pending = Some(chunk);
            }
            Some(Err(e)) => {
                let message = e.to_string();
                *self.source_error.lock_ignore_poison() = Some(e);
                return Err(std::io::Error::other(message));
            }
        }
        Ok(())
    }
}

/// The source yielded a different byte count than `size` promised.
fn size_mismatch(remote: &str, got: u64, size: u64) -> VolumeError {
    VolumeError::IoError {
        message: format!(
            "{remote}: the source promised {} and yielded {}",
            pluralize_grouped(size, "byte"),
            pluralize_grouped(got, "byte")
        ),
        raw_os_error: None,
    }
}

impl WebdavVolume {
    /// Streams `stream` into a `.cmdr-tmp-*` sibling of `dest` and moves it into
    /// place. Returns the bytes written.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let remote = self.to_remote_path(dest)?;
        let client = self.clone_client().await?;
        let temp = staging_sibling(&remote);
        debug!("WebdavVolume::write_from_stream: {remote} via {temp}");

        let counts = BodyCounts::new();
        let stop = CancellationToken::new();
        let source_error: Arc<std::sync::Mutex<Option<VolumeError>>> = Arc::new(std::sync::Mutex::new(None));
        let body = reqwest::Body::wrap_stream(futures_util::stream::unfold(
            BodySource {
                stream,
                pending: None,
                primed: false,
                counts: counts.clone(),
                stop: stop.clone(),
                source_error: Arc::clone(&source_error),
            },
            |mut source| async move {
                // Cancellation is answered before anything is pulled or handed
                // over, so a cancelled upload never puts one more byte on the
                // wire. The piece already read ahead is simply dropped with the
                // state: it was never sent, and the temp goes either way.
                if source.stop.is_cancelled() {
                    return Some((Err(std::io::Error::other("cancelled")), source));
                }
                if !source.primed {
                    source.primed = true;
                    if let Err(failed) = source.fetch().await {
                        return Some((Err(failed), source));
                    }
                }
                // Nothing in hand after priming means the source is done, which
                // ends the body.
                let chunk = source.pending.take()?;
                // ❗ The read-ahead, and the ONE line the over-long guard rests
                // on: ask the source for its next piece BEFORE handing this one
                // over, so "the source still had bytes" is on record whether or
                // not hyper ever polls again.
                if let Err(failed) = source.fetch().await {
                    return Some((Err(failed), source));
                }
                source.counts.handed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                Some((Ok(Bytes::from(chunk)), source))
            },
        ));
        let request = client
            .request(Method::PUT, client.url_for(&temp, false))
            .header(CONTENT_LENGTH, size)
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body);

        // The block scopes the in-flight request: leaving it drops the
        // request, which is what aborts a cancelled upload on the wire.
        let outcome = {
            let put = self.send(request, &temp, Attempted::Reaching);
            let mut put = std::pin::pin!(put);
            let mut tick = tokio::time::interval(PROGRESS_TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    outcome = &mut put => break outcome,
                    _ = tick.tick() => {
                        // ❗ `handed`, and clamped: the bar tracks what went out,
                        // and an over-long source must not push it past 100% on
                        // its way to being refused.
                        let sent = counts.handed.load(Ordering::Relaxed).min(size);
                        if on_progress(sent, size).is_break() {
                            // ❗ Break out NOW rather than wait for the body to be
                            // polled again: a server that stalled mid-upload would
                            // otherwise hold the cancel until it asks for more.
                            stop.cancel();
                            break Err(VolumeError::Cancelled(self.volume_id().to_string()));
                        }
                    }
                }
            }
        };

        // ❗ What came OUT of the source, never what hyper asked for. The two
        // differ by exactly the case this guard exists for.
        let total = counts.fetched.load(Ordering::Relaxed);
        if let Err(e) = outcome {
            self.remove_best_effort(&temp).await;
            if stop.is_cancelled() {
                return Err(VolumeError::Cancelled(self.volume_id().to_string()));
            }
            if let Some(source) = source_error.lock_ignore_poison().take() {
                return Err(source);
            }
            if counts.ended.load(Ordering::Relaxed) && total != size {
                // ❗ A short source ends the request from OUR side (hyper's
                // `NotEof`), which `reqwest` reports as a request error, the
                // same predicate a dropped connection answers. ❌ Not the
                // volume's fault, so not `DeviceDisconnected`: that would flip
                // the volume offline over one wrong `size`.
                return Err(size_mismatch(&remote, total, size));
            }
            return Err(e);
        }
        if total != size {
            // ❗ hyper TRUNCATES a body longer than `Content-Length` and the
            // server happily stores the prefix, then answers 201 with nothing
            // wrong anywhere on the wire. Verified on hyper 1.10.1 (its HTTP/1
            // encoder's `Kind::Length` arm), 2026-09-01. Never MOVE that.
            //
            // ❗ It also stops POLLING once the promise is met, which is why
            // `total` is the read-ahead's count: a source whose pieces divide
            // `size` exactly would otherwise agree with the promise and land a
            // truncated file on the user's name.
            self.remove_best_effort(&temp).await;
            return Err(size_mismatch(&remote, total, size));
        }
        if on_progress(total, size).is_break() {
            self.remove_best_effort(&temp).await;
            return Err(VolumeError::Cancelled(self.volume_id().to_string()));
        }

        let request = client
            .request(method("MOVE"), client.url_for(&temp, false))
            .header("Destination", client.url_for(&remote, false).as_str())
            .header("Overwrite", "T")
            .timeout(MUTATION_BUDGET);
        if let Err(e) = self.send(request, &remote, Attempted::Reaching).await {
            self.remove_best_effort(&temp).await;
            return Err(e);
        }
        Ok(total)
    }

    /// Removes a staging temp, and says nothing if that fails: the error that
    /// got us here is the one worth reporting.
    pub(super) async fn remove_best_effort(&self, remote: &str) {
        if let Ok(client) = self.clone_client().await {
            let request = client
                .request(Method::DELETE, client.url_for(remote, false))
                .timeout(MUTATION_BUDGET);
            let _ = self.send(request, remote, Attempted::Reaching).await;
        }
    }
}
