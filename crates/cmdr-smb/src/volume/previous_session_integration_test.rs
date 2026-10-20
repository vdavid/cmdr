//! A reconnect names the session it replaces (`PreviousSessionId`, #581), so the
//! server ends it, closing what it held open, before the new one is up.
//!
//! The drop is the hard kind: the network goes quiet with no FIN
//! ([`BlackHoleProxy`]), so the server keeps the old session for as long as it
//! believes the client is only quiet. Without the id, a temp the old session
//! held open stays locked for minutes, and deleting it meets
//! `STATUS_SHARING_VIOLATION`.
//!
//! Runs against `smb-consumer-auth` (share `private`, `testuser` / `testpass`):
//! Samba ignores a previous id on a guest login, so only a named account shows
//! it. `#[ignore]`d like every `smb_integration_` test; start the containers with
//! `apps/desktop/test/smb-servers/start.sh`.

use super::black_hole_proxy::BlackHoleProxy;
use super::test_support::*;
use super::*;

const MOUNT_ROOT: &str = "/tmp/smb-previous-session-mount";

/// The host port the `smb-consumer-auth` container publishes (smb2's default
/// 10481; Cmdr's stack moves it, and the check runner exports the override).
fn auth_port() -> u16 {
    std::env::var("SMB_CONSUMER_AUTH_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10481)
}

async fn auth_volume_through(port: u16) -> SmbVolume {
    let params = SmbConnectionParams::new("127.0.0.1", "private", port, Some("testuser"), Some("testpass"));
    let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "private");
    connect_smb_volume(
        "private",
        MountAnchor::at_share_root(MOUNT_ROOT),
        &volume_id,
        params,
        VolumeHost::detached(),
    )
    .await
    .unwrap_or_else(|e| {
        panic!("Failed to connect to the auth SMB share through 127.0.0.1:{port}. Is it running? ({e:?})")
    })
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_temp_the_dropped_session_held_open_is_deletable_right_after_the_reconnect() {
    let proxy = BlackHoleProxy::start(auth_port()).await;
    let vol = auth_volume_through(proxy.port()).await;
    let name = format!("{}.tmp", test_dir_name());

    // The old session opens a temp and keeps it open, the way a copy's staged
    // writer does when the network goes mid-file.
    let lease = vol.clone_session().await.expect("a connected share hands out a lease");
    let old_conn = lease.conn.clone();
    let mut writer = lease
        .tree
        .create_file_writer(lease.conn, &name)
        .await
        .expect("opening the temp on the old session");
    writer.write_chunk(b"half a file").await.expect("writing into the temp");

    // The network goes quiet: the server never hears the old session close.
    // smb2 declares it dead (here at once; on a real drop, once its liveness
    // budget runs out), and the share reads `Disconnected`.
    proxy.black_hole();
    old_conn.mark_dead();
    vol.inner.transition_to_disconnected(vol.inner.state.generation());

    vol.inner
        .do_attempt_reconnect()
        .await
        .expect("the reconnect comes up through the proxy");
    assert_eq!(vol.session_state(), ConnectionState::Direct);

    let deleted = vol.delete_impl(&PathBuf::from(MOUNT_ROOT).join(&name)).await;
    assert!(
        deleted.is_ok(),
        "the temp the dropped session held open must go on the first try after the reconnect, got {deleted:?}"
    );
    drop(writer);
}

/// ❗ Only a session smb2 has torn down is named. The server ends a named
/// session even mid-request, and every request on it after that fails as
/// `SignatureInvalid`, which the app logs as a security refusal. A reconnect
/// over a session whose connection is still up (a watcher death the main
/// session didn't share) must leave the work still riding it alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_reconnect_over_a_session_still_up_leaves_its_work_running() {
    let vol = auth_volume_through(auth_port()).await;
    let mut lease = vol.clone_session().await.expect("a connected share hands out a lease");

    vol.inner.transition_to_disconnected(vol.inner.state.generation());
    vol.inner.do_attempt_reconnect().await.expect("the reconnect comes up");

    let listed = lease.tree.list_directory(&mut lease.conn, "").await;
    assert!(
        listed.is_ok(),
        "a call on the replaced session, still up, must keep working, got {listed:?}"
    );
}
