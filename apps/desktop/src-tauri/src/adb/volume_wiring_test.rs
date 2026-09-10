//! Calling an ADB connect off, one dial per phone and what it installs, applying
//! the settings live, a pane listing a dialed phone through the real listing
//! pipeline, and the viewer opening a file on one.
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
use crate::file_system::volume::manager::get_volume_manager;

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

/// A fake server listing one ready phone under `serial`.
async fn a_fake_phone(serial: &str) -> cmdr_adb::testing::FakeAdbServer {
    let mut tree = cmdr_adb::testing::FakeTree::new();
    tree.add_dir("/sdcard");
    a_listed_phone(serial, tree).await
}

/// A fake server holding `tree` for one ready phone under `serial`, and the
/// app's cached list carrying that phone too, which is where a pane finds the
/// row it dials: a dial installs only for a phone that list still holds.
async fn a_listed_phone(serial: &str, tree: cmdr_adb::testing::FakeTree) -> cmdr_adb::testing::FakeAdbServer {
    let fake = cmdr_adb::testing::FakeAdbServer::start(tree).await;
    let phone = cmdr_adb::AdbDevice {
        serial: serial.to_string(),
        ..cmdr_adb::testing::fake_device()
    };
    fake.push_devices(vec![phone.clone()]);
    device_provider::apply_device_list(vec![phone]);
    fake
}

/// How many dials reached `fake`: a connect opens with exactly one
/// `host:devices-l`, and nothing else these cells run asks for it.
fn dials_seen(fake: &cmdr_adb::testing::FakeAdbServer) -> usize {
    fake.requests().iter().filter(|r| *r == "host:devices-l").count()
}

/// Waits until `attempt_id` is filed AND `waiting` attempts are in line for the
/// dial on `serial`, without calling anything off. Filing and joining happen in
/// one poll, but a probe from another thread can still land between the two.
async fn wait_until_joined(attempt_id: &str, serial: &str, waiting: usize) {
    crate::test_support::wait_until_async(
        std::time::Duration::from_secs(5),
        "the adb connect attempt to be filed and in line for the dial",
        || ATTEMPTS.is_filed(attempt_id) && attempts_waiting_on(serial) == waiting,
    )
    .await;
}

/// Whether the registry and the provider hold the very same volume for
/// `serial`: the one panes list through, and the one eject, `note_device_gone`,
/// and `space_for_path` reach.
fn registry_and_provider_agree(serial: &str) -> bool {
    let id = cmdr_fs::volume::adb_volume_id(serial);
    match (get_volume_manager().get(&id), device_provider::connected_volume(serial)) {
        (Some(registered), Some(remembered)) => std::ptr::addr_eq(Arc::as_ptr(&registered), Arc::as_ptr(&remembered)),
        _ => false,
    }
}

fn retire_phone(serial: &str) {
    get_volume_manager().unregister(&cmdr_fs::volume::adb_volume_id(serial));
    device_provider::forget_volume(serial);
}

/// ❗ The moment a user taps Allow, several callers reach for the same phone at
/// once. Dialing each on its own registers the FIRST volume and remembers the
/// LAST, so eject and `note_device_gone` reach a volume no pane is using.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_connects_to_one_phone_share_one_dial_and_one_volume() {
    const SERIAL: &str = "R58M-Single-Flight";
    let fake = a_fake_phone(SERIAL).await;
    // Held, so every caller is provably in before the dial can answer.
    fake.hold_answers();
    let attempts = ["adb-single-flight-1", "adb-single-flight-2", "adb-single-flight-3"];
    let dials: Vec<_> = attempts
        .iter()
        .map(|id| tokio::spawn(connect_device_at(AdbConnectionParams::at(SERIAL, fake.endpoint()), id)))
        .collect();
    for id in attempts {
        wait_until_joined(id, SERIAL, 3).await;
    }
    fake.release_answers();

    for dial in dials {
        let volume_id = dial.await.expect("the dial task ran").expect("the fake phone dials");
        assert_eq!(volume_id, cmdr_fs::volume::adb_volume_id(SERIAL));
    }
    assert_eq!(
        dials_seen(&fake),
        1,
        "three callers share one dial on the wire; requests: {:?}",
        fake.requests()
    );
    assert!(
        registry_and_provider_agree(SERIAL),
        "the provider remembers exactly the volume the registry holds"
    );
    retire_phone(SERIAL);
}

