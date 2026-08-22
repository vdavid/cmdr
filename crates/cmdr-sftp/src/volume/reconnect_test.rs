//! What a dropped session is allowed to do about itself, rung by rung.
//!
//! The cells split two ways on purpose:
//!
//! - **No server**: the policy gate. A volume built on a rung that may not
//!   reconnect unattended must answer without touching the wire, and pointing the
//!   fixture at a CLOSED port is what proves it did: a dial there answers
//!   "unreachable" and reports `Disconnected`, so an answer of `NeedsCredentials`
//!   can only have come from the gate.
//! - **A real server**: everything that turns on an authentication being
//!   REFUSED. A closed port can't refuse a password, so the one rule that matters
//!   most — a password is tried once and never again unattended — needs a server
//!   that says no.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::{CredentialStore, CredentialsNotStored, StoredCredentials};
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection, VolumeEventSink};
use cmdr_fs::volume::host::host_keys::InMemoryHostKeys;

use super::super::SftpVolume;
use super::super::test_support::{TEST_ROOT, make_test_volume_with};
use super::super::testing::*;
use crate::auth::AuthRungUsed;

const FIXTURE: &str = "sftp-servers/start.sh (sftp-fixture)";

// ── A store that counts, because "exactly once" is a count ───────────

/// A secret store that remembers how many times it was ASKED.
///
/// ❗ The instrument for "the password rung tries once". Every dial reads the
/// store exactly once (`transport::stored_secret` caches it for the dial), so the
/// read count IS the number of authentication attempts — and asserting on it
/// catches a second attempt that a state check would miss, because the state
/// after two rejections looks exactly like the state after one.
struct CountingCredentials {
    entries: std::sync::Mutex<HashMap<(String, Option<String>), StoredCredentials>>,
    reads: AtomicUsize,
}

impl CountingCredentials {
    fn holding(service: &str, scope: &str, secret: &str) -> Arc<Self> {
        let mut entries = HashMap::new();
        entries.insert(
            (service.to_string(), Some(scope.to_string())),
            StoredCredentials {
                username: scope.to_string(),
                secret: secret.to_string(),
            },
        );
        Arc::new(Self {
            entries: std::sync::Mutex::new(entries),
            reads: AtomicUsize::new(0),
        })
    }

    fn empty() -> Arc<Self> {
        Arc::new(Self {
            entries: std::sync::Mutex::new(HashMap::new()),
            reads: AtomicUsize::new(0),
        })
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    /// Replaces what the store holds, the way a user re-saving a password would.
    fn set(&self, service: &str, scope: &str, secret: &str) {
        self.entries.lock_ignore_poison().insert(
            (service.to_string(), Some(scope.to_string())),
            StoredCredentials {
                username: scope.to_string(),
                secret: secret.to_string(),
            },
        );
    }

    /// Takes the secret away, the way a session ending takes a passphrase with it.
    fn forget(&self, service: &str, scope: &str) {
        self.entries
            .lock_ignore_poison()
            .remove(&(service.to_string(), Some(scope.to_string())));
    }

    fn stored(&self, service: &str, scope: &str) -> Option<String> {
        self.entries
            .lock_ignore_poison()
            .get(&(service.to_string(), Some(scope.to_string())))
            .map(|held| held.secret.clone())
    }
}

impl CredentialStore for CountingCredentials {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.entries
            .lock_ignore_poison()
            .get(&(service.to_string(), scope.map(str::to_string)))
            .cloned()
    }

    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        self.entries
            .lock_ignore_poison()
            .insert((service.to_string(), scope.map(str::to_string)), credentials.clone());
        Ok(())
    }
}

// ── The policy gate, with nothing listening ──────────────────────────

/// A volume on `rung` whose server is a closed port, plus the events it reports.
fn offline_volume(rung: AuthRungUsed) -> (Arc<RecordingVolumeEvents>, Arc<CountingCredentials>, SftpVolume) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let credentials = CountingCredentials::holding("127.0.0.1:12599", "ada", "whatever");
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>)
        .host_keys(Arc::new(InMemoryHostKeys::new()))
        .build();
    (events, credentials, make_test_volume_with(TEST_ROOT, rung, host))
}

