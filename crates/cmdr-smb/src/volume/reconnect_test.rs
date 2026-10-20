//! Session lifecycle with no server in reach: the reconnect early-exits, the
//! state transitions and the events they suppress, the watch-coverage gate, and
//! the two ways a share retires.

use super::*;
use crate::volume::SmbVolume;
use crate::volume::session::refresh_remembered_credentials;
use crate::volume::test_support::*;
use cmdr_fs::volume::host::credentials::{CredentialStore, InMemoryCredentials};
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeConnection};
use cmdr_fs::volume::{Volume, WatchCoverage};
use std::path::Path;

/// The volume id every recorded case reports under.
const RECORDED_ID: &str = "volumestestshare";

/// A disconnected volume whose event seam remembers what the frontend was told.
///
/// The transitions below are the only thing a user ever learns about a session
/// dropping, so "did it emit, and how many times" is the assertion — the state
/// the backend kept for itself is the cheap half.
fn volume_with_recorded_events() -> (SmbVolume, Arc<RecordingVolumeEvents>) {
    let events = Arc::new(RecordingVolumeEvents::new());
    let host = VolumeHost::builder().events(events.clone()).build();
    (make_test_volume_with(RECORDED_ID, host), events)
}

#[tokio::test]
async fn attempt_reconnect_noop_when_already_direct() {
    // If state is Direct, the helper bails early without building a session.
    // This is the path concurrent callers hit after the winner finishes.
    let vol = make_test_volume_direct();
    let result = vol.inner.do_attempt_reconnect().await;
    assert!(result.is_ok(), "expected Ok when already Direct, got {:?}", result);
    assert_eq!(vol.session_state(), ConnectionState::Direct);
}

#[tokio::test]
async fn attempt_reconnect_bails_when_unmounted() {
    // After `on_unmount` runs, reconnect must not try to build a new session
    // (otherwise we'd leak a watcher + smb2 session into an orphaned volume).
    let vol = make_test_volume();
    vol.inner.unmounted.store(true, Ordering::Relaxed);
    let result = vol.inner.do_attempt_reconnect().await;
    assert!(
        matches!(result, Err(VolumeError::DeviceDisconnected(_))),
        "expected DeviceDisconnected when unmounted, got {:?}",
        result
    );
}

#[tokio::test]
async fn single_flight_concurrent_callers_serialize() {
    // Two parallel `do_attempt_reconnect` calls must serialize on
    // `reconnect_lock`. With the volume already Direct, both should return
    // Ok cheaply: the second one observes Direct after the first releases
    // the guard. Mutex contention itself is the assertion that single-flight
    // is wired up; if it wasn't, both calls would race past the early-exit
    // check.
    let vol = Arc::new(make_test_volume_direct());
    let v2 = Arc::clone(&vol);
    let v3 = Arc::clone(&vol);
    let (r1, r2) = tokio::join!(async move { v2.inner.do_attempt_reconnect().await }, async move {
        v3.inner.do_attempt_reconnect().await
    });
    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert_eq!(vol.session_state(), ConnectionState::Direct);
}

/// Several in-flight ops all meet the same broken session, and each one runs the
/// transition. The user hears about it ONCE: a second banner for a share that is
/// already showing as unreachable is noise, and the frontend's backoff cycle
/// restarts on every event it sees.
#[tokio::test]
async fn transition_to_disconnected_idempotent() {
    let (vol, events) = volume_with_recorded_events();
    vol.inner.state.set_for_test(ConnectionState::Direct);

    vol.inner.transition_to_disconnected(SessionGeneration::FIRST);
    vol.inner.transition_to_disconnected(SessionGeneration::FIRST);

    assert_eq!(vol.session_state(), ConnectionState::Disconnected);
    assert_eq!(
        events.transitions(),
        vec![(RECORDED_ID.to_string(), VolumeConnection::Disconnected)],
        "a second op meeting the same dead session must not re-announce it"
    );
}

/// The same on the way back up: a reconnect that raced another one and lost
/// still runs the transition, and a second "connected" would restart the
/// frontend's recovery for a share that is already serving.
#[tokio::test]
async fn transition_to_direct_idempotent() {
    let (vol, events) = volume_with_recorded_events();

    vol.inner.transition_to_direct(SessionGeneration::FIRST);
    vol.inner.transition_to_direct(SessionGeneration::FIRST);

    assert_eq!(vol.session_state(), ConnectionState::Direct);
    assert_eq!(
        events.transitions(),
        vec![(RECORDED_ID.to_string(), VolumeConnection::Connected)],
        "the losing side of a reconnect race must not re-announce the session"
    );
}

