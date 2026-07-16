//! Search index: in-memory entry storage and the arena loader.
//!
//! One `SearchIndex` per volume. The per-volume registry, lifecycle timers, and
//! importance weights live in [`super::volumes`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::indexing::ReadPool;
use crate::indexing::writer::WRITER_GENERATION;
use crate::pluralize::pluralize_with;

// ── Search entry (in-memory representation) ──────────────────────────

#[derive(Debug)]
pub struct SearchEntry {
    pub id: i64,
    pub parent_id: i64,
    pub name_offset: u32, // byte offset into SearchIndex.names
    pub name_len: u16,    // byte length (max filename 255 chars = up to 765 bytes UTF-8)
    pub is_directory: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
}

// ── Search index ─────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SearchIndex {
    pub names: String, // arena: all filenames concatenated
    pub entries: Vec<SearchEntry>,
    pub id_to_index: HashMap<i64, usize>,
    pub generation: u64,
}

impl SearchIndex {
    /// Empty sentinel index used during async load.
    pub fn empty() -> Self {
        Self {
            names: String::new(),
            entries: Vec::new(),
            id_to_index: HashMap::new(),
            generation: 0,
        }
    }

    /// Get the filename for an entry from the arena buffer.
    pub(crate) fn name(&self, entry: &SearchEntry) -> &str {
        &self.names[entry.name_offset as usize..entry.name_offset as usize + entry.name_len as usize]
    }
}

/// Rows between cancellation checks during load.
const CANCEL_CHECK_INTERVAL: usize = 100_000;

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Index loading ────────────────────────────────────────────────────

