//! The read path: one GET per stream, its body pulled a chunk at a time, ❌
//! never buffered whole.
//!
//! A read from an offset asks for `Range: bytes=<offset>-`. ❗ A server may
//! answer 200 and ignore it (RFC 9110 § 14.2 makes ranges optional), so a 200 is
//! handled by skipping `offset` bytes locally rather than trusted as a window,
//! the way `crates/cmdr-webdav/src/volume/streams.rs` does. Both fixtures answer
//! 206 with the exact window; [`judge_get`]'s unit cells pin the 200 case.

use std::path::Path;
use std::pin::Pin;
use std::time::SystemTime;

use cmdr_fs::volume::{StreamLength, VolumeError, VolumeReadStream};
use http::StatusCode;

use super::S3Volume;
use super::errors::map_s3_error;
use super::paths::{Holder, Resolved, Target, target_of};
use super::query::stored_mtime;
use crate::error::S3Error;
use crate::metadata::MTIME_HEADER;
use crate::ops::{self, ByteRange};
use crate::transport::{Opened, REQUEST_BUDGET};

/// What a GET's status and headers say about its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answered {
    /// The body is the object from some point on: `skip` bytes to drop first
    /// (a 200 to a ranged request), and the object's full length.
    Window { total: u64, skip: u64 },
    /// 416: the read starts at or past the end. Nothing to read.
    PastTheEnd { total: u64 },
    /// Anything else: the body is an S3 `<Error>`.
    Refused,
}

/// Reads a GET's answer to a request from `offset`.
///
/// A 206's `Content-Range: bytes a-b/total` carries the full length; a 200
/// carries it as `Content-Length`; a 416 carries it as `bytes */total`.
pub(super) fn judge_get(
    status: StatusCode,
    content_range: Option<&str>,
    content_length: Option<u64>,
    offset: u64,
) -> Answered {
    let range_total = content_range.and_then(|range| range.rsplit('/').next()?.trim().parse::<u64>().ok());
    match status {
        StatusCode::PARTIAL_CONTENT => Answered::Window {
            total: range_total.unwrap_or_else(|| content_length.unwrap_or(0) + offset),
            skip: 0,
        },
        StatusCode::RANGE_NOT_SATISFIABLE => Answered::PastTheEnd {
            total: range_total.unwrap_or(0),
        },
        status if status.is_success() => Answered::Window {
            total: content_length.unwrap_or(0),
            skip: offset,
        },
        _ => Answered::Refused,
    }
}

/// A GET body as a `VolumeReadStream`.
pub(super) struct S3ReadStream {
    /// `None` for a read that starts past the end: there's no body to pull.
    body: Option<Opened>,
    total: u64,
    read: u64,
    /// Bytes still to discard, for a 200 answer to a ranged request.
    skip: u64,
    path: String,
    /// The object's date off the GET's own headers, so a copy's destination
    /// keeps it (`VolumeReadStream::modified_at`).
    modified_at: Option<SystemTime>,
}

impl VolumeReadStream for S3ReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let body = self.body.as_mut()?;
            loop {
                // The idle budget between chunks, ❗ per chunk and never for
                // the whole body: a multi-GB download has no total ceiling.
                let chunk = match tokio::time::timeout(REQUEST_BUDGET, body.chunk()).await {
                    Ok(Some(Ok(chunk))) => chunk,
                    Ok(Some(Err(e))) => return Some(Err(e)),
                    Ok(None) => return None,
                    Err(_elapsed) => return Some(Err(VolumeError::ConnectionTimeout(self.path.clone()))),
                };
                if self.skip > 0 {
                    let drop = usize::try_from(self.skip).unwrap_or(usize::MAX).min(chunk.len());
                    self.skip -= drop as u64;
                    if drop == chunk.len() {
                        continue;
                    }
                    self.read += (chunk.len() - drop) as u64;
                    return Some(Ok(chunk[drop..].to_vec()));
                }
                self.read += chunk.len() as u64;
                return Some(Ok(chunk));
            }
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total)
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<SystemTime> {
        self.modified_at
    }
}

impl S3Volume {
    /// A GET from `offset`, in whatever chunks the body arrives in.
    pub(super) async fn open_read_stream_impl(&self, path: &Path, offset: u64) -> Result<S3ReadStream, VolumeError> {
        let range = (offset > 0).then_some(ByteRange {
            start: offset,
            end: None,
        });
        let (opened, remote) = self.get(path, range).await?;
        let answered = judge_get(
            opened.status,
            opened.header("content-range"),
            opened.object_length(),
            offset,
        );
        let modified_at = stored_mtime(opened.header(MTIME_HEADER), opened.header("last-modified"));
        match answered {
            Answered::Window { total, skip } => Ok(S3ReadStream {
                body: Some(opened),
                total,
                read: 0,
                skip,
                path: remote,
                modified_at,
            }),
            Answered::PastTheEnd { total } => Ok(S3ReadStream {
                body: None,
                total,
                read: 0,
                skip: 0,
                path: remote,
                modified_at,
            }),
            Answered::Refused => Err(refusal(opened, &remote).await),
        }
    }

    /// A GET for `[offset, offset + len)`, collected up to `len` bytes and no
    /// further: the response is dropped as soon as the window is full, so a
    /// server that ignored the range doesn't send the whole object.
    pub(super) async fn read_range_impl(&self, path: &Path, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
        if len == 0 {
            // Still a path that has to be on this place.
            self.to_remote_path(path)?;
            return Ok(Vec::new());
        }
        let end = offset.saturating_add(len as u64).saturating_sub(1);
        let range = ByteRange {
            start: offset,
            end: Some(end),
        };
        let (opened, remote) = self.get(path, Some(range)).await?;
        let answered = judge_get(
            opened.status,
            opened.header("content-range"),
            opened.object_length(),
            offset,
        );
        let skip = match answered {
            Answered::Window { skip, .. } => skip,
            // Past the end is an empty read, the same as a local file answers.
            Answered::PastTheEnd { .. } => return Ok(Vec::new()),
            Answered::Refused => return Err(refusal(opened, &remote).await),
        };
        let mut stream = S3ReadStream {
            body: Some(opened),
            total: 0,
            read: 0,
            skip,
            path: remote,
            modified_at: None,
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

    /// Sends a GET for the object at `path`, answering with its body still on
    /// the wire and the server-side path errors name. The account root and a
    /// bucket's top are folders.
    /// The folder row beside a `<name> (file)` row is a folder too.
    async fn get(&self, path: &Path, range: Option<ByteRange>) -> Result<(Opened, String), VolumeError> {
        let Resolved { remote, holder } = self.resolve(path)?;
        let Target::Key { bucket, key } = target_of(&remote) else {
            return Err(VolumeError::IsADirectory(remote));
        };
        if holder == Holder::Folder {
            return Err(VolumeError::IsADirectory(remote));
        }
        let client = self.clone_client().await?;
        let request =
            ops::get_object(client.profile(), bucket, key, range).map_err(|_| VolumeError::NotFound(remote.clone()))?;
        let opened = client.open(request, self.volume_id(), &remote).await?;
        Ok((opened, remote))
    }
}

/// A refused GET in the `Volume` vocabulary, from its `<Error>` body.
async fn refusal(opened: Opened, remote: &str) -> VolumeError {
    let status = opened.status;
    let body = opened.text(REQUEST_BUDGET).await;
    map_s3_error(&S3Error::from_response(status, &body), remote)
}

#[cfg(test)]
#[path = "streams_test.rs"]
mod streams_test;
