//! The rules both saved-server stores share: where a start folder may sit.

use super::*;

// ── The label ────────────────────────────────────────────────────────

#[test]
fn a_named_server_is_called_by_its_name() {
    assert_eq!(server_label("NAS", "david", "192.168.1.111"), "NAS");
}

/// ❗ No port, scheme, or path: a label that looks like an address is what sent a
/// person to edit the wrong field.
#[test]
fn an_unnamed_server_is_called_username_at_host() {
    assert_eq!(server_label("", "david", "192.168.1.111"), "david@192.168.1.111");
    assert_eq!(
        server_label("   ", "david", "192.168.1.111"),
        "david@192.168.1.111",
        "a blank name is no name"
    );
}

#[test]
fn an_unnamed_account_with_no_username_is_called_by_its_host() {
    assert_eq!(server_label("", "", "dav.example.test"), "dav.example.test");
}

// ── Names that only repeat the address ───────────────────────────────

const SFTP: &[(&str, u16)] = &[("sftp", 22), ("ssh", 22)];
const WEBDAV: &[(&str, u16)] = &[
    ("https", 443),
    ("http", 80),
    ("davs", 443),
    ("dav", 80),
    ("webdav", 443),
];

fn sftp_account<'a>(username: &'a str, host: &'a str, port: u16) -> OwnAddress<'a> {
    OwnAddress {
        username,
        host,
        port,
        schemes: SFTP,
    }
}

/// The prod case: an unnamed server saved with its whole address as its name.
#[test]
fn the_app_root_spelling_with_or_without_a_path_is_the_servers_own_address() {
    let nas = sftp_account("david", "192.168.1.111", 22);
    assert!(spells_own_address(
        "sftp://david@192.168.1.111:22/share/ZFS18_DATA/naspi",
        &nas
    ));
    assert!(spells_own_address("sftp://david@192.168.1.111:22", &nas));
    assert!(
        spells_own_address("  ssh://david@192.168.1.111:22/  ", &nas),
        "surrounding blanks and `ssh://` alike"
    );
}

#[test]
fn username_at_host_with_or_without_the_port_is_the_servers_own_address() {
    let nas = sftp_account("david", "192.168.1.111", 22);
    assert!(spells_own_address("david@192.168.1.111", &nas));
    assert!(spells_own_address("david@192.168.1.111:22", &nas));
    assert!(
        !spells_own_address("david@192.168.1.111:2222", &nas),
        "another port is another server"
    );
}

/// A scheme names the port when the address doesn't: `sftp://` with none is 22.
#[test]
fn a_scheme_without_a_port_means_the_schemes_own_port() {
    let address = "sftp://david@192.168.1.111/srv";
    assert!(spells_own_address(address, &sftp_account("david", "192.168.1.111", 22)));
    assert!(!spells_own_address(
        address,
        &sftp_account("david", "192.168.1.111", 2222)
    ));
}

#[test]
fn the_host_folds_case_and_the_account_does_not() {
    let nas = sftp_account("david", "NAS.local", 22);
    assert!(spells_own_address("sftp://david@nas.LOCAL:22", &nas));
    assert!(!spells_own_address("sftp://David@nas.local:22", &nas));
}

#[test]
fn a_bracketed_ipv6_host_matches_with_or_without_its_brackets() {
    assert!(spells_own_address(
        "sftp://ada@[fe80::1]:22",
        &sftp_account("ada", "fe80::1", 22)
    ));
    assert!(spells_own_address(
        "ada@[fe80::1]",
        &sftp_account("ada", "[fe80::1]", 22)
    ));
}

/// ❗ Whatever isn't provably this account's own address is a label, and stays.
#[test]
fn anything_else_is_a_label_someone_chose() {
    let nas = sftp_account("david", "192.168.1.111", 22);
    for label in [
        "NAS",
        "192.168.1.111",
        "sftp://david@192.168.1.112:22/share",
        "sftp://grace@192.168.1.111:22",
        "smb://david@192.168.1.111",
        "sftp://david@192.168.1.111:22/x?y",
        "david@192.168.1.111 at home",
        "",
    ] {
        assert!(!spells_own_address(label, &nas), "{label:?} is a label");
    }
}

