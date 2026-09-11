//! The lifecycle a connect and a disconnect owe the rest of the app: a volume in
//! the registry, an entry in the server list, and a client that is GONE
//! afterwards.
//!
//! App-side because that is what these assert on. The dial itself and the byte
//! path are the crate's own cells (`crates/cmdr-webdav/DETAILS.md`).
//!
//! ❗ Every `webdav_integration_` cell here needs the Docker stack:
//! `apps/desktop/test/webdav-servers/start.sh`. Everything else runs without
//! one.

use std::path::Path;
use std::time::Duration;

use cmdr_fs::volume::ConnectionState;
use cmdr_webdav::{WebdavConnectionParams, WebdavVolume};

use cmdr_webdav::volume::testing::{FIXTURE_USER, fixture_target};

use crate::network::one_shot_credentials::SecretOffer;
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::network::webdav_known_servers::KnownWebdavServer;
use crate::network::webdav_volume_wiring::{self, WebdavConnection};
use crate::network::{keychain, webdav_known_servers};

/// Disconnecting something that isn't a WebDAV volume answers no rather than
/// tearing down whatever is under that id.
#[tokio::test]
async fn disconnecting_a_volume_that_is_not_webdav_does_nothing() {
    assert!(
        !webdav_volume_wiring::disconnect("webdav-nothing-is-registered-here").await,
        "an unknown id is a no, not a panic"
    );
}

/// Asking about an unmounted volume answers `None`: there is no live session to
/// have a precondition.
#[tokio::test]
async fn the_unattended_answer_for_an_unmounted_volume_is_none() {
    assert!(
        webdav_volume_wiring::unattended_reconnect("webdav-nothing-is-registered-here")
            .await
            .is_none()
    );
}

// ── Calling a connect off ────────────────────────────────────────────

/// Cancelling an attempt nobody is running is a plain no.
///
/// ❗ Not an error: a click that lands just after a connect finished is ordinary,
/// and there is nothing wrong to report about it.
#[tokio::test]
async fn cancelling_an_attempt_nobody_is_running_is_a_plain_no() {
    assert!(!webdav_volume_wiring::cancel_connect(
        "webdav-no-such-attempt-was-ever-started"
    ));
}

/// A connect that ends on its own ❗ leaves nothing behind and takes its own
/// entry out of the attempt table.
///
/// ❗ **Not SFTP's "cancel a hanging dial" cell, and it can't be.** SFTP reaches
/// the wire before it reads a secret, so a routed-nowhere address hangs there
/// and a cancel has something to interrupt. This backend reads the store FIRST
/// and answers `NeedsCredentials` without dialing, so nothing here can be caught
/// mid-flight from outside. The mid-flight cancel is the crate's cell
/// (`crates/cmdr-webdav/src/volume/cancel_test.rs`), where the credential seam
/// takes an in-memory store; what the app owns, and what this asserts, is the
/// attempt table and the guard that empties it.
///
/// `192.0.2.1` is reserved for documentation (RFC 5737) and routed nowhere, so
/// even a store that surprised us here could not reach a server.
#[tokio::test]
async fn a_connect_that_ends_takes_its_attempt_entry_with_it_and_registers_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    const ATTEMPT: &str = "webdav-a-connect-that-ends";
    let base_url = url::Url::parse("https://192.0.2.1/dav/").expect("a literal");
    let params = WebdavConnectionParams::new(base_url.clone(), "nobody", "/");
    let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &params.username);

    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        webdav_volume_wiring::connect_and_register("Nowhere", None, params.clone(), ATTEMPT, None),
    )
    .await
    .expect("a dial with nothing in the store answers without touching the network");
    assert!(
        matches!(outcome, WebdavConnection::NeedsCredentials),
        "with no secret stored this backend asks for one rather than reporting the address as unreachable"
    );

    assert!(
        crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .is_none(),
        "❗ a connect that didn't come up registers nothing"
    );
    assert!(
        !webdav_known_servers::all()
            .into_iter()
            .any(|entry| entry.url == base_url.as_str()),
        "❗ and it remembers no server either"
    );
    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "❗ and it writes no secret: only `save_webdav_credentials` ever does"
    );

    // ❗ The `AttemptGuard` is what does this, on every one of the eight ways a
    // connect can leave. A token nobody collects is an id that can never be
    // reused, so this is the cell that notices the guard going missing.
    assert!(
        !webdav_volume_wiring::cancel_connect(ATTEMPT),
        "the attempt's entry goes out with the connect, however the connect ended"
    );
}

