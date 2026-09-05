//! Test isolation for the process-global `LISTING_CACHE`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use super::caching::{CachedListing, LISTING_CACHE, epoch_millis_now};
use super::metadata::FileEntry;
use super::operations::list_directory_end;
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream, WatchCoverage,
};
use crate::ignore_poison::RwLockIgnorePoison;

/// A `LISTING_CACHE` entry owned by ONE test, torn down on drop.
///
/// **Why this exists.** `cargo test` runs a crate's tests as threads in ONE
/// process, so `LISTING_CACHE` is shared by every listing test at once. Three
/// things go wrong without a guard: two tests that pick the same literal listing
/// id clobber each other's entries; a test whose assertion fails before its
/// manual `cache.remove(...)` leaks the entry into every later test's view of the
/// cache; and any test that asserts on cache-WIDE state (`find_listings_for_path`
/// counts, an orphan sweep) sees foreign listings. A UNIQUE id, teardown from
/// `Drop` (which runs on unwind, so a panicking test cleans up too), and a
/// unique path per cache-wide assertion fix all three.
///
/// Mirrors `indexing::tests::stress_test_helpers::TestInstanceGuard`, the same
/// pattern over `INDEX_REGISTRY`. Keep the guard on the stack: a `std::mem::forget`
/// or a clone that outlives the test defeats the whole thing.
///
/// Teardown goes through the production `list_directory_end`, so the entry, its
/// watcher, and any pending coalesced diff are released together.
pub(crate) struct TestListingGuard {
    listing_id: String,
}

impl TestListingGuard {
    /// Takes ownership of a listing id that production code created (a
    /// `list_directory_start_*` call), so the test doesn't hand-roll teardown.
    pub(crate) fn adopt(listing_id: impl Into<String>) -> Self {
        Self {
            listing_id: listing_id.into(),
        }
    }

    /// The unique listing id. Pass it wherever a test would have used a literal.
    pub(crate) fn id(&self) -> &str {
        &self.listing_id
    }

    /// Runs `f` against this test's `CachedListing` under the cache read lock.
    /// Panics if the entry is gone, which is the assertion a test wants anyway.
    pub(crate) fn with_listing<R>(&self, f: impl FnOnce(&CachedListing) -> R) -> R {
        let cache = LISTING_CACHE.read_ignore_poison();
        let listing = cache
            .get(&self.listing_id)
            .unwrap_or_else(|| panic!("listing `{}` is no longer cached", self.listing_id));
        f(listing)
    }

    /// This listing's cached entries.
    pub(crate) fn entries(&self) -> Vec<FileEntry> {
        self.with_listing(|listing| listing.entries().to_vec())
    }

    /// This listing's cached entry names, in cache order. The common assertion.
    pub(crate) fn entry_names(&self) -> Vec<String> {
        self.with_listing(|listing| listing.entries().iter().map(|e| e.name.clone()).collect())
    }

    /// Whether the entry is still in the cache. For tests that assert on teardown.
    pub(crate) fn is_cached(&self) -> bool {
        LISTING_CACHE.read_ignore_poison().contains_key(&self.listing_id)
    }
}

impl Drop for TestListingGuard {
    fn drop(&mut self) {
        list_directory_end(&self.listing_id);
    }
}

/// Builder for a test-owned `LISTING_CACHE` entry. Defaults to an empty listing
/// on `root` at `/test`, sorted Name / Ascending / LikeFiles.
///
/// `last_accessed_ms` defaults to NOW, matching a listing that a live pane just
/// touched. A stamp of 0 would make the fixture orphan-eligible under any other
/// test's `reap_orphaned_listings_at` sweep, which is exactly the cross-test
/// eviction this module exists to stop; the reaper's own tests set it explicitly.
pub(crate) struct TestListing {
    volume_id: String,
    path: PathBuf,
    sort_by: SortColumn,
    sort_order: SortOrder,
    directory_sort_mode: DirectorySortMode,
    entries: Vec<FileEntry>,
    sequence: u64,
    last_accessed_ms: u64,
    overlay_rows: usize,
}

impl TestListing {
    pub(crate) fn new() -> Self {
        Self {
            volume_id: "root".to_string(),
            path: PathBuf::from("/test"),
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            entries: Vec::new(),
            sequence: 0,
            last_accessed_ms: epoch_millis_now(),
            overlay_rows: 0,
        }
    }

    pub(crate) fn volume(mut self, volume_id: &str) -> Self {
        self.volume_id = volume_id.to_string();
        self
    }

