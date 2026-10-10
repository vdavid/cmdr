//! A saved server moving to a new address, through the command the edit sheet
//! calls: what goes with it, and what refuses. The move itself:
//! `server_move.rs`.
//!
//! ❗ **This suite owns `203.0.113.x`** (RFC 5737, routed nowhere), beside
//! `servers_test.rs`'s `192.0.2.x` and `server_volumes_test.rs`'s `198.51.100.x`:
//! the stores are process-global, so a shared address is two cells racing one entry.

use super::*;
use crate::commands::sftp::{has_sftp_credentials, save_sftp_credentials};
use crate::commands::webdav::{has_webdav_credentials, save_webdav_credentials};
use crate::favorites::store::{self as favorites, FavoriteVolume};
use crate::file_system::write_operations::test_support::QueuedOperationFixture;
use crate::network::s3_known_places::{KnownS3Place, S3ProviderChoice};
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

/// ❗ A move drops the old session, which stops a copy on it like Disconnect
/// would. So it's refused while one runs there, and refused BEFORE the password
/// moves: nothing is touched.
#[tokio::test]
async fn moving_a_server_while_a_copy_runs_on_it_is_refused_and_moves_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old_host, new_host) = ("203.0.113.100", "203.0.113.101");
    sftp_known_servers::remember(sftp_entry(old_host, 22));
    save_sftp_credentials(old_host.to_string(), 22, "ada".to_string(), "pa55".to_string())
        .await
        .expect("the test store always accepts");
    let _copy = QueuedOperationFixture::park_naming("server-move-running", vec![sftp_id(old_host, 22)], Vec::new());

    let outcome = update_saved_server(sftp_target(new_host, 22, "ada"), Some(sftp_id(old_host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::OperationRunning);
    assert!(
        sftp_known_servers::find(old_host, 22, "ada").is_some(),
        "the server stays put"
    );
    assert!(sftp_known_servers::find(new_host, 22, "ada").is_none());
    assert!(
        !has_sftp_credentials(new_host.to_string(), 22, "ada".to_string()).await,
        "the password wasn't copied"
    );
    let old_prefix = cmdr_fs::volume::sftp_app_root(old_host, 22, "ada");
    assert!(crate::volume_broadcast::server_moves_from(&old_prefix).is_empty());
}

/// A queued copy names the old place too, and would set out for a session the
/// move had already dropped.
#[tokio::test]
async fn moving_a_server_a_queued_copy_waits_on_is_refused() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old_host, new_host) = ("203.0.113.110", "203.0.113.111");
    sftp_known_servers::remember(sftp_entry(old_host, 22));
    let _copy = QueuedOperationFixture::park_naming("server-move-queued", Vec::new(), vec![sftp_id(old_host, 22)]);

    let outcome = update_saved_server(sftp_target(new_host, 22, "ada"), Some(sftp_id(old_host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::OperationRunning);
    assert!(sftp_known_servers::find(old_host, 22, "ada").is_some());
}

/// ❗ A dial to the old address that set out before Save and succeeds after it
/// would remember the old entry again, a second saved server beside the moved
/// one. The move calls it off, and a dial already past that can't land.
#[tokio::test]
async fn a_dial_to_the_old_address_that_was_out_during_the_move_is_called_off_and_cannot_land() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old_host, new_host) = ("203.0.113.140", "203.0.113.141");
    sftp_known_servers::remember(sftp_entry(old_host, 22));
    let (cancel, attempt) =
        sftp_volume_wiring::attempts().register_dialing("server-move-dial-out", vec![sftp_id(old_host, 22)]);

    let outcome = update_saved_server(sftp_target(new_host, 22, "ada"), Some(sftp_id(old_host, 22))).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    assert!(cancel.is_cancelled(), "the dial to the old address is called off");
    assert!(
        attempt.land().await.is_none(),
        "and one already past its last cancel check can't land"
    );
}