/// ❗ Two panes on one phone join one dial, and a cancel is the CALLER's: the
/// pane that called its attempt off hears `Cancelled` straight away, while the
/// dial the other pane still wants runs on and opens the phone for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_one_joined_attempt_leaves_the_dial_to_the_one_still_waiting() {
    const SERIAL: &str = "R58M-Joined-Cancel";
    let fake = a_fake_phone(SERIAL).await;
    fake.hold_answers();
    let staying = tokio::spawn(connect_device_at(
        AdbConnectionParams::at(SERIAL, fake.endpoint()),
        "adb-joined-staying",
    ));
    wait_until_joined("adb-joined-staying", SERIAL, 1).await;
    let leaving = tokio::spawn(connect_device_at(
        AdbConnectionParams::at(SERIAL, fake.endpoint()),
        "adb-joined-leaving",
    ));
    wait_until_joined("adb-joined-leaving", SERIAL, 2).await;

    assert!(cancel_connect("adb-joined-leaving"));
    // Answered while the server is still holding: nothing waits on the wire.
    let left = leaving.await.expect("the leaving dial task ran");
    assert!(
        matches!(left, Err(AdbConnectError::Cancelled)),
        "the attempt called off says so at once; got {left:?}"
    );

    fake.release_answers();
    let volume_id = staying
        .await
        .expect("the staying dial task ran")
        .expect("the attempt nobody called off still opens the phone");
    assert_eq!(volume_id, cmdr_fs::volume::adb_volume_id(SERIAL));
    assert_eq!(
        dials_seen(&fake),
        1,
        "the second pane joined the first one's dial; requests: {:?}",
        fake.requests()
    );
    assert!(registry_and_provider_agree(SERIAL));
    retire_phone(SERIAL);
}

/// ❗ Once the LAST joined attempt is called off, the wire dial goes too, and
/// it leaves nothing behind: no volume registered, nothing remembered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dial_every_joined_attempt_called_off_leaves_nothing_behind() {
    const SERIAL: &str = "R58M-All-Called-Off";
    let fake = a_fake_phone(SERIAL).await;
    fake.hold_answers();
    let first = tokio::spawn(connect_device_at(
        AdbConnectionParams::at(SERIAL, fake.endpoint()),
        "adb-called-off-first",
    ));
    wait_until_joined("adb-called-off-first", SERIAL, 1).await;
    let second = tokio::spawn(connect_device_at(
        AdbConnectionParams::at(SERIAL, fake.endpoint()),
        "adb-called-off-second",
    ));
    wait_until_joined("adb-called-off-second", SERIAL, 2).await;

    assert!(cancel_connect("adb-called-off-first"));
    assert!(cancel_connect("adb-called-off-second"));
    for dial in [first, second] {
        let outcome = dial.await.expect("the dial task ran");
        assert!(matches!(outcome, Err(AdbConnectError::Cancelled)), "got {outcome:?}");
    }

    // Answers flow again, so a dial that was NOT called off would now finish
    // and register. Waiting for the dial to leave the table is what makes the
    // absence below a real observation.
    fake.release_answers();
    crate::test_support::wait_until_async(
        std::time::Duration::from_secs(5),
        "the called-off dial to wind down",
        || !dial_in_flight(SERIAL),
    )
    .await;
    assert!(
        device_provider::connected_volume(SERIAL).is_none(),
        "nothing remembered"
    );
    assert!(
        get_volume_manager()
            .get(&cmdr_fs::volume::adb_volume_id(SERIAL))
            .is_none(),
        "nothing registered"
    );
    assert_eq!(
        dials_seen(&fake),
        1,
        "one dial, called off; requests: {:?}",
        fake.requests()
    );
}

/// ❗ A phone unplugged while its dial is still on the wire must not come back as
/// a volume. The tracker's retirement can't see a dial that hasn't installed
/// yet, so an install that doesn't look again registers a volume for a phone
/// that's gone, and the next plug-in is handed that dead volume without a dial.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dial_whose_phone_left_before_it_installed_answers_gone_and_leaves_nothing_behind() {
    const SERIAL: &str = "R58M-Left-Mid-Dial";
    let fake = a_fake_phone(SERIAL).await;
    // Held, so the phone provably leaves while the dial is still on the wire.
    fake.hold_answers();
    let dial = tokio::spawn(connect_device_at(
        AdbConnectionParams::at(SERIAL, fake.endpoint()),
        "adb-left-mid-dial",
    ));
    wait_until_joined("adb-left-mid-dial", SERIAL, 1).await;

    // The tracker's next push no longer carries the phone. The fake server still
    // does, so the dial itself succeeds on the wire: only the install can refuse.
    device_provider::apply_device_list(Vec::new());
    fake.release_answers();

    let outcome = dial.await.expect("the dial task ran");
    assert!(
        matches!(outcome, Err(AdbConnectError::DeviceGone(_))),
        "a dial for a phone that left says so; got {outcome:?}"
    );
    assert!(
        device_provider::connected_volume(SERIAL).is_none(),
        "nothing remembered"
    );
    assert!(
        get_volume_manager()
            .get(&cmdr_fs::volume::adb_volume_id(SERIAL))
            .is_none(),
        "nothing registered"
    );
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
    let fake = a_listed_phone(SERIAL, tree).await;
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

