//! The `recursive_has_symlinks` flag: set on upsert, cleared when the last
//! symlink goes away, and cleared when a subtree holding one is deleted.

use super::*;

#[test]
fn upsert_symlink_propagates_recursive_has_symlinks_up() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Build a 2-level dir tree first (no symlinks).
    // ROOT -> outer (id=10) -> inner (id=11)
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "outer".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "inner".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Confirm baseline: no symlinks anywhere
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 11)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
    }

    // Add a symlink under inner via UpsertEntryV2
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 11,
            name: "link".into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Flag should propagate up to both inner and outer
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 11)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "inner should flip to true"
        );
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "outer should propagate from inner"
        );
    }

    writer.shutdown();
}

#[test]
fn delete_last_symlink_clears_recursive_has_symlinks_up() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // ROOT -> outer (id=20) -> link (id=21, symlink)
    let entries = vec![
        EntryRow {
            id: 20,
            parent_id: ROOT_ID,
            name: "outer".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 21,
            parent_id: 20,
            name: "link".into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Baseline: outer has the flag set
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 20)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
    }

    // Delete the only symlink
    writer.send(WriteMessage::DeleteEntryById(21)).unwrap();
    writer.flush_blocking().unwrap();

    // Flag should clear up the chain
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 20)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "outer should clear after last symlink removed"
        );
    }

    writer.shutdown();
}

#[test]
fn delete_subtree_with_symlinks_clears_parent_flag() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // ROOT -> top (id=30)
    //   ├── doomed (id=31) -> link (id=32, symlink)
    //   └── safe (id=33)  (no symlinks)
    let entries = vec![
        EntryRow {
            id: 30,
            parent_id: ROOT_ID,
            name: "top".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 31,
            parent_id: 30,
            name: "doomed".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 32,
            parent_id: 31,
            name: "link".into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 33,
            parent_id: 30,
            name: "safe".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Baseline: top has the flag
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 30)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
    }

    // Delete the doomed subtree (which contained the only symlink)
    writer.send(WriteMessage::DeleteSubtreeById(31)).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 30)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "top should clear once the subtree containing the symlink is gone"
        );
    }

    writer.shutdown();
}
