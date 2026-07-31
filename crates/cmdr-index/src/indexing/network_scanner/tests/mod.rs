//! Scanner tests, split by theme. This module holds the shared fixtures every
//! theme uses: the tiny tree builders, the mock `Volume`s that fail on demand,
//! and the reconcile setup helpers. The themes themselves are the sibling
//! modules (`fresh_scan`, `disconnects`, `reconcile`, `nas_system_dirs`).

use std::sync::atomic::AtomicU64;

use std::future::Future;
use std::pin::Pin;

use super::*;
use crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET;
use crate::indexing::store::{ROOT_ID, resolve_path};
use crate::indexing::writer::IndexWriter;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, VolumeError};
use rusqlite::Connection;

mod disconnects;
mod fresh_scan;
mod nas_system_dirs;
mod reconcile;

pub(super) fn progress() -> Arc<ScanProgress> {
    // `ScanProgress::new` is private; build the public-fielded struct directly.
    Arc::new(ScanProgress {
        entries_scanned: Arc::new(AtomicU64::new(0)),
        dirs_found: Arc::new(AtomicU64::new(0)),
        bytes_scanned: Arc::new(AtomicU64::new(0)),
    })
}

pub(super) fn entry(name: &str, path: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        ..FileEntry::new(name.to_string(), path.to_string(), is_dir, false)
    }
}

/// A test `Volume` that delegates to an inner `InMemoryVolume` but returns a
/// TRANSIENT (`PermissionDenied`) error when listing one specific path. Lets
/// the scanner exercise the "a listing that errors is NOT marked, but the
/// walk continues" branch — a single transient/permission failure is
/// skip-and-continue, distinct from a typed `DeviceDisconnected` (terminal).
struct FailingListVolume {
    inner: InMemoryVolume,
    fail_path: PathBuf,
}

type ListFut<'a, T> = Pin<Box<dyn Future<Output = Result<T, VolumeError>> + Send + 'a>>;

impl Volume for FailingListVolume {
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
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> ListFut<'a, Vec<FileEntry>> {
        if path == self.fail_path {
            return Box::pin(async { Err(VolumeError::PermissionDenied("test: subdir listing failed".into())) });
        }
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
        self.inner.is_directory(path)
    }
}

/// A test `Volume` that counts `list_directory` calls and returns a
/// `DeviceDisconnected` error once the count reaches `fail_after_calls`. Lets
/// a test assert the walk STOPS at the disconnect (no further round trips
/// against a dead session) by reading the call counter back afterwards.
struct CountingDisconnectVolume {
    inner: InMemoryVolume,
    fail_after_calls: usize,
    /// Total `list_directory` attempts so far (incremented on every call).
    calls: Arc<AtomicU64>,
    /// When true, the failure is a plain `IoError` (a disconnect-SHAPED error
    /// that does NOT map to the typed `DeviceDisconnected`/`Disconnected`
    /// variant), to exercise the consecutive-failure backstop instead of the
    /// typed terminal branch. When false, it's `DeviceDisconnected` (typed).
    untyped_failure: bool,
}

impl Volume for CountingDisconnectVolume {
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
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> ListFut<'a, Vec<FileEntry>> {
        let n = (self.calls.fetch_add(1, Ordering::Relaxed) + 1) as usize;
        if n >= self.fail_after_calls {
            let untyped = self.untyped_failure;
            return Box::pin(async move {
                if untyped {
                    Err(VolumeError::IoError {
                        message: "test: connection reset".into(),
                        raw_os_error: None,
                    })
                } else {
                    Err(VolumeError::DeviceDisconnected("test: session dropped mid-walk".into()))
                }
            });
        }
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
        self.inner.is_directory(path)
    }
}

/// Build a wide tree: a root with `n_subdirs` empty subdirs. The BFS lists
/// the root first (call 1), then each subdir in turn (calls 2..=n_subdirs+1).
pub(super) fn wide_tree(n_subdirs: usize) -> InMemoryVolume {
    let mut entries = Vec::new();
    for i in 0..n_subdirs {
        entries.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
    }
    InMemoryVolume::with_entries("Test", entries)
}

/// A `Volume` wrapper that records the maximum number of `list_directory` calls in
/// flight at once. The `yield_now` lets sibling listings launched in the same
/// `FuturesUnordered` batch coexist before any resolves, so the recorded max
/// reflects real concurrency rather than instantly-ready mock timing.
pub(super) struct ConcurrencyTrackingVolume {
    pub(super) inner: InMemoryVolume,
    pub(super) in_flight: Arc<AtomicU64>,
    pub(super) max_in_flight: Arc<AtomicU64>,
}

impl Volume for ConcurrencyTrackingVolume {
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
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> ListFut<'a, Vec<FileEntry>> {
        Box::pin(async move {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            tokio::task::yield_now().await;
            let r = self.inner.list_directory(path, on_progress).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            r
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
        self.inner.is_directory(path)
    }
}

/// A `Volume` whose ROOT listing FAILS with a non-disconnect, non-typed
/// error (here `PermissionDenied`). Lets a test exercise the root-fatal
/// branch: the scanner must surface the error so the caller doesn't mark
/// completion over a never-built index.
struct RootFailsVolume {
    inner: InMemoryVolume,
}

impl Volume for RootFailsVolume {
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
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> ListFut<'a, Vec<FileEntry>> {
        if path == Path::new("/") {
            return Box::pin(async { Err(VolumeError::PermissionDenied("test: root listing denied".into())) });
        }
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> ListFut<'a, FileEntry> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> ListFut<'a, bool> {
        self.inner.is_directory(path)
    }
}

// ── Reconcile-rescan fixtures ────────────────────────────────────

fn entry_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .expect("count entries")
}

/// Recursive logical size of a dir by absolute path, from `dir_stats`.
fn dir_size(conn: &Connection, path: &str) -> u64 {
    let id = resolve_path(conn, path).expect("resolve").expect("present");
    IndexStore::get_dir_stats_by_id(conn, id)
        .expect("stats")
        .map(|s| s.recursive_logical_size)
        .unwrap_or(0)
}

fn min_epoch(conn: &Connection, path: &str) -> u64 {
    let id = resolve_path(conn, path).expect("resolve").expect("present");
    IndexStore::get_dir_stats_by_id(conn, id)
        .expect("stats")
        .map(|s| s.min_subtree_epoch)
        .unwrap_or(0)
}

/// Build a writer + DB pre-populated to an "already fully scanned" state by
/// running a fresh `scan_volume_via_trait` over `vol`. Returns (writer, db_path,
/// tempdir). Epoch is seeded to 1 by the fresh scan.
async fn fresh_scan(vol: Arc<dyn Volume>) -> (IndexWriter, PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("reconcile.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    let cancelled = CancellationToken::new();
    scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("fresh scan");
    writer.flush().await.expect("flush");
    (writer, db_path, dir)
}

/// A small known tree:
///   /sub/         (dir)
///   /sub/keep.txt (4 bytes)
///   /sub/mod.txt  (4 bytes)
///   /top.txt      (5 bytes)
fn base_tree() -> Vec<FileEntry> {
    vec![
        entry("sub", "/sub", true, None),
        entry("keep.txt", "/sub/keep.txt", false, Some(4)),
        entry("mod.txt", "/sub/mod.txt", false, Some(4)),
        entry("top.txt", "/top.txt", false, Some(5)),
    ]
}