/// A retired share still tracks its own state for the holders reading through
/// it, and says NOTHING under the volume id it no longer owns.
///
/// This is the whole point of the flag: the successor is serving that id on its
/// own healthy session, so a predecessor's dying watcher announcing a disconnect
/// would drop the frontend into "unreachable" and start a backoff cycle against
/// a share that never went away.
#[tokio::test]
async fn a_retired_share_announces_nothing_under_an_id_it_no_longer_owns() {
    let (vol, events) = volume_with_recorded_events();
    vol.inner.state.set_for_test(ConnectionState::Direct);
    vol.on_superseded();

    vol.inner.transition_to_disconnected(SessionGeneration::FIRST);

    assert_eq!(
        vol.session_state(),
        ConnectionState::Disconnected,
        "the share still tracks its own state for whoever still holds it"
    );
    assert!(
        events.transitions().is_empty(),
        "the id belongs to the successor, so this share must not speak for it: {:?}",
        events.transitions()
    );
}

#[test]
fn listing_watch_coverage_is_none_when_disconnected() {
    // No watcher_cancel set and state Disconnected: no coverage.
    let vol = make_test_volume();
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::None);
}

#[test]
fn listing_watch_coverage_is_none_when_direct_but_no_watcher() {
    // State Direct but `watcher_cancel` empty: still `None` (we need both).
    let vol = make_test_volume_direct();
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::None);
}

#[test]
fn listing_watch_coverage_is_none_when_watcher_set_but_disconnected() {
    // `watcher_cancel` populated but state Disconnected: `None`.
    let vol = make_test_volume();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.inner.watcher_cancel.lock().unwrap() = Some(tx);
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::None);
}

#[test]
fn listing_watch_coverage_is_every_writer_when_direct_and_watcher_set() {
    // Both conditions met. CHANGE_NOTIFY is raised by the server, so it sees
    // every client's writes, not only ours.
    let vol = make_test_volume_direct();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.inner.watcher_cancel.lock().unwrap() = Some(tx);
    assert_eq!(vol.listing_watch_coverage(Path::new("/")), WatchCoverage::EveryWriter);
}

#[test]
fn on_unmount_marks_volume_dead() {
    // `on_unmount` is sync (called from FSEvents thread) and uses
    // `blocking_lock`, so this must be a `#[test]`, not a `#[tokio::test]`
    // (the latter panics inside a runtime when calling `blocking_lock`).
    let vol = make_test_volume_direct();
    assert!(!vol.inner.unmounted.load(Ordering::Relaxed));
    vol.on_unmount();
    assert!(vol.inner.unmounted.load(Ordering::Relaxed));
    assert_eq!(vol.session_state(), ConnectionState::Disconnected);
}

/// Being superseded is not being unmounted. The successor took the volume id,
/// but this instance may still be serving a running copy, so its session and
/// connection state must survive untouched. Only the id-scoped parts retire.
#[test]
fn on_superseded_retires_the_id_but_keeps_the_session() {
    let vol = make_test_volume_direct();
    let (tx, _rx) = tokio::sync::oneshot::channel::<()>();
    *vol.inner.watcher_cancel.lock().unwrap() = Some(tx);

    vol.on_superseded();

    assert!(vol.inner.is_retired(), "the volume knows it's retired");
    assert!(
        !vol.inner.unmounted.load(Ordering::Relaxed),
        "supersede must not mark the volume dead: holders still use it"
    );
    assert_eq!(
        vol.inner.connection_state(),
        ConnectionState::Direct,
        "the session is still up, so ops on a held reference must not be gated off"
    );
    assert!(
        vol.inner.watcher_cancel.lock().unwrap().is_none(),
        "the watcher belongs to the volume id, which the successor now owns"
    );
}

/// A reply refused for its signature reaches the host's security seam, under the volume's id,
/// once per refusal; any other error tells it nothing.
#[test]
fn a_refused_signature_reaches_the_security_seam() {
    use cmdr_fs::volume::host::events::SecurityRefusal;
    let (vol, events) = volume_with_recorded_events();
    let refused = || smb2::Error::SignatureInvalid {
        command: smb2::types::Command::Read,
        message_id: smb2::types::MessageId(7),
        reason: smb2::error::SignatureFailure::Mismatch,
    };
    let mapped = vol.handle_smb_result::<()>("read", "a.txt", SessionGeneration::FIRST, Err(refused()));
    assert!(matches!(mapped, Err(VolumeError::SignatureInvalid(_))));
    let _ = vol.handle_smb_result::<()>("read", "a.txt", SessionGeneration::FIRST, Err(smb2::Error::Timeout));
    assert_eq!(
        events.refusals(),
        vec![(RECORDED_ID.to_string(), SecurityRefusal::ReplySignatureInvalid)]
    );
}

