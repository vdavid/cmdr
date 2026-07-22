use std::sync::atomic::AtomicU64;

use std::future::Future;
use std::pin::Pin;

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, VolumeError};
use crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET;
use crate::indexing::store::{ROOT_ID, resolve_path};

pub(super) fn progress() -> Arc<ScanProgress> {
    // `ScanProgress::new` is private; build the public-fielded struct directly.
    Arc::new(ScanProgress {
        entries_scanned: Arc::new(AtomicU64::new(0)),
        dirs_found: Arc::new(AtomicU64::new(0)),
        bytes_scanned: Arc::new(AtomicU64::new(0)),
    })
}

fn entry(name: &str, path: &str, is_dir: bool, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        ..FileEntry::new(name.to_string(), path.to_string(), is_dir, false)
    }
}

/// Walk a small in-memory tree over the `Volume` trait and assert the index
/// reflects its contents: the writer/aggregator reuse is exercised end to
/// end (entries land under ROOT_ID, sizes flow into dir_stats). This is the
/// backend-agnostic half of the SMB-fixture integration test; the live SMB
/// scan is pinned by `smb_integration_volume_scan_indexes_share` (Docker).
#[tokio::test]
async fn scans_in_memory_tree_into_index() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // Build an in-memory volume with a known tree:
    //   /sub/         (dir)
    //   /sub/leaf.txt (11 bytes)
    //   /top.txt      (5 bytes)
    let vol = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("sub", "/sub", true, None),
            entry("leaf.txt", "/sub/leaf.txt", false, Some(11)),
            entry("top.txt", "/top.txt", false, Some(5)),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete");

    assert!(!summary.was_cancelled);
    assert_eq!(summary.total_entries, 3, "2 files + 1 dir");
    assert_eq!(summary.total_dirs, 1);

    // Async test: await the flush rather than `flush_blocking` (which would
    // `block_on` the current runtime thread and panic).
    writer.flush().await.expect("flush");
    writer.shutdown();

    let store = IndexStore::open(&db_path).expect("reopen");
    let children = store.list_children(ROOT_ID).expect("list root");
    assert_eq!(children.len(), 2, "root has sub/ and top.txt");
    let sub = children.iter().find(|e| e.name == "sub").expect("sub dir present");
    assert!(sub.is_directory);
    let sub_children = store.list_children(sub.id).expect("list sub");
    assert_eq!(sub_children.len(), 1);
    assert_eq!(sub_children[0].name, "leaf.txt");
    assert_eq!(sub_children[0].logical_size, Some(11));
}

/// The recursive size scan must NOT descend into NAS snapshot/system dirs
/// (`@eaDir`, `@Recently-Snapshot`, …): they're hardlinked/huge and recursively
/// sizing them stalled a real first-scan (`@Recently-Snapshot` alone reported 44 TB
/// on a 10 TB volume). The dir's OWN row stays indexed (listed + navigable), but its
/// subtree is never walked — at the share root AND nested inside a normal dir.
#[tokio::test]
async fn skips_recursion_into_nas_system_dirs() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-skip.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let vol = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("photos", "/photos", true, None),
            // Synology thumbnail sidecar nested inside a normal dir → skip recursion.
            entry("@eaDir", "/photos/@eaDir", true, None),
            entry("thumb.jpg", "/photos/@eaDir/thumb.jpg", false, Some(999)),
            // Snapshot root at the share root → skip recursion.
            entry("@Recently-Snapshot", "/@Recently-Snapshot", true, None),
            entry(
                "full-copy.bin",
                "/@Recently-Snapshot/full-copy.bin",
                false,
                Some(44_000),
            ),
            entry("keep.txt", "/keep.txt", false, Some(5)),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete");

    writer.flush().await.expect("flush");
    writer.shutdown();

    let store = IndexStore::open(&db_path).expect("reopen");

    // The system dirs themselves ARE indexed (visible + navigable).
    let root_children = store.list_children(ROOT_ID).expect("list root");
    let snap = root_children
        .iter()
        .find(|e| e.name == "@Recently-Snapshot")
        .expect("@Recently-Snapshot row present (visible, navigable)");
    let photos = root_children
        .iter()
        .find(|e| e.name == "photos")
        .expect("photos present");

    // …but their subtrees are NOT walked.
    assert_eq!(
        store.list_children(snap.id).expect("list snapshot").len(),
        0,
        "snapshot subtree must not be indexed (no recursive descent)",
    );
    let photos_children = store.list_children(photos.id).expect("list photos");
    let eadir = photos_children
        .iter()
        .find(|e| e.name == "@eaDir")
        .expect("@eaDir row present under photos");
    assert_eq!(
        store.list_children(eadir.id).expect("list eaDir").len(),
        0,
        "@eaDir subtree must not be indexed even nested under a normal dir",
    );
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

