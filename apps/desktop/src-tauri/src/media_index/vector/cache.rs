//! The resident per-volume vector caches: load a volume's embeddings from `media.db`
//! ONCE and keep the [`BruteForceVectorStore`] warm, so repeated find-similar / dedup /
//! semantic-search queries don't re-read the BLOBs each time (plan § Query-time vector
//! residency; mirrors `search/`'s warm `SEARCH_INDEX` arena).
//!
//! There are TWO independent spaces per volume (plan M3): the Vision feature print
//! (`media_embedding`, image↔image similarity + dedup) and CLIP (`media_clip_embedding`,
//! natural-language text→image). They are DIFFERENT vector spaces and must never be
//! compared, so each has its own warm store keyed by `(media.db path, kind)`.
//!
//! ## Consistency + the memory watchdog
//!
//! Invalidated per COMPLETED enrichment pass ([`invalidate`], both kinds for a volume),
//! not per write — a naive per-write invalidation would thrash-reload the whole cache
//! mid-pass; the plan accepts eventual consistency until a pass completes. Dropped
//! wholesale by [`clear_all`] when the indexing memory watchdog fires (wired in
//! `scheduler::start`), so the resident vectors are counted against the ONE shared
//! resident-memory ceiling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use crate::media_index::store::{EmbeddingTable, open_read_connection, read_all_embeddings_from};

use super::BruteForceVectorStore;

/// The process-global cache, keyed by `(media.db path, embedding space)` so each volume
/// has its own warm store per space. The `Arc` lets a query clone out a snapshot and drop
/// the lock immediately.
static CACHE: LazyLock<Mutex<HashMap<(PathBuf, EmbeddingTable), Arc<BruteForceVectorStore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get the warm Vision feature-print store for a volume's `media.db` (find-similar / dedup),
/// loading it on first use.
pub fn get_or_load(db_path: &Path) -> Arc<BruteForceVectorStore> {
    get_or_load_kind(db_path, EmbeddingTable::FeaturePrint)
}

/// Get the warm CLIP store for a volume's `media.db` (semantic text→image search), loading
/// it on first use.
pub fn get_or_load_clip(db_path: &Path) -> Arc<BruteForceVectorStore> {
    get_or_load_kind(db_path, EmbeddingTable::Clip)
}

fn get_or_load_kind(db_path: &Path, table: EmbeddingTable) -> Arc<BruteForceVectorStore> {
    let key = (db_path.to_path_buf(), table);
    if let Some(store) = CACHE.lock_ignore_poison().get(&key) {
        return Arc::clone(store);
    }
    // Load OUTSIDE the lock (a DB read can be slow); a concurrent loader just does the
    // same work and the last write wins — both produce the same snapshot.
    let store = Arc::new(load(db_path, table));
    CACHE.lock_ignore_poison().insert(key, Arc::clone(&store));
    store
}

/// Load a volume's embeddings for one space into a fresh store. A missing DB or a read
/// error yields an empty store (the offline / never-enriched case).
fn load(db_path: &Path, table: EmbeddingTable) -> BruteForceVectorStore {
    if !db_path.exists() {
        return BruteForceVectorStore::default();
    }
    let entries = open_read_connection(db_path)
        .and_then(|conn| read_all_embeddings_from(&conn, table))
        .map(|rows| rows.into_iter().map(|r| (r.path, r.vector)).collect())
        .unwrap_or_else(|e| {
            log::warn!(target: "media_index", "vector cache load failed for {} ({table:?}): {e}", db_path.display());
            Vec::new()
        });
    BruteForceVectorStore::new(entries)
}

/// Drop BOTH of a volume's cached stores so the next query reloads them. Called after a
/// completed enrichment pass (its embeddings changed) and after a purge.
pub fn invalidate(db_path: &Path) {
    let mut cache = CACHE.lock_ignore_poison();
    cache.remove(&(db_path.to_path_buf(), EmbeddingTable::FeaturePrint));
    cache.remove(&(db_path.to_path_buf(), EmbeddingTable::Clip));
}

/// Drop every cached store (the memory watchdog's stop action, so resident vectors
/// release under memory pressure). The next query reloads lazily.
pub fn clear_all() {
    CACHE.lock_ignore_poison().clear();
}
