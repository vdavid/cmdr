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
use std::time::SystemTime;

use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{
    CarriedMetadata, ClassOutcome, MetadataClasses, MetadataReport, default_file_mode, routed_mode,
};
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, StreamWritten, VolumeError, VolumeReadStream, WriteMode};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use log::{debug, warn};
use openssh_sftp_client::UnixTimeStamp;
use openssh_sftp_client::file::File;
use openssh_sftp_client::metadata::{MetaDataBuilder, Permissions};
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
                message: e.to_string().into(),
                raw_os_error: e.raw_os_error(),
            })?;
        self.file
            .write(bytes)
            .await
            .map_err(|e| map_sftp_error(&e, &self.remote))
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
                message: format!("the server accepted none of a {}-byte write", bytes.len() - done).into(),
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
    /// ❗ From the transfer layer, `dest` is always a `.cmdr-tmp-*` sibling, never
    /// the user's filename: [`cmdr_fs::volume::Volume::write_is_single_shot`]
    /// keeps its `false` default here, so the transfer layer stages every write
    /// to this backend and nothing half-written ever wears a real name. ❌ Which
    /// is also why there is no "the create landed but the write didn't"
    /// classifier like SMB's — that one exists because SMB's compound path SKIPS
    /// staging.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        self.upload(dest, mode, length, stream, WRITE_WINDOW_DEPTH, on_progress)
            .await
    }

    /// The same, at a depth the caller picks. The measurement harness is the
    /// only thing that picks a different one; production takes the constant.
    pub(super) async fn upload(
        &self,
        dest: &Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        depth: usize,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        let remote = self.to_remote_path(dest)?;
        // ❗ Cloned out from under a short read guard, and the guard released
        // before a byte moves. An upload that held it would serialize every other
        // operation on the one channel, which is exactly the concurrency the
        // channel exists to provide.
        let session = self.clone_session().await?;
        debug!(
            "SftpVolume::write_from_stream: {remote}, length={length:?}",
            remote = LogPrivate::file_path(&remote)
        );

        // `CreateOrReplace` truncates: the transfer layer's staging name, which a
        // retried attempt writes onto after its predecessor. `CreateNew` is
        // `SSH_FXF_EXCL`, the same refusal `create_file` relies on, returned
        // before anything of ours exists at the name (so no cleanup arm below
        // can touch what's there).
        let mut options = session.sftp().options();
        options.write(true);
        let opened = match mode {
            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => options.create_new(true).open(&remote).await,
            WriteMode::CreateOrReplace => options.create(true).truncate(true).open(&remote).await,
        };
        let file = match opened {
            Ok(file) => file,
            Err(e) if mode.refuses_occupied() => return Err(self.name_taken(&session, &remote, &e).await),
            Err(e) => return Err(map_sftp_error(&e, &remote)),
        };
        // The source's mode, folded under the user's umask the way a local
        // destination folds it, asked before the first chunk while the stream
        // still holds what its open learned.
        let landed_mode = stream
            .metadata(MetadataClasses::MODE)
            .mode
            .and_then(|mode| routed_mode(mode, default_file_mode()));
        let mut file = file;
        if let Some(narrowed) = landed_mode.and_then(narrower_while_streaming) {
            narrow_while_streaming(&mut file, &remote, narrowed).await;
        }
        let writer = RemoteWrite::new(file, Arc::from(remote.as_str()));

        match self
            .pump(&mut stream, writer.clone(), &remote, length, depth, on_progress)
            .await
        {
            Ok(written) => {
                // ❗ The close is part of the write, not tidying after it.
                if let Err(e) = writer.close().await {
                    self.remove_partial(&session, &remote).await;
                    return Err(e);
                }
                let report = keep_source_attrs(&session, &remote, stream.modified_at(), landed_mode).await;
                cmdr_fs::volume::patching::patch_created(self, dest).await;
                let carried = CarriedMetadata {
                    mode: landed_mode,
                    ..CarriedMetadata::default()
                };
                Ok(StreamWritten::with_report(
                    written,
                    &report,
                    &carried,
                    stream.modified_at(),
                ))
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
        length: StreamLength,
        depth: usize,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
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
                // `Known(n)` promises exactly `n` bytes: a source that shrank or
                // grew while it was read fails here, and the caller takes the
                // partial away, as the WebDAV and S3 uploads do.
                if let StreamLength::Known(declared) = length
                    && written != declared
                {
                    return Err(size_mismatch(remote, written, declared));
                }
                return Ok(written);
            };
            written += landed?;
            // ❗ Cancellation arrives ONLY here. There is no token on this path:
            // the transfer engine says stop by answering `Break`, and a backend
            // that never called back would be uncancelable.
            if on_progress(StreamWriteProgress {
                bytes_written: written,
                expected_length: length,
            })
            .is_break()
            {
                return Err(VolumeError::Cancelled(remote.to_string().into()));
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
            let status = match &e {
                openssh_sftp_client::Error::SftpError(kind, _) => crate::errors::sftp_status_code(*kind),
                _ => 0,
            };
            debug!(
                "SftpVolume::write_from_stream: couldn't remove path={remote:?}: source=backend, backend=sftp, operation=remove_partial, sftp_status={status}, detail={:?}",
                cmdr_fs::log_detail::LogDetail(&e.to_string()),
                remote = LogPrivate::file_path(&remote)
            );
        }
    }
}

