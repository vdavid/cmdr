//! Calling an ADB connect off.
//!
//! The dial runs against a listener that accepts and never answers, so the
//! attempt is provably still in flight when the cancel lands: a fake that
//! answers would race the cancel and the cell would pass for the wrong reason.

use tokio::net::TcpListener;

use cmdr_adb::AdbEndpoint;

use super::*;

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
