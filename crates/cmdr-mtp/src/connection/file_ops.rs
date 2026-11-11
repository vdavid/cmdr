//! MTP streaming file transfer operations: streamed reads (windowed, or one
//! held-open transfer), ranged reads, and streamed uploads.

use log::debug;
use mtp_rs::{
    ByteRange, DEFAULT_CANCEL_TIMEOUT, DownloadPacing, FileDownload, MtpDevice, NewObjectInfo, ObjectHandle, Storage,
    StorageId,
};

use super::dates::{mtp_datetime_from_system_time, system_time_from_mtp_datetime};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tokio::sync::{Mutex, OwnedMutexGuard};

use super::errors::{MtpConnectionError, is_stale_handle_rejection};
use super::scheduler::{DevicePriorityGate, ReadHold};
use super::{MtpConnectionManager, acquire_device_lock, acquire_device_lock_owned, normalize_mtp_path};
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::rt::Runtime;

/// A streamed read of one file: an mtp-rs [`FileDownload`] plus the device lock
/// it needs, for as long as it needs it.
///
/// mtp-rs reads the file one of two ways, and this session doesn't choose:
///
/// - **Windows** (the device has a partial read that reaches the file's end):
///   one `GetPartialObject64` per window (about 2 s of transfer in
///   production, see [`ReadPacing`]), and the PTP session is free between
///   them, so [`read_next_window`] gives the device lock back whenever a
///   window ends. A window longer than `call_limit` spans several calls and
///   keeps the lock between them, exactly like the held-open path below.
/// - **One held-open `GetObject`** (no partial read, or only the 32-bit one and
///   a file past 4 GiB): the PTP session is busy from open to EOF. The session
///   then OWNS the device lock across calls (`held`) for exactly as long as
///   [`FileDownload::holds_session`] says, while each call still returns at
///   most `call_limit`, so memory stays bounded.
///
/// Giving the lock back mid-transfer would deadlock: a foreground op takes
/// Cmdr's lock and blocks inside mtp-rs's own operation lock, waiting for the
/// transfer, while the transfer waits for Cmdr's lock. `connection/DETAILS.md`
/// § "Bounded-window reads" has the whole contract.
///
/// Dropping a session that holds the device cancels its transfer on the
/// runtime and gives the lock back once that's done, so a stopped copy leaves
/// the session clean for the next op. [`cancel`](Self::cancel) is the awaited
/// form.
///
/// [`read_next_window`]: MtpConnectionManager::read_next_window
pub(crate) struct MtpReadSession {
    device_arc: Arc<Mutex<MtpDevice>>,
    /// `None` once spent: cancelled, or handed to the drop-time cancel.
    download: Option<FileDownload>,
    /// The device lock, while the download holds the PTP session between calls.
    held: Option<HeldDevice>,
    /// Where the `ReadHold` comes from. `None` for a device that vanished
    /// between the resolve and the open, which only makes the hold a no-op.
    gate: Option<DevicePriorityGate>,
    /// Where the drop-time cancel runs (`VolumeHost::runtime`, ❌ never
    /// `tokio::spawn`).
    runtime: Runtime,
    /// The most one call returns (one more transfer chunk at worst).
    call_limit: u32,
    total_size: u64,
    modified: Option<SystemTime>,
    /// File offset of the next byte this session hands out. Kept here rather
    /// than read from the download, which also counts bytes of a call that
    /// failed partway and never reached the caller.
    delivered: u64,
    /// The object this reads, for a refusal of one of its windows.
    path: String,
}

/// The device lock a read session keeps between calls, plus the gate's mark
/// that it does (so the transfer doesn't yield to an op queued behind it).
struct HeldDevice {
    guard: OwnedMutexGuard<MtpDevice>,
    _hold: Option<ReadHold>,
}

/// What a read needs from the device entry, cloned out under the registry lock
/// so the read never re-enters `devices` while it owns the device lock.
struct ReadTarget {
    device_arc: Arc<Mutex<MtpDevice>>,
    gate: DevicePriorityGate,
    storage_cache: Arc<RwLock<HashMap<u32, Arc<Storage>>>>,
    storage_id: u32,
    #[cfg(any(test, feature = "testing"))]
    storage_lookups: Arc<std::sync::atomic::AtomicUsize>,
}

