//! The allowed pairs reach `cmdr_smb` from the file, and a file that doesn't read
//! allows nothing. In-memory otherwise: these run before a load names a file.

use super::*;

fn file_with(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, contents).unwrap();
    (dir, path)
}

#[test]
fn allow_lists_once_and_forget_takes_it_back() {
    assert!(allow("list.server.test", "list.target.test"));
    assert!(allow("list.server.test", "list.target.test"));
    let listed = list()
        .into_iter()
        .filter(|e| e.server == "list.server.test")
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 1, "one row per pair");
    assert_eq!(listed[0].target, "list.target.test");

    assert!(forget("list.server.test", "list.target.test"));
    assert!(!forget("list.server.test", "list.target.test"));
    assert!(!list().iter().any(|e| e.server == "list.server.test"));
}

#[test]
fn a_name_no_person_could_read_is_never_stored() {
    assert!(!allow("sneaky.server.test", "nas\u{202E}lave"));
    assert!(!list().iter().any(|e| e.server == "sneaky.server.test"));
}

#[test]
fn a_store_round_trips_with_every_entry_that_reads() {
    let (_dir, path) = file_with(
        "smb-link-consents.json",
        r#"{"consents":[{"server":"a","target":"b","approvedAt":"2026-10-31T00:00:00Z"},{"server":5}]}"#,
    );
    let store: SmbLinkConsentsStore = server_list_file::read(&path, StateFile::SmbLinkConsents);
    assert_eq!(store.consents.len(), 1, "the broken entry goes, the good one stays");
    assert_eq!(store.consents[0].target, "b");
}

#[test]
fn a_file_that_doesnt_read_allows_nothing() {
    let (_dir, path) = file_with("smb-link-consents.json", "not json");
    let store: SmbLinkConsentsStore = server_list_file::read(&path, StateFile::SmbLinkConsents);
    assert!(store.consents.is_empty());
}
