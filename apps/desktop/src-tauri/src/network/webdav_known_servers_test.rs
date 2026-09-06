//! What the server list owes a picker: one entry per account, found again the
//! way the volume id finds it.
//!
//! In-memory only: these run before `load_known_webdav_servers` names a file, so
//! `save()` is a no-op and the cells assert on the lookup rather than the write.
//! The durability of the write itself is `config::durable_write_json`'s contract.

use super::*;

/// The store is process-global, so two cells writing the same pair would see
/// each other. Each uses a host of its own instead of taking turns on a mutex.
fn url_for(cell: &str) -> String {
    format!("https://{cell}.webdav-servers.test/dav/")
}

fn server(url: &str, username: &str) -> KnownWebdavServer {
    KnownWebdavServer {
        url: url.to_string(),
        username: username.to_string(),
        display_name: url.to_string(),
        remote_root: "/".to_string(),
        auto_reconnect: true,
        pinned: false,
        last_connected_at: "2026-09-01T10:00:00Z".to_string(),
    }
}

fn entries_for(url: &str) -> Vec<KnownWebdavServer> {
    let wanted = normalize_url(url);
    all().into_iter().filter(|entry| entry.url == wanted).collect()
}

#[test]
fn a_remembered_server_comes_back() {
    let url = url_for("remembered");
    remember(server(&url, "ada"));

    let found = entries_for(&url);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].username, "ada");
    assert_eq!(found[0].remote_root, "/");
}

#[test]
fn connecting_again_updates_the_entry_rather_than_adding_one() {
    let url = url_for("updated");
    remember(server(&url, "ada"));
    let mut later = server(&url, "ada");
    later.remote_root = "/Photos".to_string();
    later.last_connected_at = "2026-09-02T09:00:00Z".to_string();
    remember(later);

    let found = entries_for(&url);
    assert_eq!(found.len(), 1, "one server and one account is one entry");
    assert_eq!(found[0].remote_root, "/Photos");
}

#[test]
fn two_accounts_on_one_server_are_two_entries() {
    // ❗ The same rule the volume id follows: two accounts see different files
    // under the same paths, so collapsing them would show one server that opens
    // the wrong home directory half the time.
    let url = url_for("two-accounts");
    remember(server(&url, "ada"));
    remember(server(&url, "grace"));

    let mut usernames: Vec<String> = entries_for(&url).into_iter().map(|entry| entry.username).collect();
    usernames.sort();
    assert_eq!(usernames, vec!["ada".to_string(), "grace".to_string()]);
}

#[test]
fn a_trailing_slash_is_not_a_different_server() {
    // A user types the same server with and without its trailing slash on two
    // days; that is one entry, not two.
    let url = url_for("slash");
    remember(server(url.trim_end_matches('/'), "ada"));
    remember(server(&url, "ada"));

    assert_eq!(entries_for(&url).len(), 1, "one server, however it was typed");
}

#[test]
fn the_host_folds_case_and_the_account_does_not() {
    // DNS is case-insensitive; an account may not be. Same split as
    // `webdav_volume_id`, and a drift here would file one volume under two entries.
    let url = url_for("case");
    remember(server(&url, "ada"));
    remember(server(&url.replace("case.webdav", "CASE.WEBDAV"), "ada"));
    assert_eq!(entries_for(&url).len(), 1, "one server, however its host was typed");

    remember(server(&url, "Ada"));
    assert_eq!(entries_for(&url).len(), 2, "`Ada` and `ada` may be two people");
}

#[test]
fn the_path_is_part_of_the_identity() {
    // Two DAV roots on one host are two servers: Nextcloud's `/remote.php/dav/`
    // and an Apache `mod_dav` on `/share/` can sit behind one hostname.
    let url = url_for("paths");
    remember(server(&url, "ada"));
    remember(server(&format!("{url}other/"), "ada"));

    assert_eq!(entries_for(&url).len(), 1);
    assert_eq!(entries_for(&format!("{url}other/")).len(), 1);
}

#[test]
fn forgetting_answers_whether_anything_was_there() {
    let url = url_for("forget");
    remember(server(&url, "ada"));

    assert!(forget(&url, "ada"));
    assert!(entries_for(&url).is_empty());
    assert!(!forget(&url, "ada"), "a second forget has nothing to do");
}

