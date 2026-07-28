//! Small SQLite helpers shared by every store: the page-cache budget applied to
//! all connections, and freelist reclamation for the writer threads.

// ── Page cache budget ────────────────────────────────────────────────

/// Page cache for a WRITE connection, in KiB (16 MiB), applied as the negative
/// `PRAGMA cache_size` form.
///
/// Coupled to `wal_autocheckpoint = 4000` (~16 MiB of 4 KiB pages, set in
/// `indexing::store::apply_pragmas`): the cache is sized to hold the pages a
/// whole autocheckpoint window dirties, so a big write batch commits without
/// evicting pages it's about to touch again. Change one, reconsider the other.
///
/// There is at most ONE write connection per DB (a single writer thread), so this
/// budget is paid a handful of times process-wide.
pub const WRITE_PAGE_CACHE_KIB: i64 = 16_384;

/// Page cache for a READ-ONLY connection, in KiB (2 MiB), applied as the negative
/// `PRAGMA cache_size` form. Deliberately 8x smaller than
/// [`WRITE_PAGE_CACHE_KIB`], because read connections are MANY and long-lived.
///
/// Read connections are thread-local and live as long as their thread
/// (`indexing::read::enrichment`'s `THREAD_CONN`, `ImportanceIndex`'s
/// `READ_CONN`), so the count tracks tokio's blocking-thread pool rather than
/// anything semantic: 156 were open in a profiled prod session (2026-07-28,
/// v0.36.2, ~10 h uptime, `lsof` + `footprint -s`), holding ~1.15 GB against a
/// 2.5 GB ceiling. At this budget the same 156 connections cap at ~310 MB.
///
/// 2 MiB is SQLite's own default and comfortably holds the upper interior levels
/// of the hot b-trees, so the enrichment path's point lookups and per-directory
/// range scans still resolve without walking to disk; whole-table working sets
/// were never going to fit at 16 MiB either, and the OS file cache backs those.
/// Reads never commit or checkpoint, so nothing here interacts with the WAL.
pub const READ_PAGE_CACHE_KIB: i64 = 2_048;

const _: () = assert!(
    READ_PAGE_CACHE_KIB < WRITE_PAGE_CACHE_KIB,
    "read connections must stay cheaper than the single write connection"
);

/// Apply the page-cache budget for this connection's role: [`READ_PAGE_CACHE_KIB`]
/// when `readonly`, [`WRITE_PAGE_CACHE_KIB`] otherwise.
///
/// Every store's `apply_pragmas` calls this, so the split is set in ONE place and
/// a new store can't quietly hand read connections the writer's budget.
pub fn apply_page_cache(conn: &rusqlite::Connection, readonly: bool) -> rusqlite::Result<()> {
    let kib = if readonly {
        READ_PAGE_CACHE_KIB
    } else {
        WRITE_PAGE_CACHE_KIB
    };
    // Negative = KiB of memory; positive would mean a page COUNT, which varies
    // with `page_size` and is not what any of these budgets mean.
    conn.execute_batch(&format!("PRAGMA cache_size = -{kib};"))
}

/// The connection's page-cache budget in KiB. `PRAGMA cache_size` echoes back the
/// negative KiB form [`apply_page_cache`] sets, so flip the sign.
#[cfg(test)]
pub fn page_cache_kib(conn: &rusqlite::Connection) -> i64 {
    let raw: i64 = conn
        .pragma_query_value(None, "cache_size", |row| row.get(0))
        .expect("read cache_size");
    assert!(raw < 0, "cache_size should be set in negative-KiB form, got {raw}");
    -raw
}

// ── Freelist reclamation ─────────────────────────────────────────────

/// Reclaim freed pages via `PRAGMA incremental_vacuum`, stepping until the pragma
/// is exhausted.
///
/// SQLite compiles `incremental_vacuum` into a loop that frees ONE page per
/// `sqlite3_step()`, yielding a result row after each page. `execute_batch` steps
/// a statement exactly once, so it frees a single page no matter the cap — which
/// strands the freelist, draining it one page per tick. Prepare the pragma and
/// step it to completion instead.
///
/// `cap` bounds how many pages to reclaim; `None` drains the whole freelist.
pub fn run_incremental_vacuum(conn: &rusqlite::Connection, cap: Option<i64>) -> rusqlite::Result<()> {
    let sql = match cap {
        Some(n) => format!("PRAGMA incremental_vacuum({n});"),
        None => "PRAGMA incremental_vacuum;".to_string(),
    };
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while rows.next()?.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an `auto_vacuum = INCREMENTAL` DB with a freelist of at least
    /// `min_free_pages`, then return an open connection to it. Inserts a blob
    /// table, fills it, and deletes the rows so the pages land on the freelist.
    fn db_with_freelist(min_free_pages: i64) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
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

    fn freelist(conn: &rusqlite::Connection) -> i64 {
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
}
