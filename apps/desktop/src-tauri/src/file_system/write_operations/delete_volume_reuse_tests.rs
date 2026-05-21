//! Integration tests for `delete_volume_files_with_progress_inner`'s reuse of
//! the scan preview and the fresh-listing oracle (M3 of the fresh-listing-reuse
//! plan).
//!
//! These tests use the `OperationEventSink` test plumbing (no Tauri
//! `AppHandle`) and a counter-wrapping `InMemoryVolume` so we can assert call
//! counts directly. The patterns mirror `scan_preview_oracle_tests.rs` (oracle
//! wiring, listing cache seeding) and `volume_copy_tests.rs` (state + sink +
//! preview-result seeding).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use super::delete::delete_volume_files_with_progress_inner;
use super::state::{CachedScanResult, OperationIntent, SCAN_PREVIEW_RESULTS, WriteOperationState};
use super::types::{CollectorEventSink, WriteOperationConfig, WriteOperationError};
use crate::file_system::get_volume_manager;
use crate::file_system::listing::caching::{CachedListing, LISTING_CACHE};
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::{BatchScanResult, CopyScanResult, InMemoryVolume, Volume, VolumeError};

// ----------------------------------------------------------------------------
// Counter-wrapping volume
// ----------------------------------------------------------------------------

/// Wraps an `InMemoryVolume` and counts `list_directory`, `is_directory`, and
/// `delete` calls. Lets `listing_is_watched` be flipped at runtime so tests can
/// pin both oracle-hit and oracle-miss behaviours.
struct CountingVolume {
    inner: InMemoryVolume,
    watched: AtomicBool,
    list_dir_calls: AtomicUsize,
    is_dir_calls: AtomicUsize,
    delete_calls: AtomicUsize,
}

impl CountingVolume {
    fn new(name: &str, watched: bool) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            watched: AtomicBool::new(watched),
            list_dir_calls: AtomicUsize::new(0),
            is_dir_calls: AtomicUsize::new(0),
            delete_calls: AtomicUsize::new(0),
        }
    }

    fn list_dir_count(&self) -> usize {
        self.list_dir_calls.load(Ordering::Relaxed)
    }

    fn is_dir_count(&self) -> usize {
        self.is_dir_calls.load(Ordering::Relaxed)
    }

    fn delete_count(&self) -> usize {
        self.delete_calls.load(Ordering::Relaxed)
    }

    #[allow(dead_code, reason = "Kept for future tests that flip the watcher mid-scan.")]
    fn set_watched(&self, v: bool) {
        self.watched.store(v, Ordering::Relaxed);
    }
}

impl Volume for CountingVolume {
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
        self.is_dir_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.is_directory(path)
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.delete_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.delete(path)
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
        let _ = on_progress;
        self.inner.scan_for_copy_batch(paths)
    }
}

// ----------------------------------------------------------------------------
// Test helpers
// ----------------------------------------------------------------------------

fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "delreuse_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

fn make_file_entry(name: &str, parent: &str, size: u64, is_dir: bool) -> FileEntry {
    FileEntry {
        size: if is_dir { None } else { Some(size) },
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "test".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            name.to_string(),
            format!("{}/{}", parent.trim_end_matches('/'), name),
            is_dir,
            false,
        )
    }
}

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

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

/// Test 1: `delete_files_start` with a fresh `preview_id` consumes the cached
/// scan and skips the rescan. `list_directory` is called once total (the
/// preview's listing — simulated here by seeding `SCAN_PREVIEW_RESULTS`
/// directly, the same shape `run_volume_scan_preview` produces); during the
/// delete itself, the call count stays at zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_consumes_preview_id_skips_rescan() {
    let vid = unique("consumes_preview");
    let preview_id = unique("preview_consumes");

    // Keep a typed Arc so we can read the counters after the trait call.
    let vol = Arc::new(CountingVolume::new("preview-vol", false));
    vol.inner.create_file(Path::new("/a.jpg"), b"alpha").await.unwrap();
    vol.inner.create_file(Path::new("/b.jpg"), b"betabb").await.unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Simulate a completed scan preview: per-path entries for two top-level
    // files. The real `start_scan_preview` path produces this same structure
    // (see `run_volume_scan_preview` → `CachedScanResult`).
    SCAN_PREVIEW_RESULTS.write().unwrap().insert(
        preview_id.clone(),
        CachedScanResult {
            files: Vec::new(),
            dirs: Vec::new(),
            file_count: 2,
            total_bytes: 11,
            per_path: vec![
                (
                    PathBuf::from("/a.jpg"),
                    CopyScanResult {
                        file_count: 1,
                        dir_count: 0,
                        total_bytes: 5,
                        top_level_is_directory: false,
                    },
                ),
                (
                    PathBuf::from("/b.jpg"),
                    CopyScanResult {
                        file_count: 1,
                        dir_count: 0,
                        total_bytes: 6,
                        top_level_is_directory: false,
                    },
                ),
            ],
        },
    );

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = WriteOperationConfig {
        preview_id: Some(preview_id.clone()),
        ..WriteOperationConfig::default()
    };

    let result = delete_volume_files_with_progress_inner(
        vol.clone() as Arc<dyn Volume>,
        &vid,
        events.as_ref(),
        "test-op-delete-reuse",
        &state,
        &[PathBuf::from("/a.jpg"), PathBuf::from("/b.jpg")],
        &config,
    )
    .await;

    assert!(result.is_ok(), "delete should succeed: {:?}", result);
    assert_eq!(
        vol.list_dir_count(),
        0,
        "cached preview path must NOT call list_directory during delete"
    );
    assert_eq!(
        vol.is_dir_count(),
        0,
        "cached preview path must NOT probe is_directory for top-level files"
    );
    assert_eq!(vol.delete_count(), 2, "both top-level files should be deleted");

    get_volume_manager().unregister(&vid);
}

