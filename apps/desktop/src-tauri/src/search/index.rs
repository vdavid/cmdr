//! Search index: in-memory entry storage, lifecycle management, and timers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
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

pub(crate) struct SearchIndexState {
    pub index: Arc<SearchIndex>,
    pub idle_timer: Option<tauri::async_runtime::JoinHandle<()>>,
    pub backstop_timer: Option<tauri::async_runtime::JoinHandle<()>>,
    pub load_cancel: Option<Arc<AtomicBool>>,
}

pub(crate) static SEARCH_INDEX: LazyLock<Mutex<Option<SearchIndexState>>> = LazyLock::new(|| Mutex::new(None));

// ── Importance weight map (per-volume ranking snapshot) ──────────────────────

/// The current importance weight snapshot for the root volume, blended into search
/// ranking. Loaded once from [`ImportanceIndex`](crate::importance::ImportanceIndex)
/// and reloaded on each recompute (subscribe-don't-poll), so a search never queries
/// `importance.db` per result. Empty until the first load (and whenever no weights
/// are available), which degrades ranking to match-quality + recency — today's
/// behavior. Held as an `Arc` so a search clones a cheap handle and ranks against a
/// stable snapshot even if a reload swaps the map mid-search.
static IMPORTANCE_WEIGHTS: LazyLock<Mutex<Arc<super::ranking::ImportanceWeights>>> =
    LazyLock::new(|| Mutex::new(Arc::new(super::ranking::ImportanceWeights::empty())));

/// A cheap clone of the current importance weight snapshot, for the search engine
/// to rank against. Returns an empty map when no weights have loaded (degrading to
/// match-quality + recency ranking).
pub(crate) fn importance_weights_snapshot() -> Arc<super::ranking::ImportanceWeights> {
    IMPORTANCE_WEIGHTS.lock_ignore_poison().clone()
}

/// Replace the importance weight snapshot. Called by the recompute subscriber after
/// (re)loading the map from the read API.
fn store_importance_weights(weights: super::ranking::ImportanceWeights) {
    *IMPORTANCE_WEIGHTS.lock_ignore_poison() = Arc::new(weights);
}

/// The volume the search index and its importance weights track. Search is
/// root-only (it loads `get_read_pool()`, the root drive index), so its weight map
/// mirrors the root volume's `importance.db`.
const SEARCH_VOLUME_ID: &str = "root";

/// (Re)load the root volume's importance weights from the read API into the
/// snapshot. A missing/empty `importance.db` (offline, fresh install, disabled
/// indexing, a purged cache) yields an empty map — ranking then degrades to
/// match-quality + recency, byte-for-byte today's behavior. Runs on a blocking
/// thread (a SQLite read); never on the IPC thread.
fn reload_importance_weights(data_dir: &std::path::Path) {
    use crate::importance::{ImportanceIndex, SignalSet};
    // `SignalSet::all()` matters only for `explain`, which we don't call here; the
    // bulk weight read ignores it. Root is local, so `all()` is correct anyway.
    let index = ImportanceIndex::open(data_dir, SEARCH_VOLUME_ID, SignalSet::all());
    match index.all_nonzero_weights() {
        Ok(map) => {
            let count = map.len();
            store_importance_weights(super::ranking::ImportanceWeights::from_map(map));
            log::debug!(target: "search", "importance weights loaded: {} scored folders", count);
        }
        Err(e) => {
            // A read failure leaves the previous snapshot in place (or empty on
            // first load) — never a hard failure, importance is advisory.
            log::debug!(target: "search", "importance weights not loaded: {e}");
        }
    }
}

