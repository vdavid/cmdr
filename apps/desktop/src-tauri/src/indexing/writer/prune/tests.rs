//! Tests for the recursion-excluded-subtree prune.
//!
//! The load-bearing one is `prune_never_touches_a_lookalike_user_folder`: a bug in
//! the matcher here deletes a user's indexed files, so it's checked before
//! anything about what the prune DOES remove.

use crate::indexing::store::{
    EXCLUDED_SUBTREES_PRUNE_STARTED_KEY, EXCLUDED_SUBTREES_PRUNED_KEY, EntryRow, IndexStore, ROOT_ID,
};
use crate::indexing::writer::tests::setup_db;
use crate::indexing::writer::{IndexWriter, WriteMessage};

/// The names the production list carries, as the message would.
fn excluded_names() -> Vec<String> {
    ["@eaDir", "@Recently-Snapshot", "@Recycle", "System Volume Information"]
        .iter()
        .map(|n| (*n).to_string())
        .collect()
}

fn prune_msg() -> WriteMessage {
    WriteMessage::PruneExcludedSubtrees {
        excluded_dir_names: excluded_names(),
        fingerprint: "test-fingerprint".to_string(),
    }
}

fn dir(id: i64, parent_id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    }
}

fn file(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(size),
        physical_size: Some(size),
        modified_at: None,
        inode: None,
    }
}

/// Spawn a writer over a DB seeded with `rows`.
fn writer_with(rows: Vec<EntryRow>) -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
    let (db_path, dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    writer.send(WriteMessage::InsertEntriesV2(rows)).expect("seed rows");
    writer.flush_blocking().expect("flush");
    (writer, db_path, dir)
}

fn ids_present(db_path: &std::path::Path, ids: &[i64]) -> Vec<i64> {
    let conn = IndexStore::open_read_connection(db_path).expect("read conn");
    ids.iter()
        .copied()
        .filter(|id| IndexStore::get_entry_by_id(&conn, *id).expect("get entry").is_some())
        .collect()
}

// ── Crash safety: an interrupted prune must stay completable ─────

/// Insert one dir plus one file per level, `breadth` wide and `depth` deep,
/// under `parent`. Returns how many rows landed.
fn seed_subtree(conn: &rusqlite::Connection, parent: i64, depth: u32, breadth: usize) -> u64 {
    if depth == 0 {
        return 0;
    }
    let mut rows = 0;
    for i in 0..breadth {
        let d = IndexStore::insert_entry_v2(
            conn,
            parent,
            &format!("d{depth}-{i}"),
            true,
            false,
            None,
            None,
            None,
            None,
        )
        .expect("insert dir");
        IndexStore::insert_entry_v2(
            conn,
            d,
            &format!("f{depth}-{i}.bin"),
            false,
            false,
            Some(7),
            Some(7),
            None,
            None,
        )
        .expect("insert file");
        rows += 2 + seed_subtree(conn, d, depth - 1, breadth);
    }
    rows
}

/// A DB holding one `@Recycle` tree to prune and one real user tree that must
/// survive. Returns `(db_path, tempdir, excluded root id, rows under it)`.
fn db_with_an_excluded_tree() -> (std::path::PathBuf, tempfile::TempDir, i64, u64) {
    let (db_path, dir) = setup_db();
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    let excluded =
        IndexStore::insert_entry_v2(&conn, ROOT_ID, "@Recycle", true, false, None, None, None, None).expect("insert");
    let under = seed_subtree(&conn, excluded, 3, 2);
    let mine = IndexStore::insert_entry_v2(&conn, ROOT_ID, "photos", true, false, None, None, None, None)
        .expect("insert user dir");
    let user_rows = seed_subtree(&conn, mine, 2, 2);
    assert_eq!(
        (under, user_rows),
        (28, 12),
        "the fixture's shape is load-bearing below"
    );
    (db_path, dir, excluded, under)
}

/// Root + `@Recycle` + `photos` + the 12-row user tree: what any complete prune
/// of the fixture must leave behind.
const ROWS_AFTER_A_COMPLETE_PRUNE: u64 = 3 + 12;

fn entry_count(conn: &rusqlite::Connection) -> u64 {
    conn.query_row("SELECT count(*) FROM entries", [], |row| row.get::<_, u64>(0))
        .expect("count entries")
}

fn orphan_count(conn: &rusqlite::Connection) -> usize {
    IndexStore::find_orphan_entries(conn).expect("find orphans").0.len()
}