impl ReadTarget {
    /// The storage handle, from the per-device cache or, on a miss, one
    /// `GetStorageInfo` that fills it. The caller holds the device lock.
    async fn storage(&self, device: &MtpDevice) -> Result<Arc<Storage>, mtp_rs::Error> {
        let cached = self
            .storage_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(&self.storage_id).cloned());
        if let Some(storage) = cached {
            return Ok(storage);
        }
        #[cfg(any(test, feature = "testing"))]
        self.storage_lookups.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let storage = Arc::new(device.storage(StorageId(u64::from(self.storage_id))).await?);
        if let Ok(mut cache) = self.storage_cache.write() {
            cache.insert(self.storage_id, Arc::clone(&storage));
        }
        Ok(storage)
    }
}

/// What an upload creates: the `SendObjectInfo` half, ahead of the bytes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UploadedFile<'a> {
    /// Name for the new file.
    pub(crate) name: &'a str,
    /// Total size in bytes; the data stream must deliver exactly this many.
    pub(crate) size: u64,
    /// The source's modification date, sent as `DateModified` so the copy
    /// keeps it. `None` sends none and the device stamps its own.
    pub(crate) modified: Option<SystemTime>,
}

impl MtpReadSession {
    /// Full object size in bytes (anchors progress and the EOF check). Read by
    /// the volume backend's read stream.
    pub(crate) fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Bytes delivered so far, as a file offset (the offset of the next window).
    pub(crate) fn bytes_read(&self) -> u64 {
        self.delivered
    }

    /// The file's `DateModified`, from the `ObjectInfo` the open already
    /// fetched, so it costs no round trip. `None` when the device sent none.
    pub(crate) fn modified_at(&self) -> Option<SystemTime> {
        self.modified
    }

    /// Stops the read and gives the device back, awaited. Between windows this
    /// sends nothing; mid-transfer mtp-rs stops it the device's way (on Android
    /// it reads the rest of the current transfer, which is the price of a
    /// held-open one). Afterwards the session is spent: the next call is EOF.
    pub(crate) async fn cancel(&mut self) {
        let held = self.held.take();
        if let Some(mut download) = self.download.take()
            && let Err(e) = download.cancel(DEFAULT_CANCEL_TIMEOUT).await
        {
            debug!("MTP read cancel of {}: {}", LogPrivate::file_path(&self.path), e);
        }
        drop(held);
    }

    /// Keeps `device` across calls while the download holds the PTP session,
    /// else lets it go.
    fn keep_device_while_busy(&mut self, device: OwnedMutexGuard<MtpDevice>) {
        if self.download.as_ref().is_some_and(FileDownload::holds_session) {
            self.held = Some(HeldDevice {
                guard: device,
                _hold: self.gate.as_ref().map(DevicePriorityGate::read_holding_device),
            });
        }
    }
}

impl Drop for MtpReadSession {
    /// A session dropped mid-transfer (a copy that stopped, a pane that closed)
    /// cancels it on the runtime and keeps the device lock until the cancel is
    /// done, so no other op starts inside the abandoned transfer. Between
    /// windows there's nothing to stop.
    fn drop(&mut self) {
        let Some(held) = self.held.take() else {
            return;
        };
        let Some(mut download) = self.download.take() else {
            return;
        };
        let path = std::mem::take(&mut self.path);
        self.runtime.spawn(async move {
            if let Err(e) = download.cancel(DEFAULT_CANCEL_TIMEOUT).await {
                debug!("MTP read cancel on drop of {}: {}", LogPrivate::file_path(&path), e);
            }
            drop(held);
        });
    }
}

/// How a read session spreads a file over device transactions, and the most
/// one [`read_next_window`] call returns (one more transfer chunk at worst).
///
/// The two are separate because a time-paced window has no byte size known up
/// front: on a fast link one can run to tens of MiB, and `call_limit` is what
/// keeps each call's buffer bounded anyway. Production is
/// [`MTP_READ_PACING`](super::MTP_READ_PACING).
///
/// [`read_next_window`]: MtpConnectionManager::read_next_window
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadPacing {
    /// What mtp-rs sizes each `GetPartialObject64` by.
    pub(crate) download: DownloadPacing,
    /// The most one call returns, in bytes, on either path.
    pub(crate) call_limit: u32,
}

