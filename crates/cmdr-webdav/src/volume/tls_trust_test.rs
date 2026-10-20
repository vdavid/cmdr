//! Trusting a self-signed certificate, against a real one: `webdav-fixture-tls`
//! serves the stock export over HTTPS with a certificate nobody issued, the way
//! a home NAS ships. The pin's decision table without a server is
//! `src/tls_trust_test.rs`.
//!
//! The certificate is minted when the container first starts, so no cell knows
//! its fingerprint ahead of time: each reads it off the prompt, the way a
//! person does.

use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::certificates::InMemoryCertificates;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::tls::{CertificatePrompt, CertificatePromptKind};
use tokio_util::sync::CancellationToken;

use super::testing::*;
use super::{WebdavVolume, connect_webdav_volume};
use crate::WebdavConnectError;

/// Dials `service` with `certificates` answering the pins.
async fn dial_trusting(
    service: &str,
    fallback_port: u16,
    certificates: Arc<InMemoryCertificates>,
    cell: &str,
) -> Result<WebdavVolume, WebdavConnectError> {
    connect_webdav_volume(
        "fixture",
        &format!("webdav-test-{cell}"),
        fixture_target(service, fallback_port, FIXTURE_USER).params(),
        fixture_host_trusting(certificates),
        CancellationToken::new(),
    )
    .await
}

/// The prompt a dial with nothing pinned answers.
async fn first_contact(cell: &str) -> CertificatePrompt {
    match dial_trusting("TLS", 13484, Arc::new(InMemoryCertificates::new()), cell).await {
        Err(WebdavConnectError::CertificateNeedsApproval(prompt)) => prompt,
        other => panic!(
            "a self-signed server should ask to trust its certificate, got {:?}",
            other.map(|_| "a volume")
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture-tls)"]
async fn a_self_signed_server_asks_to_trust_its_certificate() {
    if not_for_your_own_server("`webdav-fixture-tls`, a server with a self-signed certificate") {
        return;
    }
    let prompt = first_contact("tls-first-contact").await;

    assert_eq!(prompt.kind, CertificatePromptKind::Unknown);
    assert_eq!(prompt.host, "127.0.0.1");
    assert_eq!(prompt.port, fixture_port("TLS", 13484));
    // The SHA-256, as 32 uppercase hex pairs, which is what `openssl x509
    // -fingerprint -sha256` prints for the same certificate.
    assert_eq!(prompt.fingerprint.split(':').count(), 32, "{}", prompt.fingerprint);
    assert!(
        prompt
            .fingerprint
            .chars()
            .all(|c| c == ':' || c.is_ascii_digit() || c.is_ascii_uppercase()),
        "{}",
        prompt.fingerprint
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture-tls)"]
async fn a_pinned_certificate_connects_lists_and_reconnects() {
    if not_for_your_own_server("`webdav-fixture-tls`, a server with a self-signed certificate") {
        return;
    }
    let prompt = first_contact("tls-pin-before").await;
    let certificates = Arc::new(InMemoryCertificates::new());
    certificates.pin(&prompt.host, prompt.port, &prompt.fingerprint);

    let volume = dial_trusting("TLS", 13484, Arc::clone(&certificates), "tls-pinned")
        .await
        .unwrap_or_else(|e| panic!("the pinned certificate should connect, got {e:?}"));
    assert!(volume.exists(&volume.root().join("hello.txt")).await);

    // ❗ A reconnect builds a new client, and reads the pin again for it.
    volume
        .attempt_reconnect()
        .await
        .unwrap_or_else(|e| panic!("a reconnect over the pin should come back, got {e:?}"));
    assert!(volume.exists(&volume.root().join("hello.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture-tls)"]
async fn a_certificate_other_than_the_pinned_one_has_changed() {
    if not_for_your_own_server("`webdav-fixture-tls`, a server with a self-signed certificate") {
        return;
    }
    let presented = first_contact("tls-changed-before").await.fingerprint;
    let certificates = Arc::new(InMemoryCertificates::new());
    // The certificate this server served before it was reinstalled, or the one
    // a box in the middle doesn't have.
    certificates.pin("127.0.0.1", fixture_port("TLS", 13484), &["AB"; 32].join(":"));

    match dial_trusting("TLS", 13484, certificates, "tls-changed").await {
        Err(WebdavConnectError::CertificateNeedsApproval(prompt)) => {
            assert_eq!(prompt.kind, CertificatePromptKind::Changed);
            assert_eq!(
                prompt.fingerprint, presented,
                "the prompt shows what the server presents NOW"
            );
        }
        other => panic!(
            "a certificate other than the pinned one must ask again, got {:?}",
            other.map(|_| "a volume")
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
async fn a_broken_handshake_is_never_offered_for_trust() {
    if not_for_your_own_server("`webdav-fixture-apache`, a plain-http port") {
        return;
    }
    // TLS spoken at a port that answers plain HTTP: the handshake breaks before
    // any certificate exists, so there is nothing a person could pin.
    let mut params = fixture_target("APACHE", 13480, FIXTURE_USER).params();
    params
        .base_url
        .set_scheme("https")
        .expect("http to https is a valid swap");
    // The secret is filed under the scheme, so this address needs its own.
    let credentials = InMemoryCredentials::new().with_entry(
        &params.credential_service(),
        Some(FIXTURE_USER),
        FIXTURE_USER,
        FIXTURE_PASSWORD,
    );
    let refused = connect_webdav_volume(
        "fixture",
        "webdav-test-tls-broken-handshake",
        params,
        VolumeHost::builder().credentials(Arc::new(credentials)).build(),
        CancellationToken::new(),
    )
    .await;
    assert!(
        matches!(refused, Err(WebdavConnectError::Unreachable(_))),
        "got {:?}",
        refused.map(|_| "a volume")
    );
}
