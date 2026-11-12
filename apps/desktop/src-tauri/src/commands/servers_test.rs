//! The protocol-agnostic server commands: what they map, what they move, and the
//! calls they refuse. What the listing lists: `saved_servers/listing_test.rs`.

use super::*;
use crate::network::sftp_volume_wiring::SftpConnection;
use crate::network::webdav_volume_wiring::WebdavConnection;
use crate::saved_servers::outcome::SftpHostKeyIdentity;
use crate::saved_servers::test_fixtures::{find, listing_of as saved_servers_of, sftp_entry, share_row, webdav_entry};

/// A self-signed NAS's first-contact certificate prompt.
fn certificate_prompt() -> cmdr_fs::volume::tls::CertificatePrompt {
    cmdr_fs::volume::tls::CertificatePrompt {
        host: "nas.local".to_string(),
        port: 5006,
        fingerprint: "AB:CD".to_string(),
        kind: cmdr_fs::volume::tls::CertificatePromptKind::Unknown,
    }
}

// ── The outcome mapping ──────────────────────────────────────────────

/// Every SFTP outcome lands on its superset twin.
#[test]
fn every_sftp_outcome_maps_to_its_superset_twin() {
    use cmdr_sftp::transport::{HostKeyPrompt, HostKeyPromptKind};

    let prompt = HostKeyPrompt {
        host: "nas.local".to_string(),
        port: 22,
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:abc".to_string(),
        kind: HostKeyPromptKind::Unknown,
    };
    let cases = [
        (
            SftpConnection::Connected {
                volume_id: "sftp-x".to_string(),
            },
            ServerConnectOutcome::Connected {
                volume_id: "sftp-x".to_string(),
            },
        ),
        (
            SftpConnection::NeedsHostKeyApproval(prompt.clone()),
            ServerConnectOutcome::NeedsHostKeyApproval(prompt),
        ),
        (
            SftpConnection::HostKeyRevoked {
                algorithm: "ssh-rsa".to_string(),
                fingerprint: "SHA256:def".to_string(),
            },
            ServerConnectOutcome::HostKeyRevoked(SftpHostKeyIdentity {
                algorithm: "ssh-rsa".to_string(),
                fingerprint: "SHA256:def".to_string(),
            }),
        ),
        (
            SftpConnection::AuthenticationRejected,
            ServerConnectOutcome::AuthenticationRejected,
        ),
        (SftpConnection::NeedsCredentials, ServerConnectOutcome::NeedsCredentials),
        (SftpConnection::TimedOut, ServerConnectOutcome::TimedOut),
        (SftpConnection::Unreachable, ServerConnectOutcome::Unreachable),
        (SftpConnection::Cancelled, ServerConnectOutcome::Cancelled),
    ];
    for (from, expected) in cases {
        assert_eq!(format!("{:?}", outcome_from_sftp(from)), format!("{expected:?}"));
    }
}

/// Every WebDAV outcome lands on its superset twin.
///
/// ❗ `AuthMethodUnsupported` stays itself rather than collapsing into
/// `AuthenticationRejected`: a Digest-only server never saw the password, so
/// "check your password" is the wrong fix to offer.
#[test]
fn every_webdav_outcome_maps_to_its_superset_twin_including_the_unsupported_scheme() {
    let cases = [
        (
            WebdavConnection::Connected {
                volume_id: "webdav-x".to_string(),
            },
            ServerConnectOutcome::Connected {
                volume_id: "webdav-x".to_string(),
            },
        ),
        (
            WebdavConnection::AuthenticationRejected,
            ServerConnectOutcome::AuthenticationRejected,
        ),
        (
            WebdavConnection::NeedsCredentials,
            ServerConnectOutcome::NeedsCredentials,
        ),
        (
            WebdavConnection::AuthMethodUnsupported,
            ServerConnectOutcome::AuthMethodUnsupported,
        ),
        (
            WebdavConnection::NeedsCertificateApproval(certificate_prompt()),
            ServerConnectOutcome::NeedsCertificateApproval(certificate_prompt()),
        ),
        (
            WebdavConnection::CertificateRejected,
            ServerConnectOutcome::CertificateRejected,
        ),
        (
            WebdavConnection::NotAWebdavServer,
            ServerConnectOutcome::NotAWebdavServer,
        ),
        (WebdavConnection::TimedOut, ServerConnectOutcome::TimedOut),
        (WebdavConnection::Unreachable, ServerConnectOutcome::Unreachable),
        (WebdavConnection::Cancelled, ServerConnectOutcome::Cancelled),
    ];
    for (from, expected) in cases {
        assert_eq!(format!("{:?}", outcome_from_webdav(from)), format!("{expected:?}"));
    }
}

