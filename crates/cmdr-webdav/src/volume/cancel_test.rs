//! Calling a connect off, on both sides of the credential read.
//!
//! ❗ These live HERE rather than beside the app's attempt table, because the
//! app's host reads the real Keychain and a test can't seed one. The crate takes
//! its host as an argument, so an `InMemoryCredentials` is what makes "a dial
//! that is genuinely in the air" reachable at all.
//!
//! `192.0.2.1` is reserved for documentation (RFC 5737) and routed nowhere, so a
//! probe at it hangs exactly the way a typo'd hostname does and can only ever
//! end by being called off.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::connect_webdav_volume;
use crate::errors::WebdavConnectError;
use crate::params::WebdavConnectionParams;

/// Params at an address that never answers, and a host that already knows the
/// secret, so the dial reaches the probe rather than stopping at the store.
fn nowhere() -> (WebdavConnectionParams, VolumeHost) {
    let params = WebdavConnectionParams::new(Url::parse("https://192.0.2.1/dav/").expect("a literal"), "ada", "/");
    let credentials =
        InMemoryCredentials::new().with_entry(&params.credential_service(), Some("ada"), "ada", "openthedoor");
    let host = VolumeHost::builder().credentials(Arc::new(credentials)).build();
    (params, host)
}

#[tokio::test]
async fn a_cancel_mid_probe_ends_the_dial_long_before_the_connect_budget() {
    let (params, host) = nowhere();
    let cancel = CancellationToken::new();

    let dialing = cancel.clone();
    let connecting =
        tokio::spawn(
            async move { connect_webdav_volume("Nowhere", "webdav-cancel-probe", params, host, dialing).await },
        );
    // The probe can't finish on its own at this address, so whenever the cancel
    // lands the `select!` has nothing else to answer with.
    cancel.cancel();

    let outcome = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .expect("a cancelled dial answers long before the 10 s connect budget")
        .expect("the connect task must not panic");
    assert!(
        matches!(outcome, Err(WebdavConnectError::Cancelled)),
        "a cancelled dial says so, rather than reporting the address as unreachable. Got {outcome:?}",
        outcome = outcome.map(|_| "a volume")
    );
}

#[tokio::test]
async fn a_cancel_that_landed_before_the_secret_was_read_is_still_honored() {
    // ❗ The store is the FIRST thing a dial touches, and on macOS it can put a
    // Keychain prompt in front of the whole connect. A cancel that landed while
    // that modal was up is answered as soon as it clears: the store read is not
    // itself interruptible, but `build_and_probe`'s `select!` sees the token
    // before anything reaches the wire. What this cell holds is that nothing is
    // dialed at an address the user already gave up on.
    let (params, host) = nowhere();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        connect_webdav_volume("Nowhere", "webdav-cancel-before-read", params, host, cancel),
    )
    .await
    .expect("an already-cancelled dial answers at once");

    assert!(
        matches!(outcome, Err(WebdavConnectError::Cancelled)),
        "Got {outcome:?}",
        outcome = outcome.map(|_| "a volume")
    );
}

#[tokio::test]
async fn a_dial_with_nothing_in_the_store_asks_for_credentials_rather_than_dialing() {
    // The counterpart to the two above: with no secret there is nothing to
    // cancel, and the dial never reaches the wire.
    let params = WebdavConnectionParams::new(Url::parse("https://192.0.2.1/dav/").expect("a literal"), "ada", "/");
    let host = VolumeHost::builder()
        .credentials(Arc::new(InMemoryCredentials::new()))
        .build();

    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        connect_webdav_volume("Nowhere", "webdav-no-secret", params, host, CancellationToken::new()),
    )
    .await
    .expect("a dial with nothing to offer answers without touching the network");

    assert!(
        matches!(outcome, Err(WebdavConnectError::NeedsCredentials)),
        "Got {outcome:?}",
        outcome = outcome.map(|_| "a volume")
    );
}
