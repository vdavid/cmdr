//! The share label: a typed server name wins, the address stands in otherwise.

use super::*;
use crate::network::manual_servers::{ManualServerEntry, named_for_test, typed_name_among};

fn entry(address: &str, port: u16, name: &str) -> ManualServerEntry {
    ManualServerEntry {
        id: crate::network::manual_servers::generate_server_id(address, port),
        display_name: name.to_string(),
        address: address.to_string(),
        port,
        added_at: "2026-11-03T00:00:00Z".to_string(),
        username: None,
        awaiting_first_connect: false,
    }
}

/// ❗ **The Name typed in the Add server sheet reaches the share's label** (#913): a
/// share filed under the discovery name `home-nas.local.:11481` reads "private on
/// Home NAS", saved or mounted.
#[test]
fn a_named_servers_share_reads_by_the_typed_name() {
    let named = named_for_test(&[entry("home-nas.local.", 11481, "Home NAS")]);
    assert_eq!(
        filed_among(&named, "private", "home-nas.local.:11481", None, None, &[]),
        "private on Home NAS"
    );
    assert_eq!(
        filed_among(
            &named,
            "private",
            "home-nas.local.:11481",
            Some("home-nas.local."),
            Some(11481),
            &[]
        ),
        "private on Home NAS"
    );
    // A mount's `statfs` spelling, without the trailing dot.
    assert_eq!(
        typed_name_among(&named, &[SmbServer::new("home-nas.local", 11481)], &[]).as_deref(),
        Some("Home NAS")
    );
}

/// The address stands in when nobody named the server, and a server on ANOTHER port of
/// the same machine never borrows the name: one machine can run several.
#[test]
fn an_unnamed_or_other_port_server_keeps_its_address() {
    let named = named_for_test(&[
        entry("nas.local", 11481, "Home NAS"),
        // Its "name" spells its own address: not a name (`ManualServerEntry::is_named`).
        entry("127.0.0.1", 11482, "127.0.0.1:11482"),
    ]);
    assert_eq!(
        filed_among(&named, "public", "nas.local:11490", None, None, &[]),
        "public on nas.local:11490"
    );
    assert_eq!(
        filed_among(&named, "public", "127.0.0.1:11482", None, None, &[]),
        "public on 127.0.0.1:11482"
    );
    assert_eq!(
        filed_among(&named, "public", "nas.local", None, None, &[]),
        "public on nas.local",
        "the server on 445 is another server"
    );
}
