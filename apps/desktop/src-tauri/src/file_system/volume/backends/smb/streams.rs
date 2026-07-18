//! Streaming read support for SMB: the channel-backed `SmbReadStream`, the
//! single-chunk `InlineReadStream`, and the `open_smb_download_stream`
//! primitive that the streaming `Volume` methods (in `volume_impl`) build on.
//! Also the inherent `write_from_stream_impl` body that the `write_from_stream`
//! trait method in `volume_impl` delegates to.

use super::*;

/// Backpressure window for the chunk channel. With smb2's ~512 KB pipelined
/// chunks, 4 slots keep peak memory at a few MB regardless of file size.
pub(super) const SMB_STREAM_CHANNEL_CAPACITY: usize = 4;

/// Streaming reader for SMB files, backed by a background producer task.
///
/// The producer task owns an `OwnedMutexGuard` over the smb2 session and drives
/// an `smb2::FileDownload`, sending each chunk down an mpsc channel. The
/// consumer (this struct) just reads from the channel. This avoids buffering
/// the whole file in memory; peak is bounded by the channel capacity.
///
/// Dropping the stream before it's fully consumed sends a cancel signal so
/// the producer can stop early and release the SMB session lock.
pub(super) struct SmbReadStream {
    // Fields are `pub(super)` (not private) because the `smb_test` sibling module
    // builds an `SmbReadStream` directly to test the consumer side in isolation.
    pub(super) rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, VolumeError>>,
    pub(super) cancel: Option<tokio::sync::oneshot::Sender<()>>,
    pub(super) total_size: u64,
    pub(super) bytes_read: u64,
}

impl Drop for SmbReadStream {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            // Best-effort: if the producer already finished, recv side is dropped
            // and the send is a no-op.
            let _ = tx.send(());
        }
    }
}

impl VolumeReadStream for SmbReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let chunk = self.rx.recv().await?;
            if let Ok(ref bytes) = chunk {
                self.bytes_read += bytes.len() as u64;
            }
            Some(chunk)
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
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
}

