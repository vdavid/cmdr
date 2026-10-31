//! Builds a fresh smb2 session ([`build_session`]) and names the one it
//! replaces ([`retired_session_id`]). Every session the volume opens comes from
//! here: the main one (`session.rs`, `reconnect.rs`), the scan pool's members,
//! and the transfer session's. A leaf, so those owners never import each other
//! for it.

use super::SmbConnectionParams;
use smb2::client::tree::Tree;
use smb2::types::SessionId;
use smb2::{ClientConfig, SmbClient};
use std::time::Duration;

/// The session id a replacement client names as its `PreviousSessionId`
/// (#581), or `None` while `client`'s connection is still up.
///
/// The server ends a named session even mid-request, and a request on it
/// afterwards fails as `SignatureInvalid`, which reads as a security refusal.
/// So a session is named only once smb2 has torn its connection down: with
/// `auto_reconnect: false` nothing can revive it, so nobody is still using it.
/// One whose connection is up (a watcher death the main session didn't share)
/// closes itself when its last clone drops.
pub(super) fn retired_session_id(client: &SmbClient) -> Option<SessionId> {
    client.connection().is_disconnected().then(|| client.session_id())
}

/// Builds a fresh smb2 session using the given params. Returns the connected
/// client + tree on success.
///
/// `replaces` names the session this one stands in for
/// ([`retired_session_id`]), so the server ends it, closing its handles, before
/// the new one is up. ❌ Never a sibling's: each pool member or watcher names
/// only its own predecessor.
pub(super) async fn build_session(
    params: &SmbConnectionParams,
    replaces: Option<SessionId>,
) -> Result<(SmbClient, Tree), smb2::Error> {
    use crate::build_smb_addr;

    let config = ClientConfig {
        addr: build_smb_addr(&params.server, params.port),
        timeout: Duration::from_secs(10),
        username: params.username.clone(),
        password: params.password.clone(),
        domain: String::new(),
        auto_reconnect: false,
        compression: true,
        // A share name that isn't a share on the server may be a DFS namespace
        // root, which is how an AD domain normally publishes a share
        // (`\\<domain>\<namespace>`). smb2 only asks for a referral once a
        // TreeConnect has been refused by a server that advertised DFS, so an
        // ordinary NAS pays nothing for this being on.
        dfs_enabled: true,
        dfs_target_overrides: dfs_target_overrides(),
        dfs_target_policy: crate::link_consent::dfs_target_policy(),
        // Safe because Cmdr asks for no durable handles: on Windows 8.1+ this
        // also closes the old session's durable opens, which a client meaning to
        // reclaim one must keep.
        previous_session_id: replaces,
        ..Default::default()
    };
    let mut client = crate::connection::connect_volume_client(config).await?;
    let tree = client.connect_share(&params.share_name).await?;
    Ok((client, tree))
}

/// Where a DFS referral's server names actually answer: always nothing in a
/// shipped build. The DFS suite (`dfs_links_integration_test.rs`) points smb2's
/// `smb-dfs-target` container name at its port-mapped loopback address, which
/// a referral can't know.
#[cfg(not(any(test, feature = "testing")))]
fn dfs_target_overrides() -> std::collections::HashMap<String, String> {
    std::collections::HashMap::new()
}

#[cfg(any(test, feature = "testing"))]
pub(super) static TEST_DFS_TARGET_OVERRIDES: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
    std::sync::Mutex::new(None);

#[cfg(any(test, feature = "testing"))]
fn dfs_target_overrides() -> std::collections::HashMap<String, String> {
    use cmdr_fs::ignore_poison::IgnorePoison;
    TEST_DFS_TARGET_OVERRIDES
        .lock_ignore_poison()
        .clone()
        .unwrap_or_default()
}
