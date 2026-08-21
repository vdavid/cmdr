use std::cell::Cell;

use super::*;

// ── Shared page cache ────────────────────────────────────────────────

/// The slab has to be handed to SQLite BEFORE the library initializes itself,
/// which the first connection open does. Every factory in this module installs
/// it first, so by the time any store has a connection the answer is
/// `Installed` — `TooLate` means something opened a raw `rusqlite::Connection`
/// and permanently defeated the process-wide budget.
#[test]
fn the_shared_page_cache_is_installed_before_any_connection_opens() {
    let conn = open_in_memory().expect("open in-memory db");
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY);")
        .expect("create table");

    match ensure_shared_page_cache() {
        SharedPageCache::Installed { slot_bytes, slots } => {
            assert!(slot_bytes > DB_PAGE_BYTES, "a slot must hold a page plus its header");
            assert!(slots > 0, "the slab must have slots");
        }
        other => panic!("the shared page cache must be installed, got {other:?}"),
    }
}

/// Pin the resolved budget, so a future edit can't silently re-inflate the
/// process's SQLite memory. The slab is the WHOLE page-cache budget; the
/// per-role numbers are each connection's contribution to SQLite's global
/// ceiling, and they multiply by the connection count.
#[test]
fn the_page_cache_budgets_are_pinned() {
    assert_eq!(SHARED_PAGE_CACHE_BYTES, 64 * 1024 * 1024, "shared slab is 64 MiB");
    assert_eq!(WRITE_PAGE_CACHE_KIB, 16_384, "write connections contribute 16 MiB each");
    assert_eq!(READ_PAGE_CACHE_KIB, 128, "read connections contribute 128 KiB each");
    assert_eq!(READ_CONNECTION_BUDGET, 256, "the read budget covers 256 connections");

    let installed = ensure_shared_page_cache();
    let bytes = installed.bytes();
    assert!(
        bytes <= SHARED_PAGE_CACHE_BYTES,
        "the slab must never exceed its budget, got {bytes}"
    );
    assert!(
        SHARED_PAGE_CACHE_BYTES - bytes < 8 * 1024,
        "the slab must fill its budget bar the slot remainder, got {bytes}"
    );
}

/// Proof the slab is actually SERVING pages, not merely configured:
/// `SQLITE_STATUS_PAGECACHE_USED` counts only slots taken from the slab (heap
/// fallback lands in `PAGECACHE_OVERFLOW` instead), so a non-zero reading after
/// a real query means SQLite took our memory.
#[test]
fn cached_pages_come_from_the_shared_slab() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let conn = open(&dir.path().join("slab.db")).expect("open db");
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB);")
        .expect("create table");
    {
        let mut stmt = conn.prepare("INSERT INTO t (data) VALUES (?1)").expect("prepare");
        let blob = vec![0u8; 2_000];
        for _ in 0..200 {
            stmt.execute(rusqlite::params![blob]).expect("insert");
        }
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 200);

    assert!(
        query_page_cache_usage().used_bytes > 0,
        "cached pages must come out of the shared slab"
    );
}

/// The slab has to be readable at RUNTIME, not only from a test: it's memory
/// neither allocator `get_memory_diagnostics` reads can see, so without this
/// reading 64 MiB of the footprint has no owner in the one payload that answers
/// "what is Cmdr holding?".
#[test]
fn the_page_cache_reading_describes_the_slab_and_what_it_holds() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let conn = open(&dir.path().join("usage.db")).expect("open db");
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t VALUES (1);")
        .expect("write a page");

    let usage = query_page_cache_usage();
    assert_eq!(
        usage.slab_bytes,
        ensure_shared_page_cache().bytes() as u64,
        "the reading reports the slab actually installed"
    );
    assert!(usage.used_bytes > 0, "a written page is held in the slab");
    assert!(
        usage.used_bytes <= usage.slab_bytes,
        "what the slab holds can't exceed the slab: {usage:?}"
    );
    assert!(
        usage.peak_used_bytes >= usage.used_bytes,
        "a high-water mark is a mark, not a second current reading: {usage:?}"
    );
}

