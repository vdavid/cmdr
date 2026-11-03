//! What the SESSION layer tells the host it was handed, against a real device.
//!
//! Every seam here is a `dyn` call that goes nowhere on a detached host, so a
//! reach that regressed back to a `crate::` path would still compile, still pass
//! every other suite, and only show up as a counter that stopped arriving or an
//! index that stayed Fresh through a dead session. These cells are the instrument
//! that notices.
//!
//! The listing seam is the other half, and it's the volume's rather than this
//! layer's: `volume::host_seam_test`.

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::analytics::RecordingAnalytics;

use crate::connection::events::no_device_events;
use crate::connection::{DeviceWatch, MtpConnectionManager, MtpDisconnectReason, MtpVolumeRegistrar};
use crate::testing::{device_lock, is_attached, recording_registrar};
use crate::virtual_device::{setup_virtual_mtp_device, unregister_virtual_mtp_device};

/// A device connecting is a thing a user did, so it earns one counter. It has to
/// travel the `AnalyticsSink` seam rather than the app's analytics client: the
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
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let device_id = crate::list_mtp_devices()
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

/// A device that vanishes under the event loop has to leave the registrar the
/// same way an explicit disconnect does.
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
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let device_id = crate::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let manager = MtpConnectionManager::new(VolumeHost::detached(), no_device_events(), recording_registrar());
    let info = manager
        .connect(&device_id, DeviceWatch::Off)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_ids: Vec<u32> = info.storages.iter().map(|storage| storage.id).collect();
    assert!(
        storage_ids.iter().all(|id| is_attached(&device_id, *id)),
        "the connect has to attach every storage first, or this proves nothing"
    );

    manager.handle_device_disconnected(&device_id).await;
    unregister_virtual_mtp_device(fixture.location_id);

    let still_attached: Vec<&u32> = storage_ids.iter().filter(|id| is_attached(&device_id, **id)).collect();
    assert!(
        still_attached.is_empty(),
        "a device that went away leaves nothing behind, but these storages stayed: {still_attached:?}"
    );
}

/// Two connects for one device that overlap open it ONCE.
///
/// Three callers reach `connect()` on their own schedule: the hotplug watcher's
/// auto-connect retry loop (#834), the user's `connect_mtp_device` (the dialogs'
/// Retry), and the session-reset reopen loop. The registry check alone is a
/// check-then-act, so two that overlapped both opened the device: on real USB
/// the loser's open reads as `ExclusiveAccess` and raises the ptpcamerad dialog
/// for a phone that just connected, and where both opens succeed the second entry
/// replaces the first and orphans its session.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overlapping_connects_for_one_device_open_it_once() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let device_id = crate::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");

    let analytics = Arc::new(RecordingAnalytics::new());
    let host = VolumeHost::builder().analytics(analytics.clone()).build();
    let manager = MtpConnectionManager::new(host, no_device_events(), MtpVolumeRegistrar::detached());

    // Two connects that really overlap: both queue while the test holds the device's
    // slot, the way a 30 s open holds it, and go once it's let go. Racing two bare
    // tasks instead caught the double open only about one run in a hundred.
    let held = manager.connect_gates.claim(&device_id).await;
    let spawn_connect = || {
        let (manager, device_id) = (Arc::clone(&manager), device_id.clone());
        tokio::spawn(async move { manager.connect(&device_id, DeviceWatch::Off).await })
    };
    let (first, second) = (spawn_connect(), spawn_connect());
    cmdr_fs::testing::wait_until_async(std::time::Duration::from_secs(5), "both connects to queue", || {
        manager.connect_gates.claimants(&device_id) == 3
    })
    .await;
    assert!(
        !first.is_finished() && !second.is_finished(),
        "a connect must wait while another one for the same device is under way"
    );
    drop(held);
    let first = first.await.unwrap().expect("the first connect succeeds");
    let second = second
        .await
        .unwrap()
        .expect("the overlapping connect gets the same session");
    assert_eq!(
        first.storages.iter().map(|s| s.id).collect::<Vec<_>>(),
        second.storages.iter().map(|s| s.id).collect::<Vec<_>>()
    );

    manager
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("disconnecting a device we just connected");
    unregister_virtual_mtp_device(fixture.location_id);

    assert_eq!(analytics.events().len(), 1, "one open, so one counter");
}