/// ❗ **A reconnect never puts an unpinned server back in the switcher.**
///
/// The connect path asks for `pinned: true`, which is how a NEW place lands in
/// the switcher on its first successful connect. A server the user has since
/// unpinned has to survive the next connect unpinned, or the unpin undoes itself
/// the moment the session comes back. `webdav_known_servers::remember` is where
/// the stored value wins.
#[tokio::test]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_reconnecting_leaves_an_unpinned_server_unpinned() {
    let _secrets = crate::test_support::isolate_secrets();
    let target = fixture_target("APACHE", 13480, FIXTURE_USER);
    let params = target.params();
    keychain::save_credentials(
        &params.credential_service(),
        Some(&params.username),
        &params.username,
        &target.password,
    )
    .expect("the test secret store always accepts");

    let saved = || {
        webdav_known_servers::all()
            .into_iter()
            .find(|entry| entry.username == params.username)
            .expect("a successful connect remembers the server")
    };

    let first =
        webdav_volume_wiring::connect_and_register("Fixture server", None, params.clone(), "webdav-pin-1", None).await;
    let WebdavConnection::Connected { volume_id } = first else {
        panic!("a fixture with its password stored must connect");
    };
    assert!(saved().pinned, "a first connect pins the new place");

    // The user unpins it, then the session drops and comes back.
    let mut unpinned = saved();
    unpinned.pinned = false;
    webdav_known_servers::forget(&unpinned.url, &unpinned.username);
    webdav_known_servers::remember(unpinned);
    webdav_volume_wiring::disconnect(&volume_id).await;

    let again =
        webdav_volume_wiring::connect_and_register("Fixture server", None, params.clone(), "webdav-pin-2", None).await;
    let WebdavConnection::Connected { volume_id } = again else {
        panic!("the same fixture connects again");
    };
    assert!(!saved().pinned, "the unpin survives the reconnect");

    webdav_volume_wiring::disconnect(&volume_id).await;
}

/// ❗ **A one-shot secret connects and stays out of the store.**
///
/// "Connect once without remembering" used to mean save → dial → delete, and a
/// Keychain entry that exists for a second is not the user's choice. The offer
/// with `remember: false` runs the dial against `one_shot_credentials`, so the
/// server comes up with nothing written behind it.
#[tokio::test]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_one_shot_secret_connects_and_leaves_the_store_empty() {
    let _secrets = crate::test_support::isolate_secrets();
    let target = fixture_target("APACHE", 13480, FIXTURE_USER);
    let params = target.params();
    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "the cell starts with nothing stored, which is what makes the dial's source unambiguous"
    );

    let outcome = webdav_volume_wiring::connect_and_register(
        "Fixture server",
        None,
        params.clone(),
        "webdav-one-shot",
        Some(SecretOffer {
            secret: target.password.clone(),
            remember: false,
        }),
    )
    .await;
    let WebdavConnection::Connected { volume_id } = outcome else {
        panic!("the offered secret is what proves this dial; nothing else could");
    };

    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "❗ a one-shot secret is never written: that is the whole meaning of the switch"
    );

    webdav_volume_wiring::disconnect(&volume_id).await;
}

