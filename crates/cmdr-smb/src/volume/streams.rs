//! Streaming read support for SMB: the producer behind a `ChannelReadStream`, the
//! single-chunk `InlineReadStream`, and the `open_smb_download_stream`
//! primitive that the streaming `Volume` methods (in `volume_impl`) build on.
//! Also the inherent `write_from_stream_impl` body that the `write_from_stream`
//! trait method in `volume_impl` delegates to.

use super::SmbVolume;
use super::mapping::map_smb_error;
use super::session::update_state_on_smb_error;
use cmdr_fs::volume::{
    ChannelReadStream, MutationEvent, StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode,
};
use log::{debug, warn};
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
/// negotiated params: the SMB2 floor, same reasoning as `ASSUMED_MAX_WRITE`.
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

/// The `max_write_size` to assume when the session hasn't reported its
/// negotiated params. Every SMB2 dialect negotiates at least 64 KiB, so this is
/// the conservative floor rather than a guess.
pub(super) const ASSUMED_MAX_WRITE: u64 = 65536;

/// THE condition for the compound CREATE+WRITE+FLUSH+CLOSE fast path: the write
/// fits `limit`, and an empty file has no WRITE to compound with, so it goes to
/// the streaming writer.
///
/// One definition on purpose. `write_is_single_shot` (the transfer layer's
/// staging exemption) answers with it on smb2's `quick_write_limit`, and
/// `write_from_stream_impl` branches on it with [`one_frame_write_limit`]. If a
/// promised write could ever take the streaming path, a force-quit mid-write
/// would leave a truncated file at the user's real filename — the 2026-07-31
/// wedge over again (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`).
///
/// The limit is what the uplink moves in 250 ms, a fixed budget for how long a
/// listing may wait behind one frame (apart from the headroom smb2's adaptive
/// write-behind window learns): one upload chunk (`smb2::UPLOAD_CHUNK_SIZE`,
/// 512 KiB) on a connection that hasn't measured its uplink, then `rate × 250 ms`
/// at the uplink's measured capacity, capped at `compound_write_limit`
/// (`max_write`, lowered to what half the credit window funds). A bigger frame
/// carries the whole file with no progress, queued ahead of every listing on
/// the connection: at 375 KB/s a `stat` waited 23 s
/// behind an 8 MiB upload, 1.5 s behind a streamed one (smb2's
/// `benchmarks/read-ahead/results/adaptive-uploads.md`, smb2 0.25.0, 2026-09-23).
/// smb2 owns that arithmetic.
pub(super) fn fits_one_compound_write(limit: u64, size: u64) -> bool {
    size > 0 && size <= limit
}

/// The limit `write_from_stream` sends one compound frame up to, which must
/// cover every size `write_is_single_shot` promised.
///
/// The promise reads smb2's `quick_write_limit`, which moves between the promise
/// and the write: the upload rate it's built on changes with every upload on the
/// connection and expires after 30 s, and the credit window can shrink under it.
/// So re-reading it here could send a promised write down the streaming path.
/// Only a promised write targets the user's real name, though (every other one
/// lands on a `.cmdr-tmp-*`), which splits it cleanly:
///
/// - A staging temp takes today's limit: anything bigger streams, which is safe
///   on a temp, and a frame the window refuses streams too.
/// - The real name keeps `max_write`, the most any promise could have covered
///   (`quick_write_limit` never exceeds it). A frame the window refuses there
///   fails the write with nothing on the wire, and nothing at the name.
pub(super) fn one_frame_write_limit(dest_is_scratch: bool, max_write: u64, quick_write_limit: u64) -> u64 {
    if dest_is_scratch {
        quick_write_limit.min(max_write)
    } else {
        max_write
    }
}

/// Whether `dest` is one of Cmdr's scratch names (a `.cmdr-tmp-*` staging
/// temp), which a partial write can never make look like the user's file.
fn is_scratch_name(dest: &Path) -> bool {
    dest.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(cmdr_fs::staging::is_staging_temp_name)
}