/// The states a volume reported, in order.
fn reported(events: &RecordingVolumeEvents) -> Vec<VolumeConnection> {
    events.transitions().into_iter().map(|(_, state)| state).collect()
}

/// A passphrase-protected key file cannot come back on its own, and must not try.
///
/// The passphrase isn't held past the session it unlocked, so a dial would offer
/// a key it can't decrypt and get nowhere. ❗ Answering `NeedsCredentials` where
/// a dial would have answered `Disconnected` is what proves the gate ran.
#[tokio::test]
async fn a_passphrase_protected_key_asks_for_a_person_instead_of_dialing() {
    let (events, credentials, volume) = offline_volume(AuthRungUsed::KeyFile {
        passphrase_protected: true,
    });

    let refusal = volume.attempt_reconnect().await;

    assert!(
        matches!(refusal, Err(cmdr_fs::volume::VolumeError::PermissionDenied(_))),
        "only a person moves this forward, got {refusal:?}"
    );
    assert_eq!(reported(&events), vec![VolumeConnection::NeedsCredentials]);
    assert_eq!(credentials.reads(), 0, "❌ no dial, so no secret was even read");
}

/// Keyboard-interactive is the server asking the questions, and there is nobody
/// to answer them.
#[tokio::test]
async fn keyboard_interactive_never_reconnects_unattended() {
    let (events, credentials, volume) = offline_volume(AuthRungUsed::KeyboardInteractive);

    let refusal = volume.attempt_reconnect().await;

    assert!(matches!(
        refusal,
        Err(cmdr_fs::volume::VolumeError::PermissionDenied(_))
    ));
    assert_eq!(reported(&events), vec![VolumeConnection::NeedsCredentials]);
    assert_eq!(credentials.reads(), 0);
}

/// The agent costs the user nothing to ask, so a dropped session just asks again.
///
/// The server here is a closed port, so the attempt fails — the point is that it
/// was MADE, which the `Disconnected` report is the evidence of.
#[tokio::test]
async fn an_agent_session_reconnects_freely() {
    let (events, _credentials, volume) = offline_volume(AuthRungUsed::Agent);

    let refusal = volume.attempt_reconnect().await;

    assert!(
        matches!(refusal, Err(cmdr_fs::volume::VolumeError::DeviceDisconnected(_))),
        "a refused connection is transient, not something a person fixes: {refusal:?}"
    );
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::Disconnected],
        "❗ a rung that may retry must never report NeedsCredentials, or the frontend stops backing off"
    );
}

/// An unencrypted key file needs no secret, so the same applies to it.
#[tokio::test]
async fn an_unencrypted_key_file_reconnects_freely() {
    let (events, _credentials, volume) = offline_volume(AuthRungUsed::KeyFile {
        passphrase_protected: false,
    });

    assert!(matches!(
        volume.attempt_reconnect().await,
        Err(cmdr_fs::volume::VolumeError::DeviceDisconnected(_))
    ));
    assert_eq!(reported(&events), vec![VolumeConnection::Disconnected]);
}

/// A volume the app has forgotten stops reconnecting and stops reporting.
#[tokio::test]
async fn a_retired_volume_neither_reconnects_nor_reports() {
    let (events, credentials, volume) = offline_volume(AuthRungUsed::Agent);
    volume.on_superseded();

    let _ = volume.attempt_reconnect().await;

    assert!(
        events.transitions().is_empty(),
        "its id belongs to a newer instance; news under it would describe a healthy volume as down"
    );
    let _ = credentials.reads();
}

// ── Signing in, rung by rung ─────────────────────────────────────────

/// ❗ The frontend must not offer a sign-in on a rung a typed secret can't mend.
///
/// An agent session and an unencrypted key file are missing nothing; a password
/// box in front of either is a button that can only ever say no.
#[tokio::test]
async fn signing_in_is_refused_where_a_secret_would_not_help() {
    for rung in [
        AuthRungUsed::Agent,
        AuthRungUsed::KeyFile {
            passphrase_protected: false,
        },
    ] {
        let (_events, credentials, volume) = offline_volume(rung);
        let refusal = volume
            .reconnect_with_credentials("ada".to_string(), "hunter2".to_string())
            .await;
        assert!(
            matches!(refusal, Err(cmdr_fs::volume::VolumeError::NotSupported)),
            "{rung:?} has nothing a typed secret fixes, got {refusal:?}"
        );
        assert_eq!(credentials.reads(), 0, "and nothing was dialed");
    }
}

