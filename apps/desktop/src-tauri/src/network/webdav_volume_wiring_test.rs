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

/// A connect the user calls off ends at once and ❗ leaves nothing behind: no
/// volume in the registry, no server in the saved list.
///
/// `192.0.2.1` is reserved for documentation (RFC 5737) and routed nowhere, so
/// this dial hangs exactly the way a typo'd hostname does. ❗ That hang IS the
/// subject: without the cancel it holds for the whole connect budget, and a
/// sign-in dialog with it.
#[tokio::test]
async fn cancelling_a_hanging_connect_ends_it_and_registers_nothing() {
    const ATTEMPT: &str = "webdav-cancel-a-hanging-connect";
    let base_url = url::Url::parse("https://192.0.2.1/dav/").expect("a literal");
    let params = WebdavConnectionParams::new(base_url.clone(), "nobody", "/");
    let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &params.username);

    let dialing = params.clone();
    let connecting =
        tokio::spawn(async move { webdav_volume_wiring::connect_and_register("Nowhere", dialing, ATTEMPT).await });

    // The attempt is cancelable from the moment the dial is in the air, which is
    // the whole reason the id is the caller's.
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the connect attempt to be cancelable", || {
        webdav_volume_wiring::cancel_connect(ATTEMPT)
    })
    .await;

    let outcome = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("a cancelled connect answers long before the connect budget runs out")
        .expect("the connect task must not panic");
    assert!(
        matches!(outcome, WebdavConnection::Cancelled),
        "a cancelled connect says so, rather than reporting the address as unreachable"
    );

    assert!(
        crate::file_system::volume::manager::get_volume_manager()
            .get(&volume_id)
            .is_none(),
        "❗ a cancelled connect registers nothing"
    );
    assert!(
        !webdav_known_servers::all()
            .into_iter()
            .any(|entry| entry.url == base_url.as_str()),
        "❗ a cancelled connect remembers no server either"
    );
    assert!(
        !keychain::has_credentials(&params.credential_service(), Some(&params.username)),
        "❗ and it writes no secret: only `save_webdav_credentials` ever does"
    );

    // The entry is gone with the attempt, so a second cancel finds nothing.
    assert!(!webdav_volume_wiring::cancel_connect(ATTEMPT));
}