/// Reading the counters initializes SQLite, and SQLite refuses the slab once it
/// is initialized. So the reading has to install the slab on its way in, or a
/// diagnostic taken early enough would COST the process its page-cache budget.
/// Nextest gives each test its own process, which is what makes "first call
/// wins" testable at all.
#[test]
fn reading_the_page_cache_before_any_connection_opens_still_leaves_the_slab_installed() {
    let usage = query_page_cache_usage();
    assert!(usage.slab_bytes > 0, "the reading installed the slab itself: {usage:?}");

    let conn = open_in_memory().expect("open in-memory db");
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY);")
        .expect("create table");
    assert!(
        matches!(ensure_shared_page_cache(), SharedPageCache::Installed { .. }),
        "and a connection opened afterwards finds it there"
    );
}

/// The bound the whole page-cache design rests on.
///
/// SQLite enforces exactly ONE ceiling on retained pages — `pGroup->nMaxPage`,
/// which is the SUM of every open connection's `cache_size` — so the read
/// connections' share of it is `count × READ_PAGE_CACHE_KIB`, and `count` tracks
/// tokio's blocking-thread pool rather than anything semantic. Budgeted at
/// [`READ_CONNECTION_BUDGET`], that share plus the concurrently scanning writers
/// the slab is sized for has to fit INSIDE the slab. Otherwise the slab isn't a
/// cap, it's a treadmill: it runs permanently full, `pcache1`'s under-pressure
/// flag latches on, and nothing ever shrinks back at idle, because the branch
/// that frees a page outright only fires above `nMaxPage`.
///
/// Measured off REAL connections rather than read from the constant: the bound is
/// only real if the pragma actually lands.
#[test]
fn the_read_connection_budget_fits_inside_the_shared_slab() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("bound.db");
    {
        let conn = open(&db_path).expect("create db");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY);")
            .expect("create table");
    }

    let readers: Vec<Connection> = (0..8)
        .map(|_| {
            let conn = open_read_only(&db_path).expect("open read-only");
            apply_page_cache(&conn, true).expect("apply the read budget");
            conn
        })
        .collect();
    for conn in &readers {
        assert_eq!(
            page_cache_kib(conn),
            READ_PAGE_CACHE_KIB,
            "a read connection must open with the read budget"
        );
    }
    let per_reader_kib = page_cache_kib(&readers[0]);

    let writer = open(&db_path).expect("open read-write");
    apply_page_cache(&writer, false).expect("apply the write budget");
    let per_writer_kib = page_cache_kib(&writer);

    let ceiling_kib =
        READ_CONNECTION_BUDGET as i64 * per_reader_kib + CONCURRENTLY_SCANNING_WRITERS as i64 * per_writer_kib;
    let slab_kib = (SHARED_PAGE_CACHE_BYTES / 1024) as i64;
    assert!(
        ceiling_kib <= slab_kib,
        "SQLite's global page ceiling must fit inside the slab: read {READ_CONNECTION_BUDGET} x {per_reader_kib} KiB + scanning-write {CONCURRENTLY_SCANNING_WRITERS} x {per_writer_kib} KiB = {ceiling_kib} KiB against a {slab_kib} KiB slab"
    );
}

/// The budget is watched rather than trusted, so the watcher has to be right: a
/// cache's opens, its evictions, its generation retires, and its own death all
/// have to move the process-wide count.
///
/// Reads the counter as a DELTA. `nextest` forks a process per test, so nothing
/// else is moving it here.
#[test]
fn the_live_read_connection_count_follows_the_caches() {
    let before = live_read_connections();
    let opener = CountingOpener::new();
    {
        let mut cache = ThreadConnCache::new(2);
        let (a, b, c) = (Path::new("/a.db"), Path::new("/b.db"), Path::new("/c.db"));

        cache.with(a, 0, opener.open(), |_| ()).expect("a");
        cache.with(b, 0, opener.open(), |_| ()).expect("b");
        assert_eq!(live_read_connections() - before, 2, "each open counts once");

        cache.with(c, 0, opener.open(), |_| ()).expect("c");
        assert_eq!(
            live_read_connections() - before,
            2,
            "an eviction releases the connection it dropped"
        );

        cache.with(c, 1, opener.open(), |_| ()).expect("c at a new generation");
        assert_eq!(
            live_read_connections() - before,
            2,
            "a generation retire swaps one connection for one, not two"
        );
    }
    assert_eq!(
        live_read_connections(),
        before,
        "a dying thread takes its whole cache's connections with it"
    );
}