/// A subdir whose listing errors is NOT stamped (`listed_epoch` stays 0),
/// while its successfully-listed siblings (including an empty-but-listed dir)
/// and the root get the current epoch. The unit-level disconnect anchor.
#[tokio::test]
async fn errored_listing_is_not_marked() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-mark.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // Tree:
    //   /good/        (dir, lists fine, has one file)
    //   /good/a.txt
    //   /empty/       (dir, lists fine but empty → empty-but-listed)
    //   /bad/         (dir, listing ERRORS transiently → must stay listed_epoch=0)
    //   /bad/hidden   (file under bad; never discovered because bad won't list)
    let inner = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("good", "/good", true, None),
            entry("a.txt", "/good/a.txt", false, Some(7)),
            entry("empty", "/empty", true, None),
            entry("bad", "/bad", true, None),
            entry("hidden", "/bad/hidden", false, Some(3)),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(FailingListVolume {
        inner,
        fail_path: PathBuf::from("/bad"),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete (a single bad subdir is skipped)");
    assert!(!summary.was_cancelled);

    writer.flush().await.expect("flush");
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let epoch = IndexStore::read_current_epoch(&conn).expect("epoch");
    assert_eq!(epoch, 1, "first scan stamps epoch 1");

    let id_of = |p: &str| -> i64 { resolve_path(&conn, p).expect("resolve").expect("present") };

    // Root and the dirs that listed successfully (incl. empty) are stamped.
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, ROOT_ID).expect("root epoch"),
        Some(1),
        "root listed",
    );
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, id_of("/good")).expect("good epoch"),
        Some(1),
        "good listed",
    );
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, id_of("/empty")).expect("empty epoch"),
        Some(1),
        "empty-but-listed dir is stamped",
    );

    // The errored subdir's row exists (parent listed it) but stays unlisted.
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, id_of("/bad")).expect("bad epoch"),
        Some(0),
        "a dir whose own listing errored stays listed_epoch=0 (honest unknown)",
    );
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