/// Test 2: no `preview_id` (MCP path) still produces a correct delete. The
/// walker walks via the volume backend; we just confirm correctness end-to-end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_without_preview_id_still_walks() {
    let vid = unique("no_preview");

    let vol = Arc::new(CountingVolume::new("no-preview-vol", false));
    vol.inner.create_file(Path::new("/x.txt"), b"x").await.unwrap();
    vol.inner.create_file(Path::new("/y.txt"), b"yy").await.unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = WriteOperationConfig::default(); // preview_id: None

    let result = delete_volume_files_with_progress_inner(
        vol.clone() as Arc<dyn Volume>,
        &vid,
        events.as_ref(),
        "test-op-no-preview",
        &state,
        &[PathBuf::from("/x.txt"), PathBuf::from("/y.txt")],
        &config,
    )
    .await;

    assert!(result.is_ok(), "delete should succeed: {:?}", result);
    assert_eq!(vol.delete_count(), 2, "both top-level files should be deleted");
    // Watcher not flipped, oracle misses → fall through to `is_directory` per
    // source. Confirms the no-preview path keeps its top-level probe.
    assert_eq!(
        vol.is_dir_count(),
        2,
        "no-preview path must probe is_directory per top-level source on oracle miss"
    );

    get_volume_manager().unregister(&vid);
}

/// Test 3: no `preview_id`, but the parent listing is watcher-fresh in
/// `LISTING_CACHE`. The walker should consult the oracle for the top-level
/// `is_directory` decision and skip the trait probe entirely. Asserts the
/// `is_directory` call count stays at zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_top_level_files_no_is_directory_probes() {
    let vid = unique("oracle_top_level");
    let parent_lid = unique("oracle_top_level_lid");

    let vol = Arc::new(CountingVolume::new("oracle-top-level-vol", true));
    vol.inner.create_file(Path::new("/dcim/a.jpg"), b"alpha").await.unwrap();
    vol.inner
        .create_file(Path::new("/dcim/b.jpg"), b"betabb")
        .await
        .unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Pane has /dcim open with entries for a.jpg and b.jpg. Both are files.
    let cached = vec![
        make_file_entry("a.jpg", "/dcim", 5, false),
        make_file_entry("b.jpg", "/dcim", 6, false),
    ];
    let parent_lid_inserted = insert_listing(&parent_lid, &vid, "/dcim", cached);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = WriteOperationConfig::default(); // preview_id: None

    let result = delete_volume_files_with_progress_inner(
        vol.clone() as Arc<dyn Volume>,
        &vid,
        events.as_ref(),
        "test-op-oracle-top-level",
        &state,
        &[PathBuf::from("/dcim/a.jpg"), PathBuf::from("/dcim/b.jpg")],
        &config,
    )
    .await;

    assert!(result.is_ok(), "delete should succeed: {:?}", result);
    assert_eq!(
        vol.is_dir_count(),
        0,
        "watched parent listing must replace the per-source is_directory probe"
    );
    assert_eq!(
        vol.list_dir_count(),
        0,
        "watched parent listing must replace the list_directory call"
    );
    assert_eq!(vol.delete_count(), 2);

    remove_listing(&parent_lid_inserted);
    get_volume_manager().unregister(&vid);
}

