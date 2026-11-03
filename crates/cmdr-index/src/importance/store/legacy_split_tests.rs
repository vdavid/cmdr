//! The split of an older build's combined importance store: every crash point
//! between its steps, a re-run after a partial split, and the files it must
//! leave alone. What's at stake is the user's navigation history, so each test
//! states the visit rows it expects to find, counted exactly once.

use std::path::{Path, PathBuf};

use cmdr_fs::sqlite_util;

use super::*;
use crate::importance::store::visits::all_visits;
use crate::importance::store::{ImportanceStore, needs_full_pass};

const ID: &str = "root";

/// The schema-3 combined store every older build wrote: weights AND visits.
const LEGACY_SCHEMA: &str = "
    CREATE TABLE weights (
        path_folded TEXT PRIMARY KEY, path TEXT NOT NULL, score REAL NOT NULL,
        signals TEXT NOT NULL, as_of_generation INTEGER NOT NULL
    ) WITHOUT ROWID;
    CREATE TABLE visits (
        path_folded TEXT PRIMARY KEY, path TEXT NOT NULL,
        visit_count INTEGER NOT NULL DEFAULT 0, last_visit_secs INTEGER NOT NULL DEFAULT 0
    ) WITHOUT ROWID;
    CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
    INSERT INTO meta VALUES ('schema_version', '3');
    INSERT INTO meta VALUES ('recompute_generation', '9');
";

/// A data dir and a cache dir apart, as the app lays them out.
struct Dirs {
    _root: tempfile::TempDir,
    dirs: StoreDirs,
}

impl Dirs {
    fn apart() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let dirs = StoreDirs {
            data: root.path().join("data"),
            drive_index: root.path().join("cache"),
        };
        std::fs::create_dir_all(&dirs.data).expect("data");
        std::fs::create_dir_all(&dirs.drive_index).expect("cache");
        Self { _root: root, dirs }
    }

    fn together() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let dirs = StoreDirs::single(root.path());
        Self { _root: root, dirs }
    }

    fn old(&self) -> PathBuf {
        VolumeStore::Importance.db_path(&self.dirs.data, ID)
    }

    fn weights(&self) -> PathBuf {
        self.dirs.db_path(VolumeStore::Importance, ID)
    }

    fn visits(&self) -> PathBuf {
        self.dirs.db_path(VolumeStore::Visits, ID)
    }
}

/// Write an older build's combined store at `path` holding `visits` as
/// `(path, count, last)`, plus one weight row, closed cleanly.
fn write_legacy(path: &Path, visits: &[(&str, i64, i64)]) {
    let conn = sqlite_util::open(path).expect("open");
    conn.execute_batch("PRAGMA journal_mode = WAL;").expect("wal");
    conn.execute_batch(LEGACY_SCHEMA).expect("schema");
    conn.execute(
        "INSERT INTO weights VALUES ('/users/me', '/Users/me', 0.7, '{}', 9)",
        [],
    )
    .expect("weight");
    for (p, count, last) in visits {
        conn.execute(
            "INSERT INTO visits VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![normalize_for_comparison(p), p, count, last],
        )
        .expect("visit");
    }
}

