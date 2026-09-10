//! The tracker's pushes reaching the provider, end to end: a real
//! `host:track-devices` subscription against `FakeAdbServer`, every push landing
//! in `apply_device_list`, and what the switcher and the registry hold after it.
//!
//! The provider's pure row mapping has its own cells in `device_provider.rs`;
//! these hold the path a push travels to get there (the short list, the long
//! refetch, the parse, the store, the retirement).

use std::time::Duration;

use cmdr_adb::testing::{FakeAdbServer, FakeTree};
use cmdr_adb::{AdbDevice, AdbDeviceState};
use cmdr_fs::volume::{ConnectionState, DeviceReadiness, DeviceUnavailableReason};

use super::device_provider::{self, AdbDeviceProvider};
use super::test_support::{dial, phone};
use super::volume_wiring::{apply_settings_at, start_tracker_at};
use crate::device_volumes::DeviceVolumeProvider;
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::wait_until_async;
use crate::volume_broadcast::volumes_changed_requests;

/// A backstop, far above a push crossing loopback, which lands in milliseconds.
const PUSH_BUDGET: Duration = Duration::from_secs(5);

/// A fake server listing `devices`, with the app following it through the real
/// tracker.
async fn a_tracked_server(devices: Vec<AdbDevice>) -> FakeAdbServer {
    let fake = FakeAdbServer::start(FakeTree::new()).await;
    fake.push_devices(devices);
    start_tracker_at(fake.endpoint());
    fake
}

/// The state the app's cached list holds for `serial`, if it lists it.
fn cached_state(serial: &str) -> Option<AdbDeviceState> {
    device_provider::cached_devices()
        .into_iter()
        .find(|d| d.serial == serial)
        .map(|d| d.state)
}

/// The readiness on the switcher's row for `serial`: `None` when there's no row.
async fn row_readiness(serial: &str) -> Option<Option<DeviceReadiness>> {
    let id = cmdr_fs::volume::adb_volume_id(serial);
    AdbDeviceProvider
        .entries()
        .await
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.device_readiness)
}

/// ❗ A phone unplugged while a pane has it open. The tracker's push is the only
/// thing that retires its volume before an operation trips over it: the volume
/// hears it's gone, the registry lets it go, and the switcher is told to
/// refetch. Miss any one and a pane keeps a dead phone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_that_drops_a_dialed_phone_retires_its_volume_and_tells_the_switcher() {
    const SERIAL: &str = "R58M-Tracker-Unplug";
    let before_anything = volumes_changed_requests();
    let fake = a_tracked_server(vec![phone(SERIAL, AdbDeviceState::Ready)]).await;
    wait_until_async(PUSH_BUDGET, "the tracker to list the phone", || {
        cached_state(SERIAL).is_some()
    })
    .await;
    let (volume_id, volume) = dial(&fake, SERIAL, "adb-tracker-unplug").await;
    // The dial asks for its own broadcast just after it answers. Waiting for a
    // request past the start keeps that one from landing after the count below
    // and passing for the retirement's. A request the tracker's first delivery
    // made can satisfy this wait early, but only when the push path broadcasts
    // at all, which is exactly what the last wait holds.
    wait_until_async(PUSH_BUDGET, "the dial's own broadcast request", || {
        volumes_changed_requests() > before_anything
    })
    .await;
    let before_the_push = volumes_changed_requests();

    fake.push_devices(Vec::new());
    wait_until_async(PUSH_BUDGET, "the push to unregister the phone's volume", || {
        get_volume_manager().get(&volume_id).is_none()
    })
    .await;

    assert!(
        device_provider::connected_volume(SERIAL).is_none(),
        "the provider forgets the phone it retired"
    );
    assert!(
        matches!(volume.connection_state(), Some(ConnectionState::Disconnected)),
        "a pane still holding the volume hears the device is gone; got {:?}",
        volume.connection_state()
    );
    wait_until_async(PUSH_BUDGET, "the retirement to ask the switcher to refetch", || {
        volumes_changed_requests() > before_the_push
    })
    .await;
    assert!(
        cached_state(SERIAL).is_none(),
        "the row leaves the switcher with the phone"
    );

    apply_settings_at(fake.endpoint(), false, None).await;
}

/// ❗ The phone's "Allow USB debugging?" tap arrives as a push that moves the
/// serial from `unauthorized` to `device`. A pane waiting on that row dials only
/// once it reads ready, so a push that doesn't reach the row leaves the pane
/// waiting forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_that_authorizes_a_waiting_phone_turns_its_row_ready() {
    const SERIAL: &str = "R58M-Tracker-Allow";
    let fake = a_tracked_server(vec![phone(SERIAL, AdbDeviceState::Unauthorized)]).await;
    wait_until_async(PUSH_BUDGET, "the tracker to list the waiting phone", || {
        cached_state(SERIAL) == Some(AdbDeviceState::Unauthorized)
    })
    .await;
    assert_eq!(
        row_readiness(SERIAL).await,
        Some(Some(DeviceReadiness::WaitingForAuthorization)),
        "a phone owing its Allow tap is listed and waiting"
    );

    fake.push_devices(vec![phone(SERIAL, AdbDeviceState::Ready)]);
    wait_until_async(PUSH_BUDGET, "the Allow tap's push to reach the cached list", || {
        cached_state(SERIAL) == Some(AdbDeviceState::Ready)
    })
    .await;
    assert_eq!(
        row_readiness(SERIAL).await,
        Some(Some(DeviceReadiness::Ready)),
        "the same row turns ready, so the waiting pane dials"
    );

    apply_settings_at(fake.endpoint(), false, None).await;
}

/// ❗ `offline` and `no permissions` rows are there so their reason can be the
/// tooltip. `no permissions` is two words on the wire, in both the short push
/// and the long refetch, which is where a parse that splits on whitespace would
/// lose the row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_and_no_permissions_rows_arrive_through_the_tracker_with_their_reasons() {
    const OFFLINE: &str = "R58M-Tracker-Offline";
    const NO_PERMISSIONS: &str = "R58M-Tracker-NoPerms";
    let fake = a_tracked_server(vec![
        phone(OFFLINE, AdbDeviceState::Offline),
        phone(NO_PERMISSIONS, AdbDeviceState::NoPermissions),
    ])
    .await;
    wait_until_async(PUSH_BUDGET, "the tracker to list both devices", || {
        cached_state(OFFLINE).is_some() && cached_state(NO_PERMISSIONS).is_some()
    })
    .await;

    assert_eq!(
        row_readiness(OFFLINE).await,
        Some(Some(DeviceReadiness::Unavailable {
            reason: DeviceUnavailableReason::Offline
        }))
    );
    assert_eq!(
        row_readiness(NO_PERMISSIONS).await,
        Some(Some(DeviceReadiness::Unavailable {
            reason: DeviceUnavailableReason::NoPermissions
        }))
    );

    apply_settings_at(fake.endpoint(), false, None).await;
}
