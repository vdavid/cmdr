//! Tests for the recursion-excluded-subtree prune.
//!
//! The load-bearing one is `prune_never_touches_a_lookalike_user_folder`: a bug in
//! the matcher here deletes a user's indexed files, so it's checked before
//! anything about what the prune DOES remove.

use crate::indexing::store::{EXCLUDED_SUBTREES_PRUNED_KEY, EntryRow, IndexStore, ROOT_ID};
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
    writer
        .send(WriteMessage::InsertEntriesV2(rows))
        .expect("seed rows");
    writer.flush_blocking().expect("flush");
    (writer, db_path, dir)
}

fn ids_present(db_path: &std::path::Path, ids: &[i64]) -> Vec<i64> {
    let conn = IndexStore::open_read_connection(db_path).expect("read conn");
    ids.iter()
        .copied()
        .filter(|id| {
            IndexStore::get_entry_by_id(&conn, *id)
                .expect("get entry")
                .is_some()
        })
        .collect()
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