/// **The crash-safety guard.** Quitting the app mid-prune is ordinary: the prune
/// runs at startup and takes 20–30 s on a real NAS index. A top-down delete
/// severs the tree at the cut, so every row below it loses its path to the root
/// and NO later descent can ever reach it again — on the author's QNAP one
/// interrupted run left 9 793 362 rows the next run found nothing to do about.
///
/// The delete must be post-order: a directory row goes only once its whole
/// subtree is gone. Checked over EVERY prefix of the deletion order, and each
/// prefix must still be completable by a plain re-run.
#[test]
fn interrupting_a_subtree_delete_never_strands_a_row() {
    let (_, _fixture_dir, _, total) = db_with_an_excluded_tree();
    for stop_after in 1..=total {
        let (db_path, _dir, excluded, under) = db_with_an_excluded_tree();
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        let before = entry_count(&conn);

        let cut = IndexStore::delete_descendants_by_id_stopping_after(&conn, excluded, stop_after)
            .expect("interrupted delete");
        assert_eq!(cut, stop_after, "the simulated interruption must stop where asked");
        assert_eq!(
            orphan_count(&conn),
            0,
            "stopping after {stop_after} of {under} rows stranded rows unreachable from the root"
        );

        let rest = IndexStore::delete_descendants_by_id(&conn, excluded).expect("resume");
        assert_eq!(cut + rest, under, "a re-run must finish exactly the rows left over");
        assert_eq!(
            entry_count(&conn),
            before - under,
            "the resumed run must land on the same index an uninterrupted one would"
        );
    }
}

/// An interrupted prune must complete on the next launch through the real
/// message path, not only at the store level, and leave the DB byte-identical to
/// what one uninterrupted run produces.
#[test]
fn an_interrupted_prune_completes_on_the_next_run() {
    let (db_path, _dir, excluded, under) = db_with_an_excluded_tree();
    {
        // Simulate the quit: a partial delete plus the durable in-progress mark
        // the handler writes before its first delete.
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::delete_descendants_by_id_stopping_after(&conn, excluded, under / 2).expect("partial");
        IndexStore::update_meta(&conn, EXCLUDED_SUBTREES_PRUNE_STARTED_KEY, "1").expect("mark in progress");
    }

    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        orphan_count(&conn),
        0,
        "the finished prune must leave no unreachable row"
    );
    assert!(
        IndexStore::get_entry_by_id(&conn, excluded).expect("get").is_some(),
        "the excluded dir's own row still survives"
    );
    assert_eq!(
        entry_count(&conn),
        ROWS_AFTER_A_COMPLETE_PRUNE,
        "the resumed run lands on the index an uninterrupted one would"
    );
    assert_eq!(
        IndexStore::get_meta(&conn, EXCLUDED_SUBTREES_PRUNE_STARTED_KEY).expect("meta"),
        None,
        "a finished run clears the in-progress mark"
    );

    writer.shutdown();
}

/// Installs that already ran the old top-down prune carry rows severed from the
/// root. They're invisible to any descent, so re-running the prune alone can
/// never find them; the run has to sweep for rows whose parent is gone.
#[test]
fn the_prune_clears_rows_an_older_run_already_stranded() {
    let (db_path, _dir, excluded, _) = db_with_an_excluded_tree();
    let survivors = {
        // Sever exactly the way a top-down delete does: drop the excluded dir's
        // direct children, leaving their subtrees hanging off nothing.
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        let children: Vec<i64> = IndexStore::list_children_on(excluded, &conn)
            .expect("children")
            .into_iter()
            .map(|row| row.id)
            .collect();
        for id in &children {
            IndexStore::delete_entry_by_id(&conn, *id).expect("sever");
        }
        assert_eq!(orphan_count(&conn), 6, "test setup: the tree really is severed");
        entry_count(&conn)
    };
    assert_eq!(survivors, 41, "test setup: 26 stranded rows still sit in the DB");

    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(orphan_count(&conn), 0, "every stranded row must go");
    assert_eq!(
        entry_count(&conn),
        ROWS_AFTER_A_COMPLETE_PRUNE,
        "the user tree, `@Recycle`'s own row, and nothing else"
    );

    writer.shutdown();
}

/// **The data-safety guard.** Folders whose names only LOOK reserved are real
/// user data (the local walker indexes such folders in full, and a user can make
/// one on a NAS too). The prune must not touch a single row of them: substring,
/// prefix, or suffix matching here would silently delete someone's files from
/// their index.
#[test]
fn prune_never_touches_a_lookalike_user_folder() {
    let lookalikes = [
        "snapshot",
        "eaDir",
        "recycle",
        "@myfiles",
        "@Recently-Snapshotted",
        "@eaDirectory",
        "System Volume Information Archive",
        "my @Recycle backup",
        ".snapshot-backup",
    ];
    let mut rows = Vec::new();
    let mut child_ids = Vec::new();
    for (i, name) in lookalikes.iter().enumerate() {
        let base = 100 + (i as i64) * 10;
        rows.push(dir(base, ROOT_ID, name));
        rows.push(dir(base + 1, base, "deep"));
        rows.push(file(base + 2, base + 1, "mine.txt", 42));
        child_ids.extend([base, base + 1, base + 2]);
    }
    let (writer, db_path, _dir) = writer_with(rows);

    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");

    assert_eq!(
        ids_present(&db_path, &child_ids),
        child_ids,
        "every row of a look-alike user folder must survive the prune"
    );

    writer.shutdown();
}

