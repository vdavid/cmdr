//! A manual server saved without contact: what it writes, what it stays out of, and
//! the first mount that makes it ordinary.

use super::super::{
    Contact, ManualServerEntry, ParsedAddress, Reachability, add_at, add_server_entry_to_path, generate_server_id,
};
use super::*;
use crate::network::known_shares;

fn temp_store() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join(MANUAL_SERVERS_FILENAME);
    (dir, path)
}

fn parsed(input: &str) -> ParsedAddress {
    parse_server_address(input).expect("a valid address")
}

fn entry(address: &str, awaiting_first_connect: bool) -> ManualServerEntry {
    ManualServerEntry {
        id: generate_server_id(address, 445),
        display_name: String::new(),
        address: address.to_string(),
        port: 445,
        added_at: "2026-10-27T00:00:00Z".to_string(),
        username: None,
        awaiting_first_connect,
    }
}

/// ❗ **A contact-free save writes the host and its share, and contacts nothing**: no
/// dial is filed, the host stays out of the discovery list (so the servers view never
/// lists its shares on Cmdr's own initiative), and the GUID probe skips its share.
/// Its first mount makes it an ordinary host, once.
#[test]
fn a_contact_free_save_stays_out_of_discovery_and_the_probe_until_its_first_mount() {
    let (_dir, path) = temp_store();
    let dials = crate::network::connect_wiring::dials_filed();
    let details = HostEdit {
        name: "Backup NAS".to_string(),
        username: None,
    };

    let saved = save_parsed_at(
        Some(&path),
        parsed("smb://ada@nas-backup.invalid/Backups"),
        &details,
        Contact::AwaitingFirstConnect,
    );

    let store = read_store_from_path(&path);
    assert_eq!(store.servers.len(), 1);
    let host = &store.servers[0];
    assert!(host.awaiting_first_connect);
    assert_eq!(host.label(), "Backup NAS");
    assert_eq!(host.username.as_deref(), Some("ada"), "the account the address named");
    assert_eq!(discoverable(&store.servers).count(), 0, "not in the discovery list");
    assert!(
        !crate::network::fresh_discovered_hosts()
            .iter()
            .any(|known| known.id == saved.host.id),
        "nothing injected it"
    );

    let shares = known_shares::saved_shares();
    let share = shares
        .iter()
        .find(|row| row.server_name == "nas-backup.invalid" && row.share_name == "Backups")
        .expect("the share row is saved");
    assert_eq!(share.volume_id, None, "no mount went through");
    assert!(
        !crate::network::smb_server_guid::rows_to_probe(&shares)
            .iter()
            .any(|row| row.server_name == "nas-backup.invalid"),
        "the probe never dials it"
    );
    assert_eq!(
        crate::network::connect_wiring::dials_filed(),
        dials,
        "no dial was filed"
    );

    let server = SmbServer::new("nas-backup.invalid", 445);
    assert!(
        note_first_connect_at_path(&path, &server, &[]),
        "its first mount wakes it"
    );
    let store = read_store_from_path(&path);
    assert!(!store.servers[0].awaiting_first_connect);
    assert_eq!(discoverable(&store.servers).count(), 1, "the next load lists it");
    assert!(!note_first_connect_at_path(&path, &server, &[]), "once");

    known_shares::forget_rows(std::slice::from_ref(share));
}

/// A mount of another server, even one on the same machine, wakes nothing.
#[test]
fn a_mount_of_another_server_wakes_nothing() {
    let (_dir, path) = temp_store();
    add_server_entry_to_path(&path, entry("10.0.9.1", true));

    assert!(!note_first_connect_at_path(
        &path,
        &SmbServer::new("10.0.9.1", 11480),
        &[]
    ));
    assert!(!note_first_connect_at_path(
        &path,
        &SmbServer::new("10.0.9.2", 445),
        &[]
    ));
    assert!(read_store_from_path(&path).servers[0].awaiting_first_connect);
}

/// ❗ **A contact-free save never hides an ordinary host**, and the user's own add of a
/// waiting host makes it ordinary: the flag survives a re-save only while both say so.
#[test]
fn only_a_host_every_save_left_waiting_stays_out_of_the_list() {
    let (_dir, path) = temp_store();
    add_server_entry_to_path(&path, entry("10.0.9.3", false));
    add_server_entry_to_path(&path, entry("10.0.9.3", true));
    assert!(!read_store_from_path(&path).servers[0].awaiting_first_connect);

    add_server_entry_to_path(&path, entry("10.0.9.4", true));
    add_server_entry_to_path(&path, entry("10.0.9.4", true));
    assert!(read_store_from_path(&path).servers[1].awaiting_first_connect);
    add_server_entry_to_path(&path, entry("10.0.9.4", false));
    assert!(!read_store_from_path(&path).servers[1].awaiting_first_connect);
}

/// An entry saved before the field reads as an ordinary host, and an ordinary one
/// writes no key, so the file a user already has doesn't change shape.
#[test]
fn an_entry_without_the_field_is_ordinary_and_an_ordinary_one_writes_no_key() {
    let legacy: ManualServerEntry =
        serde_json::from_str(r#"{"id": "manual-10-0-9-5-445", "address": "10.0.9.5", "port": 445, "addedAt": "x"}"#)
            .expect("reads");
    assert!(!legacy.awaiting_first_connect);
    let json = serde_json::to_string(&legacy).expect("writes");
    assert!(!json.contains("awaitingFirstConnect"), "{json}");
    let waiting = serde_json::to_string(&entry("10.0.9.6", true)).expect("writes");
    assert!(waiting.contains("\"awaitingFirstConnect\":true"), "{waiting}");
}

/// ❗ **The sheet's "Add anyway" saves the host without contacting it** (#777), the
/// same save an approved Ask Cmdr card makes: no probe, no dial, and out of the
/// discovery list (so nothing lists its shares) until the user first connects it.
/// Saving a server is a store write; contact starts when the user connects.
#[tokio::test]
async fn add_anyway_saves_the_host_without_contact() {
    let (_dir, path) = temp_store();
    let dials = crate::network::connect_wiring::dials_filed();

    let (saved, contact) = add_at(
        Some(&path),
        "smb://nas-asleep.invalid",
        &HostEdit::default(),
        Reachability::Skip,
    )
    .await
    .expect("an address that parses saves");

    assert_eq!(
        contact,
        Contact::AwaitingFirstConnect,
        "its host stays out of the discovery list"
    );
    let store = read_store_from_path(&path);
    assert_eq!(store.servers.len(), 1);
    assert_eq!(store.servers[0].id, saved.host.id);
    assert!(store.servers[0].awaiting_first_connect);
    assert_eq!(discoverable(&store.servers).count(), 0, "no load lists it either");
    assert_eq!(
        crate::network::connect_wiring::dials_filed(),
        dials,
        "no dial was filed"
    );
}