/// A named login the server answered with a guest session gets its own state, ❌ never the
/// generic sign-in prompt: Cmdr refused the guest session, so the pane says so (#457). Any other
/// credentials refusal is still the sign-in prompt.
#[test]
fn a_guest_session_offered_for_a_named_login_is_its_own_state() {
    let guest = smb2::Error::GuestSessionRefused {
        username: "david".to_string(),
        anonymous: false,
    };
    assert_eq!(credentials_refusal_state(&guest), VolumeConnection::GuestSessionRefused);
    let wrong = smb2::Error::Auth {
        message: "logon failure".to_string(),
    };
    assert_eq!(credentials_refusal_state(&wrong), VolumeConnection::NeedsCredentials);
}

/// A refused reply leaves the signed session in a state nothing can vouch for, so it's dropped
/// like a lost connection: the next operation, the pane's reconnect, or the notice's Reconnect
/// builds a fresh, freshly signed one (#457).
#[tokio::test]
async fn a_refused_signature_drops_the_session() {
    let (vol, events) = volume_with_recorded_events();
    vol.inner.state.set_for_test(ConnectionState::Direct);
    let refused = smb2::Error::SignatureInvalid {
        command: smb2::types::Command::Read,
        message_id: smb2::types::MessageId(7),
        reason: smb2::error::SignatureFailure::Mismatch,
    };
    let _ = vol.handle_smb_result::<()>("read", "a.txt", SessionGeneration::FIRST, Err(refused));
    assert_eq!(vol.session_state(), ConnectionState::Disconnected);
    assert_eq!(
        events.transitions(),
        vec![(RECORDED_ID.to_string(), VolumeConnection::Disconnected)]
    );
}

// ── Signing in, and what it writes down (#530) ───────────────────────

/// Nothing listens here, so every dial is refused at once, ❌ never a LAN
/// address a dial could actually reach.
const CLOSED_PORT: u16 = 1;

/// A disconnected volume on loopback's closed port, over `store`.
async fn offline_volume_over(store: &Arc<InMemoryCredentials>) -> SmbVolume {
    let host = VolumeHost::builder()
        .credentials(Arc::clone(store) as Arc<dyn CredentialStore>)
        .build();
    let vol = make_test_volume_with(RECORDED_ID, host);
    {
        let mut params = vol.inner.params.write().await;
        params.server = "127.0.0.1".to_string();
        params.port = CLOSED_PORT;
    }
    vol
}

fn secret_at(store: &InMemoryCredentials, scope: Option<&str>) -> Option<(String, String)> {
    store
        .credentials("127.0.0.1", scope)
        .map(|held| (held.username, held.secret))
}

fn held(username: &str, secret: &str) -> Option<(String, String)> {
    Some((username.to_string(), secret.to_string()))
}

