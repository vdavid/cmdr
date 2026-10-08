//! Tests for `manual_servers.rs`: address parsing, ids, the store, and its concurrency.

use super::account::set_account_at_path;
use super::*;

// -- parse_server_address: all input formats --

#[test]
fn parse_bare_ip() {
    let r = parse_server_address("192.168.1.100").unwrap();
    assert_eq!(r.host, "192.168.1.100");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_ip_with_port() {
    let r = parse_server_address("192.168.1.100:9445").unwrap();
    assert_eq!(r.host, "192.168.1.100");
    assert_eq!(r.port, 9445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_bare_hostname() {
    let r = parse_server_address("mynas").unwrap();
    assert_eq!(r.host, "mynas");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_hostname_with_underscore() {
    let r = parse_server_address("my_nas").unwrap();
    assert_eq!(r.host, "my_nas");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_hostname_with_domain() {
    let r = parse_server_address("mynas.local").unwrap();
    assert_eq!(r.host, "mynas.local");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_smb_url_basic() {
    let r = parse_server_address("smb://mynas").unwrap();
    assert_eq!(r.host, "mynas");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_smb_url_with_port() {
    let r = parse_server_address("smb://mynas:9445").unwrap();
    assert_eq!(r.host, "mynas");
    assert_eq!(r.port, 9445);
    assert_eq!(r.share_path, None);
}

#[test]
fn parse_smb_url_with_share() {
    let r = parse_server_address("smb://mynas/docs").unwrap();
    assert_eq!(r.host, "mynas");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, Some("docs".to_string()));
}

#[test]
fn parse_smb_url_with_user() {
    let r = parse_server_address("smb://user@mynas/docs").unwrap();
    assert_eq!(r.host, "mynas");
    assert_eq!(r.port, 445);
    assert_eq!(r.share_path, Some("docs".to_string()));
}

#[test]
fn parse_smb_url_with_port_and_share() {
    let r = parse_server_address("smb://mynas:9445/docs").unwrap();
    assert_eq!(r.host, "mynas");
    assert_eq!(r.port, 9445);
    assert_eq!(r.share_path, Some("docs".to_string()));
}

#[test]
fn parse_smb_url_trailing_slash() {
    let r = parse_server_address("smb://mynas/docs/").unwrap();
    assert_eq!(r.share_path, Some("docs".to_string()));
}

#[test]
fn parse_with_whitespace() {
    let r = parse_server_address("  192.168.1.100  ").unwrap();
    assert_eq!(r.host, "192.168.1.100");
}

#[test]
fn parse_uppercase_smb() {
    let r = parse_server_address("SMB://MyNas").unwrap();
    assert_eq!(r.host, "MyNas");
    assert_eq!(r.port, 445);
}

// -- parse_server_address: error cases --

#[test]
fn parse_empty() {
    assert_eq!(parse_server_address(""), Err(ParseError::Empty));
    assert_eq!(parse_server_address("  "), Err(ParseError::Empty));
}

#[test]
fn parse_unsupported_protocols() {
    assert!(matches!(parse_server_address("afp://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "afp"));
    assert!(matches!(parse_server_address("nfs://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "nfs"));
    assert!(matches!(parse_server_address("ftp://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "ftp"));
    assert!(matches!(parse_server_address("vnc://mynas"), Err(ParseError::UnsupportedProtocol(p)) if p == "vnc"));
}

#[test]
fn parse_ipv6_rejected() {
    assert_eq!(parse_server_address("[::1]:9445"), Err(ParseError::Ipv6NotSupported));
    assert_eq!(parse_server_address("fe80::1"), Err(ParseError::Ipv6NotSupported));
    assert_eq!(parse_server_address("::1"), Err(ParseError::Ipv6NotSupported));
}

#[test]
fn parse_port_out_of_range() {
    assert!(matches!(
        parse_server_address("mynas:0"),
        Err(ParseError::InvalidPort(_))
    ));
    assert!(matches!(
        parse_server_address("mynas:65536"),
        Err(ParseError::InvalidPort(_))
    ));
    assert!(matches!(
        parse_server_address("mynas:99999"),
        Err(ParseError::InvalidPort(_))
    ));
}

#[test]
fn parse_port_not_a_number() {
    assert!(matches!(
        parse_server_address("mynas:abc"),
        Err(ParseError::InvalidPort(_))
    ));
}

#[test]
fn parse_malformed_smb_url() {
    assert!(matches!(parse_server_address("smb://"), Err(ParseError::Malformed(_))));
}

#[test]
fn parse_invalid_characters() {
    assert!(matches!(parse_server_address("my nas"), Err(ParseError::Malformed(_))));
    assert!(matches!(parse_server_address("my@nas"), Err(ParseError::Malformed(_))));
}

// -- ID generation --

#[test]
fn id_deterministic() {
    let id1 = generate_server_id("192.168.1.100", 9445);
    let id2 = generate_server_id("192.168.1.100", 9445);
    assert_eq!(id1, id2);
    assert_eq!(id1, "manual-192-168-1-100-9445");
}

#[test]
fn id_different_ports() {
    let id1 = generate_server_id("mynas", 445);
    let id2 = generate_server_id("mynas", 9445);
    assert_ne!(id1, id2);
}

#[test]
fn id_format_ip() {
    assert_eq!(generate_server_id("192.168.1.100", 445), "manual-192-168-1-100-445");
}

#[test]
fn id_format_hostname() {
    assert_eq!(generate_server_id("mynas", 445), "manual-mynas-445");
}

#[test]
fn id_format_hostname_with_local() {
    assert_eq!(generate_server_id("mynas.local", 445), "manual-mynas-local-445");
}

// -- Names and accounts --

/// ❗ **The account an `smb://` address names is what the person means to sign
/// in as**, so the add records it. A password in the address is never kept.
#[test]
fn an_smb_url_names_its_account_and_never_its_password() {
    assert_eq!(
        parse_server_address("smb://sven@192.168.0.153/Container")
            .unwrap()
            .username
            .as_deref(),
        Some("sven")
    );
    assert_eq!(
        parse_server_address("smb://sven:hunter2@naspolya")
            .unwrap()
            .username
            .as_deref(),
        Some("sven")
    );
    assert_eq!(parse_server_address("smb://naspolya").unwrap().username, None);
    assert_eq!(parse_server_address("naspolya").unwrap().username, None);
}

/// An entry written before accounts existed reads as having none.
#[test]
fn an_entry_without_an_account_reads_as_none() {
    let json =
        r#"{"id":"manual-nas-445","displayName":"","address":"nas","port":445,"addedAt":"2026-09-17T10:00:00Z"}"#;
    let entry: ManualServerEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.username, None);
}

/// ❗ **The typed account is found under any name the host goes by**: the
/// sign-in sheet opens on whatever the discovery list calls the machine, which
/// for a host typed as an IP can be its Bonjour name. On the entry's own port only.
#[test]
fn the_typed_account_is_found_under_the_address_the_label_and_the_bonjour_name() {
    let mut entry = test_entry(21);
    entry.username = Some("sven".to_string());
    entry.port = 9445;
    let entries = vec![entry];
    let bonjour = NetworkHost {
        id: "mars".to_string(),
        name: "Mars".to_string(),
        hostname: Some("mars.local".to_string()),
        ip_address: Some("10.0.0.21".to_string()),
        port: 445,
        source: HostSource::Discovered,
    };

    let on = |host: &str, port: u16| SmbServer::new(host, port);
    assert_eq!(
        typed_username_in(&entries, &on("10.0.0.21", 9445), &[]).as_deref(),
        Some("sven")
    );
    assert_eq!(
        typed_username_in(&entries, &SmbServer::from_name("10.0.0.21:9445"), &[]).as_deref(),
        Some("sven")
    );
    assert_eq!(
        typed_username_in(&entries, &on("Mars", 9445), &[bonjour]).as_deref(),
        Some("sven")
    );
    assert_eq!(typed_username_in(&entries, &on("10.0.0.22", 9445), &[]), None);
}

/// ❗ **An edit can change the account an SMB host is used with**: for SMB it
/// is a preference, not the entry's identity (the address is).
#[test]
fn naming_a_host_can_set_and_clear_its_account() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    add_server_entry_to_path(&path, test_entry(22));

    let edit = HostEdit {
        name: "Attic".to_string(),
        username: Some("sven".to_string()),
    };
    let stored = name_server_entry_at_path(&path, &test_entry(22).id, "10.0.0.22", 445, &edit).unwrap();
    assert_eq!(stored.username.as_deref(), Some("sven"));

    let cleared = HostEdit {
        name: "Attic".to_string(),
        username: None,
    };
    let stored = name_server_entry_at_path(&path, &test_entry(22).id, "10.0.0.22", 445, &cleared).unwrap();
    assert_eq!(stored.username, None);
}

/// Re-adding a host without an account keeps the one it had, the way it keeps
/// a name.
#[test]
fn re_adding_a_host_without_an_account_keeps_the_one_it_had() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    let mut first = test_entry(23);
    first.username = Some("sven".to_string());
    add_server_entry_to_path(&path, first);
    add_server_entry_to_path(&path, test_entry(23));

    assert_eq!(read_store_from_path(&path).servers[0].username.as_deref(), Some("sven"));
}

/// ❗ **An entry written before names existed reads as unnamed.** Its
/// `displayName` holds the derived address, which no person chose, so the hub
/// must keep ranking a Bonjour name above it.
#[test]
fn an_entry_whose_name_is_its_own_address_is_unnamed() {
    let json = r#"{"id":"manual-192-168-0-153-445","displayName":"192.168.0.153","address":"192.168.0.153","port":445,"addedAt":"2026-09-17T10:00:00Z"}"#;
    let entry: ManualServerEntry = serde_json::from_str(json).unwrap();
    assert!(!entry.is_named());
    assert_eq!(entry.label(), "192.168.0.153");
}

#[test]
fn an_entry_with_a_typed_name_is_labelled_by_it() {
    let mut entry = test_entry(7);
    entry.display_name = "  Sven's NAS ".to_string();
    assert!(entry.is_named());
    assert_eq!(entry.label(), "Sven's NAS");
}

#[test]
fn an_empty_name_falls_back_to_the_address_and_port() {
    let mut entry = test_entry(8);
    entry.display_name = String::new();
    entry.port = 9445;
    assert!(!entry.is_named());
    assert_eq!(entry.label(), "10.0.0.8:9445");
}

/// Renaming writes the one entry it names and nothing else, and says whether it
/// found one.
#[test]
fn renaming_an_entry_rewrites_only_its_name() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    add_server_entry_to_path(&path, test_entry(1));
    add_server_entry_to_path(&path, test_entry(2));

    assert!(name_server_entry_at_path(&path, &test_entry(1).id, "10.0.0.1", 445, &named("Attic NAS")).is_some());

    let store = read_store_from_path(&path);
    let renamed = store.servers.iter().find(|s| s.id == test_entry(1).id).unwrap();
    assert_eq!(renamed.label(), "Attic NAS");
    assert_eq!(renamed.address, "10.0.0.1", "the address is identity and stays");
    let other = store.servers.iter().find(|s| s.id == test_entry(2).id).unwrap();
    assert!(!other.is_named());
}

/// ❗ **Naming a host the share history knows, but nobody typed in, saves it.**
/// A NAS someone only ever opened from the discovery list is still theirs to
/// name, and the manual store is the one place a name can live.
#[test]
fn naming_a_host_nobody_typed_in_saves_it_under_its_own_id() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    let id = generate_server_id("naspolya.local", 445);

    let entry =
        name_server_entry_at_path(&path, &id, "naspolya.local", 445, &named("Naspolya")).expect("a host with that id");

    assert_eq!(entry.port, 445);
    let store = read_store_from_path(&path);
    assert_eq!(store.servers.len(), 1);
    assert_eq!(store.servers[0].id, id);
    assert_eq!(store.servers[0].label(), "Naspolya");
}

/// An id that doesn't belong to the address can't be named: the pair would mint
/// an entry whose id and host disagree.
#[test]
fn naming_refuses_an_id_the_address_does_not_mint() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    assert!(
        name_server_entry_at_path(&path, "manual-elsewhere-445", "naspolya.local", 445, &named("Naspolya")).is_none()
    );
    assert!(read_store_from_path(&path).servers.is_empty());
}

/// ❗ **Adding a host again keeps a name the new add didn't give.** Add and
/// open on an address someone saved earlier is an ordinary move, and it must
/// not quietly unname their server.
#[test]
fn re_adding_a_host_without_a_name_keeps_the_one_it_had() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    let mut named = test_entry(3);
    named.display_name = "Garage".to_string();
    add_server_entry_to_path(&path, named);

    let mut again = test_entry(3);
    again.display_name = String::new();
    add_server_entry_to_path(&path, again);

    let store = read_store_from_path(&path);
    assert_eq!(store.servers.len(), 1);
    assert_eq!(store.servers[0].label(), "Garage");
}

/// An edit that names a host and leaves its account empty.
fn named(name: &str) -> HostEdit {
    HostEdit {
        name: name.to_string(),
        username: None,
    }
}

// -- Serialization round-trip --

#[test]
fn server_entry_serialization_round_trip() {
    let entry = ManualServerEntry {
        id: "manual-192-168-1-100-9445".to_string(),
        display_name: "192.168.1.100:9445".to_string(),
        address: "192.168.1.100".to_string(),
        port: 9445,
        added_at: "2026-04-02T10:00:00Z".to_string(),
        username: None,
    };

    let json = serde_json::to_string_pretty(&entry).unwrap();
    assert!(json.contains("\"displayName\""));
    assert!(json.contains("\"addedAt\""));

    let parsed: ManualServerEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, entry.id);
    assert_eq!(parsed.address, entry.address);
    assert_eq!(parsed.port, entry.port);
}

#[test]
fn store_serialization_round_trip() {
    let store = ManualServersStore {
        servers: vec![ManualServerEntry {
            id: "manual-mynas-445".to_string(),
            display_name: "mynas".to_string(),
            address: "mynas".to_string(),
            port: 445,
            added_at: "2026-04-02T10:00:00Z".to_string(),
            username: None,
        }],
    };

    let json = serde_json::to_string_pretty(&store).unwrap();
    let parsed: ManualServersStore = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.servers.len(), 1);
    assert_eq!(parsed.servers[0].id, "manual-mynas-445");
}

#[test]
fn store_deserialize_empty() {
    let store: ManualServersStore = serde_json::from_str("{}").unwrap();
    assert!(store.servers.is_empty());
}

// -- NetworkHost field mapping --

#[test]
fn host_mapping_bare_ip() {
    let host = create_network_host("192.168.1.100", 445);
    assert_eq!(host.name, "192.168.1.100");
    assert_eq!(host.hostname, Some("192.168.1.100".to_string()));
    assert_eq!(host.ip_address, Some("192.168.1.100".to_string()));
    assert_eq!(host.port, 445);
    assert_eq!(host.source, HostSource::Manual);
}

#[test]
fn host_mapping_ip_with_port() {
    let host = create_network_host("192.168.1.100", 9445);
    assert_eq!(host.name, "192.168.1.100:9445");
    assert_eq!(host.hostname, Some("192.168.1.100".to_string()));
    assert_eq!(host.ip_address, Some("192.168.1.100".to_string()));
    assert_eq!(host.port, 9445);
}

#[test]
fn host_mapping_hostname() {
    let host = create_network_host("mynas", 445);
    assert_eq!(host.name, "mynas");
    assert_eq!(host.hostname, Some("mynas".to_string()));
    assert_eq!(host.ip_address, None);
    assert_eq!(host.port, 445);
}

#[test]
fn host_mapping_hostname_with_local() {
    let host = create_network_host("mynas.local", 445);
    assert_eq!(host.name, "mynas.local");
    assert_eq!(host.hostname, Some("mynas.local".to_string()));
    assert_eq!(host.ip_address, None);
    assert_eq!(host.port, 445);
}

// -- Display name --

#[test]
fn discovery_name_default_port() {
    assert_eq!(discovery_name("192.168.1.100", 445), "192.168.1.100");
    assert_eq!(discovery_name("mynas", 445), "mynas");
}

#[test]
fn discovery_name_custom_port() {
    assert_eq!(discovery_name("192.168.1.100", 9445), "192.168.1.100:9445");
    assert_eq!(discovery_name("mynas", 9445), "mynas:9445");
}

// -- ManualConnectResult serialization --

#[test]
fn connect_result_serialization() {
    let result = ManualConnectResult {
        host: create_network_host("192.168.1.100", 9445),
        share_path: Some("docs".to_string()),
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"sharePath\""));
    assert!(json.contains("\"docs\""));
    // ManualConnectResult and NetworkHost are output-only (Rust → frontend), no Deserialize.
    // Verify the expected shape from the JSON string directly.
    assert!(json.contains("\"manual-192-168-1-100-9445\""));
}

// -- Concurrency tests for file-backed persistence --

/// Helper: creates a `ManualServerEntry` with a unique address.
fn test_entry(index: usize) -> ManualServerEntry {
    let address = format!("10.0.0.{}", index);
    ManualServerEntry {
        id: generate_server_id(&address, 445),
        display_name: address.clone(),
        address,
        port: 445,
        added_at: "2026-01-01T00:00:00Z".to_string(),
        username: None,
    }
}

/// Concurrent `add_server_entry_to_path` calls must not lose any writes.
/// Before the `STORE_LOCK` fix, this would fail because two threads could
/// read the same on-disk state and one write would clobber the other.
#[test]
fn concurrent_add_server_no_lost_writes() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);

    let thread_count = 20;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
    let mut handles = Vec::new();

    for i in 0..thread_count {
        let barrier = barrier.clone();
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            add_server_entry_to_path(&path, test_entry(i));
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let store = read_store_from_path(&path);
    assert_eq!(
        store.servers.len(),
        thread_count,
        "Expected {} servers but got {} (a concurrent write was lost)",
        thread_count,
        store.servers.len()
    );
}

/// Concurrent adds and removes must not corrupt the store.
#[test]
fn concurrent_add_and_remove() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);

    // Pre-populate with servers 0..10 that will be removed
    for i in 0..10 {
        add_server_entry_to_path(&path, test_entry(i));
    }
    assert_eq!(read_store_from_path(&path).servers.len(), 10);

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(20));
    let mut handles = Vec::new();

    // 10 threads remove servers 0..10
    for i in 0..10 {
        let barrier = barrier.clone();
        let path = path.clone();
        let id = test_entry(i).id;
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            remove_server_entry_from_path(&path, &id);
        }));
    }

    // 10 threads add servers 100..110
    for i in 100..110 {
        let barrier = barrier.clone();
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            add_server_entry_to_path(&path, test_entry(i));
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let store = read_store_from_path(&path);
    // All old servers removed, all new servers added
    assert_eq!(
        store.servers.len(),
        10,
        "Expected 10 servers (old removed, new added) but got {}",
        store.servers.len()
    );
    // Verify none of the old servers remain
    for i in 0..10 {
        assert!(
            !store.servers.iter().any(|s| s.id == test_entry(i).id),
            "Server {} should have been removed",
            i
        );
    }
    // Verify all new servers are present
    for i in 100..110 {
        assert!(
            store.servers.iter().any(|s| s.id == test_entry(i).id),
            "Server {} should have been added",
            i
        );
    }
}

