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

use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::volume::{StreamLength, VolumeError, VolumeReadStream};
use http::StatusCode;

use super::S3Volume;
use super::errors::map_s3_error;
use super::paths::{Holder, Resolved, Target, target_of};
use super::query::{carried_from_headers, stored_mtime};
use crate::error::S3Error;
use crate::metadata::{BTIME_HEADER, MODE_HEADER, MTIME_HEADER};
use crate::ops::{self, ByteRange};
use crate::transport::{Opened, REQUEST_BUDGET};

/// What a GET's status and headers say about its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answered {
    /// The body is the object from some point on: `skip` bytes to drop first
    /// (a 200 to a ranged request), and the object's full length when the
    /// answer names it: `None` when it doesn't (a proxy that stripped
    /// `content-length`), ❌ never a guessed `0`.
    Window { total: Option<u64>, skip: u64 },
    /// 416: the read starts at or past the end. Nothing to read.
    PastTheEnd { total: Option<u64> },
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
            total: range_total.or_else(|| content_length.map(|length| length + offset)),
            skip: 0,
        },
        StatusCode::RANGE_NOT_SATISFIABLE => Answered::PastTheEnd { total: range_total },
        status if status.is_success() => Answered::Window {
            total: content_length,
            skip: offset,
        },
        _ => Answered::Refused,
    }
}

/// A GET body as a `VolumeReadStream`.
pub(super) struct S3ReadStream {
    /// `None` for a read that starts past the end: there's no body to pull.
    body: Option<Opened>,
    /// The object's full length as the SERVER named it. A body that ends
    /// before reaching it was cut off, and fails rather than ending cleanly.
    total: Option<u64>,
    /// The caller's idea of the length (`open_read_stream_with_hint`), what
    /// the stream declares when the server named none.
    size_hint: Option<u64>,
    /// Where in the object the read starts.
    offset: u64,
    read: u64,
    /// Bytes still to discard, for a 200 answer to a ranged request.
    skip: u64,
    path: String,
    /// The object's date off the GET's own headers, so a copy's destination
    /// keeps it (`VolumeReadStream::modified_at`).
    modified_at: Option<SystemTime>,
    /// The creation date and mode off the same headers (rclone's `btime` and
    /// `mode`), when a copy put them there.
    carried: CarriedMetadata,
}

impl S3ReadStream {
    /// Declares `size_hint` when the server named no length.
    pub(super) fn hinted(mut self, size_hint: Option<u64>) -> Self {
        self.size_hint = size_hint;
        self
    }

    /// Why a body that just ended is a cut rather than the end of the object:
    /// it stopped short of the length the server named. `None` for a whole
    /// body, or one whose length nobody named.
    fn ended_short(&self) -> Option<VolumeError> {
        let total = self.total?;
        let reached = self.offset + self.read;
        (reached < total).then(|| VolumeError::IoError {
            message: format!(
                "{}: the server named {} and the body ended after {}",
                self.path,
                pluralize_grouped(total, "byte"),
                pluralize_grouped(reached, "byte")
            )
            .into(),
            raw_os_error: None,
        })
    }
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
                    Ok(None) => return self.ended_short().map(Err),
                    Err(_elapsed) => return Some(Err(VolumeError::ConnectionTimeout(self.path.clone().into()))),
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
        match self.total.or(self.size_hint) {
            Some(total) => StreamLength::Known(total),
            None => StreamLength::Unknown,
        }
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<SystemTime> {
        self.modified_at
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.carried.clone().restricted_to(wanted)
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
        let carried = carried_from_headers(opened.header(BTIME_HEADER), opened.header(MODE_HEADER));
        match answered {
            Answered::Window { total, skip } => Ok(S3ReadStream {
                body: Some(opened),
                total,
                size_hint: None,
                offset,
                read: 0,
                skip,
                path: remote,
                modified_at,
                carried,
            }),
            Answered::PastTheEnd { total } => Ok(S3ReadStream {
                body: None,
                total,
                size_hint: None,
                offset,
                read: 0,
                skip: 0,
                path: remote,
                modified_at,
                carried,
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
        // No `total`: the window ends where the caller stops pulling, well
        // before the object does.
        let mut stream = S3ReadStream {
            body: Some(opened),
            total: None,
            size_hint: None,
            offset,
            read: 0,
            skip,
            path: remote,
            modified_at: None,
            carried: CarriedMetadata::default(),
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
