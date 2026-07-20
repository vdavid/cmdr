//! MTP streaming file transfer operations (bounded-window reads and streamed uploads).

use log::debug;
use mtp_rs::{ByteRange, MtpDevice, NewObjectInfo, ObjectHandle, StorageId, WindowedDownload};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use super::errors::{MtpConnectionError, is_stale_handle_rejection, map_mtp_error};
use super::{MTP_TIMEOUT_SECS, MtpConnectionManager, acquire_device_lock, normalize_mtp_path};

/// Cached state for a bounded-window MTP read.
///
/// `open_read_session` resolves the object handle and builds an mtp-rs
/// [`WindowedDownload`] ONCE under the device lock; each window then pays only
/// `acquire_device_lock` + one `GetPartialObject64` (no per-window handle/storage
/// re-resolve). The `WindowedDownload` OWNS the window bookkeeping — total size,
/// current offset, per-window clamp, EOF, short-read advance, and the
/// 0-byte-before-EOF stall guard — so Cmdr no longer hand-rolls any of it.
///
/// Cmdr keeps `device_arc` for one reason: `WindowedDownload::next_window` reaches
/// the PTP session DIRECTLY (it holds its own `Arc<MtpDeviceInner>`) and does NOT
/// take Cmdr's per-device lock. The foreground-priority scheduler relies on every
/// device op taking that lock for its turn, so each window read MUST run under
/// `acquire_device_lock` — see [`read_next_window`](MtpConnectionManager::read_next_window).
pub(crate) struct MtpReadSession {
    device_arc: Arc<Mutex<MtpDevice>>,
    windowed: WindowedDownload,
}

impl MtpReadSession {
    /// Full object size in bytes (anchors progress and the EOF check). Read by
    /// the volume backend's read stream.
    pub(crate) fn total_size(&self) -> u64 {
        self.windowed.size()
    }

    /// Bytes delivered so far (the offset of the next window).
    pub(crate) fn bytes_read(&self) -> u64 {
        self.windowed.offset()
    }
}

impl MtpConnectionManager {
    /// Opens a bounded-window read of a file: resolves the object handle and
    /// builds an mtp-rs [`WindowedDownload`] (which reads `total_size` via one
    /// `get_object_info`) ONCE under the device lock, returning an
    /// [`MtpReadSession`] the caller feeds to [`read_next_window`] per window. The
    /// lock is released before returning, so nothing is held between windows.
    /// `window_size` is the per-window `GetPartialObject64` `max_bytes`
    /// (`MTP_READ_WINDOW` in production; tests shrink it); `offset` is the
    /// starting byte (0 for a fresh read, non-zero to resume).
    ///
    /// Takes NO `foreground_guard`: a transfer is a *background* user of the
    /// device gate (it yields TO foreground), so raising `foreground_pending`
    /// here would make the copy contend with itself. See
    /// `mtp/connection/DETAILS.md` § "Bounded-window reads".
    ///
    /// [`read_next_window`]: Self::read_next_window
    pub async fn open_read_session(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
        offset: u64,
        window_size: u32,
    ) -> Result<MtpReadSession, MtpConnectionError> {
        debug!(
            "MTP open_read_session: device={}, storage={}, path={}, offset={}",
            device_id, storage_id, path, offset
        );

        // Get the device and resolve path to handle.
        let (device_arc, object_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;
            let handle = self.resolve_path_to_handle(entry, storage_id, path)?;
            (Arc::clone(&entry.device), handle)
        };

        let windowed = {
            let device = acquire_device_lock(&device_arc, device_id, "open_read_session").await?;
            let storage = tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                device.storage(StorageId(u64::from(storage_id))),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?;

            tokio::time::timeout(
                Duration::from_secs(MTP_TIMEOUT_SECS),
                storage.download_windowed(object_handle, ByteRange::From(offset), window_size),
            )
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?
            .map_err(|e| map_mtp_error(e, device_id))?
        };

        debug!("MTP open_read_session: opened {} bytes for {}", windowed.size(), path);

        Ok(MtpReadSession { device_arc, windowed })
    }

