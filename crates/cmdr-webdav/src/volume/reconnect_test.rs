//! The "reconnect automatically" switch on a mounted volume
//! (`WebdavVolume::set_auto_reconnect`): off means no unattended probe at all,
//! and back on acts at once.
//!
//! The cells without a server drive the gate on a volume with no client; the
//! `#[ignore]`d ones are Docker cells against the stock Apache fixture
//! (`apps/desktop/test/webdav-servers/start.sh`).

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::{CredentialStore, InMemoryCredentials};
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};
use cmdr_fs::volume::{ConnectionState, Volume, VolumeError};

use super::UnattendedReconnect;
use super::fake_dav::FakeDav;
use super::test_support::{make_test_volume, make_test_volume_with};
use super::testing::*;

const FIXTURE: &str = "webdav-servers/start.sh (webdav-fixture)";

// ── Without a server ─────────────────────────────────────────────────

/// ❗ **Off means no unattended probe happens AT ALL.**
///
/// The evidence is the typed refusal: this volume has nothing stored, so a
/// probe that got past the gate could only answer `PermissionDenied`.
/// `NotSupported` has exactly one source, and it is the switch.
#[tokio::test]
async fn the_switch_off_stops_an_unattended_probe_before_it_starts() {
    let volume = make_test_volume("/");
    volume.set_auto_reconnect(false);

    let refusal = volume.attempt_reconnect().await;

    assert!(
        matches!(refusal, Err(VolumeError::NotSupported)),
        "only the switch answers NotSupported; got {refusal:?}"
    );
}

/// Back on hands the probe its ordinary terms again, rather than leaving a
/// volume stuck on the answer the switch produced.
#[tokio::test]
async fn the_switch_back_on_lets_the_probe_through_again() {
    let volume = make_test_volume("/");
    volume.set_auto_reconnect(false);
    assert!(matches!(
        volume.attempt_reconnect().await,
        Err(VolumeError::NotSupported)
    ));

    volume.set_auto_reconnect(true);

    let refusal = volume.attempt_reconnect().await;
    assert!(
        matches!(refusal, Err(VolumeError::PermissionDenied { .. })),
        "past the gate, an empty store is what answers; got {refusal:?}"
    );
}

/// ❗ **What the frontend reads follows the switch**, and the store only when
/// the switch is on.
#[tokio::test]
async fn the_readiness_answer_follows_the_switch_and_the_store() {
    let stored = InMemoryCredentials::new().with_entry("http://127.0.0.1:1", Some("ada"), "ada", "hunter2");
    let volume = make_test_volume_with("/", VolumeHost::builder().credentials(Arc::new(stored)).build());
    assert_eq!(volume.unattended_reconnect().await, UnattendedReconnect::Possible);

    volume.set_auto_reconnect(false);
    assert_eq!(volume.unattended_reconnect().await, UnattendedReconnect::SwitchOff);

    volume.set_auto_reconnect(true);
    assert_eq!(volume.unattended_reconnect().await, UnattendedReconnect::Possible);

    let empty = make_test_volume("/");
    assert_eq!(empty.unattended_reconnect().await, UnattendedReconnect::NoStoredSecret);
}

// ── Signing in, and what it writes down (#530) ───────────────────────

/// ❗ **A typed secret is written only once it WORKED.** A typo written before
/// the dial replaced the remembered secret that worked, and then the dial
/// failed. Port 1 refuses every dial, so whatever the store holds afterwards is
/// what a failed sign-in left behind.
#[tokio::test]
async fn a_sign_in_that_does_not_connect_leaves_the_remembered_secret_alone() {
    let stored = Arc::new(InMemoryCredentials::new().with_entry("http://127.0.0.1:1", Some("ada"), "ada", "hunter2"));
    let volume = make_test_volume_with(
        "/",
        VolumeHost::builder()
            .credentials(Arc::clone(&stored) as Arc<dyn CredentialStore>)
            .build(),
    );

    let refusal = volume
        .reconnect_with_credentials(cmdr_fs::volume::secret_store::SignIn {
            username: "ada".to_string(),
            secret: "a-typo".to_string(),
            remember: false,
        })
        .await;

    assert!(refusal.is_err(), "nothing listens on port 1");
    assert_eq!(
        stored
            .credentials("http://127.0.0.1:1", Some("ada"))
            .map(|held| held.secret),
        Some("hunter2".to_string()),
        "❌ the secret that worked before is still the one remembered"
    );
}

