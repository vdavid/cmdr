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
//! MOVEd into place. ❗ With one shape still open, and it is a silent
//! truncation: `DETAILS.md` § "Write staging" has it and the fix.

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

        let written = Arc::new(AtomicU64::new(0));
        // Whether the source reached its own end, which tells a short source
        // (its fault) from a connection cut mid-body (the server's).
        let source_ended = Arc::new(AtomicBool::new(false));
        let stop = CancellationToken::new();
        let source_error: Arc<std::sync::Mutex<Option<VolumeError>>> = Arc::new(std::sync::Mutex::new(None));
        let body = reqwest::Body::wrap_stream(futures_util::stream::unfold(
            (
                stream,
                Arc::clone(&written),
                stop.clone(),
                Arc::clone(&source_error),
                Arc::clone(&source_ended),
            ),
            |(mut stream, written, stop, source_error, source_ended)| async move {
                if stop.is_cancelled() {
                    return Some((
                        Err(std::io::Error::other("cancelled")),
                        (stream, written, stop, source_error, source_ended),
                    ));
                }
                let Some(next) = stream.next_chunk().await else {
                    source_ended.store(true, Ordering::Relaxed);
                    return None;
                };
                match next {
                    Ok(chunk) => {
                        written.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        Some((
                            Ok(Bytes::from(chunk)),
                            (stream, written, stop, source_error, source_ended),
                        ))
                    }
                    Err(e) => {
                        let message = e.to_string();
                        *source_error.lock_ignore_poison() = Some(e);
                        Some((
                            Err(std::io::Error::other(message)),
                            (stream, written, stop, source_error, source_ended),
                        ))
                    }
                }
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
                        if on_progress(written.load(Ordering::Relaxed), size).is_break() {
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

        let total = written.load(Ordering::Relaxed);
        if let Err(e) = outcome {
            self.remove_best_effort(&temp).await;
            if stop.is_cancelled() {
                return Err(VolumeError::Cancelled(self.volume_id().to_string()));
            }
            if let Some(source) = source_error.lock_ignore_poison().take() {
                return Err(source);
            }
            if source_ended.load(Ordering::Relaxed) && total != size {
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
            // server happily stores the prefix. Verified on hyper 1.10.1
            // (`h1/encode.rs`, `Kind::Length`), 2026-09-01. Never MOVE that.
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