impl InlineReadStream {
    pub(super) fn new(data: Vec<u8>) -> Self {
        let total_size = data.len() as u64;
        Self {
            data: Some(data),
            total_size,
            bytes_read: 0,
        }
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

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
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
    /// caller-facing `SmbReadStream`.
    ///
    /// This is the single streaming-read primitive for `SmbVolume`. The
    /// cross-volume streaming path (`open_read_stream`) goes through here, so
    /// no path has to buffer whole files in memory.
    pub(super) async fn open_smb_download_stream(&self, smb_path: &str) -> Result<SmbReadStream, VolumeError> {
        let (tree, conn) = self.clone_session().await?;

        let (size_tx, size_rx) = tokio::sync::oneshot::channel::<Result<u64, VolumeError>>();
        let (chunk_tx, chunk_rx) =
            tokio::sync::mpsc::channel::<Result<Vec<u8>, VolumeError>>(SMB_STREAM_CHANNEL_CAPACITY);
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let state_arc = Arc::clone(&self.state);
        let volume_id = self.volume_id.clone();
        let share_name = self.share_name.clone();
        let smb_path_owned = smb_path.to_string();

        tokio::spawn(async move {
            // The task owns its `Connection` clone and an `Arc<Tree>` reference.
            // No lock is held, so other tasks can spawn in parallel and each
            // drive their own download on a fresh `Connection` clone, all
            // multiplexed over the same SMB session by smb2's receiver task.
            let mut conn = conn;
            let mut download = match tree.download(&mut conn, &smb_path_owned).await {
                Ok(d) => d,
                Err(e) => {
                    update_state_on_smb_error(&state_arc, &volume_id, &e);
                    warn!(
                        "SmbVolume::download(share={}, path={}): {}",
                        share_name, smb_path_owned, e
                    );
                    let _ = size_tx.send(Err(map_smb_error(e)));
                    return;
                }
            };

            let total_size = download.size();
            if size_tx.send(Ok(total_size)).is_err() {
                // Caller dropped the stream before receiving size. Drop download
                // cleanly (Drop logs a may-leak debug line; the handle is released
                // when the SMB session closes).
                return;
            }

            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        debug!(
                            "SmbVolume::download(share={}, path={}): cancelled after {} bytes",
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
                        }
                        Some(Err(e)) => {
                            update_state_on_smb_error(&state_arc, &volume_id, &e);
                            warn!(
                                "SmbVolume::download(share={}, path={}): chunk error: {}",
                                share_name, smb_path_owned, e
                            );
                            let _ = chunk_tx.send(Err(map_smb_error(e))).await;
                            break;
                        }
                        None => break, // download complete
                    }
                }
            }
            // `download` drops here (releases SMB file handle at connection close).
            // `conn` and `tree` drop here: the `Arc<Connection>` inner and the
            // `Arc<Tree>` unwind when every concurrent task finishes.
        });

        let total_size = match size_rx.await {
            Ok(Ok(size)) => size,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(VolumeError::IoError {
                    message: "SMB download task terminated before reporting size".to_string(),
                    raw_os_error: None,
                });
            }
        };

        Ok(SmbReadStream {
            rx: chunk_rx,
            cancel: Some(cancel_tx),
            total_size,
            bytes_read: 0,
        })
    }

    /// Inherent body for the `write_from_stream` trait method (thin delegator in `volume_impl`).
    pub(super) fn write_from_stream_impl<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
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
            let smb_path = self.to_smb_path(dest);

            debug!(
                "SmbVolume::write_from_stream: share={}, path={:?}, size={}",
                self.share_name, smb_path, size
            );

            // Acquire a cloned session once, up front. Both the compound
            // fast-path and the streaming fallback drive their write on
            // this same clone — no second `clone_session` needed.
            let (tree, conn) = self.clone_session().await?;

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
            // to the streaming writer for anything larger or when the source
            // returns short.
            let bytes_written = 'write: {
                if size > 0 {
                    let max_write = conn.params().map(|p| p.max_write_size).unwrap_or(65536) as u64;
                    if size <= max_write {
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
                            if on_progress(buffer.len() as u64, size).is_break() {
                                return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
                            }
                        }
                        if buffer.len() as u64 == size {
                            debug!(
                                "SmbVolume::write_from_stream: using compound fast-path ({} bytes)",
                                buffer.len()
                            );
                            let mut conn = conn;
                            let write_result = tree.write_file_compound(&mut conn, &smb_path, &buffer).await;
                            break 'write self.handle_smb_result("write_from_stream(compound)", write_result)?;
                        }
                        // Size mismatch: feed the already-drained buffer through
                        // the streaming writer on the same cloned connection.
                        // No lock acquired; this is the rare path.
                        debug!(
                            "SmbVolume::write_from_stream: compound fast-path source yielded {} bytes, expected {}; falling back to streaming writer",
                            buffer.len(),
                            size
                        );
                        let writer_result = tree.create_file_writer(conn, &smb_path).await;
                        let mut writer = self.handle_smb_result("write_from_stream(open)", writer_result)?;
                        if !buffer.is_empty() {
                            let write_result = writer.write_chunk(&buffer).await;
                            if let Err(ve) = self.handle_smb_result("write_from_stream(write_chunk)", write_result) {
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
                        // `finish()` consumes the writer, so on failure the
                        // handle is already gone (best-effort delete only).
                        let finish_result = writer.finish().await;
                        if let Err(ve) = self.handle_smb_result("write_from_stream(finish)", finish_result) {
                            delete_partial().await;
                            return Err(ve);
                        }
                        break 'write buffer.len() as u64;
                    }
                }

                // Streaming path for large / unknown-size writes. Drives the
                // owned `FileWriter` on the cloned `Connection` directly —
                // no client mutex is held while WRITEs are in flight, so N
                // concurrent large copies pipeline over one SMB session.
                let writer_result = tree.create_file_writer(conn, &smb_path).await;
                let mut writer = self.handle_smb_result("write_from_stream(open)", writer_result)?;

                let mut bytes_read = 0u64;

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
                    if let Err(ve) = self.handle_smb_result("write_from_stream(write_chunk)", write_result) {
                        let _ = writer.abort().await;
                        delete_partial().await;
                        return Err(ve);
                    }

                    bytes_read += chunk.len() as u64;

                    if on_progress(bytes_read, size) == std::ops::ControlFlow::Break(()) {
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

                // `finish()` consumes the writer; on failure the handle is
                // already gone, so we can only best-effort delete the partial.
                let finish_result = writer.finish().await;
                if let Err(ve) = self.handle_smb_result("write_from_stream(finish)", finish_result) {
                    delete_partial().await;
                    return Err(ve);
                }

                bytes_read
            };

            // Patch the listing cache from local knowledge so the destination
            // pane sees the new file without waiting for a CHANGE_NOTIFY
            // round-trip. The SMB watcher has a loss window between
            // consecutive `next_events()` calls; relying on it alone left
            // bulk cross-volume copies showing only a subset of the just-
            // copied files until the user navigated away and back.
            if let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) {
                let parent_display = PathBuf::from(self.to_display_path(&self.to_smb_path(parent)));
                self.notify_mutation(
                    &self.volume_id,
                    &parent_display,
                    MutationEvent::Created(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(bytes_written)
        })
    }
}