/// What a killed older build leaves: the combined store whose visit rows are
/// only in its `-wal`, never checkpointed. Built by copying the files out from
/// under a connection that's still open.
fn write_crashed_legacy(path: &Path, visits: &[(&str, i64, i64)]) {
    let scratch = tempfile::tempdir().expect("scratch");
    let live = scratch.path().join("live.db");
    let conn = sqlite_util::open(&live).expect("open");
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;")
        .expect("wal");
    conn.execute_batch(LEGACY_SCHEMA).expect("schema");
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint the schema");
    for (p, count, last) in visits {
        conn.execute(
            "INSERT INTO visits VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![normalize_for_comparison(p), p, count, last],
        )
        .expect("visit");
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

fn none_of(db: &Path) -> bool {
    sqlite_util::database_files(db).iter().all(|file| !file.exists())
}

fn rows(visits: &[(&str, u64, u64)]) -> Vec<(String, u64, u64)> {
    let mut rows: Vec<_> = visits.iter().map(|(p, c, l)| (p.to_string(), *c, *l)).collect();
    rows.sort();
    rows
}

const TWO_VISITS: [(&str, i64, i64); 2] = [("/Users/me/projects", 12, 1_000), ("/Users/me/Downloads", 3, 900)];

fn two_visits() -> Vec<(String, u64, u64)> {
    rows(&[("/Users/me/projects", 12, 1_000), ("/Users/me/Downloads", 3, 900)])
}

// ── The whole split ──────────────────────────────────────────────────────────

#[test]
fn the_visits_move_to_their_own_file_and_the_old_store_goes() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert!(report.discarded.is_empty() && report.left.is_empty(), "{report:?}");
    assert_eq!(all_visits(&d.visits()), two_visits());
    assert!(none_of(&d.old()), "the old file and its sidecars are gone");
    assert!(
        !d.weights().exists(),
        "the weights aren't carried over; a pass rebuilds them"
    );
}

/// The split leaves nothing behind that would stop the volume being scored: the
/// launch probe sees no weights and asks for the one full pass.
#[test]
fn after_a_split_the_volume_is_due_its_full_pass() {
    let d = Dirs::together();
    write_legacy(&d.old(), &TWO_VISITS);

    split_legacy_stores(&d.dirs);

    assert!(
        needs_full_pass(&d.dirs.data, ID, None).expect("probe"),
        "generation 0 after the split"
    );
    assert_eq!(
        all_visits(&d.visits()),
        two_visits(),
        "and the probe's recreate kept them"
    );
}

/// Rows an older build committed but never checkpointed live only in the `-wal`.
/// They count as much as any other.
#[test]
fn visits_only_in_the_old_write_ahead_log_come_over_too() {
    let d = Dirs::apart();
    write_crashed_legacy(&d.old(), &TWO_VISITS);

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert_eq!(all_visits(&d.visits()), two_visits());
    assert!(none_of(&d.old()));
}

#[test]
fn an_empty_visit_table_still_splits_and_the_old_store_goes() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &[]);

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 0)]);
    assert!(all_visits(&d.visits()).is_empty());
    assert!(none_of(&d.old()));
}

#[test]
fn with_no_old_store_nothing_happens() {
    let d = Dirs::apart();

    assert_eq!(split_legacy_stores(&d.dirs), LegacySplit::default());
    assert!(!d.visits().exists(), "no visits file is created for nothing");
}

/// Every volume splits on its own: one that can't doesn't hold up the others.
#[test]
fn each_volume_splits_on_its_own() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    let other = VolumeStore::Importance.db_path(&d.dirs.data, "smb-nas");
    write_legacy(&other, &[("/share/photos", 1, 5)]);
    std::fs::create_dir(VolumeStore::Visits.db_path(&d.dirs.data, "smb-nas")).expect("block it");

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert_eq!(report.left.len(), 1);
    assert_eq!(report.left[0].0, "smb-nas");
    assert!(none_of(&d.old()));
    assert!(other.exists());
}

// ── Crash points ─────────────────────────────────────────────────────────────

/// Step 1 done, step 2 not: the old file carries its token, nothing was copied. The
/// re-run copies once.
#[test]
fn a_crash_after_the_token_redoes_the_copy_once() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    let token = stamp_token(&sqlite_util::open(&d.old()).expect("open")).expect("token");

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert_eq!(all_visits(&d.visits()), two_visits());
    let marked: i64 = sqlite_util::open(&d.visits())
        .expect("visits")
        .query_row("SELECT rows FROM split_from WHERE token = ?1", [&token], |r| r.get(0))
        .expect("the token the old file carried is the one marked");
    assert_eq!(marked, 2);
}

