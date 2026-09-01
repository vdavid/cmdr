use std::net::{Ipv4Addr, SocketAddr};

use super::*;

#[test]
fn default_local_dials_loopback() {
    let endpoint = AdbEndpoint::default_local();
    assert_eq!(endpoint.addr().ip(), Ipv4Addr::LOCALHOST);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fixed_endpoint_never_starts_a_server() {
    // Bind then drop a listener so the port is known-closed.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    drop(listener);
    let err = AdbEndpoint::at(addr).connect().await.unwrap_err();
    assert!(matches!(err, AdbConnectError::ServerUnreachable(_)), "{err:?}");
}

#[test]
fn locating_the_binary_never_panics() {
    let _ = locate_adb_binary();
}