/// Rapid sequential adds of distinct servers should all be persisted.
#[test]
fn rapid_sequential_adds() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);

    let count = 50;
    for i in 0..count {
        add_server_entry_to_path(&path, test_entry(i));
    }

    let store = read_store_from_path(&path);
    assert_eq!(store.servers.len(), count);
}

/// Upserts to the same server entry should not create duplicates.
#[test]
fn concurrent_upserts_same_server() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);

    let thread_count = 20;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
    let mut handles = Vec::new();

    for _ in 0..thread_count {
        let barrier = barrier.clone();
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            // All threads upsert the same server (same ID)
            add_server_entry_to_path(&path, test_entry(42));
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let store = read_store_from_path(&path);
    assert_eq!(
        store.servers.len(),
        1,
        "Concurrent upserts to the same server created {} duplicates",
        store.servers.len() - 1
    );
}

// ---------------------------------------------------------------------------
// Integration tests (require Docker SMB containers)
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "smb-e2e"))]
mod integration_tests {
    use super::*;

    /// Verifies TCP reachability against a Docker SMB container.
    ///
    /// Requires: `./test/smb-servers/start.sh minimal`
    #[tokio::test]
    async fn reachability_docker_smb_guest() {
        let port = smb2::testing::guest_port();
        let result = check_reachability("localhost", port).await;
        assert!(
            result.is_ok(),
            "Docker SMB container should be reachable on port {port}. Start it with: ./test/smb-servers/start.sh minimal"
        );
    }