/// Step 2 interrupted before its commit: the transaction rolls back whole, so the
/// new file holds none of it and no mark. The re-run copies once.
#[test]
fn a_crash_inside_the_copy_leaves_nothing_half_copied() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    {
        let old = sqlite_util::open(&d.old()).expect("open");
        let token = stamp_token(&old).expect("token");
        let mut new = crate::importance::store::visits::open_write_connection(&d.visits()).expect("new");
        let tx = new.transaction().expect("tx");
        tx.execute(
            "INSERT INTO visits VALUES ('/users/me/projects', '/Users/me/projects', 12, 1000)",
            [],
        )
        .expect("half the copy");
        tx.execute("INSERT INTO split_from VALUES (?1, 2, 0)", [&token])
            .expect("mark");
        // Dropped without a commit: what a crash before the commit leaves.
    }
    assert!(all_visits(&d.visits()).is_empty(), "test setup: nothing landed");

    split_legacy_stores(&d.dirs);

    assert_eq!(all_visits(&d.visits()), two_visits());
    assert!(none_of(&d.old()));
}

/// Step 2 committed, step 3 not: the old file is still there with its token
/// marked. The re-run deletes it and copies nothing, so no visit counts twice.
#[test]
fn a_crash_after_the_copy_only_deletes_the_old_store() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    let copied = copy_visits(&open_old(&d.old()).expect("open").expect("visits"), &d.visits(), now()).expect("copy");
    assert_eq!(copied, 2, "test setup: the copy committed");

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 0)], "nothing copied the second time");
    assert_eq!(all_visits(&d.visits()), two_visits(), "each visit counted once");
    assert!(none_of(&d.old()));
}

/// Step 3 interrupted between unlinks: the database is gone but a sidecar isn't.
/// The re-run deletes it, which matters most when both dirs are one folder: the
/// weights store created at that path next must never replay a stray log.
#[test]
fn a_crash_inside_the_delete_leaves_sidecars_the_next_run_clears() {
    for d in [Dirs::apart(), Dirs::together()] {
        write_legacy(&d.old(), &TWO_VISITS);
        split_legacy_stores(&d.dirs);
        let [_, wal, shm] = sqlite_util::database_files(&d.old());
        std::fs::write(&wal, b"a log that belongs to no database").expect("stray wal");
        std::fs::write(&shm, [0u8; 64]).expect("stray shm");

        let report = split_legacy_stores(&d.dirs);

        assert_eq!(report.discarded, [ID.to_string()]);
        assert!(none_of(&d.old()));
        assert_eq!(all_visits(&d.visits()), two_visits(), "the visits are untouched");
    }
}

/// A finished split re-run is a no-op.
#[test]
fn a_second_run_after_a_full_split_does_nothing() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    split_legacy_stores(&d.dirs);

    assert_eq!(split_legacy_stores(&d.dirs), LegacySplit::default());
    assert_eq!(all_visits(&d.visits()), two_visits());
}

// ── Files it must handle with care ──────────────────────────────────────────

/// A file SQLite can't read as a database has nothing to save: it goes, as an
/// older build would have deleted it on open, and no visits file appears.
#[test]
fn an_unreadable_old_store_is_discarded() {
    let d = Dirs::apart();
    std::fs::write(d.old(), b"this was never a database, just bytes that say so").expect("garbage");

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.discarded, [ID.to_string()]);
    assert!(none_of(&d.old()));
    assert!(all_visits(&d.visits()).is_empty());
}

/// A passing failure (here, the visits file's path is taken by a folder) never
/// costs the history: the old file stays whole for the next launch.
#[test]
fn a_failure_that_may_pass_leaves_the_old_store_whole() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    std::fs::create_dir(d.visits()).expect("block the visits file");

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.left.len(), 1, "{report:?}");
    assert!(
        crate::importance::store::holds_visits(&d.old()),
        "every visit still there"
    );

    std::fs::remove_dir(d.visits()).expect("unblock");
    let report = split_legacy_stores(&d.dirs);
    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert_eq!(all_visits(&d.visits()), two_visits());
}

/// With the dirs apart, an importance file in the data dir that holds no visits is
/// nobody's: this build keeps its weights elsewhere.
#[test]
fn apart_a_weights_only_file_in_the_data_dir_is_a_stray() {
    let d = Dirs::apart();
    ImportanceStore::open(&d.old()).expect("a weights store in the wrong folder");

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.discarded, [ID.to_string()]);
    assert!(none_of(&d.old()));
}

