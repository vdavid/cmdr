//! MTP device event loop for file watching.
//!
//! Polls for MTP device events and emits directory-diff events to the frontend
//! using the unified diff system shared with local file watching.

use log::{debug, info, warn};
use mtp_rs::MtpDevice;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::{Mutex, broadcast};

use mtp_rs::ObjectHandle;

use super::cache::EVENT_DEBOUNCE_MS;
use super::{MtpConnectionManager, connection_manager, normalize_mtp_path};
use crate::file_system::listing::{get_listings_by_volume_prefix, update_listing_entries};
use crate::file_system::{FileEntry, compute_diff};
use crate::ignore_poison::RwLockIgnorePoison;
use std::path::{Path, PathBuf};

impl MtpConnectionManager {
    /// Starts the event polling loop for a connected device.
    ///
    /// This spawns a background task that polls for MTP device events and emits
    /// `mtp-directory-changed` events to the frontend when files change on the device.
    pub(super) fn start_event_loop(&self, device_id: String, device: Arc<Mutex<MtpDevice>>, app: AppHandle) {
        let (shutdown_tx, _) = broadcast::channel(1);

        // Store shutdown sender
        {
            let mut shutdown_map = self.event_loop_shutdown.write_ignore_poison();
            shutdown_map.insert(device_id.clone(), shutdown_tx.clone());
        }

        // Clone for the spawned task
        let device_id_clone = device_id.clone();

        // Spawn the event loop task. It uses connection_manager() to access the debouncer
        // since the debouncer is part of the global singleton.
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_tx.subscribe();

            // Clone the MtpDevice for event polling. MtpDevice is cheaply cloneable (Arc
            // internally) and next_event() reads from the USB interrupt endpoint, which is
            // independent from the bulk endpoints used by file operations. This lets us poll
            // for events WITHOUT holding Cmdr's device mutex, so file operations (copy, move,
            // scan) aren't blocked by event polling.
            let event_device: MtpDevice = device.lock().await.clone();

            debug!("MTP event loop started for device: {}", device_id_clone);

            loop {
                let poll_result = tokio::select! {
                    biased;

                    // Check for shutdown signal first
                    _ = shutdown_rx.recv() => {
                        debug!("MTP event loop shutting down (signal): {}", device_id_clone);
                        break;
                    }

                    // Poll for next event (no device lock needed; interrupt endpoint is independent)
                    result = tokio::time::timeout(Duration::from_secs(5), event_device.next_event()) => {
                        result.unwrap_or(Err(mtp_rs::Error::Timeout))
                    }
                };

                match poll_result {
                    Ok(event) => {
                        Self::handle_device_event(&device_id_clone, event, &app);
                    }
                    Err(mtp_rs::Error::Timeout) => {
                        // No event within timeout period - continue polling
                        // Add a small sleep to avoid tight loop when device is idle
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(mtp_rs::Error::Disconnected) => {
                        info!("MTP device disconnected (event loop): {}", device_id_clone);
                        // Device was unplugged - clean up state and emit event
                        // IMPORTANT: Call handle_device_disconnected to remove from devices registry
                        // so reconnection attempts don't fail with "already connected"
                        connection_manager()
                            .handle_device_disconnected(&device_id_clone, Some(&app))
                            .await;
                        break;
                    }
                    Err(e) => {
                        // Log other errors but continue polling - device might recover
                        warn!("MTP event error for {}: {:?}", device_id_clone, e);
                        // Sleep a bit before retrying to avoid tight error loop
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }

            debug!("MTP event loop exited for device: {}", device_id_clone);
        });

        debug!("MTP event loop spawned for device: {}", device_id);
    }

    /// Stops the event loop for a device.
    pub(super) fn stop_event_loop(&self, device_id: &str) {
        // Remove and signal shutdown
        if let Some(tx) = self.event_loop_shutdown.write_ignore_poison().remove(device_id) {
            let _ = tx.send(()); // Signal shutdown - ignore error if receiver is gone
            debug!("MTP event loop shutdown signaled for device: {}", device_id);
        }

        // Clear debouncer state for this device
        self.event_debouncer.clear(device_id);
    }

    /// Handles a device event and emits to frontend if appropriate.
    fn handle_device_event(device_id: &str, event: mtp_rs::mtp::DeviceEvent, app: &AppHandle) {
        use mtp_rs::mtp::DeviceEvent;

        match event {
            // ObjectAdded / ObjectInfoChanged carry a live handle, so we resolve
            // it to a path and refresh only the affected directory. ObjectRemoved
            // can't resolve (the object is already gone — `GetObjectInfo` fails),
            // so it stays a blanket refresh for the live pane; the index resolves
            // removals via a per-entry stored handle instead.
            //
            // Each branch ALSO feeds the per-volume index (the second consumer):
            // the live pane gets its targeted/blanket refresh, and the persisted
            // index stays in sync so dir sizes are right while the device is Fresh,
            // even with no pane open (mirrors the SMB `notify_directory_changed`
            // dual-consumer wiring).
            DeviceEvent::ObjectAdded { handle } => {
                debug!("MTP object added: {:?} on {}", handle, device_id);
                Self::emit_change_for_handle(device_id, handle, app);
                Self::feed_index_added_or_changed(device_id, handle);
            }
            DeviceEvent::ObjectRemoved { handle } => {
                debug!("MTP object removed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
                Self::feed_index_removed(device_id, handle);
            }
            DeviceEvent::ObjectInfoChanged { handle } => {
                debug!("MTP object changed: {:?} on {}", handle, device_id);
                Self::emit_change_for_handle(device_id, handle, app);
                Self::feed_index_added_or_changed(device_id, handle);
            }
            DeviceEvent::StorageInfoChanged { storage_id } => {
                debug!("MTP storage info changed: {:?} on {}", storage_id, device_id);
            }
            DeviceEvent::StoreAdded { storage_id } => {
                info!("MTP storage added: {:?} on {}", storage_id, device_id);
                let device_id = device_id.to_string();
                let app = app.clone();
                tokio::spawn(async move {
                    connection_manager()
                        .handle_storage_added(&device_id, storage_id.0, &app)
                        .await;
                });
            }
            DeviceEvent::StoreRemoved { storage_id } => {
                info!("MTP storage removed: {:?} on {}", storage_id, device_id);
                let device_id = device_id.to_string();
                let app = app.clone();
                tokio::spawn(async move {
                    connection_manager()
                        .handle_storage_removed(&device_id, storage_id.0, &app)
                        .await;
                });
            }
            DeviceEvent::DeviceInfoChanged => {
                debug!("MTP device info changed: {}", device_id);
            }
            DeviceEvent::DeviceReset => {
                warn!("MTP device reset: {}", device_id);
            }
            DeviceEvent::Unknown { code, params } => {
                debug!("MTP unknown event {:04x} {:?} on {}", code, params, device_id);
            }
        }
    }

    /// Feed an `ObjectAdded` / `ObjectInfoChanged` into the per-volume index, if
    /// any storage on this device is indexed.
    ///
    /// PTP handles are device-wide but storages are separate namespaces, so we
    /// resolve the handle against each INDEXED storage of the device and upsert
    /// into the one where it resolves (the object lives in exactly one). Resolving
    /// the handle on the wrong storage fails cleanly, so a non-matching storage is
    /// skipped. Runs as a spawned task because resolution does USB I/O; the index
    /// writes are enqueued (never blocking the event loop). No-op when the device
    /// has no indexed storage.
    fn feed_index_added_or_changed(device_id: &str, handle: ObjectHandle) {
        let indexed = crate::indexing::registered_mtp_volume_ids_for_device(device_id);
        if indexed.is_empty() {
            return;
        }
        let device_id = device_id.to_string();
        tokio::spawn(async move {
            for volume_id in indexed {
                let Some(storage_id) = crate::mtp::identity::storage_id_of_volume(&volume_id) else {
                    continue;
                };
                match connection_manager()
                    .resolve_object_for_index(&device_id, storage_id, handle)
                    .await
                {
                    Ok(obj) => {
                        crate::indexing::apply_mtp_added_or_changed(
                            &volume_id,
                            crate::indexing::MtpUpsert {
                                path: obj.path,
                                handle: handle.0,
                                is_directory: obj.is_directory,
                                size: obj.size,
                                modified_at: obj.modified_at,
                            },
                        );
                        // The handle resolved on this storage; it can't also live
                        // on another, so we're done.
                        return;
                    }
                    Err(e) => {
                        debug!(
                            "MTP index feed: handle {:?} unresolved on {}:{} ({:?})",
                            handle, device_id, storage_id, e
                        );
                    }
                }
            }
        });
    }

    /// Feed an `ObjectRemoved` into the per-volume index. The object is gone, so
    /// there's no path to resolve — each indexed storage resolves the removal by
    /// its STORED handle (`find_entry_by_inode`); only the storage that indexed
    /// the object has a matching row, the rest are no-ops. Synchronous (DB reads +
    /// writer enqueue only, no USB), so no spawn. No-op without an indexed storage.
    fn feed_index_removed(device_id: &str, handle: ObjectHandle) {
        for volume_id in crate::indexing::registered_mtp_volume_ids_for_device(device_id) {
            crate::indexing::apply_mtp_removed(&volume_id, handle.0);
        }
    }

    /// Handles a pathful PTP change event (`ObjectAdded` / `ObjectInfoChanged`)
    /// by resolving the opaque handle to a path and refreshing ONLY the affected
    /// directory's listing, instead of the blanket all-open-listings refresh.
    ///
    /// PTP handles are device-wide but storages are separate namespaces, so we
    /// don't know up front which storage the handle lives in. We attempt
    /// resolution against each storage that currently has an open listing (the
    /// only storages where a targeted refresh could matter): the first storage
    /// whose resolved parent directory matches an open listing gets a targeted
    /// re-read. On any resolution failure (handle invalid, parent uncached and
    /// the walk fails, timeout) we fall back to the blanket refresh, so an update
    /// is never lost — just less precise.
    fn emit_change_for_handle(device_id: &str, handle: ObjectHandle, app: &AppHandle) {
        let device_id = device_id.to_string();
        let app = app.clone();
        tokio::spawn(async move {
            // Distinct storage IDs with at least one open listing on this device.
            let listings = get_listings_by_volume_prefix(&device_id);
            let mut storage_ids: Vec<u32> = listings
                .iter()
                .filter_map(|(_, volume_id, _, _)| crate::mtp::identity::storage_id_of_volume(volume_id))
                .collect();
            storage_ids.sort_unstable();
            storage_ids.dedup();

            for storage_id in storage_ids {
                match connection_manager()
                    .resolve_handle_to_path(&device_id, storage_id, handle)
                    .await
                {
                    Ok(object_path) => {
                        // The directory that changed is the object's parent (the
                        // folder whose listing shows it). A root-level object's
                        // parent is the storage root, "/".
                        let affected_dir = object_path
                            .parent()
                            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
                        if Self::emit_directory_changed_targeted(&device_id, storage_id, &affected_dir, &app) {
                            // Targeted refresh fired for an open listing; done.
                            return;
                        }
                        // Resolved, but no open listing shows that dir: nothing to
                        // refresh on THIS storage. Keep trying other storages.
                    }
                    Err(e) => {
                        debug!(
                            "MTP targeted refresh: handle {:?} unresolved on {}:{} ({:?})",
                            handle, device_id, storage_id, e
                        );
                    }
                }
            }

            // No storage produced a targeted refresh: fall back to blanket so the
            // update is never dropped (e.g. the change is in a non-open subdir, or
            // resolution failed on every storage).
            Self::emit_directory_changed(&device_id, &app);
        });
    }

    /// Re-reads and diffs ONLY the listing(s) showing `affected_dir` on
    /// `(device_id, storage_id)`. Returns `true` if at least one such listing
    /// exists (a targeted refresh fired), `false` if no open pane shows that dir.
    ///
    /// Goes through the same debouncer as the blanket path so a burst of resolved
    /// events still collapses to one re-read per window.
    fn emit_directory_changed_targeted(device_id: &str, storage_id: u32, affected_dir: &Path, app: &AppHandle) -> bool {
        // Match the affected dir against this device's open listings by their
        // normalized inner MTP path. Listings carry a `mtp://…` or `/`-rooted
        // path; `listing_inner_mtp_path` reduces both to the comparable form.
        let listings: Vec<(String, String, PathBuf, Vec<FileEntry>)> = get_listings_by_volume_prefix(device_id)
            .into_iter()
            .filter(|(_, volume_id, path, _)| {
                crate::mtp::identity::storage_id_of_volume(volume_id) == Some(storage_id)
                    && listing_inner_mtp_path(volume_id, path).as_deref() == Some(affected_dir)
            })
            .collect();

        if listings.is_empty() {
            return false;
        }

        if !connection_manager().event_debouncer.should_emit(device_id) {
            // Within the debounce window: schedule a trailing targeted re-emit so
            // the last event in a burst isn't dropped.
            debug!(
                "MTP targeted refresh: DEBOUNCED for {}:{} dir={}, scheduling trailing emit",
                device_id,
                storage_id,
                affected_dir.display()
            );
            let device_id = device_id.to_string();
            let affected_dir = affected_dir.to_path_buf();
            let app = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(EVENT_DEBOUNCE_MS + 50)).await;
                Self::emit_directory_changed_targeted(&device_id, storage_id, &affected_dir, &app);
            });
            return true;
        }

