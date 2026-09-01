//! The wire vocabulary, which is the whole point of this module: the frontend
//! branches on these values and never on a message.

use super::*;

/// The secret store is keyed per server origin, ❌ never per host alone, and the
/// key is the crate's own, so what the form saves is what the dial reads.
#[test]
fn the_credential_service_carries_the_origin_and_the_scope_carries_the_account() {
    assert_eq!(
        credential_key("https://dav.example.test/remote.php/dav/", "ada").as_deref(),
        Some("https://dav.example.test:443")
    );
    assert_ne!(
        credential_key("https://dav.example.test/dav/", "ada"),
        credential_key("https://dav.example.test:8443/dav/", "ada"),
        "two listeners on one machine are different servers"
    );
    assert_eq!(
        credential_key("https://dav.example.test/a/", "ada"),
        credential_key("https://dav.example.test/b/", "ada"),
        "the path is addressing: one origin, one secret"
    );
}

/// What isn't a server URL is a typed `None`, never a parse message.
#[test]
fn a_url_that_is_not_http_has_no_credential_key() {
    assert!(credential_key("not a url", "ada").is_none());
    assert!(credential_key("ftp://dav.example.test/", "ada").is_none());
    assert!(
        credential_key("dav.example.test/dav", "ada").is_none(),
        "no scheme, no origin"
    );
}

/// The connect outcome is tagged, so the frontend switches on a field rather
/// than sniffing which key is present, and a bad URL is a variant of its own.
#[tokio::test]
async fn the_connect_outcome_names_itself_on_the_wire() {
    let refused = serde_json::to_value(WebdavConnectResult::AuthenticationRejected).expect("serializes");
    assert_eq!(refused["outcome"], "authentication_rejected");

    let connected = serde_json::to_value(WebdavConnectResult::Connected(ConnectedWebdavVolume {
        volume_id: "webdav-x".to_string(),
    }))
    .expect("serializes");
    assert_eq!(connected["outcome"], "connected");
    assert_eq!(connected["volumeId"], "webdav-x");

    let outcome = connect_webdav_volume(
        "Nowhere".to_string(),
        "ftp://dav.example.test/".to_string(),
        "ada".to_string(),
        "/".to_string(),
        true,
        "webdav-invalid-url-attempt".to_string(),
    )
    .await;
    assert!(
        matches!(outcome, WebdavConnectResult::InvalidUrl),
        "❗ a URL that isn't http(s) is answered before anything is dialed"
    );
}

/// The three credential commands have to agree on the key, or a saved password
/// is invisible to the check that decides whether to show a sign-in form.
///
/// Round-tripped through the real store (the test backend), because the risk
/// isn't in any one of them: it's the three passing the service and scope
/// differently.
#[tokio::test]
async fn the_credential_trio_agrees_on_where_a_secret_lives() {
    let url = "https://credential-trio.webdav.test/dav/";

    assert!(
        !has_webdav_credentials(url.to_string(), "ada".to_string()).await,
        "nothing is stored before anything is saved"
    );

    save_webdav_credentials(url.to_string(), "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");
    assert!(has_webdav_credentials(url.to_string(), "ada".to_string()).await);

    // ❗ Another account on the same server is a different entry, not a shared
    // one: a reconnect that retried the wrong account's secret is how an account
    // gets locked.
    assert!(!has_webdav_credentials(url.to_string(), "grace".to_string()).await);
    // And so is the same account on another port.
    assert!(
        !has_webdav_credentials(
            "https://credential-trio.webdav.test:8443/dav/".to_string(),
            "ada".to_string()
        )
        .await
    );
    // A URL that isn't one has nothing stored and refuses to store anything.
    assert!(!has_webdav_credentials("nope".to_string(), "ada".to_string()).await);
    assert!(matches!(
        save_webdav_credentials("nope".to_string(), "ada".to_string(), "pa55".to_string()).await,
        Err(KeychainError::Other(_))
    ));

    delete_webdav_credentials(url.to_string(), "ada".to_string())
        .await
        .expect("the test store always accepts");
    assert!(!has_webdav_credentials(url.to_string(), "ada".to_string()).await);
}

/// The known-servers commands round-trip through the same store the connect path
/// writes.
#[tokio::test]
async fn the_known_servers_trio_round_trips() {
    let url = "https://known-servers-trio.webdav.test/dav";
    update_known_webdav_server(
        url.to_string(),
        "ada".to_string(),
        "Trio".to_string(),
        "/".to_string(),
        false,
    );

    let mine: Vec<_> = get_known_webdav_servers()
        .into_iter()
        .filter(|entry| entry.url.starts_with(url))
        .collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].display_name, "Trio");
    assert_eq!(
        mine[0].url,
        format!("{url}/"),
        "stored normalized, with its trailing slash"
    );
    assert!(
        !mine[0].auto_reconnect,
        "❗ the switch is the user's, so editing a server has to be able to turn it off"
    );

    assert!(forget_known_webdav_server(url.to_string(), "ada".to_string()));
    assert!(
        !get_known_webdav_servers()
            .iter()
            .any(|entry| entry.url.starts_with(url)),
        "a forgotten server is gone from the list"
    );
}
