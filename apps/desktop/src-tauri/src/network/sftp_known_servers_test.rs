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
        key_file: None,
        use_agent: true,
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