        debug!(
            "MTP targeted refresh: re-reading {} listing(s) for {}:{} dir={}",
            listings.len(),
            device_id,
            storage_id,
            affected_dir.display()
        );

        let device_id = device_id.to_string();
        tokio::spawn(async move {
            Self::compute_and_emit_diffs(&device_id, listings).await;
        });
        true
    }

    /// Emits directory-diff events for all affected listings (with debouncing).
    ///
    /// Uses the unified diff system shared with local file watching, providing
    /// smooth incremental UI updates without full directory reloads.
    fn emit_directory_changed(device_id: &str, app: &AppHandle) {
        // Check debouncer via the global connection manager.
        // When suppressed, schedule a trailing emit after the debounce window
        // so the last event in a burst is never permanently dropped.
        if !connection_manager().event_debouncer.should_emit(device_id) {
            debug!(
                "MTP event loop: directory change DEBOUNCED for device={} (within {}ms window), scheduling trailing emit",
                device_id, EVENT_DEBOUNCE_MS
            );
            let device_id_owned = device_id.to_string();
            let app_clone = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(EVENT_DEBOUNCE_MS + 50)).await;
                // Re-emit; this goes through the debouncer again (which will pass
                // since the window has expired) to avoid duplicate processing.
                Self::emit_directory_changed(&device_id_owned, &app_clone);
            });
            return;
        }

        // Find all listings for this device (volume IDs like "mtp-123:65537")
        let listings = get_listings_by_volume_prefix(device_id);
        if listings.is_empty() {
            debug!(
                "MTP event loop: no active listings for device={}, skipping diff",
                device_id
            );
            return;
        }

        debug!(
            "MTP event loop: found {} listings for device={}, computing diffs",
            listings.len(),
            device_id
        );

        // Clone what we need for the spawned task
        let device_id_owned = device_id.to_string();

        // Spawn task to re-read directories and compute diffs
        tokio::spawn(async move {
            Self::compute_and_emit_diffs(&device_id_owned, listings).await;
        });
    }

    /// Re-reads MTP directories and emits directory-diff events.
    ///
    /// For each listing belonging to this device:
    /// 1. Extract the storage_id and path from the volume_id and listing path
    /// 2. Re-read the directory from the MTP device
    /// 3. Compute the diff between old and new entries
    /// 4. Update LISTING_CACHE with new entries
    /// 5. Emit directory-diff event
    async fn compute_and_emit_diffs(device_id: &str, listings: Vec<(String, String, PathBuf, Vec<FileEntry>)>) {
        for (listing_id, volume_id, path, old_entries) in listings {
            // Extract storage_id from volume_id (format: "{device_id}:{storage}").
            // rsplit-based parse via identity tolerates a `:` in a serial device id.
            let Some(storage_id) = crate::mtp::identity::storage_id_of_volume(&volume_id) else {
                warn!(
                    "MTP diff: could not parse storage_id from volume_id={}, skipping",
                    volume_id
                );
                continue;
            };

            // The inner MTP path that `list_directory` keys on (for example,
            // "DCIM/Camera"), peeled out of the listing's `mtp://…`-scheme or
            // "/"-rooted cache path.
            let mtp_path = listing_inner_mtp_path(&volume_id, &path)
                .map(|p| p.to_string_lossy().trim_start_matches('/').to_string())
                .unwrap_or_default();

            // Invalidate the MTP listing cache before re-reading so we get fresh data.
            // Must use the normalized MTP path (for example, "/Documents"), not the raw LISTING_CACHE
            // path (for example, "mtp://mtp-device/65537/Documents"), because that's what list_directory
            // uses as the cache key.
            connection_manager()
                .invalidate_listing_cache(device_id, storage_id, &normalize_mtp_path(&mtp_path))
                .await;

            // Re-read the directory from the MTP device
            let new_entries = match connection_manager()
                .list_directory(device_id, storage_id, &mtp_path)
                .await
            {
                Ok(entries) => entries,
                Err(e) => {
                    debug!("MTP diff: failed to re-read directory {}: {:?}, skipping", mtp_path, e);
                    continue;
                }
            };

            // Compute diff
            let changes = compute_diff(&old_entries, &new_entries);
            if changes.is_empty() {
                debug!(
                    "MTP diff: no changes detected for listing_id={}, path={}",
                    listing_id, mtp_path
                );
                continue;
            }

            // Update LISTING_CACHE with new entries
            update_listing_entries(&listing_id, new_entries);

            // Route through the coalescer so bursts of MTP events (large delete,
            // many file copies) don't fire one IPC event per change.
            crate::file_system::listing::diff_emitter::enqueue_diff(&listing_id, changes);
            debug!("MTP diff: enqueued diff for listing_id={}", listing_id);
        }
    }
}

