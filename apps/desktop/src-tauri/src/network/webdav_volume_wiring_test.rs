//! The lifecycle a connect and a disconnect owe the rest of the app: a volume in
//! the registry, an entry in the server list, and a client that is GONE
//! afterwards.
//!
//! App-side because that is what these assert on. The dial itself and the byte
//! path are the crate's own cells (`crates/cmdr-webdav/DETAILS.md`).
//!
//! ❗ The cells that reach a server need the Docker stack:
//! `apps/desktop/test/webdav-servers/start.sh`. Everything here runs without
//! one; the fixture-backed lifecycle cells live beside the fixtures once they
//! land.

use std::time::Duration;

use cmdr_webdav::WebdavConnectionParams;

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

/// The reconnect switch on an unmounted volume is a plain no, so editing a
/// saved server that isn't open stays an ordinary edit.
#[test]
fn applying_the_switch_to_an_unmounted_volume_is_a_plain_no() {
    assert!(!webdav_volume_wiring::apply_auto_reconnect(
        "webdav-nothing-is-registered-here",
        false
    ));
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
    const ATTEMPT: &str = "webdav-a-connect-that-ends";
    let base_url = url::Url::parse("https://192.0.2.1/dav/").expect("a literal");
    let params = WebdavConnectionParams::new(base_url.clone(), "nobody", "/");
    let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &params.username);

    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        webdav_volume_wiring::connect_and_register("Nowhere", params.clone(), ATTEMPT),
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
