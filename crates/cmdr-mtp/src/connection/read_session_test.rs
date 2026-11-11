//! Who holds the device during a streamed read, on both of mtp-rs's paths.
//!
//! A device with partial reads gets a file as bounded windows, and the device
//! lock is free between `read_next_window` calls, so a listing slips in. A
//! device without them gets ONE held-open `GetObject`: the session owns the lock
//! from open to EOF (or cancel), and a listing queued meanwhile waits for it
//! instead of deadlocking against it. `DETAILS.md` § "Bounded-window reads" is
//! the contract these cells pin.
//!
//! The virtual device hands a held-open object over as one chunk (real USB
//! streams 64 KiB reads), so the per-call memory bound on that path is pinned
//! against a scripted download in `read_window_test.rs` instead.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::testing::wait_until_async;

use super::{MTP_READ_PACING, MtpConnectionManager, MtpReadSession, ReadPacing};
use crate::testing::{ConnectedDevice, connect_fixture, device_lock, test_connection_manager};
use crate::virtual_device::{
    VirtualDeviceFixture, rescan_virtual_device, setup_virtual_mtp_device,
    setup_virtual_mtp_device_without_partial_reads,
};

/// Small enough that the fixture spans several windows.
const WINDOW: u32 = 128 * 1024;
/// Generous: each wait is for in-process work on a virtual device.
const WAIT: Duration = Duration::from_secs(5);
/// The fixture: a little over five windows, ending mid-window.
const FILE_LEN: usize = 5 * WINDOW as usize + 12_345;