// ── Pins ─────────────────────────────────────────────────────────────

/// A pin round-trips through the family and tells the switcher to redraw.
#[test]
fn pinning_a_place_round_trips_and_republishes_the_volume_list() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.36";
    sftp_known_servers::remember(sftp_entry(host, true));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");
    let pinned_now = || {
        sftp_known_servers::all()
            .into_iter()
            .find(|e| e.host == host)
            .expect("the entry stays saved")
            .pinned
    };

    let before = crate::volume_broadcast::volumes_changed_requests();
    assert!(set_place_pinned_inner(&volume_id, false));
    assert!(!pinned_now(), "the unpin reached the store");
    assert!(
        crate::volume_broadcast::volumes_changed_requests() > before,
        "❗ the switcher only learns a row left it from a `volumes-changed`"
    );

    assert!(set_place_pinned_inner(&volume_id, true));
    assert!(pinned_now(), "and back");
}

/// An id nothing saved answers no, rather than pretending.
#[test]
fn pinning_an_unknown_place_is_a_plain_no() {
    assert!(!set_place_pinned_inner("sftp-nothing-was-ever-saved-here", true));
}

// ── Reconnect automatically ──────────────────────────────────────────

/// ❗ **The row menu's "Reconnect automatically" moves that one field and
/// nothing else**, and the listing (which both row menus read) carries it back.
/// The pin is the field a read-modify-`remember` would get wrong, so it's the
/// one checked for collateral.
#[test]
fn switching_reconnect_automatically_on_an_sftp_place_round_trips_through_the_listing() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.71";
    sftp_known_servers::remember(sftp_entry(host, false));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");
    let listed = || find(&saved_servers_of(Vec::new()), &format!("{host} over ssh")).clone();
    assert_eq!(listed().auto_reconnect, Some(true));

    let before = crate::volume_broadcast::volumes_changed_requests();
    assert!(set_place_auto_reconnect_inner(&volume_id, false));
    assert_eq!(listed().auto_reconnect, Some(false), "the switch reached the store");
    assert!(!listed().pinned, "and left the pin where it was");
    assert!(
        crate::volume_broadcast::volumes_changed_requests() > before,
        "❗ the hub re-reads the saved list on `volumes-changed`, so an open hub repaints"
    );

    assert!(set_place_auto_reconnect_inner(&volume_id, true));
    assert_eq!(listed().auto_reconnect, Some(true), "and back");
}

/// The WebDAV store has its own writer; the same round trip through it.
#[test]
fn switching_reconnect_automatically_on_a_webdav_place_round_trips_through_the_listing() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.72";
    webdav_known_servers::remember(webdav_entry(host, true));
    let volume_id = cmdr_fs::volume::webdav_volume_id(host, 8080, "ada");
    let listed = || find(&saved_servers_of(Vec::new()), &format!("{host} over dav")).clone();

    assert!(set_place_auto_reconnect_inner(&volume_id, false));
    assert_eq!(listed().auto_reconnect, Some(false));
    assert!(listed().pinned, "the pin stays put");
}

/// An id nothing saved answers no: a one-shot connection has nothing to persist.
#[test]
fn switching_reconnect_automatically_on_an_unknown_place_is_a_plain_no() {
    assert!(!set_place_auto_reconnect_inner(
        "sftp-nothing-was-ever-saved-here",
        false
    ));
}

// ── Connecting a saved place ─────────────────────────────────────────

/// An id nothing saved is a typed refusal, ❌ never a dial into the dark.
#[tokio::test]
async fn connecting_a_place_nothing_saved_is_a_typed_refusal() {
    let refusal = connect_saved_place(
        "sftp-nothing-was-ever-saved-here".to_string(),
        "attempt".to_string(),
        None,
        None,
    )
    .await
    .expect_err("an unsaved id has nothing to dial");
    assert!(matches!(refusal, SavedPlaceRefusal::NoSuchServer { .. }));
}

