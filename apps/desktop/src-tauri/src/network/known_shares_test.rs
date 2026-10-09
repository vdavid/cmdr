//! Unit tests for `known_shares.rs`: the store's keying, the username hint, and the
//! per-share direct-connection switch.

use super::*;

/// Tests that mutate the global `KNOWN_SHARES` static must hold this lock
/// to prevent cross-test interference (Rust runs tests in parallel).
static SERIAL: Mutex<()> = Mutex::new(());

fn naspolya() -> NetworkHost {
    NetworkHost {
        id: "naspolya-smb-tcp-local".to_string(),
        name: "Naspolya".to_string(),
        hostname: Some("Naspolya.local".to_string()),
        ip_address: Some("192.168.1.111".to_string()),
        port: 445,
        source: crate::network::HostSource::Discovered,
    }
}

/// A store saved before the setting existed has no opt-out list at all, and
/// every share in it must stay on the fast connection: a missing field can't be
/// what switches the behavior off.
#[test]
fn a_store_saved_before_the_setting_existed_leaves_every_share_on_the_fast_connection() {
    let old = r#"{"knownNetworkShares":[{"serverName":"Naspolya","shareName":"naspi","protocol":"smb","lastConnectedAt":"2026-01-06T12:00:00Z","lastConnectionMode":"guest","lastKnownAuthOptions":"guest_only","username":null}]}"#;

    let store: KnownSharesStore = serde_json::from_str(old).expect("an old store still loads");

    assert_eq!(store.known_network_shares.len(), 1, "the old rows survive");
    assert!(!is_opted_out(
        &store.direct_connection_opt_outs,
        &["Naspolya"],
        "naspi",
        &[]
    ));
}

/// `statfs` echoes whichever name form each mount used, so one NAS arrives as
/// an IP on one mount and as its mDNS service name on the next. A choice made
/// under one spelling that the other can't see looks like the toggle resetting
/// itself.
#[test]
fn an_opt_out_holds_under_every_name_form_of_its_server() {
    let hosts = [naspolya()];
    let mut opt_outs = Vec::new();

    apply_choice(&mut opt_outs, "192.168.1.111", "naspi", false, &hosts);

    for form in [
        "192.168.1.111",
        "Naspolya._smb._tcp.local",
        "naspolya.local",
        "Naspolya",
    ] {
        assert!(
            is_opted_out(&opt_outs, &[form], "naspi", &hosts),
            "the opt-out is invisible under {form:?}"
        );
    }
    // The share name folds case and NFC, the way `share_key` does.
    assert!(is_opted_out(&opt_outs, &["Naspolya"], "NASPI", &hosts));
    // Another share on the same server keeps its own answer, and so does another server.
    assert!(!is_opted_out(&opt_outs, &["Naspolya"], "Multimedia", &hosts));
    assert!(!is_opted_out(&opt_outs, &["raspberrypi.local"], "naspi", &hosts));
}

/// The auto path knows a mount's server twice over (the `statfs` spelling and the
/// address it's about to dial), and either one matching is enough.
#[test]
fn any_of_the_callers_server_names_can_match() {
    let hosts = [naspolya()];
    let mut opt_outs = Vec::new();
    apply_choice(&mut opt_outs, "Naspolya._smb._tcp.local", "naspi", false, &hosts);

    assert!(is_opted_out(
        &opt_outs,
        &["somewhere-else", "192.168.1.111"],
        "naspi",
        &hosts
    ));
}

/// Choosing again under another spelling replaces the entry rather than adding a
/// second, and turning it back on clears it under any spelling.
#[test]
fn one_share_has_one_entry_whatever_it_is_called() {
    let hosts = [naspolya()];
    let mut opt_outs = Vec::new();

    apply_choice(&mut opt_outs, "192.168.1.111", "naspi", false, &hosts);
    apply_choice(&mut opt_outs, "Naspolya._smb._tcp.local", "naspi", false, &hosts);
    assert_eq!(opt_outs.len(), 1, "one share, one entry: {opt_outs:?}");

    apply_choice(&mut opt_outs, "naspolya.local", "NASPI", true, &hosts);
    assert!(opt_outs.is_empty(), "turning it back on clears it: {opt_outs:?}");
    assert!(!is_opted_out(&opt_outs, &["192.168.1.111"], "naspi", &hosts));
}

