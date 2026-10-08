//! The built client end to end: a fake proxy and a fake origin on loopback, and which one each
//! request reaches.

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

/// A one-route system layer, so the test doesn't depend on this Mac's settings.
struct FixedSystem(Route);

impl SystemProxies for FixedSystem {
    fn route(&self, _url: &Url) -> Route {
        self.0.clone()
    }
}

/// An HTTP/1.1 server on `127.0.0.1` that answers every request with `name` as the body and
/// records each request line.
async fn server(name: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a loopback port");
    let address = listener.local_addr().expect("a bound address");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let log = log.clone();
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && socket.read(&mut byte).await.unwrap_or(0) == 1 {
                    head.push(byte[0]);
                }
                let line = String::from_utf8_lossy(&head)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                log.lock().expect("an unpoisoned test log").push(line);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{name}",
                    name.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    (format!("http://{address}"), seen)
}

/// One SOCKS5 `CONNECT` the fake proxy saw.
#[derive(Debug, PartialEq, Eq)]
struct SocksAsk {
    /// The RFC 1929 username and password, when the client signed in.
    credentials: Option<(String, String)>,
    /// `name host:port` when the client sent a name for the proxy to resolve, `ip addr:port`
    /// when it resolved the name itself.
    target: String,
}

async fn read_exact<const N: usize>(socket: &mut tokio::net::TcpStream) -> std::io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    socket.read_exact(&mut bytes).await?;
    Ok(bytes)
}