    /// Verifies that an unreachable port returns an error.
    #[tokio::test]
    async fn reachability_unreachable_port() {
        let result = check_reachability("localhost", 19999).await;
        assert!(result.is_err(), "Nothing should be listening on port 19999");
    }

    /// Exercises the full manual server pipeline: parse → create host → generate ID.
    #[test]
    fn manual_server_pipeline() {
        let parsed = parse_server_address("localhost:9445").unwrap();
        assert_eq!(parsed.host, "localhost");
        assert_eq!(parsed.port, 9445);

        let host = create_network_host(&parsed.host, parsed.port);
        assert_eq!(host.source, HostSource::Manual);
        assert_eq!(host.id, "manual-localhost-9445");
        assert_eq!(host.name, "localhost:9445");
        assert_eq!(host.hostname, Some("localhost".to_string()));
        assert_eq!(host.ip_address, None);
        assert_eq!(host.port, 9445);

        // ID is deterministic: same inputs produce same ID
        let id = generate_server_id(&parsed.host, parsed.port);
        assert_eq!(id, host.id);
    }
}

// -- Why an add didn't go through --

/// ❗ **An address that doesn't parse and a server that didn't answer are two
/// answers**, because only the second can be added anyway: saving an address
/// nothing can read would be saving a typo on purpose.
#[test]
fn a_refused_add_says_whether_it_was_the_address_or_the_server() {
    let invalid = serde_json::to_value(AddServerError::InvalidAddress {
        message: "Enter a server address".to_string(),
    })
    .unwrap();
    assert_eq!(invalid["type"], "invalid_address");

    let unreachable = serde_json::to_value(AddServerError::Unreachable {
        message: "Couldn't reach nas:445".to_string(),
        hint: Some(UnreachableHint::LocalNetworkPermission),
    })
    .unwrap();
    assert_eq!(unreachable["type"], "unreachable");
    assert_eq!(unreachable["hint"], "local_network_permission");
}

