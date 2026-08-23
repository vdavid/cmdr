//! Copying a file from one place on the server to another without the bytes
//! leaving it.
//!
//! `copy-data@openssh.com` (OpenSSH 9.0 and later) asks the server to copy a byte
//! range from one open handle to another. Duplicating a 4 GB video inside one
//! server otherwise means downloading it and uploading it again: twice the file
//! over the link, and at 30 MB/s that is four minutes against roughly nothing.
//!
//! Two things make it safe to reach for:
//!
//! - ❗ **The caller stages it exactly as it stages a streamed write.** A
//!   half-copied destination is a real state here, so the bytes land on a
//!   `.cmdr-tmp-*` sibling and take the user's filename only at the end. Which is
//!   why `Volume::copy_within` is documented as never single-shot.
//! - ❗ **It is chunked, not one request for the whole file.** One `copy-data`
//!   for 4 GB is one unanswered request for as long as the server's disks take,
//!   with no progress and no way to cancel. A chunk is a bounded wait, and every
//!   chunk boundary is a place to report and to stop.
//!
//! A server without the extension answers `NotSupported`, which is the caller's
//! signal to stream the file through Cmdr the ordinary way — `sftp-fixture-
//! noposixrename` is the fixture that has neither extension, so the fallback is
//! exercised rather than assumed.

use std::cmp::min;
use std::io::SeekFrom;
use std::num::NonZeroU64;
use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;

use cmdr_fs::volume::VolumeError;
use openssh_sftp_client::file::File;
use tokio::io::AsyncSeek;

use super::SftpVolume;
use crate::errors::map_sftp_error;

/// How much of a file one `copy-data` request asks the server to copy.
///
/// The whole file in one request would be a single unanswered round trip for
/// however long the server's disks take, with nothing to report and nothing to
/// cancel between. 8 MiB is a fraction of a second on a local disk and a second
/// or two on a loaded NAS, which is a fair granularity for a progress bar and for
/// a Cancel click. ❗ The bytes never cross the wire whatever this is, so a
/// bigger number buys nothing.
const COPY_CHUNK_BYTES: u64 = 8 * 1024 * 1024;

impl SftpVolume {
    /// Copies one file inside this server, if this server can do it.
    ///
    /// ❗ Answers `NotSupported` rather than falling back to a stream itself: the
    /// caller owns retry, staging, and progress for a streamed copy, and a
    /// backend quietly doing its own would take the file outside all three.
    pub(super) async fn copy_within_impl(
        &self,
        from: &Path,
        to: &Path,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        let session = self.clone_session().await?;
        if !session.extensions().copy_data {
            return Err(VolumeError::NotSupported);
        }

        let mut source = session
            .sftp()
            .open(&remote_from)
            .await
            .map_err(|e| map_sftp_error(&e, &remote_from))?;
        // The length is what the source says NOW, and it is not chased. A file
        // that grew under the copy is the same call as one that grew under a
        // streamed read, and both report the tree they started from.
        let total = source
            .metadata()
            .await
            .map_err(|e| map_sftp_error(&e, &remote_from))?
            .len()
            .unwrap_or(0);

        let mut dest = session
            .sftp()
            .options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&remote_to)
            .await
            .map_err(|e| map_sftp_error(&e, &remote_to))?;

        match self
            .copy_chunks(&mut source, &mut dest, total, &remote_to, on_progress)
            .await
        {
            Ok(()) => {}
            Err(e) => {
                // ❗ The partial goes with the failure, cancellation included. The
                // staging layer above removes its temp too; this is the backend
                // being tidy rather than the safety net, and a failure removing it
                // never replaces the error that caused it.
                drop(dest);
                let _ = session.sftp().fs().remove_file(&remote_to).await;
                return Err(e);
            }
        }

        // ❗ Awaited, ❌ never dropped. A dropped `File` sends the same
        // `SSH_FXP_CLOSE` on a detached task and throws away the one report a
        // server gives of bytes it accepted but could not commit.
        dest.close().await.map_err(|e| map_sftp_error(&e, &remote_to))?;
        Ok(total)
    }

    /// The chunk loop, split out so every exit from it takes the partial with it.
    async fn copy_chunks(
        &self,
        source: &mut File,
        dest: &mut File,
        total: u64,
        remote_to: &str,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<(), VolumeError> {
        // An empty source still reports once, so a caller's bar reaches 100%
        // rather than never being told about a zero-byte file.
        if total == 0 {
            let _ = on_progress(0, 0);
            return Ok(());
        }
        let mut copied = 0u64;
        while copied < total {
            let take = min(COPY_CHUNK_BYTES, total - copied);
            // ❗ Both offsets are named on every request. The engine advances its
            // own by the length it was ASKED for, and this path is the one place
            // two handles' offsets have to stay in step; naming them is what makes
            // that independent of the engine's bookkeeping.
            seek_to(source, copied)?;
            seek_to(dest, copied)?;
            let Some(take) = NonZeroU64::new(take) else {
                break;
            };
            source.copy_to(dest, take).await.map_err(|e| map_sftp_error(&e, remote_to))?;
            copied += take.get();
            // ❗ The only cancellation this path has. There is no token here, so a
            // caller that never looked at the callback would be uncancelable.
            if on_progress(copied, total).is_break() {
                return Err(VolumeError::Cancelled(self.volume_id().to_string()));
            }
        }
        Ok(())
    }
}

/// Points an open handle at `offset`, which for SFTP is local bookkeeping rather
/// than a round trip.
fn seek_to(file: &mut File, offset: u64) -> Result<(), VolumeError> {
    Pin::new(file)
        .start_seek(SeekFrom::Start(offset))
        .map_err(|e| VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: e.raw_os_error(),
        })
}

#[cfg(test)]
#[path = "copy_test.rs"]
mod copy_test;
