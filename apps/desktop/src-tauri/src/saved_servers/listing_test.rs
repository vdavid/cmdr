//! What the servers listing lists: every store's entries, SMB hosts grouped with
//! their shares, and the id a save is found by.

use crate::network::manual_servers::{self, ManualServerEntry};
use crate::network::{known_shares, sftp_known_servers, webdav_known_servers};
use crate::saved_servers::listing;
use crate::saved_servers::smb_hosts;
use crate::saved_servers::target_ids::place_id;
use crate::saved_servers::test_fixtures::{find, listing_of as saved_servers_of, sftp_entry, share_row, webdav_entry};
use crate::saved_servers::{ServerNameSource, ServerProtocol, ServerTarget};

fn manual_entry(address: &str) -> ManualServerEntry {
    ManualServerEntry {
        id: manual_servers::generate_server_id(address, 445),
        display_name: address.to_string(),
        address: address.to_string(),
        port: 445,
        added_at: "2026-09-03T00:00:00Z".to_string(),
        username: None,
        awaiting_first_connect: false,
    }
}

/// A manual SMB host a person gave a name of their own.
fn named_manual_entry(address: &str, name: &str) -> ManualServerEntry {
    ManualServerEntry {
        display_name: name.to_string(),
        ..manual_entry(address)
    }
}

/// ❗ **The listing is the union of three stores**, in one shape the hub renders
/// without branching on protocol.
#[test]
fn the_listing_unions_the_sftp_the_webdav_and_the_smb_stores() {
    let sftp_host = "192.0.2.31";
    let webdav_host = "192.0.2.32";
    let smb_host = "192.0.2.33";
    sftp_known_servers::remember(sftp_entry(sftp_host, true));
    webdav_known_servers::remember(webdav_entry(webdav_host, false));

    let servers = saved_servers_of(vec![manual_entry(smb_host)]);

    let sftp = find(&servers, &format!("{sftp_host} over ssh"));
    assert_eq!(sftp.protocol, ServerProtocol::Sftp);
    assert_eq!(sftp.address, format!("{sftp_host}:2222"));
    assert_eq!(sftp.username.as_deref(), Some("test_user_1"));
    assert!(sftp.pinned, "the store said pinned");
    assert_eq!(sftp.last_connected_at.as_deref(), Some("2026-09-01T00:00:00Z"));

    let webdav = find(&servers, &format!("{webdav_host} over dav"));
    assert_eq!(webdav.protocol, ServerProtocol::Webdav);
    assert_eq!(webdav.address, format!("http://{webdav_host}:8080/dav/"));
    assert!(!webdav.pinned, "the store said unpinned");

    let smb = find(&servers, smb_host);
    assert_eq!(smb.protocol, ServerProtocol::Smb);
    assert_eq!(smb.address, smb_host);
}

/// ❗ **Whether a person NAMED a server is a FACT the listing publishes**,
/// because the hub ranks three names and can't tell them apart by looking at the
/// strings. An account label is the user's; an SMB host's label is the address
/// they typed, worn as a stand-in, so a Bonjour name outranks it.
#[test]
fn an_account_label_is_the_users_name_and_an_smb_hosts_address_is_a_stand_in() {
    let sftp_host = "192.0.2.41";
    let smb_host = "192.0.2.42";
    sftp_known_servers::remember(sftp_entry(sftp_host, false));

    let servers = saved_servers_of(vec![manual_entry(smb_host)]);

    assert_eq!(
        find(&servers, &format!("{sftp_host} over ssh")).name_source,
        ServerNameSource::User
    );
    assert_eq!(find(&servers, smb_host).name_source, ServerNameSource::Fallback);
}

/// ❗ **An SMB host a person named is listed by that name, as theirs**, so the
/// hub keeps it over a Bonjour name the way it keeps a named SFTP account.
#[test]
fn a_named_smb_host_is_listed_by_its_name_as_the_users_own() {
    let smb_host = "192.0.2.43";

    let servers = saved_servers_of(vec![named_manual_entry(smb_host, "Test NAS")]);

    let smb = find(&servers, "Test NAS");
    assert_eq!(smb.name_source, ServerNameSource::User);
    assert_eq!(smb.address, smb_host, "the address stays what was typed");
}

/// ❗ **An SMB host carries the account the person typed for it**, which is what
/// the edit sheet opens on.
#[test]
fn an_smb_host_carries_the_account_typed_for_it() {
    let smb_host = "192.0.2.45";
    let entry = ManualServerEntry {
        username: Some("test_user_6".to_string()),
        ..manual_entry(smb_host)
    };

    let servers = saved_servers_of(vec![entry]);

    assert_eq!(find(&servers, smb_host).username.as_deref(), Some("test_user_6"));
}