/// ERR-XGS9X (macOS 27.0, 2026-09-30): this probe's `No route to host (os error 65)`
/// to a LAN server was a stuck Local Network permission, not a server that was off.
/// With no mount to compare against, the two look the same, so the Add sheet only
/// HINTS at the permission, and only where it can apply: the kernel refusing the
/// route to a private-range address.
#[test]
fn a_route_refused_to_a_lan_address_hints_at_the_local_network_permission() {
    use std::net::SocketAddr;
    let lan: SocketAddr = "192.168.0.153:445".parse().unwrap();
    let private_10: SocketAddr = "10.0.0.24:445".parse().unwrap();
    let link_local: SocketAddr = "169.254.10.2:445".parse().unwrap();
    let ula: SocketAddr = "[fd12:3456::1]:445".parse().unwrap();
    let public: SocketAddr = "93.184.216.34:445".parse().unwrap();
    let no_route = std::io::Error::from_raw_os_error(libc::EHOSTUNREACH);
    let no_network = std::io::Error::from_raw_os_error(libc::ENETUNREACH);
    let refused = std::io::Error::from_raw_os_error(libc::ECONNREFUSED);
    let hint = Some(UnreachableHint::LocalNetworkPermission);

    assert_eq!(unreachable_hint(&no_route, &[lan]), hint, "ERR-XGS9X's probe");
    assert_eq!(unreachable_hint(&no_network, &[private_10]), hint);
    assert_eq!(unreachable_hint(&no_route, &[link_local]), hint);
    assert_eq!(unreachable_hint(&no_route, &[ula]), hint);
    assert_eq!(
        unreachable_hint(&no_route, &[public]),
        None,
        "Local Network doesn't gate the internet"
    );
    assert_eq!(
        unreachable_hint(&no_route, &[lan, public]),
        None,
        "a name that also resolves off the LAN may just be using that route"
    );
    assert_eq!(
        unreachable_hint(&refused, &[lan]),
        None,
        "a refused connection reached something, so the route was fine"
    );
    assert_eq!(unreachable_hint(&no_route, &[]), None);
}

