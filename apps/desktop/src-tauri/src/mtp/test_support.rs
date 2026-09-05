//! What the app-side MTP suites reach a virtual device through.
//!
//! The fixtures themselves are `cmdr_mtp::testing`, shared with the crate's own
//! cells so both sides register, connect, prime, and tear down the same way. The
//! one thing that differs across the boundary is WHICH manager: a backend cell
//! wants a detached host that answers nothing, and an app cell wants the real
//! wiring, so the listing cache, the index, and the volume registry see what the
//! device reports.
//!
//! So the entry points here shadow the crate's by name, dropping the manager
//! argument and passing the app's parked one. A suite on this side gets the app's
//! wiring by writing what it always wrote. Same shape as
//! `file_system/write_operations/smb_test_support.rs`.

pub(crate) use cmdr_mtp::testing::{ConnectedDevice, device_lock};

use cmdr_mtp::MtpVolume;
use cmdr_mtp::testing;
use cmdr_mtp::virtual_device::VirtualDeviceFixture;

/// Registers a virtual device and connects THIS APP to it, root listing primed.
///
/// Shadows the crate's builder of the same name. Caller holds
/// [`cmdr_mtp::testing::device_lock`] first.
pub(crate) async fn connect_virtual_device() -> ConnectedDevice {
    testing::connect_virtual_device(super::connection_manager()).await
}

/// [`connect_virtual_device`] over a fixture the caller already seeded.
pub(crate) async fn connect_fixture(fixture: VirtualDeviceFixture) -> ConnectedDevice {
    testing::connect_fixture(super::connection_manager(), fixture).await
}

/// An `MtpVolume` over the device's writable storage, with `primed` listed too.
pub(crate) async fn volume_for(device: &ConnectedDevice, primed: Option<&str>) -> MtpVolume {
    testing::volume_for(super::connection_manager(), device, primed).await
}

/// Disconnects the device from the app's manager AND unregisters it.
pub(crate) async fn teardown(device: ConnectedDevice) {
    device.teardown(super::connection_manager()).await;
}