/// Reduces a `LISTING_CACHE` directory path to the inner MTP path, normalized
/// with a leading `/` (for example `/DCIM/Camera`, or `/` for the storage root).
///
/// A listing's stored path is either `mtp://<device>/<storage>/<inner…>` or a
/// plain `/`-rooted inner path. Returns `None` only if a `mtp://` path is
/// malformed (missing the device/storage segments). The leading-`/` form matches
/// what [`MtpConnectionManager::resolve_handle_to_path`](super::MtpConnectionManager::resolve_handle_to_path)
/// produces, so the two are directly comparable for targeted-refresh matching.
fn listing_inner_mtp_path(volume_id: &str, path: &Path) -> Option<PathBuf> {
    let raw = path.to_string_lossy();
    let inner = if let Some(without_scheme) = raw.strip_prefix("mtp://") {
        // "mtp://mtp-0-1/65537/DCIM/Camera" -> "DCIM/Camera"; the root listing
        // "mtp://mtp-0-1/65537" has only two segments -> "".
        let mut parts = without_scheme.splitn(3, '/');
        let _device = parts.next()?;
        let _storage = parts.next()?;
        parts.next().unwrap_or("")
    } else {
        raw.trim_start_matches('/')
    };

    debug_assert!(
        volume_id.starts_with("mtp-"),
        "listing_inner_mtp_path expects an MTP volume_id, got {volume_id}"
    );

    Some(normalize_mtp_path(inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_path_from_mtp_scheme() {
        let p = listing_inner_mtp_path("mtp-0-1:65537", Path::new("mtp://mtp-0-1/65537/DCIM/Camera"));
        assert_eq!(p, Some(PathBuf::from("/DCIM/Camera")));
    }

    #[test]
    fn inner_path_from_mtp_scheme_root() {
        // The storage root listing has no inner segment -> "/".
        let p = listing_inner_mtp_path("mtp-0-1:65537", Path::new("mtp://mtp-0-1/65537"));
        assert_eq!(p, Some(PathBuf::from("/")));
    }

    #[test]
    fn inner_path_from_plain_rooted() {
        let p = listing_inner_mtp_path("mtp-0-1:65537", Path::new("/Documents"));
        assert_eq!(p, Some(PathBuf::from("/Documents")));
    }

    #[test]
    fn inner_path_matches_resolver_form() {
        // The targeted-refresh filter compares this against the resolver's
        // output. A resolved object `/DCIM/IMG.jpg` has affected dir `/DCIM`,
        // which must equal the inner path of an open `/DCIM` listing in both
        // path representations.
        let resolved = PathBuf::from("/DCIM/IMG.jpg");
        let affected_dir = resolved.parent().unwrap().to_path_buf();
        assert_eq!(affected_dir, PathBuf::from("/DCIM"));

        let scheme_listing = listing_inner_mtp_path("mtp-0-1:65537", Path::new("mtp://mtp-0-1/65537/DCIM")).unwrap();
        let plain_listing = listing_inner_mtp_path("mtp-0-1:65537", Path::new("/DCIM")).unwrap();
        assert_eq!(scheme_listing, affected_dir);
        assert_eq!(plain_listing, affected_dir);
    }

    #[test]
    fn root_level_object_targets_storage_root() {
        // A root-level object `/Download` has parent dir "/", which must match
        // the storage-root listing's inner path.
        let resolved = PathBuf::from("/Download");
        let affected_dir = resolved.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        assert_eq!(affected_dir, PathBuf::from("/"));
        let root_listing = listing_inner_mtp_path("mtp-0-1:65537", Path::new("mtp://mtp-0-1/65537")).unwrap();
        assert_eq!(root_listing, affected_dir);
    }
}
