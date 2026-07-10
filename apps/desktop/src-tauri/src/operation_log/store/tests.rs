//! Store tests: the migration ladder (the risky, first-of-its-kind logic —
//! TDD'd with a real red), dir interning, and case folding. The full
//! open→write→read round-trip through the writer lives in `writer.rs` tests.

use rusqlite::{Connection, Transaction};

use super::migrations::read_schema_version;
use super::*;

// ── Synthetic ladders, so the runner mechanism is tested independent of the
//    production schema. `widget` is a throwaway table these steps evolve. ──

fn v1_create_widget(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE widget (id INTEGER PRIMARY KEY, a TEXT NOT NULL);")
}

fn v2_add_column(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE widget ADD COLUMN b TEXT NOT NULL DEFAULT 'backfilled';")
}

const LADDER_V1: &[Migration] = &[Migration {
    version: 1,
    description: "create widget",
    up: v1_create_widget,
}];

const LADDER_V2: &[Migration] = &[
    Migration {
        version: 1,
        description: "create widget",
        up: v1_create_widget,
    },
    Migration {
        version: 2,
        description: "add widget.b",
        up: v2_add_column,
    },
];

/// A v1→v2 forward migration bumps the version stepwise AND preserves the rows
/// written under v1 (backfilling the new column). This is the whole point of a
/// migration ladder over delete-and-recreate: the user's data survives a schema
/// change.
#[test]
fn forward_migration_preserves_rows_and_bumps_version() {
    let conn = Connection::open_in_memory().expect("in-memory db");

    // Bring the DB to v1 and write a row.
    run_migrations(&conn, LADDER_V1).expect("migrate to v1");
    assert_eq!(read_schema_version(&conn).expect("version"), 1, "v1 ladder ⇒ version 1");
    conn.execute("INSERT INTO widget (id, a) VALUES (1, 'hi')", [])
        .expect("insert v1 row");

    // Migrate forward to v2: the version bumps and the row survives with the
    // new column backfilled.
    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    assert_eq!(read_schema_version(&conn).expect("version"), 2, "v2 ladder ⇒ version 2");

    let (a, b): (String, String) = conn
        .query_row("SELECT a, b FROM widget WHERE id = 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("row survived the migration");
    assert_eq!(a, "hi", "the pre-migration value is preserved");
    assert_eq!(b, "backfilled", "the new column is backfilled by the step");
}

/// Re-running the same ladder is a no-op: the version stays put and nothing is
/// re-applied (an idempotent open).
#[test]
fn re_running_the_ladder_is_a_noop() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    // A second identical run must not try to re-create the table (which would
    // error) or bump the version.
    run_migrations(&conn, LADDER_V2).expect("second run is a no-op");
    assert_eq!(read_schema_version(&conn).expect("version"), 2);
}

/// A stored version NEWER than the ladder (a downgrade: the user ran a newer
/// build, then an older one) is refused, never destroyed — the newer DB may hold
/// data this build can't represent.
#[test]
fn downgrade_is_refused_not_destroyed() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    conn.execute("INSERT INTO widget (id, a) VALUES (7, 'precious')", [])
        .expect("insert");

    // Opening the same DB with an OLDER ladder (max version 1 < stored 2) refuses.
    let err = run_migrations(&conn, LADDER_V1).expect_err("a downgrade must be refused");
    assert!(
        matches!(err, OperationLogStoreError::SchemaDowngrade { found: 2, expected: 1 }),
        "expected a typed SchemaDowngrade(found=2, expected=1), got {err:?}"
    );

    // The data is untouched — refuse means leave it alone.
    let a: String = conn
        .query_row("SELECT a FROM widget WHERE id = 7", [], |row| row.get(0))
        .expect("row still there");
    assert_eq!(a, "precious", "a refused downgrade must not destroy the DB");
}

