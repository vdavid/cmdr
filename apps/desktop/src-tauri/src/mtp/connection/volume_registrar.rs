//! The seam between "a storage attached" and "a volume exists".
//!
//! The session layer knows when a device's storage comes and goes. It
//! deliberately does NOT know what a `Volume` is: the app installs two callbacks
//! at startup ([`set_volume_registrar`]) and this module calls them. See
//! `../DETAILS.md` § "Backends never register themselves" for why, and
//! `mtp::volume_wiring` for the app side.

use log::{debug, warn};
use std::sync::OnceLock;

/// The app's answer to "a storage came up" and "a storage went away".
///
/// Plain `fn` pointers rather than boxed closures: neither side needs captured
/// state, and it keeps the hook allocation-free and obviously synchronous.
pub(crate) struct MtpVolumeRegistrar {
    /// Make `(device_id, storage_id)` browsable under its MTP volume id.
    pub attach: fn(device_id: &str, storage_id: u32, storage_name: &str),
    /// Stop offering `(device_id, storage_id)`.
    pub detach: fn(device_id: &str, storage_id: u32),
}

static REGISTRAR: OnceLock<MtpVolumeRegistrar> = OnceLock::new();

/// Installs the registrar.
///
/// Call once at startup, before anything can connect a device. A second call
/// keeps the first registrar and is ignored, so a test fixture and the app
/// wiring can both call it without fighting.
pub(crate) fn set_volume_registrar(registrar: MtpVolumeRegistrar) {
    if REGISTRAR.set(registrar).is_err() {
        debug!("MTP volume registrar was already installed; keeping the first one");
    }
}

/// Attaches one storage as a browsable volume, **synchronously**.
///
/// ❌ Never spawn this and never make it async. `connect()` attaches every
/// storage before it starts the device's event loop, and that ordering is the
/// contract: every consumer the loop reaches (open listings, looked up by volume
/// id, and the per-volume index) routes through the volume registry, so an event
/// arriving ahead of the volumes has nothing to land on and the update is lost.
/// The hook adds an indirection but not a delay, which is what keeps the
/// ordering true.
pub(super) fn attach_storage_volume(device_id: &str, storage_id: u32, storage_name: &str) {
    match REGISTRAR.get() {
        Some(registrar) => (registrar.attach)(device_id, storage_id, storage_name),
        None => {
            warn!("No MTP volume registrar installed, so storage {storage_id} on {device_id} won't show up as a volume")
        }
    }
}

/// Detaches one storage's volume, synchronously. Same reasoning as
/// [`attach_storage_volume`]: a detach that lagged would leave a dead volume in
/// the sidebar.
pub(super) fn detach_storage_volume(device_id: &str, storage_id: u32) {
    match REGISTRAR.get() {
        Some(registrar) => (registrar.detach)(device_id, storage_id),
        None => warn!("No MTP volume registrar installed, so storage {storage_id} on {device_id} can't be detached"),
    }
}
