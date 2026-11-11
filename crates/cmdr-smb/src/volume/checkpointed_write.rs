//! A streamed upload that a long outage can carry on from instead of starting
//! over (`Volume::continue_write_from_stream`): the streaming loop both a fresh
//! write and a continuation drive, the FLUSH checkpoints it takes into a
//! staging temp, and the checkpoints it keeps for a continuation. `DETAILS.md`
//! § "Carrying an upload on".
//!
//! The promise this keeps (`Volume::continues_writes`): the offset reported as
//! `StreamWriteProgress::continues_from` is a contiguous, flushed prefix of the
//! temp. smb2's own count (`FileWriter::bytes_written`) can't be that: it adds
//! up pipelined WRITEs as they're answered, out of order.

use super::SmbVolume;
use super::session::TransferLease;
use super::state::RanOn;
use super::write_carries::{ClosedWriter, WriteCarries};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, StreamWritten, VolumeError, VolumeReadStream};
use smb2::WriteCheckpoint;
use smb2::client::stream::FileWriter;
use smb2::pack::FileTime;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

/// How many bytes a temp takes between two checkpoints. Each one drains the
/// write-behind window and waits on the server's fsync, so it's a trade
/// between throughput and how much an outage sends again: measured against
/// the Docker fixture (`DETAILS.md` § "Carrying an upload on").
pub(super) const CHECKPOINT_EVERY: u64 = 64 * 1024 * 1024;

/// A temp's last checkpoint, and what proves a reopened temp is still it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Kept {
    checkpoint: WriteCheckpoint,
    /// The temp's creation time, read once at its first checkpoint. smb2's
    /// `FileIdentity` is the server's inode number, which a filesystem hands
    /// out again (ext4 does at once), so a temp deleted and recreated under
    /// its name can wear the old identity; it can't wear the old birth.
    created: FileTime,
}

impl Kept {
    /// A checkpoint a continuation can prove: past byte zero, and naming its
    /// file.
    fn of(checkpoint: Option<WriteCheckpoint>, created: Option<FileTime>) -> Option<Self> {
        let checkpoint = checkpoint.filter(|c| c.offset > 0 && c.identity.is_some())?;
        Some(Self {
            checkpoint,
            created: created?,
        })
    }
}

/// The last checkpoint of every continuable write still in its temp, by share
/// path: kept as each is taken, so a write whose future the engine dropped
/// (the stall watchdog's abort) leaves its checkpoint too. A continuation
/// takes it, and keeps its own as it goes.
///
/// Forgotten as the write lands or its temp goes (`settle_partial`'s delete,
/// `delete_impl`, a fresh write to the name), so it holds one per temp a
/// failed write left at most.
pub(super) struct WriteCheckpoints {
    kept: std::sync::Mutex<HashMap<String, Kept>>,
    /// [`CHECKPOINT_EVERY`], which a test lowers to checkpoint a small file.
    every: AtomicU64,
}

impl Default for WriteCheckpoints {
    fn default() -> Self {
        Self {
            kept: std::sync::Mutex::default(),
            every: AtomicU64::new(CHECKPOINT_EVERY),
        }
    }
}

impl WriteCheckpoints {
    fn keep(&self, smb_path: &str, kept: Kept) {
        self.lock().insert(smb_path.to_owned(), kept);
    }

    fn take(&self, smb_path: &str) -> Option<Kept> {
        self.lock().remove(smb_path)
    }

    /// Drops what's kept for `smb_path`: the temp is going (or starting
    /// afresh), so nothing can carry it on.
    pub(super) fn forget(&self, smb_path: &str) {
        self.lock().remove(smb_path);
    }

    fn every(&self) -> u64 {
        self.every.load(Ordering::Relaxed)
    }

    /// Checkpoints every `bytes` from the next write on.
    #[cfg(any(test, feature = "testing"))]
    pub(super) fn set_every(&self, bytes: u64) {
        self.every.store(bytes.max(1), Ordering::Relaxed);
    }

