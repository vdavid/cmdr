//! Small SQLite helpers shared by the writer threads (indexing, operation log).

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
