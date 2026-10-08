//! What the client hands back, against a one-shot local server.

use std::time::Duration;

use reqwest::Method;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use super::{Depth, PropfindOutcome, WebdavClient};

/// `cmdr stores these bytes verbatim\n` gzipped (`gzip -9n`): a file a server
/// hands out with `Content-Encoding: gzip`, as one serving `.gz` files or
/// compressing on the fly can.
const STORED_GZIP: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0xce, 0x4d, 0x29, 0x52, 0x28, 0x2e, 0xc9, 0x2f,
    0x4a, 0x2d, 0x56, 0x28, 0xc9, 0x48, 0x2d, 0x4e, 0x55, 0x48, 0xaa, 0x2c, 0x01, 0xb2, 0xcb, 0x52, 0x8b, 0x92, 0x12,
    0x4b, 0x32, 0x73, 0xb9, 0x00, 0xce, 0xed, 0x88, 0x3e, 0x21, 0x00, 0x00, 0x00,
];

/// A server answering `count` requests with the gzip body and its headers
/// (no body for a HEAD), and a client pointed at it.
async fn serving_stored_gzip(count: usize) -> (WebdavClient, Url) {
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
                 connection: close\r\n\r\n",
                STORED_GZIP.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            if !buffer.starts_with(b"HEAD") {
                let _ = socket.write_all(STORED_GZIP).await;
            }
            let _ = socket.shutdown().await;
        }
    });
    let base = Url::parse(&format!("http://{addr}/dav/")).unwrap();
    (
        WebdavClient::new(base.clone(), "user", "secret").unwrap(),
        base.join("page.html").unwrap(),
    )
}

/// ❗ A file manager copies bytes, it doesn't decode them: a file served with
/// `Content-Encoding: gzip` reads back as the bytes the server holds, at the
/// length its `Content-Length` says. The test build has every reqwest decoder
/// on (`Cargo.toml`'s dev-dependencies), as the app's graph may: the client
/// must turn each one off itself.
#[tokio::test]
async fn an_encoded_file_reads_back_as_its_stored_bytes() {
    let (client, url) = serving_stored_gzip(2).await;

    let get = client.send(client.request(Method::GET, url.clone())).await.unwrap();
    let body = tokio::time::timeout(Duration::from_secs(5), get.bytes())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body.as_ref(), STORED_GZIP, "a GET");

    let head = client.send(client.request(Method::HEAD, url)).await.unwrap();
    assert_eq!(
        head.headers().get("content-length").and_then(|v| v.to_str().ok()),
        Some(STORED_GZIP.len().to_string().as_str()),
        "a HEAD keeps the length"
    );
}

/// A server answering one request with a 207 whose body is `size` bytes of a
/// valid-looking `multistatus`, framed by `Content-Length` or chunked.
async fn serving_a_207_of(size: usize, chunked: bool) -> (WebdavClient, Url) {
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
        let mut body = b"<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\"><!--".to_vec();
        body.resize(size.saturating_sub(20), b'x');
        body.extend_from_slice(b"--></d:multistatus>");
        let framing = if chunked {
            "transfer-encoding: chunked".to_string()
        } else {
            format!("content-length: {}", body.len())
        };
        let head = format!(
            "HTTP/1.1 207 Multi-Status\r\ncontent-type: application/xml\r\n{framing}\r\nconnection: close\r\n\r\n"
        );
        let _ = socket.write_all(head.as_bytes()).await;
        if chunked {
            for piece in body.chunks(1024) {
                let _ = socket.write_all(format!("{:x}\r\n", piece.len()).as_bytes()).await;
                let _ = socket.write_all(piece).await;
                let _ = socket.write_all(b"\r\n").await;
            }
            let _ = socket.write_all(b"0\r\n\r\n").await;
        } else {
            let _ = socket.write_all(&body).await;
        }
        let _ = socket.shutdown().await;
    });
    let base = Url::parse(&format!("http://{addr}/dav/")).unwrap();
    (WebdavClient::new(base.clone(), "user", "secret").unwrap(), base)
}

/// ❗ A hostile or broken server can stream a listing forever: the body is
/// capped, whether it announces its length or streams chunks, and a body over
/// the cap is a typed `TooLarge`, never a growing buffer.
#[tokio::test]
async fn a_propfind_body_past_the_cap_is_refused() {
    for chunked in [false, true] {
        let (client, url) = serving_a_207_of(64 * 1024, chunked).await;
        let outcome = client.propfind_within(url, Depth::One, 16 * 1024).await.unwrap();
        assert!(
            matches!(outcome, PropfindOutcome::TooLarge { limit: 16_384 }),
            "chunked={chunked}: a 64 KiB body over a 16 KiB cap must be refused"
        );
    }
}

#[tokio::test]
async fn a_propfind_body_under_the_cap_is_read() {
    let (client, url) = serving_a_207_of(8 * 1024, true).await;
    let outcome = client.propfind_within(url, Depth::One, 16 * 1024).await.unwrap();
    assert!(!matches!(outcome, PropfindOutcome::TooLarge { .. }));
}

/// ❗ This crate builds reqwest with `http2` itself, so its own tests speak
/// what the app negotiates (the app gets the feature through `genai`).
/// `http2_prior_knowledge` exists only with the feature, so dropping it from
/// `Cargo.toml` fails this file to compile.
#[test]
fn the_client_is_built_with_http2() {
    assert!(cmdr_http::client_builder().http2_prior_knowledge().build().is_ok());
}