/// Whether a compound write failed AFTER the server had created the file.
///
/// smb2 reports which command in the chain returned the non-success NTSTATUS. A
/// `Create` failure means nothing of ours reached the destination path (and any
/// file already sitting there is untouched, so it must NOT be deleted); anything
/// later means the CREATE landed, truncating the name to whatever the server
/// accepted. A transport error says nothing either way, so it counts as "not
/// ours to clean up".
fn create_succeeded_but_write_failed<T>(result: &Result<T, smb2::Error>) -> bool {
    matches!(result, Err(smb2::Error::Protocol { command, .. }) if *command != smb2::types::Command::Create)
}

/// Opens the streaming writer `mode` asks for: `FileCreate` for a name that
/// must be new, `FileOverwriteIf` for one the caller may replace.
async fn open_file_writer(
    tree: &Arc<smb2::client::Tree>,
    conn: smb2::client::Connection,
    path: &str,
    mode: WriteMode,
) -> Result<smb2::client::stream::FileWriter, smb2::Error> {
    match mode {
        WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => tree.create_file_writer_exclusive(conn, path).await,
        WriteMode::CreateOrReplace => tree.create_file_writer(conn, path).await,
    }
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
}

impl InlineReadStream {
    pub(super) fn new(data: Vec<u8>, modified_at: Option<std::time::SystemTime>) -> Self {
        let total_size = data.len() as u64;
        Self {
            data: Some(data),
            total_size,
            bytes_read: 0,
            modified_at,
        }
    }
}

/// What a streaming download's producer learned from opening the file.
struct OpenedDownload {
    total_size: u64,
    /// The server's `LastWriteTime`, from the same CREATE response.
    modified_at: Option<std::time::SystemTime>,
}

/// The times a copy stamps on what it wrote: the source's `LastWriteTime` and
/// nothing else, so the server keeps its own creation and access times. `None`
/// for a source with no date, which leaves the server's.
fn source_times(modified: Option<std::time::SystemTime>) -> Option<smb2::FileTimes> {
    modified.map(|t| smb2::FileTimes::new().set_modified(t))
}

