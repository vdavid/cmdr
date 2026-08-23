//! The write window: what makes an upload over a 50 ms link cost more than one
//! chunk per round trip.
//!
//! Same shape as the read window next door, and for the same reason: a
//! sequential SFTP write is one request, one round trip, one chunk. What differs
//! is the ceiling. Raising `russh`'s channel window buys reads real depth;
//! writes are governed by the SERVER's window, which OpenSSH fixes at 2 MiB, so
//! depth is the only lever there is.
//!
//! Three rules hold the module up:
//!
//! - **Every write carries its own offset** ([`RemoteWrite::write_at`]), the same
//!   way every read does. `File::write` keeps an honest offset of its own (it
//!   advances by what it WROTE, unlike the read side), but naming the offset is
//!   what lets N clones of one handle write different parts of the file at once.
//! - **A short write is ordinary.** The engine clamps every request to the
//!   server's negotiated limit, so a chunk takes as many requests as the server
//!   makes it take.
//! - ❗ **The close is awaited, and only the last clone may await it.** Dropping a
//!   `File` fires `SSH_FXP_CLOSE` on a detached task and DISCARDS the answer; for a
//!   staged write the close is where a server reports bytes it accepted but could
//!   not commit. A surviving clone silently downgrades that close to a no-op; see
//!   [`RemoteWrite::close`] for what has to stay true.
//!
//! `DETAILS.md` § "The write window" carries the measurements and the depth they
//! set.

use std::cmp::min;
use std::io::SeekFrom;
use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::{VolumeError, VolumeReadStream};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use log::debug;
use openssh_sftp_client::file::File;
use tokio::io::AsyncSeek;

use super::SftpVolume;
use crate::errors::map_sftp_error;

/// How many bytes one write request carries.
///
/// ❗ 255 KiB because that is what OpenSSH's `sftp-server` answers
/// `limits@openssh.com` with, so a chunk is one request. ⚠️ The engine's own
/// negotiated number is behind its `__ci-tests` feature and can't be read, so a
/// server with stingier limits splits each chunk internally instead: correct,
/// and narrower in practice than the depth below says.
pub(super) const WRITE_CHUNK_BYTES: usize = 255 * 1024;

/// How many chunk writes one upload keeps in flight.
///
/// Set from the curve in `DETAILS.md`. ⚠️ Raising the channel window does
/// nothing here — the server's window governs an upload, and OpenSSH fixes it at
/// 2 MiB — so this number is the whole of the write side's tuning.
pub(super) const WRITE_WINDOW_DEPTH: usize = 8;

/// One positioned write against an open remote file.
///
/// The seam the window is written against, so its offset bookkeeping is testable
/// without a server: a double can short-write and complete out of order where a
/// real one only sometimes does.
pub(super) trait PositionedWrite: Clone + Send + 'static {
    /// Writes as much of `bytes` at `offset` as the server takes in one request,
    /// and answers with how much that was.
    ///
    /// Fewer bytes than asked is legal and ordinary. Zero is not: a server that
    /// accepts nothing would spin a caller forever, so callers treat it as a
    /// failure rather than as progress.
    fn write_at(&mut self, offset: u64, bytes: &[u8]) -> impl Future<Output = Result<usize, VolumeError>> + Send;
}

/// An open remote file, written one position at a time.
///
/// Cloning shares the remote handle through an `Arc`, so N clones each writing
/// their own part give depth N and cost no extra `SSH_FXP_OPEN`.
#[derive(Clone)]
pub(super) struct RemoteWrite {
    file: File,
    /// The remote path this handle was opened at, carried so a failure on it can
    /// answer with the path its `VolumeError` variant is defined to carry.
    /// `Arc<str>` because a clone rides along with every in-flight write.
    remote: Arc<str>,
}

impl RemoteWrite {
    pub(super) fn new(file: File, remote: Arc<str>) -> Self {
        Self { file, remote }
    }

