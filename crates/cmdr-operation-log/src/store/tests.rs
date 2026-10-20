//! Store tests: the migration ladder (the risky, first-of-its-kind logic —
//! TDD'd with a real red), dir interning, and case folding. The full
//! open→write→read round-trip through the writer lives in `writer.rs` tests.

use rusqlite::Transaction;

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
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("in-memory db");

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
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("in-memory db");
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
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("in-memory db");
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

/// A genuinely unparseable file (garbage bytes, not a SQLite DB) is handed to the
/// host's set-aside and recreated fresh — the ONLY case where recreating is
/// correct. The recreated DB is at the current schema version with the real
/// tables present. (What the app does with the file it's handed:
/// `operation_log_host`'s `an_unreadable_operation_log_is_kept_and_reported`.)
#[test]
fn unparseable_file_recreates_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    std::fs::write(&path, b"this is not a sqlite database at all").expect("write garbage");

    let kept = path.with_extension("db.kept");
    let handed = std::sync::Mutex::new(Vec::new());
    let set_aside = |unreadable: &Path| {
        handed
            .lock()
            .expect("no other holder panics")
            .push(unreadable.to_path_buf());
        std::fs::rename(unreadable, &kept).ok().map(|()| kept.clone())
    };
    let store = OperationLogStore::open(&path, &set_aside).expect("open recreates an unparseable file");
    assert_eq!(
        *handed.lock().expect("unpoisoned"),
        vec![path.clone()],
        "handed over once"
    );
    assert_eq!(
        std::fs::read(&kept).expect("read the copy"),
        b"this is not a sqlite database at all"
    );
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

/// A row written before `rollback_skip_reason` existed reads as "reason not recorded",
/// never as a plausible-looking default. The step adds the column nullable with NO
/// backfill on purpose: those rollbacks ran without recording why they left an item
/// alone, so there is no honest value to invent for them.
#[test]
fn a_pre_migration_item_row_reads_as_no_recorded_skip_reason() {
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("in-memory db");
    // Bring the DB to v1 only — the production schema before the column existed.
    run_migrations(&conn, &MIGRATIONS[..1]).expect("migrate to v1");
    // Raw SQL, not `intern_dir`: that speaks the CURRENT dir identity, which a v1
    // schema doesn't have.
    conn.execute(
        "INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (1, 'vol-1', NULL, '', '')",
        [],
    )
    .expect("insert a v1 dir");
    let dir_id = 1_i64;
    conn.execute(
        "INSERT INTO operations (op_id, kind, initiator, execution_status, rollback_state, started_at)
         VALUES ('op', 'rename', 'user', 'done', 'partially_rolled_back', 1)",
        [],
    )
    .expect("insert a v1 operation");
    conn.execute(
        "INSERT INTO operation_items
            (op_id, seq, entry_type, row_role, source_dir_id, source_name, source_name_folded, outcome)
         VALUES ('op', 0, 'file', 'rollback_unit', ?1, 'a.txt', 'a.txt', 'skipped')",
        rusqlite::params![dir_id],
    )
    .expect("insert a v1 item");

    run_migrations(&conn, MIGRATIONS).expect("migrate to the current version");

    let items = read_operation_items(&conn, "op", 10).expect("read items");
    assert_eq!(items.len(), 1, "the pre-migration row survives the step");
    assert_eq!(items[0].outcome, ItemOutcome::Skipped, "its outcome is untouched");
    assert_eq!(
        items[0].rollback_skip_reason, None,
        "a skip recorded before the column existed reads as not recorded"
    );
}

/// A fresh `OperationLogStore::open` on a new path builds the full production
/// schema at the current version.
#[test]
fn fresh_open_builds_current_schema() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let store = OperationLogStore::open(&path, &|_| None).expect("open fresh");
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

/// A remote volume's paths are rooted at `scheme://authority`, and that root has
/// to come back byte-for-byte: the rollback hands the rebuilt path to the volume.
/// Pre-fix the `//` was split away and `adb://serial/…` came back `/adb:/serial/…`.
#[test]
fn intern_dir_round_trips_a_scheme_rooted_path() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let conn = open_write_connection(&path).expect("write conn");

    for remote in [
        "adb://46061FDAS000A4/sdcard/Download",
        "sftp://ada@nas.local:22/home/ada",
        "smb://nas/share",
        // A file directly at the remote root.
        "mtp://device",
    ] {
        let id = intern_dir(&conn, "remote", remote).expect("intern");
        assert_eq!(reconstruct_dir_path(&conn, id).expect("path"), remote);
        assert_eq!(join_leaf(remote, "a.zip").to_str(), Some(&*format!("{remote}/a.zip")));
    }

    // The root is one shared row: two folders under it are siblings of each other.
    let download = intern_dir(&conn, "remote", "adb://46061FDAS000A4/sdcard/Download").expect("intern");
    let dcim = intern_dir(&conn, "remote", "adb://46061FDAS000A4/sdcard/DCIM").expect("intern");
    assert_ne!(download, dcim);

    // A local path is untouched by the scheme rule, colon or not.
    let local = intern_dir(&conn, "root", "/Users/me/a:b/c").expect("intern local");
    assert_eq!(reconstruct_dir_path(&conn, local).expect("path"), "/Users/me/a:b/c");
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