/// The prune keeps the excluded dir's OWN row (it stays listed and navigable, a
/// deliberate invariant) and removes exactly its subtree, at any depth and with
/// any casing, leaving siblings and ancestors alone.
#[test]
fn prune_keeps_the_excluded_dir_and_removes_only_its_subtree() {
    let rows = vec![
        // A real tree that must survive intact.
        dir(10, ROOT_ID, "photos"),
        file(11, 10, "holiday.jpg", 100),
        // A nested, differently-cased system dir inside it.
        dir(20, 10, "@EADIR"),
        dir(21, 20, "thumbs"),
        file(22, 21, "thumb.jpg", 999),
        file(23, 20, "SYNOPHOTO.jpg", 999),
        // A system dir at the index root.
        dir(30, ROOT_ID, "@Recycle"),
        file(31, 30, "deleted.bin", 5000),
    ];
    let (writer, db_path, _dir) = writer_with(rows);

    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");

    assert_eq!(
        ids_present(&db_path, &[10, 11, 20, 30]),
        vec![10, 11, 20, 30],
        "the real tree AND both system dirs' own rows must survive"
    );
    assert_eq!(
        ids_present(&db_path, &[21, 22, 23, 31]),
        Vec::<i64>::new(),
        "every descendant of a system dir must go, at any depth"
    );

    writer.shutdown();
}

/// Nested excluded dirs overlap: a snapshot tree holds a full copy of the share,
/// `@Recycle` and further `@Recently-Snapshot` dirs included (24 of each on the
/// author's QNAP). Descending from the outer root already deletes the inner one's
/// row, and the inner pass must simply find nothing left rather than resurrect it.
#[test]
fn prune_handles_excluded_dirs_nested_inside_each_other() {
    let rows = vec![
        dir(10, ROOT_ID, "@Recently-Snapshot"),
        dir(11, 10, "GMT+01_2026-06-28_0100"),
        dir(12, 11, "@Recycle"),
        file(13, 12, "old.bin", 7),
        dir(14, 11, "@Recently-Snapshot"),
        file(15, 14, "older.bin", 7),
        file(16, 11, "copy-of-a-real-file.txt", 7),
    ];
    let (writer, db_path, _dir) = writer_with(rows);

    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");

    assert_eq!(
        ids_present(&db_path, &[10, 11, 12, 13, 14, 15, 16]),
        vec![10],
        "only the OUTERMOST excluded dir's row survives; everything under it goes"
    );

    writer.shutdown();
}

/// A dir an older index DID list carries a non-zero `listed_epoch` and a fat
/// `dir_stats` row. Once its subtree is gone, leaving those would make it claim an
/// exact `0 B` for a folder that really holds terabytes. It must read as unknown.
#[test]
fn prune_resets_the_excluded_dir_to_never_listed() {
    let rows = vec![dir(10, ROOT_ID, "@Recently-Snapshot"), file(11, 10, "big.bin", 9_000)];
    let (writer, db_path, _dir) = writer_with(rows);
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![10],
            epoch: 7,
        })
        .expect("mark listed");
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: crate::indexing::writer::AggSource::Sql,
        })
        .expect("aggregate");
    writer.flush_blocking().expect("flush");
    {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, 10).expect("epoch"),
            Some(7),
            "test setup: the older index listed this dir"
        );
    }

    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, 10).expect("epoch"),
        Some(0),
        "a dir the scanner won't descend into was never listed: coverage is unknown"
    );
    assert!(
        IndexStore::get_dir_stats_by_id(&conn, 10).expect("stats").is_none(),
        "the stale inflated dir_stats row must go, so nothing claims the old total"
    );

    writer.shutdown();
}

/// The fingerprint marker is what makes the load-time heal one-shot, and what
/// re-arms every existing index when the exclusion list grows.
#[test]
fn prune_records_the_list_fingerprint_and_is_idempotent() {
    let rows = vec![dir(10, ROOT_ID, "@Recycle"), file(11, 10, "gone.bin", 9)];
    let (writer, db_path, _dir) = writer_with(rows);

    writer.send(prune_msg()).expect("send prune");
    writer.flush_blocking().expect("flush");
    {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            IndexStore::get_meta(&conn, EXCLUDED_SUBTREES_PRUNED_KEY).expect("meta"),
            Some("test-fingerprint".to_string()),
        );
    }

    // A second run over an already-pruned DB changes nothing.
    writer.send(prune_msg()).expect("send prune again");
    writer.flush_blocking().expect("flush");
    assert_eq!(
        ids_present(&db_path, &[10, 11]),
        vec![10],
        "re-running the prune is a no-op on an already-pruned index"
    );

    writer.shutdown();
}
