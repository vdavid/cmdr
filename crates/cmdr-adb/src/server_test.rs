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

#[test]
fn a_configured_binary_path_wins_over_everything_the_environment_offers() {
    let dir = cmdr_fs::testing::TestDir::new("adb_binary_override");
    let fake_adb = dir.join("adb");
    std::fs::write(&fake_adb, "#!/bin/sh\nexit 0\n").expect("write the fake binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_adb, std::fs::Permissions::from_mode(0o755)).expect("make it executable");
    }

    set_adb_binary_override(Some(fake_adb.clone()));
    assert_eq!(
        locate_adb_binary(),
        Some(fake_adb),
        "the person told us where their adb is, so nothing on PATH gets a say"
    );

    set_adb_binary_override(None);
}

#[test]
fn a_configured_path_that_isnt_runnable_falls_back_to_the_search() {
    let dir = cmdr_fs::testing::TestDir::new("adb_binary_override_stale");
    let missing = dir.join("not-here/adb");

    set_adb_binary_override(Some(missing.clone()));
    assert_ne!(
        locate_adb_binary(),
        Some(missing),
        "a stale path in Settings must not make every device vanish"
    );

    set_adb_binary_override(None);
}