    /// Reads the next bounded window of an object opened with
    /// [`open_read_session`], or `Ok(None)` at EOF. Acquires the per-device lock
    /// for just this one `GetPartialObject64` (released on return), so the PTP
    /// session is free between windows for a foreground listing/nav.
    ///
    /// The window bookkeeping — offset tracking, clamp-to-remaining, EOF
    /// detection, advance-by-bytes-actually-returned (a short read mid-file is
    /// legal), and surfacing a 0-byte-before-EOF stall as an error — lives in
    /// mtp-rs's [`WindowedDownload::next_window`]. Cmdr owns only the LOCK:
    /// `next_window` reaches the PTP session directly and does NOT take Cmdr's
    /// per-device lock, so it MUST run under `acquire_device_lock` — which is
    /// exactly what this method does. Calling `next_window` without the lock would
    /// let a concurrent listing (which holds the lock) and this read drive the
    /// same USB session, desyncing it and breaking the scheduler serialization.
    ///
    /// Takes NO `foreground_guard` (a transfer is a background gate user; see
    /// `open_read_session`).
    ///
    /// Drop-safety: if this future is dropped mid-flight (task abort, device
    /// disconnect), mtp-rs's `TransactionScope` flags the pipe and the next op
    /// drains it under the operation lock (one ~300 ms self-heal), so an aborted
    /// window never permanently desyncs the session. This is what makes the
    /// buffered-window model safe to abort at any point.
    ///
    /// [`open_read_session`]: Self::open_read_session
    pub async fn read_next_window(
        &self,
        session: &mut MtpReadSession,
        device_id: &str,
    ) -> Result<Option<Vec<u8>>, MtpConnectionError> {
        let _device = acquire_device_lock(&session.device_arc, device_id, "read_window").await?;
        let outcome = tokio::time::timeout(Duration::from_secs(MTP_TIMEOUT_SECS), session.windowed.next_window())
            .await
            .map_err(|_| MtpConnectionError::Timeout {
                device_id: device_id.to_string(),
            })?;
        match outcome {
            Some(Ok(bytes)) => Ok(Some(bytes)),
            Some(Err(e)) => Err(map_mtp_error(e, device_id)),
            None => Ok(None),
        }
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
    /// * `filename` - Name for the new file
    /// * `size` - Total size in bytes
    /// * `data_stream` - Chunk stream that mtp-rs consumes lazily as the USB
    ///   transfer drains it. Don't pre-collect the source into a `Vec`; the
    ///   point of the stream is to keep the working set bounded for huge files.
    pub async fn upload_from_stream<S>(
        &self,
        device_id: &str,
        storage_id: u32,
        dest_folder: &str,
        filename: &str,
        size: u64,
        data_stream: S,
    ) -> Result<u64, MtpConnectionError>
    where
        S: futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Unpin + Send,
    {
        // Foreground priority for the whole upload: mtp-rs drains `data_stream`
        // within this call, so the guard covers the entire transfer (and the
        // nested `refresh_dir_handle` re-list, which takes its own guard).
        let _fg = self.foreground_guard(device_id).await;

        debug!(
            "MTP upload_from_stream: device={}, storage={}, dest={}/{}, size={}",
            device_id, storage_id, dest_folder, filename, size,
        );

        // Get device and resolve parent folder
        let (device_arc, parent_handle) = {
            let devices = self.devices.lock().await;
            let entry = devices.get(device_id).ok_or_else(|| MtpConnectionError::NotConnected {
                device_id: device_id.to_string(),
            })?;

            let parent = if dest_folder.is_empty() {
                ObjectHandle::ROOT
            } else {
                self.resolve_path_to_handle(entry, storage_id, dest_folder)?
            };
            (Arc::clone(&entry.device), parent)
        };

        let device = acquire_device_lock(&device_arc, device_id, "upload_from_stream").await?;

        // Get the storage
        let storage = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS),
            device.storage(StorageId(u64::from(storage_id))),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?
        .map_err(|e| map_mtp_error(e, device_id))?;

        // Create object info for the upload
        let object_info = NewObjectInfo::file(filename, size);

        let parent_opt = if parent_handle == ObjectHandle::ROOT {
            None
        } else {
            Some(parent_handle)
        };

        let upload_result = tokio::time::timeout(
            Duration::from_secs(MTP_TIMEOUT_SECS * 10),
            storage.upload(parent_opt, object_info, data_stream),
        )
        .await
        .map_err(|_| MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        })?;

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
                    if let Err(delete_err) = storage.delete(partial).await {
                        log::warn!(
                            target: "mtp_upload",
                            "Failed to delete partial object after upload error (device={device_id}, dest={dest_folder}/{filename}): {delete_err}"
                        );
                    } else {
                        log::debug!(
                            target: "mtp_upload",
                            "Deleted partial object after upload error (device={device_id}, dest={dest_folder}/{filename})"
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
                        "SendObjectInfo rejected for {dest_folder}/{filename} on {device_id}: cached parent handle is stale (device re-keyed). Refreshing handles and signaling a one-shot retry."
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
                    "Upload failed for {dest_folder}/{filename} on {device_id}: {:?}",
                    upload_err.source
                );
                return Err(map_mtp_error(upload_err.source, device_id));
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
                storage_cache.path_to_handle.insert(new_path.clone(), new_handle);
            }
        }

        // Invalidate the parent directory's listing cache
        let dest_folder_path = normalize_mtp_path(dest_folder);
        self.invalidate_listing_cache(device_id, storage_id, &dest_folder_path)
            .await;

        debug!(
            "MTP upload_from_stream complete: {} bytes to {}/{}",
            size, dest_folder, filename
        );

        Ok(size)
    }
}
