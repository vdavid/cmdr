//! What a test drives a virtual device with, on either side of the boundary.
//!
//! Every device cell in this crate and every MTP cell in the app needs the same
//! four steps: register a fixture-backed phone, find the id discovery gave it,
//! connect, and prime the root listing (`resolve_path_to_handle` is cache-only,
//! so nothing resolves until something has been listed). Written out per file
//! that's forty lines each and six chances to get the teardown wrong.
//!
//! The one thing that differs across the boundary is WHICH manager: a cell here
//! wants a detached host that answers nothing, and an app cell wants the real
//! wiring, so the listing cache, the index, and the volume registry see what the
//! device reports. So every entry point takes the manager, and the app's
//! `mtp_test_support.rs` shadows them with no-argument versions over its parked
//! one. Same shape as `cmdr_smb::volume::testing` and the app's
//! `smb_test_support.rs`.
//!
//! ❌ Never enable `testing` in a production build, and ❌ don't grow this into a
//! way to reach the session layer's state: it hands out a connected device and
//! takes it away again. The two instruments that read the backend are
//! `volume::testing`, and they stay two numbers.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use crate::connection::{DeviceWatch, MtpConnectionManager, MtpDisconnectReason};
use crate::virtual_device::{
    VirtualDeviceFixture, setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock,
};
use crate::volume::MtpVolume;

#[cfg(test)]
pub(crate) use crate::connection::testing::{is_attached, recording_registrar, test_connection_manager};

/// A virtual device that's registered, connected, and primed.
///
/// Holds its fixture, so the backing `TempDir` outlives every cell that reads
/// through it. ❗ Call [`ConnectedDevice::teardown`] rather than dropping it: a
/// device left registered survives into the next test in the same binary, which
/// then connects to a stale storage handle over a directory that's already gone
/// and fails on its first write with a bare protocol error.
pub struct ConnectedDevice {
    /// The device id discovery reports, which is serial-derived rather than
    /// `format!("mtp-{location_id}")`. Every session call takes this.
    pub id: String,
    /// The first storage the device reported: the writable internal one.
    pub storage_id: u32,
    /// Every storage the connect reported, in the order it reported them. The
    /// virtual device has two, so a cell asserting on the per-storage loop has
    /// more than one to count.
    pub storage_ids: Vec<u32>,
    /// What [`unregister_virtual_mtp_device`] takes.
    pub location_id: u64,
    fixture: VirtualDeviceFixture,
}

impl ConnectedDevice {
    /// The fixture's backing directory. `root().join("internal")` is what the
    /// writable storage serves, so a cell can seed a file there (plus a
    /// [`crate::virtual_device::rescan_virtual_device`]) or assert one is gone.
    pub fn root(&self) -> &Path {
        self.fixture.root()
    }

    /// Disconnects AND unregisters. Both halves are load-bearing; see the type's
    /// note. Tolerant of an already-disconnected device, so a cell that tested a
    /// disconnect path can still finish through here.
    pub async fn teardown(self, manager: &Arc<MtpConnectionManager>) {
        let _ = manager.disconnect(&self.id, MtpDisconnectReason::User).await;
        unregister_virtual_mtp_device(self.location_id);
    }
}

/// The lock every virtual-device cell holds for its whole span.
///
/// All virtual devices register under one serial, hence one Cmdr device id, so
/// two cells in the same process would fight over it. Re-exported here so a
/// suite needs one `use`.
pub async fn device_lock() -> tokio::sync::MutexGuard<'static, ()> {
    virtual_device_test_lock().lock().await
}

/// Registers a virtual device, connects `manager` to it with the watch off, and
/// primes the root listing.
///
/// Caller holds [`device_lock`] first. `DeviceWatch::Off` because a cell that
/// wants the event loop running says so itself; the default here is the quiet
/// one, so nothing arrives that the cell didn't ask for.
pub async fn connect_virtual_device(manager: &Arc<MtpConnectionManager>) -> ConnectedDevice {
    let fixture = setup_virtual_mtp_device();
    connect_fixture(manager, fixture).await
}

/// [`connect_virtual_device`] over a fixture the caller already seeded.
///
/// Seeding wants the backing directory before the device is connected (write the
/// file, then [`crate::virtual_device::rescan_virtual_device`]), which is why
/// this half is separate.
pub async fn connect_fixture(manager: &Arc<MtpConnectionManager>, fixture: VirtualDeviceFixture) -> ConnectedDevice {
    let location_id = fixture.location_id;
    let id = crate::list_mtp_devices()
        .into_iter()
        .find(|device| device.location_id == location_id)
        .map(|device| device.id)
        .expect("the virtual device must appear in discovery");
    let info = manager
        .connect(&id, DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_ids: Vec<u32> = info.storages.iter().map(|storage| storage.id).collect();
    let storage_id = *storage_ids.first().expect("the virtual device reports storages");
    manager
        .list_directory(&id, storage_id, "/")
        .await
        .expect("priming the root listing");
    ConnectedDevice {
        id,
        storage_id,
        storage_ids,
        location_id,
        fixture,
    }
}

/// An `MtpVolume` over the device's writable storage, with `path` primed too.
///
/// `resolve_path_to_handle` is cache-only and [`connect_virtual_device`] stops
/// at the root, so anything nested has to be reached through its parent before a
/// cell can name it. Pass `None` when the root is enough.
pub async fn volume_for(
    manager: &Arc<MtpConnectionManager>,
    device: &ConnectedDevice,
    primed: Option<&str>,
) -> MtpVolume {
    let volume = MtpVolume::new(Arc::clone(manager), &device.id, device.storage_id, "Test");
    if let Some(path) = primed {
        volume
            .list_directory(Path::new(path), None)
            .await
            .unwrap_or_else(|error| panic!("priming the {path} listing: {error:?}"));
    }
    volume
}