/// Deterministic bytes, so a reassembled read is checkable against its source.
fn payload() -> Vec<u8> {
    (0..FILE_LEN).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

/// Connects `fixture` with `/big.bin` seeded and the root listed.
async fn connect_with_big_file(fixture: VirtualDeviceFixture) -> ConnectedDevice {
    std::fs::write(fixture.root().join("internal/big.bin"), payload()).expect("seed big.bin on the device");
    rescan_virtual_device();
    connect_fixture(test_connection_manager(), fixture).await
}

async fn open(device: &ConnectedDevice) -> MtpReadSession {
    test_connection_manager()
        .open_read_session(
            &device.id,
            device.storage_id,
            "/big.bin",
            0,
            ReadPacing::fixed_window(WINDOW),
        )
        .await
        .expect("open the read session")
}

async fn next_window(session: &mut MtpReadSession, device: &ConnectedDevice) -> Option<Vec<u8>> {
    test_connection_manager()
        .read_next_window(session, &device.id)
        .await
        .expect("each window reads Ok")
}

/// Reads to EOF, appending to `assembled`.
async fn read_rest(session: &mut MtpReadSession, device: &ConnectedDevice, mut assembled: Vec<u8>) -> Vec<u8> {
    while let Some(window) = next_window(session, device).await {
        assert!(!window.is_empty(), "a window before EOF must not be empty");
        assembled.extend_from_slice(&window);
    }
    assembled
}

/// Whether someone holds the device lock right now (the read session, here).
async fn device_locked(device: &ConnectedDevice) -> bool {
    let device_arc = test_connection_manager()
        .device_arc(&device.id)
        .await
        .expect("the device is connected");
    device_arc.try_lock().is_err()
}

/// Waits until nobody holds the device lock: a dropped session's cancel runs
/// on the runtime, so its release lands a moment after the drop.
async fn wait_until_device_free(device: &ConnectedDevice) {
    let device_arc = test_connection_manager()
        .device_arc(&device.id)
        .await
        .expect("the device is connected");
    wait_until_async(WAIT, "the dropped read to give the device back", || {
        device_arc.try_lock().is_ok()
    })
    .await;
}

/// A root listing on its own task, the way a pane's nav arrives during a copy.
fn spawn_listing(device: &ConnectedDevice) -> tokio::task::JoinHandle<()> {
    let manager: Arc<MtpConnectionManager> = Arc::clone(test_connection_manager());
    let (id, storage_id) = (device.id.clone(), device.storage_id);
    tokio::spawn(async move {
        manager
            .invalidate_listing_cache(&id, storage_id, std::path::Path::new("/"))
            .await;
        manager
            .list_directory(&id, storage_id, "/")
            .await
            .expect("a listing during a read must succeed, not time out or desync");
    })
}

/// After a stopped read: a listing works and a fresh read is byte-exact, so
/// the stop left the PTP session clean.
async fn assert_session_clean(device: &ConnectedDevice) {
    spawn_listing(device).await.expect("listing task");
    let mut fresh = open(device).await;
    assert_eq!(
        read_rest(&mut fresh, device, Vec::new()).await,
        payload(),
        "a read after the stop gets the exact bytes"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn windows_free_the_device_between_calls_so_a_listing_slips_in() {
    let _guard = device_lock().await;
    let device = connect_with_big_file(setup_virtual_mtp_device()).await;

    let mut session = open(&device).await;
    assert!(!device_locked(&device).await, "opening a windowed read holds nothing");
    let first = next_window(&mut session, &device).await.expect("not EOF");
    assert_eq!(first.len(), WINDOW as usize, "a full window per call");
    assert!(!device_locked(&device).await, "between windows the device is free");

    // A foreground op pending now gets the yield: the read holds nothing.
    let gate = test_connection_manager().priority_gate(&device.id).await.expect("gate");
    let pending = gate.foreground_guard();
    assert!(test_connection_manager().transfer_should_yield(&device.id).await);
    drop(pending);

    // The listing finishes while the read is parked between windows.
    spawn_listing(&device).await.expect("listing task");

    let assembled = read_rest(&mut session, &device, first).await;
    assert_eq!(assembled, payload(), "windows reassemble to the exact source bytes");
    assert_eq!(session.bytes_read(), FILE_LEN as u64);
    device.teardown(test_connection_manager()).await;
}

/// Production pacing, end to end on the virtual device: a file past the first
/// adaptive window reads byte-exact and the device is free after EOF. The
/// per-call bound on a window longer than the limit is pinned against a script
/// in `read_window_test.rs`, since the virtual device's windows take no time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_pacing_reads_a_multi_window_file_byte_exact() {
    let _guard = device_lock().await;
    let fixture = setup_virtual_mtp_device();
    let limit = MTP_READ_PACING.call_limit as usize;
    let source: Vec<u8> = (0..limit + 3 * 1024 * 1024 + 4321)
        .map(|i| ((i * 13 + 5) % 251) as u8)
        .collect();
    std::fs::write(fixture.root().join("internal/paced.bin"), &source).expect("seed paced.bin on the device");
    rescan_virtual_device();
    let device = connect_fixture(test_connection_manager(), fixture).await;

    let mut session = test_connection_manager()
        .open_read_session(&device.id, device.storage_id, "/paced.bin", 0, MTP_READ_PACING)
        .await
        .expect("open the read session");
    let mut assembled = Vec::new();
    while let Some(window) = next_window(&mut session, &device).await {
        assembled.extend_from_slice(&window);
    }
    assert_eq!(
        assembled, source,
        "time-paced windows reassemble to the exact source bytes"
    );
    assert!(!device_locked(&device).await, "nothing holds the device after EOF");
    device.teardown(test_connection_manager()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_without_partial_reads_holds_the_device_until_eof_and_a_listing_waits() {
    let _guard = device_lock().await;
    let device = connect_with_big_file(setup_virtual_mtp_device_without_partial_reads()).await;

    let mut session = open(&device).await;
    assert!(
        device_locked(&device).await,
        "the held-open transfer starts at open, so the session owns the device from there"
    );
    let first = next_window(&mut session, &device).await.expect("not EOF");

    // A listing queued now has to wait: the transfer owns the one PTP session.
    let listing = spawn_listing(&device);
    let gate = test_connection_manager().priority_gate(&device.id).await.expect("gate");
    wait_until_async(WAIT, "the listing to queue as foreground work", || {
        gate.foreground_pending()
    })
    .await;
    assert!(
        !test_connection_manager().transfer_should_yield(&device.id).await,
        "the copy must not park for an op that's queued behind its own transfer"
    );
    // allowed-test-sleep: the subject is that the listing does NOT finish in this window
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !listing.is_finished(),
        "a listing can't run inside a held-open transfer"
    );

    let assembled = read_rest(&mut session, &device, first).await;
    assert_eq!(
        assembled,
        payload(),
        "the held-open transfer delivers the exact source bytes"
    );
    assert_eq!(session.bytes_read(), FILE_LEN as u64);

    // EOF gives the device back, and the queued listing runs: no deadlock.
    listing.await.expect("listing task");
    assert!(!device_locked(&device).await, "nothing holds the device after EOF");
    device.teardown(test_connection_manager()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_a_windowed_read_mid_file_leaves_the_session_clean() {
    let _guard = device_lock().await;
    let device = connect_with_big_file(setup_virtual_mtp_device()).await;

    let mut session = open(&device).await;
    next_window(&mut session, &device).await.expect("not EOF");
    drop(session); // a copy cancel: it stops issuing windows

    assert!(
        !device_locked(&device).await,
        "a windowed read holds nothing to give back"
    );
    assert_session_clean(&device).await;
    device.teardown(test_connection_manager()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_a_held_open_read_mid_transfer_cancels_it_and_frees_the_device() {
    let _guard = device_lock().await;
    let device = connect_with_big_file(setup_virtual_mtp_device_without_partial_reads()).await;

    let session = open(&device).await;
    assert!(device_locked(&device).await, "mid-transfer");
    drop(session); // a copy cancel, or a pane closing mid-drag-out

    wait_until_device_free(&device).await;
    assert_session_clean(&device).await;
    device.teardown(test_connection_manager()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_held_open_read_frees_the_device_before_it_returns() {
    let _guard = device_lock().await;
    let device = connect_with_big_file(setup_virtual_mtp_device_without_partial_reads()).await;

    let mut session = open(&device).await;
    session.cancel().await;
    assert!(
        !device_locked(&device).await,
        "cancel gives the device back before returning"
    );
    assert!(
        next_window(&mut session, &device).await.is_none(),
        "a cancelled session reads as EOF"
    );

    assert_session_clean(&device).await;
    device.teardown(test_connection_manager()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_reads_reuses_the_cached_storage_handle() {
    let _guard = device_lock().await;
    let device = connect_with_big_file(setup_virtual_mtp_device()).await;

    for _ in 0..3 {
        drop(open(&device).await);
    }
    assert_eq!(
        test_connection_manager().storage_lookup_count(&device.id).await,
        1,
        "three opens share one GetStorageInfo, like ranged reads do"
    );
    device.teardown(test_connection_manager()).await;
}
