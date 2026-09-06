//! What this crate's own device cells drive a session with.
//!
//! The app parks one manager built over its real host and its real registrar.
//! A cell here has neither, so it takes the same shape with a detached host and
//! a registrar that RECORDS instead of registering: the volume registry was only
//! ever an observation point for "did the seam fire", and asking the seam
//! directly says the same thing with no app in the room.
//!
//! Both statics are process-wide, which is what the app's parked manager is too.
//! Every cell that reaches them holds `virtual_device_test_lock` for its whole
//! span, and under `cargo nextest` each cell is its own process anyway.

use std::sync::{Arc, Mutex, OnceLock};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::VolumeHost;

use super::events::no_device_events;
use super::{MtpConnectionManager, MtpVolumeRegistrar};

/// The storages the registrar currently holds, as `(device_id, storage_id)`.
static ATTACHED: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

/// A registrar that remembers what it was told to attach and detach.
///
/// The stand-in for the app's `volume_wiring::volume_registrar()`: same
/// synchronous contract, nothing behind it. `fn` pointers can't capture, so the
/// record is the static above.
pub(crate) fn recording_registrar() -> MtpVolumeRegistrar {
    MtpVolumeRegistrar {
        attach: |_manager, device_id, storage_id, _storage_name| {
            ATTACHED.lock_ignore_poison().push((device_id.to_string(), storage_id));
        },
        detach: |device_id, storage_id| {
            ATTACHED
                .lock_ignore_poison()
                .retain(|(id, storage)| id != device_id || *storage != storage_id);
        },
    }
}

/// Whether `(device_id, storage_id)` is attached right now.
///
/// The crate-side reading of "is its volume still in the sidebar": the app
/// registers on attach and unregisters on detach, so the two answers only ever
/// differ if the app's wiring is broken, which is the app's own cell to write.
pub(crate) fn is_attached(device_id: &str, storage_id: u32) -> bool {
    ATTACHED
        .lock_ignore_poison()
        .iter()
        .any(|(id, storage)| id == device_id && *storage == storage_id)
}

/// The manager this crate's device cells share, over a detached host and
/// [`recording_registrar`].
///
/// Shared rather than per-cell because these cells assert across calls (a
/// session that survived a reset, a cache that was cleared), the way the app's
/// parked manager is shared. A cell that needs its OWN manager — to hand it a
/// recording event sink, say — builds one with `MtpConnectionManager::new`.
pub(crate) fn test_connection_manager() -> &'static Arc<MtpConnectionManager> {
    static MANAGER: OnceLock<Arc<MtpConnectionManager>> = OnceLock::new();
    MANAGER.get_or_init(|| MtpConnectionManager::new(VolumeHost::detached(), no_device_events(), recording_registrar()))
}
