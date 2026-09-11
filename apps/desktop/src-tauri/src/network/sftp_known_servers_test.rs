//! What the server list owes a picker: one entry per account, found again the
//! way the volume id finds it.
//!
//! In-memory only: these run before `load_known_sftp_servers` names a file, so
//! `save()` is a no-op and the cells assert on the lookup rather than the write.
//! The durability of the write itself is `config::durable_write_json`'s contract.

use super::*;

/// The store is process-global, so two cells writing the same triple would see
/// each other. Each uses a host of its own instead of taking turns on a mutex.
fn host_for(cell: &str) -> String {
    format!("{cell}.sftp-servers.test")
}

fn server(host: &str, username: &str) -> KnownSftpServer {
    KnownSftpServer {
        host: host.to_string(),
        port: 22,
        username: username.to_string(),
        display_name: host.to_string(),
        remote_root: "/srv/data".to_string(),
        start_folder: None,
        key_file: None,
        use_agent: true,
        auto_reconnect: true,
        pinned: false,
        last_connected_at: "2026-08-22T10:00:00Z".to_string(),
    }
}

fn entries_for(host: &str) -> Vec<KnownSftpServer> {
    all()
        .into_iter()
        .filter(|entry| entry.host.eq_ignore_ascii_case(host))
        .collect()
}

#[test]
fn a_remembered_server_comes_back() {
    let host = host_for("remembered");
    remember(server(&host, "ada"));

    let found = entries_for(&host);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].username, "ada");
    assert_eq!(found[0].remote_root, "/srv/data");
}

#[test]
fn connecting_again_updates_the_entry_rather_than_adding_one() {
    let host = host_for("updated");
    remember(server(&host, "ada"));
    let mut later = server(&host, "ada");
    later.remote_root = "/home/ada".to_string();
    later.last_connected_at = "2026-08-23T09:00:00Z".to_string();
    remember(later);

    let found = entries_for(&host);
    assert_eq!(found.len(), 1, "one server and one account is one entry");
    assert_eq!(found[0].remote_root, "/home/ada");
}

#[test]
fn two_accounts_on_one_server_are_two_entries() {
    // ❗ The same rule the volume id follows: two accounts see different files
    // under the same paths, so collapsing them would show one server that opens
    // the wrong home directory half the time.
    let host = host_for("two-accounts");
    remember(server(&host, "ada"));
    remember(server(&host, "grace"));

    let mut usernames: Vec<String> = entries_for(&host).into_iter().map(|entry| entry.username).collect();
    usernames.sort();
    assert_eq!(usernames, vec!["ada".to_string(), "grace".to_string()]);
}

#[test]
fn the_host_folds_case_and_the_account_does_not() {
    // DNS is case-insensitive; POSIX accounts are not. Same split as
    // `sftp_volume_id`, and a drift here would file one volume under two entries.
    let host = host_for("case");
    remember(server(&host, "ada"));
    remember(server(&host.to_uppercase(), "ada"));
    assert_eq!(entries_for(&host).len(), 1, "one server, however it was typed");

    remember(server(&host, "Ada"));
    assert_eq!(entries_for(&host).len(), 2, "`Ada` and `ada` may be two people");
}

#[test]
fn a_port_is_part_of_the_identity() {
    // A jump box and a container on one machine are different servers.
    let host = host_for("ports");
    remember(server(&host, "ada"));
    let mut other = server(&host, "ada");
    other.port = 2222;
    remember(other);

    assert_eq!(entries_for(&host).len(), 2);
}

#[test]
fn forgetting_answers_whether_anything_was_there() {
    let host = host_for("forget");
    remember(server(&host, "ada"));

    assert!(forget(&host, 22, "ada"));
    assert!(entries_for(&host).is_empty());
    assert!(!forget(&host, 22, "ada"), "a second forget has nothing to do");
}

#[test]
fn forgetting_one_account_leaves_the_other_alone() {
    let host = host_for("forget-one");
    remember(server(&host, "ada"));
    remember(server(&host, "grace"));

    assert!(forget(&host, 22, "ada"));

    let found = entries_for(&host);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].username, "grace");
}