/// The other half of the switch: `remember: true` writes the secret first, so the
/// next dial (and every unattended reconnect) reads it back the ordinary way.
#[tokio::test]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_a_remembered_secret_is_in_the_store_after_the_dial() {
    let _secrets = crate::test_support::isolate_secrets();
    let target = fixture_target("APACHE", 13480, FIXTURE_USER);
    let params = target.params();

    let outcome = webdav_volume_wiring::connect_and_register(
        "Fixture server",
        None,
        params.clone(),
        "webdav-remembered",
        Some(SecretOffer {
            secret: target.password.clone(),
            remember: true,
        }),
    )
    .await;
    let WebdavConnection::Connected { volume_id } = outcome else {
        panic!("a remembered secret is saved before the dial, so the dial reads it back");
    };

    assert!(
        keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "the switch means exactly one thing: the secret is in the store"
    );

    webdav_volume_wiring::disconnect(&volume_id).await;
}

// ── Editing a connected place ────────────────────────────────────────

/// ❗ **Saving an edit to a connected place applies it live, over the SAME
/// client**: connected at `/photos`, then widened to the account's whole tree.
/// Nothing re-probes, the shared retirement flag stays clear, and the reconnect
/// loop still brings the client back after a loss, re-probing the NEW root.
#[tokio::test]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn webdav_integration_widening_a_connected_root_applies_live_over_the_same_client() {
    let _secrets = crate::test_support::isolate_secrets();
    let target = fixture_target("APACHE", 13480, FIXTURE_USER);
    let params = WebdavConnectionParams::new(target.base_url.clone(), &target.username, "/photos");
    keychain::save_credentials(
        &params.credential_service(),
        Some(&params.username),
        &params.username,
        &target.password,
    )
    .expect("the test secret store always accepts");
    let WebdavConnection::Connected { volume_id } =
        webdav_volume_wiring::connect_and_register("", None, params.clone(), "webdav-edit-widen", None).await
    else {
        panic!("a fixture with its password stored must connect");
    };
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let narrow = manager.get(&volume_id).expect("just registered");
    let client_before = narrow
        .as_any()
        .downcast_ref::<WebdavVolume>()
        .expect("a WebDAV volume")
        .client_witness()
        .await
        .expect("a live client");
    let prefix = cmdr_fs::volume::webdav_app_root(params.host(), params.port(), &params.username);

    let outcome = webdav_volume_wiring::save_without_connecting(KnownWebdavServer {
        url: target.base_url.to_string(),
        username: target.username.clone(),
        display_name: String::new(),
        remote_root: "/".to_string(),
        start_folder: None,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
    })
    .await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    let wide = manager.get(&volume_id).expect("still registered");
    let wide_root = format!("{prefix}/");
    assert_eq!(wide.root(), Path::new(&wide_root), "the registry serves the wider root");
    assert!(
        wide.exists(Path::new(&format!("{prefix}/hello.txt"))).await,
        "a file only the wider root covers lists through the live client"
    );
    let webdav = wide.as_any().downcast_ref::<WebdavVolume>().expect("a WebDAV volume");
    let client_after = webdav.client_witness().await.expect("still a live client");
    assert!(
        std::sync::Weak::ptr_eq(&client_before, &client_after),
        "❗ no re-probe: the client that served the old root serves the new one"
    );
    assert!(
        !wide.retirement().expect("keeps a flag").is_retired(),
        "❗ the swap retired nothing"
    );

    webdav.simulate_session_loss().await;
    assert!(
        wide.list_directory(Path::new(&wide_root), None).await.is_err(),
        "an operation is what notices the loss and starts the loop"
    );
    cmdr_fs::testing::wait_until_async(
        Duration::from_secs(30),
        "the reconnect loop to bring the client back",
        || wide.connection_state().is_some_and(ConnectionState::is_live),
    )
    .await;
    assert!(
        wide.list_directory(Path::new(&wide_root), None).await.is_ok(),
        "the rebuilt client serves the wider root"
    );

    webdav_volume_wiring::disconnect(&volume_id).await;
}
