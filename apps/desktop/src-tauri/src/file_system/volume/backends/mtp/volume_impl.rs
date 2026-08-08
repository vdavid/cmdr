//! The `Volume` impl for [`MtpVolume`](super::MtpVolume).
//!
//! Split out of `mtp.rs` so neither file carries the whole backend: the parent holds
//! the struct, the cancel bridge, the read stream, and the error mapping; this holds
//! the trait surface. Same module tree, so the private fields stay private to `mtp`.

use super::{
    MtpCancelBridge, MtpReadStream, MtpVolume, map_mtp_error, mtp_read_window, volume_read_stream_to_chunk_stream,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume,
    VolumeError, VolumeReadStream,
};
use crate::mtp::connection::{MtpConnectionError, MtpDeleteScope, connection_manager};
use log::debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

impl Volume for MtpVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        // One USB pipe per device: every storage on a device shares its lane,
        // so two transfers to the same phone serialize. Key by `device_id`
        // (not `volume_id`, which is per-storage) so they collapse to one lane.
        LaneKey::new(self.device_id.clone())
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_with_cancel(path, on_progress, None)
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            #[cfg(test)]
            super::test_hooks::bump_list_directory_call_count();

            let mtp_path = self.to_mtp_path(path);

            let bridge = MtpCancelBridge::open(cancel);
            let cancel_ref = bridge.as_ref().map(MtpCancelBridge::token);

            debug!(
                "MtpVolume::list_directory: device={}, storage={}, input_path={}, mtp_path={}, cancel={}",
                self.device_id,
                self.storage_id,
                path.display(),
                mtp_path,
                cancel_ref.is_some()
            );

            let start = std::time::Instant::now();
            let result = if let Some(on_progress) = on_progress {
                connection_manager()
                    .list_directory_with_progress_and_cancel(
                        &self.device_id,
                        self.storage_id,
                        &mtp_path,
                        on_progress,
                        cancel_ref,
                    )
                    .await
            } else {
                connection_manager()
                    .list_directory_with_cancel(&self.device_id, self.storage_id, &mtp_path, cancel_ref)
                    .await
            };

            match &result {
                Ok(entries) => debug!(
                    "MtpVolume::list_directory: completed in {:?}, {} entries",
                    start.elapsed(),
                    entries.len()
                ),
                Err(e) => debug!(
                    "MtpVolume::list_directory: failed in {:?}, error={:?}",
                    start.elapsed(),
                    e
                ),
            }

            result.map_err(map_mtp_error)
        })
    }

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);
            let bridge = MtpCancelBridge::open(cancel);

            // The per-unit, foreground-yielding scan listing: never holds the USB
            // pipe across the whole folder, so a background scan can't starve
            // foreground nav/copy/delete. See `mtp/connection/directory_ops.rs`.
            connection_manager()
                .list_directory_for_scan(
                    &self.device_id,
                    self.storage_id,
                    &mtp_path,
                    bridge.as_ref().map(MtpCancelBridge::token),
                )
                .await
                .map_err(map_mtp_error)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // MTP has no single-file stat: list the parent directory and find the entry.
            let path_str = path.to_string_lossy();
            if path_str.is_empty() || path_str == "/" || path_str == "." {
                // Root: synthesize a directory entry
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.root.display().to_string(),
                    true,
                    false,
                ));
            }

            let Some(parent) = path.parent() else {
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.root.display().to_string(),
                    true,
                    false,
                ));
            };

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(VolumeError::NotFound(path.display().to_string()));
            };

            let entries = self.list_directory(parent, None).await?;
            entries
                .into_iter()
                .find(|e| e.name == name)
                .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.is_ok() })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.map(|e| e.is_directory) })
    }

    fn supports_watching(&self) -> bool {
        // Return false because MTP has its OWN file watching mechanism that is
        // independent of the listing pipeline. The MtpConnectionManager starts an
        // event loop when a device connects (see start_event_loop) that polls for
        // USB interrupt endpoint events (ObjectAdded/ObjectRemoved/ObjectInfoChanged).
        // These events emit `mtp-directory-changed` directly to the frontend.
        //
        // The `supports_watching()` check in operations.rs is used to decide whether
        // to start the local notify-based file watcher, which only works for POSIX
        // paths. MTP paths like "/DCIM/Camera" don't exist on the local filesystem,
        // so we must return false to prevent the notify watcher from failing.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        false
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        // MTP "watching" is volume-level, not path-level. The MTP event loop is
        // per-device and would report any changes the device emits to any path.
        // So as long as the device is connected, treat every cached listing on
        // this volume as oracle-eligible. Caveat: many MTP devices (cameras
        // especially) never emit per-object events, so `true` means only "the
        // device is reachable and would forward changes if it sent any".
        connection_manager().is_connected(&self.device_id)
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

            // Normalize once so every branch (incl. get_metadata's parent
            // lookup) sees the canonical absolute MTP URL. Callers happily
            // hand us either form depending on which layer they came from.
            let parent_url = self.to_url_path(parent_path);
            let parent_ref = parent_url.as_path();
            // MTP's get_metadata lists the parent dir to find the entry, which is expensive
            // but correct. The MTP event loop (connection/event_loop.rs) also handles
            // change notifications, so this is belt-and-suspenders for self-mutations.
            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_ref.join(name);
                    match self.get_metadata(&entry_path).await {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(&self.volume_id, parent_ref, change);
                        }
                        Err(e) => {
                            debug!(
                                "MtpVolume::notify_mutation: couldn't stat {}: {}",
                                entry_path.display(),
                                e
                            );
                        }
                    }
                }
                MutationEvent::Deleted(name) => {
                    notify_directory_changed(&self.volume_id, parent_ref, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_ref.join(&to);
                    match self.get_metadata(&new_path).await {
                        Ok(entry) => {
                            notify_directory_changed(
                                &self.volume_id,
                                parent_ref,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            debug!(
                                "MtpVolume::notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    /// MTP `create_directory` does NOT error on an existing same-name dir — the
    /// protocol allows same-name sibling objects, so `create_folder` would make a
    /// duplicate. The folder-merge walker pre-checks existence on MTP instead of
    /// trusting `create_directory` to surface `AlreadyExists`.
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        false
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let Some(parent) = path.parent() else {
                return Err(VolumeError::IoError {
                    message: "Cannot create root directory".into(),
                    raw_os_error: None,
                });
            };
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(VolumeError::IoError {
                    message: "Invalid directory name".into(),
                    raw_os_error: None,
                });
            };

            let parent_mtp_path = self.to_mtp_path(parent);
            let folder_name = name.to_string();

            connection_manager()
                .create_folder(&self.device_id, self.storage_id, &parent_mtp_path, &folder_name)
                .await
                .map(|_| ())
                .map_err(map_mtp_error)?;

            self.notify_mutation(&self.volume_id, parent, MutationEvent::Created(name.to_string()))
                .await;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.delete_with_cancel(path, None)
    }

    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            let bridge = MtpCancelBridge::open(cancel);
            let cancel_ref = bridge.as_ref().map(MtpCancelBridge::token);

            // `SingleNode`, because `Volume::delete` means one node on every
            // backend. A caller that wants a tree walks it itself.
            connection_manager()
                .delete_object_with_cancel(
                    &self.device_id,
                    self.storage_id,
                    &mtp_path,
                    MtpDeleteScope::SingleNode,
                    cancel_ref,
                )
                .await
                .map_err(map_mtp_error)?;

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                self.notify_mutation(
                    &self.volume_id,
                    parent,
                    MutationEvent::Deleted(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // MTP doesn't support atomic overwrite, so check for conflicts when not forced.
            if !force && self.exists(to).await {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }

            let from_mtp = self.to_mtp_path(from);
            let to_mtp = self.to_mtp_path(to);

            let from_parent = Path::new(&from_mtp).parent().unwrap_or(Path::new(""));
            let to_parent = Path::new(&to_mtp).parent().unwrap_or(Path::new(""));
            let same_parent = from_parent == to_parent;

            let from_name = Path::new(&from_mtp)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| VolumeError::IoError {
                    message: "Invalid source path".into(),
                    raw_os_error: None,
                })?;
            let to_name =
                Path::new(&to_mtp)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| VolumeError::IoError {
                        message: "Invalid destination path".into(),
                        raw_os_error: None,
                    })?;
            let same_name = from_name == to_name;

            if same_parent {
                // Same directory: just rename
                let new_name = to_name.to_string();
                connection_manager()
                    .rename_object(&self.device_id, self.storage_id, &from_mtp, &new_name)
                    .await
                    .map(|_| ())
                    .map_err(map_mtp_error)?;

                // Notify listing cache about same-directory rename
                if let Some(from_parent_path) = from.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        from_parent_path,
                        MutationEvent::Renamed {
                            from: from_name.to_string(),
                            to: to_name.to_string(),
                        },
                    )
                    .await;
                }
            } else {
                // Different directory: use MTP MoveObject
                let to_parent_str = to_parent.to_string_lossy().to_string();
                connection_manager()
                    .move_object(&self.device_id, self.storage_id, &from_mtp, &to_parent_str)
                    .await
                    .map(|_| ())
                    .map_err(map_mtp_error)?;

                // If the name also changed, rename after moving
                if !same_name {
                    let moved_path = format!(
                        "{}{}{}",
                        to_parent_str,
                        if to_parent_str.is_empty() || to_parent_str.ends_with('/') {
                            ""
                        } else {
                            "/"
                        },
                        from_name
                    );
                    let new_name = to_name.to_string();
                    connection_manager()
                        .rename_object(&self.device_id, self.storage_id, &moved_path, &new_name)
                        .await
                        .map(|_| ())
                        .map_err(map_mtp_error)?;
                }

                // Cross-directory move: remove from source dir, add in dest dir
                if let Some(from_parent_path) = from.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        from_parent_path,
                        MutationEvent::Deleted(from_name.to_string()),
                    )
                    .await;
                }
                if let Some(to_parent_path) = to.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        to_parent_path,
                        MutationEvent::Created(to_name.to_string()),
                    )
                    .await;
                }
            }
            Ok(())
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_impl(path)
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_with_progress(paths, None)
    }

    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_with_progress_impl(paths, on_progress)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.scan_for_conflicts_impl(source_items, dest_path)
    }

    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(5))
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let info = connection_manager()
                .get_device_info(&self.device_id)
                .await
                .ok_or_else(|| {
                    map_mtp_error(MtpConnectionError::NotConnected {
                        device_id: self.device_id.clone(),
                    })
                })?;

            // Find this storage in the device info
            let storage = info.storages.iter().find(|s| s.id == self.storage_id).ok_or_else(|| {
                map_mtp_error(MtpConnectionError::Other {
                    device_id: self.device_id.clone(),
                    message: format!("Storage {} not found", self.storage_id),
                })
            })?;

            Ok(SpaceInfo {
                total_bytes: storage.total_bytes,
                available_bytes: storage.available_bytes,
                used_bytes: storage.total_bytes.saturating_sub(storage.available_bytes),
            })
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // MTP is a single USB bulk transport, so parallel ops would just
        // serialize on the wire with extra overhead.
        1
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }

    // Reads in bounded windows from `offset` forward. Since the copy path
    // (`CheckpointStream`) parks in place between windows rather than releasing +
    // reopening, NOTHING calls this with a non-zero offset anymore; it's reached
    // only via `open_read_stream`'s `offset == 0`. Keep the offset parameter
    // (the resumable primitive is correct and cheap) — don't "clean it up" as
    // dead just because the non-zero path currently has no caller.
    fn open_read_stream_at_offset<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            // The window size (`mtp_read_window()`, shrinkable in tests) and the
            // start `offset` (non-zero resumes a reopened read; see
            // `CheckpointStream`) are baked into the `WindowedDownload` here.
            let session = connection_manager()
                .open_read_session(&self.device_id, self.storage_id, &mtp_path, offset, mtp_read_window())
                .await
                .map_err(map_mtp_error)?;

            Ok(Box::new(MtpReadStream {
                session,
                device_id: self.device_id.clone(),
            }) as Box<dyn VolumeReadStream>)
        })
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if len == 0 {
                return Ok(Vec::new());
            }
            let mtp_path = self.to_mtp_path(path);
            // ONE `GetPartialObject64` per call, straight to the device: no
            // session, no `GetStorageInfo`, no `GetObjectInfo`. A bounded read
            // discards everything those two round trips would produce, and the
            // archive extraction loop issues one of these per 256 KiB, so the
            // saving is the whole extraction. (`open_read_session` stays the
            // right shape for a streaming copy, where one `GetObjectInfo`
            // amortizes over hundreds of windows and anchors progress.)
            let window = u32::try_from(len).unwrap_or(u32::MAX);
            let mut out = Vec::with_capacity(len.min(window as usize));
            while out.len() < len {
                let remaining = u32::try_from(len - out.len()).unwrap_or(u32::MAX);
                let chunk = connection_manager()
                    .read_range_direct(
                        &self.device_id,
                        self.storage_id,
                        &mtp_path,
                        offset + out.len() as u64,
                        remaining,
                    )
                    .await
                    .map_err(map_mtp_error)?;
                // A short read is legal mid-file, so keep asking for the rest.
                // An EMPTY read is the terminator (EOF, or a device with nothing
                // more to give): stop instead of spinning.
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
            }
            Ok(out)
        })
    }

    fn pause_releases_read_stream(&self) -> bool {
        // MTP reads in bounded windows (~8 MiB) and holds the one-per-device PTP
        // session ONLY during a window — between windows the session is free. So a
        // pause has nothing scarce to release: it just stops starting the next
        // window (park-in-place, like every other backend). The phone stays
        // navigable while paused because the copy isn't holding the session.
        false
    }

    fn supports_foreground_yield(&self) -> bool {
        // A running MTP copy reads in bounded windows, so a foreground listing/nav
        // already slips in between windows; this opt-in tells `CheckpointStream`
        // to ALSO not start the next window while foreground work is pending (so
        // the copy doesn't immediately re-grab the device lock and starve it). The
        // yield is "don't start the next window," not a session release. See
        // `CheckpointStream`'s auto-yield arm.
        true
    }

    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { connection_manager().foreground_pending(&self.device_id).await })
    }

    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { connection_manager().background_yield_point(&self.device_id).await })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let dest_folder = dest.parent().map(|p| self.to_mtp_path(p)).unwrap_or_default();
            let filename = dest
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| VolumeError::IoError {
                    message: "Invalid filename".into(),
                    raw_os_error: None,
                })?
                .to_string();

            let chunk_stream = volume_read_stream_to_chunk_stream(stream, size, on_progress);
            let chunk_stream = Box::pin(chunk_stream);

            let bytes_written = connection_manager()
                .upload_from_stream(
                    &self.device_id,
                    self.storage_id,
                    &dest_folder,
                    &filename,
                    size,
                    chunk_stream,
                )
                .await
                .map_err(map_mtp_error)?;

            // Patch the listing cache from local knowledge so the destination
            // pane sees the new file immediately. The MTP USB event loop is
            // unreliable for self-mutations (many devices emit no events at
            // all), so without this the cache would only catch up on the
            // next manual refresh.
            if let Some(parent) = dest.parent() {
                self.notify_mutation(&self.volume_id, parent, MutationEvent::Created(filename))
                    .await;
            }
            Ok(bytes_written)
        })
    }
}