/// WebDAV's own URL counts with or without its path, and with or without the
/// account in it.
#[test]
fn a_webdav_url_for_the_same_endpoint_is_the_servers_own_address() {
    let dav = OwnAddress {
        username: "ada",
        host: "dav.example.test",
        port: 443,
        schemes: WEBDAV,
    };
    assert!(spells_own_address("https://dav.example.test/remote.php/dav/", &dav));
    assert!(spells_own_address("https://dav.example.test", &dav));
    assert!(spells_own_address("https://ada@dav.example.test:443/dav", &dav));
    assert!(spells_own_address("ada@dav.example.test", &dav));
    assert!(
        !spells_own_address("http://dav.example.test/dav/", &dav),
        "plain HTTP is port 80, another endpoint"
    );
    assert!(
        !spells_own_address("https://grace@dav.example.test/dav/", &dav),
        "another account"
    );
    assert!(
        !spells_own_address("https://other.example.test/dav/", &dav),
        "another server"
    );
}

/// ❗ An account with an `@` in it (common on WebDAV) still finds the host after it.
#[test]
fn an_account_with_an_at_sign_still_finds_the_host() {
    let dav = OwnAddress {
        username: "ada@example.test",
        host: "dav.example.test",
        port: 443,
        schemes: WEBDAV,
    };
    assert!(spells_own_address("ada@example.test@dav.example.test", &dav));
    assert!(spells_own_address("https://dav.example.test/dav/", &dav));
}

#[test]
fn no_start_folder_means_the_root() {
    assert_eq!(start_folder_under_root("/srv/data", None), Ok(None));
    assert_eq!(start_folder_under_root("/srv/data", Some("")), Ok(None));
    assert_eq!(start_folder_under_root("/srv/data", Some("   ")), Ok(None));
}

/// The root itself, however it's spelled, is stored as no start folder at all,
/// so one landing has one spelling.
#[test]
fn a_start_folder_at_the_root_is_stored_as_none() {
    assert_eq!(start_folder_under_root("/srv/data", Some("/srv/data")), Ok(None));
    assert_eq!(start_folder_under_root("/srv/data/", Some("/srv/./data")), Ok(None));
    assert_eq!(start_folder_under_root("/", Some("/")), Ok(None));
}

/// The prod case: a root widened to `naspi`, landing where it used to be rooted.
#[test]
fn a_start_folder_under_the_root_is_stored_normalized() {
    assert_eq!(
        start_folder_under_root("/share/ZFS18_DATA/naspi", Some("/share/ZFS18_DATA/naspi/tmp/")),
        Ok(Some("/share/ZFS18_DATA/naspi/tmp".to_string()))
    );
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv/data/photos/../docs")),
        Ok(Some("/srv/data/docs".to_string()))
    );
    assert_eq!(
        start_folder_under_root("/", Some("/home/ada")),
        Ok(Some("/home/ada".to_string()))
    );
}

/// ❗ The trap a string prefix falls into: a sibling whose name merely starts
/// with the root's.
#[test]
fn a_sibling_that_spells_the_roots_prefix_is_outside_it() {
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv/data-1/photos")),
        Err(StartFolderOutsideRoot)
    );
}

#[test]
fn a_start_folder_above_the_root_or_escaping_it_is_outside() {
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv")),
        Err(StartFolderOutsideRoot)
    );
    assert_eq!(
        start_folder_under_root("/srv/data", Some("/srv/data/../etc")),
        Err(StartFolderOutsideRoot)
    );
}

/// A connect carrying a saved start folder across a root that no longer holds it
/// lands at the root, ❌ never saves a place no pane could reach.
#[test]
fn a_connect_drops_a_saved_start_folder_the_root_no_longer_holds() {
    assert_eq!(
        start_folder_for_root("/srv/data", Some("/srv/data/photos".to_string())),
        Some("/srv/data/photos".to_string())
    );
    assert_eq!(
        start_folder_for_root("/srv/other", Some("/srv/data/photos".to_string())),
        None
    );
    assert_eq!(start_folder_for_root("/srv/data", None), None);
}

/// A relative start folder is read from `/`, the way `RemoteRoot` reads a
/// relative root, ❌ never from the root: the field says an absolute server path.
#[test]
fn a_relative_start_folder_is_read_from_the_server_root() {
    assert_eq!(
        start_folder_under_root("/srv/data", Some("photos")),
        Err(StartFolderOutsideRoot)
    );
    assert_eq!(
        start_folder_under_root("/", Some("photos")),
        Ok(Some("/photos".to_string()))
    );
}
