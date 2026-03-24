//! Search index: in-memory entry storage, lifecycle management, and timers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use crate::indexing::ReadPool;
use crate::indexing::writer::WRITER_GENERATION;

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

pub(crate) struct SearchIndexState {
    pub index: Arc<SearchIndex>,
    pub idle_timer: Option<tauri::async_runtime::JoinHandle<()>>,
    pub backstop_timer: Option<tauri::async_runtime::JoinHandle<()>>,
    pub load_cancel: Option<Arc<AtomicBool>>,
}

pub(crate) static SEARCH_INDEX: LazyLock<Mutex<Option<SearchIndexState>>> = LazyLock::new(|| Mutex::new(None));

/// Timestamp of the last search-related IPC call, for backstop timeout.
static LAST_SEARCH_ACTIVITY: AtomicU64 = AtomicU64::new(0);

/// Whether the search dialog is currently open. Timers check this before dropping.
pub(crate) static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

/// Idle timeout: drop the index 5 minutes after `release_search_index`.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Backstop timeout: drop the index if no search calls arrive within 10 minutes.
const BACKSTOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Rows between cancellation checks during load.
const CANCEL_CHECK_INTERVAL: usize = 100_000;

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Record a search activity timestamp (resets backstop timer logic).
pub(crate) fn touch_activity() {
    LAST_SEARCH_ACTIVITY.store(now_secs(), Ordering::Relaxed);
}

// ── Index loading ────────────────────────────────────────────────────

/// Load all entries from the index DB into an in-memory `SearchIndex`.
///
/// `name_folded` is NOT loaded — the search pattern is normalized instead
/// (NFD on macOS) to avoid ~5.1M extra String allocations and ~300 MB of memory.
pub(crate) fn load_search_index(pool: &ReadPool, cancel: &AtomicBool) -> Result<SearchIndex, String> {
    pool.with_conn(|conn: &rusqlite::Connection| {
        let t = std::time::Instant::now();
        let generation = WRITER_GENERATION.load(Ordering::Relaxed);

        let sql = "SELECT id, parent_id, name, is_directory, logical_size, modified_at FROM entries";

        let mut stmt = conn.prepare(sql).map_err(|e| format!("Prepare failed: {e}"))?;

        // Phase 1: Load all entries into Vec (sequential writes to contiguous memory)
        // Arena-allocate all filenames into a single String to avoid per-entry heap allocations.
        let mut names = String::with_capacity(100_000_000); // ~5M entries × ~20 bytes avg
        let mut entries = Vec::with_capacity(5_000_000);

        let mut rows = stmt.query([]).map_err(|e| format!("Query failed: {e}"))?;
        let mut row_count = 0usize;

        while let Some(row) = rows.next().map_err(|e| format!("Row read failed: {e}"))? {
            if row_count.is_multiple_of(CANCEL_CHECK_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err("Load cancelled".to_string());
            }

            let id: i64 = row.get(0).map_err(|e| format!("{e}"))?;
            let parent_id: i64 = row.get(1).map_err(|e| format!("{e}"))?;
            // Borrow directly from SQLite's internal buffer via ValueRef — zero heap allocations.
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
            "Search index loaded: {row_count} entries, generation {generation}, took {:?}",
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

// ── Backstop timer ───────────────────────────────────────────────────

/// Start the backstop timer. Drops the index if no search activity within `BACKSTOP_TIMEOUT`.
pub(crate) fn start_backstop_timer() -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(BACKSTOP_TIMEOUT).await;
            let last = LAST_SEARCH_ACTIVITY.load(Ordering::Relaxed);
            let elapsed = now_secs().saturating_sub(last);
            if elapsed >= BACKSTOP_TIMEOUT.as_secs() {
                if DIALOG_OPEN.load(Ordering::Relaxed) {
                    log::debug!("Search index backstop timer deferred, dialog still open");
                    continue;
                }
                log::debug!("Search index backstop timeout reached, dropping index");
                drop_search_index();
                break;
            }
            // Activity happened recently — loop and check again
        }
    })
}

/// Start the idle timer (5 min). Called when the search dialog closes.
pub(crate) fn start_idle_timer() -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(IDLE_TIMEOUT).await;
            if DIALOG_OPEN.load(Ordering::Relaxed) {
                log::debug!("Search index idle timer deferred, dialog still open");
                continue;
            }
            log::debug!("Search index idle timeout reached, dropping index");
            drop_search_index();
            break;
        }
    })
}

/// Drop the search index and cancel any timers.
pub(crate) fn drop_search_index() {
    let mut guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if let Some(state) = guard.take() {
        if let Some(h) = state.idle_timer {
            h.abort();
        }
        if let Some(h) = state.backstop_timer {
            h.abort();
        }
        log::debug!("Search index dropped");
    }
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
            limit: 30,
            case_sensitive: None,
            exclude_system_dirs: Some(false),
        };
        let result = search(&index, &query).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.entries[0].name, "report.pdf");
        assert_eq!(result.entries[0].path, "/Users/alice/report.pdf");
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
