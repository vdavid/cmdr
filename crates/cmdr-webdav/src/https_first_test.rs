//! Which address an `http` add dials (#540): its `https` twin when that
//! answers as WebDAV and its dial comes up, else the address as typed.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::dial_preferring_https_with;
use crate::errors::WebdavConnectError;
use crate::params::WebdavConnectionParams;
use crate::transport;

fn params(url: &str) -> WebdavConnectionParams {
    let mut params = WebdavConnectionParams::new(Url::parse(url).expect("a valid test URL"), "test_user_1", "/Photos");
    params.auto_reconnect = false;
    params
}

/// A dial that records every address it was handed, and answers each with
/// `answer(url)`.
struct Dials {
    tried: Mutex<Vec<String>>,
}

impl Dials {
    fn new() -> Self {
        Self {
            tried: Mutex::new(Vec::new()),
        }
    }

    fn tried(&self) -> Vec<String> {
        self.tried.lock().expect("test lock").clone()
    }

    async fn dial(
        &self,
        params: WebdavConnectionParams,
        answer: impl Fn(&str) -> Result<(), WebdavConnectError>,
    ) -> Result<WebdavConnectionParams, WebdavConnectError> {
        let url = params.base_url.to_string();
        self.tried.lock().expect("test lock").push(url.clone());
        answer(&url).map(|()| params)
    }
}

fn up(_: &str) -> Result<(), WebdavConnectError> {
    Ok(())
}

#[tokio::test]
async fn an_http_address_moves_to_https_when_that_answers() {
    let dials = Dials::new();
    let chosen = dial_preferring_https_with(
        params("http://testnas.local/dav/"),
        |url| async move {
            assert_eq!(url.as_str(), "https://testnas.local/dav/", "the probe asks the twin");
            true
        },
        &CancellationToken::new(),
        |params| dials.dial(params, up),
    )
    .await
    .expect("connected");

    assert_eq!(chosen.base_url.as_str(), "https://testnas.local/dav/");
    assert_eq!(chosen.username, "test_user_1");
    assert_eq!(chosen.remote_root.to_string_lossy(), "/Photos");
    assert!(!chosen.auto_reconnect, "the person's switches come along");
    assert_eq!(dials.tried(), ["https://testnas.local/dav/"]);
}

#[tokio::test]
async fn an_http_address_is_dialed_as_typed_when_https_does_not_answer() {
    let dials = Dials::new();
    let chosen = dial_preferring_https_with(
        params("http://testnas.local:5005/dav/"),
        |_| async { false },
        &CancellationToken::new(),
        |params| dials.dial(params, up),
    )
    .await
    .expect("connected");
    assert_eq!(chosen.base_url.as_str(), "http://testnas.local:5005/dav/");
    assert_eq!(dials.tried(), ["http://testnas.local:5005/dav/"]);
}

/// ❗ **An `https` twin that answered the probe but whose dial didn't come up
/// falls back to the typed address** (#540): a 443 serving another site, or a
/// root the twin doesn't have, must not strand an Add the `http` address would
/// have connected.
#[tokio::test]
async fn an_https_twin_whose_dial_fails_falls_back_to_the_typed_address() {
    let refusals: [fn() -> WebdavConnectError; 3] = [
        || WebdavConnectError::NotAWebdavServer,
        || WebdavConnectError::AuthenticationRejected,
        || WebdavConnectError::Unreachable("gone".to_string()),
    ];
    for refusal in refusals {
        let dials = Dials::new();
        let chosen = dial_preferring_https_with(
            params("http://testnas.local/dav/"),
            |_| async { true },
            &CancellationToken::new(),
            |params| {
                dials.dial(params, move |url| {
                    if url.starts_with("https") {
                        Err(refusal())
                    } else {
                        Ok(())
                    }
                })
            },
        )
        .await
        .expect("the typed address connected");

        assert_eq!(chosen.base_url.as_str(), "http://testnas.local/dav/");
        assert_eq!(
            dials.tried(),
            ["https://testnas.local/dav/", "http://testnas.local/dav/"]
        );
    }
}

/// ❌ A cancelled twin dial never dials again: Cancel means stop.
#[tokio::test]
async fn a_cancelled_https_dial_does_not_fall_back() {
    let dials = Dials::new();
    let answer = dial_preferring_https_with(
        params("http://testnas.local/dav/"),
        |_| async { true },
        &CancellationToken::new(),
        |params| dials.dial(params, |_| Err(WebdavConnectError::Cancelled)),
    )
    .await;

    assert!(matches!(answer, Err(WebdavConnectError::Cancelled)));
    assert_eq!(dials.tried(), ["https://testnas.local/dav/"]);
}

