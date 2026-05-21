//! Integration tests for the fresh-listing oracle wired into `run_volume_scan_preview`.
//!
//! These exercise `run_oracle_aware_batch_scan` directly (the function called by
//! `run_volume_scan_preview` once it's inside its async block). Skipping the
//! Tauri `AppHandle` plumbing keeps the tests focused on the
//! oracle-hit-vs-miss-vs-mid-walk decisions.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use super::scan_preview::run_oracle_aware_batch_scan;
use crate::file_system::get_volume_manager;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::{BatchScanResult, CopyScanResult, InMemoryVolume, Volume, VolumeError};

/// Wraps an `InMemoryVolume` and counts `list_directory` calls so tests can
/// assert that the oracle short-circuited or fell through.
///
/// `watched` is the test-only `listing_is_watched` override. Each test flips
/// it independently — the oracle picker reads it via the `Volume` trait
/// dispatch through `get_volume_manager()`, so we need the wrapper registered
/// in the manager.
struct CountingWatchedVolume {
    inner: InMemoryVolume,
    watched: AtomicBool,
    list_dir_calls: AtomicUsize,
}

impl CountingWatchedVolume {
    fn new(name: &str, watched: bool) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            watched: AtomicBool::new(watched),
            list_dir_calls: AtomicUsize::new(0),
        }
    }

    #[allow(dead_code, reason = "Kept for future tests that flip the watcher mid-scan.")]
    fn set_watched(&self, v: bool) {
        self.watched.store(v, Ordering::Relaxed);
    }

    fn list_dir_count(&self) -> usize {
        self.list_dir_calls.load(Ordering::Relaxed)
    }
}

impl Volume for CountingWatchedVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_dir_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        self.watched.load(Ordering::Relaxed)
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }

    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        // Reuse the default trait implementation by calling through self.inner.
        // The inner volume's `scan_for_copy` does the actual list_directory work
        // and is counted via this wrapper's `list_directory` only if the inner
        // backend actually calls it. InMemory's `scan_for_copy` walks the entries
        // map directly, so calls from this path don't bump `list_dir_calls` —
        // which is exactly what we want for the cold-cache assertion (a real
        // list_directory call only happens on the oracle path when we miss).
        let _ = on_progress;
        self.inner.scan_for_copy_batch(paths)
    }
}

/// Unique-per-test id so tests can run in parallel without colliding in
/// `LISTING_CACHE` / `VolumeManager`.
fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "scanprev_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn make_file_entry(name: &str, parent: &str, size: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        permissions: 0o644,
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            false,
            false,
        )
    }
}

fn make_dir_entry(name: &str, parent: &str) -> FileEntry {
    FileEntry {
        permissions: 0o755,
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            true,
            false,
        )
    }
}

fn make_symlinked_dir_entry(name: &str, parent: &str) -> FileEntry {
    FileEntry {
        permissions: 0o755,
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            true,
            true,
        )
    }
}

/// Inserts a CachedListing directly into LISTING_CACHE. Returns the listing_id.
fn insert_listing(id: &str, volume_id: &str, path: &str, entries: Vec<FileEntry>) -> String {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.insert(
        id.to_string(),
        CachedListing {
            volume_id: volume_id.to_string(),
            path: PathBuf::from(path),
            entries,
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            sequence: AtomicU64::new(1),
            created_at: std::time::Instant::now(),
        },
    );
    id.to_string()
}

fn remove_listing(id: &str) {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.remove(id);
}