/// ❗ **A typed password is written only once it WORKED.** A typo written before
/// the dial replaced the server-level password every share on the server falls
/// back to, and then the dial failed.
#[tokio::test]
async fn a_sign_in_that_does_not_connect_leaves_the_remembered_password_alone() {
    let store = Arc::new(InMemoryCredentials::new().with_entry("127.0.0.1", None, "ada", "right"));
    let vol = offline_volume_over(&store).await;

    let refusal = vol
        .inner
        .do_reconnect_with_credentials(SignIn {
            username: "ada".to_string(),
            secret: "a-typo".to_string(),
            remember: false,
        })
        .await;

    assert!(refusal.is_err(), "nothing listens on the closed port");
    assert_eq!(secret_at(&store, None), held("ada", "right"));
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

/// ❗ **Remember off means nothing is written.** The sheet forgets the entry
/// before it dials, and the sign-in used to write the typed password straight
/// back.
#[tokio::test]
async fn a_sign_in_with_nothing_remembered_writes_nothing() {
    let store = Arc::new(InMemoryCredentials::new());
    let vol = offline_volume_over(&store).await;

    let _ = vol
        .inner
        .do_reconnect_with_credentials(SignIn {
            username: "ada".to_string(),
            secret: "typed".to_string(),
            remember: false,
        })
        .await;

    assert_eq!(secret_at(&store, None), None);
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

// The rule a sign-in that DID connect writes by, one cell per entry shape.

fn connected_as(username: &str, password: &str) -> SmbConnectionParams {
    SmbConnectionParams::new("127.0.0.1", "TestShare", CLOSED_PORT, Some(username), Some(password))
}

fn host_over(store: &Arc<InMemoryCredentials>) -> VolumeHost {
    VolumeHost::builder()
        .credentials(Arc::clone(store) as Arc<dyn CredentialStore>)
        .build()
}

#[tokio::test]
async fn a_working_sign_in_refreshes_the_shares_own_entry_and_leaves_the_servers_alone() {
    let store = Arc::new(
        InMemoryCredentials::new()
            .with_entry("127.0.0.1", Some("TestShare"), "ada", "old")
            .with_entry("127.0.0.1", None, "ada", "server-wide"),
    );

    assert!(refresh_remembered_credentials(&host_over(&store), &connected_as("ada", "new"), false).await);

    assert_eq!(secret_at(&store, Some("TestShare")), held("ada", "new"));
    assert_eq!(secret_at(&store, None), held("ada", "server-wide"));
}

#[tokio::test]
async fn a_working_sign_in_refreshes_the_server_entry_it_reads_for_the_same_account() {
    let store = Arc::new(InMemoryCredentials::new().with_entry("127.0.0.1", None, "ada", "old"));

    assert!(refresh_remembered_credentials(&host_over(&store), &connected_as("ada", "new"), false).await);

    assert_eq!(secret_at(&store, None), held("ada", "new"));
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

/// ❗ Another account with Remember off writes nothing: a share entry made here
/// is one the person never asked for (#530).
#[tokio::test]
async fn a_working_sign_in_as_another_account_with_remember_off_writes_nothing() {
    let store = Arc::new(InMemoryCredentials::new().with_entry("127.0.0.1", None, "ada", "ada-pw"));

    assert!(refresh_remembered_credentials(&host_over(&store), &connected_as("grace", "grace-pw"), false).await);

    assert_eq!(secret_at(&store, None), held("ada", "ada-pw"));
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

/// ❌ Remember off never seeds: the sheet forgot the entry before it dialed.
#[tokio::test]
async fn a_working_sign_in_with_remember_off_never_seeds_an_empty_store() {
    let store = Arc::new(InMemoryCredentials::new());

    assert!(refresh_remembered_credentials(&host_over(&store), &connected_as("ada", "new"), false).await);

    assert_eq!(secret_at(&store, None), None);
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

/// ❗ A checked Remember box is a promise: an empty store gets the SERVER's
/// entry, the one every sign-in sheet writes and "Forget saved password" takes
/// back (#530).
#[tokio::test]
async fn a_working_sign_in_with_remember_on_seeds_the_server_entry() {
    let store = Arc::new(InMemoryCredentials::new());

    assert!(refresh_remembered_credentials(&host_over(&store), &connected_as("ada", "new"), true).await);

    assert_eq!(secret_at(&store, None), held("ada", "new"));
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

/// ❗ With Remember on, another account on this share still never replaces the
/// server-level entry every other share on the server signs in with: the share
/// gets its own.
#[tokio::test]
async fn a_working_sign_in_with_remember_on_as_another_account_still_files_the_share_its_own() {
    let store = Arc::new(InMemoryCredentials::new().with_entry("127.0.0.1", None, "ada", "ada-pw"));

    assert!(refresh_remembered_credentials(&host_over(&store), &connected_as("grace", "grace-pw"), true).await);

    assert_eq!(secret_at(&store, None), held("ada", "ada-pw"));
    assert_eq!(secret_at(&store, Some("TestShare")), held("grace", "grace-pw"));
}

/// The failing sender's `SendFailed` and every other waiter's `Disconnected` both
/// end the session through the funnel; a local I/O failure, whatever its errno,
/// never does.
#[tokio::test]
async fn a_send_failure_and_a_disconnect_end_the_session_and_a_local_io_failure_doesnt() {
    const EPIPE: i32 = 32;
    const EACCES: i32 = 13;
    const ENOSPC: i32 = 28;
    let send_failed = || smb2::Error::SendFailed {
        command: smb2::types::Command::Write,
        source: std::io::Error::from_raw_os_error(EPIPE),
    };
    for lost in [send_failed(), smb2::Error::Disconnected] {
        let vol = make_test_volume_direct();
        let mapped = vol.handle_smb_result::<()>("write", "a.txt", SessionGeneration::FIRST, Err(lost));
        assert!(matches!(mapped, Err(VolumeError::DeviceDisconnected(_))), "{mapped:?}");
        assert_eq!(vol.session_state(), ConnectionState::Disconnected);
    }

    let vol = make_test_volume_direct();
    for errno in [EPIPE, EACCES, ENOSPC] {
        let local = smb2::Error::Io(std::io::Error::from_raw_os_error(errno));
        let _ = vol.handle_smb_result::<()>("write", "a.txt", SessionGeneration::FIRST, Err(local));
        assert_eq!(
            vol.session_state(),
            ConnectionState::Direct,
            "a local Io (errno {errno}) must not end the session"
        );
    }
}

/// A loss from a call that ran on the session a reconnect has since replaced is
/// a late error from the OLD connection: it must not take down the new one, or
/// every sibling on it takes an outage it never had. Through both entries: the
/// per-call `handle_smb_result` and the producers' `note_smb_error`.
#[tokio::test]
async fn a_late_loss_from_a_replaced_session_leaves_the_new_one_direct() {
    let (vol, events) = volume_with_recorded_events();
    let old = SessionGeneration::FIRST;
    // A reconnect installs the next session.
    let new = vol.inner.state.advance();
    vol.inner.transition_to_direct(new);
    let announced_up = events.transitions();

    let mapped = vol.handle_smb_result::<()>("read", "a.txt", old, Err(smb2::Error::Disconnected));
    assert!(
        matches!(mapped, Err(VolumeError::DeviceDisconnected(_))),
        "the call itself still failed on the transport: {mapped:?}"
    );
    assert!(
        vol.inner.note_smb_error(old, &smb2::Error::Disconnected),
        "it's still a loss"
    );
    assert_eq!(vol.session_state(), ConnectionState::Direct);
    assert_eq!(
        events.transitions(),
        announced_up,
        "nothing more to announce: the share never went down"
    );

    // The new session's own loss still counts.
    vol.inner.note_smb_error(new, &smb2::Error::Disconnected);
    assert_eq!(vol.session_state(), ConnectionState::Disconnected);
}

/// `Volume::connection_generation` names the installed session: a reconnect that
/// installs one moves it, and a reconnect that finds the session already live
/// leaves it alone (the transfer layer reads a moved generation as "my call's
/// error was the old session's", so a no-op that moved it would turn a real
/// error into a needless re-run). The install itself needs a live server; its
/// Docker twin is `session_integration_test.rs`'s
/// `smb_integration_attempt_reconnect_rebuilds_session`.
#[tokio::test]
async fn the_reported_generation_moves_only_when_a_session_is_installed() {
    let vol = make_test_volume_direct();
    let first = Volume::connection_generation(&vol).expect("an SMB share reports its generation");

    vol.inner.do_attempt_reconnect().await.expect("already Direct");
    assert_eq!(
        Volume::connection_generation(&vol),
        Some(first),
        "a reconnect that found the session live installed nothing"
    );

    // What `do_attempt_reconnect` does when it installs a rebuilt session.
    let installed = vol.inner.state.advance();
    vol.inner.transition_to_direct(installed);
    assert_eq!(Volume::connection_generation(&vol), Some(first + 1));
}

// ── Filing a sign-in that already connected (#603) ───────────────────

/// ❗ **"Try again" after a refused save files the credentials WITHOUT a dial.**
/// The sign-in already proved them, and with the session live a second sign-in
/// dials nothing and so saved nothing. The port is closed, so a store holding
/// them is evidence the save didn't wait on a server.
#[tokio::test]
async fn remembering_a_sign_in_files_it_by_the_same_rule_without_dialing() {
    let store = Arc::new(InMemoryCredentials::new());
    let vol = offline_volume_over(&store).await;

    let filed = vol
        .remember_sign_in(SignIn {
            username: "ada".to_string(),
            secret: "typed".to_string(),
            remember: true,
        })
        .await
        .expect("filing needs no server");

    assert!(!filed.remember_refused);
    assert_eq!(secret_at(&store, None), held("ada", "typed"));
    assert_eq!(secret_at(&store, Some("TestShare")), None);
}

/// ❗ The server-vs-share rule holds here too: another account never replaces
/// the server entry every other share signs in with.
#[tokio::test]
async fn remembering_another_accounts_sign_in_files_the_share_its_own() {
    let store = Arc::new(InMemoryCredentials::new().with_entry("127.0.0.1", None, "ada", "ada-pw"));
    let vol = offline_volume_over(&store).await;

    vol.remember_sign_in(SignIn {
        username: "grace".to_string(),
        secret: "grace-pw".to_string(),
        remember: true,
    })
    .await
    .expect("filing needs no server");

    assert_eq!(secret_at(&store, None), held("ada", "ada-pw"));
    assert_eq!(secret_at(&store, Some("TestShare")), held("grace", "grace-pw"));
}