/// THE regression test for the reported prod bug. A volume disconnects after
/// listing K of N dirs: the walk must STOP promptly (not churn the remaining
/// N−K queued dirs into empty rows), return the typed `DeviceDisconnected`
/// error, and — crucially — the caller must write NO `scan_completed_at`
/// (asserted at the manager level; here we assert the typed error + prompt
/// stop, which is what the completion handler routes on).
#[tokio::test]
async fn disconnect_mid_walk_stops_promptly_and_returns_typed_error() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-disconnect.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // Root + 200 empty subdirs (≫ FULL_LISTING_BUDGET). BFS: list root (call 1)
    // discovers 200 dirs, then lists them concurrently (up to FULL_LISTING_BUDGET in
    // flight). The 4th list call returns a typed disconnect. The walk must stop
    // topping up and drop the in-flight listings rather than churning all 200.
    let n_subdirs = 200;
    let fail_after_calls = 4;
    let calls = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
        inner: wide_tree(n_subdirs),
        fail_after_calls,
        calls: Arc::clone(&calls),
        untyped_failure: false,
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    // The typed terminal error, NOT a clean Ok (which is today's bug: a clean
    // finish over silently-empty rows). Matched by the TYPED variant.
    match result {
        Err(VolumeScanError::Volume(VolumeError::DeviceDisconnected(_))) => {}
        other => panic!("expected typed DeviceDisconnected terminal error, got {other:?}"),
    }

    // Prompt stop: the walk bailed within ~one concurrency window of the disconnect
    // and did NOT churn the remaining queued dirs. With concurrency the count is no
    // longer exactly `fail_after_calls` (up to FULL_LISTING_BUDGET listings were already
    // in flight), but it's bounded well below the full `n_subdirs`.
    let made = calls.load(Ordering::Relaxed) as usize;
    assert!(
        made < n_subdirs,
        "walk must stop at the disconnect, not churn all {n_subdirs} queued dirs (made {made})",
    );
    assert!(
        made <= 1 + FULL_LISTING_BUDGET + fail_after_calls,
        "walk must stop within ~one concurrency window of the disconnect (made {made})",
    );

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// The consecutive-failure backstop: a disconnect-shaped error that does NOT
/// map to the typed variant (here `IoError`) must still abort the walk after
/// `CONSECUTIVE_FAILURE_ABORT` consecutive failures, rather than churning
/// every queued dir into an empty row.
#[tokio::test]
async fn consecutive_untyped_failures_trip_the_backstop() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-backstop.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // Enough subdirs that the backstop (N consecutive) trips well before the
    // queue drains, even with up to FULL_LISTING_BUDGET listings in flight. Root lists
    // fine (call 1), then every subdir listing fails with an untyped IoError.
    let n_subdirs = CONSECUTIVE_FAILURE_ABORT * 6;
    let calls = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
        inner: wide_tree(n_subdirs),
        fail_after_calls: 2, // root ok, then every child fails
        calls: Arc::clone(&calls),
        untyped_failure: true,
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::ConsecutiveFailures { count, .. }) => {
            assert_eq!(count, CONSECUTIVE_FAILURE_ABORT, "aborts at exactly the threshold");
        }
        other => panic!("expected ConsecutiveFailures backstop abort, got {other:?}"),
    }

    // Bounded stop: the backstop aborts after ~root + one concurrency window +
    // N failures (concurrency means some listings were already in flight), and the
    // remaining dirs were never attempted — well short of the full queue.
    let made = calls.load(Ordering::Relaxed) as usize;
    assert!(
        made < n_subdirs,
        "backstop must stop well short of churning the whole {n_subdirs}-dir queue (made {made})",
    );
    assert!(
        made <= 1 + FULL_LISTING_BUDGET + CONSECUTIVE_FAILURE_ABORT,
        "backstop stops within ~one concurrency window of the threshold (made {made})",
    );

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// A single transient failure followed by successes does NOT trip the
/// backstop: the consecutive counter resets on every success, so an isolated
/// bad dir is still skip-and-continue (the existing behavior we keep).
#[tokio::test]
async fn isolated_transient_failure_does_not_trip_backstop() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-transient.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // One subdir fails (untyped), the rest list fine. The scan completes
    // cleanly (the bad dir is skipped, stays listed_epoch=0).
    let inner = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("good", "/good", true, None),
            entry("a.txt", "/good/a.txt", false, Some(7)),
            entry("bad", "/bad", true, None),
            entry("alsogood", "/alsogood", true, None),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(FailingListVolume {
        inner,
        fail_path: PathBuf::from("/bad"),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("an isolated transient failure is skipped, scan completes");
    assert!(!summary.was_cancelled);

    writer.flush().await.expect("flush");
    writer.shutdown();
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

/// THE speedup regression guard: the walk lists directories CONCURRENTLY, capped at
/// `FULL_LISTING_BUDGET`. With many sibling dirs queued, multiple `list_directory` round
/// trips are in flight at once — a revert to a serial walk would record a max of 1.
#[tokio::test]
async fn walk_lists_directories_concurrently() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-concurrency.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // Root with many empty subdirs (≫ FULL_LISTING_BUDGET): the root listing discovers
    // them all, then they list concurrently up to the cap.
    let in_flight = Arc::new(AtomicU64::new(0));
    let max_in_flight = Arc::new(AtomicU64::new(0));
    let vol: Arc<dyn Volume> = Arc::new(ConcurrencyTrackingVolume {
        inner: wide_tree(FULL_LISTING_BUDGET * 2),
        in_flight: Arc::clone(&in_flight),
        max_in_flight: Arc::clone(&max_in_flight),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan completes");
    writer.flush().await.expect("flush");
    writer.shutdown();

    let max = max_in_flight.load(Ordering::SeqCst) as usize;
    assert!(
        max > 1,
        "the walk must list concurrently, not serially (max in flight = {max})"
    );
    assert!(
        max <= FULL_LISTING_BUDGET,
        "concurrency must stay capped at FULL_LISTING_BUDGET (max in flight = {max})",
    );
}

/// `is_terminal_disconnect` routes the completion handler: true for a typed
/// `DeviceDisconnected` and the consecutive-failure backstop (keep honest
/// partial + Stale), false for a timeout / context / writer-send (discard).
#[test]
fn terminal_disconnect_classification() {
    assert!(
        VolumeScanError::Volume(VolumeError::DeviceDisconnected("x".into())).is_terminal_disconnect(),
        "typed DeviceDisconnected is a terminal disconnect"
    );
    assert!(
        VolumeScanError::ConsecutiveFailures {
            count: CONSECUTIVE_FAILURE_ABORT,
            last: "io".into()
        }
        .is_terminal_disconnect(),
        "the consecutive-failure backstop is a terminal disconnect"
    );
    // Non-disconnect terminations are NOT kept as honest partials.
    assert!(
        !VolumeScanError::Timeout(PathBuf::from("/wedged")).is_terminal_disconnect(),
        "a timeout is discarded, not kept"
    );
    assert!(
        !VolumeScanError::Volume(VolumeError::PermissionDenied("root".into())).is_terminal_disconnect(),
        "a non-disconnect volume error (root-fatal) is discarded"
    );
    assert!(!VolumeScanError::WriterSend("gone".into()).is_terminal_disconnect());
    assert!(!VolumeScanError::Context("ctx".into()).is_terminal_disconnect());
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

/// A fresh scan whose ROOT listing SUCCEEDS but returns ZERO children must
/// NOT report a clean completion: it returns the typed `EmptyRoot` error so
/// the completion handler leaves `scan_completed_at` unwritten. This is the
/// guard against the real-hardware bug where a NAS scan that walked nothing
/// stamped a false "complete" marker and stranded the index forever. (The
/// completion handler's persistence of the marker is asserted at the manager
/// level; here we pin the typed error the handler routes on.)
#[tokio::test]
async fn empty_root_fresh_scan_does_not_complete() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-empty-root.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    // Root lists fine but has no children at all.
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", vec![]));

    let cancelled = Arc::new(AtomicBool::new(false));
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::EmptyRoot) => {}
        other => panic!("expected EmptyRoot (no completion), got {other:?}"),
    }
    // EmptyRoot is NOT a terminal disconnect: the completion handler discards
    // and resets to gray rather than keeping a "stale" empty partial.
    assert!(
        !VolumeScanError::EmptyRoot.is_terminal_disconnect(),
        "an empty root is a failed scan to discard, not an honest partial to keep",
    );

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// The root-fatal case stays fatal: a ROOT listing that ERRORS (not empty,
/// not a disconnect) surfaces the error so no completion marker is written.
/// Distinguishes "root listing FAILED" (`Volume`) from "root listed EMPTY"
/// (`EmptyRoot`) — both refuse completion, via different typed variants.
#[tokio::test]
async fn failed_root_listing_does_not_complete() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-root-fail.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let vol: Arc<dyn Volume> = Arc::new(RootFailsVolume {
        inner: InMemoryVolume::with_entries("Test", vec![entry("a.txt", "/a.txt", false, Some(1))]),
    });

    let cancelled = Arc::new(AtomicBool::new(false));
    let result = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::Volume(VolumeError::PermissionDenied(_))) => {}
        other => panic!("expected the root-fatal Volume error (no completion), got {other:?}"),
    }

    writer.flush().await.expect("flush");
    writer.shutdown();
}

