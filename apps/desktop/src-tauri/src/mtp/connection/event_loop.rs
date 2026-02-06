//! MTP device event loop for file watching.
//!
//! Polls for MTP device events and emits directory-diff events to the frontend
//! using the unified diff system shared with local file watching.

use log::{debug, info, warn};
use mtp_rs::MtpDevice;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, broadcast};

use super::cache::EVENT_DEBOUNCE_MS;
use super::{MtpConnectionManager, connection_manager};
use crate::file_system::listing::{get_listings_by_volume_prefix, update_listing_entries};
use crate::file_system::{DirectoryDiff, FileEntry, compute_diff};
use std::path::PathBuf;

impl MtpConnectionManager {
    /// Starts the event polling loop for a connected device.
    ///
    /// This spawns a background task that polls for MTP device events and emits
    /// `mtp-directory-changed` events to the frontend when files change on the device.
    pub(super) fn start_event_loop(&self, device_id: String, device: Arc<Mutex<MtpDevice>>, app: AppHandle) {
        let (shutdown_tx, _) = broadcast::channel(1);

        // Store shutdown sender
        {
            let mut shutdown_map = self.event_loop_shutdown.write().unwrap();
            shutdown_map.insert(device_id.clone(), shutdown_tx.clone());
        }

        // Clone for the spawned task
        let device_id_clone = device_id.clone();

        // Spawn the event loop task. It uses connection_manager() to access the debouncer
        // since the debouncer is part of the global singleton.
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_tx.subscribe();

            debug!("MTP event loop started for device: {}", device_id_clone);

            loop {
                // Try to acquire the device lock with a short timeout to check for shutdown
                let poll_result = tokio::select! {
                    biased;

                    // Check for shutdown signal first
                    _ = shutdown_rx.recv() => {
                        debug!("MTP event loop shutting down (signal): {}", device_id_clone);
                        break;
                    }

                    // Poll for next event (with timeout built into next_event)
                    result = async {
                        // Try to lock the device - use a timeout to prevent deadlocks
                        match tokio::time::timeout(Duration::from_secs(5), device.lock()).await {
                            Ok(guard) => {
                                // Poll for event
                                guard.next_event().await
                            }
                            Err(_) => {
                                // Timeout acquiring lock - device might be busy with another operation
                                // Return timeout to continue polling
                                Err(mtp_rs::Error::Timeout)
                            }
                        }
                    } => {
                        result
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
        if let Some(tx) = self.event_loop_shutdown.write().unwrap().remove(device_id) {
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
            DeviceEvent::ObjectAdded { handle } => {
                debug!("MTP object added: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::ObjectRemoved { handle } => {
                debug!("MTP object removed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::ObjectInfoChanged { handle } => {
                debug!("MTP object changed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::StorageInfoChanged { storage_id } => {
                debug!("MTP storage info changed: {:?} on {}", storage_id, device_id);
                // Could emit a storage space update event in the future
            }
            DeviceEvent::StoreAdded { storage_id } => {
                info!("MTP storage added: {:?} on {}", storage_id, device_id);
                // Could emit a storage list update event in the future
            }
            DeviceEvent::StoreRemoved { storage_id } => {
                info!("MTP storage removed: {:?} on {}", storage_id, device_id);
                // Could emit a storage list update event in the future
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

    /// Emits directory-diff events for all affected listings (with debouncing).
    ///
    /// Uses the unified diff system shared with local file watching, providing
    /// smooth incremental UI updates without full directory reloads.
    fn emit_directory_changed(device_id: &str, app: &AppHandle) {
        // Check debouncer via the global connection manager
        if !connection_manager().event_debouncer.should_emit(device_id) {
            debug!(
                "MTP event loop: directory change DEBOUNCED for device={} (within {}ms window)",
                device_id, EVENT_DEBOUNCE_MS
            );
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
        let app_clone = app.clone();

        // Spawn task to re-read directories and compute diffs
        tokio::spawn(async move {
            Self::compute_and_emit_diffs(&device_id_owned, listings, &app_clone).await;
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
    async fn compute_and_emit_diffs(
        device_id: &str,
        listings: Vec<(String, String, PathBuf, Vec<FileEntry>)>,
        app: &AppHandle,
    ) {
        // Track sequence numbers per listing (simple counter, increments each diff)
        static SEQUENCE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        for (listing_id, volume_id, path, old_entries) in listings {
            // Extract storage_id from volume_id (format: "mtp-{device}:{storage}")
            let Some(storage_id) = volume_id.split(':').nth(1).and_then(|s| s.parse::<u32>().ok()) else {
                warn!(
                    "MTP diff: could not parse storage_id from volume_id={}, skipping",
                    volume_id
                );
                continue;
            };

            // Convert path to MTP inner path
            let mtp_path = path.to_string_lossy();
            let mtp_path = if mtp_path.starts_with("mtp://") {
                // Parse: mtp://mtp-0-1/65537/DCIM/Camera -> DCIM/Camera
                let without_scheme = mtp_path.strip_prefix("mtp://").unwrap_or(&mtp_path);
                let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
                if parts.len() >= 3 {
                    parts[2].to_string()
                } else {
                    String::new()
                }
            } else if mtp_path == "/" || mtp_path.is_empty() {
                String::new()
            } else {
                mtp_path.strip_prefix('/').unwrap_or(&mtp_path).to_string()
            };

            // Invalidate the MTP listing cache before re-reading so we get fresh data
            // (otherwise we'd compare stale cached data with itself and detect no changes)
            connection_manager()
                .invalidate_listing_cache(device_id, storage_id, &path)
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

            // Get sequence number
            let sequence = SEQUENCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

            // Emit directory-diff event (same format as local watcher)
            let diff = DirectoryDiff {
                listing_id: listing_id.clone(),
                sequence,
                changes,
            };

            if let Err(e) = app.emit("directory-diff", &diff) {
                warn!("MTP diff: failed to emit event: {}", e);
            } else {
                info!(
                    "MTP diff: emitted directory-diff for listing_id={}, sequence={}",
                    listing_id, sequence
                );
            }
        }
    }
}
