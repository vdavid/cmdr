use std::path::{Path, PathBuf};

use cmdr_fs::sqlite_util;

use super::*;

/// A WAL-mode database at `path` holding `rows` rows, closed cleanly.
fn write_db(path: &Path, rows: i64) {
    let conn = sqlite_util::open(path).expect("open");
    conn.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE t (x INTEGER);")
        .expect("schema");
    for x in 0..rows {
        conn.execute("INSERT INTO t (x) VALUES (?1)", [x]).expect("insert");
    }
}

/// What a crash leaves behind: a database whose last `rows` rows are only in its
/// `-wal`, never checkpointed. Built by copying the files out from under a
/// connection that's still open, which is exactly the state a killed process
/// leaves on disk.
fn write_crashed_db(path: &Path, rows: i64) {
    let scratch = tempfile::tempdir().expect("scratch");
    let live = scratch.path().join("live.db");
    let conn = sqlite_util::open(&live).expect("open");
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0; CREATE TABLE t (x INTEGER);")
        .expect("schema");
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint the schema");
    for x in 0..rows {
        conn.execute("INSERT INTO t (x) VALUES (?1)", [x]).expect("insert");
    }
    let [live_db, live_wal, _] = sqlite_util::database_files(&live);
    let [db, wal, _] = sqlite_util::database_files(path);
    std::fs::copy(&live_db, &db).expect("copy db");
    std::fs::copy(&live_wal, &wal).expect("copy wal");
    assert!(
        std::fs::metadata(&wal).expect("wal").len() > 0,
        "the rows must be in the WAL"
    );
    drop(conn);
}

fn row_count(path: &Path) -> i64 {
    let conn = sqlite_util::open(path).expect("open");
    let ok: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity");
    assert_eq!(ok, "ok", "{} must be a sound database", path.display());
    conn.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
        .expect("count")
}

fn index_db(dir: &Path, volume_id: &str) -> PathBuf {
    dir.join(format!("index-{volume_id}.db"))
}

fn sidecars(db: &Path) -> [PathBuf; 2] {
    let [_, wal, shm] = sqlite_util::database_files(db);
    [wal, shm]
}

fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("root");
    let from = root.path().join("data");
    let to = root.path().join("cache").join("drive-index");
    std::fs::create_dir_all(&from).expect("from");
    (root, from, to)
}

#[test]
fn moves_a_volumes_index_and_leaves_nothing_behind() {
    let (_root, from, to) = dirs();
    write_db(&index_db(&from, "root"), 3);
    write_db(&index_db(&from, "smb-nas-1a2b"), 5);

    let report = relocate_drive_index(&from, &to);

    let mut moved = report.moved.clone();
    moved.sort();
    assert_eq!(moved, vec!["root".to_string(), "smb-nas-1a2b".to_string()]);
    assert!(report.discarded.is_empty() && report.left.is_empty(), "{report:?}");
    assert_eq!(row_count(&index_db(&to, "root")), 3);
    assert_eq!(row_count(&index_db(&to, "smb-nas-1a2b")), 5);
    for file in sqlite_util::database_files(&index_db(&from, "root")) {
        assert!(!file.exists(), "{} must not linger in the old folder", file.display());
    }
}

#[test]
fn folds_a_crashed_write_ahead_log_in_before_the_move() {
    let (_root, from, to) = dirs();
    write_crashed_db(&index_db(&from, "root"), 7);

    let report = relocate_drive_index(&from, &to);

    assert_eq!(report.moved, vec!["root".to_string()]);
    assert_eq!(
        row_count(&index_db(&to, "root")),
        7,
        "the rows that were only in the WAL must survive"
    );
    for file in sqlite_util::database_files(&index_db(&from, "root")) {
        assert!(!file.exists(), "{} must not linger in the old folder", file.display());
    }
}