/// The unchecked add parses exactly like a checked one: only the TCP probe is
/// skipped, never the reading of the address.
#[tokio::test]
async fn an_unchecked_add_still_refuses_an_address_that_does_not_parse() {
    let refusal = checked_parse("not a server!!", Reachability::Skip).await;
    assert!(
        matches!(refusal, Err(AddServerError::InvalidAddress { .. })),
        "got {refusal:?}"
    );
}

/// ❗ Skipping the probe dials nothing: `host.invalid` would be an unreachable
/// server if anything tried it.
#[tokio::test]
async fn an_unchecked_add_dials_nothing() {
    let parsed = checked_parse("host.invalid", Reachability::Skip).await.expect("parses");
    assert_eq!(parsed.host, "host.invalid");
}

/// ❗ **Naming a host nobody typed in saves it where it dials, port included.**
/// A host off 445 is filed in the share history under `host:port`, and an entry
/// minted from that spelling on 445 would dial nothing.
#[test]
fn naming_a_host_off_445_saves_it_on_its_own_port() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    let id = generate_server_id("localhost", 11482);

    let stored =
        name_server_entry_at_path(&path, &id, "localhost", 11482, &named("Both box")).expect("a host with that id");

    assert_eq!(stored.address, "localhost");
    assert_eq!(stored.port, 11482);
    assert_eq!(
        create_network_host(&stored.address, stored.port).name,
        "localhost:11482"
    );
}