/// Test 1: when the parent listing is watcher-backed, scan-preview reads sizes
/// from the cache and never calls `list_directory` on the volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_preview_uses_watched_listing_for_top_level_files() {
    let vid = unique("uses_watched");
    let lid = unique("uses_watched_lid");

    // Pre-populate the InMemoryVolume so the wrapper's `list_directory` would
    // actually return useful data IF called. We assert it ISN'T called.
    let vol = Arc::new(CountingWatchedVolume::new("watched-vol", true));
    // Note: we do NOT also seed entries via `create_file`. The whole point is
    // that the oracle reads from the cached listing rather than the backend.
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    let cached = vec![
        make_file_entry("a.jpg", "/dcim", 1000),
        make_file_entry("b.jpg", "/dcim", 2000),
        make_file_entry("c.jpg", "/dcim", 3000),
    ];
    let lid_inserted = insert_listing(&lid, &vid, "/dcim", cached);

    let sources = vec![
        PathBuf::from("/dcim/a.jpg"),
        PathBuf::from("/dcim/b.jpg"),
        PathBuf::from("/dcim/c.jpg"),
    ];
    let is_cancelled = || false;
    let on_progress = |_: crate::file_system::volume::ListingProgress| {};
    let result = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("oracle-aware batch scan should succeed");

    assert_eq!(
        vol.list_dir_count(),
        0,
        "expected zero list_directory calls on oracle hit"
    );
    assert_eq!(result.aggregate.file_count, 3);
    assert_eq!(result.aggregate.total_bytes, 6000);
    assert_eq!(result.per_path.len(), 3);
    // Order matches the caller's `sources` order, per BatchScanResult contract.
    assert_eq!(result.per_path[0].0, PathBuf::from("/dcim/a.jpg"));
    assert_eq!(result.per_path[1].0, PathBuf::from("/dcim/b.jpg"));
    assert_eq!(result.per_path[2].0, PathBuf::from("/dcim/c.jpg"));

    remove_listing(&lid_inserted);
    get_volume_manager().unregister(&vid);
}

/// Test 2: when `listing_is_watched` returns false (watcher dead), the oracle
/// returns None and we fall through to the volume's `scan_for_copy_batch`
/// path, which on InMemoryVolume calls scan_for_copy per path (no list_directory
/// call). What we really want to assert: the oracle did NOT short-circuit
/// (so the per-path data still comes from the volume backend, not the cache).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_preview_falls_through_when_watcher_dead() {
    let vid = unique("dead_watcher");
    let lid = unique("dead_watcher_lid");

    let vol = Arc::new(CountingWatchedVolume::new("dead-watcher-vol", false));
    // Seed the backend so the cold-path scan can answer correctly.
    vol.inner
        .create_file(Path::new("/cold/a.jpg"), b"hello-12byte")
        .await
        .unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Cached entries with a clearly-bogus size: if the oracle were used despite
    // the watcher being dead, the result would carry this size instead of the
    // backend's real 12 bytes.
    let cached = vec![make_file_entry("a.jpg", "/cold", 99999)];
    let lid_inserted = insert_listing(&lid, &vid, "/cold", cached);

    let sources = vec![PathBuf::from("/cold/a.jpg")];
    let is_cancelled = || false;
    let on_progress = |_: crate::file_system::volume::ListingProgress| {};
    let result = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("scan should succeed via fallthrough");

    assert_eq!(result.aggregate.file_count, 1);
    // Real backend size (12 bytes "hello-12byte"), NOT the cached 99999.
    assert_eq!(
        result.aggregate.total_bytes, 12,
        "watcher-dead path must NOT consume the cached size"
    );

    remove_listing(&lid_inserted);
    get_volume_manager().unregister(&vid);
}

/// Test 3: when a subfolder of the scanned dir is open in another pane (a
/// second watcher-backed listing in `LISTING_CACHE`), the walker reuses that
/// subfolder's listing and never lists it via the volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_preview_uses_cached_subfolder_listing_when_other_pane_has_it() {
    let vid = unique("subfolder_pane");
    let parent_lid = unique("subfolder_parent_lid");
    let sub_lid = unique("subfolder_child_lid");

    let vol = Arc::new(CountingWatchedVolume::new("subfolder-vol", true));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Parent pane lists `/a` with one entry: subfolder `sub` (a directory).
    let parent_cached = vec![make_dir_entry("sub", "/a")];
    let parent_lid_inserted = insert_listing(&parent_lid, &vid, "/a", parent_cached);

    // Other pane lists `/a/sub` with two cached files. Both panes share the
    // same volume; `listing_is_watched` returns true volume-wide so the oracle
    // hits both.
    let sub_cached = vec![
        make_file_entry("x.txt", "/a/sub", 100),
        make_file_entry("y.txt", "/a/sub", 200),
    ];
    let sub_lid_inserted = insert_listing(&sub_lid, &vid, "/a/sub", sub_cached);

    // Scanning a copy of `/a` (selecting the subfolder).
    let sources = vec![PathBuf::from("/a/sub")];
    let is_cancelled = || false;
    let on_progress = |_: crate::file_system::volume::ListingProgress| {};
    let result = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("scan should succeed");

    assert_eq!(
        vol.list_dir_count(),
        0,
        "expected no list_directory call when the subfolder is open in another pane"
    );
    assert_eq!(result.aggregate.file_count, 2);
    // `BatchScanResult::aggregate.dir_count` counts descendant directories only,
    // not the top-level selected source itself. `/a/sub` has no subdirectories
    // among its cached entries, so 0 is correct. (Same convention as
    // `Volume::scan_for_copy_batch`'s default impl.)
    assert_eq!(result.aggregate.dir_count, 0);
    assert_eq!(result.aggregate.total_bytes, 300);

    remove_listing(&parent_lid_inserted);
    remove_listing(&sub_lid_inserted);
    get_volume_manager().unregister(&vid);
}