/// ❗ **A registered volume is never re-dialed.** Re-dialing would register a
/// second volume under the same id; a session that dropped is mended by
/// `reconnect_volume_with_credentials`, which is what enforces the read-only
/// username rule and the never-seeds rule.
#[tokio::test]
async fn connecting_a_place_that_is_already_registered_is_refused() {
    let host = "192.0.2.37";
    sftp_known_servers::remember(sftp_entry(host, true));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");

    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(
        &volume_id,
        std::sync::Arc::new(cmdr_fs::volume::InMemoryVolume::new("stand-in")),
    );

    let refusal = connect_saved_place(volume_id.clone(), "attempt".to_string(), None, None)
        .await
        .expect_err("a registered volume is mended, never re-dialed");
    assert!(matches!(refusal, SavedPlaceRefusal::AlreadyConnected { .. }));

    manager.unregister(&volume_id);
}

// ── Forgetting a server ──────────────────────────────────────────────

/// ❗ **A forgotten server is GONE, and the pane hears so first.**
///
/// `VolumeUnmounted` is what redirects a pane standing on the place;
/// `volumes-changed` is what takes the row out of the store. The other order
/// leaves the pane on a volume nothing can name, which is the bug this cell
/// exists to keep out. Asserting on the recorded GENERATION rather than on
/// wall-clock order is what makes it un-flaky.
#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-global broadcast recorders for the whole cell; holding it across the await IS the point"
)]
async fn forgetting_a_server_tells_the_panes_before_it_takes_the_row_away() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.74";
    sftp_known_servers::remember(sftp_entry(host, true));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");

    let before = crate::volume_broadcast::volumes_changed_requests();
    assert!(forget_server(volume_id.clone()).await, "the entry was there");

    assert!(
        !sftp_known_servers::all().iter().any(|e| e.host == host),
        "the entry left the store"
    );
    let (gone_id, gone_at) = crate::volume_broadcast::last_volume_gone().expect("the panes were told");
    assert_eq!(gone_id, volume_id);
    assert_eq!(
        gone_at, before,
        "❗ the gone event went out BEFORE anything asked for a republish"
    );
    assert!(
        crate::volume_broadcast::volumes_changed_requests() > before,
        "and the republish followed, so the row leaves the switcher"
    );
}

/// An id nothing saved answers no and tells nobody: a spurious `VolumeUnmounted`
/// would send a pane home for no reason.
#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-global broadcast recorders for the whole cell; holding it across the await IS the point"
)]
async fn forgetting_a_server_nothing_saved_is_a_plain_no() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let before = crate::volume_broadcast::last_volume_gone();
    assert!(!forget_server("sftp-nothing-was-ever-saved-here".to_string()).await);
    assert_eq!(
        crate::volume_broadcast::last_volume_gone(),
        before,
        "nothing was forgotten, so nothing was announced"
    );
}

/// ❗ **Disconnecting a place tells the panes too**, or a pane keeps a volume id
/// the registry no longer answers for and every listing on it fails instead of
/// redirecting. Unlike a forget, the ROW survives (it becomes `saved`).
#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-global broadcast recorders for the whole cell; holding it across the await IS the point"
)]
async fn disconnecting_a_connected_place_tells_the_panes_and_keeps_it_saved() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.73";
    sftp_known_servers::remember(sftp_entry(host, true));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");
    // A real `SftpVolume` with no session behind it: the wiring downcasts to the
    // concrete type, so an `InMemoryVolume` stand-in would have nothing to drop.
    let volume = cmdr_sftp::volume::testing::offline_volume(
        "stand-in",
        SftpConnectionParams::new(host, 2222, "ada", "/srv/data"),
        cmdr_sftp::auth::AuthRungUsed::Agent,
        cmdr_fs::volume::host::VolumeHost::detached(),
    );
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(&volume_id, std::sync::Arc::new(volume));

    assert!(disconnect_place(volume_id.clone()).await, "there was a session to drop");

    let (gone_id, _) = crate::volume_broadcast::last_volume_gone().expect("❗ the panes were told");
    assert_eq!(gone_id, volume_id);
    assert!(manager.get(&volume_id).is_none(), "the volume left the registry");
    let place = crate::server_volumes::server_places()
        .into_iter()
        .find(|place| place.id == volume_id)
        .expect("❗ a disconnect never forgets the place");
    assert_eq!(place.state, cmdr_fs::volume::ConnectionState::Saved);
}

/// A place with no session has nothing to drop, and a spurious
/// `VolumeUnmounted` would send a pane home for no reason.
#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-global broadcast recorders for the whole cell; holding it across the await IS the point"
)]
async fn disconnecting_a_place_that_has_no_session_announces_nothing() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.75";
    sftp_known_servers::remember(sftp_entry(host, true));
    let volume_id = cmdr_fs::volume::sftp_volume_id(host, 2222, "ada");

    let before = crate::volume_broadcast::last_volume_gone();
    assert!(
        !disconnect_place(volume_id).await,
        "a saved-but-not-connected place has no session to drop"
    );
    assert_eq!(
        crate::volume_broadcast::last_volume_gone(),
        before,
        "and nothing was announced, so no pane goes home for nothing"
    );
}

