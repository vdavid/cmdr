//! Path resolver: resolves filesystem paths to integer entry IDs.
//!
//! Uses a component-by-component walk with a full-path LRU cache.
//! The cache key is a `CacheKey` wrapper that implements `Hash` and `Eq`
//! using the same normalization as the `platform_case` SQLite collation:
//! - **macOS**: NFD normalize + case fold (matching APFS)
//! - **Linux**: binary comparison (zero overhead)
//!
//! Cache invalidation: on delete or rename, all entries whose key starts
//! with the affected path prefix are dropped from the cache. This is a
//! fast linear scan over the cache (microseconds for 50K entries).

use std::num::NonZeroUsize;

use lru::LruCache;
use rusqlite::Connection;

use crate::indexing::store::{self, IndexStoreError, ROOT_ID};

/// Default LRU cache capacity (~10 MB RAM for 50K entries).
const DEFAULT_CACHE_CAPACITY: usize = 50_000;

// ── CacheKey ─────────────────────────────────────────────────────────

/// A path string wrapper that implements `Hash` and `Eq` using the same
/// normalization as the `platform_case` collation.
///
/// The original-case path is stored as-is inside the wrapper for display.
/// Only hashing and equality use the normalized form.
#[derive(Debug, Clone)]
struct CacheKey {
    /// The original path string (preserved case). Only used by test-only
    /// cache invalidation methods (invalidate_prefix).
    #[cfg(test)]
    original: String,
    /// The normalized form used for hashing and equality.
    normalized: String,
}

impl CacheKey {
    fn new(path: &str) -> Self {
        Self {
            #[cfg(test)]
            original: path.to_string(),
            normalized: store::normalize_for_comparison(path),
        }
    }
}

impl std::hash::Hash for CacheKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.normalized.hash(state);
    }
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.normalized == other.normalized
    }
}

impl Eq for CacheKey {}

/// Check whether `path` starts with `prefix` using the same comparison
/// logic as `CacheKey` equality (normalized on macOS, binary on Linux).
#[cfg(test)]
fn path_starts_with(path: &str, prefix: &str) -> bool {
    let norm_path = store::normalize_for_comparison(path);
    let norm_prefix = store::normalize_for_comparison(prefix);

    if norm_path.len() < norm_prefix.len() {
        return false;
    }
    if !norm_path.starts_with(&norm_prefix) {
        return false;
    }
    // Ensure it's a proper prefix (either exact match or followed by '/')
    norm_path.len() == norm_prefix.len() || norm_path.as_bytes()[norm_prefix.len()] == b'/'
}

// ── PathResolver ─────────────────────────────────────────────────────

/// Resolves filesystem paths to integer entry IDs using a full-path LRU cache.
///
/// Thread safety: `PathResolver` is `Send` but not `Sync`. It's designed to be
/// owned by a single thread or protected by a `Mutex` when shared.
pub struct PathResolver {
    cache: LruCache<CacheKey, i64>,
}

