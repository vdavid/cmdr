//! MTP-volume indexing entry point and the device-disconnect freshness hook.
//!
//! MTP indexing is the USB analogue of SMB indexing (`smb_index.rs`): a phone or
//! camera storage is scanned over the same `Volume` trait into its own per-volume
//! index DB, kept Fresh by the live PTP event loop while the device is connected,
//! and dropped to Stale the moment the device disconnects (plan D4 — MTP Fresh is
//! as strong as SMB Fresh).
//!
//! Two ways MTP enable differs from SMB:
//!
//! - **No connection gate.** SMB indexing requires upgrading an `os_mount` to a
//!   direct smb2 session first; MTP has only one connection mode (the USB session
//!   the `MtpConnectionManager` already owns), so enable just needs the volume
//!   registered (the device connected). FDA-independent like SMB (USB isn't
//!   TCC-protected).
//! - **Removals resolve by stored handle.** PTP `ObjectRemoved` carries only an
//!   opaque handle and the object is already gone, so `GetObjectInfo` can't map it
//!   to a path. The MTP scan stores each entry's object handle in the index's
//!   `inode` column, so a removal resolves via `find_entry_by_inode` (see
//!   `mtp_watch.rs`).

use tauri::AppHandle;

/// Turn on indexing for an MTP volume (the per-drive "Turn on indexing" action,
/// routed here by `commands/indexing.rs` for `mtp-*` volume ids).
///
/// Requires the device connected (the volume registered in `VolumeManager`);
/// otherwise there's nothing to scan. FDA-independent. A no-op if the volume's
/// index is already active. Errors (as a plain string for the IPC surface) only
/// on an internal start failure or an unregistered volume — there's no typed
/// gate reason because MTP has no connection-upgrade step to refuse.
pub(crate) fn start_indexing_for_mtp(app: AppHandle, volume_id: String) -> Result<(), String> {
    if super::state::is_active(&volume_id) {
        log::info!("start_indexing_for_mtp: '{volume_id}' already active, no-op");
        return Ok(());
    }

    // The MTP volume must be registered (device connected) to resolve its root
    // and list it. A missing registration means the device isn't connected.
    let volume_root = match crate::file_system::get_volume_manager().get(&volume_id) {
        Some(v) => v.root().to_path_buf(),
        None => {
            return Err(format!(
                "MTP volume '{volume_id}' isn't connected; plug in the device before indexing it"
            ));
        }
    };

    super::state::start_indexing_for_mtp_inner(&app, &volume_id, volume_root)
}

/// Record that an MTP device's live event loop died (the device disconnected or
/// the PTP `next_event()` loop returned). Flips a Fresh index to Stale via the
/// shared freshness state machine — the MTP analogue of `on_smb_watcher_died`.
///
/// Fired for EVERY MTP volume on the device (one device hosts N storages, each a
/// separate index). A reconnect respawns the event loop, but continuity already
/// broke (events were lost while unplugged), so the index stays Stale until a
/// rescan — the model's "Stale ⇒ Fresh only via rescan" rule. No-op for an
/// unindexed volume.
pub(crate) fn on_mtp_device_disconnected(device_id: &str) {
    // Every registered MTP volume on this device transitions to Stale. The
    // registry is keyed by volume id (`{device_id}:{storage_id}`), so we match by
    // the device-id prefix plus a numeric storage tail (robust to a `:` in a
    // serial device id via `mtp::identity`).
    for volume_id in super::state::registered_mtp_volume_ids_for_device(device_id) {
        super::state::apply_freshness_event(&volume_id, super::freshness::FreshnessEvent::WatcherDied);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::indexing::enrichment::{ReadPool, uninstall_read_pool};
    use crate::indexing::freshness::Freshness;
    use crate::indexing::pending_sizes::{PendingSizes, uninstall_pending_sizes};
    use crate::indexing::state::{INDEX_REGISTRY, get_freshness, try_reserve_initializing_phase};
    use crate::indexing::store::IndexStore;

    /// Reserve a volume's registry instance at a given freshness, run the body,
    /// then remove it. Mirrors `smb_index`'s test harness.
    fn with_reserved_volume(vid: &str, initial: Freshness, body: impl FnOnce()) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join(format!("{vid}.db"));
        let store = IndexStore::open(&db_path).expect("open store");
        let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
        let pending = Arc::new(PendingSizes::new());
        INDEX_REGISTRY.lock().expect("registry").remove(vid);
        assert!(
            try_reserve_initializing_phase(vid, store, pool, pending, Some(initial)).is_ok(),
            "reserve must succeed",
        );
        body();
        INDEX_REGISTRY.lock().expect("registry").remove(vid);
        uninstall_read_pool(vid);
        uninstall_pending_sizes(vid);
    }

    #[test]
    fn device_disconnect_flips_a_fresh_mtp_volume_to_stale() {
        // The headline D4 transition: when the device's event loop dies, every
        // MTP volume on it goes Fresh ⇒ Stale.
        let device_id = "mtp-DISC-TEST";
        let volume_id = crate::mtp::identity::mtp_volume_id(device_id, 65537);
        with_reserved_volume(&volume_id, Freshness::Fresh, || {
            on_mtp_device_disconnected(device_id);
            assert_eq!(
                get_freshness(&volume_id),
                Some(Freshness::Stale),
                "a disconnected MTP device must mark its Fresh index Stale",
            );
        });
    }

    #[test]
    fn device_disconnect_is_a_noop_for_an_unindexed_volume() {
        // No registered instance ⇒ nothing to transition; must not panic.
        on_mtp_device_disconnected("mtp-never-registered");
        let volume_id = crate::mtp::identity::mtp_volume_id("mtp-never-registered", 65537);
        assert_eq!(get_freshness(&volume_id), None);
    }

    #[test]
    fn disconnect_only_touches_the_named_device() {
        // Two devices' volumes registered; disconnecting one must not flip the
        // other (the device-id prefix match must not over-match).
        let dev_a = "mtp-AAA";
        let dev_b = "mtp-BBB";
        let vol_a = crate::mtp::identity::mtp_volume_id(dev_a, 65537);
        let vol_b = crate::mtp::identity::mtp_volume_id(dev_b, 65537);
        with_reserved_volume(&vol_a, Freshness::Fresh, || {
            with_reserved_volume(&vol_b, Freshness::Fresh, || {
                on_mtp_device_disconnected(dev_a);
                assert_eq!(
                    get_freshness(&vol_a),
                    Some(Freshness::Stale),
                    "device A's volume goes Stale"
                );
                assert_eq!(
                    get_freshness(&vol_b),
                    Some(Freshness::Fresh),
                    "device B's volume stays Fresh (no over-match)",
                );
            });
        });
    }
}