// ── Saving without dialing, and the start folder ─────────────────────

fn sftp_target(host: &str, port: u16, username: &str, start_folder: Option<&str>) -> ServerTarget {
    ServerTarget::Sftp {
        display_name: "Edited".to_string(),
        host: host.to_string(),
        port,
        username: username.to_string(),
        remote_root: "/srv/data".to_string(),
        start_folder: start_folder.map(str::to_string),
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
    }
}

fn webdav_target(url: &str, username: &str, start_folder: Option<&str>) -> ServerTarget {
    ServerTarget::Webdav {
        display_name: "Edited".to_string(),
        url: url.to_string(),
        username: username.to_string(),
        remote_root: "/Photos".to_string(),
        start_folder: start_folder.map(str::to_string),
        auto_reconnect: true,
    }
}

/// ❗ **A start folder outside the root is refused, and NOTHING is written**:
/// the stored entry keeps every field it had, the name included.
#[tokio::test]
async fn saving_a_start_folder_outside_the_root_is_refused_and_writes_nothing() {
    let host = "192.0.2.61";
    sftp_known_servers::remember(sftp_entry(host, true));

    let outcome = update_saved_server(sftp_target(host, 2222, "ada", Some("/srv/data-1")), None).await;

    assert_eq!(outcome, SavedServerOutcome::StartFolderOutsideRoot);
    let stored = sftp_known_servers::find(host, 2222, "ada").expect("the entry stays saved");
    assert_eq!(
        stored.display_name,
        format!("{host} over ssh"),
        "not even the name moved"
    );
    assert_eq!(stored.start_folder, None);
}

/// The start folder is stored normalized, and the root itself stores as none,
/// so one landing has one spelling.
#[tokio::test]
async fn saving_a_start_folder_under_the_root_stores_it_normalized() {
    let host = "192.0.2.62";
    sftp_known_servers::remember(sftp_entry(host, true));
    let stored = || sftp_known_servers::find(host, 2222, "ada").expect("the entry stays saved");

    let deeper = update_saved_server(sftp_target(host, 2222, "ada", Some("/srv/data/photos/")), None).await;
    assert_eq!(deeper, SavedServerOutcome::Saved);
    assert_eq!(stored().start_folder.as_deref(), Some("/srv/data/photos"));
    assert_eq!(stored().display_name, "Edited");

    let at_root = update_saved_server(sftp_target(host, 2222, "ada", Some("/srv/data")), None).await;
    assert_eq!(at_root, SavedServerOutcome::Saved);
    assert_eq!(stored().start_folder, None);
}

/// ❗ **A saved edit republishes the volume list, whatever it changed.** The
/// switcher's name and landing come from that list, and neither an unconnected
/// place's edit nor a start-folder-only one moves anything in the registry that
/// would announce it. The hub re-reads the saved list on the same broadcast. A
/// refusal wrote nothing, so it asks for nothing.
#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-global broadcast recorders for the whole cell; holding it across the await IS the point"
)]
async fn saving_an_edit_republishes_the_volume_list_and_a_refusal_does_not() {
    let _recorder = crate::volume_broadcast::recorder_test_lock();
    let host = "192.0.2.66";
    sftp_known_servers::remember(sftp_entry(host, true));

    let before = crate::volume_broadcast::volumes_changed_requests();
    let refused = update_saved_server(sftp_target(host, 2222, "ada", Some("/srv/data-1")), None).await;
    assert_eq!(refused, SavedServerOutcome::StartFolderOutsideRoot);
    assert_eq!(
        crate::volume_broadcast::volumes_changed_requests(),
        before,
        "a refusal changed nothing, so nothing is republished"
    );

    let saved = update_saved_server(sftp_target(host, 2222, "ada", Some("/srv/data/photos")), None).await;
    assert_eq!(saved, SavedServerOutcome::Saved);
    assert!(
        crate::volume_broadcast::volumes_changed_requests() > before,
        "❗ without it the switcher keeps the old name and landing until something else republishes"
    );
}