// ── Per-thread connection cache ──────────────────────────────────────

/// Count opens so reuse is asserted on something real; timing would only flake.
struct CountingOpener(Cell<usize>);

impl CountingOpener {
    fn new() -> Self {
        Self(Cell::new(0))
    }

    fn open(&self) -> impl FnOnce(&Path) -> Result<Connection, rusqlite::Error> + '_ {
        move |_| {
            self.0.set(self.0.get() + 1);
            open_in_memory()
        }
    }

    fn count(&self) -> usize {
        self.0.get()
    }
}

/// The thrash regression: a thread alternating between two volumes (left pane on
/// the boot disk, right pane on a NAS share) must keep BOTH connections, not
/// close and reopen on every alternation. Each reopen re-runs the pragmas and
/// the collation registration and discards the connection's whole
/// `prepare_cached` statement cache, which is the expensive part.
#[test]
fn alternating_between_two_dbs_reuses_both_connections() {
    let mut cache = ThreadConnCache::new(THREAD_CONN_SLOTS);
    let opener = CountingOpener::new();
    let left = Path::new("/tmp/cmdr-test/index-root.db");
    let right = Path::new("/tmp/cmdr-test/index-smb-naspi.db");

    for _ in 0..50 {
        cache.with(left, 0, opener.open(), |_| ()).expect("left");
        cache.with(right, 0, opener.open(), |_| ()).expect("right");
    }

    assert_eq!(
        opener.count(),
        2,
        "alternating between two dbs must open each exactly once"
    );
    assert_eq!(cache.len(), 2);
}

/// The cache is bounded: a third db evicts the least recently used, so a thread
/// can't accumulate a connection per volume it ever touched.
#[test]
fn a_full_cache_evicts_the_least_recently_used_db() {
    let mut cache = ThreadConnCache::new(2);
    let opener = CountingOpener::new();
    let (a, b, c) = (Path::new("/a.db"), Path::new("/b.db"), Path::new("/c.db"));

    cache.with(a, 0, opener.open(), |_| ()).expect("a");
    cache.with(b, 0, opener.open(), |_| ()).expect("b");
    cache.with(c, 0, opener.open(), |_| ()).expect("c");

    assert_eq!(cache.len(), 2, "the cache stays within its capacity");
    assert_eq!(cache.generation_for(a), None, "the least recently used was evicted");
    assert!(cache.generation_for(b).is_some());
    assert!(cache.generation_for(c).is_some());
    assert_eq!(opener.count(), 3);
}

/// Reuse is per db path, so touching a second db doesn't cost the first one its
/// connection the way the single-slot cache did.
#[test]
fn a_hit_moves_the_entry_to_the_front() {
    let mut cache = ThreadConnCache::new(2);
    let opener = CountingOpener::new();
    let (a, b) = (Path::new("/a.db"), Path::new("/b.db"));

    cache.with(a, 0, opener.open(), |_| ()).expect("a");
    cache.with(b, 0, opener.open(), |_| ()).expect("b");
    // Touch `a` so `b` becomes the eviction candidate.
    cache.with(a, 0, opener.open(), |_| ()).expect("a again");
    cache.with(Path::new("/c.db"), 0, opener.open(), |_| ()).expect("c");

    assert!(cache.generation_for(a).is_some(), "the recently used entry survives");
    assert_eq!(cache.generation_for(b), None, "the stale one is evicted");
}