/// F3 on a phone's file. The viewer can't `std::fs::open` `adb://<serial>/…`, so it
/// pulls the file through the dialed volume into a bounded temp and reads that.
/// Pre-fix the open answered `NotFound` on a real phone, while copying the same file
/// off it worked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_viewer_opens_a_text_file_on_a_dialed_phone() {
    const SERIAL: &str = "R58M-Viewer-Cell";
    let mut tree = cmdr_adb::testing::FakeTree::new();
    tree.add_file("/sdcard/notes.txt", b"first line\nsecond line\n");
    let fake = a_listed_phone(SERIAL, tree).await;
    let volume_id = connect_device_at(AdbConnectionParams::at(SERIAL, fake.endpoint()), "adb-viewer-cell")
        .await
        .expect("the fake phone dials");

    let path = format!("{}/sdcard/notes.txt", cmdr_fs::volume::adb_app_root(SERIAL));
    // The open blocks on the volume's async reads, so it runs where the viewer
    // command runs it: on a blocking thread.
    let opened = tokio::task::spawn_blocking({
        let volume_id = volume_id.clone();
        move || crate::file_viewer::open_session(&path, &volume_id)
    })
    .await
    .expect("the open ran")
    .expect("the phone's file opens");
    assert_eq!(opened.file_name, "notes.txt");
    assert_eq!(opened.initial_lines.lines[0], "first line");
    assert_eq!(opened.initial_lines.lines[1], "second line");

    crate::file_viewer::close_session(&opened.session_id).expect("close");
    get_volume_manager().unregister(&volume_id);
    device_provider::forget_volume(SERIAL);
}

/// Lists a phone the way the tracker does, without anyone dialing it: a row, no
/// volume.
fn list_undialed_phone(serial: &str) {
    install_device_provider();
    device_provider::apply_device_list(vec![cmdr_adb::AdbDevice {
        serial: serial.to_string(),
        ..cmdr_adb::testing::fake_device()
    }]);
}

/// ❗ Listing a phone that is listed but not dialed answers that the device isn't
/// connected, ❌ never `NotFound`: the frontend reads `NotFound` as "this folder
/// was deleted" and walks the pane up and off the phone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_a_listed_phone_nobody_dialed_says_it_is_not_connected() {
    const SERIAL: &str = "R58M-Undialed-Listing";
    list_undialed_phone(SERIAL);

    let listing = TestListingGuard::adopt(unique_test_id("adb-undialed-listing"));
    let events: Arc<dyn ListingEventSink> = Arc::new(CollectorListingEventSink::new());
    let state = Arc::new(StreamingListingState {
        cancel: CancellationToken::new(),
    });
    let pane_path = format!("adb://{SERIAL}/sdcard");
    let outcome = read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        &cmdr_fs::volume::adb_volume_id(SERIAL),
        Path::new(&pane_path),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await;

    assert!(
        matches!(outcome, Err(cmdr_fs::volume::VolumeError::DeviceDisconnected(_))),
        "an undialed phone isn't connected, and nothing was deleted; got {outcome:?}"
    );
    device_provider::apply_device_list(Vec::new());
}

/// ❗ Asking whether a path exists on a listed-but-undialed phone answers
/// "couldn't tell", ❌ never a confident `false`: nothing has looked, and a
/// `false` reads as "gone" to every caller that evicts a pane on it.
#[tokio::test]
async fn path_exists_on_a_listed_phone_nobody_dialed_answers_that_it_couldnt_tell() {
    const SERIAL: &str = "R58M-Undialed-Exists";
    list_undialed_phone(SERIAL);

    let answer = crate::commands::file_system::path_exists(
        Some(cmdr_fs::volume::adb_volume_id(SERIAL)),
        format!("adb://{SERIAL}/sdcard"),
    )
    .await;

    assert!(
        answer.timed_out && !answer.data,
        "an undialed phone can't say either way; got {answer:?}"
    );
    device_provider::apply_device_list(Vec::new());
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
