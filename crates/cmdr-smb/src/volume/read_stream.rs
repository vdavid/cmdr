//! Reading an SMB file as a stream: the producer behind a `ChannelReadStream`
//! (`open_smb_download_stream`, the one streaming-read primitive the `Volume`
//! read methods in `volume_impl` build on), the single-chunk `InlineReadStream`
//! the compound CREATE+READ+CLOSE fast path hands out, and the condition for
//! taking that fast path. Writing a stream into a file is `write_stream`.

use super::copy_xattrs::{SourceXattrs, XattrAsk, source_xattrs};
use super::dfs_links::is_behind_dfs_link;
use super::mapping::{map_smb_error, user_flags_of};
use super::resume::Reattach;
use super::session::{LeaseOn, TransferLease};
use super::state::{RanOn, ends_the_session};
use super::{SmbVolume, SmbVolumeInner};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataAsk, MetadataClasses, UserFlags};
use cmdr_fs::volume::{ChannelReadStream, SourceVersion, StreamLength, VolumeError, VolumeReadStream};
use log::{debug, info, warn};
use smb2::client::stream::{DownloadCheckpoint, FileDownload, FileVersion, resume_file_download};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

/// Backpressure window for the chunk channel: 4 × `smb2::DOWNLOAD_CHUNK_SIZE`
/// (512 KiB) = 2 MiB of delivered-but-unconsumed bytes per download, on top of
/// the up-to-4 MiB smb2's adaptive read-ahead keeps on the wire. The wire window
/// is what fills the link; this only absorbs the consumer's per-chunk jitter,
/// and a consumer slower than the link gains nothing from more slots. Worth
/// raising (to 8) only if a profile shows the producer parked on `send` while
/// the consumer is idle.
pub(super) const SMB_STREAM_CHANNEL_CAPACITY: usize = 4;

/// The `max_read_size` to assume when the session hasn't reported its
/// negotiated params: the SMB2 floor, same reasoning as `write_stream::ASSUMED_MAX_WRITE`.
pub(super) const ASSUMED_MAX_READ: u64 = 65536;

/// THE condition for `open_read_stream_with_hint`'s compound CREATE+READ+CLOSE
/// fast path: the file fits the connection's `quick_read_limit()`, what the
/// link moves in 250 ms at the capacity a recent download measured (timed from
/// READ answers that queued behind each other, so it errs low). That's one
/// streaming-download chunk (`smb2::DOWNLOAD_CHUNK_SIZE`, 512 KiB) on a cold
/// connection, never more than the server's `max_read`, and never more than
/// half the credit window funds once smb2 has seen the server's ceiling; smb2
/// owns that arithmetic. The 250 ms is a fixed budget for how long a listing
/// may wait behind one frame, apart from the read-ahead window's learned
/// headroom. The scan pool's prefetch deliberately doesn't use it
/// (`scan_pool.rs` says why).
///
/// The compound saves the round trip a stream spends on its own CREATE, but
/// carries the whole file as ONE READ with no progress, queued ahead of every
/// listing on the connection, while `Tree::download` streams it in chunks
/// through an adaptive window. So it pays off exactly while the READ is short:
/// at 375 KB/s a single READ took 23 s for 8 MiB with nothing in between
/// (cmdr-reports#15), which the cold one-chunk limit and the measured rate both
/// keep off it. Measured on smb2's read-ahead bench
/// (`benchmarks/read-ahead/results/close-and-quick-read.md` in the smb2 repo,
/// smb2 0.24.2, warm connection, last chunk in ms, 2026-09-23): at +60 ms a
/// 4 MiB file took 139 compounded against 203 streamed; at +200 ms 1 MiB took
/// 212 against 422. The rate errs low, so a borderline file streams. Because
/// it measures the link rather than the last download's pace, it lets fast,
/// distant links compound more: at +200 ms on 30 MB/s, 4 MiB compounds and
/// lands in 673 ms, where streaming it took 919
/// (`benchmarks/read-ahead/results/self-tuning.md`, smb2 0.25.2, 2026-09-24).
pub(super) fn fits_one_compound_read(quick_read_limit: u64, size: u64) -> bool {
    size > 0 && size <= quick_read_limit
}