/// With one folder, the weights store sits at the old path, and it's this build's
/// own: left alone, rows and all.
#[test]
fn together_this_builds_weights_store_is_left_alone() {
    let d = Dirs::together();
    let writer = crate::importance::writer::ImportanceWriter::spawn(&d.weights(), Some(&d.visits())).expect("writer");
    {
        ImportanceStore::open(&d.weights()).expect("schema");
        writer.record_visit("/Users/me", 1).expect("visit");
        writer
            .write_weights(
                3,
                vec![crate::importance::writer::WeightRow {
                    path: "/Users/me".to_string(),
                    score: 0.5,
                    signals_json: "{}".to_string(),
                }],
            )
            .expect("weights");
        writer.flush_blocking().expect("flush");
    }
    writer.shutdown();

    assert_eq!(split_legacy_stores(&d.dirs), LegacySplit::default());
    let store = ImportanceStore::open(&d.weights()).expect("still there");
    assert_eq!(store.recompute_generation().expect("generation"), 3);
    assert_eq!(all_visits(&d.visits()), rows(&[("/Users/me", 1, 1)]));
}

/// With one folder the combined store IS at the weights path; it splits the same.
#[test]
fn together_the_combined_store_splits_too() {
    let d = Dirs::together();
    write_legacy(&d.old(), &TWO_VISITS);

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert_eq!(all_visits(&d.visits()), two_visits());
    assert!(none_of(&d.old()));
}

/// Unix seconds now, for visits an older build records AFTER a split.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64
}

/// The downgrade round trip: after a split, an older build finds no combined store,
/// starts a fresh one and records visits in it, and the user upgrades again. That
/// file is a different one (a new token) whose visits all happened after the split,
/// so they ADD to what the visits file already holds.
#[test]
fn a_downgrade_round_trip_merges_both_histories() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    split_legacy_stores(&d.dirs);

    // The older build's session: one known folder again, one new.
    let later = now() + 100;
    write_legacy(
        &d.old(),
        &[("/Users/me/projects", 2, later), ("/Users/me/Music", 1, later)],
    );

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.split, [(ID.to_string(), 2)]);
    assert_eq!(
        all_visits(&d.visits()),
        rows(&[
            ("/Users/me/projects", 14, later as u64),
            ("/Users/me/Downloads", 3, 900),
            ("/Users/me/Music", 1, later as u64),
        ])
    );
    assert!(none_of(&d.old()));
}

/// The copy commits and marks its token, the delete fails, and the user runs an
/// older build, which keeps recording into the SAME file (same token). The next
/// split adds exactly what that build wrote, never zero and never the whole file
/// again.
#[test]
fn an_older_build_writing_under_a_marked_token_adds_only_what_it_wrote() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    copy_visits(&open_old(&d.old()).expect("open").expect("visits"), &d.visits(), now()).expect("copy");

    // The older build's session in the combined file it still finds.
    let later = now() + 100;
    {
        let conn = sqlite_util::open(&d.old()).expect("open");
        conn.execute(
            "UPDATE visits SET visit_count = 15, last_visit_secs = ?1 WHERE path = '/Users/me/projects'",
            [later],
        )
        .expect("three more visits");
        conn.execute(
            "INSERT INTO visits VALUES (?1, '/Users/me/Music', 1, ?2)",
            rusqlite::params![normalize_for_comparison("/Users/me/Music"), later],
        )
        .expect("a new folder");
    }

    split_legacy_stores(&d.dirs);

    assert_eq!(
        all_visits(&d.visits()),
        rows(&[
            ("/Users/me/projects", 15, later as u64),
            ("/Users/me/Downloads", 3, 900),
            ("/Users/me/Music", 1, later as u64),
        ])
    );
    assert!(none_of(&d.old()));
}

/// Time Machine brings back a copy of the old combined store after the split,
/// taken either before the split stamped it or after. Its visits are history the
/// visits file already holds, so they add nothing.
#[test]
fn a_restored_copy_of_a_split_store_adds_nothing() {
    for stamped in [false, true] {
        let d = Dirs::apart();
        write_legacy(&d.old(), &TWO_VISITS);
        let backup = d.dirs.data.join("backup.db");
        if stamped {
            copy_visits(&open_old(&d.old()).expect("open").expect("visits"), &d.visits(), now()).expect("copy");
        }
        sqlite_util::open(&d.old())
            .expect("open")
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("fold");
        std::fs::copy(d.old(), &backup).expect("back it up");
        split_legacy_stores(&d.dirs);

        std::fs::rename(&backup, d.old()).expect("restore it");
        split_legacy_stores(&d.dirs);

        assert_eq!(all_visits(&d.visits()), two_visits(), "stamped: {stamped}");
        assert!(none_of(&d.old()));
    }
}

