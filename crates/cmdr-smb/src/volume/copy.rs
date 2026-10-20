//! Copying a file within one share without the bytes leaving the server
//! (`FSCTL_SRV_COPYCHUNK`, the mechanism Windows Explorer uses for same-share
//! copies).
//!
//! Duplicating a 4 GB video on a NAS otherwise pulls every byte down to the Mac
//! and pushes it straight back up: minutes over Wi-Fi, against a handful of
//! small control messages here.
//!
//! - ❗ **The caller stages it exactly as it stages a streamed write.** The
//!   destination holds a byte-incomplete file while this runs, so the bytes land
//!   on a `.cmdr-tmp-*` sibling and take the user's filename only at the end.
//! - ❗ **It runs in slices, ❌ never smb2's whole-file `server_side_copy_file`.**
//!   One call for 4 GB has nothing to report and no place to stop. A slice is
//!   one copychunk request, and every slice boundary is a place to report, to
//!   park a pause, and to stop.
//! - **A server without it** (older Samba, some NAS firmware) refuses the
//!   resume-key request before anything is written at `to`, and that refusal
//!   answers `NotSupported`: the caller's signal to stream the file instead.
//! - **Only one share qualifies.** A copychunk names its source by a resume key
//!   from an open on the same tree; the engine asks only when both sides are the
//!   very same volume (the trait's default `may_copy_on_server_from`).

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{MutationEvent, ServerCopyProgress, Volume, VolumeError};
use log::debug;
use smb2::client::Connection;
use smb2::client::tree::Tree;
use smb2::types::FileId;
use smb2::{FileInfo, ResumeKey};

use super::SmbVolume;
use super::mapping::user_flags_of;
use super::session::SessionLease;
use super::stamping::source_times;
use super::state::SessionGeneration;

/// How much of a file one slice asks the server to copy: exactly one copychunk
/// request at smb2's opening limits (16 chunks of 1 MiB, which Windows and
/// Samba both take), so a slice never costs a renegotiation round trip of its
/// own. A fraction of a second on a NAS's disks, which is a fair granularity
/// for a progress bar and for a Cancel click.
pub(super) const COPY_SLICE_BYTES: u64 = smb2::ServerSideCopyLimits::CONSERVATIVE.max_data_size as u64;

impl SmbVolume {
    /// Copies one file inside this share, if the server can do it.
    ///
    /// ❗ Answers `NotSupported` rather than streaming the file itself: the
    /// caller owns retry, staging, and progress for a streamed copy, and a
    /// backend quietly doing its own would take the file outside all three.
    pub(super) async fn copy_on_server_impl(
        &self,
        from: &Path,
        to: &Path,
        progress: &dyn ServerCopyProgress,
    ) -> Result<u64, VolumeError> {
        let smb_from = self.to_smb_path(from)?;
        let smb_to = self.to_smb_path(to)?;
        debug!(
            "SmbVolume::copy_on_server: share={:?}, from={:?}, to={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::file_path(&smb_from),
            LogPrivate::file_path(&smb_to)
        );
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;

        // The dates come from a `stat`, the handle from an open; both ride one
        // round trip on the multiplexed connection.
        let (stat, open) = {
            let mut stat_conn = conn.clone();
            tokio::join!(
                tree.stat(&mut stat_conn, &smb_from),
                tree.open_file(&mut conn, &smb_from)
            )
        };
        let (source, total) = self.handle_copy_result("copy_on_server(open source)", &smb_from, generation, open)?;
        let copied = match stat {
            Ok(info) if info.is_directory => Err(VolumeError::IsADirectory(self.to_display_path(&smb_from))),
            Ok(info) => {
                let copy = SliceCopy {
                    tree: &tree,
                    source,
                    total,
                    info: &info,
                    smb_from: &smb_from,
                    smb_to: &smb_to,
                    generation,
                };
                self.copy_from_open_source(&mut conn, &copy, progress).await
            }
            Err(e) => self.handle_copy_result("copy_on_server(stat source)", &smb_from, generation, Err(e)),
        };
        let _ = tree.close_handle(&mut conn, source).await;
        let copied = copied?;

        // Patch the listing cache from local knowledge, as a streamed write does.
        if let (Some(parent), Some(name)) = (to.parent(), to.file_name())
            && let Some(parent_display) = self.display_path_for(parent)
        {
            self.notify_mutation(
                &self.inner.volume_id,
                &parent_display,
                MutationEvent::Created(name.to_string_lossy().to_string()),
            )
            .await;
        }
        Ok(copied)
    }