/// The interning identity is the EXACT name: case twins (and NFC/NFD twins) intern
/// to separate dir rows and each reconstructs to its own spelling. The log can't
/// know a volume's case rules, and on a case-sensitive one (SFTP to Linux, S3,
/// case-sensitive APFS) `/srv/Media` and `/srv/media` are different folders, so a
/// folded identity sent a rollback into the wrong twin (#527).
#[test]
fn intern_dir_keeps_case_and_normalization_twins_apart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let conn = open_write_connection(&path).expect("write conn");

    let upper = intern_dir(&conn, "vol-1", "/srv/Media").expect("intern upper");
    let lower = intern_dir(&conn, "vol-1", "/srv/media").expect("intern lower");
    assert_ne!(upper, lower, "case twins intern to separate dirs");
    assert_eq!(reconstruct_dir_path(&conn, upper).expect("path"), "/srv/Media");
    assert_eq!(reconstruct_dir_path(&conn, lower).expect("path"), "/srv/media");

    let composed = intern_dir(&conn, "vol-1", "/srv/Café").expect("intern NFC");
    let decomposed = intern_dir(&conn, "vol-1", "/srv/Cafe\u{0301}").expect("intern NFD");
    assert_ne!(composed, decomposed, "NFC and NFD spellings intern to separate dirs");

    assert_eq!(
        intern_dir(&conn, "vol-1", "/srv/media").expect("intern again"),
        lower,
        "the exact spelling still dedupes"
    );
}

/// The v3 step swaps the dir identity from the folded name to the exact one in
/// place. Rows a v2 DB already merged survive untouched (they can't be split: only
/// the first spelling was kept), and a NEW intern of the other spelling gets its own
/// row instead of reusing the merged one.
#[test]
fn v3_migration_keeps_merged_dirs_and_stops_merging_new_twins() {
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, &MIGRATIONS[..2]).expect("migrate to v2");
    // What a v2 build left behind: `/srv/Media` interned, folded identity.
    conn.execute_batch(
        "INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (1, 'vol-1', NULL, '', '');
         INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (2, 'vol-1', 1, 'srv', 'srv');
         INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (3, 'vol-1', 2, 'Media', 'media');",
    )
    .expect("seed v2 dirs");

    run_migrations(&conn, MIGRATIONS).expect("migrate to current");

    assert_eq!(
        intern_dir(&conn, "vol-1", "/srv/Media").expect("intern"),
        3,
        "the old row survives"
    );
    let lower = intern_dir(&conn, "vol-1", "/srv/media").expect("intern lower");
    assert_ne!(lower, 3, "the other spelling no longer merges into it");
    assert_eq!(reconstruct_dir_path(&conn, lower).expect("path"), "/srv/media");
}

/// The v4 step adds the dest identity without inventing one: a row a v3 build wrote
/// survives whole and reads as "identity not recorded", which keeps its undo on the
/// older checks rather than refusing it.
#[test]
fn v4_migration_keeps_older_rows_with_no_recorded_identity() {
    let conn = cmdr_fs::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, &MIGRATIONS[..3]).expect("migrate to v3");
    // What a v3 build left behind: a folder trashed from `/work` to `/.Trash`.
    conn.execute_batch(
        "INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (1, 'root', NULL, '', '');
         INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (2, 'root', 1, 'work', 'work');
         INSERT INTO dirs (dir_id, volume_id, parent_dir_id, name, name_folded) VALUES (3, 'root', 1, '.Trash', '.trash');
         INSERT INTO operations (op_id, kind, initiator, execution_status, rollback_state, started_at)
             VALUES ('op', 'trash', 'user', 'done', 'rollbackable', 1);
         INSERT INTO operation_items
             (op_id, seq, entry_type, row_role, source_dir_id, source_name, source_name_folded,
              dest_dir_id, dest_name, dest_name_folded, size, outcome)
             VALUES ('op', 0, 'dir', 'rollback_unit', 2, 'Invoices', 'invoices', 3, 'Invoices', 'invoices', 64, 'done');",
    )
    .expect("seed a v3 trash");

    run_migrations(&conn, MIGRATIONS).expect("migrate to current");

    let units = read_rollback_units_page(&conn, "op", i64::MAX, 10).expect("units");
    assert_eq!(units.len(), 1, "the pre-migration row survives the step");
    assert_eq!(units[0].dest_path, Some(PathBuf::from("/.Trash/Invoices")));
    assert_eq!(
        units[0].dest_identity, None,
        "no identity was recorded, so none is made up"
    );
}

/// Read-only connections open with the smaller page cache, write connections
/// with the bigger one. Both are upper bounds drawn from the process-wide slab;
/// budgets and rationale: `cmdr_fs::sqlite_util`.
#[test]
fn read_connections_get_a_smaller_page_cache_than_write_connections() {
    use cmdr_fs::sqlite_util::{READ_PAGE_CACHE_KIB, WRITE_PAGE_CACHE_KIB, page_cache_kib};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = operation_log_db_path(dir.path());
    let write = open_write_connection(&path).expect("write conn");
    let read = open_read_connection(&path).expect("read conn");

    assert_eq!(page_cache_kib(&write), WRITE_PAGE_CACHE_KIB);
    assert_eq!(page_cache_kib(&read), READ_PAGE_CACHE_KIB);
}