/// Start the recompute subscriber that keeps the search importance weight map fresh.
///
/// Subscribes to the root volume's recompute-completed `watch` (subscribe-don't-poll)
/// and reloads the weight map on each pass, so a search always ranks against the
/// latest weights without polling. Loads once up front too: the `watch` retains the
/// last generation, so if a recompute already completed at launch the initial
/// `borrow_and_update` reload covers it; the loop then catches every later pass.
/// Called once from app setup, alongside the importance scheduler start.
pub(crate) fn start_importance_weight_subscriber(data_dir: std::path::PathBuf) {
    let mut rx = crate::importance::read::subscribe(SEARCH_VOLUME_ID);
    tauri::async_runtime::spawn(async move {
        // Initial load (covers a recompute that finished before this subscription).
        let dir = data_dir.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || reload_importance_weights(&dir)).await;
        rx.borrow_and_update();
        while rx.changed().await.is_ok() {
            let _generation = *rx.borrow_and_update();
            let dir = data_dir.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || reload_importance_weights(&dir)).await;
        }
    });
}

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
            // Activity happened recently: loop and check again
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

    // ── Importance weight snapshot lifecycle ─────────────────────────

    use std::sync::Mutex as StdMutex;

    /// Serializes the two weight-snapshot tests: `IMPORTANCE_WEIGHTS` is a
    /// process-global they both mutate, so they can't run concurrently.
    static WEIGHTS_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    /// Write a `(path, score)` set into a fresh `importance-root.db` under `data_dir`
    /// via the real writer, so `reload_importance_weights` reads a genuine store.
    fn write_importance_db(data_dir: &std::path::Path, rows: &[(&str, f64)]) {
        use crate::importance::store::importance_db_path;
        use crate::importance::writer::{ImportanceWriter, WeightRow};
        let db_path = importance_db_path(data_dir, SEARCH_VOLUME_ID);
        let writer = ImportanceWriter::spawn(&db_path).expect("spawn writer");
        let weight_rows: Vec<WeightRow> = rows
            .iter()
            .map(|(path, score)| WeightRow {
                path: path.to_string(),
                score: *score,
                signals_json: "{}".to_string(),
            })
            .collect();
        writer.write_weights(1, weight_rows).expect("write");
        writer.flush_blocking().expect("flush");
        writer.shutdown();
    }

    /// `reload_importance_weights` loads a populated store into the snapshot the
    /// search engine ranks against, and a missing store yields the neutral empty
    /// snapshot (the degradation contract's runtime path).
    #[test]
    fn reload_loads_populated_store_and_missing_is_empty() {
        let _guard = WEIGHTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Missing store ⇒ empty snapshot (no importance.db at all).
        let empty_dir = tempfile::tempdir().expect("temp dir");
        reload_importance_weights(empty_dir.path());
        let snap = importance_weights_snapshot();
        assert!(snap.is_empty(), "no importance.db ⇒ empty weight snapshot");

        // Populated store ⇒ the snapshot reflects its non-zero weights.
        let dir = tempfile::tempdir().expect("temp dir");
        write_importance_db(
            dir.path(),
            &[
                ("/Users/me/Documents", 0.7),
                ("/Users/me/proj", 0.9),
                ("/Users/me/node_modules", 0.0), // floored ⇒ omitted
            ],
        );
        reload_importance_weights(dir.path());
        let snap = importance_weights_snapshot();
        assert_eq!(snap.weight_for("/Users/me/Documents"), 0.7);
        assert_eq!(snap.weight_for("/Users/me/proj"), 0.9);
        assert_eq!(
            snap.weight_for("/Users/me/node_modules"),
            0.0,
            "floored folder unscored"
        );
        assert_eq!(snap.weight_for("/Users/me/unscored"), 0.0, "unknown path ⇒ neutral 0.0");

        // Reset the global so a later test isn't affected by this one.
        store_importance_weights(super::super::ranking::ImportanceWeights::empty());
    }

    /// A recompute completing fires the subscription, and the next reload picks up
    /// the freshly-written weights (the subscribe-don't-poll reload path). Proves a
    /// second recompute's weights replace the first. Uses `has_changed()` (no await)
    /// so it stays a plain sync test — the `watch` sender flips the flag on
    /// `send_replace`, no runtime needed.
    #[test]
    fn recompute_notification_refreshes_the_snapshot() {
        let _guard = WEIGHTS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().expect("temp dir");

        // First pass writes an early weight; reload picks it up.
        write_importance_db(dir.path(), &[("/Users/me/proj", 0.4)]);
        reload_importance_weights(dir.path());
        assert_eq!(importance_weights_snapshot().weight_for("/Users/me/proj"), 0.4);

        // A subscriber observes the recompute notification (subscribe-don't-poll),
        // then reloads — seeing the second pass's higher weight.
        let mut rx = crate::importance::read::subscribe(SEARCH_VOLUME_ID);
        rx.borrow_and_update();
        write_importance_db(dir.path(), &[("/Users/me/proj", 0.95)]);
        crate::importance::read::notify_recompute_completed_for_test(SEARCH_VOLUME_ID, 2);
        assert!(rx.has_changed().expect("sender alive"), "the notification fired");
        rx.borrow_and_update();
        reload_importance_weights(dir.path());
        assert_eq!(
            importance_weights_snapshot().weight_for("/Users/me/proj"),
            0.95,
            "the next reload after a recompute sees the new weights"
        );

        store_importance_weights(super::super::ranking::ImportanceWeights::empty());
    }
}
