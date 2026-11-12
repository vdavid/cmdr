use super::*;

/// The database file's bytes, so a test can prove a refusal changed nothing in it.
///
/// The main file alone: any reader of a WAL database creates an EMPTY `-wal` and a
/// `-shm` beside it (verified via rusqlite's bundled SQLite, this test,
/// 2026-10-26). Neither holds a row, and the main file is where a rebuild or a
/// schema stamp would show.
fn snapshot(db_path: &Path) -> Option<Vec<u8>> {
    std::fs::read(db_path).ok()
}

/// A stopped volume's index: written by a live store, then closed.
fn stopped_index(dir: &Path) -> std::path::PathBuf {
    let db_path = dir.join("index-smb-test.db");
    let store = IndexStore::open(&db_path).expect("open a live store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
    IndexStore::update_meta(&conn, "volume_path", "/Volumes/testshare").expect("volume_path");
    IndexStore::update_meta(&conn, "scan_completed_at", "1761400000").expect("scan stamp");
    drop(conn);
    drop(store);
    db_path
}

#[test]
fn reads_a_stopped_volumes_mount_root_and_last_scan() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = stopped_index(dir.path());

    let facts = StoredIndex::open(&db_path)
        .expect("a current-schema index opens")
        .last_scan()
        .expect("its meta reads");
    assert_eq!(facts.volume_path.as_deref(), Some("/Volumes/testshare"));
    assert_eq!(facts.scan_completed_at.as_deref(), Some("1761400000"));
}

#[test]
fn a_missing_database_is_refused_and_not_created() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-never.db");

    assert!(matches!(StoredIndex::open(&db_path), Err(StoredIndexRefusal::Missing)));
    assert!(!db_path.exists(), "a reader must never create the database");
}

/// ❗ `IndexStore::open` deletes and recreates a database on a schema mismatch.
/// That's a live start's call: a reader asking "what did this drive hold" must
/// never throw a whole index away to answer.
#[test]
fn another_schemas_database_is_refused_without_touching_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = stopped_index(dir.path());
    let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
    IndexStore::update_meta(&conn, "schema_version", "9999").expect("stamp a newer schema");
    drop(conn);
    let before = snapshot(&db_path);

    match StoredIndex::open(&db_path) {
        Err(StoredIndexRefusal::OtherSchema { found }) => assert_eq!(found.as_deref(), Some("9999")),
        Err(other) => panic!("expected OtherSchema, got {other:?}"),
        Ok(_) => panic!("a newer schema must be refused"),
    }
    assert!(snapshot(&db_path) == before, "the refused database is byte-identical");
}

#[test]
fn a_file_that_isnt_an_index_is_refused_without_touching_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-junk.db");
    std::fs::write(&db_path, b"not a database").expect("write junk");
    let before = snapshot(&db_path);

    assert!(matches!(
        StoredIndex::open(&db_path),
        Err(StoredIndexRefusal::Unreadable)
    ));
    assert!(snapshot(&db_path) == before, "the junk file is byte-identical");
}