/// Test 4: a cached entry with `is_symlink == true` and `is_directory == true`
/// is counted as one file-shaped entry. The walker must NOT recurse into it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_preview_preserves_symlink_semantics() {
    let vid = unique("symlink");
    let parent_lid = unique("symlink_parent_lid");

    let vol = Arc::new(CountingWatchedVolume::new("symlink-vol", true));
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // /a is open, has one entry: `link-to-elsewhere`, a symlinked directory.
    let parent_cached = vec![make_symlinked_dir_entry("link-to-elsewhere", "/a")];
    let parent_lid_inserted = insert_listing(&parent_lid, &vid, "/a", parent_cached);

    // We do NOT cache anything for /a/link-to-elsewhere. If the walker were
    // to recurse into a symlinked directory, the oracle would miss for that
    // child and `list_directory` would be called. We assert that doesn't happen.

    let sources = vec![PathBuf::from("/a/link-to-elsewhere")];
    let is_cancelled = || false;
    let on_progress = |_: crate::file_system::volume::ListingProgress| {};
    let result = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("symlink scan should succeed");

    assert_eq!(
        vol.list_dir_count(),
        0,
        "expected no list_directory call: symlinks must not be recursed into"
    );
    // Symlink counts as one file-shaped entry (size 0 from the default
    // FileEntry; the walker just records what the cache holds).
    assert_eq!(result.aggregate.file_count, 1);
    assert_eq!(result.aggregate.dir_count, 0);

    remove_listing(&parent_lid_inserted);
    get_volume_manager().unregister(&vid);
}

/// Test 5: if the user closes pane B (where `/a/sub` was open) right before
/// the walker recurses into `/a/sub`, the oracle returns None for `/a/sub`
/// and the walker falls through to `volume.list_directory`. The end-to-end
/// result is still correct.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scan_preview_handles_listing_closed_mid_walk() {
    let vid = unique("mid_walk_close");
    let parent_lid = unique("mid_walk_parent_lid");
    let sub_lid = unique("mid_walk_sub_lid");

    let vol = Arc::new(CountingWatchedVolume::new("mid-walk-vol", true));
    // Seed the backing volume with what /a/sub really contains, so the
    // fallthrough path produces sensible results.
    vol.inner
        .create_file(Path::new("/a/sub/real.bin"), b"abcdef")
        .await
        .unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Parent pane lists `/a` with one entry: subfolder `sub`.
    let parent_cached = vec![make_dir_entry("sub", "/a")];
    let parent_lid_inserted = insert_listing(&parent_lid, &vid, "/a", parent_cached);

    // Pane B has `/a/sub` cached too, but we close it BEFORE running the scan.
    let sub_entries = vec![make_file_entry("phantom.bin", "/a/sub", 12345)];
    let sub_lid_inserted = insert_listing(&sub_lid, &vid, "/a/sub", sub_entries);
    // Simulate pane B closing: remove the listing right before the scan.
    remove_listing(&sub_lid_inserted);

    let sources = vec![PathBuf::from("/a/sub")];
    let is_cancelled = || false;
    let on_progress = |_: crate::file_system::volume::ListingProgress| {};
    let result = run_oracle_aware_batch_scan(vol.as_ref(), &vid, &sources, &is_cancelled, &on_progress)
        .await
        .expect("mid-walk close scan should succeed");

    // /a is still oracle-hit (it's the parent of the input source). The
    // recursion into /a/sub goes through `scan_subtree_with_oracle` which
    // misses the closed sub-listing and calls `volume.list_directory`.
    assert!(
        vol.list_dir_count() >= 1,
        "expected fallthrough to list_directory for the closed /a/sub listing"
    );
    // Result should reflect the real backend content, not the stale cached
    // phantom file.
    assert_eq!(result.aggregate.file_count, 1);
    assert_eq!(
        result.aggregate.total_bytes, 6,
        "should reflect real file size, not the stale cache"
    );

    remove_listing(&parent_lid_inserted);
    get_volume_manager().unregister(&vid);
}