/// Load all entries from the index DB into an in-memory `SearchIndex`.
///
/// `name_folded` is NOT loaded: the search pattern is normalized instead
/// (NFD on macOS) to avoid ~5.1M extra String allocations and ~300 MB of memory.
pub(crate) fn load_search_index(pool: &ReadPool, cancel: &AtomicBool) -> Result<SearchIndex, String> {
    pool.with_conn(|conn: &rusqlite::Connection| {
        let t = std::time::Instant::now();
        let generation = WRITER_GENERATION.load(Ordering::Relaxed);

        let sql = "SELECT id, parent_id, name, is_directory, logical_size, modified_at FROM entries";

        let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare failed: {e}"))?;

        // Phase 1: Load all entries into Vec (sequential writes to contiguous memory)
        // Arena-allocate all filenames into a single String to avoid per-entry heap allocations.
        // Right-size both from the actual row count: a small index used to pay a fixed
        // ~100 MB / 5M-slot worst-case allocation on every load. `COUNT(*)` is a cheap
        // b-tree count, run once. The name arena estimate is clamped so a bogus count
        // can't request gigabytes; the Vec/String still grow if the estimate is low.
        const AVG_NAME_BYTES: usize = 20;
        const NAMES_ARENA_CEILING: usize = 512 * 1024 * 1024; // 512 MiB
        let row_count_estimate: usize = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get::<_, i64>(0))
            .map(|n| n.max(0) as usize)
            .unwrap_or(0);
        let names_capacity = row_count_estimate
            .saturating_mul(AVG_NAME_BYTES)
            .min(NAMES_ARENA_CEILING);
        let mut names = String::with_capacity(names_capacity);
        let mut entries = Vec::with_capacity(row_count_estimate);

        let mut rows = stmt.query([]).map_err(|e| format!("Query failed: {e}"))?;
        let mut row_count = 0usize;

        while let Some(row) = rows.next().map_err(|e| format!("Row read failed: {e}"))? {
            if row_count.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err("Load cancelled".to_string());
            }

            let id: i64 = row.get(0).map_err(|e| format!("{e}"))?;
            let parent_id: i64 = row.get(1).map_err(|e| format!("{e}"))?;
            // Borrow directly from SQLite's internal buffer via ValueRef: zero heap allocations.
            let name_ref = row.get_ref(2).map_err(|e| format!("{e}"))?;
            let name_str = name_ref.as_str().map_err(|e| format!("{e}"))?;
            let name_offset = names.len() as u32;
            let name_len = name_str.len() as u16;
            names.push_str(name_str);
            let is_directory: bool = row.get(3).map_err(|e| format!("{e}"))?;
            let logical_size: Option<u64> = row.get(4).map_err(|e| format!("{e}"))?;
            let modified_at: Option<u64> = row.get(5).map_err(|e| format!("{e}"))?;
            entries.push(SearchEntry {
                id,
                parent_id,
                name_offset,
                name_len,
                is_directory,
                size: logical_size,
                modified_at,
            });
            row_count += 1;
        }

        // Phase 2: Build id_to_index from completed Vec (sequential reads + HashMap writes)
        let mut id_to_index = HashMap::with_capacity(entries.len());
        for (i, entry) in entries.iter().enumerate() {
            id_to_index.insert(entry.id, i);
        }

        log::debug!(
            "Search index loaded: {}, generation {generation}, took {:?}",
            pluralize_with(row_count as u64, "entry", "entries"),
            t.elapsed()
        );
        Ok(SearchIndex {
            names,
            entries,
            id_to_index,
            generation,
        })
    })?
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use crate::indexing::ReadPool;
    use crate::indexing::store::{IndexStore, ROOT_ID};

    use super::*;

    // ── Integration test: load from real SQLite DB ───────────────────

    #[test]
    fn integration_load_and_search() {
        use super::super::engine::search;
        use super::super::ranking::ImportanceWeights;
        use super::super::types::{PatternType, SearchQuery};

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Insert test entries
        let users_id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        let alice_id =
            IndexStore::insert_entry_v2(&conn, users_id, "alice", true, false, None, None, None, None).unwrap();
        let _pdf_id = IndexStore::insert_entry_v2(
            &conn,
            alice_id,
            "report.pdf",
            false,
            false,
            Some(1_000_000),
            Some(1_000_000),
            Some(1700000000),
            None,
        )
        .unwrap();
        let _txt_id = IndexStore::insert_entry_v2(
            &conn,
            alice_id,
            "notes.txt",
            false,
            false,
            Some(500),
            Some(500),
            Some(1700000100),
            None,
        )
        .unwrap();

        // Load the index using ReadPool
        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(false);
        let index = load_search_index(&pool, &cancel).unwrap();

        // Root sentinel + 4 entries
        assert_eq!(index.entries.len(), 5);
        assert_eq!(index.id_to_index.len(), 5);

        // Search for PDFs
        let query = SearchQuery {
            name_pattern: Some("*.pdf".to_string()),
            pattern_type: PatternType::Glob,
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
            include_paths: None,
            exclude_dir_names: None,
            include_path_ids: None,
            count_only: false,
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query, &ImportanceWeights::empty()).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
        assert_eq!(result.entries[0].path, "/Users/alice/report.pdf");
    }

    #[test]
    fn load_rightsizes_arena_from_row_count() {
        // A small index must not pre-allocate the ~5M-entry / ~100 MB worst-case
        // arena. Capacity should track the actual row count.
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");

        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(false);
        let index = load_search_index(&pool, &cancel).unwrap();

        // Root sentinel only: before right-sizing this was Vec::with_capacity(5_000_000).
        assert!(
            index.entries.capacity() < 1000,
            "entries capacity {} should track the row count, not the 5M worst case",
            index.entries.capacity()
        );
    }

    #[test]
    fn load_cancellation() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("failed to open store");

        let pool = ReadPool::new(db_path).unwrap();
        let cancel = AtomicBool::new(true); // Pre-cancelled
        let result = load_search_index(&pool, &cancel);
        // With only the root sentinel, cancellation check happens at row 0, but CANCEL_CHECK_INTERVAL
        // is 100K so the first check is at row 0 (0 % 100K == 0). The load should be cancelled.
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }
}