impl ReadPacing {
    /// Windows of exactly `bytes`, one per call: what a test uses to make a
    /// small fixture span several windows on the virtual device, whose
    /// instant transfers would otherwise teach the adaptive pacer an enormous
    /// speed.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) const fn fixed_window(bytes: u32) -> Self {
        Self {
            download: DownloadPacing::FixedWindow(bytes),
            call_limit: bytes,
        }
    }
}

/// What [`pull_window`] reads from: mtp-rs's [`FileDownload`], or a cell's
/// scripted stand-in. The virtual device hands a whole held-open object over as
/// ONE chunk where real USB streams 64 KiB reads, so the per-call memory bound
/// is only observable against a script (`read_window_test.rs`).
pub(super) trait ChunkSource {
    /// The next chunk, or `None` at the end of the download.
    async fn next_chunk(&mut self) -> Option<Result<bytes::Bytes, mtp_rs::Error>>;
    /// Whether a transfer is in flight, holding the device's PTP session.
    fn holds_session(&self) -> bool;
}

impl ChunkSource for FileDownload {
    async fn next_chunk(&mut self) -> Option<Result<bytes::Bytes, mtp_rs::Error>> {
        FileDownload::next_chunk(self).await
    }

    fn holds_session(&self) -> bool {
        FileDownload::holds_session(self)
    }
}

/// One call's worth of a download: chunks until the session frees (a window
/// ended) or `limit` bytes are in, whichever comes first, so a held-open
/// transfer, or a time-paced window longer than `limit`, returns at most
/// `limit` plus one chunk. Empty at EOF. The caller holds the device lock.
///
/// `capacity` is a guess: a time-paced window can end well short of it. A
/// buffer that came back under half full is shrunk, because the read stream
/// hands it on as `Bytes`, which keeps the whole allocation alive while the
/// copy pipeline queues it.
pub(super) async fn pull_window<S: ChunkSource>(
    source: &mut S,
    limit: usize,
    capacity: usize,
) -> Result<Vec<u8>, mtp_rs::Error> {
    let mut window = Vec::with_capacity(capacity);
    while let Some(chunk) = source.next_chunk().await {
        window.extend_from_slice(&chunk?);
        if !source.holds_session() || window.len() >= limit {
            break;
        }
    }
    if window.len() < window.capacity() / 2 {
        window.shrink_to_fit();
    }
    Ok(window)
}

