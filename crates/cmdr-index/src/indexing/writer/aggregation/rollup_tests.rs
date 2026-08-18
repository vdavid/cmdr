//! Tests for the per-burst ancestor roll-up ([`super::super::pending_rollups`]).
//!
//! A walk stopped after a wide directory was listed leaves every unwalked child
//! of it a frontier root of its own, and each root's walk ends in one
//! `ComputeSubtreeAggregates`. Rolling the shared parent up per message made that
//! `O(width²)` and took a 60,000-child directory to 73 minutes
//! (`docs/notes/wide-dir-scaling-2026-08-18.md`). These pin the fix: the burst
//! costs ONE roll-up, the answer it lands is the same one the per-message version
//! landed, and a quit inside a burst still pays what the burst owes.

use super::tests::{dir_row, file_row, set_epoch, settle};
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::stress_test_helpers::check_db_consistency;
use crate::indexing::writer::tests::setup_db;
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

/// One parent holding `width` child dirs, each with one file of `size` bytes.
/// The shape a stopped walk of a wide directory leaves behind.
fn wide_parent(writer: &IndexWriter, width: i64, size: u64) {
    let mut entries = vec![dir_row(10, ROOT_ID, "big")];
    let mut listed = vec![ROOT_ID, 10];
    for index in 0..width {
        let dir_id = 100 + index * 2;
        entries.push(dir_row(dir_id, 10, &format!("sub-{index}")));
        entries.push(file_row(dir_id + 1, dir_id, "leaf", size));
        listed.push(dir_id);
    }
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::MarkDirsListed { ids: listed, epoch: 1 })
        .unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();
}

/// **The scaling guard.** `width` frontier roots sharing one parent must cost a
/// HANDFUL of ancestor walks, not one each — that ratio IS the difference between
/// linear and quadratic, and it's what a well-meaning "repair it right here, it's
/// race-free that way" would put back.
///
/// Reads `rollup_walks`, the writer's own count of drained roll-ups, so the
/// assertion is about the mechanism rather than about a stopwatch. Pre-fix this
/// number was exactly `width`.
#[test]
fn a_burst_of_roots_under_one_parent_costs_a_handful_of_rollups() {
    const WIDTH: i64 = 400;

    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();
    wide_parent(&writer, WIDTH, 1000);
    // Blank the ancestors' coverage the way a live new-dir upsert does, so the
    // assertions below can only pass if a roll-up really ran: without this the
    // rows are already right and a skipped drain would read as a pass.
    set_epoch(&db_path, 10, 0);
    set_epoch(&db_path, ROOT_ID, 0);

    // Every child's walk finishes and reports, back to back, the way the phase
    // machine's drain does — nothing flushes in between, so the writer stays
    // behind and the burst is one burst.
    for index in 0..WIDTH {
        writer
            .send(WriteMessage::ComputeSubtreeAggregates {
                root_id: 100 + index * 2,
            })
            .unwrap();
    }
    settle(&writer);

    let walks = writer.rollup_walks();
    assert!(
        walks <= (WIDTH as u64) / 10,
        "{WIDTH} roots sharing one parent cost {walks} ancestor roll-ups; the burst must coalesce, \
         or a wide directory is quadratic again"
    );

    // And the coalescing changed no answer.
    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let big = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (
            big.recursive_logical_size,
            big.recursive_file_count,
            big.recursive_dir_count
        ),
        (WIDTH as u64 * 1000, WIDTH as u64, WIDTH as u64),
    );
    assert_eq!(big.min_subtree_epoch, 1, "the one roll-up restored the coverage");
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(root.min_subtree_epoch, 1, "and carried it the rest of the way up");
    check_db_consistency(&conn);

    writer.shutdown();
}

/// A live mutation landing INSIDE the window where the roll-up is still owed must
/// not be double-counted or lost. The roll-up recomputes from committed children
/// rather than adding a delta, so the later it runs the more of the truth it sees
/// — this is the whole race argument, asserted.
#[test]
fn a_mutation_inside_the_pending_window_is_counted_once() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();
    wide_parent(&writer, 4, 1000);

    // Two roots report, then somebody's editor writes a file straight into the
    // wide directory, then the rest report. The roll-up is owed throughout.
    for index in 0..2 {
        writer
            .send(WriteMessage::ComputeSubtreeAggregates {
                root_id: 100 + index * 2,
            })
            .unwrap();
    }
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 10,
            name: "typed-just-now.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(7),
            physical_size: Some(7),
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    for index in 2..4 {
        writer
            .send(WriteMessage::ComputeSubtreeAggregates {
                root_id: 100 + index * 2,
            })
            .unwrap();
    }
    settle(&writer);

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let big = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        (
            big.recursive_logical_size,
            big.recursive_file_count,
            big.recursive_dir_count
        ),
        (4 * 1000 + 7, 5, 4),
        "the new file counts exactly once, whichever side of the roll-up it landed on"
    );
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(root.recursive_logical_size, 4 * 1000 + 7);
    check_db_consistency(&conn);

    writer.shutdown();
}

/// A quit landing inside a burst still pays what the burst owes. `shutdown` runs
/// the same settle the caught-up point does, so the ancestors are right on the
/// next launch instead of waiting for the run to complete.
#[test]
fn a_shutdown_inside_a_burst_still_rolls_the_ancestors_up() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();
    wide_parent(&writer, 6, 1000);

    // Blank the ancestors the way a live new-dir upsert does, so only a roll-up
    // can put them back.
    set_epoch(&db_path, 10, 0);
    set_epoch(&db_path, ROOT_ID, 0);

    for index in 0..6 {
        writer
            .send(WriteMessage::ComputeSubtreeAggregates {
                root_id: 100 + index * 2,
            })
            .unwrap();
    }
    // ❌ No settle: the quit is what has to pay.
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let big = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(big.min_subtree_epoch, 1, "the quit rolled the coverage back up");
    let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(root.min_subtree_epoch, 1);
    check_db_consistency(&conn);
}