impl PathResolver {
    /// Create a new resolver with the default cache capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CACHE_CAPACITY)
    }

    /// Create a new resolver with a specific cache capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).expect("capacity must be > 0")),
        }
    }

    /// Resolve a path to an entry ID.
    ///
    /// First checks the LRU cache. On miss, walks component-by-component
    /// using `idx_parent_name` lookups, and populates intermediate entries
    /// in the cache.
    pub fn resolve(&mut self, conn: &Connection, path: &str) -> Result<Option<i64>, IndexStoreError> {
        if path == "/" {
            return Ok(Some(ROOT_ID));
        }

        let path = path.strip_suffix('/').unwrap_or(path);

        // Check full-path cache first
        let key = CacheKey::new(path);
        if let Some(&id) = self.cache.get(&key) {
            return Ok(Some(id));
        }

        // Cache miss: walk component-by-component
        let components: Vec<&str> = path
            .strip_prefix('/')
            .unwrap_or(path)
            .split('/')
            .filter(|c| !c.is_empty())
            .collect();

        if components.is_empty() {
            return Ok(Some(ROOT_ID));
        }

        let mut current_id = ROOT_ID;
        let mut current_path = String::new();

        for component in &components {
            current_path.push('/');
            current_path.push_str(component);

            // Check cache for this intermediate path
            let intermediate_key = CacheKey::new(&current_path);
            if let Some(&id) = self.cache.get(&intermediate_key) {
                current_id = id;
                continue;
            }

            // DB lookup
            match store::IndexStore::resolve_component(conn, current_id, component)? {
                Some(id) => {
                    current_id = id;
                    // Cache intermediate result
                    self.cache.put(CacheKey::new(&current_path), current_id);
                }
                None => return Ok(None),
            }
        }

        // Cache the full path
        self.cache.put(CacheKey::new(path), current_id);
        Ok(Some(current_id))
    }

    /// Invalidate all cache entries whose path starts with `prefix`.
    ///
    /// Used after deletes and renames. With a 50K-entry cache this is
    /// a fast linear scan (~microseconds).
    #[cfg(test)]
    pub fn invalidate_prefix(&mut self, prefix: &str) {
        // Collect keys to remove (can't mutate while iterating)
        let keys_to_remove: Vec<CacheKey> = self
            .cache
            .iter()
            .filter(|(k, _)| path_starts_with(&k.original, prefix))
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_remove {
            self.cache.pop(&key);
        }
    }

    /// Invalidate a single exact path from the cache.
    #[cfg(test)]
    pub fn invalidate_exact(&mut self, path: &str) {
        self.cache.pop(&CacheKey::new(path));
    }

    /// Manually insert a path→id mapping into the cache.
    #[cfg(test)]
    pub fn insert(&mut self, path: &str, id: i64) {
        self.cache.put(CacheKey::new(path), id);
    }

    /// Clear the entire cache.
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Return the current number of cached entries.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Whether the cache is empty.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for PathResolver {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::IndexStore;

    fn open_test_conn() -> (Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-resolver.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        (conn, dir)
    }

    #[test]
    fn resolve_root() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();
        assert_eq!(resolver.resolve(&conn, "/").unwrap(), Some(ROOT_ID));
    }

    #[test]
    fn resolve_simple_path() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
        let foo_id = IndexStore::insert_entry_v2(&conn, users_id, "foo", true, false, None, None).unwrap();

        assert_eq!(resolver.resolve(&conn, "/Users").unwrap(), Some(users_id));
        assert_eq!(resolver.resolve(&conn, "/Users/foo").unwrap(), Some(foo_id));
    }

    #[test]
    fn resolve_caches_intermediates() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None).unwrap();
        let c = IndexStore::insert_entry_v2(&conn, b, "c", true, false, None, None).unwrap();

        // First resolve populates cache for all intermediates
        assert_eq!(resolver.resolve(&conn, "/a/b/c").unwrap(), Some(c));

        // Subsequent resolves should hit cache
        assert_eq!(resolver.resolve(&conn, "/a").unwrap(), Some(a));
        assert_eq!(resolver.resolve(&conn, "/a/b").unwrap(), Some(b));

        // Verify cache is populated
        assert!(resolver.len() >= 3);
    }

    #[test]
    fn resolve_nonexistent() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        assert_eq!(resolver.resolve(&conn, "/nonexistent").unwrap(), None);
        assert_eq!(resolver.resolve(&conn, "/a/b/c").unwrap(), None);
    }

    #[test]
    fn resolve_trailing_slash() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        assert_eq!(resolver.resolve(&conn, "/a/").unwrap(), Some(a));
    }

    #[test]
    fn invalidate_prefix_removes_subtree() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None).unwrap();
        let _c = IndexStore::insert_entry_v2(&conn, b, "c.txt", false, false, Some(10), None).unwrap();

        // Populate cache
        resolver.resolve(&conn, "/a/b/c.txt").unwrap();
        assert!(resolver.len() >= 3);

        // Invalidate /a — should remove /a, /a/b, /a/b/c.txt
        resolver.invalidate_prefix("/a");
        assert_eq!(resolver.len(), 0);
    }

    #[test]
    fn invalidate_prefix_preserves_siblings() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let _a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, ROOT_ID, "b", true, false, None, None).unwrap();

        // Populate cache
        resolver.resolve(&conn, "/a").unwrap();
        resolver.resolve(&conn, "/b").unwrap();
        assert_eq!(resolver.len(), 2);

        // Invalidate /a — should keep /b
        resolver.invalidate_prefix("/a");
        assert_eq!(resolver.len(), 1);
        assert_eq!(resolver.resolve(&conn, "/b").unwrap(), Some(b));
    }

    #[test]
    fn invalidate_exact() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        let _b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None).unwrap();

        resolver.resolve(&conn, "/a/b").unwrap();
        assert!(resolver.len() >= 2);

        // Invalidate just /a/b, /a should remain
        resolver.invalidate_exact("/a/b");
        // /a should still be cached
        assert_eq!(resolver.resolve(&conn, "/a").unwrap(), Some(a));
    }

    #[test]
    fn manual_insert_and_clear() {
        let (_conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        resolver.insert("/test/path", 42);
        assert_eq!(resolver.len(), 1);

        resolver.clear();
        assert!(resolver.is_empty());
    }

    #[test]
    fn invalidate_prefix_does_not_match_partial_names() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        // /app and /apple — invalidating /app should NOT remove /apple
        let _app = IndexStore::insert_entry_v2(&conn, ROOT_ID, "app", true, false, None, None).unwrap();
        let apple = IndexStore::insert_entry_v2(&conn, ROOT_ID, "apple", true, false, None, None).unwrap();

        resolver.resolve(&conn, "/app").unwrap();
        resolver.resolve(&conn, "/apple").unwrap();
        assert_eq!(resolver.len(), 2);

        resolver.invalidate_prefix("/app");
        assert_eq!(resolver.len(), 1);
        // /apple should still be cached
        assert_eq!(resolver.resolve(&conn, "/apple").unwrap(), Some(apple));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn case_insensitive_cache_on_macos() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();

        // Resolve with original case
        assert_eq!(resolver.resolve(&conn, "/Users").unwrap(), Some(users));

        // Cache should hit with different case
        assert_eq!(resolver.resolve(&conn, "/users").unwrap(), Some(users));
        assert_eq!(resolver.resolve(&conn, "/USERS").unwrap(), Some(users));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn case_insensitive_invalidation_on_macos() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
        let _foo = IndexStore::insert_entry_v2(&conn, users, "foo", true, false, None, None).unwrap();

        resolver.resolve(&conn, "/Users/foo").unwrap();
        assert!(resolver.len() >= 2);

        // Invalidate with different case
        resolver.invalidate_prefix("/users");
        assert_eq!(
            resolver.len(),
            0,
            "Case-insensitive invalidation should clear /Users and /Users/foo"
        );
    }

    #[test]
    fn deeply_nested_resolution() {
        let (conn, _dir) = open_test_conn();
        let mut resolver = PathResolver::new();

        // Build 20-level deep tree
        let mut parent = ROOT_ID;
        let mut path = String::new();
        for i in 0..20 {
            let name = format!("level{i}");
            let id = IndexStore::insert_entry_v2(&conn, parent, &name, true, false, None, None).unwrap();
            path.push('/');
            path.push_str(&name);
            parent = id;
        }

        // Resolve the full deep path
        assert_eq!(resolver.resolve(&conn, &path).unwrap(), Some(parent));
        // Cache should have all intermediates
        assert!(resolver.len() >= 20);
    }
}