/// ❗ **An account typed for one server never reaches another server on the same
/// machine.** A typed `testuser` on `localhost:11482` made the guest-only
/// `localhost:11480` skip guest and demand a password (QA 2026-09-25).
#[test]
fn a_typed_account_belongs_to_its_own_port_only() {
    let mut entry = test_entry(23);
    entry.username = Some("testuser".to_string());
    entry.port = 11482;
    let entries = vec![entry];

    assert_eq!(
        typed_username_in(&entries, &SmbServer::new("10.0.0.23", 11480), &[]),
        None
    );
    assert_eq!(
        typed_username_in(&entries, &SmbServer::new("10.0.0.23", 445), &[]),
        None
    );
    assert_eq!(
        typed_username_in(&entries, &SmbServer::new("10.0.0.23", 11482), &[]).as_deref(),
        Some("testuser")
    );
}

/// ❗ **"Sign in as…" makes the account the server's preference**, the same one a typed
/// username is: the entry keeps its name, and a host nobody typed in is saved so the
/// preference has somewhere to live. Clearing it (guest) leaves the entry.
#[test]
fn signing_in_as_an_account_makes_it_the_hosts_preference() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    let mut named = test_entry(24);
    named.display_name = "My NAS".to_string();
    add_server_entry_to_path(&path, named);

    let stored = set_account_at_path(&path, &SmbServer::new("10.0.0.24", 445), &[], Some("testuser")).unwrap();
    assert_eq!(stored.username.as_deref(), Some("testuser"));
    assert_eq!(stored.display_name, "My NAS", "the name stays");

    let stored = set_account_at_path(&path, &SmbServer::new("10.0.0.24", 445), &[], None).unwrap();
    assert_eq!(stored.username, None);
    assert_eq!(read_store_from_path(&path).servers.len(), 1);

    assert!(
        set_account_at_path(&path, &SmbServer::new("10.0.0.26", 445), &[], None).is_none(),
        "guest on a host nobody saved saves nothing"
    );
    let fresh = set_account_at_path(&path, &SmbServer::new("10.0.0.25", 11482), &[], Some("ada")).unwrap();
    assert_eq!(fresh.id, generate_server_id("10.0.0.25", 11482));
    assert_eq!(fresh.username.as_deref(), Some("ada"));
    assert_eq!(read_store_from_path(&path).servers.len(), 2);
}