/// A genuinely unparseable file (garbage bytes, not a SQLite DB) is deleted and
/// recreated fresh through `OperationLogStore::open` — the ONLY case where
/// destroy-and-recreate is correct. The recreated DB is at the current schema
/// version with the real tables present.
#[test]
fn unparseable_file_recreates_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    std::fs::write(&path, b"this is not a sqlite database at all").expect("write garbage");

    let store = OperationLogStore::open(&path).expect("open recreates an unparseable file");
    assert_eq!(
        store.schema_version().expect("version"),
        MIGRATIONS.last().expect("ladder non-empty").version,
        "the recreated DB is at the current schema version"
    );
    // The real schema exists: a read against `operations` succeeds and is empty.
    assert!(
        recent_operations(store.conn(), 10).expect("read works").is_empty(),
        "the recreated DB has the operations table and no rows"
    );
}

/// A fresh `OperationLogStore::open` on a new path builds the full production
/// schema at the current version.
#[test]
fn fresh_open_builds_current_schema() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let store = OperationLogStore::open(&path).expect("open fresh");
    assert_eq!(
        store.schema_version().expect("version"),
        MIGRATIONS.last().expect("ladder non-empty").version
    );
    assert!(recent_operations(store.conn(), 10).expect("read").is_empty());
}

// ── Interning ────────────────────────────────────────────────────────────────

/// Interning the same path twice returns the same id (dedup across operations —
/// a hot dir is stored once forever); sibling paths get distinct ids.
#[test]
fn intern_dir_dedups_and_distinguishes_siblings() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let conn = open_write_connection(&path).expect("write conn");

    let first = intern_dir(&conn, "vol-1", "/Users/me/project").expect("intern");
    let again = intern_dir(&conn, "vol-1", "/Users/me/project").expect("intern again");
    assert_eq!(first, again, "the same path interns to the same id");

    let sibling = intern_dir(&conn, "vol-1", "/Users/me/other").expect("intern sibling");
    assert_ne!(first, sibling, "a sibling path gets a distinct id");

    // The parent chain is shared: /Users/me is interned once for both leaves.
    let parent = intern_dir(&conn, "vol-1", "/Users/me").expect("intern parent");
    assert_ne!(parent, first);
    assert_ne!(parent, sibling);

    // A different volume with the same path is a different dir.
    let other_vol = intern_dir(&conn, "vol-2", "/Users/me/project").expect("intern other vol");
    assert_ne!(first, other_vol, "same path on a different volume is a distinct dir");

    // The round-trips reconstruct back to the original paths.
    assert_eq!(reconstruct_dir_path(&conn, first).expect("path"), "/Users/me/project");
    assert_eq!(reconstruct_dir_path(&conn, parent).expect("path"), "/Users/me");
}

/// A file directly at the volume root interns to the root anchor (name ""), and
/// that root reconstructs to "/".
#[test]
fn intern_dir_handles_the_volume_root() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let conn = open_write_connection(&path).expect("write conn");

    let root = intern_dir(&conn, "vol-1", "/").expect("intern root");
    let root_again = intern_dir(&conn, "vol-1", "").expect("intern empty");
    assert_eq!(root, root_again, "'/' and '' both resolve to the one root anchor");
    assert_eq!(reconstruct_dir_path(&conn, root).expect("path"), "/");
}

// ── Folding ──────────────────────────────────────────────────────────────────

/// `fold_name` lowercases (Unicode) and NFC-normalizes, so case variants and
/// decomposed/composed variants of one name fold to the same key.
#[test]
fn fold_name_folds_case_and_normalizes() {
    assert_eq!(fold_name("HeLLo.TXT"), "hello.txt");
    assert_eq!(fold_name("É"), fold_name("é"), "case folds");

    // A decomposed "e + combining acute" folds to the same key as composed "é".
    let decomposed = "Cafe\u{0301}"; // Cafe + U+0301
    let composed = "Café";
    assert_eq!(fold_name(decomposed), fold_name(composed), "NFC normalizes composition");
    assert_eq!(fold_name(composed), "café");
}

/// On macOS the interning identity folds case: the same path in different case
/// interns to one dir row (matching how the filesystem treats it).
#[test]
#[cfg(target_os = "macos")]
fn intern_dir_folds_case_on_macos() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let conn = open_write_connection(&path).expect("write conn");

    let mixed = intern_dir(&conn, "vol-1", "/Users/Me/Project").expect("intern");
    let lower = intern_dir(&conn, "vol-1", "/users/me/project").expect("intern lower");
    assert_eq!(mixed, lower, "case-variant paths intern to the same dir");
}