impl MtpConnectionManager {
    /// The device entry's read-side handles, or `NotConnected`.
    async fn read_target(&self, device_id: &str, storage_id: u32) -> Result<ReadTarget, MtpConnectionError> {
        let devices = self.devices.lock().await;
        let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
            device_id: device_id.to_string(),
        })?;
        Ok(ReadTarget {
            device_arc: Arc::clone(&entry.device),
            gate: entry.priority_gate.clone(),
            storage_cache: Arc::clone(&entry.storage_cache),
            storage_id,
            #[cfg(any(test, feature = "testing"))]
            storage_lookups: Arc::clone(&entry.storage_lookups),
        })
    }

    /// Opens a streamed read of a file from `offset`: resolves the object
    /// handle, then, under the device lock, starts an mtp-rs [`FileDownload`]
    /// paced by `pacing.download` (which reads `total_size` via one
    /// `get_object_info`; the storage handle comes from the per-device cache).
    /// The caller feeds the returned [`MtpReadSession`] to [`read_next_window`].
    ///
    /// On a windowed device the lock is released before returning. On a device
    /// that needs one held-open transfer, mtp-rs starts it here and the session
    /// keeps the lock (see [`MtpReadSession`]). `pacing` is `MTP_READ_PACING`
    /// in production; tests use small fixed windows.
    ///
    /// Takes NO `foreground_guard`: a transfer is a *background* user of the
    /// device gate (it yields TO foreground), so raising `foreground_pending`
    /// here would make the copy contend with itself. See
    /// `mtp/connection/DETAILS.md` § "Bounded-window reads".
    ///
    /// [`read_next_window`]: Self::read_next_window
    pub(crate) async fn open_read_session(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
        offset: u64,
        pacing: ReadPacing,
    ) -> Result<MtpReadSession, MtpConnectionError> {
        debug!(
            "MTP open_read_session: device={}, storage={}, path={}, offset={}",
            device_id,
            storage_id,
            LogPrivate::file_path(path),
            offset
        );

        let object_handle = self.resolve_path_to_handle(device_id, storage_id, path).await?;
        let target = self.read_target(device_id, storage_id).await?;
        let device = acquire_device_lock_owned(&target.device_arc, device_id, "open_read_session").await?;
        let storage = target
            .storage(&device)
            .await
            .map_err(|e| self.map_device_read_error(e, device_id, path))?;
        let download = storage
            .download_with_pacing(object_handle, ByteRange::From(offset), pacing.download)
            .await
            .map_err(|e| self.map_device_read_error(e, device_id, path))?;

        debug!(
            "MTP open_read_session: opened {} bytes for {} ({})",
            download.size(),
            LogPrivate::file_path(path),
            if download.holds_session() {
                "held-open"
            } else {
                "windowed"
            }
        );

        let mut session = MtpReadSession {
            device_arc: target.device_arc,
            total_size: download.size(),
            modified: download.modified().and_then(system_time_from_mtp_datetime),
            delivered: download.offset(),
            download: Some(download),
            held: None,
            gate: Some(target.gate),
            runtime: self.host().runtime(),
            call_limit: pacing.call_limit,
            path: path.to_string(),
        };
        session.keep_device_while_busy(device);
        Ok(session)
    }

    /// Reads the next part of a session opened with [`open_read_session`]: at
    /// most `call_limit` bytes (one more transfer chunk at worst), or
    /// `Ok(None)` at EOF.
    ///
    /// Under the device lock, it pulls mtp-rs chunks until the download frees
    /// the PTP session (a window ended) or `call_limit` is reached. If the
    /// session is still busy then (a held-open transfer, or a time-paced window
    /// longer than `call_limit`), the lock stays with the session for the next
    /// call; otherwise it's released on return, so a foreground listing/nav
    /// slips in between windows. mtp-rs owns the window
    /// bookkeeping: offsets, clamp-to-remaining, EOF, short reads, and the
    /// 0-byte-before-EOF stall, which surfaces as an error.
    ///
    /// ❌ mtp-rs's download reaches the PTP session DIRECTLY and takes no Cmdr
    /// lock, so every chunk MUST be pulled under the device lock, which is
    /// exactly what this method does. Without it a concurrent listing and this
    /// read would drive the same USB session.
    ///
    /// Takes NO `foreground_guard` (a transfer is a background gate user; see
    /// `open_read_session`).
    ///
    /// Drop-safety: ❌ don't rely on it. If this future IS dropped (a genuine
    /// disconnect), mtp-rs flags the pipe and the next op drains it under its
    /// operation lock, a seatbelt, not a guarantee: some devices don't come
    /// back from an abandoned data phase without a replug. So a copy cancel
    /// stops ISSUING windows rather than dropping one in flight, and nothing
    /// wraps this in a timeout (see the module `CLAUDE.md`).
    ///
    /// [`open_read_session`]: Self::open_read_session
    pub(crate) async fn read_next_window(
        &self,
        session: &mut MtpReadSession,
        device_id: &str,
    ) -> Result<Option<Vec<u8>>, MtpConnectionError> {
        if session.download.is_none() {
            return Ok(None);
        }
        let device = match session.held.take() {
            Some(held) => held.guard,
            None => acquire_device_lock_owned(&session.device_arc, device_id, "read_window").await?,
        };

        let limit = session.call_limit as usize;
        let remaining = session.total_size.saturating_sub(session.delivered);
        let capacity = usize::try_from(remaining).unwrap_or(usize::MAX).min(limit);
        let outcome = match session.download.as_mut() {
            Some(download) => pull_window(download, limit, capacity).await,
            None => Ok(Vec::new()),
        };
        session.keep_device_while_busy(device);

        match outcome {
            Err(e) => Err(self.map_device_read_error(e, device_id, &session.path)),
            Ok(window) if window.is_empty() => Ok(None),
            Ok(window) => {
                session.delivered += window.len() as u64;
                Ok(Some(window))
            }
        }
    }

    /// Reads ONE bounded range of an object in a single `GetPartialObject64`,
    /// with no session setup: the direct path behind `MtpVolume::read_range`.
    ///
    /// A bounded read wants bytes, not a session. Going through
    /// [`open_read_session`](Self::open_read_session) would cost an extra USB
    /// round trip per call, the `GetObjectInfo` a download reads `total_size`
    /// with, to produce bookkeeping the caller discards. This path resolves the
    /// storage from the per-device cache and issues only the read, so an
    /// archive extraction loop pays one round trip per window.
    ///
    /// Returns what the device delivered, which may be SHORT: a legal partial
    /// read mid-file, or fewer bytes near EOF. Callers loop (see
    /// `MtpVolume::read_range`) rather than assume `len` bytes.
    ///
    /// Takes the per-device lock for the one read, like every other device op,
    /// so the foreground-priority scheduler still sees this read take its turn.
    /// Takes NO `foreground_guard` — a read is a background gate user
    /// (see [`open_read_session`](Self::open_read_session)).
    pub(crate) async fn read_range_direct(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
        offset: u64,
        len: u32,
    ) -> Result<Vec<u8>, MtpConnectionError> {
        let object_handle = self.resolve_path_to_handle(device_id, storage_id, path).await?;
        let target = self.read_target(device_id, storage_id).await?;
        let device = acquire_device_lock(&target.device_arc, device_id, "read_range_direct").await?;
        let storage = target
            .storage(&device)
            .await
            .map_err(|e| self.map_device_read_error(e, device_id, path))?;
        storage
            .read_range(object_handle, offset, len)
            .await
            .map_err(|e| self.map_device_read_error(e, device_id, path))
    }

    /// Uploads pre-collected chunks to the MTP device.
    ///
    /// This variant takes already-collected chunks instead of a stream reference,
    /// avoiding nested `block_on` issues when the stream uses `block_on` internally.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `dest_folder` - Destination folder path on device (like "DCIM")
    /// * `file` - The new file's name, size, and date ([`UploadedFile`])
    /// * `data_stream` - Chunk stream that mtp-rs consumes lazily as the USB
    ///   transfer drains it. Don't pre-collect the source into a `Vec`; the
    ///   point of the stream is to keep the working set bounded for huge files.
    pub(crate) async fn upload_from_stream<S>(
        &self,
        device_id: &str,
        storage_id: u32,
        dest_folder: &str,
        file: UploadedFile<'_>,
        data_stream: S,
    ) -> Result<u64, MtpConnectionError>
    where
        S: futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Unpin + Send,
    {
        let UploadedFile {
            name: filename,
            size,
            modified,
        } = file;
        // Foreground priority for the whole upload: mtp-rs drains `data_stream`
        // within this call, so the guard covers the entire transfer (and the
        // nested `refresh_dir_handle` re-list, which takes its own guard).
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP upload_from_stream: device={}, storage={}, dest={}, size={}",
            device_id,
            storage_id,
            LogPrivate::file_path(&Path::new(dest_folder).join(filename)),
            size,
        );

        let parent_handle = self.resolve_path_to_handle(device_id, storage_id, dest_folder).await?;
        let device_arc = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;
            Arc::clone(&entry.device)
        };

        let device = acquire_device_lock(&device_arc, device_id, "upload_from_stream").await?;

        // Get the storage
        let storage = device
            .storage(StorageId(u64::from(storage_id)))
            .await
            .map_err(|e| self.map_device_error(e, device_id))?;

        // `SendObjectInfo` precedes the bytes, so the date has to be known now:
        // a stream answers it from its open.
        let mut object_info = NewObjectInfo::file(filename, size);
        if let Some(date) = modified.and_then(mtp_datetime_from_system_time) {
            object_info = object_info.with_modified(date);
        }

        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let upload_result = storage.upload(parent_opt, object_info, data_stream).await;

        let new_handle = match upload_result {
            Ok(handle) => handle,
            Err(upload_err) => {
                // PTP uploads are two-phase: SendObjectInfo creates the object,
                // then SendObject streams the data. When the data phase fails
                // (genuine error OR user cancel), mtp-rs surfaces the created
                // object as `upload_err.partial` instead of auto-deleting it —
                // the caller owns the cleanup-or-resume decision. cmdr's
                // no-corrupt-artifact policy: best-effort delete the partial so
                // no half-file lingers on the user's phone. This covers the
                // cancel path too: `source` is `Error::Cancelled` and `partial`
                // is `Some`, so a cancelled upload also gets cleaned up, then
                // maps back to `MtpConnectionError::Cancelled` below (cancel
                // classification preserved).
                if let Some(partial) = upload_err.partial {
                    // Best-effort: a failed delete must NOT mask the upload
                    // error. The delete needs a live device/session; if the
                    // device just disconnected, this fails and the partial
                    // lingers (recognizable, nothing we can do with a dead
                    // device) — we log and move on.
                    let dest = Path::new(dest_folder).join(filename);
                    if let Err(delete_err) = storage.delete(partial).await {
                        log::warn!(
                            target: "mtp_upload",
                            "Failed to delete partial object after upload error (device={device_id}, dest={}): {delete_err}",
                            LogPrivate::file_path(&dest)
                        );
                    } else {
                        log::debug!(
                            target: "mtp_upload",
                            "Deleted partial object after upload error (device={device_id}, dest={})",
                            LogPrivate::file_path(&dest)
                        );
                    }
                }
                // A stale cached parent handle (the device re-keyed its handles
                // since this folder was last listed; common on Android when
                // MediaProvider rescans between listing and upload) presents as
                // an `InvalidParentObject` / `InvalidObjectHandle` rejection of
                // `SendObjectInfo`. That's recoverable: refresh the folder's
                // handle and signal the caller to retry once with a fresh source
                // stream. Classify before `source` is moved into the mapper.
                let is_stale = is_stale_handle_rejection(&upload_err.source);

                // Release the device lock before any re-list: `refresh_dir_handle`
                // re-acquires it through `list_directory`, and the tokio Mutex
                // isn't reentrant — holding it here would deadlock the heal.
                drop(storage);
                drop(device);

                if is_stale {
                    log::warn!(
                        target: "mtp_upload",
                        "SendObjectInfo rejected for dir={:?} file={:?} on {device_id}: cached parent handle is stale (device re-keyed). Refreshing handles and signaling a one-shot retry.",
                        LogPrivate::dir_path(dest_folder),
                        LogPrivate::file_name(filename)
                    );
                    self.refresh_dir_handle(device_id, storage_id, Path::new(dest_folder))
                        .await;
                    return Err(MtpConnectionError::StaleParentHandle {
                        device_id: device_id.to_string(),
                        dest_folder: normalize_mtp_path(dest_folder).to_string_lossy().to_string(),
                    });
                }

                // Always surface the original upload error (mapped), never the
                // cleanup outcome. Log it: a bare protocol rejection here would
                // otherwise leave no trace (no `error-report` breadcrumb).
                log::warn!(
                    target: "mtp_upload",
                    "Upload failed for dir={:?} file={:?} on {device_id}: {:?}",
                    LogPrivate::dir_path(dest_folder),
                    LogPrivate::file_name(filename),
                    // allowed-log-private: the device's protocol error, which names no path
                    upload_err.source
                );
                return Err(self.map_device_error(upload_err.source, device_id));
            }
        };

        // Release device lock
        drop(storage);
        drop(device);

        // Update path cache
        let new_path = normalize_mtp_path(dest_folder).join(filename);
        {
            let devices = self.devices.lock().await;
            if let Some(entry) = devices.get(device_id)
                && let Ok(mut cache_map) = entry.path_cache.write()
            {
                let storage_cache = cache_map.entry(storage_id).or_default();
                storage_cache.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let dest_folder_path = normalize_mtp_path(dest_folder);
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path)
            .await;

        debug!(
            "MTP upload_from_stream complete: {} bytes to {}",
            size,
            LogPrivate::file_path(&new_path)
        );

        Ok(size)
    }
}
