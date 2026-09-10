//! Fixtures the app-side ADB suites share: a fake phone the app's cached list
//! carries, a dial through the pane's own entry, and the check that the registry
//! and the provider hold the same volume.
//!
//! Every suite drives `cmdr_adb::testing::FakeAdbServer`, ❌ never a real `adb`.

use std::sync::Arc;

use cmdr_adb::testing::{FakeAdbServer, FakeTree};
use cmdr_adb::{AdbConnectionParams, AdbDevice, AdbDeviceState};
use cmdr_fs::volume::Volume;

use super::device_provider;
use super::volume_wiring::connect_device_at;
use crate::file_system::volume::manager::get_volume_manager;

/// A device row for `serial` in `state`, with the fake's long-format fields.
pub(crate) fn phone(serial: &str, state: AdbDeviceState) -> AdbDevice {
    AdbDevice {
        serial: serial.to_string(),
        state,
        ..cmdr_adb::testing::fake_device()
    }
}

/// A fake server holding `tree` for one ready phone under `serial`, and the
/// app's cached list carrying that phone too, which is where a pane finds the
/// row it dials: a dial installs only for a phone that list still holds.
pub(crate) async fn a_listed_phone(serial: &str, tree: FakeTree) -> FakeAdbServer {
    let fake = FakeAdbServer::start(tree).await;
    let row = phone(serial, AdbDeviceState::Ready);
    fake.push_devices(vec![row.clone()]);
    device_provider::apply_device_list(vec![row]);
    fake
}

/// [`a_listed_phone`] with the default tree: `/` and an empty `/sdcard`.
pub(crate) async fn a_fake_phone(serial: &str) -> FakeAdbServer {
    a_listed_phone(serial, FakeTree::new()).await
}

/// Dials the listed phone the way a pane does, and hands back its id and the
/// volume the registry now holds under it.
pub(crate) async fn dial(fake: &FakeAdbServer, serial: &str, attempt_id: &str) -> (String, Arc<dyn Volume>) {
    let volume_id = connect_device_at(AdbConnectionParams::at(serial, fake.endpoint()), attempt_id)
        .await
        .unwrap_or_else(|e| panic!("the fake phone {serial} must dial; got {e:?}"));
    let volume = get_volume_manager()
        .get(&volume_id)
        .unwrap_or_else(|| panic!("a dial that answered Ok must have registered {volume_id}"));
    (volume_id, volume)
}

/// How many dials reached `fake`: a connect opens with exactly one
/// `host:devices-l`. ❗ The tracker refetches it on every push too, so count
/// only where no tracker follows `fake`.
pub(crate) fn dials_seen(fake: &FakeAdbServer) -> usize {
    fake.requests().iter().filter(|r| *r == "host:devices-l").count()
}

/// Whether the registry and the provider hold the very same volume for
/// `serial`: the one panes list through, and the one eject, `note_device_gone`,
/// and `space_for_path` reach.
pub(crate) fn registry_and_provider_agree(serial: &str) -> bool {
    let id = cmdr_fs::volume::adb_volume_id(serial);
    match (get_volume_manager().get(&id), device_provider::connected_volume(serial)) {
        (Some(registered), Some(remembered)) => std::ptr::addr_eq(Arc::as_ptr(&registered), Arc::as_ptr(&remembered)),
        _ => false,
    }
}

/// Unregisters and forgets whatever a cell dialed for `serial`.
pub(crate) fn retire_phone(serial: &str) {
    get_volume_manager().unregister(&cmdr_fs::volume::adb_volume_id(serial));
    device_provider::forget_volume(serial);
}