/// The other half: a sign-in that connects keeps a remembered secret current,
/// so the next drop is silent.
#[tokio::test]
async fn a_sign_in_that_connects_refreshes_a_remembered_secret() {
    let server = FakeDav::start().await;
    let service = server.credential_service();

    let remembered = Arc::new(InMemoryCredentials::new().with_entry(&service, Some("ada"), "ada", "the-old-one"));
    let (volume, _events) = server
        .connected_with(Arc::clone(&remembered) as Arc<dyn CredentialStore>)
        .await;
    volume
        .reconnect_with_credentials(cmdr_fs::volume::secret_store::SignIn {
            username: "ada".to_string(),
            secret: "the-new-one".to_string(),
            remember: false,
        })
        .await
        .expect("the fake takes any password");
    assert_eq!(
        remembered.credentials(&service, Some("ada")).map(|held| held.secret),
        Some("the-new-one".to_string())
    );
}

/// ❗ **A checked Remember box is a promise**: a sign-in that connects with it
/// on files the secret even over an empty store. Off, it stays empty.
#[tokio::test]
async fn a_sign_in_that_connects_files_the_secret_the_way_remember_asks() {
    let server = FakeDav::start().await;
    let service = server.credential_service();

    for remember in [true, false] {
        let store = Arc::new(InMemoryCredentials::new().with_entry(&service, Some("ada"), "ada", "dial-me-in"));
        let (volume, _events) = server
            .connected_with(Arc::clone(&store) as Arc<dyn CredentialStore>)
            .await;
        // Remember was off, or turned off: the sheet forgot the entry before this round.
        store.forget(&service, Some("ada"));

        volume
            .reconnect_with_credentials(cmdr_fs::volume::secret_store::SignIn {
                username: "ada".to_string(),
                secret: "typed".to_string(),
                remember,
            })
            .await
            .expect("the fake takes any password");

        assert_eq!(
            store.credentials(&service, Some("ada")).map(|held| held.secret),
            remember.then(|| "typed".to_string()),
            "remember={remember}"
        );
    }
}

/// A store that answers reads and refuses every write: Deny on the Keychain
/// prompt, a locked keychain.
struct RefusesWrites(InMemoryCredentials);

impl CredentialStore for RefusesWrites {
    fn credentials(
        &self,
        service: &str,
        scope: Option<&str>,
    ) -> Option<cmdr_fs::volume::host::credentials::StoredCredentials> {
        self.0.credentials(service, scope)
    }

    fn save_credentials(
        &self,
        _service: &str,
        _scope: Option<&str>,
        _credentials: &cmdr_fs::volume::host::credentials::StoredCredentials,
    ) -> Result<(), cmdr_fs::volume::host::credentials::CredentialsNotStored> {
        Err(cmdr_fs::volume::host::credentials::CredentialsNotStored)
    }
}

/// ❗ **A checked box the store refused is said out loud**, and the connection
/// stays up: the sheet shows `secret_not_stored` from `remember_refused`.
#[tokio::test]
async fn a_refused_remember_is_reported_while_the_connection_stands() {
    let server = FakeDav::start().await;
    let service = server.credential_service();
    let store = RefusesWrites(InMemoryCredentials::new().with_entry(&service, Some("ada"), "ada", "dial-me-in"));
    let (volume, _events) = server.connected_with(Arc::new(store)).await;

    let sign_in = |remember| cmdr_fs::volume::secret_store::SignIn {
        username: "ada".to_string(),
        secret: "typed".to_string(),
        remember,
    };
    let refused = volume
        .reconnect_with_credentials(sign_in(true))
        .await
        .expect("the connection came up");
    assert!(refused.remember_refused);
    assert_eq!(volume.connection_state(), Some(ConnectionState::Direct));

    let unchecked = volume
        .reconnect_with_credentials(sign_in(false))
        .await
        .expect("the connection came up");
    assert!(!unchecked.remember_refused, "no box, no promise to break");
}

/// ❗ **"Try again" after a refused save files the secret WITHOUT a dial** (#603),
/// and a store that refuses again says so again. Port 1 refuses every dial, so
/// the stored secret is evidence the save didn't wait on a server.
#[tokio::test]
async fn remembering_a_sign_in_files_it_without_dialing() {
    let sign_in = |username: &str| cmdr_fs::volume::secret_store::SignIn {
        username: username.to_string(),
        secret: "typed".to_string(),
        remember: true,
    };
    let store = Arc::new(InMemoryCredentials::new());
    let host = VolumeHost::builder()
        .credentials(Arc::clone(&store) as Arc<dyn CredentialStore>)
        .build();
    let volume = make_test_volume_with("/", host);
    let service = volume.inner.params().credential_service();

    let filed = volume
        .remember_sign_in(sign_in("ada"))
        .await
        .expect("filing needs no server");
    assert!(!filed.remember_refused);
    assert_eq!(
        store.credentials(&service, Some("ada")).map(|held| held.secret),
        Some("typed".to_string())
    );

    let another = volume.remember_sign_in(sign_in("grace")).await;
    assert!(matches!(another, Err(VolumeError::NotSupported)), "got {another:?}");

    let refusing = make_test_volume_with(
        "/",
        VolumeHost::builder()
            .credentials(Arc::new(RefusesWrites(InMemoryCredentials::new())))
            .build(),
    );
    let refused = refusing
        .remember_sign_in(sign_in("ada"))
        .await
        .expect("filing needs no server");
    assert!(refused.remember_refused, "a refused store is said out loud again");
}

