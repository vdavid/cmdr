//! A/B probe for the LIVE writer path's cost, run by hand rather than in CI.
//!
//! `store/tests/insert_throughput_probe.rs` times the store call directly, which
//! doesn't exercise `writer_loop` at all. This one pushes `UpsertEntryV2` messages
//! through a real `IndexWriter` — channel, loop, implicit batching, delta propagation
//! and all — so the number is what a watcher-driven burst actually pays per file.
//!
//! ❌ Not a CI test: it asserts nothing about wall clock, because a timing assertion on
//! a shared machine is a flake generator. It prints, and `--ignored` keeps it out of
//! the default run. The CI-safe proof that batching happens is
//! `a_queued_run_of_live_mutations_commits_once_instead_of_once_per_message`, which
//! counts SQLite's own commits.
//!
//! ```sh
//! cargo test -p cmdr-index --lib writer_upsert_throughput -- --ignored --nocapture
//! ```

#![allow(
    clippy::print_stderr,
    reason = "a manual measurement probe reports its number to the operator running it; \
              the same allow the store's insert probe takes"
)]

use std::time::Instant;

use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::writer::tests::setup_db;
use crate::indexing::writer::{IndexWriter, WriteMessage};

/// How many live upserts the probe sends. Comfortably past the writer channel's 20 K
/// capacity is NOT the goal — this is a sustained burst that keeps the queue non-empty,
/// which is exactly the case batching is for.
const ROWS: usize = 20_000;

/// The id given to the parent directory every probe row lands in.
const PARENT_ID: i64 = 10;

#[test]
#[ignore = "manual measurement probe, not a CI assertion"]
fn writer_upsert_throughput() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

    // One parent, so the run measures the live upsert path and not tree growth.
    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id: PARENT_ID,
            parent_id: ROOT_ID,
            name: "parent".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }]))
        .expect("insert the parent");
    writer.flush_blocking().expect("flush the parent");

    let names: Vec<String> = (0..ROWS).map(|i| format!("file_{i:06}.txt")).collect();

    let started = Instant::now();
    for name in &names {
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: PARENT_ID,
                name: name.clone(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(4096),
                physical_size: Some(4096),
                modified_at: Some(1_700_000_000),
                inode: None,
                nlink: None,
            })
            .expect("send upsert");
    }
    // The flush is part of the measurement: it's what makes the last batch durable.
    writer.flush_blocking().expect("flush the run");
    let elapsed = started.elapsed();

    let per_row = elapsed / ROWS as u32;
    // allowed-pluralize-noun: ROWS is a 20,000 constant, never 1
    eprintln!("IndexWriter UpsertEntryV2: {ROWS} rows in {elapsed:?} ({per_row:?} per row)");

    // The only real assertion: the rows landed. The number above is the point.
    let conn = IndexStore::open_write_connection(&db_path).expect("read-back conn");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE parent_id = ?1", [PARENT_ID], |r| {
            r.get(0)
        })
        .expect("count the probe rows");
    assert_eq!(count, ROWS as i64, "every probe row should have been upserted");

    writer.shutdown();
}
