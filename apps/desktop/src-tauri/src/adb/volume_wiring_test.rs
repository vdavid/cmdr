//! Calling an ADB connect off, applying the settings live, and a pane listing a
//! dialed phone through the real listing pipeline.
//!
//! The cancel cell dials a listener that accepts and never answers, so the
//! attempt is provably still in flight when the cancel lands: a fake that
//! answers would race the cancel and the cell would pass for the wrong reason.

use std::path::Path;

use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use cmdr_adb::AdbEndpoint;

use super::*;
use crate::file_system::listing::caching_test_support::{TestListingGuard, unique_test_id};
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::listing::streaming::{
    CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
};

/// An endpoint whose server accepts connections and never says anything, so a
/// dial against it hangs until something calls it off. `at_without_adb` because
/// there is no binary to spawn for a socket that is already listening.
async fn a_server_that_never_answers() -> (AdbEndpoint, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let held = tokio::spawn(async move {
        let mut sockets = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            sockets.push(stream);
        }
    });
    (AdbEndpoint::at_without_adb(addr), held)
}

#[tokio::test]
async fn a_connect_the_user_calls_off_answers_cancelled_and_leaves_no_volume() {
    let (endpoint, held) = a_server_that_never_answers().await;
    let attempt_id = "adb-cancel-cell";
    let dial = tokio::spawn(connect_device_at(
        AdbConnectionParams::at("R58M-cancel-cell", endpoint),
        attempt_id,
    ));

    // The dial files its attempt before its first await on the wire, so the
    // first cancel that finds it is the one that lands. ❗ A single try would
    // race the spawn itself.
    crate::test_support::wait_until_async(
        std::time::Duration::from_secs(5),
        "the adb connect attempt to be cancelable",
        || cancel_connect(attempt_id),
    )
    .await;

    let outcome = dial.await.expect("the dial task ran");
    assert!(
        matches!(outcome, Err(AdbConnectError::Cancelled)),
        "a called-off dial says so; got {outcome:?}"
    );
    assert!(
        device_provider::connected_volume("R58M-cancel-cell").is_none(),
        "a cancelled connect leaves no volume behind"
    );
    held.abort();
}

#[tokio::test]
async fn cancelling_an_id_nobody_is_dialing_under_is_a_plain_no() {
    assert!(!cancel_connect("adb-nothing-is-filed-under-this"));
}

/// ❗ Turning ADB off has to take the ROWS away too, not only the subscription:
/// a stopped tracker on its own leaves the last device list frozen on screen and
/// its volumes registered, so the phone looks browsable and answers nothing.
#[tokio::test(flavor = "multi_thread")]
async fn turning_adb_off_stops_the_tracker_and_empties_the_device_list() {
    let fake = cmdr_adb::testing::FakeAdbServer::start(cmdr_adb::testing::FakeTree::new()).await;

    apply_settings_at(fake.endpoint(), true, None).await;
    crate::test_support::wait_until_async(
        std::time::Duration::from_secs(5),
        "the tracker to deliver the fake server's device list",
        || !device_provider::cached_devices().is_empty(),
    )
    .await;
    assert!(adb_install_status().tracking, "an enabled ADB follows the server");

    apply_settings_at(fake.endpoint(), false, None).await;
    assert!(!adb_install_status().tracking, "a disabled ADB follows nothing");
    assert!(device_provider::cached_devices().is_empty(), "and the rows go with it");

    // Back on: the subscription comes back without a restart, which is the whole
    // point of a live-applied setting.
    apply_settings_at(fake.endpoint(), true, None).await;
    crate::test_support::wait_until_async(std::time::Duration::from_secs(5), "the tracker to come back", || {
        !device_provider::cached_devices().is_empty()
    })
    .await;

    apply_settings_at(fake.endpoint(), false, None).await;
}

/// ❗ A pane holds a phone as `adb://<serial>/sdcard`, and
/// `read_directory_with_progress` hands that path to the volume untouched. So
/// the volume has to read its own prefix, and hand entries back in the same
/// spelling, or opening a subfolder sends a bare `/sdcard/x` to path resolution,
/// which lands on the Mac's boot disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_on_an_adb_path_lists_the_phone_through_the_listing_pipeline() {
    const SERIAL: &str = "R58M-Listing-Cell";
    let mut tree = cmdr_adb::testing::FakeTree::new();
    tree.add_file("/sdcard/photo.jpg", b"jpeg").add_dir("/sdcard/DCIM");
    let fake = cmdr_adb::testing::FakeAdbServer::start(tree).await;
    fake.push_devices(vec![cmdr_adb::AdbDevice {
        serial: SERIAL.to_string(),
        ..cmdr_adb::testing::fake_device()
    }]);
    let volume_id = connect_device_at(AdbConnectionParams::at(SERIAL, fake.endpoint()), "adb-listing-cell")
        .await
        .expect("the fake phone dials");

    let listing = TestListingGuard::adopt(unique_test_id("adb-pane-listing"));
    let sink = Arc::new(CollectorListingEventSink::new());
    let events: Arc<dyn ListingEventSink> = Arc::clone(&sink) as Arc<dyn ListingEventSink>;
    let state = Arc::new(StreamingListingState {
        cancel: CancellationToken::new(),
    });
    let pane_path = format!("adb://{SERIAL}/sdcard");
    let outcome = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        &volume_id,
        Path::new(&pane_path),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;
    assert!(outcome.is_ok(), "the listing pipeline reads the phone; got {outcome:?}");
    assert!(sink.errors.lock().unwrap().is_empty(), "no listing error was emitted");

    let mut paths: Vec<String> = listing.entries().into_iter().map(|e| e.path).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        vec![format!("{pane_path}/DCIM"), format!("{pane_path}/photo.jpg")],
        "entries carry the pane's own spelling, so opening one stays on the phone"
    );

    drop(listing);
    get_volume_manager().unregister(&volume_id);
    device_provider::forget_volume(SERIAL);
}

#[test]
fn an_empty_binary_path_hands_the_search_back_to_the_platform() {
    set_adb_binary_path(Some("   ".to_string()));
    assert_eq!(
        cmdr_adb::locate_adb_binary(),
        {
            set_adb_binary_path(None);
            cmdr_adb::locate_adb_binary()
        },
        "a blanked-out Settings field means the platform search, never a path of spaces"
    );
}