/// Wraps a pre-read `Vec<u8>` as a `VolumeReadStream` that yields the whole
/// buffer as a single chunk. Used by the compound fast-path in
/// `open_read_stream_with_hint`, where the full file body came back inside one
/// SMB compound response; there's no more I/O to drive, just hand the bytes
/// to the consumer.
pub(super) struct InlineReadStream {
    data: Option<Vec<u8>>,
    total_size: u64,
    bytes_read: u64,
    modified_at: Option<std::time::SystemTime>,
    /// The server's `CreationTime`, from the same CREATE response.
    created: Option<std::time::SystemTime>,
    /// Hidden and Locked, from the same CREATE response's attributes.
    flags: UserFlags,
    /// The xattrs, from the stream listing in the same frame.
    xattrs: SourceXattrs,
}

impl InlineReadStream {
    pub(super) fn new(data: Vec<u8>, modified_at: Option<std::time::SystemTime>) -> Self {
        let total_size = data.len() as u64;
        Self {
            data: Some(data),
            total_size,
            bytes_read: 0,
            modified_at,
            created: None,
            flags: UserFlags::NONE,
            xattrs: SourceXattrs::None,
        }
    }

    /// The source file's creation date and flags, from the CREATE response
    /// `info` of the file at `smb_path`, for [`VolumeReadStream::metadata`].
    #[must_use]
    pub(super) fn with_source_info(mut self, info: &smb2::FileInfo, smb_path: &str) -> Self {
        self.created = info.created.to_system_time();
        self.flags = user_flags_of(info.attributes, name_of(smb_path));
        self
    }

    /// The source file's xattrs, as far as the read learned them.
    #[must_use]
    pub(super) fn with_xattrs(mut self, xattrs: SourceXattrs) -> Self {
        self.xattrs = xattrs;
        self
    }
}

/// Which version of a file an SMB download read, as smb2 checks it again on a
/// resume (identity, length, last-write and change times): what
/// `VolumeReadStream::source_version` hands the engine.
#[derive(Clone, Copy)]
pub(super) struct SmbSourceVersion(FileVersion);

/// Where a carried-on download starts, and the version it must still be.
#[derive(Clone, Copy)]
pub(super) struct CarryFrom {
    version: FileVersion,
    offset: u64,
}

/// What a streaming download's producer learned from opening the file.
struct OpenedDownload {
    /// Which version of the file the download reads, when it opened its own.
    version: Option<FileVersion>,
    total_size: u64,
    /// The server's `LastWriteTime`, from the same CREATE response.
    modified_at: Option<std::time::SystemTime>,
    /// The server's `CreationTime`, from the same CREATE response.
    created: Option<std::time::SystemTime>,
    /// Hidden and Locked, from the same CREATE response's attributes.
    flags: UserFlags,
    /// The xattrs, from the stream listing in the same frame.
    xattrs: SourceXattrs,
}

/// The last component of an SMB path, which decides whether Samba's Hidden is
/// the name's own (`user_flags_of`).
fn name_of(smb_path: &str) -> &str {
    smb_path.rsplit(['\\', '/']).next().unwrap_or(smb_path)
}

/// After a download's last byte: collects the CLOSE's answer, which smb2 sent
/// as the last chunk landed.
async fn finish_download(mut download: FileDownload<'_>) -> Result<(), smb2::Error> {
    match download.next_chunk().await {
        Some(Err(e)) => Err(e),
        _ => Ok(()),
    }
}

impl VolumeReadStream for InlineReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let data = self.data.take()?;
            self.bytes_read = data.len() as u64;
            Some(Ok(data))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total_size)
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.modified_at
    }

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        CarriedMetadata {
            created: self.created,
            flags: self.flags,
            xattrs: self.xattrs.values(),
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        CarriedMetadata {
            created: self.created,
            flags: self.flags,
            xattrs: self.xattrs.outline(),
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }
}

