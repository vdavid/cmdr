//! What the transport puts on the wire, read back by a one-shot local server.

use std::time::Duration;

use http::Method;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use super::S3Client;
use crate::profile::{Preset, ProviderProfile};
use crate::request::S3Request;
use crate::sigv4::Credentials;

/// Serves one request with `204` and hands back its head, header names
/// lowercased.
async fn one_request_head(send: impl AsyncFnOnce(S3Client, String)) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            let read = socket.read(&mut chunk).await.unwrap();
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        let _ = socket
            .write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n")
            .await;
        String::from_utf8_lossy(&buffer).to_lowercase()
    });
    let profile = ProviderProfile::from_preset(&Preset::Other {
        endpoint: Url::parse(&format!("http://{addr}")).unwrap(),
        region: None,
        path_style: true,
    })
    .unwrap();
    let host = profile.endpoint_host.clone();
    send(
        S3Client::new(profile, Credentials::new("AKID", "secret")).unwrap(),
        host,
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
}

/// ❗ GCS answers `411 Length Required` to a bodyless POST without
/// `Content-Length` (`CreateMultipartUpload`; observed live on 2026-10-02).
#[tokio::test]
async fn a_bodyless_post_says_its_length_is_zero() {
    let head = one_request_head(async |client, host| {
        let request = S3Request::new(Method::POST, "http", &host, "/b/k".into()).query("uploads", "");
        client.exchange(request, Duration::from_secs(5)).await.unwrap();
    })
    .await;
    assert!(head.contains("\r\ncontent-length: 0\r\n"), "{head}");
}

/// ❗ A folder marker is a PUT of zero bytes held in memory: R2, Hetzner, and
/// GCS answer `411` when it says no length (live, 2026-10-02).
#[tokio::test]
async fn an_empty_put_body_says_its_length_is_zero() {
    let head = one_request_head(async |client, host| {
        let mut request = S3Request::new(Method::PUT, "http", &host, "/b/folder/".into());
        request.body = crate::request::Body::Bytes(Vec::new());
        client.exchange(request, Duration::from_secs(5)).await.unwrap();
    })
    .await;
    assert!(head.contains("\r\ncontent-length: 0\r\n"), "{head}");
}

/// `cmdr stores these bytes verbatim\n` gzipped (`gzip -9n`): an object stored
/// with `Content-Encoding: gzip`, the way web assets often sit on S3.
const STORED_GZIP: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0xce, 0x4d, 0x29, 0x52, 0x28, 0x2e, 0xc9, 0x2f,
    0x4a, 0x2d, 0x56, 0x28, 0xc9, 0x48, 0x2d, 0x4e, 0x55, 0x48, 0xaa, 0x2c, 0x01, 0xb2, 0xcb, 0x52, 0x8b, 0x92, 0x12,
    0x4b, 0x32, 0x73, 0xb9, 0x00, 0xce, 0xed, 0x88, 0x3e, 0x21, 0x00, 0x00, 0x00,
];

/// A server that answers every request (up to `count`) with the stored gzip
/// object and its headers, and a client pointed at it.
async fn serving_stored_gzip(count: usize) -> (S3Client, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        for _ in 0..count {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 4096];
            while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                let read = socket.read(&mut chunk).await.unwrap();
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-encoding: gzip\r\ncontent-length: {}\r\n\
                 etag: \"stored\"\r\nconnection: close\r\n\r\n",
                STORED_GZIP.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            if !buffer.starts_with(b"HEAD") {
                let _ = socket.write_all(STORED_GZIP).await;
            }
            let _ = socket.shutdown().await;
        }
    });
    let profile = ProviderProfile::from_preset(&Preset::Other {
        endpoint: Url::parse(&format!("http://{addr}")).unwrap(),
        region: None,
        path_style: true,
    })
    .unwrap();
    let host = profile.endpoint_host.clone();
    (
        S3Client::new(profile, Credentials::new("AKID", "secret")).unwrap(),
        host,
    )
}

