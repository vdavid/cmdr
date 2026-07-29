use super::*;

// ── Shared page cache ────────────────────────────────────────────────

/// Read one `sqlite3_status64` counter's current value.
fn sqlite_status(op: c_int) -> i64 {
    let mut current: i64 = 0;
    let mut highwater: i64 = 0;
    // SAFETY: `sqlite3_status64` writes two `sqlite3_int64` out-parameters,
    // both live `i64`s here, and takes no ownership. It's safe to call from any
    // thread once SQLite is initialized, which opening a connection guarantees.
    let rc = unsafe { ffi::sqlite3_status64(op, &raw mut current, &raw mut highwater, 0) };
    assert_eq!(rc, ffi::SQLITE_OK, "sqlite3_status64({op}) failed with {rc}");
    current
}

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
/// per-role numbers are upper bounds per connection out of it.
#[test]
fn the_page_cache_budgets_are_pinned() {
    assert_eq!(SHARED_PAGE_CACHE_BYTES, 64 * 1024 * 1024, "shared slab is 64 MiB");
    assert_eq!(WRITE_PAGE_CACHE_KIB, 16_384, "write connections cap at 16 MiB");
    assert_eq!(READ_PAGE_CACHE_KIB, 8_192, "read connections cap at 8 MiB");

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
        sqlite_status(ffi::SQLITE_STATUS_PAGECACHE_USED) > 0,
        "cached pages must come out of the shared slab"
    );
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