/// ❗ Another account is another volume, not this one signed in differently.
///
/// The volume id is `host:port:username`, so two accounts on one server carry
/// separate identities, separate indexes, and separate remembered paths. Quietly
/// authenticating as somebody else would leave the volume showing one account's
/// name over another account's files.
#[tokio::test]
async fn signing_in_as_a_different_account_is_refused() {
    let (_events, credentials, volume) = offline_volume(AuthRungUsed::Password);

    let refusal = volume
        .reconnect_with_credentials("grace".to_string(), "hunter2".to_string())
        .await;

    assert!(matches!(refusal, Err(cmdr_fs::volume::VolumeError::NotSupported)));
    assert_eq!(credentials.reads(), 0);
}

// ── What only a server that says no can show ─────────────────────────

/// A host wired to a counting store and an event recorder.
fn watched_host(
    params: &crate::SftpConnectionParams,
    secret: Option<&str>,
) -> (Arc<RecordingVolumeEvents>, Arc<CountingCredentials>, VolumeHost) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let credentials = match secret {
        Some(secret) => CountingCredentials::holding(&params.credential_service(), FIXTURE_USER, secret),
        None => CountingCredentials::empty(),
    };
    let host = VolumeHost::builder()
        .events(Arc::clone(&events) as Arc<dyn VolumeEventSink>)
        .credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>)
        .host_keys(Arc::new(InMemoryHostKeys::new()))
        .build();
    (events, credentials, host)
}

/// ❗ **A host key that no longer matches never reaches a sign-in prompt.**
///
/// A changed key is the shape a man-in-the-middle takes, and a password box in
/// front of one is how a password gets typed into it. So the volume reports its
/// own state, the backoff stops, and recovery is the user opening the server
/// again through the full approval flow.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_changed_host_key_stops_the_loop_without_asking_for_a_password() {
    let params = fixture_params("OPENSSH", 12480);
    let (events, credentials, host) = watched_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params.clone()).await;

    // The server's identity changes under a live volume: the store now holds a
    // fingerprint this server will never present.
    host.host_keys().record(
        &params.host,
        params.port,
        "ssh-ed25519",
        "SHA256:notthekeythisserverholds",
    );
    volume.simulate_session_loss().await;
    let reads_before = credentials.reads();

    let refusal = volume.attempt_reconnect().await;

    assert!(
        matches!(refusal, Err(cmdr_fs::volume::VolumeError::PermissionDenied(_))),
        "only a person moves this forward, got {refusal:?}"
    );
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::NeedsHostKeyApproval],
        "❌ never NeedsCredentials: that is the state that puts a password box in front of a possible impostor"
    );
    assert_eq!(
        credentials.reads(),
        reads_before,
        "❌ the key exchange refused before authentication, so no secret was offered"
    );
}

/// ❗ **A password is tried once, and then never again unattended.**
///
/// The frontend's reconnect manager calls `attempt_reconnect` on every backoff
/// tick. Without the latch that is a wrong password offered every few seconds
/// until the server locks the account, which is the one failure in this module
/// that costs the user something they can't undo themselves.
///
/// The password is changed under the volume rather than being wrong from the
/// start, so the cell also pins the other half of the rule: the store is RE-READ
/// on a reconnect, because the user may have fixed it.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_a_refused_password_is_offered_exactly_once() {
    let params = fixture_params("OPENSSH", 12480);
    let (events, credentials, host) = watched_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params.clone()).await;
    assert_eq!(volume.auth_rung(), AuthRungUsed::Password);

    volume.simulate_session_loss().await;
    credentials.set(&params.credential_service(), FIXTURE_USER, "not-the-password");
    let reads_before = credentials.reads();

    let first = volume.attempt_reconnect().await;
    assert!(
        matches!(first, Err(cmdr_fs::volume::VolumeError::PermissionDenied(_))),
        "the server turned the new password down, which only a person fixes: {first:?}"
    );
    assert_eq!(
        credentials.reads(),
        reads_before + 1,
        "the store is re-read on a reconnect: one dial, one read"
    );

    for _ in 0..3 {
        assert!(matches!(
            volume.attempt_reconnect().await,
            Err(cmdr_fs::volume::VolumeError::PermissionDenied(_))
        ));
    }

    assert_eq!(
        credentials.reads(),
        reads_before + 1,
        "❌ every later attempt stops at the latch: three more reads is three more chances to lock the account"
    );
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::NeedsCredentials],
        "and the user is asked once, not once per tick"
    );
}