/// ❌ Nor does a dial whose attempt was called off while it failed some other way.
#[tokio::test]
async fn a_called_off_attempt_does_not_fall_back() {
    let cancel = CancellationToken::new();
    let dials = Dials::new();
    let answer = dial_preferring_https_with(
        params("http://testnas.local/dav/"),
        |_| async { true },
        &cancel,
        |params| {
            cancel.cancel();
            dials.dial(params, |_| Err(WebdavConnectError::TimedOut))
        },
    )
    .await;

    assert!(matches!(answer, Err(WebdavConnectError::TimedOut)));
    assert_eq!(dials.tried(), ["https://testnas.local/dav/"]);
}

/// A probe Cancel cut short dials nothing at all.
#[tokio::test]
async fn a_probe_cut_by_cancel_dials_nothing() {
    let cancel = CancellationToken::new();
    let dials = Dials::new();
    let answer = dial_preferring_https_with(
        params("http://testnas.local/dav/"),
        |_| {
            cancel.cancel();
            async { false }
        },
        &cancel,
        |params| dials.dial(params, up),
    )
    .await;

    assert!(matches!(answer, Err(WebdavConnectError::Cancelled)));
    assert!(dials.tried().is_empty());
}

#[tokio::test]
async fn an_https_address_asks_nothing() {
    let asked = AtomicBool::new(false);
    let dials = Dials::new();
    let chosen = dial_preferring_https_with(
        params("https://cloud.example.com/dav/"),
        |_| {
            asked.store(true, Ordering::Relaxed);
            async { true }
        },
        &CancellationToken::new(),
        |params| dials.dial(params, up),
    )
    .await
    .expect("connected");
    assert_eq!(chosen.base_url.as_str(), "https://cloud.example.com/dav/");
    assert!(!asked.load(Ordering::Relaxed));
}

// ── The real probe ───────────────────────────────────────────────────

/// A one-response-per-connection server answering every request with `reply`.
async fn serving(reply: &'static [u8]) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await;
            let _ = socket.write_all(reply).await;
        }
    });
    port
}

/// ❗ The commonest NAS case: `http://nas:5005` has no TLS on that port. A TLS
/// hello to a plain HTTP server must read as "doesn't answer", fast.
#[tokio::test]
async fn a_plain_http_port_does_not_answer_over_tls() {
    let port = serving(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    let dials = Dials::new();

    let chosen = params(&format!("http://127.0.0.1:{port}/dav/"))
        .dial_preferring_https(&CancellationToken::new(), |params| dials.dial(params, up))
        .await
        .expect("connected");

    assert_eq!(chosen.base_url.scheme(), "http");
}

#[tokio::test]
async fn a_closed_port_does_not_answer_over_tls() {
    let dials = Dials::new();
    let chosen = params("http://127.0.0.1:1/dav/")
        .dial_preferring_https(&CancellationToken::new(), |params| dials.dial(params, up))
        .await
        .expect("connected");
    assert_eq!(chosen.base_url.scheme(), "http");
}

/// ❗ **Only a WebDAV answer counts** (#540): a 443 serving another site (or a
/// redirect back to `http`) answers `OPTIONS` without a `DAV` header, and
/// counting it moved the dial to a server that isn't the one typed. Asked over
/// plain `http` here; the scheme is the caller's, the header is the probe's.
#[tokio::test]
async fn an_answer_without_a_dav_header_is_not_a_webdav_server() {
    let port =
        serving(b"HTTP/1.1 200 OK\r\nAllow: GET, HEAD, OPTIONS\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
    let url = Url::parse(&format!("http://127.0.0.1:{port}/dav/")).expect("url");
    assert!(!transport::answers_as_webdav(url, &CancellationToken::new()).await);

    let port = serving(b"HTTP/1.1 301 Moved Permanently\r\nLocation: http://testnas.local/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    let url = Url::parse(&format!("http://127.0.0.1:{port}/dav/")).expect("url");
    assert!(!transport::answers_as_webdav(url, &CancellationToken::new()).await);
}

#[tokio::test]
async fn an_answer_with_a_dav_header_is_a_webdav_server() {
    let port = serving(b"HTTP/1.1 200 OK\r\nDAV: 1, 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    let url = Url::parse(&format!("http://127.0.0.1:{port}/dav/")).expect("url");
    assert!(transport::answers_as_webdav(url, &CancellationToken::new()).await);
}