/// Test 4: a subfolder of the delete source is open in pane B. We start the
/// delete with the source as a top-level directory, but pane B closes mid-walk
/// (we simulate this by removing the sub-listing while the watcher gate is on).
/// The walker must fall through to a real `list_directory` for the subfolder
/// and produce a correct delete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_mid_scan_listing_close() {
    let vid = unique("mid_scan_close");
    let parent_lid = unique("mid_scan_parent_lid");
    let sub_lid = unique("mid_scan_sub_lid");

    let vol = Arc::new(CountingVolume::new("mid-scan-vol", true));
    // Real backend content for the subfolder so the fallthrough produces a
    // sensible delete (one real file).
    vol.inner
        .create_file(Path::new("/root/sub/real.bin"), b"abcdef")
        .await
        .unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    // Pane A has /root open: one subfolder `sub`.
    let parent_cached = vec![make_file_entry("sub", "/root", 0, true)];
    let parent_lid_inserted = insert_listing(&parent_lid, &vid, "/root", parent_cached);

    // Pane B has /root/sub open, but we close it BEFORE the delete starts —
    // mirroring the "user closes pane mid-recursion" scenario.
    let sub_entries = vec![make_file_entry("phantom.bin", "/root/sub", 12345, false)];
    let sub_lid_inserted = insert_listing(&sub_lid, &vid, "/root/sub", sub_entries);
    remove_listing(&sub_lid_inserted);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = WriteOperationConfig::default();

    let result = delete_volume_files_with_progress_inner(
        vol.clone() as Arc<dyn Volume>,
        &vid,
        events.as_ref(),
        "test-op-mid-scan-close",
        &state,
        &[PathBuf::from("/root/sub")],
        &config,
    )
    .await;

    assert!(result.is_ok(), "delete should succeed: {:?}", result);
    // /root oracle-hit (top-level `is_directory` resolved without trait probe);
    // /root/sub falls through to list_directory because pane B closed before
    // recursion got there. So: zero is_directory probes, ≥1 list_directory.
    assert_eq!(
        vol.is_dir_count(),
        0,
        "parent oracle should have answered top-level is_directory"
    );
    assert!(
        vol.list_dir_count() >= 1,
        "fallthrough to list_directory required after pane B closed"
    );
    // Delete the real backend file + the now-empty `/root/sub` directory.
    assert!(
        vol.delete_count() >= 1,
        "at least the real file should be deleted; dir cleanup is best-effort"
    );

    remove_listing(&parent_lid_inserted);
    get_volume_manager().unregister(&vid);
}

/// Regression test: cancel landing during the scan phase of a volume delete
/// must emit `write-cancelled` before propagating `Err(Cancelled)` up. Prior
/// to the fix, `scan_volume_recursive`'s top-of-function cancel check returned
/// `Cancelled` without emitting, and the `?` propagation at the call site
/// passed it through silently — the FE never saw the terminal cancel event.
///
/// The pattern: pre-set `state.intent` to `Stopped` so the recursion's cancel
/// check fires on first entry. Direct `intent.store(...)` is acceptable here
/// because we're testing the emit behaviour on Cancelled return, not the
/// state-machine transition itself.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_cancel_during_scan_emits_write_cancelled() {
    let vid = unique("cancel_scan_emits");

    let vol = Arc::new(CountingVolume::new("cancel-scan-vol", false));
    vol.inner.create_file(Path::new("/dir/a.jpg"), b"alpha").await.unwrap();
    vol.inner.create_file(Path::new("/dir/b.jpg"), b"betabb").await.unwrap();
    get_volume_manager().register(&vid, vol.clone() as Arc<dyn Volume>);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Pre-cancel: the very first cancel check inside `scan_volume_recursive`
    // will fire and return `Cancelled`. Without the fix, no `write-cancelled`
    // event would be emitted before the `?` propagates the error.
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);

    let config = WriteOperationConfig::default(); // preview_id: None

    let result = delete_volume_files_with_progress_inner(
        vol.clone() as Arc<dyn Volume>,
        &vid,
        events.as_ref(),
        "test-op-cancel-scan-emits",
        &state,
        &[PathBuf::from("/dir")],
        &config,
    )
    .await;

    assert!(
        matches!(result, Err(WriteOperationError::Cancelled { .. })),
        "scan-time cancel must propagate as Cancelled: {:?}",
        result
    );
    let cancelled = events.cancelled.lock().unwrap();
    assert!(
        !cancelled.is_empty(),
        "write-cancelled must be emitted before Cancelled propagates from scan",
    );
    assert!(
        !cancelled.first().unwrap().rolled_back,
        "scan-time cancel is a Stopped (not RollingBack) outcome"
    );

    get_volume_manager().unregister(&vid);
}