/// ❗ A file manager copies bytes, it doesn't decode them: an object stored
/// with `Content-Encoding: gzip` reads back as the stored gzip, at the length
/// its `Content-Length` (and its ETag) describe. The test build has every
/// reqwest decoder on (`Cargo.toml`'s dev-dependencies), as the app's graph
/// may: the client must turn each one off itself.
#[tokio::test]
async fn an_encoded_object_reads_back_as_its_stored_bytes() {
    let (client, host) = serving_stored_gzip(3).await;

    let get = S3Request::new(Method::GET, "http", &host, "/b/page.html".into());
    let answer = client.exchange(get, Duration::from_secs(5)).await.unwrap();
    assert_eq!(answer.body, STORED_GZIP, "a whole-body GET");

    let head = S3Request::new(Method::HEAD, "http", &host, "/b/page.html".into());
    let answer = client.exchange(head, Duration::from_secs(5)).await.unwrap();
    assert_eq!(
        answer.header("content-length"),
        Some(STORED_GZIP.len().to_string().as_str()),
        "a HEAD keeps the stored length"
    );

    let get = S3Request::new(Method::GET, "http", &host, "/b/page.html".into());
    let mut opened = client.open(get, "volume", "/b/page.html").await.unwrap();
    let mut streamed = Vec::new();
    while let Some(piece) = opened.chunk().await {
        streamed.extend(piece.unwrap());
    }
    assert_eq!(streamed, STORED_GZIP, "a streamed GET (every file read)");
}

/// The head of every request `send` makes to a GCS-profile client, served
/// `204` by a local server behind `storage.googleapis.com`.
async fn gcs_request_heads(count: usize, send: impl AsyncFnOnce(S3Client)) -> Vec<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut heads = Vec::new();
        for _ in 0..count {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 4096];
            while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
                let read = socket.read(&mut chunk).await.unwrap();
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
            }
            let _ = socket
                .write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
            heads.push(String::from_utf8_lossy(&buffer).to_lowercase());
        }
        heads
    });
    let mut profile = ProviderProfile::from_preset(&Preset::Gcs).unwrap();
    profile.scheme = "http".into();
    let client = S3Client::resolving(
        profile,
        Credentials::new("GOOG1EXAMPLE", "secret"),
        &["storage.googleapis.com".to_string()],
        addr,
    );
    send(client).await;
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
}

/// ❗ GCS serves an object stored with `Content-Encoding: gzip` DECOMPRESSED
/// (its "decompressive transcoding": 33 B back for 51 B stored, no
/// `Content-Length`, and a `Range` ignored) unless asked with `Accept-Encoding:
/// gzip`. Its front end rewrites that header before checking the signature
/// (`SignatureDoesNotMatch` when it's signed), so it goes out unsigned, on
/// every read of an object (live, 2026-10-02).
#[tokio::test]
async fn a_read_from_gcs_asks_for_the_stored_bytes_unsigned() {
    let heads = gcs_request_heads(2, async |client| {
        for method in [Method::GET, Method::HEAD] {
            let request = S3Request::new(method, "http", "storage.googleapis.com", "/b/page.html".into());
            client.exchange(request, Duration::from_secs(5)).await.unwrap();
        }
    })
    .await;
    for head in heads {
        assert!(head.contains("\r\naccept-encoding: gzip\r\n"), "{head}");
        let signed = head
            .lines()
            .find(|line| line.starts_with("authorization:"))
            .unwrap_or_default()
            .to_string();
        assert!(!signed.contains("accept-encoding"), "{signed}");
    }
}