/// Generation-based invalidation still retires the stale connection: a bumped
/// generation reopens, and the old entry must not linger beside the new one.
#[test]
fn a_bumped_generation_reopens_and_retires_the_stale_connection() {
    let mut cache = ThreadConnCache::new(THREAD_CONN_SLOTS);
    let opener = CountingOpener::new();
    let db = Path::new("/index-root.db");

    cache.with(db, 0, opener.open(), |_| ()).expect("generation 0");
    assert_eq!(cache.generation_for(db), Some(0));

    cache.with(db, 1, opener.open(), |_| ()).expect("generation 1");
    assert_eq!(
        cache.generation_for(db),
        Some(1),
        "the entry moved to the new generation"
    );
    assert_eq!(
        cache.len(),
        1,
        "the stale connection must not linger beside the new one"
    );
    assert_eq!(opener.count(), 2);
}

/// A failed open leaves the cache untouched, so the next call retries rather
/// than serving a half-populated slot.
#[test]
fn a_failed_open_leaves_the_cache_unchanged() {
    let mut cache = ThreadConnCache::new(THREAD_CONN_SLOTS);
    let result: Result<(), rusqlite::Error> = cache.with(
        Path::new("/nope.db"),
        0,
        |_| Err(rusqlite::Error::QueryReturnedNoRows),
        |_| (),
    );
    assert!(result.is_err());
    assert_eq!(cache.len(), 0);
}

// ── Freelist reclamation ─────────────────────────────────────────────

/// Build an `auto_vacuum = INCREMENTAL` DB with a freelist of at least
/// `min_free_pages`, then return an open connection to it. Inserts a blob
/// table, fills it, and deletes the rows so the pages land on the freelist.
fn db_with_freelist(min_free_pages: i64) -> Connection {
    let conn = open_in_memory().expect("open in-memory db");
    // auto_vacuum must be set before any table is created.
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL; PRAGMA page_size = 4096;")
        .expect("set auto_vacuum");
    conn.execute_batch("CREATE TABLE blobs (id INTEGER PRIMARY KEY, data BLOB);")
        .expect("create table");
    // One ~4 KiB row per page keeps the row count near the page count.
    let blob = vec![0u8; 4000];
    {
        let mut stmt = conn
            .prepare("INSERT INTO blobs (data) VALUES (?1)")
            .expect("prepare insert");
        for _ in 0..(min_free_pages + 50) {
            stmt.execute(rusqlite::params![blob]).expect("insert blob");
        }
    }
    conn.execute_batch("DELETE FROM blobs;").expect("delete rows");
    let free: i64 = conn
        .pragma_query_value(None, "freelist_count", |row| row.get(0))
        .expect("freelist_count");
    assert!(
        free >= min_free_pages,
        "test setup: wanted >= {min_free_pages} free pages, got {free}"
    );
    conn
}

fn freelist(conn: &Connection) -> i64 {
    conn.pragma_query_value(None, "freelist_count", |row| row.get(0))
        .expect("freelist_count")
}

#[test]
fn capped_vacuum_reclaims_exactly_the_cap() {
    let conn = db_with_freelist(50);
    let before = freelist(&conn);
    run_incremental_vacuum(&conn, Some(10)).expect("vacuum");
    let after = freelist(&conn);
    assert_eq!(
        before - after,
        10,
        "a capped vacuum must reclaim exactly the cap; before={before}, after={after}"
    );
}

#[test]
fn uncapped_vacuum_drains_the_whole_freelist() {
    let conn = db_with_freelist(50);
    assert!(freelist(&conn) > 0, "test setup: expected a non-empty freelist");
    run_incremental_vacuum(&conn, None).expect("vacuum");
    assert_eq!(freelist(&conn), 0, "an uncapped vacuum must drain the freelist to zero");
}

#[test]
fn cap_larger_than_freelist_drains_all_without_error() {
    let conn = db_with_freelist(50);
    let before = freelist(&conn);
    run_incremental_vacuum(&conn, Some(before + 1_000)).expect("vacuum");
    assert_eq!(freelist(&conn), 0, "a cap above the freelist size drains all pages");
}