/// A pre-set cancel flag stops the walk immediately and reports
/// `was_cancelled` (the caller then discards the partial — D-interrupted).
#[tokio::test]
async fn honors_cancellation_before_first_listing() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-cancel.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");

    let vol = InMemoryVolume::with_entries("Test", vec![entry("a.txt", "/a.txt", false, Some(1))]);
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let cancelled = Arc::new(AtomicBool::new(true));
    let summary = scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("cancelled scan still returns Ok");
    assert!(summary.was_cancelled);
    assert_eq!(summary.total_entries, 0, "nothing scanned after immediate cancel");

    writer.shutdown();
}

// ── Non-destructive reconcile rescan (network path) ────────

use crate::indexing::writer::IndexWriter;
use rusqlite::Connection;

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
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    let cancelled = Arc::new(AtomicBool::new(false));
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

/// A reconcile rescan over an UNCHANGED tree writes ZERO entry rows (the
/// no-op-cheap property the perf bench relied on): unchanged rows are diffed and
/// skipped, never re-UPSERTed, so the catastrophic INSERT OR REPLACE path is
/// never touched. Coverage still re-stamps to the new epoch.
#[tokio::test]
async fn reconcile_noop_writes_zero_entry_rows() {
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let rows_before = entry_count(&conn);
    let max_id_before: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |r| r.get(0))
        .unwrap();

    // A continuity break would bump the epoch before a rescan; mirror that.
    let new_epoch = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    };

    // Reconcile the SAME tree (nothing changed on disk).
    let cancelled = Arc::new(AtomicBool::new(false));
    reconcile_volume_via_trait(
        Arc::clone(&vol),
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        entry_count(&conn),
        rows_before,
        "no-op reconcile must not change the entry row count"
    );
    let max_id_after: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        max_id_after, max_id_before,
        "no-op reconcile must not allocate any new ids (zero rows written)"
    );
    // Coverage re-stamped to the new epoch (the single aggregate ran).
    assert_eq!(
        min_epoch(&conn, "/sub"),
        new_epoch,
        "no-op reconcile re-stamps coverage to the new epoch"
    );

    writer.shutdown();
}

