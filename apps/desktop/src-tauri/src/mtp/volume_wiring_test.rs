//! The registrar this module hands the session layer, from the app's side of it.
//!
//! Both cells assert on the APP's volume registry rather than on what the backend
//! does with a device, which is why they sit here rather than in `cmdr-mtp`: a
//! connect has to leave a browsable volume behind for every storage the device
//! reported, and it has to do that BEFORE the device's event loop can report into
//! one. The crate's `connection::host_seam_test` asks the registrar seam the same
//! question with no app around it; this is the half where the answer is a real
//! `MtpVolume` in a real registry.

use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;
use crate::mtp::test_support::{connect_virtual_device, device_lock, teardown};
use crate::mtp::{DeviceWatch, MtpConnectionManager, MtpDisconnectReason, MtpVolumeRegistrar};
use cmdr_mtp::virtual_device::{setup_virtual_mtp_device, unregister_virtual_mtp_device};

/// Connecting a device must leave a browsable volume behind for EVERY storage it
/// reported, and disconnecting must take them all away again.
///
/// The session layer doesn't do this itself; it calls the `MtpVolumeRegistrar`
/// its manager was built with. Two things are pinned here:
///
/// 1. The registration happens at all (drop the registrar and this goes red).
/// 2. It happens SYNCHRONOUSLY, inside `connect()`. There's no polling or
///    `wait_until` below on purpose: `connect()` attaches the volumes before it
///    starts the device's event loop, and an attach that merely got scheduled
///    would let the loop's first event race a registry that doesn't know the
///    volume yet. If someone spawns the hook, this assertion fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_attaches_a_volume_for_every_storage_and_disconnect_detaches_them() {
    let _guard = device_lock().await;
    let device = connect_virtual_device().await;

    assert!(
        device.storage_ids.len() > 1,
        "the virtual device reports two storages, so this covers the per-storage loop"
    );
    let volume_ids: Vec<String> = device
        .storage_ids
        .iter()
        .map(|storage_id| cmdr_fs::volume::mtp_ids::mtp_volume_id(&device.id, *storage_id))
        .collect();
    for volume_id in &volume_ids {
        assert!(
            get_volume_manager().get(volume_id).is_some(),
            "storage {volume_id} must be browsable the moment connect() returns"
        );
    }

    teardown(device).await;
    for volume_id in &volume_ids {
        assert!(
            get_volume_manager().get(volume_id).is_none(),
            "storage {volume_id} must be gone the moment disconnect() returns"
        );
    }
}

/// Every attach that a connect performs, and the thread it ran on.
static ATTACH_THREADS: std::sync::Mutex<Vec<std::thread::ThreadId>> = std::sync::Mutex::new(Vec::new());

/// The app's real registrar with a note taken first, so a test can see WHERE the
/// attach ran as well as what it produced.
fn thread_recording_registrar() -> MtpVolumeRegistrar {
    MtpVolumeRegistrar {
        attach: |manager, device_id, storage_id, storage_name| {
            ATTACH_THREADS.lock_ignore_poison().push(std::thread::current().id());
            (super::volume_wiring::volume_registrar().attach)(manager, device_id, storage_id, storage_name);
        },
        detach: |device_id, storage_id| {
            (super::volume_wiring::volume_registrar().detach)(device_id, storage_id);
        },
    }
}

/// The same registration ordering with the change watch RUNNING, pinned by WHERE
/// the attach ran rather than by when it was observed.
///
/// ❗ Asserting only that the volumes exist when `connect()` returns does NOT pin
/// this: `host.runtime()` is a different runtime from the test's, so a scheduled
/// attach usually still wins that race and the cell passes green. What a
/// scheduled attach cannot do is run on the CALLER's thread, which under the
/// single-threaded flavor below is this test's own. Why the ordering matters:
/// `mtp/DETAILS.md` § "Backends never register themselves".
#[tokio::test(flavor = "current_thread")]
async fn a_live_watch_never_starts_before_the_volumes_it_reports_into_exist() {
    let _guard = device_lock().await;
    ATTACH_THREADS.lock_ignore_poison().clear();
    let caller = std::thread::current().id();

    let fixture = setup_virtual_mtp_device();
    let location_id = fixture.location_id;
    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|device| device.location_id == location_id)
        .map(|device| device.id)
        .expect("the virtual device must appear in discovery");

    // A SECOND manager, over the app's real host but with a registrar that
    // records which thread each attach ran on. The device lock this cell holds is
    // what keeps it from racing the parked one.
    let manager = MtpConnectionManager::new(
        crate::volume_host::host(),
        cmdr_mtp::no_device_events(),
        thread_recording_registrar(),
    );
    let info = manager
        .connect(&device_id, DeviceWatch::Live)
        .await
        .expect("virtual-mtp connect should succeed");

    let attach_threads = ATTACH_THREADS.lock_ignore_poison().clone();
    assert_eq!(
        attach_threads.len(),
        info.storages.len(),
        "connect() must attach every storage it reports, and no more"
    );
    assert!(
        attach_threads.iter().all(|thread| *thread == caller),
        "every attach must run inline on the thread driving connect(), never on a runtime worker"
    );

    for storage in &info.storages {
        let volume_id = cmdr_fs::volume::mtp_ids::mtp_volume_id(&device_id, storage.id);
        assert!(
            get_volume_manager().get(&volume_id).is_some(),
            "storage {volume_id} must be browsable the moment a WATCHED connect() returns"
        );
    }

    manager
        .disconnect(&device_id, MtpDisconnectReason::User)
        .await
        .expect("virtual-mtp disconnect should succeed");
    unregister_virtual_mtp_device(location_id);
}