impl SmbVolume {
    /// Inherent body for the `open_read_stream_for_copy` trait method (thin
    /// delegator in `volume_impl`): the hinted read, listing the file's streams
    /// in its own frame when the copy asks about xattrs and the share has them
    /// (`copy_xattrs.rs`).
    pub(super) async fn open_read_stream_for_copy_impl(
        &self,
        path: &Path,
        size_hint: Option<u64>,
        ask: MetadataAsk,
    ) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
        let xattrs = XattrAsk::of(ask, self.stores_xattrs());
        self.open_read_stream_with_hint_impl(path, size_hint, xattrs).await
    }

    /// Inherent body for the `open_read_stream_with_hint` trait method (thin
    /// delegator in `volume_impl`, with `XattrAsk::Nothing`).
    pub(super) async fn open_read_stream_with_hint_impl(
        &self,
        path: &Path,
        size_hint: Option<u64>,
        xattrs: XattrAsk,
    ) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        // Compound fast-path: if the caller-provided hint fits the
        // connection's `quick_read_limit()` (`fits_one_compound_read` says
        // why that limit and not `max_read`), send CREATE+READ+CLOSE as a
        // single compound frame (1 RTT) instead of the streaming open's
        // extra round trips. Drives the compound on a cloned
        // `Connection` with no lock held, so N concurrent small reads
        // pipeline over one SMB session.
        //
        // The READ is sized to the HINT, never to `max_read`: the credit
        // charge follows the response size the request asks for, so an
        // unsized read of a 100 KB file books a whole 8 MB `max_read` (128
        // credits), and a handful of them exhaust the connection's budget
        // while barely filling the wire.
        //
        // Falls through to the streaming path when the hint is missing or
        // over the limit, when the credit window can't fund the one READ,
        // or when the file changed size since the
        // scan. `expected_size` is a HARD bound, so the two drift arms split
        // cleanly and TOGETHER are what keeps a changed file from being
        // copied truncated: a file that SHRANK comes back short of the hint
        // (`data.len() != size`), and one that GREW comes back as a typed
        // `ErrorKind::TooLarge`, because smb2 compares the server's
        // authoritative size against the length asked for and refuses rather
        // than hand back a prefix. ❌ Neither arm is decoration; dropping
        // either one copies a changed file under its final name, short.
        if let Some(size) = size_hint {
            // On the transfer session, like every byte-moving read
            // (`transfer_session.rs`). The lease lives to the end of this
            // block, so the session can't idle-close under the frame.
            let TransferLease {
                tree,
                mut conn,
                ran_on,
                on: _lease,
            } = self.transfer_lease().await?;
            if fits_one_compound_read(conn.quick_read_limit(), size) {
                debug!(
                    "SmbVolume::open_read_stream_with_hint: share={:?}, path={:?}, size={}; using compound fast-path",
                    LogPrivate::volume(&self.inner.share_name),
                    LogPrivate::file_path(&smb_path),
                    size
                );
                // The date rides on the frame's own CREATE response, and the
                // stream listing (asked only for a copy that wants it) on the
                // same frame.
                let read_result = match xattrs.lists() {
                    true => {
                        tree.read_file_compound_sized_with_streams(&mut conn, &smb_path, size)
                            .await
                    }
                    false => tree
                        .read_file_compound_sized_with_info(&mut conn, &smb_path, size)
                        .await
                        .map(|(data, info)| (data, info, None)),
                };
                match read_result {
                    Err(e) if matches!(e.kind(), smb2::ErrorKind::TooLarge) => {
                        debug!(
                            "SmbVolume::open_read_stream_with_hint: file grew past the hinted size since the scan ({}); falling back to streaming",
                            e
                        );
                    }
                    // The credit window can't fund this READ in one go.
                    // `quick_read_limit` counts the window's ceiling, so this
                    // is the ceiling arriving or shrinking after that check.
                    // smb2 refused before anything reached the wire, and a
                    // stream needs only a chunk's worth at a time. A server
                    // that stopped granting altogether starves the stream
                    // too, which reports it then.
                    Err(e @ smb2::Error::CreditStarvation { .. }) => {
                        debug!(
                            "SmbVolume::open_read_stream_with_hint: the credit window can't fund one compound read ({}); falling back to streaming",
                            e
                        );
                    }
                    // Behind a DFS link: the compound's CREATE was refused,
                    // so nothing was read, and the streaming path follows
                    // the link (`dfs_links.rs`).
                    Err(e) if is_behind_dfs_link(&e) => {
                        debug!(
                            "SmbVolume::open_read_stream_with_hint: the file sits behind a DFS link; streaming it through the link"
                        );
                    }
                    read_result => {
                        let (data, info, listed) = self.handle_smb_result(
                            "open_read_stream_with_hint(compound)",
                            &smb_path,
                            ran_on,
                            read_result,
                        )?;
                        if data.len() as u64 == size {
                            let modified_at = info.modified.to_system_time();
                            let carried = source_xattrs(&tree, &mut conn, &smb_path, listed.as_deref(), xattrs).await;
                            return Ok(Box::new(
                                InlineReadStream::new(data, modified_at)
                                    .with_source_info(&info, &smb_path)
                                    .with_xattrs(carried),
                            ) as Box<dyn VolumeReadStream>);
                        }
                        debug!(
                            "SmbVolume::open_read_stream_with_hint: compound read returned {} bytes, expected {}; falling back to streaming",
                            data.len(),
                            size
                        );
                    }
                }
            }
        }

        debug!(
            "SmbVolume::open_read_stream_with_hint: share={:?}, path={:?}; using streaming path",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::file_path(&smb_path)
        );
        let stream = self.open_smb_download_stream(&smb_path, xattrs, None).await?;
        Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
    }

    /// Inherent body for the `resume_read_stream` trait method: the streaming
    /// download again, from `offset`, once smb2 has proven the file is still
    /// `version` (`resume_file_download`). A version another backend made, or
    /// one from a compound read (which records none), can't be carried on.
    pub(super) async fn resume_read_stream_impl(
        &self,
        path: &Path,
        version: &SourceVersion,
        offset: u64,
    ) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
        let Some(SmbSourceVersion(version)) = version.downcast_ref::<SmbSourceVersion>() else {
            return Err(VolumeError::NotSupported);
        };
        let smb_path = self.to_smb_path(path)?;
        debug!(
            "SmbVolume::resume_read_stream: share={:?}, path={:?}, offset={offset}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::file_path(&smb_path)
        );
        let from = CarryFrom {
            version: *version,
            offset,
        };
        let stream = self
            .open_smb_download_stream(&smb_path, XattrAsk::Nothing, Some(from))
            .await?;
        Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
    }

    /// Opens a streaming download on the given SMB-relative path.
    ///
    /// Takes a [`TransferLease`] (the share's transfer session, or the main
    /// session while that one is down), then spawns a background task that owns
    /// the lease and drives `Tree::download` on its connection clone. Each
    /// concurrent call gets its own clone, all multiplexing frames over one SMB
    /// session, so N downloads run pipelined instead of serializing on a mutex.
    /// Chunks flow through a bounded mpsc channel to the caller-facing
    /// [`ChannelReadStream`]. A file behind a DFS link streams through the link
    /// instead (`dfs_links.rs`).
    ///
    /// This is the single streaming-read primitive for `SmbVolume`. The
    /// cross-volume streaming path (`open_read_stream`) goes through here, so
    /// no path has to buffer whole files in memory.
    ///
    /// `from` carries a copy on after a long outage
    /// (`Volume::resume_read_stream`): the download opens with smb2's resume,
    /// which proves the file is still that version before reading from its
    /// offset, and a changed one fails the open as `SourceChanged`.
    pub(super) async fn open_smb_download_stream(
        &self,
        smb_path: &str,
        xattrs: XattrAsk,
        from: Option<CarryFrom>,
    ) -> Result<ChannelReadStream, VolumeError> {
        let lease = self.transfer_lease().await?;

        // The open reports the size and the date together: both ride on the
        // download's own CREATE response, so the date costs no round trip.
        let (opened_tx, opened_rx) = tokio::sync::oneshot::channel::<Result<OpenedDownload, VolumeError>>();
        let (chunk_tx, chunk_rx) =
            tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        // The producer outlives this call and reports a mid-stream session loss
        // through the share's funnel, against the session it streams on.
        let share = Arc::clone(&self.inner);
        let share_name = self.inner.share_name.clone();
        let smb_path_owned = smb_path.to_string();
        // Computed here rather than in the task: `to_display_path` needs `&self`,
        // which the spawned producer deliberately doesn't capture.
        let display_path = self.to_display_path(smb_path);

        self.inner.host().runtime().spawn(async move {
            // The task owns the lease (its connection clone, its `Arc<Tree>`, and
            // on the transfer session the hold that keeps it open). No lock is
            // held, so other tasks can spawn in parallel and each drive their own
            // download, all multiplexed over the same SMB session by smb2's
            // receiver task.
            let TransferLease {
                tree,
                mut conn,
                mut ran_on,
                mut on,
            } = lease;
            // For the xattr reads beside the download, which holds `conn`.
            let mut side_conn = conn.clone();
            // A file behind a DFS link opens through the link, and that
            // download owns the target's connection (`dfs_links.rs`). Under a
            // known link it goes there first, skipping the refused open.
            let known_link = share.link_paths.covers(&smb_path_owned);
            // A copy carried on after an outage opens with the resume, which
            // proves the version before its first READ.
            let checkpoint = from.map(|from| DownloadCheckpoint::new(smb_path_owned.clone(), from.offset, from.version));
            let through_link = match (known_link, &checkpoint) {
                (true, None) => share.open_link_download(ran_on, &on, &tree, &smb_path_owned).await,
                (true, Some(checkpoint)) => {
                    share
                        .resume_link_download(ran_on, &on, &tree, &smb_path_owned, checkpoint)
                        .await
                }
                (false, _) => None,
            };
            // Whether the download reads through a link, so a resume goes the
            // same way.
            let mut via_link = through_link.is_some();
            let opened = match through_link {
                Some(opened) => opened,
                None => {
                    let direct = match &checkpoint {
                        None => open_download(&tree, &mut conn, &smb_path_owned, xattrs).await,
                        Some(checkpoint) => resume_file_download(Arc::clone(&tree), conn.clone(), checkpoint).await,
                    };
                    match direct {
                        Err(e) if is_behind_dfs_link(&e) => {
                            via_link = true;
                            let through = match &checkpoint {
                                None => share.open_link_download(ran_on, &on, &tree, &smb_path_owned).await,
                                Some(checkpoint) => {
                                    share
                                        .resume_link_download(ran_on, &on, &tree, &smb_path_owned, checkpoint)
                                        .await
                                }
                            };
                            through.unwrap_or(Err(e))
                        }
                        opened => opened,
                    }
                }
            };
            let mut download = match opened {
                Ok(d) => d,
                Err(e) => {
                    share.note_error(ran_on, &e);
                    warn!(
                        "SmbVolume::download(share={:?}, path={:?}): {}",
                        LogPrivate::volume(&share_name),
                        LogPrivate::file_path(&smb_path_owned),
                        e
                    );
                    let _ = opened_tx.send(Err(map_smb_error(e, &display_path)));
                    return;
                }
            };

            let total_size = download.size();
            let info = download.info();
            // A download through a DFS link listed nothing: its xattrs stay
            // unknown, which the loss note can't count. A carried-on one asks
            // nothing: the engine replays the first stream's answers.
            let ask = match checkpoint {
                None => xattrs,
                Some(_) => XattrAsk::Nothing,
            };
            let carried = source_xattrs(&tree, &mut side_conn, &smb_path_owned, download.streams(), ask).await;
            // Which version this download reads (the first open's, through any
            // resume), for the copy to carry on from after a long outage.
            let version = download.checkpoint().map(|checkpoint| checkpoint.version);
            let opened = OpenedDownload {
                version,
                total_size,
                modified_at: info.and_then(|info| info.modified.to_system_time()),
                created: info.and_then(|info| info.created.to_system_time()),
                flags: info.map_or(UserFlags::NONE, |info| {
                    user_flags_of(info.attributes, name_of(&smb_path_owned))
                }),
                xattrs: carried,
            };
            if opened_tx.send(Ok(opened)).is_err() {
                // Caller dropped the stream before receiving size. Dropping the
                // download queues its CLOSE (smb2 0.32.0), so the handle goes too.
                return;
            }

            let mut finished = false;
            // Held until the CLOSE's answer, so a delete of this file waits for it.
            let mut closing = None;
            // Bytes of the file the consumer has, from its first byte, across
            // every resume: a resumed download counts only its own part.
            let mut delivered: u64 = from.map_or(0, |from| from.offset);
            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        debug!(
                            "SmbVolume::download(share={:?}, path={:?}): cancelled after {} bytes",
                            LogPrivate::volume(&share_name),
                            LogPrivate::file_path(&smb_path_owned),
                            delivered
                        );
                        break;
                    }
                    chunk = download.next_chunk() => match chunk {
                        Some(Ok(bytes)) => {
                            delivered += bytes.len() as u64;
                            // The last byte. smb2 put the CLOSE on the wire before
                            // handing it out; only its answer is left. Registered
                            // before the consumer can have it.
                            let last = delivered == total_size;
                            if last {
                                closing = Some(share.closing_reads.begin(&smb_path_owned));
                            }
                            if chunk_tx.send(Ok(bytes)).await.is_err() {
                                // Consumer dropped; stop pumping.
                                break;
                            }
                            if last {
                                finished = true;
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            share.note_error(ran_on, &e);
                            // The session went, not the file: carry on from the
                            // last byte the consumer got, on whichever session is
                            // up next (`resume.rs`). A consumer that goes away
                            // meanwhile stops the wait.
                            let resumed = match (ends_the_session(&e), download.checkpoint()) {
                                (true, Some(checkpoint)) => tokio::select! {
                                    biased;
                                    _ = &mut cancel_rx => break,
                                    () = chunk_tx.closed() => break,
                                    resumed = share.resume_download(&smb_path_owned, &checkpoint, via_link) => Some(resumed),
                                },
                                _ => None,
                            };
                            let failure = match resumed {
                                Some(Ok(Resumed { download: carried_on, ran_on: now_on, on: lease_on })) => {
                                    download = carried_on;
                                    ran_on = now_on;
                                    // The dead session's hold goes; the new one's is kept
                                    // to the task's end, which is all it's for.
                                    drop(std::mem::replace(&mut on, lease_on));
                                    continue;
                                }
                                // The resume's own answer: the file changed or went.
                                Some(Err(Some(refusal))) => refusal,
                                // No session came back in time: the loss stands.
                                Some(Err(None)) | None => e,
                            };
                            warn!(
                                "SmbVolume::download(share={:?}, path={:?}): chunk error: {}",
                                LogPrivate::volume(&share_name),
                                LogPrivate::file_path(&smb_path_owned),
                                failure
                            );
                            let _ = chunk_tx.send(Err(map_smb_error(failure, &display_path))).await;
                            break;
                        }
                        None => break, // download complete (a file that shrank ends here)
                    }
                }
            }
            // End the consumer's stream NOW, before the CLOSE's answer: waiting
            // for it cost every streamed file one round trip. That's safe for a
            // move that deletes the source next: the delete waits for `closing`.
            drop(chunk_tx);
            if finished && let Err(e) = finish_download(download).await {
                // The consumer has every byte, so there's nobody to hand this to.
                // A dead connection still counts for the session's state.
                share.note_error(ran_on, &e);
                debug!(
                    "SmbVolume::download(share={:?}, path={:?}): CLOSE after the last byte: {}",
                    LogPrivate::volume(&share_name),
                    LogPrivate::file_path(&smb_path_owned),
                    e
                );
            }
            drop(closing);
            // `download` drops here. After the last byte its handle is closed
            // (the CLOSE is out); a download stopped earlier (cancel, consumer
            // gone, error) queues its CLOSE on the drop (smb2 0.32.0).
            // `conn`, `tree`, and the lease's hold drop here: the transfer
            // session closes once its last hold is gone and it has idled.
        });

        let opened = match opened_rx.await {
            Ok(Ok(opened)) => opened,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(VolumeError::IoError {
                    message: "SMB download task terminated before reporting size".into(),
                    raw_os_error: None,
                });
            }
        };

        let carried = CarriedMetadata {
            created: opened.created,
            flags: opened.flags,
            xattrs: opened.xattrs.values(),
            ..CarriedMetadata::default()
        };
        let mut stream = ChannelReadStream::new(chunk_rx, cancel_tx, StreamLength::Known(opened.total_size))
            .with_modified_at(opened.modified_at);
        if let Some(version) = opened.version {
            stream = stream.with_source_version(SourceVersion::new(SmbSourceVersion(version)));
        }
        Ok(match opened.xattrs {
            SourceXattrs::Names(names) => stream.with_metadata_outline(CarriedMetadata {
                xattrs: names,
                ..carried.clone()
            }),
            SourceXattrs::None | SourceXattrs::Values(_) => stream,
        }
        .with_metadata(carried))
    }
}