/// A reconcile rescan with changes (add / remove / modify) refreshes sizes
/// correctly AND ends byte-identical (entry set + dir sizes) to a
/// fresh-from-scratch scan of the SAME final tree. The 1.83 TB-ghost guard.
#[tokio::test]
async fn reconcile_with_changes_matches_fresh_from_scratch() {
    let vol_before: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_before)).await;

    // Final tree: remove keep.txt, modify mod.txt (4→20 bytes), add new.txt,
    // add a new subdir with a file.
    let final_tree = vec![
        entry("sub", "/sub", true, None),
        entry("mod.txt", "/sub/mod.txt", false, Some(20)),
        entry("new.txt", "/sub/new.txt", false, Some(7)),
        entry("deep", "/sub/deep", true, None),
        entry("d.txt", "/sub/deep/d.txt", false, Some(3)),
        entry("top.txt", "/top.txt", false, Some(5)),
    ];
    let vol_after: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", final_tree.clone()));

    // Bump epoch (continuity break) then reconcile to the final tree.
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    reconcile_volume_via_trait(
        Arc::clone(&vol_after),
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    // Fresh-from-scratch oracle: scan the final tree into a clean DB.
    let vol_oracle: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", final_tree));
    let (oracle_writer, oracle_db, _odir) = fresh_scan(vol_oracle).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let oconn = IndexStore::open_read_connection(&oracle_db).expect("oracle read conn");

    // keep.txt gone; new.txt + deep/ present.
    assert!(
        resolve_path(&conn, "/sub/keep.txt").unwrap().is_none(),
        "removed file gone"
    );
    assert!(
        resolve_path(&conn, "/sub/new.txt").unwrap().is_some(),
        "added file present"
    );
    assert!(
        resolve_path(&conn, "/sub/deep/d.txt").unwrap().is_some(),
        "new subtree present"
    );

    // Same recursive sizes as a fresh build (no ghosts).
    assert_eq!(
        dir_size(&conn, "/sub"),
        dir_size(&oconn, "/sub"),
        "/sub size matches fresh"
    );
    assert_eq!(dir_size(&conn, "/"), dir_size(&oconn, "/"), "root size matches fresh");
    // mod.txt's new size is reflected: /sub = mod(20) + new(7) + deep/d(3) = 30.
    assert_eq!(dir_size(&conn, "/sub"), 30, "reconciled /sub reflects modify + adds");

    writer.shutdown();
    oracle_writer.shutdown();
}