/// WebDAV takes the same rule through the same family, so the frontend branches
/// on protocol nowhere new.
#[tokio::test]
async fn saving_a_webdav_start_folder_outside_the_root_is_refused_and_writes_nothing() {
    let host = "192.0.2.63";
    webdav_known_servers::remember(webdav_entry(host, true));
    let url = format!("http://{host}:8080/dav/");

    let outcome = update_saved_server(webdav_target(&url, "ada", Some("/Documents")), None).await;

    assert_eq!(outcome, SavedServerOutcome::StartFolderOutsideRoot);
    let stored = webdav_known_servers::find(&url, "ada").expect("the entry stays saved");
    assert_eq!(
        stored.display_name,
        format!("{host} over dav"),
        "not even the name moved"
    );
}

/// ❗ **An add with a start folder outside the root is refused BEFORE dialing**,
/// so nothing is registered and nothing is saved. Port 1 on loopback refuses
/// connections, so a dial that slipped through would answer `Unreachable`.
#[tokio::test]
async fn connecting_with_a_start_folder_outside_the_root_is_refused_before_dialing() {
    let _secrets = crate::test_support::isolate_secrets();
    let username = "start-folder-refusal";

    let sftp = connect_server(
        sftp_target("127.0.0.1", 1, username, Some("/srv")),
        "start-folder-refusal-sftp".to_string(),
        None,
    )
    .await;
    assert!(
        matches!(sftp, ServerConnectOutcome::StartFolderOutsideRoot),
        "got {sftp:?}"
    );
    assert!(
        sftp_known_servers::find("127.0.0.1", 1, username).is_none(),
        "nothing was saved"
    );

    let url = "http://127.0.0.1:1/dav/";
    let webdav = connect_server(
        webdav_target(url, username, Some("/Documents")),
        "start-folder-refusal-webdav".to_string(),
        None,
    )
    .await;
    assert!(
        matches!(webdav, ServerConnectOutcome::StartFolderOutsideRoot),
        "got {webdav:?}"
    );
    assert!(webdav_known_servers::find(url, username).is_none(), "nothing was saved");
}

/// ❗ A URL that isn't `http`(s) is answered before anything is dialed: the
/// WebDAV arm parses the address first, so a garbled scheme never reaches the
/// wiring and nothing is registered or saved.
#[tokio::test]
async fn connecting_a_webdav_target_with_a_non_http_url_answers_invalid_url_without_dialing() {
    let username = "invalid-url";
    let url = "ftp://127.0.0.1/dav/";

    let outcome = connect_server(
        webdav_target(url, username, None),
        "invalid-url-attempt".to_string(),
        None,
    )
    .await;

    assert!(matches!(outcome, ServerConnectOutcome::InvalidUrl), "got {outcome:?}");
    assert!(webdav_known_servers::find(url, username).is_none(), "nothing was saved");
}

/// An id nothing saved has no secret to forget, so the menu item stays off
/// rather than offering to revoke a credential that was never stored.
#[tokio::test]
async fn an_unsaved_id_has_no_remembered_secret() {
    assert!(!has_server_secret_among("sftp-nothing-was-ever-saved-here", Vec::new()).await);
}

/// ❗ **A share's own Keychain entry is a remembered secret too** (#530): the
/// sign-in that connected reads it first, so a check that missed it showed
/// Remember off while it signed in, and a forget that missed it left it to come
/// back on every reconnect.
#[tokio::test]
async fn an_smb_shares_own_entry_is_remembered_and_forgotten_with_the_servers() {
    let _secrets = crate::test_support::isolate_secrets();
    let server = "192.0.2.75";
    let row = share_row(server, "Ledger", Some("grace"), Some("smb-ledger-530"));
    known_shares::remember_share(row.clone());
    crate::network::keychain::save_credentials(server, Some("Ledger"), "grace", "grace-pw").expect("saved");

    assert!(has_server_secret_among("smb-ledger-530", Vec::new()).await);
    assert!(forget_server_secret_among("smb-ledger-530", Vec::new()).await);

    assert!(matches!(
        crate::network::keychain::get_credentials(server, Some("Ledger")),
        Err(crate::network::keychain::KeychainError::NotFound(_))
    ));
    assert!(!has_server_secret_among("smb-ledger-530", Vec::new()).await);
    known_shares::forget_rows(&[row]);
}