// ── Against the fixture ──────────────────────────────────────────────

/// ❗ **The switch off leaves a perfectly reachable server alone.**
///
/// Every ingredient of a successful reconnect is present: the server is up and
/// the secret is stored. The volume stays down anyway, and comes back once the
/// switch does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn the_switch_off_leaves_a_reachable_server_alone() {
    if not_for_your_own_server("the seeded `hello.txt`") {
        return;
    }
    let volume = connect_fixture("APACHE", 13480).await;
    let hello = volume.root().join("hello.txt");
    volume.set_auto_reconnect(false);
    assert_eq!(volume.unattended_reconnect().await, UnattendedReconnect::SwitchOff);

    volume.simulate_session_loss().await;

    let refusal = volume.attempt_reconnect().await;
    assert!(
        matches!(refusal, Err(VolumeError::NotSupported)),
        "❌ no probe, however easy one would have been: {refusal:?}"
    );
    assert!(!volume.exists(&hello).await, "and the volume is genuinely still down");

    let first = volume
        .connection_generation()
        .expect("a WebDAV volume reports its generation");
    volume.set_auto_reconnect(true);
    volume.attempt_reconnect().await.expect(FIXTURE);
    assert!(volume.exists(&hello).await);
    assert_eq!(
        volume.connection_generation(),
        Some(first + 1),
        "the rebuilt client is a new generation, and the refused attempt before it moved nothing"
    );

    volume.attempt_reconnect().await.expect(FIXTURE);
    assert_eq!(
        volume.connection_generation(),
        Some(first + 1),
        "a reconnect that found the server live installed nothing"
    );
}

/// ❗ **Switching it back on brings a volume that is already down back**, on
/// its own.
///
/// The one moment the switch would look broken if it only took effect on the
/// NEXT drop: a user flips it on precisely because a volume is sitting there
/// disconnected. The wait covers the first backoff step (2 s) plus a probe, and
/// ❗ stays under nextest's 8 s cap, so a regression fails with its own message
/// rather than as an opaque timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn switching_the_switch_on_revives_a_volume_that_is_already_down() {
    if not_for_your_own_server("the seeded `hello.txt`") {
        return;
    }
    let volume = connect_fixture("APACHE", 13480).await;
    let hello = volume.root().join("hello.txt");
    volume.set_auto_reconnect(false);

    // The client drops and an operation NOTICES, which is what leaves the volume
    // `Disconnected`. With the switch off, no loop starts.
    volume.simulate_session_loss().await;
    assert!(!volume.exists(&hello).await);
    assert!(matches!(
        volume.attempt_reconnect().await,
        Err(VolumeError::NotSupported)
    ));
    assert!(!volume.connection_state().is_some_and(ConnectionState::is_live));

    volume.set_auto_reconnect(true);

    cmdr_fs::testing::wait_until_async(
        Duration::from_secs(6),
        "❗ turning the switch on to act now, not at the next drop",
        || volume.connection_state().is_some_and(ConnectionState::is_live),
    )
    .await;
    assert!(volume.exists(&hello).await, "the rebuilt client serves the root");
}

// ── Which client a failure belongs to ────────────────────────────────

/// A volume with no client, reporting into `events`.
fn recorded_volume() -> (Arc<RecordingVolumeEvents>, super::WebdavVolume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .build();
    (events, make_test_volume_with("/", host))
}

/// ❗ **A late loss from a replaced client leaves the new one connected.**
///
/// A request that went out on the old client and fails after a reconnect
/// installed a fresh one is old news. Flipping the state on it would drop the
/// HEALTHY client, cut every request waiting on it, and hand each an outage it
/// never had. Driven through `noting`, the funnel every delegator uses, so the
/// generation is read where production reads it: before the call runs. The
/// install itself needs a live server; `the_switch_off_leaves_a_reachable_server_alone`
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
            Err(VolumeError::DeviceDisconnected("old client".into()))
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
        .noting(async { Err(VolumeError::DeviceDisconnected("new client".into())) })
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
        .expect("a WebDAV volume reports its generation");

    volume.inner.session.connection().lose(first);
    assert!(
        volume.attempt_reconnect().await.is_err(),
        "nothing is stored to probe with"
    );
    assert_eq!(volume.connection_generation(), Some(first));

    volume.inner.session.connection().install();
    assert_eq!(volume.connection_generation(), Some(first + 1));
}
