//! A saved server moving to a new address, through the command the edit sheet
//! calls: what goes with it, and what refuses. The move itself:
//! `network/server_move.rs`.
//!
//! ❗ **This suite owns `203.0.113.x`** (RFC 5737, routed nowhere), beside
//! `servers_test.rs`'s `192.0.2.x` and `server_volumes_test.rs`'s `198.51.100.x`:
//! the stores are process-global, so a shared address is two cells racing one entry.

use super::*;
use crate::commands::sftp::{has_sftp_credentials, save_sftp_credentials};
use crate::commands::webdav::{has_webdav_credentials, save_webdav_credentials};
use crate::favorites::store::{self as favorites, FavoriteVolume};
use crate::network::sftp_known_servers::KnownSftpServer;
use crate::network::webdav_known_servers::KnownWebdavServer;

fn sftp_entry(host: &str, port: u16) -> KnownSftpServer {
    KnownSftpServer {
        host: host.to_string(),
        port,
        username: "ada".to_string(),
        display_name: "Naspolya".to_string(),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-01T00:00:00Z".to_string(),
    }
}

fn sftp_target(host: &str, port: u16, username: &str) -> ServerTarget {
    ServerTarget::Sftp {
        display_name: "Naspolya".to_string(),
        host: host.to_string(),
        port,
        username: username.to_string(),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
    }
}

fn webdav_entry(url: &str) -> KnownWebdavServer {
    KnownWebdavServer {
        url: url.to_string(),
        username: "ada".to_string(),
        display_name: "Cloud".to_string(),
        remote_root: "/Photos".to_string(),
        start_folder: None,
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-02T00:00:00Z".to_string(),
    }
}

fn webdav_target(url: &str) -> ServerTarget {
    ServerTarget::Webdav {
        display_name: "Cloud".to_string(),
        url: url.to_string(),
        username: "ada".to_string(),
        remote_root: "/Photos".to_string(),
        start_folder: None,
        auto_reconnect: true,
    }
}

fn sftp_id(host: &str, port: u16) -> String {
    cmdr_fs::volume::sftp_volume_id(host, port, "ada")
}

#[tokio::test]
async fn moving_an_sftp_server_takes_its_entry_password_and_favorites_to_the_new_address() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old_host, new_host) = ("203.0.113.10", "203.0.113.11");
    sftp_known_servers::remember(sftp_entry(old_host, 22));
    save_sftp_credentials(old_host.to_string(), 22, "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");
    let old_prefix = cmdr_fs::volume::sftp_app_root(old_host, 22, "ada");
    let new_prefix = cmdr_fs::volume::sftp_app_root(new_host, 2200, "ada");
    favorites::add(
        &format!("{old_prefix}/srv/data/photos"),
        Some("Photos".to_string()),
        FavoriteVolume {
            id: sftp_id(old_host, 22),
            root: format!("{old_prefix}/srv/data"),
            name: "Naspolya".to_string(),
        },
    );

    let outcome = update_saved_server(sftp_target(new_host, 2200, "ada"), Some(sftp_id(old_host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    assert!(
        sftp_known_servers::find(old_host, 22, "ada").is_none(),
        "the old address is gone"
    );
    let moved = sftp_known_servers::find(new_host, 2200, "ada").expect("saved at the new address");
    assert!(moved.pinned, "the same server keeps its pin");
    assert!(
        has_sftp_credentials(new_host.to_string(), 2200, "ada".to_string()).await,
        "the password moved with it"
    );
    assert!(
        !has_sftp_credentials(old_host.to_string(), 22, "ada".to_string()).await,
        "and left nothing behind under the old address"
    );
    let favorite = favorites::list()
        .into_iter()
        .find(|f| f.name == "Photos")
        .expect("the favorite stays");
    assert_eq!(favorite.path, format!("{new_prefix}/srv/data/photos"));
    assert_eq!(favorite.volume.map(|v| v.id), Some(sftp_id(new_host, 2200)));
    let announced = crate::volume_broadcast::server_moves_from(&old_prefix);
    assert_eq!(announced.len(), 1, "the panes hear about it once");
    assert_eq!(announced[0].new_prefix, new_prefix);
    assert_eq!(announced[0].places[0].old_volume_id, sftp_id(old_host, 22));
    assert_eq!(announced[0].places[0].new_volume_id, sftp_id(new_host, 2200));
    assert_eq!(announced[0].places[0].new_root, format!("{new_prefix}/srv/data"));
}

/// ❗ Refused rather than merged, and refused BEFORE the password moves: a copy
/// under the other server's address would overwrite that server's own.
#[tokio::test]
async fn moving_onto_an_address_another_saved_server_holds_is_refused_and_moves_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old_host, taken_host) = ("203.0.113.20", "203.0.113.21");
    sftp_known_servers::remember(sftp_entry(old_host, 22));
    sftp_known_servers::remember(KnownSftpServer {
        display_name: "The other NAS".to_string(),
        ..sftp_entry(taken_host, 22)
    });
    save_sftp_credentials(old_host.to_string(), 22, "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");

    let outcome = update_saved_server(sftp_target(taken_host, 22, "ada"), Some(sftp_id(old_host, 22))).await;

    assert_eq!(
        outcome,
        SavedServerOutcome::AddressTaken {
            name: "The other NAS".to_string()
        }
    );
    assert!(
        sftp_known_servers::find(old_host, 22, "ada").is_some(),
        "the edited server stays put"
    );
    assert!(has_sftp_credentials(old_host.to_string(), 22, "ada".to_string()).await);
    assert!(
        !has_sftp_credentials(taken_host.to_string(), 22, "ada".to_string()).await,
        "the other server's address got no password"
    );
    let old_prefix = cmdr_fs::volume::sftp_app_root(old_host, 22, "ada");
    assert!(crate::volume_broadcast::server_moves_from(&old_prefix).is_empty());
}