/// ❗ **A manual entry wins over a share-history row for the same host**, so a
/// name typed at add time survives the host being opened once: the share list
/// writes a server-level row under the same address, and the dedup keeps the
/// first row it sees.
#[test]
fn a_named_smb_host_keeps_its_name_once_its_shares_were_listed() {
    let smb_host = "192.0.2.44";
    let history = known_shares::KnownNetworkShare {
        server_name: smb_host.to_string(),
        share_name: String::new(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-09-05T00:00:00Z".to_string(),
        last_connection_mode: known_shares::ConnectionMode::Guest,
        last_known_auth_options: known_shares::AuthOptions::GuestOrCredentials,
        username: None,
        address: None,
        port: None,
        volume_id: None,
        mount_path: None,
        pinned: false,
        server_guid: None,
        server_guid_proof: crate::network::server_guid::GuidProof::Unauthenticated,
    };

    let servers = listing(vec![named_manual_entry(smb_host, "Office NAS")], vec![history], &[]);

    let rows: Vec<_> = servers.iter().filter(|s| s.address == smb_host).collect();
    assert_eq!(rows.len(), 1, "one host, one row");
    assert_eq!(rows[0].display_name, "Office NAS");
    assert_eq!(
        rows[0].last_connected_at.as_deref(),
        Some("2026-09-05T00:00:00Z"),
        "the share history still says when it was last used"
    );
}

/// ❗ **An SFTP or WebDAV account has exactly ONE place, and its id is the volume
/// id**, so a `saved` row and the volume it becomes are the same thing to a tab,
/// a favorite, and the switcher.
#[test]
fn an_sftp_account_has_one_place_carrying_the_volume_id() {
    let host = "192.0.2.34";
    sftp_known_servers::remember(sftp_entry(host, true));

    let servers = saved_servers_of(Vec::new());
    let sftp = find(&servers, &format!("{host} over ssh"));

    assert_eq!(sftp.places.len(), 1, "one account, one place");
    let place = &sftp.places[0];
    assert_eq!(
        place.volume_id,
        cmdr_fs::volume::sftp_volume_id(host, 2222, "test_user_1")
    );
    assert_eq!(
        place.volume_id, sftp.id,
        "the account IS the place for a one-place protocol"
    );
    assert!(place.pinned);
    assert!(!place.connected, "nothing is registered under that id");
}

/// ❗ **An SMB host with no saved share lists no places, and the host row itself is
/// never pinned**: its places are its saved SHARES, each carrying its own pin
/// (`a_saved_share_is_a_place_under_its_host_with_its_account` below). A manual
/// entry alone saves no share.
#[test]
fn an_smb_host_with_no_saved_share_lists_no_places_and_is_never_pinned() {
    let smb_host = "192.0.2.35";

    let servers = saved_servers_of(vec![manual_entry(smb_host)]);
    let smb = find(&servers, smb_host);

    assert!(smb.places.is_empty());
    assert!(!smb.pinned);
    assert!(smb.username.is_none(), "an SMB host is not an account yet");
    assert!(
        smb.auto_reconnect.is_none(),
        "SMB has no \"Reconnect automatically\" switch, so there is no row checkbox to fill"
    );
}

/// ❗ **An account nobody named is listed by its derived label, and says so**:
/// the hub shows `username@host`, and knows no person chose it.
#[test]
fn an_unnamed_account_is_listed_by_its_derived_label_as_a_stand_in() {
    let sftp_host = "192.0.2.64";
    let mut unnamed_sftp = sftp_entry(sftp_host, true);
    unnamed_sftp.display_name = String::new();
    sftp_known_servers::remember(unnamed_sftp);
    let webdav_host = "192.0.2.65";
    let mut unnamed_webdav = webdav_entry(webdav_host, true);
    unnamed_webdav.display_name = String::new();
    webdav_known_servers::remember(unnamed_webdav);

    let servers = saved_servers_of(Vec::new());

    for label in [format!("test_user_1@{sftp_host}"), format!("test_user_1@{webdav_host}")] {
        let server = find(&servers, &label);
        assert_eq!(server.name_source, ServerNameSource::Fallback, "{label}");
        assert_eq!(server.places[0].name, label, "the place carries the same label");
    }
}

/// ❗ **A saved share is a place under its host** (cmdr-reports#7: "user +
/// server + share"), carrying the account it opens as and the id its last mount
/// had, so the hub can open it and a pin points at the real volume.
#[test]
fn a_saved_share_is_a_place_under_its_host_with_its_account() {
    let smb_host = "192.0.2.46";
    let servers = listing(
        vec![named_manual_entry(smb_host, "Test NAS")],
        vec![share_row(
            smb_host,
            "Container",
            Some("test_user_6"),
            Some("smb-container"),
        )],
        &[],
    );

    let host = find(&servers, "Test NAS");
    assert_eq!(host.places.len(), 1);
    let place = &host.places[0];
    assert_eq!(place.volume_id, "smb-container");
    assert_eq!(place.name, "Container");
    assert_eq!(place.username.as_deref(), Some("test_user_6"));
    assert_eq!(place.app_root, "/Volumes/Container");
    assert!(place.pinned);
    assert_eq!(host.last_connected_at.as_deref(), Some("2026-09-24T10:00:00Z"));
}

/// A share on a host nobody typed in still gets a host row to hang under.
#[test]
fn a_saved_share_on_a_host_nobody_typed_in_brings_its_host() {
    let servers = listing(
        Vec::new(),
        vec![share_row("192.0.2.47", "photos", None, Some("smb-photos"))],
        &[],
    );

    let host = find(&servers, "192.0.2.47");
    assert_eq!(host.protocol, ServerProtocol::Smb);
    assert_eq!(host.places.len(), 1);
    assert_eq!(host.places[0].username, None, "a guest share has no account");
}

/// A share an Add named but nothing mounted yet is listed with a stand-in id and
/// its `smb://` address, which is how the hub knows to open it through the host.
#[test]
fn a_share_no_mount_went_through_is_listed_by_its_address() {
    let servers = listing(
        vec![named_manual_entry("192.0.2.48", "Office")],
        vec![share_row("192.0.2.48", "Scans", Some("test_user_1"), None)],
        &[],
    );

    let place = &find(&servers, "Office").places[0];
    assert_eq!(place.app_root, "smb://192.0.2.48/Scans");
    assert!(!place.pinned);
}

/// A manual SMB host on `port`, the way "Add server" files one.
fn manual_entry_on_port(address: &str, port: u16, name: &str) -> ManualServerEntry {
    ManualServerEntry {
        id: manual_servers::generate_server_id(address, port),
        display_name: name.to_string(),
        address: address.to_string(),
        port,
        added_at: "2026-09-03T00:00:00Z".to_string(),
        username: None,
        awaiting_first_connect: false,
    }
}

/// A share a mount through Cmdr filed: under the host's discovery name, with the
/// address and port it dialed.
fn mounted_share(
    server_name: &str,
    address: &str,
    port: u16,
    share: &str,
    volume_id: &str,
) -> known_shares::KnownNetworkShare {
    known_shares::KnownNetworkShare {
        address: Some(address.to_string()),
        port: (port != 445).then_some(port),
        ..share_row(server_name, share, None, Some(volume_id))
    }
}

/// The host-level row the share list files when a person signs in to it, under
/// the host's discovery name and nothing else.
fn signed_in_history(server_name: &str) -> known_shares::KnownNetworkShare {
    known_shares::KnownNetworkShare {
        share_name: String::new(),
        ..share_row(server_name, "unused", Some("testuser"), None)
    }
}

/// ❗ **Naming a host never splits it in two.** A manual host off port 445 is
/// listed by its discovery name (`localhost:11482`) until someone names it; its
/// sign-in history and shares are filed under that discovery name. Matching them by the name a person
/// typed gave the named host's history a second, made-up host row, and
/// "Edit server…" on that second row then saved into the first (QA 2026-09-25).
#[test]
fn a_named_host_off_port_445_keeps_its_shares_under_it() {
    let servers = listing(
        vec![manual_entry_on_port("localhost", 11482, "Both box")],
        vec![
            signed_in_history("localhost:11482"),
            mounted_share("localhost:11482", "localhost", 11482, "public", "smb-public"),
        ],
        &[],
    );

    let smb: Vec<_> = servers.iter().filter(|s| s.protocol == ServerProtocol::Smb).collect();
    assert_eq!(smb.len(), 1, "one host, one row: {smb:?}");
    assert_eq!(smb[0].display_name, "Both box");
    assert_eq!(smb[0].places.len(), 1, "its share stays under it");
    assert_eq!(smb[0].places[0].name, "public");
}

/// ❗ **A share belongs to the host on ITS port.** Two SMB servers on one machine
/// are two hosts (the Docker fixtures are ten on `localhost`), so a share mounted
/// from one never lands under the other, whichever was typed in first.
#[test]
fn a_share_belongs_to_the_host_on_its_own_port() {
    let servers = listing(
        vec![
            manual_entry_on_port("localhost", 11480, ""),
            manual_entry_on_port("localhost", 11482, ""),
        ],
        vec![mounted_share(
            "localhost",
            "localhost",
            11482,
            "public",
            "smb-public-11482",
        )],
        &[],
    );

    let on = |port: u16| {
        servers
            .iter()
            .find(|s| s.id == manual_servers::generate_server_id("localhost", port))
            .unwrap_or_else(|| panic!("the host on {port} must be listed"))
    };
    assert!(on(11480).places.is_empty(), "not a share of the host on 11480");
    assert_eq!(on(11482).places.len(), 1);
}

/// An SMB host's address says its port when it isn't 445: `localhost` alone names
/// a different server, and the Address column is where a person tells them apart.
#[test]
fn an_smb_hosts_address_carries_its_port_off_445() {
    let servers = saved_servers_of(vec![manual_entry_on_port("localhost", 11482, "")]);

    let smb = servers.iter().find(|s| s.protocol == ServerProtocol::Smb).unwrap();
    assert_eq!(smb.address, "localhost:11482");
}

/// ❗ **What Forget server takes is exactly what the row showed**: the store rows
/// the listing filed under that host, and none of another host's, even one on
/// the same machine.
#[test]
fn a_hosts_group_holds_its_own_rows_and_no_other_hosts() {
    let groups = smb_hosts::smb_host_groups(
        vec![
            manual_entry_on_port("localhost", 11480, ""),
            manual_entry_on_port("localhost", 11482, "Both box"),
        ],
        vec![
            mounted_share("localhost:11480", "localhost", 11480, "public", "smb-a"),
            mounted_share("localhost:11482", "localhost", 11482, "public", "smb-b"),
            mounted_share("localhost:11482", "localhost", 11482, "private", "smb-c"),
        ],
        &[],
    );

    let id = manual_servers::generate_server_id("localhost", 11482);
    let group = groups.iter().find(|g| g.server.id == id).expect("the named host");
    let volume_ids: Vec<_> = group.rows.iter().filter_map(|r| r.volume_id.as_deref()).collect();
    assert_eq!(volume_ids, ["smb-b", "smb-c"]);
}

/// A host only the share history knows is dialed where its mount went, port
/// included: naming it saves a manual entry, and one filed under the discovery
/// name (`localhost:11482`) on 445 would dial nothing.
#[test]
fn a_host_only_the_share_history_knows_dials_where_its_mount_went() {
    let groups = smb_hosts::smb_host_groups(
        Vec::new(),
        vec![mounted_share("localhost:11482", "localhost", 11482, "public", "smb-b")],
        &[],
    );

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].host, "localhost");
    assert_eq!(groups[0].port, 11482);
    assert_eq!(
        groups[0].server.id,
        manual_servers::generate_server_id("localhost", 11482)
    );
    assert!(!groups[0].manual);
}

