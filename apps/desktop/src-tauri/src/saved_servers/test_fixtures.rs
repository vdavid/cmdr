//! Saved entries the servers suites share (`listing_test.rs` here, and
//! `commands/servers_test.rs` over the commands).

use super::SavedServer;
use crate::network::known_shares;
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::webdav_known_servers::KnownWebdavServer;

/// The listing over manual entries alone, with no share history and nothing
/// discovered: the stores come in as arguments, so no cell races another over
/// the process-global copies.
pub(crate) fn listing_of(manual: Vec<crate::network::manual_servers::ManualServerEntry>) -> Vec<SavedServer> {
    super::listing(manual, Vec::new(), &[])
}

/// A host nobody else's cell uses, so these suites can share the process-global
/// stores with whatever runs beside them.
///
/// ❗ **These suites own `192.0.2.x` and `server_volumes_test.rs` owns
/// `198.51.100.x`, one reserved block each.** Both write the same process-global
/// `KNOWN` store, so under a thread-per-test runner (`cargo test --lib`, which
/// nextest's process-per-test hides) a shared address is one cell removing the
/// entry another is about to look up, or writing it with the other pin. Pick a
/// free last octet inside your OWN block; a number that merely looks unused
/// across both files is how they collided.
///
/// Both blocks are reserved for documentation (RFC 5737) and routed nowhere.
pub(crate) fn sftp_entry(host: &str, pinned: bool) -> KnownSftpServer {
    KnownSftpServer {
        host: host.to_string(),
        port: 2222,
        username: "test_user_1".to_string(),
        display_name: format!("{host} over ssh"),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
        pinned,
        last_connected_at: "2026-09-01T00:00:00Z".to_string(),
    }
}

pub(crate) fn webdav_entry(host: &str, pinned: bool) -> KnownWebdavServer {
    KnownWebdavServer {
        url: format!("http://{host}:8080/dav/"),
        username: "test_user_1".to_string(),
        display_name: format!("{host} over dav"),
        remote_root: "/Photos".to_string(),
        start_folder: None,
        auto_reconnect: true,
        pinned,
        last_connected_at: "2026-09-02T00:00:00Z".to_string(),
    }
}

pub(crate) fn find<'a>(servers: &'a [SavedServer], display_name: &str) -> &'a SavedServer {
    servers
        .iter()
        .find(|s| s.display_name == display_name)
        .unwrap_or_else(|| panic!("{display_name} must be in the listing"))
}

/// A saved share row, as a mount through Cmdr files it.
pub(crate) fn share_row(
    server_name: &str,
    share: &str,
    username: Option<&str>,
    volume_id: Option<&str>,
) -> known_shares::KnownNetworkShare {
    known_shares::KnownNetworkShare {
        server_name: server_name.to_string(),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-09-24T10:00:00Z".to_string(),
        last_connection_mode: known_shares::ConnectionMode::Credentials,
        last_known_auth_options: known_shares::AuthOptions::GuestOrCredentials,
        username: username.map(str::to_string),
        address: volume_id.map(|_| server_name.to_string()),
        port: None,
        volume_id: volume_id.map(str::to_string),
        mount_path: volume_id.map(|_| format!("/Volumes/{share}")),
        pinned: volume_id.is_some(),
        server_guid: None,
        server_guid_proof: crate::network::server_guid::GuidProof::Unauthenticated,
    }
}