#[test]
fn forgetting_one_account_leaves_the_other_alone() {
    let url = url_for("forget-one");
    remember(server(&url, "ada"));
    remember(server(&url, "grace"));

    assert!(forget(&url, "ada"));

    let found = entries_for(&url);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].username, "grace");
}

#[test]
fn normalizing_adds_the_slash_and_folds_only_the_origin() {
    assert_eq!(
        normalize_url("HTTPS://Dav.Example.test:8443/Remote.php/dav"),
        "https://dav.example.test:8443/Remote.php/dav/"
    );
    assert_eq!(normalize_url("https://dav.example.test"), "https://dav.example.test/");
    assert_eq!(
        normalize_url("  https://dav.example.test/  "),
        "https://dav.example.test/"
    );
}

/// ❗ **A server saved before this setting existed still reconnects
/// automatically.**
#[test]
fn a_server_saved_before_the_setting_existed_still_reconnects_automatically() {
    let stored = r#"{
      "knownWebdavServers": [
        {
          "url": "https://dav.example.test/dav/",
          "username": "ada",
          "displayName": "Example",
          "remoteRoot": "/",
          "lastConnectedAt": "2026-09-01T10:00:00Z"
        }
      ]
    }"#;

    let store: KnownWebdavServersStore = serde_json::from_str(stored).expect("an older file still parses");

    assert!(
        store.known_webdav_servers[0].auto_reconnect,
        "the field wasn't in the file, and the default has to be the behavior that was already shipping"
    );
}

/// The switch survives a round trip through the file, both ways.
#[test]
fn the_switch_round_trips_through_the_stored_file() {
    let mut off = server("https://round-trip.webdav-servers.test/dav/", "ada");
    off.auto_reconnect = false;
    let store = KnownWebdavServersStore {
        known_webdav_servers: vec![off],
    };

    let written = serde_json::to_string(&store).expect("serializable");
    let read: KnownWebdavServersStore = serde_json::from_str(&written).expect("parseable");

    assert!(!read.known_webdav_servers[0].auto_reconnect);
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
      "knownWebdavServers": [
        {
          "url": "https://nextcloud.example/remote.php/dav/files/ada/",
          "username": "ada",
          "displayName": "Nextcloud",
          "remoteRoot": "/",
          "autoReconnect": true,
          "lastConnectedAt": "2026-09-01T10:00:00Z"
        }
      ]
    }"#;

    let store: KnownWebdavServersStore = serde_json::from_str(stored).expect("an older file still parses");

    assert!(
        !store.known_webdav_servers[0].pinned,
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
    let url = url_for("pin-preserved");

    let mut first = server(&url, "ada");
    first.pinned = true;
    remember(first);

    // The user unpins it. Mutated in place under the lock, the way the writer for
    // that will: the store is process-global, and rebuilding the whole vec here
    // would drop entries other cells are appending in parallel.
    let normalized = normalize_url(&url);
    {
        let mut store = known().lock_ignore_poison();
        for entry in store
            .known_webdav_servers
            .iter_mut()
            .filter(|entry| same_server(entry, &normalized, "ada"))
        {
            entry.pinned = false;
        }
    }

    // A later connect remembers the server again, pin flag and all.
    let mut reconnected = server(&url, "ada");
    reconnected.pinned = true;
    reconnected.display_name = "Renamed".to_string();
    remember(reconnected);

    let found = entries_for(&url);
    assert_eq!(found.len(), 1);
    assert!(!found[0].pinned, "the stored pin wins over whatever the connect passed");
    assert_eq!(found[0].display_name, "Renamed", "every other field still updates");
}

/// A server nobody has saved yet takes the pin the caller asked for, which is how
/// "a new place is pinned on its first successful connect" happens at all.
#[test]
fn a_first_connect_pins_the_new_server() {
    let url = url_for("pin-on-first-connect");

    let mut fresh = server(&url, "ada");
    fresh.pinned = true;
    remember(fresh);

    assert!(entries_for(&url)[0].pinned);
}
