//! An `#[ignore]`d harness that MEASURES what the page-cache budgets cost, so the
//! numbers in `indexing/store/DETAILS.md` § "SQLite page memory is one process-wide
//! slab" are readings rather than arithmetic.
//!
//! ```text
//! cargo nextest run --release -p cmdr-fs --run-ignored all --no-capture page_cache_probe
//! ```
//!
//! It drives real full-table scans through many read connections at once — the shape
//! an idle app reaches after hours, when enrichment has touched every blocking thread
//! — and reads SQLite's own accounting back:
//!
//! - `SQLITE_STATUS_PAGECACHE_USED` counts slots taken from OUR slab. Read against the
//!   slab's size, this is the headline: a slab sitting at ~100% is one that ran out of
//!   room and is holding every page it can rather than every page it wants, permanently.
//! - `SQLITE_STATUS_PAGECACHE_OVERFLOW` counts bytes SQLite had to take from the HEAP
//!   because the slab couldn't serve them. Measured at zero on both sides of the budget
//!   change: an oversized `Σ cache_size` doesn't leak page cache onto the heap, it just
//!   keeps the slab pegged. Worth watching anyway, since that is where it would show.
//!
//! ❌ Not a regression gate, and deliberately not part of any lane. It asserts nothing
//! about the numbers it prints — the bound itself is pinned by
//! `tests::the_read_connection_budget_fits_inside_the_shared_slab` — and it opens 132
//! connections and builds a 16 MB database to get them, which no lane should pay for.

use super::*;

/// Read connections to drive at once: the count a profiled prod session held
/// (v0.36.2, ~10 h uptime, macOS 26.5.2, `lsof`, 2026-07-28 — 156 across 69
/// blocking threads, of which 132 on the two local index databases), so the
/// reading lines up with the shape that motivated the budget.
const CONNECTIONS: usize = 132;

/// Database pages to build. Comfortably more than one connection's budget, so each
/// scan really wants more cache than it is allowed and the caches compete.
const DB_PAGES: usize = 4_000;

#[test]
#[ignore = "measurement harness: it reports numbers, it doesn't assert them"]
#[allow(clippy::print_stdout, reason = "a measurement harness prints its measurements")]
fn page_cache_probe() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("probe.db");
    {
        let conn = open(&db_path).expect("create db");
        conn.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB);")
            .expect("create table");
        conn.execute_batch("BEGIN;").expect("begin");
        {
            let mut stmt = conn.prepare("INSERT INTO t (data) VALUES (?1)").expect("prepare");
            // One ~4 KiB row per page keeps the row count near the page count.
            let blob = vec![7u8; 4_000];
            for _ in 0..DB_PAGES {
                stmt.execute(rusqlite::params![blob]).expect("insert");
            }
        }
        conn.execute_batch("COMMIT;").expect("commit");
    }

    let readers: Vec<Connection> = (0..CONNECTIONS)
        .map(|_| {
            let conn = open_read_only(&db_path).expect("open read-only");
            apply_page_cache(&conn, true).expect("apply the read budget");
            conn
        })
        .collect();

    // Two passes: the first fills every cache, the second makes them compete for
    // pages they all still want.
    for _ in 0..2 {
        for conn in &readers {
            let seen: i64 = conn
                .query_row("SELECT count(*) FROM t WHERE data IS NOT NULL", [], |row| row.get(0))
                .expect("full scan");
            assert_eq!(seen, DB_PAGES as i64);
        }
    }

    let per_reader_kib = page_cache_kib(&readers[0]);
    let slab_mib = SHARED_PAGE_CACHE_BYTES / (1024 * 1024);
    let ceiling_mib = (CONNECTIONS as i64 * per_reader_kib) / 1024;
    let usage = query_page_cache_usage();
    assert!(usage.slab_bytes > 0, "the slab must be installed, got {usage:?}");
    let from_slab_mib = usage.used_bytes / (1024 * 1024);
    let from_heap_bytes = usage.overflow_bytes;

    println!("── page-cache probe ───────────────────────────────");
    println!("  read connections        {CONNECTIONS}");
    println!("  per-connection budget   {per_reader_kib} KiB");
    println!("  their global ceiling    {ceiling_mib} MiB  (SQLite's `pGroup->nMaxPage`)");
    println!("  slab                    {slab_mib} MiB");
    println!("  pages served from slab  {from_slab_mib} MiB");
    println!(
        "  pages served from HEAP  {} MiB  ({from_heap_bytes} B) ← what the slab does not describe",
        from_heap_bytes / (1024 * 1024)
    );
    println!("  slab in use             {}%", from_slab_mib * 100 / slab_mib as u64);
}
