//! Reading an SMB file as a stream: the producer behind a `ChannelReadStream`
//! (`open_smb_download_stream`, the one streaming-read primitive the `Volume`
//! read methods in `volume_impl` build on), the single-chunk `InlineReadStream`
//! the compound CREATE+READ+CLOSE fast path hands out, and the condition for
//! taking that fast path. Writing a stream into a file is `write_stream`.

use super::SmbVolume;
use super::mapping::{map_smb_error, user_flags_of};
use super::session::SessionLease;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses, UserFlags};
use cmdr_fs::volume::{ChannelReadStream, StreamLength, VolumeError, VolumeReadStream};
use log::{debug, warn};
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
}

/// What a streaming download's producer learned from opening the file.
struct OpenedDownload {
    total_size: u64,
    /// The server's `LastWriteTime`, from the same CREATE response.
    modified_at: Option<std::time::SystemTime>,
    /// The server's `CreationTime`, from the same CREATE response.
    created: Option<std::time::SystemTime>,
    /// Hidden and Locked, from the same CREATE response's attributes.
    flags: UserFlags,
}

/// The last component of an SMB path, which decides whether Samba's Hidden is
/// the name's own (`user_flags_of`).
fn name_of(smb_path: &str) -> &str {
    smb_path.rsplit(['\\', '/']).next().unwrap_or(smb_path)
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
            ..CarriedMetadata::default()
        }
        .restricted_to(wanted)
    }
}

impl SmbVolume {
    /// Opens a streaming download on the given SMB-relative path.
    ///
    /// Briefly locks the client mutex to clone the underlying `Connection`,
    /// releases the lock, then spawns a background task that owns the clone
    /// and drives `Tree::download` on it. Each concurrent call gets its own
    /// cloned `Connection` (all multiplexing frames over the same SMB
    /// session), so N downloads run pipelined instead of serializing on the
    /// session mutex. Chunks flow through a bounded mpsc channel to the
    /// caller-facing [`ChannelReadStream`].
    ///
    /// This is the single streaming-read primitive for `SmbVolume`. The
    /// cross-volume streaming path (`open_read_stream`) goes through here, so
    /// no path has to buffer whole files in memory.
    pub(super) async fn open_smb_download_stream(&self, smb_path: &str) -> Result<ChannelReadStream, VolumeError> {
        let SessionLease { tree, conn, generation } = self.clone_session().await?;

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
            // The task owns its `Connection` clone and an `Arc<Tree>` reference.
            // No lock is held, so other tasks can spawn in parallel and each
            // drive their own download on a fresh `Connection` clone, all
            // multiplexed over the same SMB session by smb2's receiver task.
            let mut conn = conn;
            let mut download = match tree.download(&mut conn, &smb_path_owned).await {
                Ok(d) => d,
                Err(e) => {
                    share.note_smb_error(generation, &e);
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
            let opened = OpenedDownload {
                total_size,
                modified_at: download.info().and_then(|info| info.modified.to_system_time()),
                created: download.info().and_then(|info| info.created.to_system_time()),
                flags: download.info().map_or(UserFlags::NONE, |info| {
                    user_flags_of(info.attributes, name_of(&smb_path_owned))
                }),
            };
            if opened_tx.send(Ok(opened)).is_err() {
                // Caller dropped the stream before receiving size. Dropping the
                // download queues its CLOSE (smb2 0.32.0), so the handle goes too.
                return;
            }

            let mut finished = false;
            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        debug!(
                            "SmbVolume::download(share={:?}, path={:?}): cancelled after {} bytes",
                            LogPrivate::volume(&share_name),
                            LogPrivate::file_path(&smb_path_owned),
                            download.bytes_received()
                        );
                        break;
                    }
                    chunk = download.next_chunk() => match chunk {
                        Some(Ok(bytes)) => {
                            if chunk_tx.send(Ok(bytes)).await.is_err() {
                                // Consumer dropped; stop pumping.
                                break;
                            }
                            if download.bytes_received() == total_size {
                                // The last byte. smb2 put the CLOSE on the wire
                                // before handing it out; only its answer is left.
                                finished = true;
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            share.note_smb_error(generation, &e);
                            warn!(
                                "SmbVolume::download(share={:?}, path={:?}): chunk error: {}",
                                LogPrivate::volume(&share_name),
                                LogPrivate::file_path(&smb_path_owned),
                                e
                            );
                            let _ = chunk_tx.send(Err(map_smb_error(e, &display_path))).await;
                            break;
                        }
                        None => break, // download complete (a file that shrank ends here)
                    }
                }
            }
            // End the consumer's stream NOW, before the CLOSE's answer: waiting
            // for it cost every streamed file one round trip. That's safe for a
            // move that deletes the source next: the CLOSE went out on this
            // connection's socket before the last chunk was handed out, so any
            // request the consumer sends afterwards is behind it on the wire,
            // and the download opened with `FILE_SHARE_DELETE` anyway.
            drop(chunk_tx);
            if finished && let Some(Err(e)) = download.next_chunk().await {
                // The consumer has every byte, so there's nobody to hand this to.
                // A dead connection still counts for the volume's state.
                share.note_smb_error(generation, &e);
                debug!(
                    "SmbVolume::download(share={:?}, path={:?}): CLOSE after the last byte: {}",
                    LogPrivate::volume(&share_name),
                    LogPrivate::file_path(&smb_path_owned),
                    e
                );
            }
            // `download` drops here. After the last byte its handle is closed
            // (the CLOSE is out); a download stopped earlier (cancel, consumer
            // gone, error) queues its CLOSE on the drop (smb2 0.32.0).
            // `conn` and `tree` drop here: the `Arc<Connection>` inner and the
            // `Arc<Tree>` unwind when every concurrent task finishes.
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

        Ok(
            ChannelReadStream::new(chunk_rx, cancel_tx, StreamLength::Known(opened.total_size))
                .with_modified_at(opened.modified_at)
                .with_metadata(CarriedMetadata {
                    created: opened.created,
                    flags: opened.flags,
                    ..CarriedMetadata::default()
                }),
        )
    }
}

#[cfg(test)]
#[path = "read_stream_test.rs"]
mod read_stream_test;
