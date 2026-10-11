//! A host moving to a new address in the manual store.

use super::super::{ManualServersStore, read_store_from_path, write_store_to_path};
use super::*;
use crate::test_support::TestDir;

fn entry(address: &str, port: u16, name: &str) -> ManualServerEntry {
    ManualServerEntry {
        id: generate_server_id(address, port),
        display_name: name.to_string(),
        address: address.to_string(),
        port,
        added_at: "2026-09-01T00:00:00Z".to_string(),
        username: Some("ada".to_string()),
    }
}

fn edit(name: &str, username: Option<&str>) -> HostEdit {
    HostEdit {
        name: name.to_string(),
        username: username.map(str::to_string),
    }
}

/// The same host at a new address: one entry, where the old one stood, keeping when it was
/// added, with the name and account the edit set.
#[test]
fn a_moved_host_replaces_its_entry_in_place() {
    let dir = TestDir::new("manual-relocate-in-place");
    let path = dir.join("manual-servers.json");
    let store = ManualServersStore {
        servers: vec![entry("10.0.0.2", 445, "Attic NAS"), entry("10.0.0.9", 445, "")],
    };
    write_store_to_path(&path, &store);

    let moved = relocate_at_path(
        &path,
        &generate_server_id("10.0.0.2", 445),
        &SmbServer::new("nas.tail1234.ts.net", 11480),
        &edit("Attic NAS", Some("bob")),
    )
    .expect("nothing holds the new address");

    let servers = read_store_from_path(&path).servers;
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0].id, generate_server_id("nas.tail1234.ts.net", 11480));
    assert_eq!(servers[0].address, "nas.tail1234.ts.net");
    assert_eq!(servers[0].port, 11480);
    assert_eq!(servers[0].added_at, "2026-09-01T00:00:00Z");
    assert_eq!(servers[0].username.as_deref(), Some("bob"));
    assert_eq!(servers[0].display_name, "Attic NAS");
    assert_eq!(moved.id, servers[0].id);
}

/// A host only the share history knew is saved at its new address.
#[test]
fn a_moved_host_nobody_typed_is_saved_at_the_new_address() {
    let dir = TestDir::new("manual-relocate-unlisted");
    let path = dir.join("manual-servers.json");

    relocate_at_path(
        &path,
        &generate_server_id("10.0.0.2", 445),
        &SmbServer::new("10.0.0.3", 445),
        &edit("", None),
    )
    .expect("nothing holds the new address");

    let servers = read_store_from_path(&path).servers;
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].address, "10.0.0.3");
}

/// ❗ Another host's entry at the new address refuses, and nothing is written.
#[test]
fn an_address_another_host_holds_is_refused_untouched() {
    let dir = TestDir::new("manual-relocate-taken");
    let path = dir.join("manual-servers.json");
    let store = ManualServersStore {
        servers: vec![
            entry("10.0.0.2", 445, "Attic NAS"),
            entry("10.0.0.3", 445, "Office NAS"),
        ],
    };
    write_store_to_path(&path, &store);

    let refused = relocate_at_path(
        &path,
        &generate_server_id("10.0.0.2", 445),
        &SmbServer::new("10.0.0.3", 445),
        &edit("Attic NAS", None),
    );

    assert_eq!(refused.map(|e| e.id), Err("Office NAS".to_string()));
    assert_eq!(read_store_from_path(&path).servers[0].address, "10.0.0.2");
}