    /// Everything after the source is open: the resume key, the destination, the
    /// slices, and the dates. The source handle is the caller's to close.
    async fn copy_from_open_source(
        &self,
        conn: &mut Connection,
        copy: &SliceCopy<'_>,
        progress: &dyn ServerCopyProgress,
    ) -> Result<u64, VolumeError> {
        // ❗ Before the destination is touched: this is where a server without
        // copychunk says so, and a refusal here leaves `to` exactly as it was.
        let key = self.handle_copy_result(
            "copy_on_server(resume key)",
            copy.smb_from,
            copy.generation,
            copy.tree.request_resume_key(conn, copy.source).await,
        )?;
        let dest = self
            .open_copy_destination(copy.tree, conn, copy.smb_to, copy.generation)
            .await?;

        let mut outcome = self.copy_slices(conn, copy, dest, &key, progress).await;
        if outcome.is_ok() {
            // Copychunk moves bytes only, so the date and Hidden are ours to
            // carry. On the copy's OWN handle, before it closes: a path stamp
            // while it's open would lose to the close. Best effort, as on a
            // streamed write.
            let name = copy.smb_from.rsplit('/').next().unwrap_or(copy.smb_from);
            let times = source_times(
                copy.info.modified.to_system_time(),
                copy.info.created.to_system_time(),
                user_flags_of(copy.info.attributes, name),
            );
            if let Some(times) = times
                && let Err(e) = copy.tree.set_handle_times(conn, dest, times).await
            {
                log::warn!(
                    "SmbVolume::copy_on_server: path={:?} keeps the server's date, not the source's: {e}",
                    LogPrivate::file_path(&copy.smb_to)
                );
            }
        }
        let closed = copy.tree.close_handle(conn, dest).await;
        if outcome.is_ok() {
            outcome = self.handle_smb_result("copy_on_server(close)", copy.smb_to, copy.generation, closed);
        }
        if outcome.is_err() {
            // ❗ The partial goes with the failure, cancellation included. The
            // staging layer above removes its temp too; this is the backend being
            // tidy rather than the safety net, and a failed delete never replaces
            // the error that caused it.
            let _ = copy.tree.delete_file(conn, copy.smb_to).await;
        }
        outcome.map(|()| copy.total)
    }

    /// Opens `smb_to` as a copy destination, created or TRUNCATED as the trait
    /// promises. Copychunk needs read and write on it, and smb2's only such open
    /// keeps what's there, so a non-empty file is emptied in place first (an
    /// empty write that replaces it, keeping its identity) and opened again.
    async fn open_copy_destination(
        &self,
        tree: &Tree,
        conn: &mut Connection,
        smb_to: &str,
        generation: SessionGeneration,
    ) -> Result<FileId, VolumeError> {
        let op = "copy_on_server(open destination)";
        let (dest, existing) =
            self.handle_smb_result(op, smb_to, generation, tree.open_file_readwrite(conn, smb_to).await)?;
        if existing == 0 {
            return Ok(dest);
        }
        let _ = tree.close_handle(conn, dest).await;
        self.handle_smb_result(
            op,
            smb_to,
            generation,
            tree.write_file_compound(conn, smb_to, &[]).await,
        )?;
        let (dest, _) = self.handle_smb_result(op, smb_to, generation, tree.open_file_readwrite(conn, smb_to).await)?;
        Ok(dest)
    }

    /// The slice loop: a checkpoint before each slice (where a pause parks and a
    /// Cancel arrives on the engine's path), and a report after it.
    async fn copy_slices(
        &self,
        conn: &mut Connection,
        copy: &SliceCopy<'_>,
        dest: FileId,
        key: &ResumeKey,
        progress: &dyn ServerCopyProgress,
    ) -> Result<(), VolumeError> {
        let cancelled = || VolumeError::Cancelled("Operation cancelled by user".into());
        // An empty source still reports once, so a caller's bar reaches 100%
        // rather than never being told about a zero-byte file.
        if copy.total == 0 {
            let _ = progress.advanced(0, 0);
            return Ok(());
        }
        let mut copied = 0u64;
        while copied < copy.total {
            if progress.checkpoint().await.is_break() {
                return Err(cancelled());
            }
            let take = COPY_SLICE_BYTES.min(copy.total - copied);
            // Same offset on both sides: the slice lands where it came from.
            let result = copy
                .tree
                .server_side_copy_range(conn, dest, key, copied, copied, take)
                .await;
            self.handle_copy_result("copy_on_server(copychunk)", copy.smb_to, copy.generation, result)?;
            copied += take;
            // ❗ The only cancel `copy_within` has: there is no token here.
            if progress.advanced(copied, copy.total).is_break() {
                return Err(cancelled());
            }
        }
        Ok(())
    }

    /// [`handle_smb_result`](Self::handle_smb_result), except that a server
    /// without server-side copy (`ErrorKind::Unsupported`) answers
    /// `NotSupported`, the caller's "stream it instead", ❌ never an I/O failure.
    pub(super) fn handle_copy_result<T>(
        &self,
        op_name: &str,
        smb_path: &str,
        ran_on: SessionGeneration,
        result: Result<T, smb2::Error>,
    ) -> Result<T, VolumeError> {
        match result {
            Err(e) if e.kind() == smb2::ErrorKind::Unsupported => {
                debug!(
                    // allowed-log-private: `op_name` is the volume method's name, not a user value
                    "SmbVolume::{op_name}(share={:?}): the server has no server-side copy ({:?}); the caller streams instead",
                    LogPrivate::volume(&self.inner.share_name),
                    e.status()
                );
                Err(VolumeError::NotSupported)
            }
            other => self.handle_smb_result(op_name, smb_path, ran_on, other),
        }
    }
}

/// One copy's fixed facts, so the steps after the open don't each take seven
/// arguments.
struct SliceCopy<'a> {
    tree: &'a Tree,
    source: FileId,
    total: u64,
    info: &'a FileInfo,
    smb_from: &'a str,
    smb_to: &'a str,
    /// The session the copy runs on, for the error funnel.
    generation: SessionGeneration,
}

/// `copy_within`'s bare progress callback as a [`ServerCopyProgress`]: reports
/// pass through, and there is no pause to park at.
pub(super) struct ProgressOnly<'a>(pub(super) &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync));

impl ServerCopyProgress for ProgressOnly<'_> {
    fn advanced(&self, done: u64, total: u64) -> ControlFlow<()> {
        (self.0)(done, total)
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

#[cfg(test)]
#[path = "copy_test.rs"]
mod copy_test;