    /// Closes the handle and reports what the server said.
    ///
    /// ❗ The whole reason this exists rather than a drop: a drop sends the same
    /// `SSH_FXP_CLOSE` on a detached task and throws the answer away, and the
    /// close is the last chance a server has to say it couldn't commit the bytes
    /// it accepted.
    ///
    /// ❗ **Only the LAST clone may call this.** The engine's `OwnedHandle::close`
    /// sends `SSH_FXP_CLOSE` only while `Arc::strong_count(&handle) == 1`, and returns
    /// `Ok(())` in silence otherwise (verified by reading `openssh-sftp-client` 0.15.7,
    /// 2026-08-23). A clone still alive here turns the awaited close into a no-op that
    /// reports success on bytes the server never committed, and neither the compiler
    /// nor a test says a word. What keeps it true today: [`SftpVolume::pump`] takes its
    /// `RemoteWrite` BY VALUE and each in-flight write owns its own clone, so all of
    /// them are gone before `write_from_stream` closes. Anything new that holds a clone
    /// must drop it before the close.
    pub(super) async fn close(self) -> Result<(), VolumeError> {
        self.file.close().await.map_err(|e| map_sftp_error(&e, &self.remote))
    }
}

impl PositionedWrite for RemoteWrite {
    async fn write_at(&mut self, offset: u64, bytes: &[u8]) -> Result<usize, VolumeError> {
        Pin::new(&mut self.file)
            .start_seek(SeekFrom::Start(offset))
            .map_err(|e| VolumeError::IoError {
                message: e.to_string(),
                raw_os_error: e.raw_os_error(),
            })?;
        self.file.write(bytes).await.map_err(|e| map_sftp_error(&e, &self.remote))
    }
}

/// Writes the whole of `bytes` starting at `offset`, taking as many round trips
/// as the server makes it take.
pub(super) async fn write_all_at<W: PositionedWrite>(
    writer: &mut W,
    offset: u64,
    bytes: &[u8],
) -> Result<u64, VolumeError> {
    let mut done = 0usize;
    while done < bytes.len() {
        let took = writer.write_at(offset + done as u64, &bytes[done..]).await?;
        if took == 0 {
            // ❗ Not a short write to loop on: a server that takes nothing takes
            // nothing again, and treating it as progress would hang the upload
            // instead of failing it.
            return Err(VolumeError::IoError {
                message: format!("the server accepted none of a {}-byte write", bytes.len() - done),
                raw_os_error: None,
            });
        }
        done += took;
    }
    Ok(done as u64)
}

/// Pulls `chunk` bytes out of `stream`, coalescing what the source hands over.
///
/// A source's chunk size is its own business (SMB pipelines ~512 KB, a local
/// read may hand over far less), and the window wants pieces the server takes in
/// one request. Answers short only at end of stream.
async fn take_chunk(
    stream: &mut Box<dyn VolumeReadStream>,
    pending: &mut Vec<u8>,
    chunk: usize,
) -> Result<Option<Vec<u8>>, VolumeError> {
    while pending.len() < chunk {
        match stream.next_chunk().await {
            None => break,
            Some(Ok(bytes)) => pending.extend_from_slice(&bytes),
            Some(Err(e)) => return Err(e),
        }
    }
    if pending.is_empty() {
        return Ok(None);
    }
    let take = min(chunk, pending.len());
    Ok(Some(pending.drain(..take).collect()))
}