/// ❗ **A server saved before this setting existed still reconnects
/// automatically.**
///
/// SFTP has always come back on its own with a bounded backoff. Reading a missing
/// field as `false` would switch that off under everyone who already has saved
/// servers, which is a regression dressed as a migration.
#[test]
fn a_server_saved_before_the_setting_existed_still_reconnects_automatically() {
    let stored = r#"{
      "knownSftpServers": [
        {
          "host": "naspolya",
          "port": 22,
          "username": "ada",
          "displayName": "Naspolya",
          "remoteRoot": "/srv/data",
          "keyFile": null,
          "useAgent": true,
          "lastConnectedAt": "2026-08-22T10:00:00Z"
        }
      ]
    }"#;

    let store: KnownSftpServersStore = serde_json::from_str(stored).expect("an older file still parses");

    assert!(
        store.known_sftp_servers[0].auto_reconnect,
        "the field wasn't in the file, and the default has to be the behavior that was already shipping"
    );
}

/// A server saved before start folders existed lands at its root, the way it
/// always did.
#[test]
fn a_server_saved_before_start_folders_existed_lands_at_its_root() {
    let stored = r#"{
      "knownSftpServers": [
        {
          "host": "naspolya",
          "port": 22,
          "username": "ada",
          "displayName": "Naspolya",
          "remoteRoot": "/srv/data",
          "keyFile": null,
          "useAgent": true,
          "lastConnectedAt": "2026-08-22T10:00:00Z"
        }
      ]
    }"#;

    let store: KnownSftpServersStore = serde_json::from_str(stored).expect("an older file still parses");

    assert_eq!(store.known_sftp_servers[0].start_folder, None);
}

#[test]
fn the_start_folder_round_trips_through_the_stored_file() {
    let mut deeper = server("start-folder-round-trip.sftp-servers.test", "ada");
    deeper.start_folder = Some("/srv/data/photos".to_string());
    let store = KnownSftpServersStore {
        known_sftp_servers: vec![deeper],
    };

    let written = serde_json::to_string(&store).expect("serializable");
    let read: KnownSftpServersStore = serde_json::from_str(&written).expect("parseable");

    assert_eq!(
        read.known_sftp_servers[0].start_folder.as_deref(),
        Some("/srv/data/photos")
    );
}

/// `find` answers by the identity `remember` files under, so a lookup can't
/// miss an entry a connect just wrote.
#[test]
fn finding_a_server_uses_the_same_identity_as_remembering_one() {
    let host = host_for("find");
    remember(server(&host, "ada"));

    assert!(find(&host.to_uppercase(), 22, "ada").is_some(), "the host folds case");
    assert!(find(&host, 22, "Ada").is_none(), "the account doesn't");
    assert!(find(&host, 2222, "ada").is_none(), "the port is part of the identity");
}

// ── The label, and names that only repeat the address ────────────────

#[test]
fn an_unnamed_server_is_labeled_username_at_host() {
    let mut unnamed = server("192.168.1.111", "david");
    unnamed.display_name = String::new();
    assert_eq!(unnamed.label(), "david@192.168.1.111");

    let mut named = server("192.168.1.111", "david");
    named.display_name = "NAS".to_string();
    assert_eq!(named.label(), "NAS");
}

/// ❗ The prod case, and what the cleanup owes it: the address it was saved under
/// stops being its name, and nothing else about the entry moves.
#[test]
fn a_name_that_only_repeats_the_servers_address_is_cleared_on_load() {
    let mut prod = server("192.168.1.111", "david");
    prod.display_name = "sftp://david@192.168.1.111:22/share/ZFS18_DATA/naspi".to_string();
    prod.remote_root = "/share/ZFS18_DATA/naspi/tmp".to_string();
    let mut labeled = server("192.168.1.111", "grace");
    labeled.display_name = "NAS".to_string();
    let mut elsewhere = server("192.168.1.112", "david");
    elsewhere.display_name = "sftp://david@192.168.1.111:22".to_string();
    let mut store = KnownSftpServersStore {
        known_sftp_servers: vec![prod, labeled, elsewhere],
    };

    assert_eq!(clear_address_shaped_names(&mut store), 1);

    let [prod, labeled, elsewhere] = store.known_sftp_servers.as_slice() else {
        panic!("three entries in, three out");
    };
    assert_eq!(prod.display_name, "");
    assert_eq!(prod.label(), "david@192.168.1.111");
    assert_eq!(prod.remote_root, "/share/ZFS18_DATA/naspi/tmp", "only the name moves");
    assert_eq!(labeled.display_name, "NAS", "a person's own label stays");
    assert_eq!(
        elsewhere.display_name, "sftp://david@192.168.1.111:22",
        "another server's address is a label on this one"
    );
}

