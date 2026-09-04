//! The seam between "a storage attached" and "a volume exists".
//!
//! The session layer knows when a device's storage comes and goes. It
//! deliberately does NOT know what a `Volume` is: the app hands two callbacks to
//! [`MtpConnectionManager::new`] and this module calls them. See `../DETAILS.md`
//! § "Backends never register themselves" for why, and `mtp::volume_wiring` for
//! the app side.

use std::sync::Arc;

use log::warn;

use super::MtpConnectionManager;

/// The app's answer to "a storage came up" and "a storage went away".
///
/// Plain `fn` pointers rather than boxed closures: neither side needs captured
/// state, and it keeps the hook allocation-free and obviously synchronous. The
/// manager comes along on `attach` because the volume it builds talks to the
/// device through that manager, and a `fn` pointer has nothing captured to find
/// one with.
pub(crate) struct MtpVolumeRegistrar {
    /// Make `(device_id, storage_id)` browsable under its MTP volume id.
    pub attach: fn(manager: &Arc<MtpConnectionManager>, device_id: &str, storage_id: u32, storage_name: &str),
    /// Stop offering `(device_id, storage_id)`.
    pub detach: fn(device_id: &str, storage_id: u32),
}

impl MtpVolumeRegistrar {
    /// Nowhere to register: a storage attaches and detaches without ever
    /// becoming a volume.
    ///
    /// What a bench or a tool driving a session directly wants, and it's why the
    /// manager never carries an `Option<MtpVolumeRegistrar>`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "only a caller driving sessions with no volume registry wants this, and that is a test"
        )
    )]
    pub(crate) fn detached() -> Self {
        Self {
            attach: |_manager, _device_id, _storage_id, _storage_name| {},
            detach: |_device_id, _storage_id| {},
        }
    }
}

impl MtpConnectionManager {
    /// Attaches one storage as a browsable volume, **synchronously**.
    ///
    /// ❌ Never spawn this and never make it async. `connect()` attaches every
    /// storage before it starts the device's event loop, and that ordering is
    /// the contract: every consumer the loop reaches (open listings, looked up
    /// by volume id, and the per-volume index) routes through the volume
    /// registry, so an event arriving ahead of the volumes has nothing to land
    /// on and the update is lost. The hook adds an indirection but not a delay,
    /// which is what keeps the ordering true.
    pub(super) fn attach_storage_volume(&self, device_id: &str, storage_id: u32, storage_name: &str) {
        let Some(manager) = self.self_handle() else {
            warn!("MTP manager is gone, so storage {storage_id} on {device_id} won't show up as a volume");
            return;
        };
        (self.registrar.attach)(&manager, device_id, storage_id, storage_name);
    }

    /// Detaches one storage's volume, synchronously. Same reasoning as
    /// [`attach_storage_volume`](Self::attach_storage_volume): a detach that
    /// lagged would leave a dead volume in the sidebar.
    pub(super) fn detach_storage_volume(&self, device_id: &str, storage_id: u32) {
        (self.registrar.detach)(device_id, storage_id);
    }
}