/// Stamps the source's date on a streaming writer's OWN handle, before
/// `finish()`: the server rewrites the date when a writing handle closes, so a
/// path stamp while this one is open would lose to that close. Best effort: a
/// date that won't set leaves the server's, and the copy goes on.
async fn stamp_writer(writer: &mut smb2::client::stream::FileWriter, times: smb2::FileTimes, smb_path: &str) {
    if let Err(e) = writer.set_times(times).await {
        warn!("SmbVolume::write_from_stream: path={smb_path:?} keeps the server's date, not the source's: {e}");
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
        let (tree, conn) = self.clone_session().await?;

        // The open reports the size and the date together: both ride on the
        // download's own CREATE response, so the date costs no round trip.
        let (opened_tx, opened_rx) = tokio::sync::oneshot::channel::<Result<OpenedDownload, VolumeError>>();
        let (chunk_tx, chunk_rx) =
            tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let state_arc = Arc::clone(&self.inner.state);
        let retirement = Arc::clone(&self.inner.retirement);
        let volume_id = self.inner.volume_id.clone();
        let share_name = self.inner.share_name.clone();
        let smb_path_owned = smb_path.to_string();
        // Computed here rather than in the task: `to_display_path` needs `&self`,
        // which the spawned producer deliberately doesn't capture.
        let display_path = self.to_display_path(smb_path);
        // The producer outlives this call and reports a mid-stream session loss,
        // so it carries its own clone of the host.
        let host = self.inner.host().clone();

        self.inner.host().runtime().spawn(async move {
            // The task owns its `Connection` clone and an `Arc<Tree>` reference.
            // No lock is held, so other tasks can spawn in parallel and each
            // drive their own download on a fresh `Connection` clone, all
            // multiplexed over the same SMB session by smb2's receiver task.
            let mut conn = conn;
            let mut download = match tree.download(&mut conn, &smb_path_owned).await {
                Ok(d) => d,
                Err(e) => {
                    update_state_on_smb_error(&host, &state_arc, &retirement, &volume_id, &e);
                    warn!(
                        "SmbVolume::download(share={:?}, path={:?}): {}",
                        share_name, smb_path_owned, e
                    );
                    let _ = opened_tx.send(Err(map_smb_error(e, &display_path)));
                    return;
                }
            };

            let total_size = download.size();
            let opened = OpenedDownload {
                total_size,
                modified_at: download.info().and_then(|info| info.modified.to_system_time()),
            };
            if opened_tx.send(Ok(opened)).is_err() {
                // Caller dropped the stream before receiving size. Drop download
                // cleanly (Drop logs a may-leak debug line; the handle is released
                // when the SMB session closes).
                return;
            }

            let mut finished = false;
            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        debug!(
                            "SmbVolume::download(share={:?}, path={:?}): cancelled after {} bytes",
                            share_name, smb_path_owned, download.bytes_received()
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
                            update_state_on_smb_error(&host, &state_arc, &retirement, &volume_id, &e);
                            warn!(
                                "SmbVolume::download(share={:?}, path={:?}): chunk error: {}",
                                share_name, smb_path_owned, e
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
                update_state_on_smb_error(&host, &state_arc, &retirement, &volume_id, &e);
                debug!(
                    "SmbVolume::download(share={:?}, path={:?}): CLOSE after the last byte: {}",
                    share_name, smb_path_owned, e
                );
            }
            // `download` drops here. After the last byte its handle is closed
            // (the CLOSE is out); a download stopped earlier (cancel, consumer
            // gone, error) leaves its handle open until the SMB session ends.
            // `conn` and `tree` drop here: the `Arc<Connection>` inner and the
            // `Arc<Tree>` unwind when every concurrent task finishes.
        });

        let opened = match opened_rx.await {
            Ok(Ok(opened)) => opened,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(VolumeError::IoError {
                    message: "SMB download task terminated before reporting size".to_string(),
                    raw_os_error: None,
                });
            }
        };

        Ok(
            ChannelReadStream::new(chunk_rx, cancel_tx, StreamLength::Known(opened.total_size))
                .with_modified_at(opened.modified_at),
        )
    }

    /// The negotiated `max_write_size` for the live session, or `None` when
    /// there isn't one.
    ///
    /// Reads the client's own connection rather than cloning a session: the
    /// clones `clone_session` hands out share the connection's negotiated
    /// params, so this is the same number the upload will see, for the price of
    /// a brief uncontended mutex and no wire traffic. Only tests size a payload
    /// off it, so a release build without `testing` leaves it out.
    #[cfg(any(test, feature = "testing"))]
    pub(super) async fn negotiated_max_write(&self) -> Option<u64> {
        let guard = self.inner.client.lock().await;
        guard.as_ref().and_then(|c| c.params()).map(|p| p.max_write_size as u64)
    }

    /// The largest write worth sending as one compound frame on the live
    /// session right now (smb2's `quick_write_limit`, see
    /// [`fits_one_compound_write`]), or `None` when there isn't a session. Same
    /// no-clone, no-wire read as [`negotiated_max_write`](Self::negotiated_max_write).
    async fn quick_write_limit(&self) -> Option<u64> {
        let guard = self.inner.client.lock().await;
        guard
            .as_ref()
            .filter(|c| c.params().is_some())
            .map(|c| c.connection().quick_write_limit())
    }

    /// Inherent body for the `write_is_single_shot` trait method (thin delegator
    /// in `volume_impl`): whether a write of `size` bytes takes the compound
    /// fast path below, which is what makes it all-or-nothing.
    pub(super) async fn write_is_single_shot_impl(&self, size: u64) -> bool {
        match self.quick_write_limit().await {
            Some(limit) => fits_one_compound_write(limit, size),
            // No live session: no promise. The transfer stages, as it would for
            // any backend without the guarantee.
            None => false,
        }
    }

    /// Inherent body for the `write_from_stream` trait method (thin delegator in `volume_impl`).
    pub(super) fn write_from_stream_impl<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(cmdr_fs::volume::StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        // Lock-free streaming write path.
        //
        // Both branches below drive the upload on a cloned `Connection`
        // (cheap `Arc::clone`) and an `Arc<Tree>`. The client mutex is
        // held only for the few microseconds of `clone_session()`, never
        // for the upload itself. With smb2 0.9's owned `FileWriter`, N
        // concurrent `write_from_stream` calls on one `SmbVolume`
        // pipeline N WRITE chains over a single SMB session: smb2's
        // receiver task multiplexes responses by `MessageId`.
        //
        // This collapses the historical two-phase pattern (brief
        // `clone_session` for the fast-path → drop → long
        // session-mutex hold for the streaming fallback) into a single
        // clone. The old shape deadlocked under sustained concurrent
        // pressure; the regression test
        // `smb_integration_concurrent_streaming_writes_no_deadlock`
        // pins this shape.
        Box::pin(async move {
            let smb_path = self.to_smb_path(dest)?;

            debug!(
                "SmbVolume::write_from_stream: share={:?}, path={:?}, length={length:?}",
                self.inner.share_name, smb_path
            );

            // Acquire a cloned session once, up front. Both the compound
            // fast-path and the streaming fallback drive their write on
            // this same clone — no second `clone_session` needed.
            let (tree, mut conn) = self.clone_session().await?;
            let times = source_times(stream.modified_at());

            // Best-effort delete of a partial file on a FRESH cloned session.
            // Once a `FileWriter` is open and bytes have streamed into it, an
            // early error (source read error, `write_chunk` / `finish`
            // failure) would otherwise leave a half-written file at the real
            // destination name (AGENTS.md principle #4: a failed copy must not
            // leave corrupt bytes under the user's intended name). The delete
            // runs on a fresh session because the writer's own connection may
            // be gone. The caller MUST close the leaked write handle first
            // (`writer.abort()` where the writer is still owned), else this
            // delete hits a sharing violation against the still-open handle.
            let delete_partial = || async {
                if let Ok((tree_for_delete, mut conn_for_delete)) = self.clone_session().await {
                    let _ = tree_for_delete.delete_file(&mut conn_for_delete, &smb_path).await;
                }
            };

            // Compound fast-path: when the caller promised a size that fits
            // in one WRITE, drain the source stream into a buffer and send
            // CREATE+WRITE+FLUSH+CLOSE as a single compound frame (1 RTT
            // instead of 4). Small files are the hot case; we fall through
            // to the streaming writer for anything larger.
            //
            // The frame is also what makes this write ALL-OR-NOTHING, which is
            // why `write_is_single_shot` lets the transfer layer skip its
            // `.cmdr-tmp-*` staging for exactly these writes: the server either
            // gets the whole length-prefixed frame and runs all four ops, or
            // discards it and creates nothing. ❌ So keep the drained buffer on
            // this path whenever it still fits one WRITE — dropping into the
            // multi-round-trip streaming writer after promising one shot is what
            // would put a truncated file at the user's real filename.
            //
            // Which limit applies depends on the destination
            // (`one_frame_write_limit`): a staging temp follows the quick-write
            // limit as it is now, the user's real name keeps the limit any
            // promise could have been made under.
            let dest_is_scratch = is_scratch_name(dest);
            let bytes_written = 'write: {
                let max_write = conn
                    .params()
                    .map(|p| p.max_write_size as u64)
                    .unwrap_or(ASSUMED_MAX_WRITE);
                let limit = one_frame_write_limit(dest_is_scratch, max_write, conn.quick_write_limit());
                if length.known().is_some_and(|size| fits_one_compound_write(limit, size)) {
                    let size = length.known().expect("the compound branch requires a known length");
                    let mut buffer = Vec::with_capacity(size as usize);
                    while let Some(chunk_result) = stream.next_chunk().await {
                        // Compound drain buffers in memory; no writer/handle
                        // is open yet, so a source error here can't leave a
                        // partial on the server. Just propagate.
                        let chunk = chunk_result?;
                        buffer.extend_from_slice(&chunk);
                        // Fire progress per chunk AND honor cancellation, so
                        // the fast-path has the same cancel/progress contract
                        // as the streaming fallback below. Cancel here aborts
                        // before the compound WRITE touches the wire: the
                        // destination never sees a partial file.
                        if on_progress(cmdr_fs::volume::StreamWriteProgress {
                            bytes_written: buffer.len() as u64,
                            expected_length: length,
                        })
                        .is_break()
                        {
                            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                        }
                    }
                    // The drained bytes, not the promised count, decide: a
                    // source that yielded short still goes out as one frame.
                    if fits_one_compound_write(limit, buffer.len() as u64) {
                        debug!(
                            "SmbVolume::write_from_stream: using compound fast-path ({} bytes)",
                            buffer.len()
                        );
                        // `CreateNew` asks the server for `FileCreate`, so a name
                        // someone else took while we drained is refused whole
                        // (`STATUS_OBJECT_NAME_COLLISION` on the CREATE, nothing
                        // opened, nothing written) and never replaced. That
                        // refusal is a CREATE failure, so the cleanup below leaves
                        // their file alone.
                        let write_result = match mode {
                            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                                tree.write_file_compound_exclusive(&mut conn, &smb_path, &buffer).await
                            }
                            WriteMode::CreateOrReplace => tree.write_file_compound(&mut conn, &smb_path, &buffer).await,
                        };
                        if dest_is_scratch && matches!(write_result, Err(smb2::Error::CreditStarvation { .. })) {
                            // The window shrank after the limit was read, and
                            // smb2 refused the frame before anything reached the
                            // wire. A staging temp can take the bytes streamed
                            // (the streaming writer below honors `mode` too); the
                            // real name can't, so that one surfaces the error
                            // below with nothing written, whichever `mode`.
                            debug!(
                                "SmbVolume::write_from_stream: the credit window can't fund one compound write; streaming the drained {} bytes instead",
                                buffer.len()
                            );
                        } else {
                            if create_succeeded_but_write_failed(&write_result) {
                                // The server created (hence truncated) the file and
                                // then refused the bytes: out of space, over quota,
                                // a lost lease. What's left is a 0-byte file that
                                // may be wearing the user's real filename, since
                                // this write was allowed to skip staging. Take it
                                // away. A CREATE failure needs no cleanup: nothing
                                // was created, and any existing file at that name
                                // is still untouched, so a delete there would be
                                // data loss.
                                delete_partial().await;
                            }
                            let written =
                                self.handle_smb_result("write_from_stream(compound)", &smb_path, write_result)?;
                            // The frame's CLOSE already stamped the server's own
                            // date, so this goes by path, after it. Best effort,
                            // like `stamp_writer`.
                            if let Some(times) = times
                                && let Err(e) = tree.set_times(&mut conn, &smb_path, times).await
                            {
                                warn!(
                                    "SmbVolume::write_from_stream: path={smb_path:?} keeps the server's date, not the source's: {e}"
                                );
                            }
                            break 'write written;
                        }
                    }
                    // The source yielded MORE than one frame can carry (it
                    // reported a smaller size than it had), or the window
                    // refused the frame for a staging temp. Feed the drained
                    // buffer through the streaming writer on the same cloned
                    // connection. No lock acquired; this is the rare path.
                    debug!(
                        "SmbVolume::write_from_stream: {} drained bytes ({} expected) don't go out as one frame; falling back to streaming writer",
                        buffer.len(),
                        size
                    );
                    let writer_result = open_file_writer(&tree, conn, &smb_path, mode).await;
                    let mut writer = self.handle_smb_result("write_from_stream(open)", &smb_path, writer_result)?;
                    if !buffer.is_empty() {
                        let write_result = writer.write_chunk(&buffer).await;
                        if let Err(ve) =
                            self.handle_smb_result("write_from_stream(write_chunk)", &smb_path, write_result)
                        {
                            // Writer still owned: abort (closes the leaked
                            // handle) then delete the partial, mirroring the
                            // cancel branch, then propagate the original error.
                            let _ = writer.abort().await;
                            delete_partial().await;
                            return Err(ve);
                        }
                    }
                    // The source signalled end-of-stream by returning None
                    // above (we exited the drain loop). No further chunks.
                    if let Some(times) = times {
                        stamp_writer(&mut writer, times, &smb_path).await;
                    }
                    // `finish()` consumes the writer, so on failure the
                    // handle is already gone (best-effort delete only).
                    let finish_result = writer.finish().await;
                    if let Err(ve) = self.handle_smb_result("write_from_stream(finish)", &smb_path, finish_result) {
                        delete_partial().await;
                        return Err(ve);
                    }
                    break 'write buffer.len() as u64;
                }

                // Streaming path for large / unknown-size writes. Drives the
                // owned `FileWriter` on the cloned `Connection` directly —
                // no client mutex is held while WRITEs are in flight, so N
                // concurrent large copies pipeline over one SMB session.
                let writer_result = open_file_writer(&tree, conn, &smb_path, mode).await;
                let mut writer = self.handle_smb_result("write_from_stream(open)", &smb_path, writer_result)?;

                loop {
                    let chunk = match stream.next_chunk().await {
                        None => break,
                        Some(Ok(chunk)) => chunk,
                        Some(Err(e)) => {
                            // Source read error with the writer already open:
                            // abort (closes the leaked handle), delete the
                            // partial, then propagate the original error.
                            let _ = writer.abort().await;
                            delete_partial().await;
                            return Err(e);
                        }
                    };
                    if chunk.is_empty() {
                        continue;
                    }

                    let write_result = writer.write_chunk(&chunk).await;
                    if let Err(ve) = self.handle_smb_result("write_from_stream(write_chunk)", &smb_path, write_result) {
                        let _ = writer.abort().await;
                        delete_partial().await;
                        return Err(ve);
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
                    if on_progress(cmdr_fs::volume::StreamWriteProgress {
                        bytes_written: writer.bytes_written(),
                        expected_length: length,
                    }) == std::ops::ControlFlow::Break(())
                    {
                        // Abort drains in-flight WRITE responses and closes the
                        // handle without the server-side fsync that `finish()`
                        // would force (we're about to delete the partial file
                        // anyway). Dropping directly would leave stale responses
                        // on the connection and poison the next op.
                        let _ = writer.abort().await;
                        // Best-effort delete of the partial file on its own
                        // cloned connection (the writer's connection is gone).
                        delete_partial().await;
                        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                    }
                }

                if let Some(times) = times {
                    stamp_writer(&mut writer, times, &smb_path).await;
                }
                // `finish()` consumes the writer; on failure the handle is
                // already gone, so we can only best-effort delete the partial.
                let finish_result = writer.finish().await;
                let confirmed = match self.handle_smb_result("write_from_stream(finish)", &smb_path, finish_result) {
                    Ok(confirmed) => confirmed,
                    Err(ve) => {
                        delete_partial().await;
                        return Err(ve);
                    }
                };

                // `finish()` drains the whole window, so the last chunks are confirmed
                // only now. Without this call the file's progress would stop up to a
                // window short of its size and never reach 100%. Its `ControlFlow` is
                // deliberately ignored: the bytes are committed and the handle closed,
                // so there is nothing left here for a cancel to stop.
                let _ = on_progress(cmdr_fs::volume::StreamWriteProgress {
                    bytes_written: confirmed,
                    expected_length: length,
                });

                confirmed
            };

            // Patch the listing cache from local knowledge so the destination
            // pane sees the new file without waiting for a CHANGE_NOTIFY
            // round-trip. The SMB watcher has a loss window between
            // consecutive `next_events()` calls; relying on it alone left
            // bulk cross-volume copies showing only a subset of the just-
            // copied files until the user navigated away and back.
            if let (Some(parent), Some(name)) = (dest.parent(), dest.file_name())
                && let Some(parent_display) = self.display_path_for(parent)
            {
                self.notify_mutation(
                    &self.inner.volume_id,
                    &parent_display,
                    MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(bytes_written)
        })
    }
}

#[cfg(test)]
#[path = "streams_test.rs"]
mod streams_test;
