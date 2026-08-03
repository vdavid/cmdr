//! A/B probe for the single-row insert path's cost, run by hand rather than in CI.
//!
//! `insert_entry_v2_with_id` is the LIVE reconcile write path (the scan path batches
//! through `insert_entries_v2_batch` instead), so its per-row cost is paid once per
//! file the watcher sees. This times a run of it so a change to that path can be
//! measured instead of argued about.
//!
//! ❌ Not a CI test: it asserts nothing about wall clock, because a timing assertion
//! on a shared machine is a flake generator. It prints, and `--ignored` keeps it out
//! of the default run.
//!
//! ```sh
//! cargo test -p cmdr-index --lib insert_throughput_probe -- --ignored --nocapture
//! ```

#![allow(
    clippy::print_stderr,
    reason = "a manual measurement probe reports its number to the operator running it; \
              the same allow the repo's examples take"
)]

use super::*;

/// How many rows the probe inserts. Big enough that per-row cost dominates the
/// fixture setup, small enough to run in a few seconds either way.
const ROWS: i64 = 20_000;

#[test]
#[ignore = "manual measurement probe, not a CI assertion"]
fn insert_entry_v2_with_id_throughput() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // One parent, so the run measures the insert and not directory-tree growth.
    let parent = insert_entry(&conn, ROOT_ID, "parent", true, None);

    let names: Vec<String> = (0..ROWS).map(|i| format!("file_{i:06}.txt")).collect();

    let started = std::time::Instant::now();
    for (i, name) in names.iter().enumerate() {
        IndexStore::insert_entry_v2_with_id(
            &conn,
            1_000_000 + i as i64,
            parent,
            name,
            false,
            false,
            Some(4096),
            Some(4096),
            Some(1_700_000_000),
            None,
        )
        .unwrap();
    }
    let elapsed = started.elapsed();

    let per_row = elapsed / ROWS as u32;
    eprintln!("insert_entry_v2_with_id: {ROWS} rows in {elapsed:?} ({per_row:?} per row)");

    // The only real assertion: the rows landed. The number above is the point.
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE parent_id = ?1", [parent], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, ROWS, "every probe row should have been inserted");
}

/// The same rows through the same call, but inside ONE transaction instead of
/// `ROWS` autocommit ones.
///
/// Separates the two costs the profile conflated: statement preparation (removed by
/// `prepare_cached`) and the per-row COMMIT plus WAL frame write that autocommit
/// forces. Run both probes together; the gap between them is what batching is worth.
#[test]
#[ignore = "manual measurement probe, not a CI assertion"]
fn insert_entry_v2_with_id_throughput_in_one_transaction() {
    let (store, _dir) = open_temp_store();
    let mut conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    let parent = insert_entry(&conn, ROOT_ID, "parent", true, None);
    let names: Vec<String> = (0..ROWS).map(|i| format!("file_{i:06}.txt")).collect();

    let started = std::time::Instant::now();
    let tx = conn.transaction().unwrap();
    for (i, name) in names.iter().enumerate() {
        IndexStore::insert_entry_v2_with_id(
            &tx,
            1_000_000 + i as i64,
            parent,
            name,
            false,
            false,
            Some(4096),
            Some(4096),
            Some(1_700_000_000),
            None,
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let elapsed = started.elapsed();

    let per_row = elapsed / ROWS as u32;
    eprintln!("insert_entry_v2_with_id (ONE transaction): {ROWS} rows in {elapsed:?} ({per_row:?} per row)");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE parent_id = ?1", [parent], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, ROWS, "every probe row should have been inserted");
}