    /// Whether anything is kept for `smb_path`.
    #[cfg(test)]
    pub(super) fn holds(&self, smb_path: &str) -> bool {
        self.lock().contains_key(smb_path)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Kept>> {
        self.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One streamed write's moving parts, for [`SmbVolume::stream_into_writer`].
pub(super) struct Streamed<'a> {
    pub(super) smb_path: &'a str,
    pub(super) ran_on: RanOn,
    pub(super) length: StreamLength,
    pub(super) on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    /// The write goes to a staging temp (`.cmdr-tmp-*`), so it checkpoints,
    /// and a transport fault keeps its partial for a continuation.
    pub(super) continuable: bool,
    /// A continuation's temp's creation time, as its first write read it.
    pub(super) created: Option<FileTime>,
}

impl SmbVolume {
    /// Streams `stream` into `writer` and closes it, checkpointing a
    /// continuable write every [`CHECKPOINT_EVERY`] bytes. A resumed writer
    /// starts at its checkpoint, and every count is from the file's first byte.
    ///
    /// On a failure the handle closes first, then the partial goes
    /// (`delete_partial`), or, for a continuable write that failed on the
    /// transport past a checkpoint, stays with its checkpoint kept for the
    /// continuation. A cancel always deletes.
    pub(super) async fn stream_into_writer<F: Future<Output = ()>>(
        &self,
        mut writer: FileWriter,
        streamed: &Streamed<'_>,
        stream: &mut dyn VolumeReadStream,
        carries: WriteCarries<'_>,
        delete_partial: &impl Fn() -> F,
    ) -> Result<ClosedWriter, VolumeError> {
        let Streamed {
            smb_path,
            ran_on,
            length,
            on_progress,
            continuable,
            mut created,
        } = *streamed;
        let checkpoints = &self.inner.write_checkpoints;
        let every = checkpoints.every();
        // A server that won't identify the file can't have a checkpoint
        // proven later, so taking one would be pure cost.
        let mut checkpointing = continuable && writer.identity().is_some();
        let start = writer.durable_offset().unwrap_or(0);
        let mut last = writer
            .durable_offset()
            .map(|offset| WriteCheckpoint::new(offset, writer.identity()));
        // A continuation's own checkpoint, kept again: its future may be
        // dropped before it takes another.
        if let Some(kept) = Kept::of(last, created) {
            checkpoints.keep(smb_path, kept);
        }
        let mut pushed = start;
        let mut next_checkpoint = start + every;
        let progress = |writer: &FileWriter, last: Option<WriteCheckpoint>| {
            StreamWriteProgress::new(start + writer.bytes_written(), length)
                .continuing_from(last.map_or(0, |checkpoint| checkpoint.offset))
        };

        loop {
            let chunk = match stream.next_chunk().await {
                None => break,
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => {
                    // The source broke, the share didn't: one more checkpoint
                    // saves what's already on the wire.
                    if checkpointing
                        && created.is_some()
                        && e.transport_fault().is_some()
                        && let Ok(checkpoint) = writer.flush_checkpoint().await
                    {
                        last = Some(checkpoint);
                        let _ = on_progress(progress(&writer, last));
                    }
                    let _ = writer.abort().await;
                    self.settle_partial(&e, Kept::of(last, created), smb_path, delete_partial)
                        .await;
                    return Err(e);
                }
            };
            if chunk.is_empty() {
                continue;
            }

            let write_result = writer.write_chunk(&chunk).await;
            pushed += chunk.len() as u64;
            let checkpointed = match write_result {
                Ok(()) if checkpointing && pushed >= next_checkpoint => writer.flush_checkpoint().await.map(Some),
                Ok(()) => Ok(None),
                Err(e) => Err(e),
            };
            match self.handle_smb_result("write_from_stream(write_chunk)", smb_path, ran_on, checkpointed) {
                Ok(Some(checkpoint)) => {
                    last = Some(checkpoint);
                    next_checkpoint = checkpoint.offset + every;
                    if created.is_none() {
                        created = self.temp_created(&carries, smb_path, writer.identity()).await;
                        // A temp nothing can prove later isn't worth a flush.
                        checkpointing = created.is_some();
                    }
                    if let Some(kept) = Kept::of(last, created) {
                        checkpoints.keep(smb_path, kept);
                    }
                }
                Ok(None) => {}
                Err(ve) => {
                    let _ = writer.abort().await;
                    self.settle_partial(&ve, Kept::of(last, created), smb_path, delete_partial)
                        .await;
                    return Err(ve);
                }
            }

            // `bytes_written()` is what the SERVER has acknowledged, not what
            // we handed to the pipeline. `write_chunk` returns as soon as the
            // chunk's WRITEs are on the wire, inside smb2's write-behind window,
            // so the two numbers differ by up to a full window per file (up to
            // 4 MiB, or 32 WRITEs on a fixed window) — and by
            // `concurrency x window` across an operation.
            //
            // Gotcha/Why: reporting the accepted count here made progress a
            // lie that compounded. In ERR-9WZRR (10-wide copy of 66 MB files
            // to a NAS over a 6.7 MB/s link) ~320 MiB sat queued client-side,
            // so the bar raced ahead by ~48 s of wire time, then flatlined
            // while the queue drained; the transfer watchdog read the flatline
            // as "no byte movement for 20s" on a perfectly healthy copy, and
            // the ETA was built on bytes nothing had committed. The staging
            // rename is the only durability signal that matters, and it lands
            // on the acknowledged count. ❌ Don't report accepted bytes to make
            // the bar move sooner: the wait is real and hiding it is what cost
            // us the diagnosis.
            //
            // Called on EVERY chunk even when the count hasn't moved, because
            // this is also the cancel poll (`ControlFlow::Break` below).
            if on_progress(progress(&writer, last)).is_break() {
                // Abort drains in-flight WRITE responses and closes the
                // handle without the server-side fsync that `finish()`
                // would force (we're about to delete the partial file
                // anyway). Dropping directly would leave stale responses
                // on the connection and poison the next op.
                let _ = writer.abort().await;
                checkpoints.forget(smb_path);
                delete_partial().await;
                return Err(VolumeError::Cancelled("Operation cancelled by user".into()));
            }
        }

        let kept = Kept::of(last, created);
        let closed = self
            .close_writer(writer, smb_path, ran_on, carries, kept, delete_partial)
            .await?;
        checkpoints.forget(smb_path);
        // `finish()` drains the whole window, so the last chunks are confirmed
        // only now. Without this call the file's progress would stop up to a
        // window short of its size and never reach 100%. Its `ControlFlow` is
        // deliberately ignored: the bytes are committed and the handle closed,
        // so there is nothing left here for a cancel to stop.
        let total = start + closed.confirmed;
        let _ = on_progress(StreamWriteProgress::in_order(total, length));
        Ok(ClosedWriter {
            confirmed: total,
            ..closed
        })
    }

    /// The temp's creation time, once its writer has a checkpoint: `None` when
    /// the stat fails or names another file than `identity` (then nothing can
    /// prove a reopened temp is this one).
    async fn temp_created(
        &self,
        carries: &WriteCarries<'_>,
        smb_path: &str,
        identity: Option<smb2::FileIdentity>,
    ) -> Option<FileTime> {
        let mut conn = carries.side_conn.clone();
        match carries.tree.stat_with_identity(&mut conn, smb_path).await {
            Ok((info, found)) if found.is_some() && found == identity => Some(info.created),
            _ => None,
        }
    }

    /// What a failed streamed write leaves: its partial with `kept`, for the
    /// engine to carry on from after a transport fault, else nothing. The
    /// handle is already closed.
    pub(super) async fn settle_partial<F: Future<Output = ()>>(
        &self,
        error: &VolumeError,
        kept: Option<Kept>,
        smb_path: &str,
        delete_partial: &impl Fn() -> F,
    ) {
        match kept {
            Some(kept) if error.transport_fault().is_some() => {
                log::debug!(
                    "SmbVolume::write_from_stream: keeping {} at its checkpoint, byte {}, to carry on from",
                    LogPrivate::file_path(smb_path),
                    kept.checkpoint.offset
                );
                self.inner.write_checkpoints.keep(smb_path, kept);
            }
            _ => {
                self.inner.write_checkpoints.forget(smb_path);
                delete_partial().await;
            }
        }
    }

    /// Inherent body for the `continue_write_from_stream` trait method (thin
    /// delegator in `volume_impl`): reopens the temp a failed write left at
    /// its kept checkpoint once it's proven the same file (its birth here,
    /// then smb2's identity and length check), and streams the rest as a fresh
    /// write would. `finish` trims whatever the failed write left past the
    /// last byte.
    ///
    /// `NotSupported` (the engine runs the file again) for a temp with no
    /// checkpoint kept, an `at` past it, a temp that's gone, replaced, or
    /// shorter, or any server refusal. A transport fault before the reopen
    /// answers keeps the checkpoint for the next try.
    pub(super) fn continue_write_from_stream_impl<'a>(
        &'a self,
        target: &'a Path,
        at: u64,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(target)?;
            let checkpoints = &self.inner.write_checkpoints;
            let Some(kept) = checkpoints.take(&smb_path) else {
                return Err(VolumeError::NotSupported);
            };
            // The engine carries on from the last offset it heard, which can
            // trail the kept one (a checkpoint taken on the way out); every
            // byte before either is flushed.
            if at == 0 || at > kept.checkpoint.offset {
                return Err(VolumeError::NotSupported);
            }
            let checkpoint = WriteCheckpoint::new(at, kept.checkpoint.identity);
            let refused = |why: &dyn std::fmt::Display| {
                log::info!(
                    "SmbVolume::continue_write_from_stream: can't carry {} on from byte {at}: {why}",
                    LogPrivate::file_path(&smb_path)
                );
                VolumeError::NotSupported
            };
            let TransferLease {
                tree,
                conn,
                ran_on,
                on: _,
            } = match self.transfer_lease().await {
                Ok(lease) => lease,
                Err(ve) => {
                    if ve.transport_fault().is_some() {
                        checkpoints.keep(&smb_path, kept);
                    }
                    return Err(ve);
                }
            };
            // A transport fault keeps the checkpoint for the next try; any
            // other answer means this temp can't be carried on.
            let transport_or_refused = |op: &str, e: smb2::Error| {
                let ve = self
                    .handle_smb_result(op, &smb_path, ran_on, Err::<(), _>(e))
                    .err()
                    .unwrap_or(VolumeError::NotSupported);
                if ve.transport_fault().is_some() {
                    checkpoints.keep(&smb_path, kept);
                    ve
                } else {
                    refused(&LogError(&ve))
                }
            };
            let mut side_conn = conn.clone();
            match tree.stat_with_identity(&mut side_conn, &smb_path).await {
                Ok((info, found)) if found == kept.checkpoint.identity && info.created == kept.created => {}
                Ok(_) => return Err(refused(&"the temp was replaced")),
                Err(e) => return Err(transport_or_refused("continue_write_from_stream(stat)", e)),
            }
            let writer = match tree.resume_file_writer(conn, &smb_path, &checkpoint).await {
                Ok(writer) => writer,
                Err(smb2::Error::ResumeRefused { reason, .. }) => return Err(refused(&format_args!("{reason:?}"))),
                Err(e) => return Err(transport_or_refused("continue_write_from_stream(open)", e)),
            };
            log::debug!(
                "SmbVolume::continue_write_from_stream: carrying {} on from byte {at}",
                LogPrivate::file_path(&smb_path)
            );
            let carried = self.carried_for_write(&*stream);
            let delete_partial = self.partial_deleter(&smb_path);
            let streamed = Streamed {
                smb_path: &smb_path,
                ran_on,
                length,
                on_progress,
                continuable: true,
                created: Some(kept.created),
            };
            let carries = WriteCarries {
                tree: &tree,
                side_conn: &mut side_conn,
                carried: &carried,
            };
            let closed = self
                .stream_into_writer(writer, &streamed, &mut *stream, carries, &delete_partial)
                .await?;
            Ok(self.written(target, closed, &carried, stream.modified_at()).await)
        })
    }
}