/// ❗ GCS answers a HEAD of a gzip-stored object with no `Content-Length`, only
/// `x-goog-stored-content-length` (live, 2026-10-02): an object's length is
/// the first, else the second, so a stat still knows its size.
#[test]
fn an_objects_length_falls_back_to_the_stored_length_gcs_names() {
    let answer = |pairs: &[(&'static str, &'static str)]| {
        let mut headers = http::HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, http::HeaderValue::from_static(value));
        }
        super::Answer {
            status: http::StatusCode::OK,
            headers,
            body: Vec::new(),
        }
    };
    assert_eq!(
        answer(&[("x-goog-stored-content-length", "51")]).object_length(),
        Some(51)
    );
    assert_eq!(
        answer(&[("content-length", "5"), ("x-goog-stored-content-length", "5")]).object_length(),
        Some(5)
    );
    assert_eq!(answer(&[("content-length", "7")]).object_length(), Some(7));
    assert_eq!(answer(&[]).object_length(), None);
}

/// A client pointed at a server that answers one request with a 200 whose
/// body runs `size` bytes, framed by `Content-Length` or chunked.
async fn serving_an_answer_of(size: usize, chunked: bool) -> (S3Client, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        while !buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            let read = socket.read(&mut chunk).await.unwrap();
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        let framing = if chunked {
            "transfer-encoding: chunked".to_string()
        } else {
            format!("content-length: {size}")
        };
        let head =
            format!("HTTP/1.1 200 OK\r\ncontent-type: application/xml\r\n{framing}\r\nconnection: close\r\n\r\n");
        let _ = socket.write_all(head.as_bytes()).await;
        let piece = vec![b'x'; 64 * 1024];
        let mut left = size;
        while left > 0 {
            let n = left.min(piece.len());
            if chunked {
                let _ = socket.write_all(format!("{n:x}\r\n").as_bytes()).await;
            }
            if socket.write_all(&piece[..n]).await.is_err() {
                return;
            }
            if chunked {
                let _ = socket.write_all(b"\r\n").await;
            }
            left -= n;
        }
        if chunked {
            let _ = socket.write_all(b"0\r\n\r\n").await;
        }
        let _ = socket.shutdown().await;
    });
    let profile = ProviderProfile::from_preset(&Preset::Other {
        endpoint: Url::parse(&format!("http://{addr}")).unwrap(),
        region: None,
        path_style: true,
    })
    .unwrap();
    let host = profile.endpoint_host.clone();
    (
        S3Client::new(profile, Credentials::new("AKID", "secret")).unwrap(),
        host,
    )
}

/// ❗ A hostile or broken endpoint can answer a listing with a body that never
/// ends: past `MAX_ANSWER_BODY` it's a typed `BodyTooLarge`, never a growing
/// buffer, however the body is framed.
#[tokio::test]
async fn an_answer_past_the_cap_is_refused() {
    for chunked in [false, true] {
        let (client, host) = serving_an_answer_of(super::MAX_ANSWER_BODY + 1, chunked).await;
        let request = S3Request::new(Method::GET, "http", &host, "/b".into()).query("list-type", "2");
        let result = client.exchange(request, Duration::from_secs(30)).await;
        assert!(
            matches!(result, Err(super::ExchangeError::BodyTooLarge { .. })),
            "chunked={chunked}: {result:?}"
        );
    }
}

#[tokio::test]
async fn an_answer_under_the_cap_is_read() {
    let (client, host) = serving_an_answer_of(64 * 1024, true).await;
    let request = S3Request::new(Method::GET, "http", &host, "/b".into()).query("list-type", "2");
    let answer = client.exchange(request, Duration::from_secs(30)).await.unwrap();
    assert_eq!(answer.body.len(), 64 * 1024);
}

/// ❗ This crate builds reqwest with `http2` itself, so its own tests and the
/// live suite negotiate HTTP/2 the way the app does (the app gets the feature
/// through `genai` anyway). `http2_prior_knowledge` exists only with the
/// feature, so dropping it from `Cargo.toml` fails this file to compile.
#[test]
fn the_client_is_built_with_http2() {
    assert!(cmdr_http::client_builder().http2_prior_knowledge().build().is_ok());
}
