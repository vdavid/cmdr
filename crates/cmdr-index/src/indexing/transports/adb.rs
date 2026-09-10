//! Android-phone-over-ADB indexing: the enable.
//!
//! A phone over ADB indexes exactly like an MTP storage: its own per-volume
//! database, filled by the `Volume`-trait walk (`network_scanner`), served by the
//! shared read path. Two things differ from MTP:
//!
//! - **There is no live watch, so this module has no `watch.rs`.** ADB reports
//!   nothing when a file changes on the phone. Cmdr's own writes still reach the
//!   index: the backend reports every mutation to its listing host, and the host
//!   folds it in through `Index::apply_directory_change`, the same translation an
//!   SMB `CHANGE_NOTIFY` takes (`transports/smb/watch.rs`).
//! - **Dispatch is by the registered backend** (`BackendKind::Adb`), a typed fact
//!   off the volume the host registered, rather than by the id's shape.

use cmdr_fs::volume::BackendKind;

use crate::indexing::lifecycle::state;

/// Whether `volume_id` names a phone over ADB the host has registered.
///
/// `false` for an unplugged phone, whose volume is gone: a start for it then
/// falls through to the SMB gate, which refuses it as `NotRegistered`.
pub(crate) fn is_registered_adb_volume(volume_id: &str) -> bool {
    crate::indexing::host::volumes::current()
        .get(volume_id)
        .is_some_and(|volume| volume.backend_kind() == BackendKind::Adb)
}

/// Turn on indexing for a phone over ADB (the per-drive "Turn on indexing" action,
/// routed here by `Index::start_volume`).
///
/// Needs the phone dialed (its volume registered) and nothing more: ADB has one
/// session shape, so there is no upgrade to gate on, and a phone isn't
/// TCC-protected. A no-op if the phone's index is already active.
pub(crate) fn start_indexing_for_adb(volume_id: &str) -> Result<(), String> {
    // ❌ Not `is_active`: a volume with a teardown claimed on it is active right up
    // to the moment it stops, and this is the enable that has to bring it back.
    if state::is_active_and_staying(volume_id) {
        log::info!("start_indexing_for_adb: '{volume_id}' already active, no-op");
        return Ok(());
    }

    let volume_root = match crate::indexing::host::volumes::current().get(volume_id) {
        Some(volume) => volume.root().to_path_buf(),
        None => return Err(format!("ADB volume '{volume_id}' isn't connected")),
    };

    state::start_indexing_for_adb_inner(volume_id, volume_root)?;

    // A new external index DB just came online: cap accumulation by evicting the
    // least-recently-used OFFLINE external DBs, exactly as MTP does. See `retention`.
    crate::indexing::resources::retention::enforce_external_index_cap();
    Ok(())
}
