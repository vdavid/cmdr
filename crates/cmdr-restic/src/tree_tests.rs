use super::{child, entry_of, permissions_of};
use cmdr_fs::volume::VolumeError;
use rustic_core::jiff::Timestamp;
use rustic_core::repofile::{Metadata, Node, NodeType, Tree};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Go's `os.ModeDir` and `os.ModeSetuid`: what restic stores in a node's mode
/// next to the permission bits.
const GO_MODE_DIR: u32 = 1 << 31;
const GO_MODE_SETUID: u32 = 1 << 23;

fn node(name: &OsStr, node_type: NodeType, meta: Metadata) -> Node {
    Node::new_node(name, node_type, meta)
}

fn file(name: &str, size: u64) -> Node {
    node(
        OsStr::new(name),
        NodeType::File,
        Metadata {
            size,
            mode: Some(0o644),
            ..Metadata::default()
        },
    )
}

fn tree(nodes: Vec<Node>) -> Tree {
    Tree { nodes }
}

const DIR: &str = "/r/Browse snapshots/latest/Users";

#[test]
fn a_file_lists_with_its_size_date_mode_and_owner() {
    let mut n = file("notes.txt", 1_234);
    n.meta.mtime = Some("2026-10-12T19:00:03.5Z".parse::<Timestamp>().expect("time"));
    n.meta.user = Some("test_user_3".to_string());
    n.meta.group = Some("staff".to_string());

    let entry = entry_of(&n, Path::new(DIR));
    assert_eq!(entry.name, "notes.txt");
    assert_eq!(entry.path, format!("{DIR}/notes.txt"));
    assert!(!entry.is_directory && !entry.is_symlink);
    assert_eq!(entry.size, Some(1_234));
    assert_eq!(
        entry.modified_at,
        Some(1_791_831_603),
        "whole seconds, like every backend"
    );
    assert_eq!(entry.permissions, 0o644);
    assert_eq!((entry.owner.as_str(), entry.group.as_str()), ("test_user_3", "staff"));
}

#[test]
fn a_directory_lists_as_one_without_a_size() {
    let n = node(
        OsStr::new("Users"),
        NodeType::Dir,
        Metadata {
            size: 0,
            mode: Some(GO_MODE_DIR | 0o755),
            ..Metadata::default()
        },
    );
    let entry = entry_of(&n, Path::new(DIR));
    assert!(entry.is_directory);
    assert_eq!(entry.size, None);
}

/// ❗ Restic stores Go `os.FileMode` bits, and only the low nine are POSIX
/// permissions. Reporting the raw value would put garbage on every copied file,
/// and the setuid bit is dropped on purpose: a restore shouldn't recreate a
/// setuid binary.
#[test]
fn the_mode_is_the_permission_bits_alone() {
    assert_eq!(permissions_of(Some(GO_MODE_DIR | 0o755)), 0o755);
    assert_eq!(permissions_of(Some(GO_MODE_SETUID | 0o755)), 0o755);
    assert_eq!(permissions_of(Some(0o600)), 0o600);
    // No mode recorded: the "no permission concept" sentinel, ❌ never a
    // plausible made-up default.
    assert_eq!(permissions_of(None), 0);
}

#[test]
fn a_symlink_is_a_row_of_its_own_and_never_followed() {
    let n = node(
        OsStr::new("current"),
        NodeType::from_link(Path::new("/Users/test_user_3")),
        Metadata {
            size: 12,
            ..Metadata::default()
        },
    );
    let entry = entry_of(&n, Path::new(DIR));
    assert!(entry.is_symlink);
    assert!(!entry.is_directory, "a link to a folder is still not a folder here");
}

#[test]
fn devices_fifos_and_sockets_are_plain_rows_with_no_bytes() {
    for node_type in [
        NodeType::Dev { device: 1 },
        NodeType::Chardev { device: 2 },
        NodeType::Fifo,
        NodeType::Socket,
    ] {
        let n = node(
            OsStr::new("special"),
            node_type,
            Metadata {
                size: 99,
                ..Metadata::default()
            },
        );
        let entry = entry_of(&n, Path::new(DIR));
        assert!(!entry.is_directory && !entry.is_symlink);
        assert_eq!(entry.size, Some(0));
    }
}

#[test]
fn a_date_before_1970_is_left_out() {
    let mut n = file("old.txt", 1);
    n.meta.mtime = Some("1969-07-20T20:17:00Z".parse::<Timestamp>().expect("time"));
    assert_eq!(entry_of(&n, Path::new(DIR)).modified_at, None);
}

#[test]
fn odd_names_pass_through_as_stored() {
    let long = "x".repeat(255);
    for name in ["Cafe\u{301}.txt", "with  two spaces", " leading", long.as_str()] {
        let entry = entry_of(&file(name, 1), Path::new(DIR));
        assert_eq!(entry.name, name);
        let listing = tree(vec![file(name, 1)]);
        let found = child(&listing, OsStr::new(name), Path::new(DIR)).expect("no error");
        assert!(found.is_some(), "{name:?} is found by its stored name");
    }
}

/// A Linux machine's backup can hold a name that isn't UTF-8. It lists the way
/// `LocalPosixVolume` lists one (lossy), and the lossy name never reaches a file.
#[test]
fn a_non_utf8_name_lists_lossily_and_its_lossy_name_finds_nothing() {
    let raw = OsStr::from_bytes(b"caf\xe9.txt");
    let n = node(raw, NodeType::File, Metadata::default());
    let entry = entry_of(&n, Path::new(DIR));
    assert_eq!(entry.name, "caf\u{FFFD}.txt");

    let listing = tree(vec![n]);
    let found = child(&listing, OsStr::new(&entry.name), Path::new(DIR)).expect("no error");
    assert!(found.is_none(), "the lossy name is not the stored one");
}

/// ❗ The one way a lossy name could reach ANOTHER file: a real sibling whose
/// stored name IS the lossy spelling. Both list under one name, so the lookup
/// refuses rather than guess.
#[test]
fn a_lossy_name_shared_with_a_real_sibling_is_ambiguous() {
    let raw = node(OsStr::from_bytes(b"caf\xe9.txt"), NodeType::File, Metadata::default());
    let real = file("caf\u{FFFD}.txt", 1);
    let listing = tree(vec![raw, real]);
    let result = child(&listing, OsStr::new("caf\u{FFFD}.txt"), Path::new(DIR));
    assert!(matches!(result, Err(VolumeError::AmbiguousName(_))), "got {result:?}");
}