/// ❗ **The id a save is found by is the listing's own**, whatever spelling the
/// store keeps: a WebDAV URL gains its trailing slash when saved, and finding the
/// row by the typed URL missed it, so "Add anyway" said nothing was saved.
#[test]
fn a_saved_servers_id_is_the_listings_own_whatever_the_url_spelling() {
    let host = "192.0.2.51";
    webdav_known_servers::remember(webdav_entry(host, false));
    let typed = |url: &str| ServerTarget::Webdav {
        display_name: String::new(),
        url: url.to_string(),
        username: "test_user_1".to_string(),
        remote_root: "/".to_string(),
        start_folder: None,
        auto_reconnect: true,
    };

    let listed = find(&saved_servers_of(Vec::new()), &format!("{host} over dav"))
        .id
        .clone();
    assert_eq!(
        place_id(typed(&format!("http://{host}:8080/dav"))),
        Some(listed.clone())
    );
    assert_eq!(place_id(typed(&format!("http://{host}:8080/dav/"))), Some(listed));
    assert_eq!(place_id(typed("not a url")), None);
}

/// ❗ **Forget server's "Also forget the saved password" takes every name this
/// server's password can be filed under, on ITS port, and none on another.** The
/// share list files it under the discovery name (`localhost:11482`), "Connect
/// directly" under host + port, and a discovered twin under its Bonjour name;
/// `localhost` alone is a different server (the one on 445) and keeps its password.
#[test]
fn a_hosts_credential_names_are_its_own_on_its_port() {
    let groups = smb_hosts::smb_host_groups(
        vec![
            manual_entry_on_port("localhost", 11482, "Both box"),
            manual_entry_on_port("localhost", 11480, ""),
        ],
        vec![
            signed_in_history("localhost:11482"),
            mounted_share("localhost:11482", "127.0.0.1", 11482, "private", "smb-c"),
            mounted_share("localhost:11480", "localhost", 11480, "public", "smb-a"),
        ],
        &[],
    );
    let id = manual_servers::generate_server_id("localhost", 11482);
    let group = groups.iter().find(|g| g.server.id == id).expect("the named host");

    let names = group.credential_names(&[]);
    let keys: std::collections::BTreeSet<_> = names
        .iter()
        .map(|name| crate::network::server_identity::credential_key(name))
        .collect();
    assert_eq!(
        keys,
        ["127.0.0.1:11482", "localhost:11482"]
            .into_iter()
            .map(String::from)
            .collect(),
        "every spelling of this server on 11482, and nothing of 445 or 11480: {names:?}"
    );
    assert_eq!(group.share_names(), ["private"]);
}
