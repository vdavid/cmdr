//! Two `#[ignore]`d harnesses that MEASURE what the page-cache budgets cost, so the
//! numbers in `indexing/store/DETAILS.md` § "SQLite page memory is one process-wide
//! slab" are readings rather than arithmetic.
//!
//! ```text
//! cargo nextest run --release -p cmdr-fs --run-ignored all --no-capture page_cache_probe
//! ```
//!
//! [`page_cache_probe`] takes the READ term: real full-table scans through many read
//! connections at once, the shape an idle app reaches after hours once enrichment has
//! touched every blocking thread. [`writer_page_cache_probe`] takes the WRITE term,
//! which is the one the sizing calls a design target rather than a bound — a
//! production process holds far more write connections than the slab is sized for, and
//! a probe that only opens readers structurally cannot see that.
//!
//! ❗ Run them in SEPARATE processes (nextest gives each test its own by default, which
//! is why they live here rather than in one function). The page-cache counters are
//! process-wide, so a reader probe that ran first leaves its pages in the slab and
//! flatters the writer reading.
//!
//! Both read SQLite's own accounting back:
//!
//! - `SQLITE_STATUS_PAGECACHE_USED` counts slots taken from OUR slab. Read against the
//!   slab's size, this is the headline: a slab sitting at ~100% is one that ran out of
//!   room and is holding every page it can rather than every page it wants, permanently.
//! - `SQLITE_STATUS_PAGECACHE_OVERFLOW` counts bytes SQLite had to take from the HEAP
//!   because the slab couldn't serve them. Measured at zero on both sides of the budget
//!   change: an oversized `Σ cache_size` doesn't leak page cache onto the heap, it just
//!   keeps the slab pegged. Worth watching anyway, since that is where it would show.
//!
//! ❌ Not regression gates, and deliberately not part of any lane. They assert nothing
//! about the numbers they print — the arithmetic is pinned by
//! `tests::the_read_connection_budget_fits_inside_the_shared_slab` — and between them
//! they open 141 connections and write ~160 MB, which no lane should pay for.

use super::*;

/// Read connections to drive at once: the count a profiled prod session held
/// (v0.36.2, ~10 h uptime, macOS 26.5.2, `lsof`, 2026-07-28 — 156 across 69
/// blocking threads, of which 132 on the two local index databases), so the
/// reading lines up with the shape that motivated the budget.
const CONNECTIONS: usize = 132;

/// Database pages to build. Comfortably more than one connection's budget, so each
/// scan really wants more cache than it is allowed and the caches compete.
const DB_PAGES: usize = 4_000;

/// Fill `conn`'s database with `pages` rows of roughly one page each, in one
/// transaction. One ~4 KiB row per page keeps the row count near the page count.
fn fill(conn: &Connection, pages: usize) {
    conn.execute_batch("BEGIN;").expect("begin");
    {
        let mut stmt = conn.prepare("INSERT INTO t (data) VALUES (?1)").expect("prepare");
        let blob = vec![7u8; 4_000];
        for _ in 0..pages {
            stmt.execute(rusqlite::params![blob]).expect("insert");
        }
    }
    conn.execute_batch("COMMIT;").expect("commit");
}

/// A fresh WAL database at `db_path` with the one table these probes write.
fn create_db(db_path: &Path) -> Connection {
    let conn = open(db_path).expect("create db");
    conn.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB);")
        .expect("create table");
    conn
}

/// The slab reading as a `(held MiB, overflow bytes, percent of slab held)` triple.
fn slab_reading() -> (u64, u64, u64) {
    let usage = query_page_cache_usage();
    assert!(usage.slab_bytes > 0, "the slab must be installed, got {usage:?}");
    let held_mib = usage.used_bytes / (1024 * 1024);
    (
        held_mib,
        usage.overflow_bytes,
        usage.used_bytes * 100 / usage.slab_bytes,
    )
}

