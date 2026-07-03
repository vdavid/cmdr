//! Parsed-index cache, keyed by `(path, size, mtime)`.
//!
//! Parsing a central directory is real I/O plus allocation, and browsing hits
//! `list`/`get` constantly, so the parsed [`ArchiveIndex`] is cached. The key
//! includes the file's size and mtime, so an external edit to the archive
//! (which changes at least one of them) is a natural cache miss and forces a
//! re-parse — no explicit invalidation needed. Indexes are shared as `Arc`, so
//! a hit is a cheap clone.
//!
//! This is a plain content cache with no eviction policy of its own; the volume
//! layer owns archive lifetime (refcount + LRU per the plan) and can drop the
//! whole cache on teardown.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::ignore_poison::IgnorePoison;

use super::error::ArchiveError;
use super::index::ArchiveIndex;
use super::source::LocalFileSource;

/// Identity of a cached archive: its path plus the size and mtime that were
/// current when it was parsed. Any change to size or mtime misses the cache.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    path: PathBuf,
    size: u64,
    /// Modification time as nanoseconds since the Unix epoch, or `None` if the
    /// platform/file doesn't report one (then only size guards freshness).
    mtime_nanos: Option<i128>,
}

/// A thread-safe cache of parsed archive indexes for local files.
#[derive(Default)]
pub struct ArchiveIndexCache {
    entries: Mutex<HashMap<CacheKey, Arc<ArchiveIndex>>>,
}

impl ArchiveIndexCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the parsed index for the local archive at `path`, parsing on a
    /// miss and caching the result under `(path, size, mtime)`.
    ///
    /// Blocking: stats the file and (on a miss) reads and parses the central
    /// directory. Call from a blocking context (`spawn_blocking`), not directly
    /// on the async executor.
    pub fn index_for_local(&self, path: &Path) -> Result<Arc<ArchiveIndex>, ArchiveError> {
        let key = cache_key_for(path)?;

        if let Some(hit) = self.get(&key) {
            return Ok(hit);
        }

        let source = LocalFileSource::open(path)?;
        let index = Arc::new(ArchiveIndex::parse(&source)?);
        self.insert(key, Arc::clone(&index));
        Ok(index)
    }

    /// Drops every cached index (e.g. on volume teardown).
    pub fn clear(&self) {
        self.entries.lock_ignore_poison().clear();
    }

    /// Number of cached indexes. For tests and diagnostics.
    pub fn len(&self) -> usize {
        self.entries.lock_ignore_poison().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, key: &CacheKey) -> Option<Arc<ArchiveIndex>> {
        self.entries.lock_ignore_poison().get(key).map(Arc::clone)
    }

    fn insert(&self, key: CacheKey, index: Arc<ArchiveIndex>) {
        self.entries.lock_ignore_poison().insert(key, index);
    }
}

/// Builds the `(path, size, mtime)` key by stat-ing `path`.
fn cache_key_for(path: &Path) -> Result<CacheKey, ArchiveError> {
    let meta = std::fs::metadata(path)?;
    let mtime_nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i128);
    Ok(CacheKey {
        path: path.to_path_buf(),
        size: meta.len(),
        mtime_nanos,
    })
}