#[test]
fn a_copy_already_in_the_new_folder_wins_and_the_old_one_goes() {
    // A downgrade ran an older build in between, which rebuilt an index in the
    // old folder; the one in the new folder is what this build has been keeping.
    let (_root, from, to) = dirs();
    std::fs::create_dir_all(&to).expect("to");
    write_db(&index_db(&from, "root"), 2);
    write_db(&index_db(&to, "root"), 9);

    let report = relocate_drive_index(&from, &to);

    assert_eq!(report.discarded, vec!["root".to_string()]);
    assert!(report.moved.is_empty());
    assert_eq!(row_count(&index_db(&to, "root")), 9);
    for file in sqlite_util::database_files(&index_db(&from, "root")) {
        assert!(!file.exists(), "{} must not linger in the old folder", file.display());
    }
}

#[test]
fn a_stray_log_in_the_new_folder_is_never_replayed_onto_the_moved_database() {
    // A `-wal` with no database beside it belongs to nothing. Left there, SQLite
    // would replay it onto the database that arrives, and that's corruption.
    let (_root, from, to) = dirs();
    std::fs::create_dir_all(&to).expect("to");
    write_db(&index_db(&from, "root"), 4);
    let stranger = tempfile::tempdir().expect("stranger");
    write_crashed_db(&index_db(stranger.path(), "root"), 50);
    let [stray_wal, _] = sidecars(&index_db(stranger.path(), "root"));
    let [to_wal, _] = sidecars(&index_db(&to, "root"));
    std::fs::copy(&stray_wal, &to_wal).expect("plant the stray WAL");

    let report = relocate_drive_index(&from, &to);

    assert_eq!(report.moved, vec!["root".to_string()]);
    assert_eq!(row_count(&index_db(&to, "root")), 4);
}

#[test]
fn orphaned_sidecars_in_the_old_folder_are_removed_and_never_moved() {
    let (_root, from, to) = dirs();
    let [wal, shm] = sidecars(&index_db(&from, "smb-gone"));
    std::fs::write(&wal, b"orphan").expect("wal");
    std::fs::write(&shm, b"orphan").expect("shm");

    let report = relocate_drive_index(&from, &to);

    assert_eq!(report.discarded, vec!["smb-gone".to_string()]);
    assert!(!wal.exists() && !shm.exists());
    for file in sidecars(&index_db(&to, "smb-gone")) {
        assert!(!file.exists(), "{} must not be carried over", file.display());
    }
}

#[test]
fn the_other_stores_and_the_apps_own_files_stay_put() {
    let (_root, from, to) = dirs();
    let staying = [
        "importance-root.db",
        "importance-root.db-wal",
        "media-root.db",
        "media-root.clip.usearch",
        "operation-log.db",
        "main.db",
        "settings.json",
    ];
    for name in staying {
        std::fs::write(from.join(name), b"keep").expect("write");
    }
    write_db(&index_db(&from, "root"), 1);

    relocate_drive_index(&from, &to);

    for name in staying {
        assert!(from.join(name).exists(), "{name} must stay in the data folder");
        assert!(!to.join(name).exists(), "{name} must not be moved");
    }
}

#[test]
fn a_second_run_finds_nothing_to_do() {
    let (_root, from, to) = dirs();
    write_db(&index_db(&from, "root"), 3);
    relocate_drive_index(&from, &to);

    assert_eq!(relocate_drive_index(&from, &to), DriveIndexRelocation::default());
    assert_eq!(row_count(&index_db(&to, "root")), 3);
}

#[test]
fn the_same_folder_is_a_no_op() {
    let (_root, from, _to) = dirs();
    write_db(&index_db(&from, "root"), 3);

    assert_eq!(relocate_drive_index(&from, &from), DriveIndexRelocation::default());
    assert_eq!(row_count(&index_db(&from, "root")), 3);
}

#[test]
fn a_missing_old_folder_is_a_no_op() {
    let root = tempfile::tempdir().expect("root");
    let report = relocate_drive_index(&root.path().join("never-made"), &root.path().join("to"));
    assert_eq!(report, DriveIndexRelocation::default());
}