/// An edit that keeps the address is the edit it always was: no move, no event.
#[tokio::test]
async fn an_edit_that_keeps_the_address_saves_in_place_and_announces_no_move() {
    let host = "203.0.113.30";
    sftp_known_servers::remember(sftp_entry(host, 22));
    let renamed = ServerTarget::Sftp {
        display_name: "Renamed".to_string(),
        host: host.to_uppercase(),
        port: 22,
        username: "ada".to_string(),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
    };

    let outcome = update_saved_server(renamed, Some(sftp_id(host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    assert_eq!(
        sftp_known_servers::find(host, 22, "ada").map(|s| s.display_name),
        Some("Renamed".to_string())
    );
    let prefix = cmdr_fs::volume::sftp_app_root(host, 22, "ada");
    assert!(crate::volume_broadcast::server_moves_from(&prefix).is_empty());
}

/// Another account is another place: the sheet locks the field, and the backend
/// refuses rather than quietly moving one account's settings onto another.
#[tokio::test]
async fn an_edit_naming_another_account_is_refused() {
    let host = "203.0.113.40";
    sftp_known_servers::remember(sftp_entry(host, 22));

    let outcome = update_saved_server(sftp_target(host, 22, "grace"), Some(sftp_id(host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::AccountChanged);
    assert!(sftp_known_servers::find(host, 22, "grace").is_none());
    assert!(sftp_known_servers::find(host, 22, "ada").is_some());
}

/// A Forget in another pane between opening the sheet and saving it: the save
/// has nothing to edit, and ❌ never quietly re-saves the server.
#[tokio::test]
async fn editing_a_server_nobody_saved_any_more_is_refused() {
    let host = "203.0.113.50";

    let outcome = update_saved_server(sftp_target(host, 22, "ada"), Some(sftp_id(host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::Unreachable);
    assert!(sftp_known_servers::find(host, 22, "ada").is_none());
}

/// A WebDAV base URL can move without its host, port, or account (a Nextcloud
/// under a new path): the id stays, the store and the session still move.
#[tokio::test]
async fn moving_a_webdav_server_to_a_new_path_keeps_its_id_and_its_password() {
    let _secrets = crate::test_support::isolate_secrets();
    let old_url = "https://203.0.113.60/remote.php/dav/files/ada/";
    let new_url = "https://203.0.113.60/dav/";
    webdav_known_servers::remember(webdav_entry(old_url));
    save_webdav_credentials(old_url.to_string(), "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");
    let id = cmdr_fs::volume::webdav_volume_id("203.0.113.60", 443, "ada");

    let outcome = update_saved_server(webdav_target(new_url), Some(id.clone())).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    assert!(webdav_known_servers::find(old_url, "ada").is_none());
    assert!(webdav_known_servers::find(new_url, "ada").is_some_and(|s| s.pinned));
    assert!(has_webdav_credentials(new_url.to_string(), "ada".to_string()).await);
    let prefix = cmdr_fs::volume::webdav_app_root("203.0.113.60", 443, "ada");
    let announced = crate::volume_broadcast::server_moves_from(&prefix);
    assert_eq!(announced.len(), 1, "the session at the old URL still has to go");
    assert_eq!(announced[0].places[0].old_volume_id, id);
    assert_eq!(announced[0].places[0].new_volume_id, id);
}

#[tokio::test]
async fn moving_a_webdav_server_to_a_new_host_takes_its_password_along() {
    let _secrets = crate::test_support::isolate_secrets();
    let old_url = "http://203.0.113.70:8080/dav/";
    let new_url = "https://203.0.113.71/dav/";
    webdav_known_servers::remember(webdav_entry(old_url));
    save_webdav_credentials(old_url.to_string(), "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");
    let old_id = cmdr_fs::volume::webdav_volume_id("203.0.113.70", 8080, "ada");

    let outcome = update_saved_server(webdav_target(new_url), Some(old_id)).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    assert!(has_webdav_credentials(new_url.to_string(), "ada".to_string()).await);
    assert!(!has_webdav_credentials(old_url.to_string(), "ada".to_string()).await);
}
