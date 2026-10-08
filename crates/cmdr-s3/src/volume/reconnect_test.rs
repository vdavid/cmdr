//! The "reconnect automatically" switch and the attended sign-in, on a volume
//! with no client (no server needed) and against a fixture.

use cmdr_fs::volume::{ConnectionState, Volume, VolumeError};

use super::UnattendedReconnect;
use super::test_support::make_test_volume;
use super::testing::*;

const FIXTURE: &str = "s3-servers/start.sh (s3-fixture)";

/// ❗ **Off means no unattended probe happens AT ALL.** This volume has nothing
/// stored, so a probe past the gate could only answer `PermissionDenied`;
/// `NotSupported` has exactly one source, the switch.
#[tokio::test]
async fn the_switch_off_stops_an_unattended_probe_before_it_starts() {
    let volume = make_test_volume(Some("cmdr-test"));
    volume.set_auto_reconnect(false);
    let refusal = volume.attempt_reconnect().await;
    assert!(matches!(refusal, Err(VolumeError::NotSupported)), "got {refusal:?}");
    assert_eq!(volume.unattended_reconnect().await, UnattendedReconnect::SwitchOff);
}

#[tokio::test]
async fn with_nothing_stored_an_unattended_probe_asks_for_a_person() {
    let volume = make_test_volume(Some("cmdr-test"));
    assert_eq!(volume.unattended_reconnect().await, UnattendedReconnect::NoStoredSecret);
    let refusal = volume.attempt_reconnect().await;
    assert!(
        matches!(refusal, Err(VolumeError::PermissionDenied { .. })),
        "got {refusal:?}"
    );
    assert_eq!(volume.connection_state(), Some(ConnectionState::NeedsSignIn));
}

/// ❗ The access key id IS the account, so a sign-in as another key is another
/// volume, never this one rewritten.
#[tokio::test]
async fn a_sign_in_with_another_key_is_refused() {
    let volume = make_test_volume(Some("cmdr-test"));
    let refusal = volume
        .reconnect_with_credentials("AKIASOMEONEELSE".to_string(), "secret".to_string())
        .await;
    assert!(matches!(refusal, Err(VolumeError::NotSupported)), "got {refusal:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn a_dropped_client_comes_back_on_an_unattended_probe() {
    for service in FIXTURE_SERVICES {
        let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
        volume.simulate_session_loss().await;
        let gone = volume.list_directory(volume.root(), None).await;
        assert!(
            matches!(gone, Err(VolumeError::DeviceDisconnected(_))),
            "{}: got {gone:?}",
            service.key
        );
        assert_eq!(volume.connection_state(), Some(ConnectionState::Disconnected));
        volume.attempt_reconnect().await.expect(FIXTURE);
        assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));
        volume.list_directory(volume.root(), None).await.expect(FIXTURE);
    }
}
