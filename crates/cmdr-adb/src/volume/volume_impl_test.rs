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
async fn space_comes_from_df() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    let space = volume.get_space_info().await.expect("df -k");
    assert_eq!(
        space,
        SpaceInfo::Bounded {
            total_bytes: 118_120_468 * 1024,
            available_bytes: 96_764_008 * 1024,
            used_bytes: (118_120_468 - 96_764_008) * 1024,
        }
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
