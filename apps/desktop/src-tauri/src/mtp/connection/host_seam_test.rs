//! What the session layer tells the host it was handed, against a real device.
//!
//! Every seam here is a `dyn` call that goes nowhere on a detached host, so a
//! reach that regressed back to a `crate::` path would still compile, still pass
//! every other suite, and only show up as a counter that stopped arriving or an
//! index that stayed Fresh through a dead session. These cells are the instrument
//! that notices.

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::analytics::RecordingAnalytics;

use super::events::no_device_events;
use super::{DeviceWatch, MtpConnectionManager, MtpDisconnectReason, MtpVolumeRegistrar};
use crate::file_system::volume::manager::get_volume_manager;
use crate::mtp::virtual_device::{setup_virtual_mtp_device, unregister_virtual_mtp_device, virtual_device_test_lock};

/// A device connecting is a thing a user did, so it earns one counter. It has to
/// travel the `AnalyticsSink` seam rather than the app's PostHog client: the
/// backend must behave identically whether the counter goes anywhere or not, and
/// a session layer that names the app's analytics module can't compile outside
/// the app at all.
///
/// The empty property list is the other half. ❌ Nothing identifying may ride
/// along with an MTP counter — not the serial, not the product name, not the
/// storage description, none of which the seam's `&[(&str, &str)]` shape would
/// stop somebody from formatting into a string.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connecting_a_device_records_one_counter_carrying_nothing_identifying() {
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let analytics = Arc::new(RecordingAnalytics::new());
    let host = VolumeHost::builder().analytics(analytics.clone()).build();
    let manager = MtpConnectionManager::new(host, no_device_events(), MtpVolumeRegistrar::detached());

    manager
        .connect(&device_id, DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect should succeed");
    manager
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("disconnecting a device we just connected");
    unregister_virtual_mtp_device(fixture.location_id);

    assert_eq!(
        analytics.events(),
        vec![("mtp_connected".to_string(), Vec::new())],
        "one counter per connect, with no properties at all"
    );
}

/// A device that vanishes under the event loop has to leave the volume registry
/// the same way an explicit disconnect does.
///
/// Two paths remove a device: `disconnect()`, which the settings toggle and the
/// hotplug diff both take, and `handle_device_disconnected`, which the event loop
/// takes when the poll returns `Error::Disconnected` and the reopen loop takes
/// when the phone is gone for good. If only the first one detaches, an unplug
/// spotted by the event loop leaves an `MtpVolume` in the registry answering for
/// a device that isn't there, and it never publishes the `Retirement` that tells
/// in-flight background work it stopped being live.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_lost_under_the_event_loop_detaches_its_volumes() {
    let _guard = virtual_device_test_lock().lock().await;
    let fixture = setup_virtual_mtp_device();
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let manager = MtpConnectionManager::new(
        VolumeHost::detached(),
        no_device_events(),
        crate::mtp::volume_wiring::volume_registrar(),
    );
    let info = manager
        .connect(&device_id, DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect should succeed");
    let volume_ids: Vec<String> = info
        .storages
        .iter()
        .map(|storage| cmdr_fs::volume::mtp_ids::mtp_volume_id(&device_id, storage.id))
        .collect();
    assert!(
        volume_ids.iter().all(|id| get_volume_manager().get(id).is_some()),
        "the connect has to attach every storage first, or this proves nothing"
    );

    manager.handle_device_disconnected(&device_id).await;
    unregister_virtual_mtp_device(fixture.location_id);

    let still_registered: Vec<&String> = volume_ids
        .iter()
        .filter(|id| get_volume_manager().get(id).is_some())
        .collect();
    assert!(
        still_registered.is_empty(),
        "a device that went away leaves nothing behind, but these stayed: {still_registered:?}"
    );
}
