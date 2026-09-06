//! Which server a remote path belongs to, and the one thing asking must never
//! do: connect.

use std::time::Duration;

use super::*;
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::webdav_known_servers::KnownWebdavServer;

/// A host nobody else's cell will use, so this suite can share the
/// process-global store with whatever runs beside it.
///
/// ❗ `192.0.2.x` is reserved for documentation (RFC 5737) and routed nowhere, so
/// a dial against it hangs rather than reaching a stranger's machine. That hang
/// is what makes "the resolver never connects" assertable.
fn saved_sftp(host: &str, pinned: bool) -> KnownSftpServer {
    KnownSftpServer {
        host: host.to_string(),
        port: 2222,
        username: "ada".to_string(),
        display_name: format!("{host} server"),
        remote_root: "/srv/data".to_string(),
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
        pinned,
        last_connected_at: "2026-09-06T00:00:00Z".to_string(),
    }
}

#[test]
fn a_pinned_saved_server_answers_for_its_own_paths_without_connecting() {
    let host = "192.0.2.11";
    sftp_known_servers::remember(saved_sftp(host, true));

    let started = std::time::Instant::now();
    let volume = server_volume_for_path(&format!("sftp://ada@{host}:2222/srv/data/photos"))
        .expect("a saved server answers for a path under its own root");

    assert_eq!(volume.id, cmdr_fs::volume::sftp_volume_id(host, 2222, "ada"));
    assert_eq!(volume.path, format!("sftp://ada@{host}:2222/srv/data"));
    assert_eq!(volume.fs_type.as_deref(), Some("sftp"));
    assert_eq!(volume.category, LocationCategory::Network);
    assert_eq!(
        volume.connection_state,
        Some(ConnectionState::Saved),
        "nothing is registered under that id, so the row is the greyed one"
    );
    assert!(!volume.is_ejectable, "a server has nothing to unplug");
    assert!(!volume.supports_trash);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "❗ resolving must never dial: this address is routed nowhere, so a connect would hang"
    );
}

/// ❗ Pins govern the SWITCHER, not identity. A tab restored onto an unpinned
/// server still has to find its way home.
#[test]
fn an_unpinned_saved_server_still_answers_for_its_paths() {
    let host = "192.0.2.12";
    sftp_known_servers::remember(saved_sftp(host, false));

    let volume = server_volume_for_path(&format!("sftp://ada@{host}:2222/srv/data"))
        .expect("an unpinned server is still where its paths live");

    assert_eq!(volume.id, cmdr_fs::volume::sftp_volume_id(host, 2222, "ada"));
    assert_eq!(volume.connection_state, Some(ConnectionState::Saved));
}

#[test]
fn a_saved_webdav_server_answers_for_its_own_paths() {
    let host = "192.0.2.13";
    webdav_known_servers::remember(KnownWebdavServer {
        url: format!("http://{host}:8080/dav/"),
        username: "ada".to_string(),
        display_name: "Docs".to_string(),
        remote_root: "/Photos".to_string(),
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-06T00:00:00Z".to_string(),
    });

    let volume = server_volume_for_path(&format!("webdav://ada@{host}:8080/Photos/2026"))
        .expect("a saved WebDAV server answers for a path under its own root");

    assert_eq!(volume.id, cmdr_fs::volume::webdav_volume_id(host, 8080, "ada"));
    assert_eq!(volume.path, format!("webdav://ada@{host}:8080/Photos"));
    assert_eq!(volume.fs_type.as_deref(), Some("webdav"));
}

/// A prefix nothing saved matches is `None`, ❌ never the boot disk: that is the
/// whole reason a remote path carries a scheme.
#[test]
fn an_unknown_prefix_is_nobodys_volume() {
    assert!(server_volume_for_path("sftp://nobody@192.0.2.99:22/srv/data").is_none());
    assert!(server_volume_for_path("webdav://nobody@192.0.2.99:80/dav").is_none());
}

/// The trap a raw string prefix compare falls into: a sibling root whose name
/// merely starts with a saved one's.
#[test]
fn a_sibling_root_is_not_this_servers_path() {
    let host = "192.0.2.14";
    sftp_known_servers::remember(saved_sftp(host, true));

    assert!(
        server_volume_for_path(&format!("sftp://ada@{host}:2222/srv/data-1/photos")).is_none(),
        "`/srv/data-1` is a different tree, however much of `/srv/data` it spells"
    );
}