async fn read_string(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
    let [len] = read_exact::<1>(socket).await?;
    let mut bytes = vec![0u8; usize::from(len)];
    socket.read_exact(&mut bytes).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The SOCKS5 handshake (RFC 1928, with RFC 1929 sign-in when offered), up to the `CONNECT`.
async fn socks5_handshake(socket: &mut tokio::net::TcpStream) -> std::io::Result<SocksAsk> {
    let [_version, count] = read_exact::<2>(socket).await?;
    let mut methods = vec![0u8; usize::from(count)];
    socket.read_exact(&mut methods).await?;
    let sign_in = methods.contains(&2);
    socket.write_all(&[5, if sign_in { 2 } else { 0 }]).await?;
    let credentials = if sign_in {
        let [_auth_version] = read_exact::<1>(socket).await?;
        let user = read_string(socket).await?;
        let password = read_string(socket).await?;
        socket.write_all(&[1, 0]).await?;
        Some((user, password))
    } else {
        None
    };
    let [_version, _command, _reserved, kind] = read_exact::<4>(socket).await?;
    let host = match kind {
        1 => format!("ip {}", std::net::Ipv4Addr::from(read_exact::<4>(socket).await?)),
        3 => format!("name {}", read_string(socket).await?),
        _ => format!("ip [{}]", std::net::Ipv6Addr::from(read_exact::<16>(socket).await?)),
    };
    let port = u16::from_be_bytes(read_exact::<2>(socket).await?);
    socket.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    Ok(SocksAsk {
        credentials,
        target: format!("{host}:{port}"),
    })
}

/// A SOCKS5 proxy on `127.0.0.1` that records each `CONNECT` and then answers the tunneled HTTP
/// request itself with `via socks`, standing in for the origin.
async fn socks_server() -> (String, Arc<Mutex<Vec<SocksAsk>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a loopback port");
    let address = listener.local_addr().expect("a bound address");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let log = log.clone();
            tokio::spawn(async move {
                let Ok(ask) = socks5_handshake(&mut socket).await else {
                    return;
                };
                log.lock().expect("an unpoisoned test log").push(ask);
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && socket.read(&mut byte).await.unwrap_or(0) == 1 {
                    head.push(byte[0]);
                }
                let body = "via socks";
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    (address.to_string(), seen)
}

fn env(vars: &[(&str, &str)]) -> Arc<EnvProxies> {
    Arc::new(EnvProxies::from_vars(|name| {
        vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string())
    }))
}

async fn body(client: &reqwest::Client, url: &str) -> Result<String, reqwest::Error> {
    client.get(url).send().await?.text().await
}

#[tokio::test]
async fn loopback_goes_direct_even_with_a_proxy_set() {
    let (proxy, proxy_seen) = server("proxy").await;
    let (origin, _) = server("origin").await;
    let client = builder_with(
        env(&[("HTTP_PROXY", &proxy)]),
        Arc::new(FixedSystem(Route::Proxy(proxy.clone()))),
    )
    .build()
    .expect("a client");

    assert_eq!(
        body(&client, &format!("{origin}/health"))
            .await
            .expect("a direct answer"),
        "origin"
    );
    assert!(proxy_seen.lock().expect("an unpoisoned test log").is_empty());
}

#[tokio::test]
async fn other_hosts_go_through_the_environment_proxy() {
    let (proxy, proxy_seen) = server("proxy").await;
    let client = builder_with(env(&[("HTTP_PROXY", &proxy)]), Arc::new(FixedSystem(Route::Direct)))
        .build()
        .expect("a client");

    assert_eq!(
        body(&client, "http://cmdr.invalid/x")
            .await
            .expect("the proxy's answer"),
        "proxy"
    );
    assert_eq!(
        *proxy_seen.lock().expect("an unpoisoned test log"),
        ["GET http://cmdr.invalid/x HTTP/1.1"]
    );
}

#[tokio::test]
async fn the_system_proxy_carries_what_the_environment_leaves() {
    let (proxy, proxy_seen) = server("proxy").await;
    let client = builder_with(env(&[]), Arc::new(FixedSystem(Route::Proxy(proxy.clone()))))
        .build()
        .expect("a client");

    assert_eq!(
        body(&client, "http://cmdr.invalid/y")
            .await
            .expect("the proxy's answer"),
        "proxy"
    );
    assert_eq!(
        *proxy_seen.lock().expect("an unpoisoned test log"),
        ["GET http://cmdr.invalid/y HTTP/1.1"]
    );
}

#[tokio::test]
async fn a_system_socks_proxy_carries_the_request_and_resolves_the_name() {
    let (socks, seen) = socks_server().await;
    let client = builder_with(
        env(&[]),
        Arc::new(FixedSystem(Route::Proxy(format!("socks5h://{socks}")))),
    )
    .build()
    .expect("a client");

    // `.invalid` never resolves locally, so an answer at all means the proxy got the name.
    assert_eq!(
        body(&client, "http://cmdr.invalid/s")
            .await
            .expect("the answer through the tunnel"),
        "via socks"
    );
    assert_eq!(
        *seen.lock().expect("an unpoisoned test log"),
        [SocksAsk {
            credentials: None,
            target: "name cmdr.invalid:80".into(),
        }]
    );
}

#[tokio::test]
async fn an_environment_socks_proxy_signs_in_with_its_credentials() {
    let (socks, seen) = socks_server().await;
    let client = builder_with(
        env(&[("ALL_PROXY", &format!("socks5h://ada:p%40ss%20word@{socks}"))]),
        Arc::new(FixedSystem(Route::Direct)),
    )
    .build()
    .expect("a client");

    assert_eq!(
        body(&client, "http://cmdr.invalid/t")
            .await
            .expect("the answer through the tunnel"),
        "via socks"
    );
    assert_eq!(
        *seen.lock().expect("an unpoisoned test log"),
        [SocksAsk {
            credentials: Some(("ada".into(), "p@ss word".into())),
            target: "name cmdr.invalid:80".into(),
        }]
    );
}

#[tokio::test]
async fn no_proxy_sends_a_host_direct_past_both_proxies() {
    let (proxy, proxy_seen) = server("proxy").await;
    let client = builder_with(
        env(&[("HTTP_PROXY", &proxy), ("NO_PROXY", "cmdr.invalid")]),
        Arc::new(FixedSystem(Route::Proxy(proxy.clone()))),
    )
    .build()
    .expect("a client");

    // Direct to a `.invalid` host can't resolve, which is the point: it never reached the proxy.
    assert!(body(&client, "http://cmdr.invalid/z").await.is_err());
    assert!(proxy_seen.lock().expect("an unpoisoned test log").is_empty());
}