#[test]
#[ignore = "measurement harness: it reports numbers, it doesn't assert them"]
#[allow(clippy::print_stdout, reason = "a measurement harness prints its measurements")]
fn page_cache_probe() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("probe.db");
    fill(&create_db(&db_path), DB_PAGES);

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
    let (from_slab_mib, from_heap_bytes, in_use_pct) = slab_reading();

    println!("── page-cache probe: the READ term ────────────────");
    println!("  read connections        {CONNECTIONS}");
    println!("  per-connection budget   {per_reader_kib} KiB");
    println!("  their global ceiling    {ceiling_mib} MiB  (their share of `pGroup->nMaxPage`)");
    println!("  slab                    {slab_mib} MiB");
    println!("  pages served from slab  {from_slab_mib} MiB");
    println!(
        "  pages served from HEAP  {} MiB  ({from_heap_bytes} B) ← what the slab does not describe",
        from_heap_bytes / (1024 * 1024)
    );
    println!("  slab in use             {in_use_pct}%");
}

/// Write connections a modest two-volume session durably holds, which is the term
/// [`CONCURRENTLY_SCANNING_WRITERS`] is a design TARGET for rather than a bound on:
/// `IndexStore` plus `IndexWriter` on each volume's `index.db` (two apiece),
/// `ImportanceWriter` on each `importance.db`, `MediaWriter` on each `media.db`, and
/// the process-wide operation-log writer. Every one of them holds its connection —
/// and so its full [`WRITE_PAGE_CACHE_KIB`] share of `pGroup->nMaxPage` — for as long
/// as it lives, scanning or not, and the two registries never drain.
const WRITERS: usize = 9;

/// Pages each writer dirties: one whole [`WRITE_PAGE_CACHE_KIB`] budget, so every
/// writer really does claim the share it is allowed rather than a token slice of it.
const WRITER_PAGES: usize = (WRITE_PAGE_CACHE_KIB as usize * 1024) / DB_PAGE_BYTES;

/// What the WRITE term costs, which [`page_cache_probe`] structurally cannot see
/// because it opens readers only.
///
/// It reads the same population twice: at [`CONCURRENTLY_SCANNING_WRITERS`], the
/// design target, and at [`WRITERS`], what production actually holds. Each writer
/// runs one burst and then goes quiet, which is the honest steady state — a volume
/// finishes its scan and its writers sit idle — so the reading afterwards is what
/// IDLE writers still pin. The mechanism is `pcache1Unpin`, which frees an unpinned
/// page outright only while `nPurgeable > nMaxPage`: the more writers exist, the
/// higher `nMaxPage` climbs, and the more each idle one gets to keep.
#[test]
#[ignore = "measurement harness: it reports numbers, it doesn't assert them"]
#[allow(clippy::print_stdout, reason = "a measurement harness prints its measurements")]
fn writer_page_cache_probe() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let slab_mib = SHARED_PAGE_CACHE_BYTES / (1024 * 1024);

    println!("── page-cache probe: the WRITE term ───────────────");
    println!("  slab                    {slab_mib} MiB");
    println!("  per-connection budget   {WRITE_PAGE_CACHE_KIB} KiB");

    for (writers, label) in [
        (CONCURRENTLY_SCANNING_WRITERS, "the design target"),
        (WRITERS, "what a two-volume session HOLDS"),
    ] {
        // Held together, exactly as the registries hold theirs, and dropped at the
        // end of the iteration so the next reading starts from an emptied slab.
        let conns: Vec<Connection> = (0..writers)
            .map(|i| {
                let conn = create_db(&dir.path().join(format!("writer-{writers}-{i}.db")));
                apply_page_cache(&conn, false).expect("apply the write budget");
                conn
            })
            .collect();
        for conn in &conns {
            fill(conn, WRITER_PAGES);
        }

        let ceiling_mib = (writers as i64 * page_cache_kib(&conns[0])) / 1024;
        let (from_slab_mib, from_heap_bytes, in_use_pct) = slab_reading();
        println!("\n  {writers} write connections: {label}");
        println!("    their global ceiling  {ceiling_mib} MiB  (their share of `pGroup->nMaxPage`)");
        println!("    slab held while IDLE  {from_slab_mib} MiB  ({in_use_pct}% of the slab)");
        println!(
            "    served from the HEAP  {} MiB  ({from_heap_bytes} B)",
            from_heap_bytes / (1024 * 1024)
        );
    }
}