/// An edit that keeps the address doesn't touch the session, so a copy running
/// on the place is no reason to refuse it.
#[tokio::test]
async fn an_in_place_edit_saves_while_a_copy_runs() {
    let host = "203.0.113.120";
    sftp_known_servers::remember(sftp_entry(host, 22));
    let _copy = QueuedOperationFixture::park_naming("server-edit-running", vec![sftp_id(host, 22)], Vec::new());
    let renamed = ServerTarget::Sftp {
        display_name: "Renamed".to_string(),
        host: host.to_string(),
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

// -- an S3 account's endpoint --

fn s3_other(endpoint: &str) -> S3ProviderChoice {
    S3ProviderChoice::Other {
        endpoint: endpoint.to_string(),
        region: None,
        path_style: true,
    }
}

fn s3_place_at(provider: S3ProviderChoice, key: &str, bucket: Option<&str>) -> KnownS3Place {
    KnownS3Place {
        provider,
        access_key_id: key.to_string(),
        bucket: bucket.map(str::to_string),
        auto_reconnect: true,
        pinned: true,
        last_connected_at: "2026-09-03T00:00:00Z".to_string(),
    }
}

fn s3_id(place: &KnownS3Place) -> String {
    place.volume_id().expect("a valid test provider")
}

/// A self-hosted MinIO on a new IP: the ACCOUNT moves, every bucket under the key
/// with it, and the one secret they share.
#[tokio::test]
async fn moving_an_s3_accounts_endpoint_takes_every_bucket_and_the_shared_secret_along() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old, new) = (
        s3_other("http://203.0.113.90:9000"),
        s3_other("https://203.0.113.91:9443"),
    );
    let photos = s3_place_at(old.clone(), "AKIAMOVE", Some("photos"));
    let root = s3_place_at(old.clone(), "AKIAMOVE", None);
    s3_known_places::remember(photos.clone());
    s3_known_places::remember(root.clone());
    crate::commands::s3::save_s3_credentials(old.clone(), "AKIAMOVE".to_string(), "s3cret".to_string())
        .await
        .expect("the test store always accepts");
    let account = s3_id(&root);

    let outcome = update_saved_s3_account(account, "Home MinIO".to_string(), Some(new.clone())).await;

    assert_eq!(outcome, SavedServerOutcome::Saved);
    let moved_photos = s3_place_at(new.clone(), "AKIAMOVE", Some("photos"));
    assert!(
        s3_known_places::find(&s3_id(&photos)).is_none(),
        "the old endpoint is gone"
    );
    let stored = s3_known_places::find(&s3_id(&moved_photos)).expect("the bucket lives at the new endpoint");
    assert_eq!(s3_known_places::account_name(&stored), "Home MinIO");
    assert!(crate::commands::s3::has_s3_credentials(new, "AKIAMOVE".to_string()).await);
    assert!(!crate::commands::s3::has_s3_credentials(old, "AKIAMOVE".to_string()).await);
    let announced =
        crate::volume_broadcast::server_moves_from(&cmdr_fs::volume::s3_app_root("203.0.113.90", 9000, "AKIAMOVE"));
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].places.len(), 2, "both places follow, with one event");
}

/// A preset's region or account ID names other STORAGE, not a new road to the same
/// one, so it stays an Add.
#[tokio::test]
async fn moving_a_preset_accounts_endpoint_is_refused() {
    let aws = |region: &str| S3ProviderChoice::Aws {
        region: region.to_string(),
    };
    let place = s3_place_at(aws("eu-west-1"), "AKIAPRESET", Some("photos"));
    s3_known_places::remember(place.clone());
    let account = s3_id(&s3_place_at(aws("eu-west-1"), "AKIAPRESET", None));

    let outcome = update_saved_s3_account(account, String::new(), Some(aws("us-east-2"))).await;

    assert_eq!(outcome, SavedServerOutcome::AccountChanged);
    assert!(s3_known_places::find(&s3_id(&place)).is_some(), "nothing moved");
}

/// Every bucket moves with the account, so a copy on any one of them holds the
/// whole move.
#[tokio::test]
async fn moving_an_s3_account_while_a_copy_runs_on_one_of_its_buckets_is_refused() {
    let _secrets = crate::test_support::isolate_secrets();
    let (old, new) = (
        s3_other("http://203.0.113.130:9000"),
        s3_other("http://203.0.113.131:9000"),
    );
    let photos = s3_place_at(old.clone(), "AKIABUSY", Some("photos"));
    s3_known_places::remember(photos.clone());
    let account = s3_id(&s3_place_at(old, "AKIABUSY", None));
    let _copy = QueuedOperationFixture::park_naming("s3-move-running", vec![s3_id(&photos)], Vec::new());

    let outcome = update_saved_s3_account(account, String::new(), Some(new)).await;

    assert_eq!(outcome, SavedServerOutcome::OperationRunning);
    assert!(s3_known_places::find(&s3_id(&photos)).is_some(), "nothing moved");
}