#[test]
fn the_account_at_host_spellings_are_cleared_and_the_port_still_counts() {
    for (name, cleared) in [
        ("david@192.168.1.111", true),
        ("david@192.168.1.111:22", true),
        ("DAVID@192.168.1.111", false),
        ("david@192.168.1.111:2222", false),
    ] {
        let mut entry = server("192.168.1.111", "david");
        entry.display_name = name.to_string();
        let mut store = KnownSftpServersStore {
            known_sftp_servers: vec![entry],
        };
        assert_eq!(clear_address_shaped_names(&mut store) == 1, cleared, "{name:?}");
    }
}

#[test]
fn a_host_typed_in_another_case_is_still_the_same_servers_address() {
    let mut entry = server("NAS.local", "david");
    entry.display_name = "sftp://david@nas.LOCAL:22/srv".to_string();
    let mut store = KnownSftpServersStore {
        known_sftp_servers: vec![entry],
    };
    assert_eq!(clear_address_shaped_names(&mut store), 1);
}

/// Nothing to clear reports nothing, which is what keeps a load from rewriting
/// the file. `server()` names an entry after its bare host, and a bare host is a
/// name someone might have chosen.
#[test]
fn a_store_with_no_address_shaped_names_reports_nothing_cleared() {
    let mut store = KnownSftpServersStore {
        known_sftp_servers: vec![server("nas.local", "ada")],
    };
    assert_eq!(clear_address_shaped_names(&mut store), 0);
    assert_eq!(store.known_sftp_servers[0].display_name, "nas.local");
}

/// The switch survives a round trip through the file, both ways.
#[test]
fn the_switch_round_trips_through_the_stored_file() {
    let mut off = server("round-trip.sftp-servers.test", "ada");
    off.auto_reconnect = false;
    let store = KnownSftpServersStore {
        known_sftp_servers: vec![off],
    };

    let written = serde_json::to_string(&store).expect("serializable");
    let read: KnownSftpServersStore = serde_json::from_str(&written).expect("parseable");

    assert!(!read.known_sftp_servers[0].auto_reconnect);
}

/// ❗ **A server saved before pins existed is NOT pinned.**
///
/// The opposite default from `auto_reconnect`, and for the same reason: read the
/// missing field the way the behavior already shipping reads. Nothing was in the
/// switcher before pins, so defaulting to `true` would drop every saved server
/// into it at once — the exact crowding the pin cap exists to prevent.
#[test]
fn a_server_saved_before_pins_existed_is_not_pinned() {
    let stored = r#"{
      "knownSftpServers": [
        {
          "host": "naspolya",
          "port": 22,
          "username": "ada",
          "displayName": "Naspolya",
          "remoteRoot": "/srv/data",
          "keyFile": null,
          "useAgent": true,
          "autoReconnect": true,
          "lastConnectedAt": "2026-08-22T10:00:00Z"
        }
      ]
    }"#;

    let store: KnownSftpServersStore = serde_json::from_str(stored).expect("an older file still parses");

    assert!(
        !store.known_sftp_servers[0].pinned,
        "a file written before pins says nothing about them, and nothing is what it meant"
    );
}

/// ❗ **A reconnect never re-pins a server the user unpinned.**
///
/// `remember` runs on EVERY successful connect, so a pin taken from the caller
/// would put an unpinned row back in the switcher the next time the session came
/// back — an unpin that undoes itself the moment the server answers.
#[test]
fn remembering_an_existing_server_carries_its_pin_across() {
    let host = host_for("pin-preserved");

    let mut first = server(&host, "ada");
    first.pinned = true;
    remember(first);

    // The user unpins it. Mutated in place under the lock, the way the writer for
    // that will: the store is process-global, and rebuilding the whole vec here
    // would drop entries other cells are appending in parallel.
    {
        let mut store = known().lock_ignore_poison();
        for entry in store
            .known_sftp_servers
            .iter_mut()
            .filter(|entry| same_server(entry, &host, 22, "ada"))
        {
            entry.pinned = false;
        }
    }

    // A later connect remembers the server again, pin flag and all.
    let mut reconnected = server(&host, "ada");
    reconnected.pinned = true;
    reconnected.display_name = "Renamed".to_string();
    remember(reconnected);

    let found = entries_for(&host);
    assert_eq!(found.len(), 1);
    assert!(!found[0].pinned, "the stored pin wins over whatever the connect passed");
    assert_eq!(found[0].display_name, "Renamed", "every other field still updates");
}

/// A server nobody has saved yet takes the pin the caller asked for, which is how
/// "a new place is pinned on its first successful connect" happens at all.
#[test]
fn a_first_connect_pins_the_new_server() {
    let host = host_for("pin-on-first-connect");

    let mut fresh = server(&host, "ada");
    fresh.pinned = true;
    remember(fresh);

    assert!(entries_for(&host)[0].pinned);
}
