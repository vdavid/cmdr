//! The resident per-volume vector cache: load a volume's embeddings from `media.db`
//! ONCE and keep the [`BruteForceVectorStore`] warm, so repeated find-similar / dedup
//! queries don't re-read the BLOBs each time (plan § Query-time vector residency;
//! mirrors `search/`'s warm `SEARCH_INDEX` arena).
//!
//! ## Consistency + the memory watchdog
//!
//! Invalidated per COMPLETED enrichment pass ([`invalidate`]), not per write — a
//! naive per-write invalidation would thrash-reload the whole cache mid-pass; the plan
//! accepts eventual consistency until a pass completes. Dropped wholesale by
//! [`clear_all`] when the indexing memory watchdog fires (wired in
//! `scheduler::start`), so the resident vectors are counted against the ONE shared
//! resident-memory ceiling, never a second budget.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use crate::media_index::store::{open_read_connection, read_all_embeddings};

use super::BruteForceVectorStore;

/// The process-global cache, keyed by `media.db` path so each volume has its own warm
/// store. The `Arc` lets a query clone out a snapshot and drop the lock immediately.
static CACHE: LazyLock<Mutex<HashMap<PathBuf, Arc<BruteForceVectorStore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get the warm vector store for a volume's `media.db`, loading it from disk on first
/// use (or after an [`invalidate`]). A missing/never-enriched DB yields an empty
/// store, not an error, so a find-similar over an un-enriched volume returns nothing.
pub fn get_or_load(db_path: &Path) -> Arc<BruteForceVectorStore> {
    if let Some(store) = CACHE.lock_ignore_poison().get(db_path) {
        return Arc::clone(store);
    }
    // Load OUTSIDE the lock (a DB read can be slow); a concurrent loader just does the
    // same work and the last write wins — both produce the same snapshot.
    let store = Arc::new(load(db_path));
    CACHE
        .lock_ignore_poison()
        .insert(db_path.to_path_buf(), Arc::clone(&store));
    store
}

/// Load a volume's embeddings into a fresh store. A missing DB or a read error yields
/// an empty store (the offline / never-enriched case).
fn load(db_path: &Path) -> BruteForceVectorStore {
    if !db_path.exists() {
        return BruteForceVectorStore::default();
    }
    let entries = open_read_connection(db_path)
        .and_then(|conn| read_all_embeddings(&conn))
        .map(|rows| rows.into_iter().map(|r| (r.path, r.vector)).collect())
        .unwrap_or_else(|e| {
            log::warn!(target: "media_index", "vector cache load failed for {}: {e}", db_path.display());
            Vec::new()
        });
    BruteForceVectorStore::new(entries)
}

/// Drop a volume's cached store so the next query reloads it. Called after a completed
/// enrichment pass (its embeddings changed) and after a purge.
pub fn invalidate(db_path: &Path) {
    CACHE.lock_ignore_poison().remove(db_path);
}

/// Drop every cached store (the memory watchdog's stop action, so resident vectors
/// release under memory pressure). The next query reloads lazily.
pub fn clear_all() {
    CACHE.lock_ignore_poison().clear();
}
