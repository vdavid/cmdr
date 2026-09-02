//! The read path: one GET per stream, its body pulled a chunk at a time.
//!
//! `Range` asks for a byte window; ❗ a server may answer 200 and ignore it
//! (RFC 9110 § 14.2 makes ranges optional), so a 200 is handled by skipping
//! `offset` bytes locally rather than trusted as a window. No real server has
//! been watched doing it — Nextcloud answers 206 with the exact window
//! (`DETAILS.md` § "What a real server answers") — so the fixture stack carries
//! one that does, `webdav-fixture-norange`, and both the resumed stream and
//! `read_range` are pinned against it.

use std::path::Path;
use std::pin::Pin;

use bytes::Bytes;
use cmdr_fs::volume::{VolumeError, VolumeReadStream};
use futures_util::Stream;
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Method, Response, StatusCode};

use super::WebdavVolume;
use crate::errors::Attempted;
use crate::transport::REQUEST_BUDGET;

type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

/// A GET body as a `VolumeReadStream`.
pub(super) struct WebdavReadStream {
    body: BodyStream,
    total: u64,
    read: u64,
    /// Bytes still to discard, for a 200 answer to a ranged request.
    skip: u64,
    volume_id: String,
    path: String,
}

impl VolumeReadStream for WebdavReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            loop {
                // The idle budget between chunks, ❗ per chunk and never for the
                // whole body: a multi-GB download has no total ceiling.
                let next = match tokio::time::timeout(REQUEST_BUDGET, self.body.next()).await {
                    Ok(next) => next?,
                    Err(_elapsed) => return Some(Err(VolumeError::ConnectionTimeout(self.path.clone()))),
                };
                let chunk = match next {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        return Some(Err(crate::errors::map_transport_error(&e, &self.volume_id, &self.path)));
                    }
                };
                if self.skip > 0 {
                    let drop = self.skip.min(chunk.len() as u64);
                    self.skip -= drop;
                    if drop as usize == chunk.len() {
                        continue;
                    }
                    self.read += (chunk.len() - drop as usize) as u64;
                    return Some(Ok(chunk[drop as usize..].to_vec()));
                }
                self.read += chunk.len() as u64;
                return Some(Ok(chunk.to_vec()));
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.total
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }
}

/// The file's full length from a 206's `Content-Range: bytes a-b/total`.
fn content_range_total(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_RANGE)?
        .to_str()
        .ok()?
        .rsplit('/')
        .next()?
        .parse()
        .ok()
}

impl WebdavVolume {
    /// A GET, from `offset`, in whatever chunks the body arrives in.
    pub(super) async fn open_read_stream_impl(
        &self,
        path: &Path,
        offset: u64,
    ) -> Result<WebdavReadStream, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let mut request = client.request(Method::GET, client.url_for(&remote, false));
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = self.send(request, &remote, Attempted::Reaching).await?;
        let (total, skip) = match response.status() {
            StatusCode::PARTIAL_CONTENT => (
                content_range_total(&response).unwrap_or(response.content_length().unwrap_or(0) + offset),
                0,
            ),
            _ => (response.content_length().unwrap_or(0), offset),
        };
        Ok(WebdavReadStream {
            body: Box::pin(response.bytes_stream()),
            total,
            read: 0,
            skip,
            volume_id: self.volume_id().to_string(),
            path: remote,
        })
    }

    /// A GET with `Range`, collected up to `len` bytes and no further: the
    /// response is dropped as soon as the window is full, so a server that
    /// ignored the header doesn't send the whole file.
    pub(super) async fn read_range_impl(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        let remote = self.to_remote_path(path)?;
        if len == 0 {
            return Ok(Vec::new());
        }
        let client = self.clone_client().await?;
        let end = offset.saturating_add(len as u64).saturating_sub(1);
        let request = client
            .request(Method::GET, client.url_for(&remote, false))
            .header(RANGE, format!("bytes={offset}-{end}"));
        // Judged by the typed status before the table, ❌ never by message:
        // 416 (past the end) is an empty read, the same as a local file answers.
        let response = request
            .send()
            .await
            .map_err(|e| crate::errors::map_transport_error(&e, self.volume_id(), &remote))?;
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(crate::errors::map_status(
                response.status(),
                &remote,
                Attempted::Reaching,
            ));
        }
        let mut stream = WebdavReadStream {
            skip: if response.status() == StatusCode::PARTIAL_CONTENT {
                0
            } else {
                offset
            },
            total: 0,
            read: 0,
            volume_id: self.volume_id().to_string(),
            path: remote,
            body: Box::pin(response.bytes_stream()),
        };
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let Some(chunk) = stream.next_chunk().await else {
                break;
            };
            let chunk = chunk?;
            let take = (len - out.len()).min(chunk.len());
            out.extend_from_slice(&chunk[..take]);
        }
        Ok(out)
    }
}
