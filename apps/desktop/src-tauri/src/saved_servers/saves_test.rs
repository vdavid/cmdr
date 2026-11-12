//! An add without a dial: what "Add anyway" (and an approved Ask Cmdr suggestion)
//! writes, and that it contacts nothing.

use super::save_target;
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::network::{nfs_known_places, s3_known_places, sftp_known_servers, webdav_known_servers};
use crate::saved_servers::ServerTarget;
use crate::saved_servers::target_ids::place_id;

/// Saves `target` and answers its place id, asserting the save went through with no
/// dial filed and no volume registered under that id.
async fn saved_without_contact(target: ServerTarget) -> String {
    let id = place_id(target.clone()).expect("a target with an id");
    let dials = crate::network::connect_wiring::dials_filed();

    assert_eq!(save_target(target).await, SavedServerOutcome::Saved);

    assert_eq!(
        crate::network::connect_wiring::dials_filed(),
        dials,
        "no dial was filed"
    );
    let manager = crate::file_system::volume::manager::get_volume_manager();
    assert!(manager.get(&id).is_none(), "nothing was registered");
    id
}

/// ❗ **Every protocol's add without a dial is a store write and nothing else.** The
/// addresses are documentation blocks (RFC 5737) routed nowhere, so a dial that
/// slipped through would show as a filed attempt, never as a quiet success.
#[tokio::test]
async fn an_add_without_a_dial_writes_the_store_and_contacts_nothing() {
    let _secrets = crate::test_support::isolate_secrets();

    saved_without_contact(ServerTarget::Sftp {
        display_name: "Build box".to_string(),
        host: "192.0.2.81".to_string(),
        port: 22,
        username: "test_user_1".to_string(),
        remote_root: "/srv".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: false,
    })
    .await;
    let sftp = sftp_known_servers::find("192.0.2.81", 22, "test_user_1").expect("saved");
    assert!(sftp.pinned, "a new place shows in the switcher, as the add form's does");
    assert_eq!(sftp.last_connected_at, "", "a save claims no connection");

    let url = "https://192.0.2.82:5006/dav/";
    saved_without_contact(ServerTarget::Webdav {
        display_name: "NAS over dav".to_string(),
        url: url.to_string(),
        username: "test_user_1".to_string(),
        remote_root: "/".to_string(),
        start_folder: None,
        auto_reconnect: false,
    })
    .await;
    let webdav = webdav_known_servers::find(url, "test_user_1").expect("saved");
    assert_eq!(webdav.last_connected_at, "", "a save claims no connection");

    let s3 = saved_without_contact(ServerTarget::S3 {
        display_name: "Archive".to_string(),
        provider: s3_known_places::S3ProviderChoice::Wasabi {
            region: "eu-central-1".to_string(),
        },
        access_key_id: "AKIASAVEDNOCONTACT".to_string(),
        bucket: Some("archive".to_string()),
        auto_reconnect: false,
    })
    .await;
    let s3 = s3_known_places::find(&s3).expect("saved");
    assert_eq!(s3.last_connected_at, "", "a save claims no connection");

    let nfs = saved_without_contact(ServerTarget::Nfs {
        display_name: "Media NAS".to_string(),
        host: "192.0.2.84".to_string(),
        port: None,
        export: "/export/media".to_string(),
        uid: 501,
        gid: 20,
        other_gids: Vec::new(),
        start_folder: None,
        auto_reconnect: false,
    })
    .await;
    let nfs = nfs_known_places::find(&nfs).expect("saved");
    assert_eq!(nfs.last_connected_at, "", "a save claims no connection");
}

/// ❗ An approved Ask Cmdr edit names only what it changes, so it can't drop the person's
/// other groups: they ride along from the stored entry (`save_place_edit`).
#[tokio::test]
async fn a_place_edit_keeps_the_other_groups_and_adding_them_keeps_the_id() {
    use crate::saved_servers::saves::{PlaceEdit, save_place_edit};
    let _secrets = crate::test_support::isolate_secrets();
    let target = |other_gids: Vec<u32>| ServerTarget::Nfs {
        display_name: String::new(),
        host: "192.0.2.86".to_string(),
        port: None,
        export: "/export/media".to_string(),
        uid: 501,
        gid: 20,
        other_gids,
        start_folder: None,
        auto_reconnect: false,
    };
    assert_eq!(place_id(target(Vec::new())), place_id(target(vec![2001, 2002])));

    let id = saved_without_contact(target(vec![2001, 2002])).await;
    let edit = PlaceEdit {
        start_folder: Some("/Photos".to_string()),
        ..PlaceEdit::default()
    };
    assert_eq!(save_place_edit(&id, edit).await, SavedServerOutcome::Saved);
    let saved = nfs_known_places::find(&id).expect("saved");
    assert_eq!(saved.start_folder.as_deref(), Some("/Photos"));
    assert_eq!(saved.other_gids, vec![2001, 2002]);
}

/// An edit of a saved, unconnected place keeps when it was last connected: the
/// time feeds "Last used" and recency order, and a rename isn't a connection.
#[tokio::test]
async fn an_edit_without_a_dial_keeps_the_last_connected_time() {
    use crate::saved_servers::save_edit;
    use crate::saved_servers::test_fixtures::{sftp_entry, webdav_entry};
    let _secrets = crate::test_support::isolate_secrets();

    let sftp = sftp_entry("192.0.2.85", true);
    sftp_known_servers::remember(sftp.clone());
    let id = cmdr_fs::volume::sftp_volume_id(&sftp.host, sftp.port, &sftp.username);
    let renamed = ServerTarget::Sftp {
        display_name: "Renamed box".to_string(),
        host: sftp.host.clone(),
        port: sftp.port,
        username: sftp.username.clone(),
        remote_root: sftp.remote_root.clone(),
        start_folder: None,
        key_file: None,
        use_agent: false,
        auto_reconnect: true,
    };
    assert_eq!(save_edit(&id, renamed).await, SavedServerOutcome::Saved);
    let saved = sftp_known_servers::find(&sftp.host, sftp.port, &sftp.username).expect("saved");
    assert_eq!(saved.display_name, "Renamed box");
    assert_eq!(saved.last_connected_at, sftp.last_connected_at);

    let webdav = webdav_entry("192.0.2.86", true);
    webdav_known_servers::remember(webdav.clone());
    let target = ServerTarget::Webdav {
        display_name: "Renamed dav".to_string(),
        url: webdav.url.clone(),
        username: webdav.username.clone(),
        remote_root: webdav.remote_root.clone(),
        start_folder: None,
        auto_reconnect: true,
    };
    let id = place_id(target.clone()).expect("an id");
    assert_eq!(save_edit(&id, target).await, SavedServerOutcome::Saved);
    let saved = webdav_known_servers::find(&webdav.url, &webdav.username).expect("saved");
    assert_eq!(saved.display_name, "Renamed dav");
    assert_eq!(saved.last_connected_at, webdav.last_connected_at);
}
