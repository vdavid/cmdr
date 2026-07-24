//! The query-side ANN route: per-volume, per-space, decide ONCE (until invalidated)
//! whether semantic search runs on the ANN index or the exact brute-force scan, and
//! keep the mmap `view` handle warm.
//!
//! Mirrors `vector::cache`'s discipline: loaded on first use, invalidated per
//! completed enrichment pass (rode by `vector::cache::invalidate`, the one choke
//! point for "this volume's derived query caches changed"), dropped wholesale by
//! the memory-watchdog stop hook. A `view` handle's at-rest RSS is tiny (pages
//! fault in on demand and stay evictable — the spike's mmap numbers), so this cache
//! is about avoiding per-query file opens and route decisions, not about RAM.
//!
//! The route decision:
//!
//! - fewer than `threshold` ([`super::ANN_MIN_VECTORS`]) stored vectors → brute
//!   force (exact, small, no index file to maintain);
//! - at/above it with a healthy index file → ANN (`view` mode, `expansion_search`
//!   scaled to the corpus);
//! - at/above it with a missing/corrupt/stale index → kick ONE background rebuild
//!   and fall back to brute force until it lands. A bad index NEVER breaks search.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use super::{AnnError, AnnSpace, engine_err, expansion_search_for, index_path, read_meta, verify_index_checksum};
use crate::ignore_poison::IgnorePoison;
use crate::media_index::store;

/// A warm, immutable mmap `view` of a volume's ANN index. `usearch::Index` is
/// thread-safe for concurrent searches, so one handle serves every query until
/// invalidation.
pub(crate) struct AnnHandle {
    pub(crate) index: usearch::Index,
    pub(crate) dims: usize,
}

/// The routing decision for one volume's space, cached until invalidated.
#[derive(Clone)]
pub(crate) enum Route {
    /// Score the resident f16 cache exactly (small corpus, or ANN unusable while a
    /// rebuild runs).
    BruteForce,
    /// Search the warm ANN view.
    Ann(Arc<AnnHandle>),
}

type RouteMap = HashMap<(PathBuf, AnnSpace), Route>;

static CACHE: LazyLock<Mutex<RouteMap>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resolve (and cache) the route for a volume's space. `threshold` is
/// [`super::ANN_MIN_VECTORS`] in production; tests pass small values.
pub(crate) fn route(db_path: &Path, space: AnnSpace, threshold: usize, model_id: &str) -> Route {
    let key = (db_path.to_path_buf(), space);
    if let Some(route) = CACHE.lock_ignore_poison().get(&key) {
        return route.clone();
    }
    // Decide OUTSIDE the lock (a DB count + an index open can be slow); a concurrent
    // decider does the same work and the last write wins — both produce the same route.
    let route = decide(db_path, space, threshold, model_id);
    CACHE.lock_ignore_poison().insert(key, route.clone());
    route
}

fn decide(db_path: &Path, space: AnnSpace, threshold: usize, model_id: &str) -> Route {
    let count = store::embedding_count(db_path, space.table());
    if (count as usize) < threshold {
        return Route::BruteForce;
    }
    match open_view(db_path, space, model_id) {
        Ok(handle) => Route::Ann(Arc::new(handle)),
        Err(e) => {
            log::info!(
                target: "media_index",
                "ann index for {} unusable ({e}); brute-force fallback while it rebuilds",
                db_path.display()
            );
            super::rebuild::kick(db_path, space, model_id);
            Route::BruteForce
        }
    }
}

/// Open the index in mmap `view` mode after validating the sidecar AND the file's
/// checksum, and tune `expansion_search` to the corpus size. The checksum gate is
/// load-bearing: usearch trusts the bytes it maps, and a corrupt body would
/// otherwise SIGSEGV at search time instead of failing closed into the fallback.
fn open_view(db_path: &Path, space: AnnSpace, model_id: &str) -> Result<AnnHandle, AnnError> {
    let meta = read_meta(db_path, space, model_id)?;
    let idx_path = index_path(db_path, space);
    if !idx_path.exists() {
        return Err(AnnError::Missing);
    }
    verify_index_checksum(db_path, space, &meta)?;
    let idx_str = idx_path
        .to_str()
        .ok_or_else(|| AnnError::Io(std::io::Error::other("non-utf8 ann index path")))?;
    let index = usearch::new_index(&super::index_options(meta.dims)).map_err(engine_err)?;
    index.view(idx_str).map_err(engine_err)?;
    index.change_expansion_search(expansion_search_for(index.size()));
    Ok(AnnHandle { index, dims: meta.dims })
}

/// Drop a volume's cached routes (every space) so the next query re-decides.
/// Called from `vector::cache::invalidate` — the shared per-pass invalidation
/// choke point — and after a rebuild lands.
pub(crate) fn invalidate(db_path: &Path) {
    let mut cache = CACHE.lock_ignore_poison();
    cache.retain(|(path, _), _| path != db_path);
}

/// Drop every cached route/view (the memory-watchdog stop action, alongside the
/// resident vector caches). Views reload lazily.
pub(crate) fn clear_all() {
    CACHE.lock_ignore_poison().clear();
}