/// A server added from the sheet the way #915 found it: typed `host.local.:port`,
/// signed in at its share list (filed under the discovery name), mounted by IP, and
/// "Connect directly" filing the share's own entry under the resolved hostname and
/// under the dialed IP, plus an entry under the key a trailing dot used to make.
/// Answers the manual entry, the share row, and every key a sign-in wrote.
fn signed_in_manual_server(
    address: &str,
    ip: &str,
    volume_id: &str,
) -> (
    manual_servers::ManualServerEntry,
    known_shares::KnownNetworkShare,
    Vec<(String, Option<String>)>,
) {
    use crate::network::keychain::save_credentials;
    use crate::network::server_identity::smb_server;

    let port = 11481;
    let entry = manual_servers::ManualServerEntry {
        id: manual_servers::generate_server_id(address, port),
        display_name: "Home NAS".to_string(),
        address: address.to_string(),
        port,
        added_at: "2026-11-03T00:00:00Z".to_string(),
        username: Some("ada".to_string()),
        awaiting_first_connect: false,
    };
    let discovery_name = manual_servers::discovery_name(address, port);
    let mut row = share_row(&discovery_name, "private", Some("ada"), Some(volume_id));
    row.address = Some(ip.to_string());
    row.port = Some(port);
    known_shares::remember_share(row.clone());

    // The share list's sign-in (`PlacesBrowser` → `save_smb_credentials`), under the
    // host's discovery name.
    save_credentials(&discovery_name, None, "ada", "pw").expect("saved");
    let resolved = smb_server(address.trim_end_matches('.'), port);
    save_credentials(&resolved, Some("private"), "ada", "pw").expect("saved");
    let dialed = smb_server(ip, port);
    save_credentials(&dialed, Some("private"), "ada", "pw").expect("saved");
    // What a save under the sheet's spelling made before the key dropped the dot.
    let legacy = format!("smb://{}", discovery_name.to_lowercase());
    crate::secrets::store().set(&legacy, b"ada\0old").expect("planted");

    let keys = vec![
        (discovery_name, None),
        (resolved, Some("private".to_string())),
        (dialed, Some("private".to_string())),
    ];
    (entry, row, keys)
}

/// Asserts nothing answers under `keys` any more, in the store or the cache.
fn assert_all_forgotten(keys: &[(String, Option<String>)], legacy_host: &str) {
    use crate::network::keychain::{KeychainError, get_credentials, has_cached_credentials};

    for (server, share) in keys {
        assert!(
            matches!(
                get_credentials(server, share.as_deref()),
                Err(KeychainError::NotFound(_))
            ),
            "❗ {server} / {share:?} outlived the forget"
        );
        assert!(!has_cached_credentials(server), "the cache lets go of {server} too");
    }
    assert!(
        crate::secrets::store()
            .get(&format!("smb://{}", legacy_host.to_lowercase()))
            .is_err(),
        "the pre-fix spelling's entry goes too"
    );
}

/// ❗ **Forgetting a saved share's password takes every entry its sign-in filed**
/// (#915), ❌ not only the one under the share row's server name: the forget left
/// the host-level password behind, and the next add signed in silently.
#[tokio::test]
async fn forgetting_a_shares_password_takes_what_every_sign_in_filed() {
    let _secrets = crate::test_support::isolate_secrets();
    let (entry, row, keys) = signed_in_manual_server("home-nas-915a.local.", "192.0.2.111", "smb-915-a-private");
    let manual = vec![entry];

    assert!(has_server_secret_among("smb-915-a-private", manual.clone()).await);
    assert!(forget_server_secret_among("smb-915-a-private", manual.clone()).await);

    assert_all_forgotten(&keys, "home-nas-915a.local.:11481");
    assert!(!has_server_secret_among("smb-915-a-private", manual).await);
    known_shares::forget_rows(&[row]);
}

/// The same through the HOST's id, the one `list_saved_servers` hands out for an SMB
/// server: the protocol-agnostic secret commands answer for it too, rather than a
/// silent `false` that a caller reads as "nothing was saved".
#[tokio::test]
async fn forgetting_a_hosts_password_by_its_listing_id_takes_every_entry() {
    let _secrets = crate::test_support::isolate_secrets();
    let (entry, row, keys) = signed_in_manual_server("home-nas-915b.local.", "192.0.2.112", "smb-915-b-private");
    let host_id = entry.id.clone();
    let manual = vec![entry];

    assert!(has_server_secret_among(&host_id, manual.clone()).await);
    assert!(forget_server_secret_among(&host_id, manual.clone()).await);

    assert_all_forgotten(&keys, "home-nas-915b.local.:11481");
    assert!(!has_server_secret_among(&host_id, manual).await);
    known_shares::forget_rows(&[row]);
}