/// A mid-rescan DISCONNECT leaves the PRIOR complete index intact (now possible
/// — no truncate ran) and surfaces the typed terminal error. The re-listed dirs
/// are stamped at the rescan epoch; unreached dirs keep their prior data. The
/// completion handler (manager) then bumps past the epoch so everything reads
/// stale — here we assert the prior data SURVIVES (the headline reconcile property).
#[tokio::test]
async fn mid_reconcile_disconnect_keeps_prior_index() {
    // Wide tree so the disconnect leaves real dirs unreached.
    let mut before = vec![entry("top.txt", "/top.txt", false, Some(5))];
    for i in 0..20 {
        before.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
        before.push(entry("f.txt", &format!("/d{i}/f.txt"), false, Some(10)));
    }
    let vol_before: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", before));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_before)).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let rows_before = entry_count(&conn);
    assert!(rows_before > 20, "prior complete index has all dirs");
    let root_size_before = dir_size(&conn, "/");

    // A disconnecting volume: lists the root + a couple dirs, then drops.
    let calls = Arc::new(AtomicU64::new(0));
    let mut after = vec![entry("top.txt", "/top.txt", false, Some(5))];
    for i in 0..20 {
        after.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
        after.push(entry("f.txt", &format!("/d{i}/f.txt"), false, Some(10)));
    }
    let vol_disc: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
        inner: InMemoryVolume::with_entries("Test", after),
        fail_after_calls: 4, // root + a few dirs, then disconnect
        calls: Arc::clone(&calls),
        untyped_failure: false,
    });

    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let result = reconcile_volume_via_trait(
        vol_disc,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::Volume(VolumeError::DeviceDisconnected(_))) => {}
        other => panic!("expected typed terminal disconnect, got {other:?}"),
    }
    writer.flush().await.expect("flush");

    // The prior index is INTACT: no truncate ran, all rows still present, sizes
    // unchanged (the unreached dirs were never re-listed, so their data stands).
    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        entry_count(&conn),
        rows_before,
        "mid-rescan disconnect must not lose any prior rows (no truncate)"
    );
    assert_eq!(
        dir_size(&conn, "/"),
        root_size_before,
        "prior root size survives a mid-rescan disconnect"
    );

    writer.shutdown();
}

/// First scan (empty DB) is a fresh truncate+build, NOT a reconcile: the manager
/// chooses by entry-count, but at this layer we confirm `scan_volume_via_trait`
/// builds correctly from empty (the precondition the reconcile path relies on:
/// a populated DB). This pins that the two entry points produce the same index.
#[tokio::test]
async fn first_scan_builds_then_reconcile_is_a_no_op() {
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let built = entry_count(&conn);
    // 4 tree entries (sub, keep.txt, mod.txt, top.txt) + the ROOT_ID sentinel.
    assert_eq!(built, 5, "first scan built all 4 entries plus the root sentinel");

    // Immediately reconciling the same tree is a no-op (zero new rows).
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    reconcile_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(entry_count(&conn), built, "reconcile after first scan adds no rows");

    writer.shutdown();
}

/// Count entries stamped at exactly `epoch` (the dirs this reconcile pass
/// successfully re-listed). A reconcile that descends the whole tree stamps
/// every dir; one that stops at the root stamps only the root.
fn dirs_listed_at_epoch(conn: &Connection, epoch: u64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE is_directory = 1 AND listed_epoch = ?1",
        [epoch],
        |r| r.get(0),
    )
    .expect("count listed dirs")
}