/// The mode a staged upload wears while its bytes stream, when the server's own
/// default would show them to more people than the final mode will: the final
/// group and other bits, owner read and write. `None` when the default is
/// already no wider, which is every ordinary file, so a plain upload costs no
/// extra request (`DETAILS.md` § "Mode on copies").
///
/// The server's umask is unknowable from here, so the user's own stands in for
/// it, the way a local destination reads it.
pub(super) fn narrower_while_streaming(landed_mode: u32) -> Option<u32> {
    let shown = default_file_mode() & 0o066;
    (shown & !landed_mode != 0).then_some((landed_mode & 0o077) | 0o600)
}

/// Puts `mode` on the open upload handle before the first byte (`SSH_FXP_FSETSTAT`).
/// Best effort: a refusal leaves the server's default, which is today's answer.
async fn narrow_while_streaming(file: &mut File, remote: &str, mode: u32) {
    if let Err(e) = file.set_permissions(Permissions::from(mode as u16)).await {
        warn!(
            "SftpVolume::write_from_stream: path={remote:?} streams under the server's default mode, not {mode:o}: source=backend, backend=sftp, operation=fsetstat, detail={:?}",
            cmdr_fs::log_detail::LogDetail(&e.to_string()),
            remote = LogPrivate::file_path(&remote)
        );
    }
}

/// Stamps the source's modification date and its folded mode onto what an
/// upload just wrote, in ONE `SSH_FXP_SETSTAT`: the mode rides the request the
/// date already costs.
///
/// ❗ After the close, by path: every byte is committed by then, so no server
/// that buffers writes until the close can bump the date past ours. Still before
/// the transfer layer's final rename (`remote` is its staging name), so the file
/// never wears its real name with the wrong date. SFTP v3 carries whole seconds
/// and forces atime alongside (`ATTR_ACMODTIME`); atime gets the same value.
/// `mode` is already folded (`cmdr_fs::metadata::routed_mode`): never wider than
/// the source, never setuid.
///
/// Best effort: a refusal is logged and reported, and the upload stays a
/// success, since the bytes are what matter. `None` leaves the server's own
/// value. The contract:
/// `crates/cmdr-operations/src/transfer/volume/DETAILS.md`
/// § "Copies keep the source's metadata".
pub(super) async fn keep_source_attrs(
    session: &crate::transport::SshConnection,
    remote: &str,
    modified_at: Option<SystemTime>,
    mode: Option<u32>,
) -> MetadataReport {
    let mut attrs = MetaDataBuilder::new();
    let mut any = false;
    if let Some(modified_at) = modified_at {
        match UnixTimeStamp::new(modified_at) {
            Ok(stamp) => {
                attrs.time(stamp, stamp);
                any = true;
            }
            // Before 1970 or past 2106: outside what SFTP v3's `u32` can say.
            Err(e) => {
                warn!(
                    "SftpVolume::keep_source_attrs: can't express the source date of {remote:?} in SFTP v3: {e}",
                    remote = LogPrivate::file_path(&remote)
                );
            }
        }
    }
    if let Some(mode) = mode {
        attrs.permissions(Permissions::from(mode as u16));
        any = true;
    }
    if !any {
        return MetadataReport::ABSENT;
    }
    let carried = CarriedMetadata {
        mode,
        ..CarriedMetadata::default()
    };
    let attrs = attrs.create();
    if let Err(e) = session.sftp().fs().set_metadata(remote, attrs).await {
        let status = match &e {
            openssh_sftp_client::Error::SftpError(kind, _) => crate::errors::sftp_status_code(*kind),
            _ => 0,
        };
        warn!(
            "SftpVolume::keep_source_attrs: couldn't keep the source date and mode on path={remote:?}: source=backend, backend=sftp, operation=setstat, sftp_status={status}, detail={:?}",
            cmdr_fs::log_detail::LogDetail(&e.to_string()),
            remote = LogPrivate::file_path(&remote)
        );
        return MetadataReport::uniform(&carried, modified_at, ClassOutcome::Refused);
    }
    MetadataReport::uniform(&carried, modified_at, ClassOutcome::Kept)
}

/// The source yielded a different byte count than its declared length.
fn size_mismatch(remote: &str, got: u64, declared: u64) -> VolumeError {
    VolumeError::IoError {
        message: ErrorDetail::path_then(
            remote,
            format!(
                ": the source declared {} and yielded {}",
                pluralize_grouped(declared, "byte"),
                pluralize_grouped(got, "byte")
            ),
        ),
        raw_os_error: None,
    }
}

#[cfg(test)]
#[path = "writes_test.rs"]
mod writes_test;
