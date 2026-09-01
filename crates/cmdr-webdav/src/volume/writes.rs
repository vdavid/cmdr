//! The upload: one streaming PUT to a staging sibling, then a MOVE onto the
//! user's filename.
//!
//! ❗ `Content-Length` is set from `size` rather than sending chunked: Nextcloud
//! and ownCloud (sabre/dav) answer 411 to a chunked PUT. A source that yields a
//! different byte count fails the request, which is reported honestly and the
//! temp removed; it is never MOVEd into place.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::staging::STAGING_TEMP_MARKER;
use cmdr_fs::volume::{VolumeError, VolumeReadStream};
use log::debug;
use reqwest::Method;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio_util::sync::CancellationToken;

use super::WebdavVolume;
use crate::errors::Attempted;
use crate::transport::method;

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
        let stop = CancellationToken::new();
        let source_error: Arc<std::sync::Mutex<Option<VolumeError>>> = Arc::new(std::sync::Mutex::new(None));
        let body = reqwest::Body::wrap_stream(futures_util::stream::unfold(
            (stream, Arc::clone(&written), stop.clone(), Arc::clone(&source_error)),
            |(mut stream, written, stop, source_error)| async move {
                if stop.is_cancelled() {
                    return Some((
                        Err(std::io::Error::other("cancelled")),
                        (stream, written, stop, source_error),
                    ));
                }
                match stream.next_chunk().await? {
                    Ok(chunk) => {
                        written.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        Some((Ok(Bytes::from(chunk)), (stream, written, stop, source_error)))
                    }
                    Err(e) => {
                        let message = e.to_string();
                        *source_error.lock_ignore_poison() = Some(e);
                        Some((
                            Err(std::io::Error::other(message)),
                            (stream, written, stop, source_error),
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

        let put = self.send(request, &temp, Attempted::Reaching);
        let mut put = std::pin::pin!(put);
        let mut tick = tokio::time::interval(PROGRESS_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let outcome = loop {
            tokio::select! {
                outcome = &mut put => break outcome,
                _ = tick.tick() => {
                    if on_progress(written.load(Ordering::Relaxed), size).is_break() {
                        stop.cancel();
                    }
                }
            }
        };

        if let Err(e) = outcome {
            self.remove_best_effort(&temp).await;
            if stop.is_cancelled() {
                return Err(VolumeError::Cancelled(self.volume_id().to_string()));
            }
            if let Some(source) = source_error.lock_ignore_poison().take() {
                return Err(source);
            }
            return Err(e);
        }
        let total = written.load(Ordering::Relaxed);
        if on_progress(total, size).is_break() {
            self.remove_best_effort(&temp).await;
            return Err(VolumeError::Cancelled(self.volume_id().to_string()));
        }

        let request = client
            .request(method("MOVE"), client.url_for(&temp, false))
            .header("Destination", client.url_for(&remote, false).as_str())
            .header("Overwrite", "T");
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
            let request = client.request(Method::DELETE, client.url_for(remote, false));
            let _ = self.send(request, remote, Attempted::Reaching).await;
        }
    }
}