/// Damage found in the NEW visits file mid-copy says nothing about the old one:
/// the old store stays whole for the next launch rather than being discarded.
#[test]
fn damage_in_the_new_visits_file_leaves_the_old_store_whole() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    {
        let conn = crate::importance::store::visits::open_write_connection(&d.visits()).expect("new");
        let tx = conn.unchecked_transaction().expect("tx");
        for i in 0..2_000 {
            tx.execute(
                "INSERT INTO visits VALUES (?1, ?1, 1, 1)",
                [format!("/padding/folder-{i:05}-with-a-longer-name-to-fill-pages")],
            )
            .expect("pad");
        }
        tx.commit().expect("commit");
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").expect("fold");
    }
    let root_page: u64 = sqlite_util::open(&d.visits())
        .expect("open")
        .query_row("SELECT rootpage FROM sqlite_master WHERE name = 'visits'", [], |r| {
            r.get(0)
        })
        .expect("root page");
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(d.visits())
            .expect("open file");
        file.seek(SeekFrom::Start((root_page - 1) * 4096)).expect("seek");
        file.write_all(&[0xFF; 4096]).expect("damage the visits table");
    }

    let report = split_legacy_stores(&d.dirs);

    assert_eq!(report.left.len(), 1, "{report:?}");
    assert!(report.discarded.is_empty(), "{report:?}");
    assert!(
        crate::importance::store::holds_visits(&d.old()),
        "every old visit still there"
    );
}

/// The copy and the token are what the delete relies on, so both reach the disk
/// before it: full syncs, and `F_FULLFSYNC` on macOS.
#[test]
fn the_copy_and_the_token_are_written_durably() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    let pragma = |conn: &Connection, name: &str| -> i64 {
        conn.query_row(&format!("PRAGMA {name}"), [], |r| r.get(0)).expect(name)
    };
    let old = open_old(&d.old()).expect("open").expect("visits");
    let new = open_new(&d.visits()).expect("new");
    for conn in [&old, &new] {
        assert_eq!(pragma(conn, "synchronous"), 2, "FULL");
        assert_eq!(pragma(conn, "fullfsync"), 1);
        assert_eq!(pragma(conn, "checkpoint_fullfsync"), 1);
    }
}

/// The clock moved back after a split, so an older build's fresh visits carry
/// timestamps from before it. A row that beats what the ledger saw for its path is
/// still new history and adds, rather than being taken for a restored copy.
#[test]
fn a_clock_set_back_after_a_split_still_adds_a_downgrades_visits() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);
    split_legacy_stores(&d.dirs);

    write_legacy(
        &d.old(),
        &[("/Users/me/projects", 13, 500), ("/Users/me/Music", 1, 500)],
    );
    split_legacy_stores(&d.dirs);

    assert_eq!(
        all_visits(&d.visits()),
        rows(&[
            ("/Users/me/projects", 25, 1_000),
            ("/Users/me/Downloads", 3, 900),
            ("/Users/me/Music", 1, 500),
        ])
    );
}

/// A panic in the split or in its report still ends the background run: nothing
/// waits out the cap behind it.
#[test]
fn a_panicking_background_split_still_lets_waiters_go() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);

    split_in_background(d.dirs.clone(), |_| panic!("the report panics"));
    let started = std::time::Instant::now();
    wait_for_background_split();

    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

/// The split runs off the caller's thread; whatever needs the visits waits for it.
#[test]
fn a_background_split_is_waited_for() {
    let d = Dirs::apart();
    write_legacy(&d.old(), &TWO_VISITS);

    split_in_background(d.dirs.clone(), |_| {});
    wait_for_background_split();

    assert_eq!(all_visits(&d.visits()), two_visits());
    assert!(none_of(&d.old()));
}
