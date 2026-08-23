//! Eject commands: thin delegates to the volume-teardown logic in
//! [`crate::file_system::volume::eject`]. `EjectError` IS the wire type, so
//! these pass the typed refusal straight through.

use crate::file_system::volume::eject::{self, EjectError};

/// Ejects a volume. Picks the right teardown for the volume's kind.
///
/// Returns `Ok(())` once the unmount or disconnect is initiated. The frontend
/// shouldn't wait for the volume to fully disappear — `volume-unmounted` (for
/// disk volumes) or `mtp-device-disconnected` (for MTP) will fire shortly
/// after and panes rooted at the volume redirect to root.
#[tauri::command]
#[specta::specta]
pub async fn eject_volume(volume_id: String) -> Result<(), EjectError> {
    eject::eject(&volume_id).await
}

/// Returns the IDs of volumes that currently have a write op (copy / move /
/// delete) reading from or writing to them. The volume picker bootstraps its
/// busy set from this once on startup, then keeps it live via the
/// `volumes-busy-changed` event. Used to disable Eject for a busy device.
#[tauri::command]
#[specta::specta]
pub fn get_busy_volume_ids() -> Vec<String> {
    crate::file_system::busy_volume_ids()
}