impl SftpVolume {
    /// Streams `stream` onto `dest`, `WRITE_WINDOW_DEPTH` chunks in flight.
    ///
    /// ❗ `dest` is always a `.cmdr-tmp-*` sibling, never the user's filename:
    /// [`cmdr_fs::volume::Volume::write_is_single_shot`] keeps its `false`
    /// default here, so the transfer layer stages every write to this backend and
    /// nothing half-written ever wears a real name. ❌ Which is also why there is
    /// no "the create landed but the write didn't" classifier like SMB's — that
    /// one exists because SMB's compound path SKIPS staging.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        self.upload(dest, size, stream, WRITE_WINDOW_DEPTH, on_progress).await
    }

    /// The same, at a depth the caller picks. The measurement harness is the
    /// only thing that picks a different one; production takes the constant.
    pub(super) async fn upload(
        &self,
        dest: &Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        depth: usize,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let remote = self.to_remote_path(dest)?;
        // ❗ Cloned out from under a short read guard, and the guard released
        // before a byte moves. An upload that held it would serialize every other
        // operation on the one channel, which is exactly the concurrency the
        // channel exists to provide.
        let session = self.clone_session().await?;
        debug!("SftpVolume::write_from_stream: {remote}, size={size}");

        // Truncating rather than exclusive: this is a staging name, and a retried
        // attempt writes onto whatever its predecessor left there.
        let file = session
            .sftp()
            .options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&remote)
            .await
            .map_err(|e| map_sftp_error(&e, &remote))?;
        let writer = RemoteWrite::new(file, Arc::from(remote.as_str()));

        match self
            .pump(&mut stream, writer.clone(), &remote, size, depth, on_progress)
            .await
        {
            Ok(written) => {
                // ❗ The close is part of the write, not tidying after it.
                if let Err(e) = writer.close().await {
                    self.remove_partial(&session, &remote).await;
                    return Err(e);
                }
                self.notify_created(dest).await;
                Ok(written)
            }
            Err(e) => {
                // ❗ Every error path takes the partial away. Closing first so the
                // server isn't asked to remove a path it still holds open.
                let _ = writer.close().await;
                self.remove_partial(&session, &remote).await;
                Err(e)
            }
        }
    }

    /// The window itself: chunks out of the source, `depth` writes in flight,
    /// progress reported as they land.
    ///
    /// ❗ Takes `writer` BY VALUE on purpose: it and every in-flight clone die with
    /// this call, which is the precondition [`RemoteWrite::close`] needs to reach the
    /// wire at all.
    async fn pump(
        &self,
        stream: &mut Box<dyn VolumeReadStream>,
        writer: RemoteWrite,
        remote: &str,
        size: u64,
        depth: usize,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        #[allow(
            clippy::type_complexity,
            reason = "boxing the write future is what makes the window a nameable type; one allocation per 255 KiB"
        )]
        let mut in_flight: FuturesUnordered<Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send>>> =
            FuturesUnordered::new();
        let mut pending: Vec<u8> = Vec::new();
        let mut next_offset = 0u64;
        let mut written = 0u64;
        let mut source_done = false;

        loop {
            while in_flight.len() < depth.max(1) && !source_done {
                match take_chunk(stream, &mut pending, WRITE_CHUNK_BYTES).await? {
                    None => source_done = true,
                    Some(bytes) => {
                        let offset = next_offset;
                        next_offset += bytes.len() as u64;
                        let mut writer = writer.clone();
                        in_flight.push(Box::pin(async move { write_all_at(&mut writer, offset, &bytes).await }));
                    }
                }
            }

            let Some(landed) = in_flight.next().await else {
                return Ok(written);
            };
            written += landed?;
            // ❗ Cancellation arrives ONLY here. There is no token on this path:
            // the transfer engine says stop by answering `Break`, and a backend
            // that never called back would be uncancelable.
            if on_progress(written, size).is_break() {
                return Err(VolumeError::Cancelled(remote.to_string()));
            }
        }
    }

    /// Best effort removal of a partial upload.
    ///
    /// The staging layer removes the temp too, so this is the backend being tidy
    /// rather than the safety net — and a failure here must never replace the
    /// error that caused it.
    async fn remove_partial(&self, session: &crate::transport::SshConnection, remote: &str) {
        if let Err(e) = session.sftp().fs().remove_file(remote).await {
            debug!("SftpVolume::write_from_stream: couldn't remove the partial {remote}: {e}");
        }
    }
}

#[cfg(test)]
#[path = "writes_test.rs"]
mod writes_test;