/// A download carried on after a reconnect, with the lease it now rides.
struct Resumed {
    download: FileDownload<'static>,
    ran_on: RanOn,
    /// Keeps the new session's hold (on the transfer session, the one that
    /// stops its idle close).
    on: LeaseOn,
}

impl SmbVolumeInner {
    /// Carries a download that lost its session on from `checkpoint`, on the
    /// next session up (`resume.rs`): smb2 reopens the file read-only, proves
    /// it's still the version the download was reading, and reads from the
    /// first byte the consumer hasn't had. `via_link` resumes it through the
    /// DFS link it was opened through.
    ///
    /// `Err(Some(_))` is the resume's own answer (`FileChanged`, the file
    /// gone), already through the funnel; `Err(None)` means no session came
    /// back in time, and the caller reports the loss it had.
    async fn resume_download(
        &self,
        smb_path: &str,
        checkpoint: &DownloadCheckpoint,
        via_link: bool,
    ) -> Result<Resumed, Option<smb2::Error>> {
        let mut tries = Reattach::new(self);
        while tries.next().await.is_ok() {
            let Ok(TransferLease { tree, conn, ran_on, on }) = self.transfer_lease().await else {
                continue;
            };
            let resumed = match via_link {
                true => match self
                    .resume_link_download(ran_on, &on, &tree, smb_path, checkpoint)
                    .await
                {
                    Some(resumed) => resumed,
                    // A reconnect replaced the lease's session meanwhile.
                    None => continue,
                },
                false => resume_file_download(Arc::clone(&tree), conn, checkpoint).await,
            };
            match resumed {
                Ok(download) => {
                    info!(
                        "SmbVolume::download(volume={:?}, path={:?}): resumed at byte {} after a reconnect",
                        self.volume_id,
                        LogPrivate::file_path(smb_path),
                        checkpoint.offset
                    );
                    return Ok(Resumed { download, ran_on, on });
                }
                // The new session went too: funnel it and wait for the next.
                Err(e) if ends_the_session(&e) => {
                    self.note_error(ran_on, &e);
                }
                Err(e) => {
                    self.note_error(ran_on, &e);
                    return Err(Some(e));
                }
            }
        }
        Err(None)
    }
}

/// The download's open, listing the file's streams in the same frame when
/// `xattrs` asks.
async fn open_download<'a>(
    tree: &'a smb2::client::Tree,
    conn: &'a mut smb2::client::Connection,
    smb_path: &str,
    xattrs: XattrAsk,
) -> Result<FileDownload<'a>, smb2::Error> {
    match xattrs.lists() {
        true => tree.download_with_streams(conn, smb_path).await,
        false => tree.download(conn, smb_path).await,
    }
}

#[cfg(test)]
#[path = "read_stream_test.rs"]
mod read_stream_test;
