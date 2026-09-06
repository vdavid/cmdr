//! `MoveEntryV2`: same-parent renames, destination collisions with a file
//! and with a dir subtree, cross-parent delta and `recursive_has_symlinks`
//! propagation, the no-op case, and the writer-generation bump.

use super::*;

#[test]
fn move_entry_v2_same_parent_preserves_dir_stats() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Parent dir + child dir with non-trivial dir_stats. The whole point
    // of MoveEntryV2 vs. delete+insert is preserving these numbers.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "home",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 5_000,
            recursive_physical_size: 5_000,
            recursive_file_count: 7,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "Foo",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 5_000,
            recursive_physical_size: 5_000,
            recursive_file_count: 7,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    // Same-parent rename: "Foo" → "Bar".
    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 10,
            new_name: "Bar".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(entry.name, "Bar", "name should be updated");
    assert_eq!(entry.parent_id, 10, "parent unchanged");

    let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(
        moved_stats.recursive_logical_size, 5_000,
        "moved dir keeps its own stats"
    );
    assert_eq!(moved_stats.recursive_file_count, 7);

    let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        parent_stats.recursive_logical_size, 5_000,
        "parent stats unchanged for same-parent rename"
    );
    assert_eq!(parent_stats.recursive_file_count, 7);
    assert_eq!(parent_stats.recursive_dir_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_destination_collision_replaces_conflicting_file() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // One dir with two files. Moving "draft.txt" onto "final.txt"'s name
    // (a rename-with-overwrite, or a concurrent upsert racing ahead of the
    // move) used to fail the UNIQUE (parent_id, name_folded) constraint and
    // leave the moved entry stuck at its old name.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "docs",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 150,
            recursive_physical_size: 150,
            recursive_file_count: 2,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_file(&writer, 20, 10, "draft.txt", 100);
    insert_file(&writer, 21, 10, "final.txt", 50);

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 10,
            new_name: "final.txt".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(moved.name, "final.txt", "moved entry owns the destination name");
    assert_eq!(moved.parent_id, 10);
    assert!(
        IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
        "conflicting entry is deleted"
    );

    // The conflicting file's contribution is subtracted from the parent.
    let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(parent_stats.recursive_logical_size, 100);
    assert_eq!(parent_stats.recursive_file_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_destination_collision_replaces_conflicting_dir_subtree() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // A/proj (id 20, rich dir_stats) moves to B/proj, but B already has a
    // stale dir row "proj" (id 21) with a child file. The stale subtree must
    // go and the moved dir must keep its id and dir_stats.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 1000,
            recursive_physical_size: 1000,
            recursive_file_count: 3,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 500,
            recursive_physical_size: 500,
            recursive_file_count: 1,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "proj",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 1000,
            recursive_physical_size: 1000,
            recursive_file_count: 3,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        21,
        11,
        "proj",
        DirStatsById {
            entry_id: 21,
            recursive_logical_size: 500,
            recursive_physical_size: 500,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_file(&writer, 22, 21, "old.txt", 500);

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 11,
            new_name: "proj".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(moved.parent_id, 11, "moved dir landed under B");
    assert_eq!(moved.name, "proj");
    assert!(
        IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
        "conflicting dir is deleted"
    );
    assert!(
        IndexStore::get_entry_by_id(&conn, 22).unwrap().is_none(),
        "conflicting dir's children are deleted"
    );

    let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(
        moved_stats.recursive_logical_size, 1000,
        "moved dir keeps its own stats"
    );
    assert_eq!(moved_stats.recursive_file_count, 3);

    // A lost the moved dir's contribution entirely.
    let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0);
    assert_eq!(a_stats.recursive_file_count, 0);
    assert_eq!(a_stats.recursive_dir_count, 0);

    // B lost the stale subtree (-500, -1 file, -1 dir) and gained the moved
    // dir (+1000, +3 files, +1 dir).
    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 1000);
    assert_eq!(b_stats.recursive_file_count, 3);
    assert_eq!(b_stats.recursive_dir_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_cross_parent_propagates_deltas() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Two sibling dirs A and B, each with their own pre-populated stats.
    // Then a child dir D under A with non-trivial stats.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 1024,
            recursive_physical_size: 2048,
            recursive_file_count: 5,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "D",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 1024,
            recursive_physical_size: 2048,
            recursive_file_count: 5,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 11,
            new_name: "D".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // D itself: same dir_stats, new parent.
    let d_entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(d_entry.parent_id, 11);
    let d_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(d_stats.recursive_logical_size, 1024);
    assert_eq!(d_stats.recursive_file_count, 5);

    // A: lost D's contribution (size 1024, 5 files, 1 dir for D itself).
    let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0);
    assert_eq!(a_stats.recursive_physical_size, 0);
    assert_eq!(a_stats.recursive_file_count, 0);
    assert_eq!(a_stats.recursive_dir_count, 0);

    // B: gained D's contribution.
    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 1024);
    assert_eq!(b_stats.recursive_physical_size, 2048);
    assert_eq!(b_stats.recursive_file_count, 5);
    assert_eq!(b_stats.recursive_dir_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_file_cross_parent_propagates_deltas() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Two parent dirs, both starting with empty stats.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 700,
            recursive_physical_size: 700,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    // Insert a file under A (size 700, contributes 1 file).
    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id: 30,
            parent_id: 10,
            name: "f.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(700),
            physical_size: Some(700),
            modified_at: Some(1700000000),
            inode: Some(99),
        }]))
        .unwrap();
    writer.flush_blocking().unwrap();

    // Move file to B.
    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 30,
            new_parent_id: 11,
            new_name: "f.txt".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0, "A loses the file's size");
    assert_eq!(a_stats.recursive_file_count, 0);

    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 700);
    assert_eq!(b_stats.recursive_file_count, 1);
    assert_eq!(b_stats.recursive_dir_count, 0, "files don't contribute to dir count");

    writer.shutdown();
}

#[test]
fn move_entry_v2_no_op_when_target_matches_current() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "home",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 1024,
            recursive_physical_size: 1024,
            recursive_file_count: 3,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    // Capture the per-writer mutation count before the no-op. Reading the
    // global `WRITER_GENERATION` here would flake under concurrent tests,
    // since `cargo test` runs tests as threads in one process and any other
    // writer that mutates between `before` and `after` would bump it.
    let gen_before = writer.mutation_count();

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 10,
            new_parent_id: ROOT_ID,
            new_name: "home".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 1024, "no-op preserves stats");
    assert_eq!(stats.recursive_file_count, 3);

    // The per-writer counter should not have moved (the no-op short-circuits
    // before `bump_generation`).
    let gen_after = writer.mutation_count();
    assert_eq!(
        gen_before, gen_after,
        "no-op should not bump the writer's mutation counter"
    );

    writer.shutdown();
}

#[test]
fn move_entry_v2_cross_parent_propagates_recursive_has_symlinks() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 1,
            recursive_has_symlinks: true,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    // The dir being moved carries the symlink flag in its own subtree.
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "D",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: true,
            min_subtree_epoch: 0,
        },
    );

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 11,
            new_name: "D".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert!(
        b_stats.recursive_has_symlinks,
        "new parent should pick up the symlink-bearing subtree"
    );

    writer.shutdown();
}

#[test]
fn move_entry_v2_bumps_writer_generation() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "Foo",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    let before = writer.mutation_count();
    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 10,
            new_parent_id: ROOT_ID,
            new_name: "Bar".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();
    let after = writer.mutation_count();
    assert!(
        after > before,
        "writer's mutation counter should bump after a real move"
    );

    writer.shutdown();
}