/// THE regression test for the reported prod bug: a reconcile over an
/// already-partially-indexed share must DESCEND into every existing child
/// dir, not stop at the root after matching its children by name.
///
/// Setup mirrors prod (`naspi`): the DB knows the root + its top-level dirs
/// from an earlier interrupted scan, but those dirs are EMPTY in the index —
/// their real subtrees were never listed. The live volume has the full tree.
/// A child dir being "unchanged" at the root's level (same mtime → no UPSERT)
/// says NOTHING about whether its own subtree was ever scanned, so the
/// reconcile must recurse into it regardless.
///
/// Pre-fix (recursion gated on a change/upsert) this stamped only the root
/// and left every deep file missing — a green badge over an unscanned share.
#[tokio::test]
async fn reconcile_descends_into_existing_unchanged_child_dirs() {
    // Prior index: root + 3 top-level dirs, each EMPTY (the interrupted-scan
    // state). A fresh scan stamps these at epoch 1 with stable mtimes.
    let shallow = vec![
        entry("a", "/a", true, None),
        entry("b", "/b", true, None),
        entry("c", "/c", true, None),
    ];
    let vol_prior: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", shallow));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_prior)).await;

    // The full live tree: the SAME 3 top dirs (unchanged → no UPSERT at the
    // root), now each holding a subdir with a deep file. 3 top dirs + 3
    // subdirs = 6 dirs total under the root, plus the root itself = 7 dirs.
    let full = vec![
        entry("a", "/a", true, None),
        entry("sub_a", "/a/sub_a", true, None),
        entry("deep_a.txt", "/a/sub_a/deep_a.txt", false, Some(11)),
        entry("b", "/b", true, None),
        entry("sub_b", "/b/sub_b", true, None),
        entry("deep_b.txt", "/b/sub_b/deep_b.txt", false, Some(22)),
        entry("c", "/c", true, None),
        entry("sub_c", "/c/sub_c", true, None),
        entry("deep_c.txt", "/c/sub_c/deep_c.txt", false, Some(33)),
    ];
    let vol_full: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", full));

    // A continuity break bumps the epoch before a rescan; mirror that so the
    // reconcile stamps re-listed dirs at the NEW epoch (distinct from epoch 1).
    let new_epoch = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    };

    let cancelled = Arc::new(AtomicBool::new(false));
    reconcile_volume_via_trait(
        vol_full,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");

    // The walk descended into EVERY dir: root + 3 top + 3 sub = 7 dirs, all
    // stamped at the new epoch. Pre-fix only the root (1) was stamped.
    assert_eq!(
        dirs_listed_at_epoch(&conn, new_epoch),
        7,
        "reconcile must re-list every dir (root + 3 top + 3 sub), not stop at the root"
    );

    // The deep files the prior index never had are now present and sized —
    // proof the recursion actually listed the subtrees, not just stamped them.
    for (path, size) in [
        ("/a/sub_a/deep_a.txt", 11u64),
        ("/b/sub_b/deep_b.txt", 22),
        ("/c/sub_c/deep_c.txt", 33),
    ] {
        let id = resolve_path(&conn, path)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{path} should be indexed after reconcile descends"));
        let row = IndexStore::get_entry_by_id(&conn, id).expect("entry").expect("present");
        assert_eq!(row.logical_size, Some(size), "{path} reconciled with its real size");
    }

    // Recursive sizes rolled up through the descended tree: root = 11+22+33.
    assert_eq!(
        dir_size(&conn, "/"),
        66,
        "root recursive size reflects the deep files the reconcile descended to find"
    );

    writer.shutdown();
}

/// A reconcile rescan whose ROOT suddenly lists EMPTY (the share glitched or
/// the session is half-dead) must NOT report a clean completion: it returns
/// the typed `EmptyRoot` error so the prior (stale-but-real) index is kept
/// and never overwritten as falsely-complete-and-empty. Without this guard a
/// transient empty root strands the index as "complete" with zero entries.
#[tokio::test]
async fn reconcile_empty_root_does_not_complete() {
    // Start from a real, fully-scanned tree so the reconcile path runs over a
    // populated index.
    let populated: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&populated)).await;

    let rows_before = {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        entry_count(&conn)
    };
    assert!(rows_before > 0, "precondition: the index has data to reconcile against");

    // A continuity break bumps the epoch before a rescan; mirror that.
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }

    // Now reconcile against a volume whose root lists EMPTY (the glitch).
    let empty: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", vec![]));
    let cancelled = Arc::new(AtomicBool::new(false));
    let result = reconcile_volume_via_trait(
        empty,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::EmptyRoot) => {}
        other => panic!("expected EmptyRoot from a reconcile whose root went empty, got {other:?}"),
    }
    writer.flush().await.expect("flush");

    // The prior index is untouched — reconcile wrote no changes and we bailed
    // before the diff/removal/marks, so the stale-but-real rows survive.
    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        entry_count(&conn),
        rows_before,
        "a glitched empty-root reconcile must not blank the prior index",
    );

    writer.shutdown();
}