    pub(crate) fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = path.into();
        self
    }

    pub(crate) fn sort(mut self, by: SortColumn, order: SortOrder, mode: DirectorySortMode) -> Self {
        self.sort_by = by;
        self.sort_order = order;
        self.directory_sort_mode = mode;
        self
    }

    pub(crate) fn entries(mut self, entries: Vec<FileEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub(crate) fn sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    pub(crate) fn last_accessed_ms(mut self, last_accessed_ms: u64) -> Self {
        self.last_accessed_ms = last_accessed_ms;
        self
    }

    /// How many of the entries a listing overlay contributed. Nonzero makes this
    /// a PANE listing, which the fresh-listing oracle declines.
    pub(crate) fn overlay_rows(mut self, overlay_rows: usize) -> Self {
        self.overlay_rows = overlay_rows;
        self
    }

    /// Inserts the listing under a unique id derived from `tag` and hands back the
    /// RAII guard. Bind it (`let listing = …`), never `let _ = …`: a `_` binding
    /// drops immediately and the entry is gone before the test runs.
    pub(crate) fn insert(self, tag: &str) -> TestListingGuard {
        let listing_id = unique_test_id(tag);
        let listing = CachedListing::new(
            self.volume_id,
            self.path,
            self.entries,
            self.sort_by,
            self.sort_order,
            self.directory_sort_mode,
        )
        .with_overlay_rows(self.overlay_rows);
        listing.sequence.store(self.sequence, Ordering::Relaxed);
        listing.last_accessed_ms.store(self.last_accessed_ms, Ordering::Relaxed);
        LISTING_CACHE.write_ignore_poison().insert(listing_id.clone(), listing);
        TestListingGuard { listing_id }
    }
}

/// A process-unique key for a test-owned entry in any global map (a listing id, a
/// font id, an operation id). The counter alone would collide across
/// concurrently-running test binaries, so the pid goes in too.
pub(crate) fn unique_test_id(tag: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "test-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// A `Volume` whose `listing_watch_coverage` answer a test pins, delegating every
/// other method to an `InMemoryVolume`.
///
/// The fresh-listing oracle only answers on `EveryWriter`, and `InMemoryVolume`
/// reports `None` (the trait's default). Pinning the answer is what lets a test
/// drive all three states without an `AppHandle` or a real `WATCHER_MANAGER`
/// entry — including `ThisMachineOnly`, which has no other reachable fixture.
pub(crate) struct WatchCoverageVolume {
    inner: InMemoryVolume,
    /// The pinned answer, as the `WatchCoverage` discriminant so the field stays
    /// lock-free (`set_coverage` races with the oracle reading it).
    coverage: AtomicU8,
}

impl WatchCoverageVolume {
    pub(crate) fn new(name: &str, coverage: WatchCoverage) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            coverage: AtomicU8::new(encode_coverage(coverage)),
        }
    }

    pub(crate) fn set_coverage(&self, coverage: WatchCoverage) {
        self.coverage.store(encode_coverage(coverage), Ordering::Relaxed);
    }
}

fn encode_coverage(coverage: WatchCoverage) -> u8 {
    match coverage {
        WatchCoverage::None => 0,
        WatchCoverage::ThisMachineOnly => 1,
        WatchCoverage::EveryWriter => 2,
    }
}

fn decode_coverage(raw: u8) -> WatchCoverage {
    match raw {
        1 => WatchCoverage::ThisMachineOnly,
        2 => WatchCoverage::EveryWriter,
        _ => WatchCoverage::None,
    }
}

impl Volume for WatchCoverageVolume {
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

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        decode_coverage(self.coverage.load(Ordering::Relaxed))
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy_batch(paths)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_conflicts(source_items, dest_path)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
}

/// A `Volume` whose `list_directory` answers from a SCRIPT: each call pops the next
/// `(delay, entries)` pair, so a test can make an early read finish late.
///
/// This is what makes the full-refresh ordering hazard reproducible. Two refreshes
/// of one directory each do read-then-write-the-cache; if the read that started
/// FIRST also finishes LAST, it writes a snapshot the newer one has already
/// superseded, and the cache keeps the older truth until some later event happens
/// to correct it. Without a scripted delay the interleaving is timing-dependent and
/// a test for it would itself be a flake.
pub(crate) struct ScriptedListVolume {
    inner: InMemoryVolume,
    script: std::sync::Mutex<std::collections::VecDeque<(std::time::Duration, Vec<FileEntry>)>>,
}

impl ScriptedListVolume {
    pub(crate) fn new(name: &str, script: Vec<(std::time::Duration, Vec<FileEntry>)>) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            script: std::sync::Mutex::new(script.into_iter().collect()),
        }
    }

    /// Scripted reads not yet consumed. A test asserts on this to count how many
    /// times the directory was actually read.
    pub(crate) fn unread(&self) -> usize {
        self.script.lock().map(|s| s.len()).unwrap_or(0)
    }
}

impl Volume for ScriptedListVolume {
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
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let next = self.script.lock().ok().and_then(|mut s| s.pop_front());
        Box::pin(async move {
            match next {
                Some((delay, entries)) => {
                    // allowed-test-sleep: the scripted latency IS the subject — it is what makes the
                    // read that started first finish last, deterministically.
                    tokio::time::sleep(delay).await;
                    Ok(entries)
                }
                None => Ok(Vec::new()),
            }
        })
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

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy(path)
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_copy_batch(paths)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.inner.scan_for_conflicts(source_items, dest_path)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }
}
