//! The "reconnect automatically" switch and the attended sign-in, on a volume
//! with no client (no server needed) and against a fixture.

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};
use cmdr_fs::volume::{ConnectionState, Volume, VolumeError};

use super::UnattendedReconnect;
use super::test_support::{make_test_volume, make_test_volume_with};
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
        .reconnect_with_credentials(cmdr_fs::volume::secret_store::SignIn {
            username: "AKIASOMEONEELSE".to_string(),
            secret: "secret".to_string(),
            remember: false,
        })
        .await;
    assert!(matches!(refusal, Err(VolumeError::NotSupported)), "got {refusal:?}");
}

/// ❗ **A typed secret is written only once it WORKED** (#530). Writing it
/// before the dial let a typo replace the secret that worked. Port 1 refuses
/// every dial, so whatever the store holds afterwards is what a failed sign-in
/// left behind, Remember on or off.
#[tokio::test]
async fn a_sign_in_that_does_not_connect_leaves_the_store_alone() {
    use cmdr_fs::volume::host::VolumeHost;
    use cmdr_fs::volume::host::credentials::{CredentialStore, InMemoryCredentials};
    use cmdr_fs::volume::secret_store::SignIn;
    use std::sync::Arc;

    for remember in [false, true] {
        let store = Arc::new(InMemoryCredentials::new());
        let host = VolumeHost::builder()
            .credentials(Arc::clone(&store) as Arc<dyn CredentialStore>)
            .build();
        let volume = make_test_volume_with(Some("cmdr-test"), host);
        // Filed after the volume exists, since the store key is the volume's own.
        let service = volume.inner.params().credential_service();
        store
            .save_credentials(
                &service,
                Some("AKIATEST"),
                &cmdr_fs::volume::host::credentials::StoredCredentials {
                    username: "AKIATEST".to_string(),
                    secret: "right".to_string(),
                },
            )
            .expect("the test store takes it");

        let refusal = volume
            .reconnect_with_credentials(SignIn {
                username: "AKIATEST".to_string(),
                secret: "a-typo".to_string(),
                remember,
            })
            .await;

        assert!(refusal.is_err(), "nothing listens on port 1");
        assert_eq!(
            store.credentials(&service, Some("AKIATEST")).map(|held| held.secret),
            Some("right".to_string()),
            "remember={remember}: ❌ the secret that worked is still the one remembered"
        );
    }
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
        let first = volume
            .connection_generation()
            .expect("an S3 volume reports its generation");
        volume.attempt_reconnect().await.expect(FIXTURE);
        assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));
        volume.list_directory(volume.root(), None).await.expect(FIXTURE);
        assert_eq!(
            volume.connection_generation(),
            Some(first + 1),
            "{}: the rebuilt client is a new generation",
            service.key
        );

        volume.attempt_reconnect().await.expect(FIXTURE);
        assert_eq!(
            volume.connection_generation(),
            Some(first + 1),
            "{}: a reconnect that found the client live installed nothing",
            service.key
        );
    }
}

// ── Which client a failure belongs to ────────────────────────────────

/// A volume with no client, reporting into `events`.
fn recorded_volume() -> (Arc<RecordingVolumeEvents>, super::S3Volume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    (events, make_test_volume_with(Some("cmdr-test"), host))
}

/// ❗ **A late loss from a replaced client leaves the new one connected.**
///
/// A request that went out on the old client and fails after a reconnect
/// installed a fresh one is old news. Flipping the state on it would drop the
/// HEALTHY client, cut every request waiting on it, and hand each an outage it
/// never had. Driven through `noting`, the funnel every delegator uses, so the
/// generation is read where production reads it: before the call runs. The
/// install itself needs a live server; `a_dropped_client_comes_back_on_an_unattended_probe`
/// is its Docker twin.
#[tokio::test]
async fn a_late_loss_from_a_replaced_client_leaves_the_new_one_connected() {
    let (events, volume) = recorded_volume();

    let outcome: Result<(), _> = volume
        .noting(async {
            // While the call is out, a sibling's failure marks the old client
            // lost and a reconnect installs a new one.
            assert!(
                volume
                    .inner
                    .session
                    .connection()
                    .lose(volume.inner.session.connection().generation())
            );
            volume.inner.session.connection().install();
            Err(VolumeError::DeviceDisconnected("old client".to_string()))
        })
        .await;

    assert!(outcome.is_err(), "the call's own answer is passed through untouched");
    assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));
    assert_eq!(
        events
            .transitions()
            .into_iter()
            .map(|(_, state)| state)
            .collect::<Vec<_>>(),
        vec![VolumeConnection::Disconnected, VolumeConnection::Connected],
        "the sibling's loss and the reconnect, and nothing from the late error"
    );

    // The new client's own loss still counts.
    let outcome: Result<(), _> = volume
        .noting(async { Err(VolumeError::DeviceDisconnected("new client".to_string())) })
        .await;
    assert!(outcome.is_err());
    assert_eq!(volume.connection_state(), Some(ConnectionState::Disconnected));
}

/// The reported generation moves only when a client is installed: ❌ not on a
/// loss, ❌ not on a reconnect attempt that didn't take.
#[tokio::test]
async fn only_an_installed_client_moves_the_generation() {
    let (_events, volume) = recorded_volume();
    let first = volume
        .connection_generation()
        .expect("an S3 volume reports its generation");

    volume.inner.session.connection().lose(first);
    assert!(
        volume.attempt_reconnect().await.is_err(),
        "nothing is stored to probe with"
    );
    assert_eq!(volume.connection_generation(), Some(first));

    volume.inner.session.connection().install();
    assert_eq!(volume.connection_generation(), Some(first + 1));
}

/// ❗ **"Try again" after a refused save files the secret WITHOUT a dial** (#603).
/// Port 1 refuses every dial, so a stored secret and an unmoved state are the
/// evidence nothing dialed; another key is refused as the sign-in refuses it.
#[tokio::test]
async fn remembering_a_sign_in_files_it_without_dialing() {
    use cmdr_fs::volume::host::credentials::{CredentialStore, InMemoryCredentials};
    use cmdr_fs::volume::secret_store::SignIn;

    let store = Arc::new(InMemoryCredentials::new());
    let host = VolumeHost::builder()
        .credentials(Arc::clone(&store) as Arc<dyn CredentialStore>)
        .build();
    let volume = make_test_volume_with(Some("cmdr-test"), host);
    let service = volume.inner.params().credential_service();
    let before = volume.connection_state();
    let sign_in = |username: &str| SignIn {
        username: username.to_string(),
        secret: "typed".to_string(),
        remember: true,
    };

    let filed = volume
        .remember_sign_in(sign_in("AKIATEST"))
        .await
        .expect("filing needs no server");

    assert!(!filed.remember_refused);
    assert_eq!(
        store.credentials(&service, Some("AKIATEST")).map(|held| held.secret),
        Some("typed".to_string())
    );
    assert_eq!(volume.connection_state(), before);

    let refusal = volume.remember_sign_in(sign_in("AKIASOMEONEELSE")).await;
    assert!(matches!(refusal, Err(VolumeError::NotSupported)), "got {refusal:?}");
}