/// THE regression test for the post-Forget SMB enable bug: a reconcile over an
/// EMPTY DB whose scan root is NOT `/` (the real case — an SMB share mounts at
/// `/Volumes/<share>`) must still DESCEND into every newly-discovered child
/// dir, fully indexing the multi-level tree.
///
/// The enable path routes a no-completion-marker DB through the reconcile walk;
/// post-Forget that DB is empty, so EVERY dir is "new". New dirs are resolved
/// after a flush to get their freshly-assigned ids before recursing. Resolving
/// by ABSOLUTE PATH (`/Volumes/naspi/_test`) walks component-by-component from
/// ROOT_ID, but the index root IS `/Volumes/naspi` (mapped to ROOT_ID) — so the
/// walk fails at the first component (`Volumes`) and resolves NOTHING. The
/// reconcile then stops at the root and falsely "completes" with only the
/// top-level entries (badge green, no real scan). Resolving by `(parent_id,
/// name)` is correct for any root. Pre-fix this assertion fails: only the root
/// and its immediate children are indexed, the subtrees are missing.
#[tokio::test]
async fn reconcile_from_empty_db_with_non_root_mount_indexes_full_tree() {
    // An SMB-shaped mount: root is `/Volumes/naspi`, with a multi-level tree.
    let root = PathBuf::from("/Volumes/naspi");
    let tree = vec![
        entry("top", "/Volumes/naspi/top", true, None),
        entry("sub", "/Volumes/naspi/top/sub", true, None),
        entry("deep.txt", "/Volumes/naspi/top/sub/deep.txt", false, Some(42)),
        entry("other", "/Volumes/naspi/other", true, None),
        entry("leaf.txt", "/Volumes/naspi/other/leaf.txt", false, Some(7)),
    ];
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("naspi", tree));

    // Empty DB + writer (the post-Forget state). The manager bumps the epoch at
    // the scan-start funnel before spawning the walk; mirror that so listed dirs
    // stamp the bumped epoch.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("reconcile-empty.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
    let new_epoch = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    };

    let cancelled = Arc::new(AtomicBool::new(false));
    let summary = reconcile_volume_via_trait(vol, root, writer.clone(), progress(), cancelled, ScanPacer::unpaced())
        .await
        .expect("reconcile from empty DB on a non-`/` mount");
    assert!(!summary.was_cancelled);
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");

    // The walk descended into EVERY dir: root + top + top/sub + other = 4 dirs,
    // all stamped at the new epoch. Pre-fix only the root (1) was stamped.
    assert_eq!(
        dirs_listed_at_epoch(&conn, new_epoch),
        4,
        "reconcile must re-list every dir (root + top + top/sub + other), not stop at the root"
    );

    // The deep files prove recursion actually listed the subtrees rather than
    // just stamping the top level. Resolved by (parent_id, name) chains since
    // `resolve_path` from `/` can't reach a `/Volumes/naspi`-rooted index.
    let id_of = |parent: i64, name: &str| -> i64 {
        IndexStore::resolve_component(&conn, parent, name)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{name} should be indexed after reconcile descends"))
    };
    let top = id_of(ROOT_ID, "top");
    let sub = id_of(top, "sub");
    let deep = id_of(sub, "deep.txt");
    let deep_row = IndexStore::get_entry_by_id(&conn, deep)
        .expect("entry")
        .expect("present");
    assert_eq!(
        deep_row.logical_size,
        Some(42),
        "deep.txt reconciled with its real size"
    );

    let other = id_of(ROOT_ID, "other");
    let leaf = id_of(other, "leaf.txt");
    let leaf_row = IndexStore::get_entry_by_id(&conn, leaf)
        .expect("entry")
        .expect("present");
    assert_eq!(leaf_row.logical_size, Some(7), "leaf.txt reconciled with its real size");

    writer.shutdown();
}
