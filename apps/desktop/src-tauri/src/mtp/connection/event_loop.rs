//! MTP device event loop for file watching.
//!
//! Polls a connected device for object and storage events, and reports each one
//! to the two consumers that care: the open panes, through the `ListingHost`
//! seam, and the file index, through `IndexNotifier`.

use log::{debug, info, warn};
use mtp_rs::MtpDevice;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};

use mtp_rs::ObjectHandle;

use super::cache::{EVENT_DEBOUNCE_MS, EventDebouncer};
use super::{MtpConnectionManager, normalize_mtp_path};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::DirectoryChange;
use std::path::{Path, PathBuf};

impl MtpConnectionManager {
    /// Starts the event polling loop for a connected device.
    ///
    /// This spawns a background task that polls for MTP device events and emits
    /// `mtp-directory-changed` events to the frontend when files change on the device.
    pub(super) fn start_event_loop(&self, device_id: String, device: Arc<Mutex<MtpDevice>>) {
        let (shutdown_tx, _) = broadcast::channel(1);

        // Store shutdown sender
        {
            let mut shutdown_map = self.event_loop_shutdown.write_ignore_poison();
            shutdown_map.insert(device_id.clone(), shutdown_tx.clone());
        }

        // Clone for the spawned task
        let device_id_clone = device_id.clone();

        // The loop reaches back for the manager per event, through a `Weak` so a
        // manager nothing else holds can retire while its device is still
        // plugged in: the next upgrade fails and the loop leaves.
        let manager = self.self_ref.clone();

        self.host().runtime().spawn(async move {
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
                    // allowed-dropping-timeout: this reads the INTERRUPT endpoint, not the bulk pipe, so there's no PTP transaction to abandon. mtp-rs leaves the transfer pending on drop and picks it up on the next poll.
                    result = tokio::time::timeout(Duration::from_secs(5), event_device.next_event()) => {
                        result.unwrap_or(Err(mtp_rs::Error::Timeout))
                    }
                };

                let Some(manager) = manager.upgrade() else {
                    debug!(
                        "MTP event loop stopping: the manager for {} has retired",
                        device_id_clone
                    );
                    break;
                };

                match poll_result {
                    Ok(event) => {
                        manager.handle_device_event(&device_id_clone, event);
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
                        manager.handle_device_disconnected(&device_id_clone).await;
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
    fn handle_device_event(self: &Arc<Self>, device_id: &str, event: mtp_rs::mtp::DeviceEvent) {
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
                self.emit_change_for_handle(device_id, handle);
                self.feed_index_added_or_changed(device_id, handle);
            }
            DeviceEvent::ObjectRemoved { handle } => {
                debug!("MTP object removed: {:?} on {}", handle, device_id);
                self.refresh_whole_device(device_id);
                self.feed_index_removed(device_id, handle);
            }
            DeviceEvent::ObjectInfoChanged { handle } => {
                debug!("MTP object changed: {:?} on {}", handle, device_id);
                self.emit_change_for_handle(device_id, handle);
                self.feed_index_added_or_changed(device_id, handle);
            }
            DeviceEvent::StorageInfoChanged { storage_id } => {
                debug!("MTP storage info changed: {:?} on {}", storage_id, device_id);
                // The cached `Storage` handle carries a snapshot of the storage
                // info; drop it so the next bounded read re-resolves rather than
                // serving stale free-space/capacity numbers.
                let device_id = device_id.to_string();
                let storage_id = storage_id.0 as u32;
                let manager = Arc::clone(self);
                self.host().runtime().spawn(async move {
                    manager.invalidate_storage_cache(&device_id, Some(storage_id)).await;
                });
            }
            DeviceEvent::StoreAdded { storage_id } => {
                info!("MTP storage added: {:?} on {}", storage_id, device_id);
                let device_id = device_id.to_string();
                let manager = Arc::clone(self);
                self.host().runtime().spawn(async move {
                    manager.handle_storage_added(&device_id, storage_id.0 as u32).await;
                });
            }
            DeviceEvent::StoreRemoved { storage_id } => {
                info!("MTP storage removed: {:?} on {}", storage_id, device_id);
                let device_id = device_id.to_string();
                let manager = Arc::clone(self);
                self.host().runtime().spawn(async move {
                    manager.handle_storage_removed(&device_id, storage_id.0 as u32).await;
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

    /// Feed an `ObjectAdded` / `ObjectInfoChanged` into whichever of this device's
    /// storages has it indexed.
    ///
    /// The index owns the routing and the gate-before-resolve discipline (during a
    /// walk it buffers the raw handle rather than paying a device round trip the
    /// walk is about to make anyway), so this hands over the bare PTP handle and
    /// nothing else.
    fn feed_index_added_or_changed(&self, device_id: &str, handle: ObjectHandle) {
        self.host().indexing().device_object_changed(device_id, handle.0 as u32);
    }

    /// Feed an `ObjectRemoved` into whichever of this device's storages had it.
    /// Costs no device round trip: the object is gone, so each indexed storage
    /// matches on the handle it stored.
    fn feed_index_removed(&self, device_id: &str, handle: ObjectHandle) {
        self.host().indexing().device_object_removed(device_id, handle.0 as u32);
    }

    /// Handles a pathful PTP change event (`ObjectAdded` / `ObjectInfoChanged`)
    /// by resolving the opaque handle to a path and refreshing ONLY the affected
    /// directory, instead of the blanket whole-device refresh.
    ///
    /// PTP handles are device-wide but storages are separate namespaces, so we
    /// don't know up front which storage the handle lives in, and resolving one
    /// costs a device round trip per storage. The host names the storages a pane
    /// is showing, which are the only ones where a targeted refresh could change
    /// anything on screen, so the search runs over those. On any resolution
    /// failure (handle invalid, parent uncached and the walk fails, timeout) we
    /// fall back to the whole-device refresh, so an update is never lost — just
    /// less precise.
    fn emit_change_for_handle(self: &Arc<Self>, device_id: &str, handle: ObjectHandle) {
        let device_id = device_id.to_string();
        let manager = Arc::clone(self);
        self.host().runtime().spawn(async move {
            let mut storage_ids: Vec<u32> = manager
                .host()
                .listings()
                .volumes_with_open_listings(&device_id)
                .iter()
                .filter_map(|volume_id| cmdr_fs::volume::mtp_ids::storage_id_of_volume(volume_id))
                .collect();
            storage_ids.sort_unstable();
            storage_ids.dedup();

            for storage_id in storage_ids {
                match manager.resolve_handle_to_path(&device_id, storage_id, handle).await {
                    Ok(object_path) => {
                        // The directory that changed is the object's parent (the
                        // folder whose listing shows it). A root-level object's
                        // parent is the storage root, "/".
                        let affected_dir = object_path
                            .parent()
                            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
                        if manager.refresh_directory(&device_id, storage_id, &affected_dir) {
                            // A pane shows that directory and its re-read is under
                            // way; done.
                            return;
                        }
                        // Resolved, but no pane shows that dir: nothing to refresh
                        // on THIS storage. Keep trying other storages.
                    }
                    Err(e) => {
                        debug!(
                            "MTP targeted refresh: handle {:?} unresolved on {}:{} ({:?})",
                            handle, device_id, storage_id, e
                        );
                    }
                }
            }

            // No storage produced a targeted refresh: fall back to the whole
            // device so the update is never dropped (the change is in a subdir
            // nobody is showing, or resolution failed on every storage).
            manager.refresh_whole_device(&device_id);
        });
    }

    /// Re-reads ONE directory on `(device_id, storage_id)` and hands its contents
    /// to the host, which sorts them each pane's way, diffs, and patches.
    /// `false` when no pane is showing that directory, so the caller can keep
    /// looking on another storage.
    ///
    /// The fresh-listing oracle is the "is a pane showing this?" probe: it
    /// answers only for a listing a live watch is keeping fresh, which is what a
    /// connected MTP device with a running event loop is. A miss also covers the
    /// device-lock-contended case (`MtpVolume::listing_watch_coverage` reads
    /// `try_lock`), and the caller's fallback is the whole-device refresh, so the
    /// update survives either way.
    ///
    /// Goes through the same debouncer as the whole-device path so a burst of
    /// resolved events still collapses to one re-read per window.
    fn refresh_directory(self: &Arc<Self>, device_id: &str, storage_id: u32, affected_dir: &Path) -> bool {
        let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(device_id, storage_id);
        let listing_path = listing_path_for(device_id, storage_id, affected_dir);

        if self
            .host()
            .listings()
            .authoritative_listing(&volume_id, &listing_path)
            .is_none()
        {
            return false;
        }

        if !self.event_debouncer.should_emit(device_id) {
            // Within the debounce window: schedule ONE trailing targeted re-emit
            // so the last event in a burst isn't dropped. Claiming first is what
            // keeps a burst from spawning a task per event (see
            // `EventDebouncer::claim_trailing`).
            let key = EventDebouncer::targeted_key(device_id, affected_dir);
            if !self.event_debouncer.claim_trailing(&key) {
                debug!(
                    "MTP targeted refresh: DEBOUNCED for {}:{} dir={}, trailing emit already pending",
                    device_id,
                    storage_id,
                    affected_dir.display()
                );
                return false;
            }
            debug!(
                "MTP targeted refresh: DEBOUNCED for {}:{} dir={}, scheduling trailing emit",
                device_id,
                storage_id,
                affected_dir.display()
            );
            let device_id = device_id.to_string();
            let affected_dir = affected_dir.to_path_buf();
            let manager = Arc::clone(self);
            self.host().runtime().spawn(async move {
                tokio::time::sleep(Duration::from_millis(EVENT_DEBOUNCE_MS + 50)).await;
                // Release BEFORE re-emitting, so an event arriving during the
                // re-emit can still claim the following window.
                manager.event_debouncer.release_trailing(&key);
                manager.refresh_directory(&device_id, storage_id, &affected_dir);
            });
            return true;
        }

        debug!(
            "MTP targeted refresh: re-reading {}:{} dir={}",
            device_id,
            storage_id,
            affected_dir.display()
        );

        let device_id = device_id.to_string();
        let affected_dir = affected_dir.to_path_buf();
        let manager = Arc::clone(self);
        self.host().runtime().spawn(async move {
            manager.publish_directory(&device_id, storage_id, &affected_dir).await;
        });
        true
    }

    /// Re-reads `dir` from the device and reports its contents as ONE
    /// [`DirectoryChange::Replaced`].
    ///
    /// ❌ Never one seam call per entry: however many entries came back, this is
    /// one call, and the host does the sorting and the diffing (a device answers
    /// in object-handle order, so a diff computed against that order carries
    /// indices pointing at the wrong rows in a pane sorted any other way).
    async fn publish_directory(&self, device_id: &str, storage_id: u32, dir: &Path) {
        let inner = dir.to_string_lossy().trim_start_matches('/').to_string();

        // ❗ Before the re-read, or the 5-second listing cache answers with
        // exactly the entries the event says are out of date. It keys on the
        // normalized inner path (`/Documents`), not the pane's URL.
        self.invalidate_listing_cache(device_id, storage_id, &normalize_mtp_path(&inner))
            .await;

        let entries = match self.list_directory(device_id, storage_id, &inner).await {
            Ok(entries) => entries,
            Err(e) => {
                debug!("MTP refresh: couldn't re-read directory {}: {:?}, skipping", inner, e);
                return;
            }
        };

        self.host().listings().directory_changed(
            &cmdr_fs::volume::mtp_ids::mtp_volume_id(device_id, storage_id),
            &listing_path_for(device_id, storage_id, dir),
            DirectoryChange::Replaced(entries),
        );
    }

    /// The device says something changed and can't say where, so every pane on
    /// it re-reads.
    ///
    /// ONE [`DirectoryChange::FullRefresh`] per open volume, reported at the
    /// DEVICE path rather than at a directory: no listing sits above the storage
    /// root, and a `FullRefresh` whose path matches no listing is what asks the
    /// host to fan the re-read out to every listing on that volume
    /// (`file_system/listing/DETAILS.md` § "Change notification API"). Aiming it
    /// at a storage root instead would refresh a pane showing that root and miss
    /// its sibling pane two directories down.
    fn refresh_whole_device(self: &Arc<Self>, device_id: &str) {
        // When suppressed, schedule a trailing emit after the debounce window
        // so the last event in a burst is never permanently dropped.
        if !self.event_debouncer.should_emit(device_id) {
            // ONE trailing emit per burst. Without the claim, every suppressed
            // event spawns its own task that re-enters here and re-spawns, so a
            // burst retires one event per window instead of collapsing (see
            // `EventDebouncer::claim_trailing`).
            if !self.event_debouncer.claim_trailing(device_id) {
                debug!(
                    "MTP event loop: directory change DEBOUNCED for device={}, trailing emit already pending",
                    device_id
                );
                return;
            }
            debug!(
                "MTP event loop: directory change DEBOUNCED for device={} (within {}ms window), scheduling trailing emit",
                device_id, EVENT_DEBOUNCE_MS
            );
            let device_id_owned = device_id.to_string();
            let manager = Arc::clone(self);
            self.host().runtime().spawn(async move {
                tokio::time::sleep(Duration::from_millis(EVENT_DEBOUNCE_MS + 50)).await;
                // Release BEFORE re-emitting, so an event arriving during the
                // re-emit can still claim the following window.
                manager.event_debouncer.release_trailing(&device_id_owned);
                // Re-emit; this goes through the debouncer again (which will pass
                // since the window has expired) to avoid duplicate processing.
                manager.refresh_whole_device(&device_id_owned);
            });
            return;
        }

        let open_volumes = self.host().listings().volumes_with_open_listings(device_id);
        if open_volumes.is_empty() {
            debug!(
                "MTP event loop: no active listings for device={}, skipping refresh",
                device_id
            );
            return;
        }

        debug!(
            "MTP event loop: refreshing {} open volume(s) for device={}",
            open_volumes.len(),
            device_id
        );

        let device_id_owned = device_id.to_string();
        let manager = Arc::clone(self);
        self.host().runtime().spawn(async move {
            // The host re-reads through `Volume::list_directory`, which the
            // 5-second listing cache would otherwise answer with the pre-change
            // entries. We don't know which directories are open, so the device's
            // whole cache goes.
            manager.clear_listing_caches_for_device(&device_id_owned).await;

            let device_path = PathBuf::from(format!("mtp://{device_id_owned}"));
            for volume_id in open_volumes {
                manager
                    .host()
                    .listings()
                    .directory_changed(&volume_id, &device_path, DirectoryChange::FullRefresh);
            }
        });
    }
}

/// The path a pane showing `dir` on `(device_id, storage_id)` is cached under:
/// the canonical absolute MTP URL, `mtp://{device}/{storage}[/inner]`.
///
/// Pane navigation feeds that URL into the listing pipeline, and
/// `MtpVolume::to_url_path` normalizes every mutation's parent to the same form,
/// so it is the ONE representation `ListingHost` lookups match on. `dir` is the
/// resolver's output (`/DCIM/Camera`, or `/` for the storage root).
fn listing_path_for(device_id: &str, storage_id: u32, dir: &Path) -> PathBuf {
    let root = PathBuf::from(format!("mtp://{device_id}/{storage_id}"));
    let inner = dir.to_string_lossy().trim_start_matches('/').to_string();
    if inner.is_empty() { root } else { root.join(inner) }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use cmdr_fs::volume::DirectoryChange;
    use cmdr_fs::volume::host::listings::RecordingListings;

    use super::super::MtpConnectionManager;
    use super::listing_path_for;
    use crate::test_support::wait_until_async;

    #[test]
    fn a_storage_root_listing_path_has_no_inner_segment() {
        assert_eq!(
            listing_path_for("mtp-0-1", 65_537, Path::new("/")),
            PathBuf::from("mtp://mtp-0-1/65537")
        );
    }

    #[test]
    fn an_inner_directory_hangs_off_the_storage_root() {
        assert_eq!(
            listing_path_for("mtp-0-1", 65_537, Path::new("/DCIM/Camera")),
            PathBuf::from("mtp://mtp-0-1/65537/DCIM/Camera")
        );
    }

    /// The two halves of a targeted refresh have to line up, or the seam lookup
    /// misses and the pane stays stale: the resolver answers with an inner path,
    /// and the host matches on the URL a pane navigated to.
    #[test]
    fn a_resolved_object_targets_the_url_its_pane_is_cached_under() {
        let resolved = PathBuf::from("/DCIM/IMG.jpg");
        let affected_dir = resolved.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        assert_eq!(affected_dir, PathBuf::from("/DCIM"));
        assert_eq!(
            listing_path_for("mtp-0-1", 65_537, &affected_dir),
            PathBuf::from("mtp://mtp-0-1/65537/DCIM"),
        );
    }

    /// A root-level object's parent is the storage root, which is the pane URL
    /// with nothing appended — ❌ never a trailing slash, which wouldn't compare
    /// equal to what navigation cached.
    #[test]
    fn a_root_level_object_targets_the_storage_root_pane() {
        let resolved = PathBuf::from("/Download");
        let affected_dir = resolved.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        assert_eq!(affected_dir, PathBuf::from("/"));
        assert_eq!(
            listing_path_for("mtp-0-1", 65_537, &affected_dir),
            PathBuf::from("mtp://mtp-0-1/65537"),
        );
    }

    /// A manager whose only real seam is `listings`, so a refresh can be observed
    /// without a device, a runtime of its own, or a volume registry.
    fn manager_reporting_to(listings: Arc<RecordingListings>) -> Arc<MtpConnectionManager> {
        MtpConnectionManager::new(
            cmdr_fs::volume::host::VolumeHost::builder()
                .listings(listings as Arc<dyn cmdr_fs::volume::host::listings::ListingHost>)
                .build(),
            crate::mtp::connection::events::no_device_events(),
            crate::mtp::connection::MtpVolumeRegistrar::detached(),
        )
    }

    /// A device event that names no directory refreshes every VOLUME a pane is
    /// showing, one call each, at the device path — the shape that makes the host
    /// fan out to every listing on the volume instead of to one directory.
    #[tokio::test]
    async fn a_whole_device_refresh_reports_once_per_open_volume() {
        let listings = Arc::new(
            RecordingListings::new()
                .with_open_listing("mtp-fanout:65537")
                .with_open_listing("mtp-fanout:131073"),
        );
        let manager = manager_reporting_to(Arc::clone(&listings));

        manager.refresh_whole_device("mtp-fanout");
        wait_until_async(Duration::from_secs(2), "the whole-device refresh to report", || {
            listings.change_count() == 2
        })
        .await;

        let reported = listings.changes();
        let mut volumes: Vec<&str> = reported.iter().map(|(volume_id, ..)| volume_id.as_str()).collect();
        volumes.sort_unstable();
        assert_eq!(
            volumes,
            ["mtp-fanout:131073", "mtp-fanout:65537"],
            "each open volume hears once, whatever it has open"
        );
        for (volume_id, path, change) in &reported {
            assert_eq!(
                path,
                &PathBuf::from("mtp://mtp-fanout"),
                "{volume_id} must be refreshed at the DEVICE path, or the host refreshes one directory instead of the volume"
            );
            assert!(
                matches!(change, DirectoryChange::FullRefresh),
                "the backend has no entries in hand here, so the host does the re-read"
            );
        }
    }

    /// A pane nobody has open costs nothing: no seam call, and no device round
    /// trip to produce one.
    #[tokio::test]
    async fn a_device_with_nothing_on_screen_reports_nothing() {
        let listings = Arc::new(RecordingListings::new());
        let manager = manager_reporting_to(Arc::clone(&listings));

        manager.refresh_whole_device("mtp-quiet");
        // allowed-test-sleep: negative assertion. The refresh spawns, so "it reported nothing" needs
        // a window for the report that must not arrive; there is no event to wait on
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(listings.change_count(), 0);
    }

    /// A directory no pane is showing is not re-read: the oracle miss is the
    /// answer, and ❌ never a device listing issued on the off chance.
    #[tokio::test]
    async fn a_directory_no_pane_shows_is_never_re_read() {
        let listings = Arc::new(RecordingListings::new().with_open_listing("mtp-unshown:65537"));
        let manager = manager_reporting_to(Arc::clone(&listings));

        assert!(
            !manager.refresh_directory("mtp-unshown", 65_537, Path::new("/DCIM")),
            "an unshown directory must send the caller on to the next storage"
        );
        assert_eq!(listings.change_count(), 0);
    }

    /// A serial-based device id can carry a `:`, and the URL keeps it verbatim:
    /// the id is opaque, and only `mtp_ids` may take it apart.
    #[test]
    fn a_device_id_containing_a_colon_survives_verbatim() {
        assert_eq!(
            listing_path_for("mtp-R5CT:123", 65_537, Path::new("/Music")),
            PathBuf::from("mtp://mtp-R5CT:123/65537/Music"),
        );
    }
}

/// The whole pane-refresh chain against a live (virtual) device: an object
/// appears, the loop hears it, and the pane showing its folder is handed the new
/// contents.
#[cfg(all(test, feature = "virtual-mtp"))]
mod device_tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use cmdr_fs::volume::DirectoryChange;

    use super::super::MtpConnectionManager;

    use crate::mtp::connection::events::no_device_events;
    use crate::mtp::connection::{DeviceWatch, MtpDisconnectReason, MtpVolumeRegistrar};
    use crate::mtp::virtual_device::{
        VIRTUAL_DEVICE_SERIAL, setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
    };
    use crate::test_support::wait_until_async;
    use cmdr_fs::volume::host::VolumeHost;
    use cmdr_fs::volume::host::listings::RecordingListings;

    /// The virtual device's writable storage, which mtp-rs numbers rather than
    /// the fixture, so it has to be asked for. A throwaway UNWATCHED session,
    /// because the recorder that watches has to know the volume id before the
    /// manager it belongs to exists.
    async fn writable_storage_id(device_id: &str) -> u32 {
        let probe = MtpConnectionManager::new(
            VolumeHost::detached(),
            no_device_events(),
            MtpVolumeRegistrar::detached(),
        );
        let info = probe
            .connect(device_id, DeviceWatch::Off)
            .await
            .expect("virtual-mtp connect should succeed");
        let storage_id = info.storages.first().expect("a writable storage").id;
        probe
            .disconnect(device_id, MtpDisconnectReason::User)
            .await
            .expect("the probe session must close before the watched one opens");
        storage_id
    }

    /// A device event that resolves must reach the pane showing that folder, as
    /// ONE `Replaced` carrying what the device now holds. Everything between is
    /// real: the interrupt-endpoint poll, the handle→path walk, the oracle probe,
    /// the re-read.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_file_appearing_on_the_device_hands_the_pane_its_new_contents() {
        let _guard = virtual_device_test_lock().lock().await;
        let fixture = setup_virtual_mtp_device();
        let device_id = crate::mtp::list_mtp_devices()
            .into_iter()
            .find(|d| d.location_id == fixture.location_id)
            .map(|d| d.id)
            .expect("the virtual device must appear in discovery");

        let storage_id = writable_storage_id(&device_id).await;
        let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(&device_id, storage_id);
        // The URL a pane navigating there is cached under, written out rather
        // than built with `listing_path_for`: the point is that the loop reports
        // at the representation the pane holds, and a test that derived both
        // sides from one function would pass however that function drifted.
        let documents = PathBuf::from(format!("mtp://{device_id}/{storage_id}/Documents"));

        // A pane on `/Documents`, watched: that is what makes the loop refresh
        // this directory instead of falling back to the whole device.
        let listings =
            Arc::new(RecordingListings::new().with_authoritative_listing(&volume_id, documents.clone(), Vec::new()));
        let manager = MtpConnectionManager::new(
            VolumeHost::builder()
                .listings(Arc::clone(&listings) as Arc<dyn cmdr_fs::volume::host::listings::ListingHost>)
                .build(),
            no_device_events(),
            MtpVolumeRegistrar::detached(),
        );

        manager
            .connect(&device_id, DeviceWatch::Live)
            .await
            .expect("virtual-mtp connect should succeed");
        // Prime the path cache the way navigating there does. Root first:
        // `resolve_path_to_handle` is cache-only, so a folder whose parent was
        // never listed has no handle to reach it by.
        manager
            .list_directory(&device_id, storage_id, "/")
            .await
            .expect("listing the storage root");
        manager
            .list_directory(&device_id, storage_id, "/Documents")
            .await
            .expect("listing the folder the pane is showing");

        std::fs::write(fixture.root().join("internal/Documents/arrived.txt"), b"from the phone")
            .expect("writing into the backing dir");
        mtp_rs::rescan_virtual_device(VIRTUAL_DEVICE_SERIAL).expect("the device must be rescannable");

        // Under nextest's 8 s per-test cap, so a broken chain fails with this
        // message rather than being killed without one. The happy path is ~1.5 s.
        wait_until_async(Duration::from_secs(5), "the pane to be handed the new contents", || {
            listings.changes().iter().any(|(_, path, change)| {
                path == &documents && matches!(change, DirectoryChange::Replaced(entries) if entries.iter().any(|e| e.name == "arrived.txt"))
            })
        })
        .await;

        let replacements: Vec<_> = listings
            .changes()
            .into_iter()
            .filter(|(_, _, change)| matches!(change, DirectoryChange::Replaced(_)))
            .collect();
        assert_eq!(
            replacements.len(),
            1,
            "one re-read for one changed directory, ❌ never one report per entry"
        );
        assert_eq!(
            replacements[0].0, volume_id,
            "the report must name the storage's volume, not the device"
        );

        manager
            .disconnect(&device_id, MtpDisconnectReason::User)
            .await
            .expect("virtual-mtp disconnect should succeed");
        unregister_virtual_mtp_device(fixture.location_id);
    }
}
