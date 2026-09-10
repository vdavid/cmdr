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

/// A background index walk keeps eight listings in flight on a phone: the knee a
/// real phone measured, past which a walk gains nothing and the pane's own
/// listings slow down (`network_scanner/DETAILS.md` § "A backend's own ceiling").
#[test]
fn an_index_walk_keeps_eight_listings_in_flight_on_a_phone() {
    assert_eq!(detached_volume().max_concurrent_scan_listings(), 8);
}

/// An index walk of a phone descends only where a person keeps files: the shared
/// storage the pane reaches through the `/sdcard` link, and each SD card under
/// `/storage`. Every other tree keeps its row and isn't walked: the kernel's views
/// (`/proc`, `/sys`, `/dev`), the system image, private data, and the second paths
/// Android mounts onto the same storage (`/storage/emulated`, `/storage/self`,
/// `/mnt`), which would count every file again.
#[test]
fn an_index_walk_of_a_phone_descends_only_its_storage() {
    use cmdr_fs::volume::IndexWalk::{Descend, RowOnly};

    let volume = detached_volume();
    let walk = |device: &str, is_symlink: bool| volume.index_walk(&fixture_path(device), is_symlink);

    assert_eq!(walk("/", false), Descend, "the root, on the way to storage");
    assert_eq!(walk("/sdcard", true), Descend, "the link IS the phone's storage");
    assert_eq!(
        walk("/sdcard", false),
        Descend,
        "as is a plain /sdcard on an older phone"
    );
    assert_eq!(walk("/sdcard/DCIM", false), Descend);
    assert_eq!(
        walk("/sdcard/DCIM/elsewhere", true),
        RowOnly,
        "a link inside storage is one row"
    );
    assert_eq!(walk("/storage", false), Descend, "on the way to SD cards");
    assert_eq!(walk("/storage/1234-5678", false), Descend, "an SD card");
    assert_eq!(walk("/storage/1234-5678/Music", false), Descend);
    for alias in ["/storage/emulated", "/storage/self"] {
        assert_eq!(walk(alias, false), RowOnly, "{alias} is a second path onto /sdcard");
    }
    for tree in [
        "/proc",
        "/sys",
        "/dev",
        "/data",
        "/data/local/tmp",
        "/mnt",
        "/system",
        "/apex",
        "/vendor",
        "/acct",
    ] {
        assert_eq!(walk(tree, false), RowOnly, "{tree} is not where a person keeps files");
    }
    assert_eq!(
        volume.index_walk(Path::new("adb://ANOTHER-PHONE/sdcard"), true),
        RowOnly,
        "a path on another phone is nothing to walk here"
    );
}

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
}

/// ❗ A copy may target a folder it will create, and the pre-flight asks about
/// that folder before it exists: a missing path answers for the nearest folder
/// above it that does, found by stat, ❌ never by reading `df`'s stderr.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn space_at_a_folder_that_does_not_exist_yet_is_the_filesystem_it_would_land_on() {
    let mut tree = FakeTree::new();
    tree.add_dir("/storage/1A2B-3C4D")
        .mount(crate::testing::FakeMount::sized(
            "/storage/1A2B-3C4D",
            "/dev/fuse",
            60_000_000,
            59_000_000,
        ));
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;

    let deeper = volume
        .get_space_info_at(&fixture_path("/sdcard/New album/Deeper"))
        .await
        .expect("the shared storage answers for a folder two levels short");
    assert_eq!(deeper, PIXEL_SHARED_STORAGE);
    let card = volume
        .get_space_info_at(&fixture_path("/storage/1A2B-3C4D/New"))
        .await
        .expect("the SD card answers for a folder on it");
    assert_eq!(card.available_bytes(), Some(59_000_000 * 1024));
    // A plain space answer even where nothing could be created: `/` is the
    // read-only system image, and saying so is the copy's job, not this one's.
    let top = volume
        .get_space_info_at(&fixture_path("/nowhere"))
        .await
        .expect("the root answers for a folder at the top");
    assert_eq!(top.available_bytes(), Some(0));
}

/// A phone's stat answers `ENOTDIR` for a path under a file, and nothing can
/// land there, so the climb stops at "can't tell" rather than answering for
/// the file's filesystem.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_under_a_file_is_cant_tell_where_the_climb_stops() {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/photo.jpg", b"jpeg");
    let server = FakeAdbServer::start(tree).await;
    assert_eq!(
        server.tree().lock().unwrap().stat("/sdcard/photo.jpg/New").err(),
        Some(crate::errors::ENOTDIR),
        "the fake's stat says what a phone's does"
    );
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    let outcome = volume.get_space_info_at(&fixture_path("/sdcard/photo.jpg/New")).await;
    assert!(
        matches!(outcome, Err(cmdr_fs::volume::VolumeError::NotSupported)),
        "{outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_df_that_fails_anywhere_else_is_cant_tell_never_a_zero() {
    let mut tree = FakeTree::new();
    // No mounts, so `df` fails on every path, the ones that exist included.
    tree.add_dir("/sdcard/Download").unmount_all();
    let server = FakeAdbServer::start(tree).await;
    let (volume, _) = connect_fake(&server, FIXTURE_SERIAL).await;
    for path in ["/sdcard/Download", "/sdcard/Missing"] {
        let outcome = volume.get_space_info_at(&fixture_path(path)).await;
        assert!(
            matches!(outcome, Err(cmdr_fs::volume::VolumeError::NotSupported)),
            "{path}: {outcome:?}"
        );
    }
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
