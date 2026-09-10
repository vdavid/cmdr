//! The capability answers and the connect's refusals, against the fake server.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::SpaceInfo;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection};
use cmdr_fs::volume::{LaneKey, SignInShape, Volume, WatchCoverage, adb_volume_id};
use tokio_util::sync::CancellationToken;

use super::testing::{FIXTURE_SERIAL, connect_fake, detached_volume, fixture_path};
use super::{ConnectionState, connect_adb_volume};
use crate::devices::{AdbDevice, AdbDeviceState};
use crate::errors::AdbConnectError;
use crate::params::AdbConnectionParams;
use crate::testing::{FakeAdbServer, FakeTree, fake_device};

#[test]
fn the_device_anchored_answers() {
    let volume = detached_volume();
    assert_eq!(
        volume.root(),
        fixture_path(""),
        "rooted at the device's app prefix, no trailing slash"
    );
    assert_eq!(volume.volume_id(), adb_volume_id(FIXTURE_SERIAL));
    assert_eq!(volume.lane_key(), LaneKey::new(format!("adb:{FIXTURE_SERIAL}")));
    assert!(volume.rerooted(Path::new("/other")).is_none());
    assert!(volume.supports_export());
    assert!(volume.is_writable());
    assert!(volume.supports_streaming());
    assert!(!volume.can_watch_listings());
    assert_eq!(
        volume.listing_watch_coverage(&fixture_path("/sdcard")),
        WatchCoverage::None
    );
    assert!(!volume.supports_local_fs_access());
    assert!(!volume.paths_are_os_visible());
    assert!(!volume.operations_are_local());
    assert!(volume.local_path().is_none());
    assert!(!volume.create_directory_errors_on_existing_dir());
    assert_eq!(volume.space_poll_interval(), Some(Duration::from_secs(30)));
    assert_eq!(volume.sign_in_prompt(), SignInShape::Nothing);
    assert!(volume.retirement().is_some());
    assert_eq!(volume.session_state(), ConnectionState::Connected);
    // The pure fold the frontend reads agrees with the predicates.
    let capabilities = volume.capabilities();
    assert!(capabilities.can_export);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connecting_names_the_device_and_says_hello() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    assert_eq!(volume.name(), "Fake Phone");
    assert_eq!(volume.device_name(), "Fake Phone");
    assert_eq!(volume.serial(), FIXTURE_SERIAL);
    assert!(volume.features().shell_v2);
    assert!(volume.exists(Path::new("/")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_serial_the_server_does_not_list_is_device_gone() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let outcome = connect_adb_volume(
        AdbConnectionParams::at("nope", server.endpoint()),
        VolumeHost::detached(),
        CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(outcome, Err(AdbConnectError::DeviceGone(ref s)) if s == "nope"),
        "{:?}",
        outcome.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unauthorized_device_is_reported_as_such() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    server.push_devices(vec![AdbDevice {
        state: AdbDeviceState::Unauthorized,
        ..fake_device()
    }]);
    let outcome = connect_adb_volume(
        AdbConnectionParams::at(FIXTURE_SERIAL, server.endpoint()),
        VolumeHost::detached(),
        CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(outcome, Err(AdbConnectError::Unauthorized(_))),
        "{:?}",
        outcome.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_without_shell_v2_is_too_old() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    server.set_features("stat_v2,ls_v2");
    let outcome = connect_adb_volume(
        AdbConnectionParams::at(FIXTURE_SERIAL, server.endpoint()),
        VolumeHost::detached(),
        CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(outcome, Err(AdbConnectError::DeviceTooOld { .. })),
        "{:?}",
        outcome.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_connect_answers_cancelled() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let outcome = connect_adb_volume(
        AdbConnectionParams::at(FIXTURE_SERIAL, server.endpoint()),
        VolumeHost::detached(),
        cancel,
    )
    .await;
    assert!(
        matches!(outcome, Err(AdbConnectError::Cancelled)),
        "{:?}",
        outcome.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_maps_kinds_sizes_and_symlinked_folders() {
    let mut tree = FakeTree::new();
    tree.add_dir("/sdcard")
        .add_file("/sdcard/a.txt", b"hello")
        .add_dir("/sdcard/DCIM")
        .add_symlink("/sdcard/shortcut", "/sdcard/DCIM")
        .add_symlink("/sdcard/alias.txt", "/sdcard/a.txt");
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;

    let progress = std::sync::Mutex::new(Vec::new());
    let entries = volume
        .list_directory(&fixture_path("/sdcard"), Some(&|p| progress.lock().unwrap().push(p)))
        .await
        .expect("list");
    let by_name = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(by_name("a.txt").size, Some(5));
    assert_eq!(
        Path::new(&by_name("a.txt").path),
        fixture_path("/sdcard/a.txt"),
        "an entry carries the pane's spelling"
    );
    assert!(by_name("DCIM").is_directory);
    assert!(by_name("shortcut").is_symlink, "a link stays a link");
    assert!(
        by_name("shortcut").is_directory,
        "a link to a folder navigates like one"
    );
    assert!(by_name("alias.txt").is_symlink);
    assert!(!by_name("alias.txt").is_directory);
    let progress = progress.lock().unwrap();
    assert_eq!(
        progress.last().map(|p| p.entries()),
        Some(4),
        "one final report at least"
    );
}

/// ❗ **The pane addresses a phone as `adb://<serial>[/device path]`**, and a
/// listing has to hand back entries in that same spelling: a pane opens a
/// subfolder by passing its entry's path straight back, and a bare `/sdcard/x`
/// resolves to the Mac's boot disk. The serial keeps its exact case (only the
/// volume id's slug is folded).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_through_the_app_prefix_reaches_the_device_and_hands_back_prefixed_paths() {
    const SERIAL: &str = "46061FDAS000A4";
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/a.txt", b"hello").add_dir("/sdcard/DCIM");
    let server = FakeAdbServer::start(tree).await;
    server.push_devices(vec![AdbDevice {
        serial: SERIAL.to_string(),
        ..fake_device()
    }]);
    let (volume, _) = connect_fake(&server, SERIAL).await;
    let prefix = format!("adb://{SERIAL}");

    let root = volume
        .list_directory(Path::new(&prefix), None)
        .await
        .expect("the device root lists through its app prefix");
    let sdcard = root
        .iter()
        .find(|e| e.name == "sdcard")
        .expect("/sdcard is on the device");
    assert_eq!(sdcard.path, format!("{prefix}/sdcard"));

    let entries = volume
        .list_directory(Path::new(&sdcard.path), None)
        .await
        .expect("a folder lists through the path its entry carried");
    let mut paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        vec![format!("{prefix}/sdcard/DCIM"), format!("{prefix}/sdcard/a.txt")]
    );

    let file = volume
        .get_metadata(Path::new(&format!("{prefix}/sdcard/a.txt")))
        .await
        .expect("a prefixed file stats");
    assert_eq!(file.path, format!("{prefix}/sdcard/a.txt"));
    assert_eq!(file.size, Some(5));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_of_a_missing_directory_carries_the_path() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    // Prefixed, so the refusal is the device's `ENOENT` and not the translation's.
    let outcome = volume.list_directory(&fixture_path("/nowhere"), None).await;
    assert!(
        matches!(outcome, Err(cmdr_fs::volume::VolumeError::NotFound(ref p)) if p == "/nowhere"),
        "{outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_phones_space_is_its_shared_storage_not_the_system_image_at_the_root() {
    // ❗ A phone's `/` is a read-only system image that reports 0 free. Asking
    // `df` about the volume root put "0 bytes free" in the pane and refused every
    // copy onto a real Pixel. The fake's `/sdcard` answers with that Pixel's
    // own `df -k /sdcard` line, which names the `/storage/emulated` mount.
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    let space = volume.get_space_info().await.expect("df -k");
    assert_eq!(space, PIXEL_SHARED_STORAGE);
}

/// What the Pixel's `df -k /sdcard` comes to (`testing::pixel_captures`).
const PIXEL_SHARED_STORAGE: SpaceInfo = SpaceInfo::Bounded {
    total_bytes: 114_786_388 * 1024,
    available_bytes: 26_956_476 * 1024,
    used_bytes: (114_786_388 - 26_956_476) * 1024,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_at_a_path_is_the_filesystem_holding_it() {
    let mut tree = FakeTree::new();
    tree.add_dir("/sdcard/Download")
        .add_dir("/sdcard/DCIM/Camera")
        .add_dir("/storage/1A2B-3C4D/Music")
        .mount(crate::testing::FakeMount::sized(
            "/storage/1A2B-3C4D",
            "/dev/fuse",
            60_000_000,
            59_000_000,
        ));
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;

    let card = volume
        .get_space_info_at(&fixture_path("/storage/1A2B-3C4D/Music"))
        .await
        .expect("df -k on the SD card");
    assert_eq!(card.available_bytes(), Some(59_000_000 * 1024));
    let shared = volume
        .get_space_info_at(&fixture_path("/sdcard/Download"))
        .await
        .expect("df -k on the shared storage");
    assert_eq!(shared, PIXEL_SHARED_STORAGE);
    let nested = volume
        .get_space_info_at(&fixture_path("/sdcard/DCIM/Camera"))
        .await
        .expect("df -k on a folder deep in the shared storage");
    assert_eq!(nested, PIXEL_SHARED_STORAGE);
    // The root really is the full system image; only the volume's own figure
    // stands for the phone.
    let root = volume
        .get_space_info_at(&fixture_path(""))
        .await
        .expect("df -k on the root");
    assert_eq!(
        root,
        SpaceInfo::Bounded {
            total_bytes: 904_496 * 1024,
            available_bytes: 0,
            used_bytes: 904_496 * 1024,
        }
    );

    // A `df` that can't answer is "can't tell", never a zero.
    let missing = volume.get_space_info_at(&fixture_path("/nowhere")).await;
    assert!(
        matches!(missing, Err(cmdr_fs::volume::VolumeError::NotSupported)),
        "{missing:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_device_is_reported_once_and_a_reconnect_reports_the_way_back() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn cmdr_fs::volume::host::events::VolumeEventSink>)
        .build();
    let volume = connect_adb_volume(
        AdbConnectionParams::at(FIXTURE_SERIAL, server.endpoint()),
        host,
        CancellationToken::new(),
    )
    .await
    .expect("connect");

    volume.note_device_gone();
    volume.note_device_gone();
    assert_eq!(volume.session_state(), ConnectionState::Disconnected);
    assert_eq!(
        events.transitions(),
        vec![(volume.volume_id().to_string(), VolumeConnection::Disconnected)],
        "transitions, never states"
    );

    volume.attempt_reconnect().await.expect("the fake is still there");
    assert_eq!(volume.session_state(), ConnectionState::Connected);
    assert_eq!(events.transitions().len(), 2);
    assert_eq!(events.transitions()[1].1, VolumeConnection::Connected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_write_patches_the_pane_once() {
    let mut tree = FakeTree::new();
    tree.add_dir("/sdcard");
    let server = FakeAdbServer::start(tree).await;
    let (volume, listings) = connect_fake(&server, FIXTURE_SERIAL).await;

    volume
        .create_file(&fixture_path("/sdcard/a.txt"), b"abc")
        .await
        .expect("create");
    assert_eq!(listings.change_count(), 1);
    volume
        .create_directory(&fixture_path("/sdcard/dir"))
        .await
        .expect("mkdir");
    assert_eq!(listings.change_count(), 2);
    volume
        .rename(&fixture_path("/sdcard/a.txt"), &fixture_path("/sdcard/b.txt"), false)
        .await
        .expect("rename");
    assert_eq!(listings.change_count(), 3);
    volume.delete(&fixture_path("/sdcard/b.txt")).await.expect("delete");
    assert_eq!(listings.change_count(), 4);
    // A scan is a read and reports nothing.
    volume.scan_for_copy(&fixture_path("/sdcard")).await.expect("scan");
    assert_eq!(listings.change_count(), 4);
}