#[test]
fn test_share_key() {
    assert_eq!(share_key("MyNAS", "Documents"), "mynas/documents");
    assert_eq!(share_key("server.local", "Media"), "server.local/media");
}

/// One accented share reaches this store composed (the frontend's share list)
/// and decomposed (`statfs` on the mount), so a byte-keyed store remembers it
/// twice and the auth mode saved on one spelling is missing on the other.
/// Reported as ERR-ABXW4.
#[test]
fn share_key_folds_unicode_normalization() {
    assert_eq!(
        share_key("Szabolcs-DS224", "R\u{e9}gi NAS"),
        share_key("Szabolcs-DS224", "Re\u{301}gi NAS")
    );
}

#[test]
fn test_connection_mode_serialization() {
    let guest = ConnectionMode::Guest;
    let creds = ConnectionMode::Credentials;

    assert_eq!(serde_json::to_string(&guest).unwrap(), r#""guest""#);
    assert_eq!(serde_json::to_string(&creds).unwrap(), r#""credentials""#);

    let guest_back: ConnectionMode = serde_json::from_str(r#""guest""#).unwrap();
    assert_eq!(guest_back, ConnectionMode::Guest);
}

#[test]
fn test_auth_options_serialization() {
    let guest_only = AuthOptions::GuestOnly;
    let creds_only = AuthOptions::CredentialsOnly;
    let both = AuthOptions::GuestOrCredentials;

    assert_eq!(serde_json::to_string(&guest_only).unwrap(), r#""guest_only""#);
    assert_eq!(serde_json::to_string(&creds_only).unwrap(), r#""credentials_only""#);
    assert_eq!(serde_json::to_string(&both).unwrap(), r#""guest_or_credentials""#);
}

#[test]
fn test_known_share_serialization() {
    let share = KnownNetworkShare {
        server_name: "Alpha".to_string(),
        share_name: "Documents".to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-01-03T21:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Credentials,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: Some("david".to_string()),
        address: None,
        port: None,
        volume_id: None,
        mount_path: None,
        pinned: false,
    };

    let json = serde_json::to_string_pretty(&share).unwrap();
    assert!(json.contains(r#""serverName": "Alpha""#));
    assert!(json.contains(r#""shareName": "Documents""#));
    assert!(json.contains(r#""lastConnectionMode": "credentials""#));
    assert!(json.contains(r#""lastKnownAuthOptions": "guest_or_credentials""#));
    assert!(json.contains(r#""username": "david""#));

    // Round-trip
    let parsed: KnownNetworkShare = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.server_name, "Alpha");
    assert_eq!(parsed.share_name, "Documents");
    assert_eq!(parsed.last_connection_mode, ConnectionMode::Credentials);
}

#[test]
fn test_store_serialization() {
    let store = KnownSharesStore {
        known_network_shares: vec![
            KnownNetworkShare {
                server_name: "Alpha".to_string(),
                share_name: "Documents".to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-03T21:00:00Z".to_string(),
                last_connection_mode: ConnectionMode::Credentials,
                last_known_auth_options: AuthOptions::GuestOrCredentials,
                username: Some("david".to_string()),
                address: None,
                port: None,
                volume_id: None,
                mount_path: None,
                pinned: false,
            },
            KnownNetworkShare {
                server_name: "Bravo".to_string(),
                share_name: "media".to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-02T15:30:00Z".to_string(),
                last_connection_mode: ConnectionMode::Guest,
                last_known_auth_options: AuthOptions::GuestOnly,
                username: None,
                address: None,
                port: None,
                volume_id: None,
                mount_path: None,
                pinned: false,
            },
        ],
        direct_connection_opt_outs: Vec::new(),
    };

    let json = serde_json::to_string_pretty(&store).unwrap();
    assert!(json.contains("knownNetworkShares"));

    // Round-trip
    let parsed: KnownSharesStore = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.known_network_shares.len(), 2);
}

#[test]
fn test_in_memory_operations() {
    let _guard = SERIAL.lock().unwrap();
    // Test the in-memory cache operations directly
    let cache = get_known_shares_mutex();

    // Clear any previous state
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }

    // Get all should return empty
    let all = get_all_known_shares();
    assert!(all.is_empty());

    // Add a share directly to cache (simulating update without app handle)
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.push(KnownNetworkShare {
            server_name: "TestServer".to_string(),
            share_name: "TestShare".to_string(),
            protocol: "smb".to_string(),
            last_connected_at: "2026-01-06T12:00:00Z".to_string(),
            last_connection_mode: ConnectionMode::Guest,
            last_known_auth_options: AuthOptions::GuestOnly,
            username: None,
            address: None,
            port: None,
            volume_id: None,
            mount_path: None,
            pinned: false,
        });
    }

    // Should find it now
    let found = get_known_share("TestServer", "TestShare");
    assert!(found.is_some());
    assert_eq!(found.unwrap().share_name, "TestShare");

    // Case-insensitive lookup
    let found_lower = get_known_share("testserver", "testshare");
    assert!(found_lower.is_some());

    // Clean up
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

#[test]
fn test_username_hints() {
    let _guard = SERIAL.lock().unwrap();
    let cache = get_known_shares_mutex();

    // Clear and set up test data
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
        c.known_network_shares.push(KnownNetworkShare {
            server_name: "Server1".to_string(),
            share_name: "Share1".to_string(),
            protocol: "smb".to_string(),
            last_connected_at: "2026-01-06T12:00:00Z".to_string(),
            last_connection_mode: ConnectionMode::Credentials,
            last_known_auth_options: AuthOptions::CredentialsOnly,
            username: Some("alice".to_string()),
            address: None,
            port: None,
            volume_id: None,
            mount_path: None,
            pinned: false,
        });
        c.known_network_shares.push(KnownNetworkShare {
            server_name: "Server2".to_string(),
            share_name: "Share2".to_string(),
            protocol: "smb".to_string(),
            last_connected_at: "2026-01-06T12:00:00Z".to_string(),
            last_connection_mode: ConnectionMode::Guest,
            last_known_auth_options: AuthOptions::GuestOnly,
            username: None,
            address: None,
            port: None,
            volume_id: None,
            mount_path: None,
            pinned: false,
        });
    }

    assert_eq!(get_username_hint("Server1"), Some("alice".to_string()));
    assert_eq!(get_username_hint("Server2"), None); // No username for guest-only
    assert_eq!(get_username_hint("nobody-here"), None);

    // Clean up
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

/// The hint has to survive the server arriving under another of its names. The
/// login form opens on whatever `NetworkHost` discovery produced (an mDNS instance
/// name), while the share was saved under the name the connect used, so a lookup
/// that compared raw strings prefilled nothing for the exact case it exists for.
#[test]
fn a_username_hint_is_found_under_every_name_form_of_its_server() {
    let _guard = SERIAL.lock().unwrap();
    let cache = get_known_shares_mutex();

    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
        c.known_network_shares.push(KnownNetworkShare {
            server_name: "Naspolya".to_string(),
            share_name: "naspi".to_string(),
            protocol: "smb".to_string(),
            last_connected_at: "2026-01-06T12:00:00Z".to_string(),
            last_connection_mode: ConnectionMode::Credentials,
            last_known_auth_options: AuthOptions::CredentialsOnly,
            username: Some("david".to_string()),
            address: None,
            port: None,
            volume_id: None,
            mount_path: None,
            pinned: false,
        });
    }

    for form in [
        "Naspolya",
        "naspolya",
        "Naspolya.local",
        "naspolya.local.",
        "Naspolya._smb._tcp.local",
    ] {
        assert_eq!(
            get_username_hint(form),
            Some("david".to_string()),
            "no hint found for the server spelled {form:?}"
        );
    }
    // A different server keeps its own answer.
    assert_eq!(get_username_hint("raspberrypi.local"), None);

    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

/// Shares are appended in connect order, so the LAST one carrying a username is the
/// most recent thing the person actually signed in as.
#[test]
fn the_newest_username_on_a_server_wins() {
    let _guard = SERIAL.lock().unwrap();
    let cache = get_known_shares_mutex();

    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
        for (share, user) in [("old", Some("alice")), ("newer", Some("bob")), ("guest", None)] {
            c.known_network_shares.push(KnownNetworkShare {
                server_name: "Naspolya".to_string(),
                share_name: share.to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-06T12:00:00Z".to_string(),
                last_connection_mode: ConnectionMode::Credentials,
                last_known_auth_options: AuthOptions::CredentialsOnly,
                username: user.map(str::to_string),
                address: None,
                port: None,
                volume_id: None,
                mount_path: None,
                pinned: false,
            });
        }
    }

    assert_eq!(get_username_hint("naspolya"), Some("bob".to_string()));

    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

/// Concurrent threads adding distinct shares must not lose any writes.
#[test]
fn concurrent_in_memory_updates_no_lost_writes() {
    let _guard = SERIAL.lock().unwrap();
    let cache = get_known_shares_mutex();

    // Clear previous state
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }

    let thread_count = 20;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
    let mut handles = Vec::new();

    for i in 0..thread_count {
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait(); // All threads start at the same time
            let key = format!("server-{}", i);
            if let Ok(mut c) = get_known_shares_mutex().lock() {
                c.known_network_shares.push(KnownNetworkShare {
                    server_name: key.clone(),
                    share_name: "share".to_string(),
                    protocol: "smb".to_string(),
                    last_connected_at: "2026-01-01T00:00:00Z".to_string(),
                    last_connection_mode: ConnectionMode::Guest,
                    last_known_auth_options: AuthOptions::GuestOnly,
                    username: None,
                    address: None,
                    port: None,
                    volume_id: None,
                    mount_path: None,
                    pinned: false,
                });
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let all = get_all_known_shares();
    assert_eq!(
        all.len(),
        thread_count,
        "Expected {} shares but got {}. A concurrent write was lost.",
        thread_count,
        all.len()
    );

    // Clean up
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

/// Concurrent reads while another thread writes should not panic or return corrupt data.
#[test]
fn concurrent_read_during_write() {
    let _guard = SERIAL.lock().unwrap();
    let cache = get_known_shares_mutex();

    // Seed with initial data
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
        c.known_network_shares.push(KnownNetworkShare {
            server_name: "seed".to_string(),
            share_name: "share".to_string(),
            protocol: "smb".to_string(),
            last_connected_at: "2026-01-01T00:00:00Z".to_string(),
            last_connection_mode: ConnectionMode::Guest,
            last_known_auth_options: AuthOptions::GuestOnly,
            username: None,
            address: None,
            port: None,
            volume_id: None,
            mount_path: None,
            pinned: false,
        });
    }

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut handles = Vec::new();

    // Writer thread: adds shares
    let b = barrier.clone();
    handles.push(std::thread::spawn(move || {
        b.wait();
        for i in 0..50 {
            if let Ok(mut c) = get_known_shares_mutex().lock() {
                c.known_network_shares.push(KnownNetworkShare {
                    server_name: format!("writer-{}", i),
                    share_name: "share".to_string(),
                    protocol: "smb".to_string(),
                    last_connected_at: "2026-01-01T00:00:00Z".to_string(),
                    last_connection_mode: ConnectionMode::Guest,
                    last_known_auth_options: AuthOptions::GuestOnly,
                    username: None,
                    address: None,
                    port: None,
                    volume_id: None,
                    mount_path: None,
                    pinned: false,
                });
            }
        }
    }));

    // Two reader threads: read all shares repeatedly
    for _ in 0..2 {
        let b = barrier.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            for _ in 0..100 {
                let shares = get_all_known_shares();
                // Must always have at least the seed share
                assert!(!shares.is_empty(), "Read returned empty during concurrent write");
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    // Clean up
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

/// Rapid sequential updates to the same share should keep the last value.
#[test]
fn rapid_sequential_updates_same_share() {
    let _guard = SERIAL.lock().unwrap();
    let cache = get_known_shares_mutex();

    // Clear previous state
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }

    let iterations = 100;
    for i in 0..iterations {
        if let Ok(mut c) = cache.lock() {
            let key = share_key("rapid-server", "rapid-share");
            if let Some(existing) = c
                .known_network_shares
                .iter_mut()
                .find(|s| share_key(&s.server_name, &s.share_name) == key)
            {
                existing.last_connected_at = format!("2026-01-01T00:00:{:02}Z", i);
                existing.username = Some(format!("user-{}", i));
            } else {
                c.known_network_shares.push(KnownNetworkShare {
                    server_name: "rapid-server".to_string(),
                    share_name: "rapid-share".to_string(),
                    protocol: "smb".to_string(),
                    last_connected_at: format!("2026-01-01T00:00:{:02}Z", i),
                    last_connection_mode: ConnectionMode::Credentials,
                    last_known_auth_options: AuthOptions::GuestOrCredentials,
                    username: Some(format!("user-{}", i)),
                    address: None,
                    port: None,
                    volume_id: None,
                    mount_path: None,
                    pinned: false,
                });
            }
        }
    }

    let share = get_known_share("rapid-server", "rapid-share").expect("share should exist");
    assert_eq!(share.username, Some(format!("user-{}", iterations - 1)));

    // Only one entry should exist (upsert, not duplicate)
    let all = get_all_known_shares();
    let rapid_count = all.iter().filter(|s| s.server_name == "rapid-server").count();
    assert_eq!(rapid_count, 1, "Rapid updates should not create duplicate entries");

    // Clean up
    if let Ok(mut c) = cache.lock() {
        c.known_network_shares.clear();
    }
}

// -- Saved share rows --

/// A share row as a mount through Cmdr files it.
fn mounted(
    server_name: &str,
    address: &str,
    share: &str,
    username: Option<&str>,
    volume_id: &str,
) -> KnownNetworkShare {
    KnownNetworkShare {
        server_name: server_name.to_string(),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-09-24T10:00:00Z".to_string(),
        last_connection_mode: if username.is_some() {
            ConnectionMode::Credentials
        } else {
            ConnectionMode::Guest
        },
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: username.map(str::to_string),
        address: Some(address.to_string()),
        port: None,
        volume_id: Some(volume_id.to_string()),
        mount_path: Some(format!("/Volumes/{share}")),
        pinned: false,
    }
}

/// A share row as Add files it: named in `smb://user@host/share`, never mounted.
fn added(server_name: &str, share: &str, username: Option<&str>) -> KnownNetworkShare {
    KnownNetworkShare {
        address: None,
        volume_id: None,
        mount_path: None,
        ..mounted(server_name, server_name, share, username, "unused")
    }
}

/// ❗ **One row per share, whatever account opened it and whichever name the
/// server went by.** An SMB volume id carries no username, so two rows for one
/// share would be two saved places over one volume id.
#[test]
fn a_share_is_one_row_under_every_account_and_every_name_of_its_server() {
    let hosts = [naspolya()];
    let mut rows = Vec::new();

    upsert_share_row(
        &mut rows,
        mounted("Naspolya", "192.168.1.111", "naspi", Some("david"), "smb-a"),
        &hosts,
    );
    upsert_share_row(
        &mut rows,
        mounted("192.168.1.111", "192.168.1.111", "NASPI", None, "smb-a"),
        &hosts,
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].username, None, "the row remembers who opened it LAST");
}

/// ❗ **A share's first mount pins it** (rule 1 of the servers model); later
/// mounts leave the pin where the person put it.
#[test]
fn the_first_mount_pins_a_share_and_later_ones_keep_the_persons_pin() {
    let mut rows = Vec::new();

    upsert_share_row(&mut rows, added("192.168.0.153", "Container", Some("sven")), &[]);
    assert!(!rows[0].pinned, "an add alone mounted nothing");

    upsert_share_row(
        &mut rows,
        mounted("192.168.0.153", "192.168.0.153", "Container", Some("sven"), "smb-c"),
        &[],
    );
    assert!(rows[0].pinned, "the first mount pins it");

    rows[0].pinned = false; // the person unpinned it
    upsert_share_row(
        &mut rows,
        mounted("192.168.0.153", "192.168.0.153", "Container", Some("sven"), "smb-c"),
        &[],
    );
    assert!(!rows[0].pinned, "an unpin survives the next mount");
}

/// An add of a share that was mounted before keeps what the mount learned: the
/// add names no volume id, mount path, or address.
#[test]
fn re_adding_a_mounted_share_keeps_its_place() {
    let mut rows = Vec::new();
    upsert_share_row(&mut rows, mounted("nas", "10.0.0.2", "photos", None, "smb-p"), &[]);

    upsert_share_row(&mut rows, added("nas", "photos", Some("ada")), &[]);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].volume_id.as_deref(), Some("smb-p"));
    assert_eq!(rows[0].mount_path.as_deref(), Some("/Volumes/photos"));
    assert_eq!(rows[0].address.as_deref(), Some("10.0.0.2"));
    assert_eq!(rows[0].username.as_deref(), Some("ada"));
}

/// ❗ **An Add never erases a known account.** A plain re-Add of
/// `smb://localhost:11481/private` filed the share "as nobody", and its next open
/// asked for a password that was stored all along (QA round 6). An account the Add
/// names still replaces it; a MOUNT still records who opened it last.
#[test]
fn a_plain_re_add_keeps_the_account_the_share_is_saved_with() {
    let mut rows = Vec::new();
    upsert_share_row(
        &mut rows,
        mounted("nas", "10.0.0.2", "private", Some("testuser"), "smb-p"),
        &[],
    );

    upsert_share_row(&mut rows, added("nas", "private", None), &[]);
    assert_eq!(rows[0].username.as_deref(), Some("testuser"));
    assert_eq!(rows[0].last_connection_mode, ConnectionMode::Credentials);

    upsert_share_row(&mut rows, added("nas", "private", Some("ada")), &[]);
    assert_eq!(rows[0].username.as_deref(), Some("ada"), "a named account replaces it");
}

/// A share row never replaces the host's sign-in history, and the history never
/// replaces a share.
#[test]
fn a_share_row_and_the_hosts_history_are_separate_rows() {
    let mut rows = vec![KnownNetworkShare {
        share_name: String::new(),
        address: None,
        volume_id: None,
        mount_path: None,
        ..mounted("nas", "nas", "x", Some("ada"), "unused")
    }];

    upsert_share_row(&mut rows, mounted("nas", "10.0.0.2", "photos", None, "smb-p"), &[]);

    assert_eq!(rows.len(), 2);
    assert!(!rows[0].is_share());
    assert!(rows[1].is_share());
}

/// ❗ **Forget takes exactly the rows it was handed**, and nothing that merely
/// shares a name with them: here the same share name on the same machine, but
/// another server (port).
#[test]
fn forgetting_rows_takes_exactly_those_rows() {
    let other_port = KnownNetworkShare {
        port: Some(11480),
        ..mounted("localhost", "localhost", "public", None, "smb-a")
    };
    let mine = KnownNetworkShare {
        port: Some(11482),
        ..mounted("localhost", "localhost", "public", None, "smb-b")
    };
    let mut rows = vec![other_port.clone(), mine.clone()];

    let removed = forget_rows_in(&mut rows, &[mine]);

    assert_eq!(removed, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].volume_id.as_deref(), Some("smb-a"));
}

/// ❗ **Two servers on one machine hold two rows for a share of the same name.**
/// The Docker fixtures all share `public` on `localhost`, one port each; folding
/// them into one row handed the second mount the first one's place.
#[test]
fn a_share_on_another_port_of_the_same_machine_is_another_row() {
    let mut rows = Vec::new();
    let on = |port: u16, volume_id: &str| KnownNetworkShare {
        port: Some(port),
        ..mounted("localhost", "localhost", "public", None, volume_id)
    };

    upsert_share_row(&mut rows, on(11480, "smb-a"), &[]);
    upsert_share_row(&mut rows, on(11482, "smb-b"), &[]);

    assert_eq!(rows.len(), 2);
}

/// ❗ **A share an Add named is found by the id the servers listing shows for
/// it**, so Forget share and Pin work on it before any mount went through.
#[test]
fn a_share_no_mount_went_through_is_found_by_its_listed_id() {
    let row = added("192.168.0.153", "Container", Some("sven"));

    assert_eq!(
        place_id(&row),
        cmdr_fs::volume::smb_volume_id("192.168.0.153", 445, "Container")
    );
}

/// An old store row reads with no place fields and unpinned.
#[test]
fn a_row_saved_before_share_places_reads_as_the_hosts_history() {
    let old = r#"{"serverName":"Naspolya","shareName":"","protocol":"smb","lastConnectedAt":"2026-01-06T12:00:00Z","lastConnectionMode":"guest","lastKnownAuthOptions":"guest_only","username":null}"#;
    let row: KnownNetworkShare = serde_json::from_str(old).unwrap();
    assert!(!row.is_share());
    assert_eq!(row.volume_id, None);
    assert!(!row.pinned);
}

/// ❗ **Favoriting a folder on a share Cmdr didn't mount saves the share, unpinned**
/// (`docs/specs/saved-smb-shares.md` writer 4): a Finder-mounted share has no row,
/// so once it unmounts nothing could dial the favorite. Unpinned, because a
/// favorite is no request to crowd the switcher.
#[test]
fn a_favorited_share_nobody_saved_gets_an_unpinned_row_with_its_place() {
    let mut rows = Vec::new();

    let added = insert_unless_saved(
        &mut rows,
        mounted("192.0.2.9", "192.0.2.9", "naspi", Some("david"), "smb-n"),
        &[],
    );

    assert!(added);
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].pinned, "a favorite doesn't pin its share");
    assert_eq!(rows[0].volume_id.as_deref(), Some("smb-n"));
    assert_eq!(rows[0].mount_path.as_deref(), Some("/Volumes/naspi"));
}

/// A share that's already saved keeps everything the person chose: its pin and
/// the account it's saved under.
#[test]
fn a_favorited_share_already_saved_is_left_alone() {
    let hosts = [naspolya()];
    let mut rows = Vec::new();
    upsert_share_row(
        &mut rows,
        mounted("Naspolya", "192.168.1.111", "naspi", Some("david"), "smb-a"),
        &hosts,
    );
    rows[0].pinned = true;

    let added = insert_unless_saved(
        &mut rows,
        mounted("192.168.1.111", "192.168.1.111", "naspi", None, "smb-a"),
        &hosts,
    );

    assert!(!added);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].pinned);
    assert_eq!(rows[0].username.as_deref(), Some("david"));
    assert_eq!(rows[0].server_name, "Naspolya");
}