/// The sign-in that clears the latch: a password the user typed comes back, and
/// it is remembered so the next drop is silent.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_signing_in_with_the_right_password_comes_back_and_is_remembered() {
    let params = fixture_params("OPENSSH", 12480);
    let (events, credentials, host) = watched_host(&params, Some(FIXTURE_PASSWORD));
    let volume = connect_fixture(&host, params.clone()).await;

    volume.simulate_session_loss().await;
    credentials.set(&params.credential_service(), FIXTURE_USER, "not-the-password");
    assert!(volume.attempt_reconnect().await.is_err(), "the stored one is wrong now");

    volume
        .reconnect_with_credentials(FIXTURE_USER.to_string(), FIXTURE_PASSWORD.to_string())
        .await
        .expect(FIXTURE);

    assert!(volume.exists(std::path::Path::new("hello.txt")).await);
    assert_eq!(
        credentials
            .stored(&params.credential_service(), FIXTURE_USER)
            .as_deref(),
        Some(FIXTURE_PASSWORD),
        "a password lives in the store, so saving it is what makes the NEXT reconnect silent"
    );
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::NeedsCredentials, VolumeConnection::Connected],
        "one ask, one recovery"
    );
}

/// ❗ **A key passphrase is used and then forgotten.**
///
/// Putting a passphrase on a key says it isn't to be left lying around. Writing
/// it to the secret store would quietly turn this rung into one that reconnects
/// unattended forever after, which is the opposite of what encrypting the key
/// asked for.
#[tokio::test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
async fn sftp_integration_an_attended_reconnect_never_writes_a_key_passphrase_down() {
    let params = fixture_params("PASSPHRASE", 12482).with_key_file(fixture_key_path("passphrase"));
    let (events, credentials, host) = watched_host(&params, Some(FIXTURE_KEY_PASSPHRASE));
    let volume = connect_fixture(&host, params.clone()).await;
    assert_eq!(
        volume.auth_rung(),
        AuthRungUsed::KeyFile {
            passphrase_protected: true
        }
    );

    // The session ends, and the passphrase ends with it.
    volume.simulate_session_loss().await;
    credentials.forget(&params.credential_service(), FIXTURE_USER);
    let reads_before = credentials.reads();

    assert!(
        matches!(
            volume.attempt_reconnect().await,
            Err(cmdr_fs::volume::VolumeError::PermissionDenied(_))
        ),
        "an encrypted key genuinely cannot come back on its own"
    );
    assert_eq!(
        credentials.reads(),
        reads_before,
        "and it doesn't waste a dial finding out"
    );

    volume
        .reconnect_with_credentials(FIXTURE_USER.to_string(), FIXTURE_KEY_PASSPHRASE.to_string())
        .await
        .expect(FIXTURE);

    assert!(
        volume.exists(std::path::Path::new("hello.txt")).await,
        "the typed passphrase unlocked the key for this session"
    );
    assert_eq!(
        credentials.stored(&params.credential_service(), FIXTURE_USER),
        None,
        "❌ and it was not written down"
    );
    assert_eq!(
        volume.auth_rung(),
        AuthRungUsed::KeyFile {
            passphrase_protected: true
        },
        "so the next drop still needs a person, exactly as before"
    );
    assert_eq!(
        reported(&events),
        vec![VolumeConnection::NeedsCredentials, VolumeConnection::Connected]
    );
}
