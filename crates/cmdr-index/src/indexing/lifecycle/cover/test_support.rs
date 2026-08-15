//! Helpers the cover test files share.
//!
//! Two kinds of thing live here. `drain` is what every one of them does to a
//! walk. The rest is the `Volume`-trait scaffolding `network_tests.rs` and
//! `network_give_up_tests.rs` run on: a share driven through the public handle,
//! and the hand-rolled backends that record, stall, double, refuse, or go away. They're here rather than beside the tests
//! because they're ~350 lines of `Pin<Box<dyn Future>>` ceremony that says nothing
//! about what any single test is checking.
//!
//! The single-file fixtures stay with their tests: the temp-tree `Fixture` in
//! `tests.rs`, the `ColdDrive` in `cold_drive_tests.rs`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};

use super::*;
use crate::indexing::read::coverage::CoverageMap;

/// Drain a walk, collecting every entry it emitted.
pub(super) fn drain(walk: CoverWalk) -> (Vec<CoveredEntry>, CoverOutcome) {
    let mut entries = Vec::new();
    while let Some(batch) = walk.next_batch() {
        entries.extend(batch);
    }
    (entries, walk.finish())
}

/// A platform-appropriate mount root. Read routing sends a path to a per-mount
/// index only when it sits under an external-mount prefix, and those differ per
/// OS, so a hardcoded `/Volumes/…` would route back to `root`'s index on Linux.
#[cfg(target_os = "macos")]
const MOUNT_PREFIX: &str = "/Volumes";
#[cfg(not(target_os = "macos"))]
const MOUNT_PREFIX: &str = "/media";

/// A share the index has never seen, driven through the public handle.
///
/// Everything behind the handle is process-wide, so this holds the test lock for
/// its whole life and forgets the volume on the way out. ⚠️ Field order IS
/// teardown order: the seam guard has to drop before the lock guard, or it
/// restores the previous data directory over the top of whichever test took the
/// lock next.
pub(super) struct Share {
    _installed: crate::indexing::handle::TestInstallGuard,
    index: crate::indexing::handle::Index,
    volume_id: &'static str,
    root: String,
    _data: tempfile::TempDir,
    _serialized: std::sync::MutexGuard<'static, ()>,
}

impl Share {
    /// A share whose contents are what `build` puts under its mount root,
    /// registered as a network mount (which is what keeps the LOCAL guarded
    /// walker off it).
    pub(super) fn new(volume_id: &'static str, build: impl FnOnce(&Tree) -> Vec<FileEntry>) -> Self {
        Self::with_volume(volume_id, |root| {
            let entries = build(&Tree(root.to_string()));
            Arc::new(InMemoryVolume::with_entries("Share", entries).with_root(root))
        })
    }

    /// The same share behind an [`Instrumented`] wrapper, handed back alongside it
    /// so a test can read what the walk did to the backend. `gate` names the one
    /// listing that blocks until the test releases it, relative to the mount root.
    pub(super) fn instrumented(
        volume_id: &'static str,
        build: impl FnOnce(&Tree) -> Vec<FileEntry>,
        gate: Option<&str>,
    ) -> (Self, Arc<Instrumented>) {
        let mut instrumented = None;
        let share = Self::with_volume(volume_id, |root| {
            let tree = Tree(root.to_string());
            let inner = InMemoryVolume::with_entries("Share", build(&tree)).with_root(root);
            let mut volume = Instrumented::new(inner);
            if let Some(gate) = gate {
                volume = volume.gated_at(tree.path(gate));
            }
            let volume = Arc::new(volume);
            instrumented = Some(Arc::clone(&volume));
            volume as Arc<dyn Volume>
        });
        (share, instrumented.expect("the wrapper is built while registering"))
    }

    /// The same, with the registered volume built by `describe` — a wrapper that
    /// counts scan sessions, one that stalls, whatever the test needs.
    pub(super) fn with_volume(volume_id: &'static str, describe: impl FnOnce(&str) -> Arc<dyn Volume>) -> Self {
        let serialized = crate::indexing::handle::test_lock();
        let data = tempfile::tempdir().expect("index data dir");
        let root = format!("{MOUNT_PREFIX}/{volume_id}");

        let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
        volumes.register(volume_id, describe(&root)).mark_network(&root);

        let events = Arc::new(crate::indexing::events::RecordingSink::new());
        let (index, installed) = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(Arc::clone(&volumes) as Arc<_>)
            .events(events as Arc<dyn crate::indexing::events::EventSink>)
            .install_for_test();

        Self {
            _installed: installed,
            index,
            volume_id,
            root,
            _data: data,
            _serialized: serialized,
        }
    }

    pub(super) fn path(&self, relative: &str) -> String {
        Tree(self.root.clone()).path(relative)
    }

    pub(super) fn coverage(&self, path: &str) -> CoverageMap {
        self.index
            .coverage(self.volume_id, path, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
    }

    /// Whether this share's index has a window open to retry the ground its walks
    /// gave up on.
    pub(super) fn retry_window_is_open(&self) -> bool {
        crate::indexing::writer::retry_window_is_open(&self.read_connection())
    }

    /// The ids of everything the index holds under an absolute path, sorted. Read
    /// straight off the database, because the point of the tests that use it is
    /// which ROWS survived a second walk.
    pub(super) fn child_ids(&self, absolute: &str) -> Vec<i64> {
        let conn = self.read_connection();
        let relative = absolute.strip_prefix(&self.root).unwrap_or(absolute);
        let Some(id) = crate::indexing::store::resolve_path(&conn, relative).expect("resolve") else {
            return Vec::new();
        };
        let mut ids: Vec<i64> = IndexStore::list_children_on(id, &conn)
            .expect("list children")
            .iter()
            .map(|row| row.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// A read connection to this share's own index database.
    fn read_connection(&self) -> rusqlite::Connection {
        let db_path = self._data.path().join(format!("index-{}.db", self.volume_id));
        IndexStore::open_read_connection(&db_path).expect("read connection")
    }

    /// Start a walk over one scope, with the token that stops it.
    pub(super) fn walk(&self, scope: &str) -> (CoverWalk, CancellationToken) {
        let cancel = CancellationToken::new();
        let walk = self
            .index
            .cover(
                self.volume_id,
                vec![scope.to_string()],
                CoverageDimension::Listing,
                cancel.clone(),
            )
            .expect("the share is walkable");
        (walk, cancel)
    }

    /// Walk one scope to the end, waiting for the rows to land.
    pub(super) fn cover(&self, scope: &str) -> (Vec<CoveredEntry>, CoverOutcome) {
        let (entries, outcome) = drain(self.walk(scope).0);
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walked scope to read as covered",
            // Frontier only: ground the walk deliberately won't read into (a NAS
            // snapshot dir) lands in `unreadable`, which is a settled answer rather
            // than something still to wait for.
            || self.coverage(scope).frontier.is_empty(),
        );
        (entries, outcome)
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        let _ = self.index.forget_volume(self.volume_id);
    }
}

/// Builds absolute paths under a share's mount root, so a test names its ground
/// the way a user's scope does.
pub(super) struct Tree(pub(super) String);

impl Tree {
    pub(super) fn path(&self, relative: &str) -> String {
        if relative.is_empty() {
            self.0.clone()
        } else {
            format!("{}/{relative}", self.0)
        }
    }

    pub(super) fn dir(&self, relative: &str) -> FileEntry {
        FileEntry::new(leaf(relative), self.path(relative), true, false)
    }

    pub(super) fn file(&self, relative: &str, size: u64) -> FileEntry {
        FileEntry {
            size: Some(size),
            ..FileEntry::new(leaf(relative), self.path(relative), false, false)
        }
    }
}

fn leaf(relative: &str) -> String {
    relative.rsplit('/').next().unwrap_or(relative).to_string()
}

// ── An instrumented backend ──────────────────────────────────────────

/// A `Volume` that records what a walk did to it, and can hold ONE listing open
/// until the test lets it go.
///
/// The gate is what makes a cancel deterministic: a walk parked on a listing has
/// exactly one round trip in flight, so "cancel now" means the same thing every
/// run.
pub(super) struct Instrumented {
    inner: InMemoryVolume,
    sessions_begun: AtomicU64,
    sessions_ended: AtomicU64,
    in_flight: AtomicU64,
    pub(super) max_in_flight: AtomicU64,
    /// The one listing that blocks, if any.
    gate: Option<PathBuf>,
    gate_reached: AtomicBool,
    /// Released by the test. A permit stored before the walk even reaches the gate
    /// is still honored, so the test can't lose the race by releasing early.
    gate_released: tokio::sync::Notify,
}

impl Instrumented {
    fn new(inner: InMemoryVolume) -> Self {
        Self {
            inner,
            sessions_begun: AtomicU64::new(0),
            sessions_ended: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            max_in_flight: AtomicU64::new(0),
            gate: None,
            gate_reached: AtomicBool::new(false),
            gate_released: tokio::sync::Notify::new(),
        }
    }

    fn gated_at(mut self, path: impl Into<PathBuf>) -> Self {
        self.gate = Some(path.into());
        self
    }

    pub(super) fn sessions(&self) -> (u64, u64) {
        (
            self.sessions_begun.load(Ordering::SeqCst),
            self.sessions_ended.load(Ordering::SeqCst),
        )
    }

    /// Block until the walk has reached the gated listing.
    pub(super) fn wait_for_the_gate(&self) {
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walk to reach the gated dir",
            || self.gate_reached.load(Ordering::SeqCst),
        );
    }

    pub(super) fn release_the_gate(&self) {
        self.gate_released.notify_one();
    }
}

/// The future every `Volume` method here hands back.
type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl Volume for Instrumented {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn begin_scan_session<'a>(&'a self) -> Fut<'a, ()> {
        self.sessions_begun.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
    fn end_scan_session<'a>(&'a self) -> Fut<'a, ()> {
        self.sessions_ended.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {})
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        Box::pin(async move {
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(now, Ordering::SeqCst);
            // Let sibling listings launched in the same batch coexist before any
            // resolves, so the recorded maximum reflects real concurrency rather
            // than instantly-ready mock timing.
            tokio::task::yield_now().await;
            if self.gate.as_deref() == Some(path) {
                self.gate_reached.store(true, Ordering::SeqCst);
                self.gate_released.notified().await;
            }
            let result = self.inner.list_directory(path, on_progress).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            result
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        self.inner.is_directory(path)
    }
}

/// A backend that reports every child of one directory TWICE, which is what a
/// phone does: MTP objects are handles, not names, so one folder can genuinely
/// hold two children called the same thing.
pub(super) struct SameNameSiblings {
    pub(super) inner: InMemoryVolume,
    pub(super) doubled: PathBuf,
}

impl Volume for SameNameSiblings {
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
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        Box::pin(async move {
            let entries = self.inner.list_directory(path, on_progress).await?;
            if path != self.doubled {
                return Ok(entries);
            }
            Ok([entries.clone(), entries].concat())
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        self.inner.is_directory(path)
    }
}

/// A backend that refuses to list the paths it was given, and serves everything
/// else from memory.
///
/// The refusals are behind a lock so a test can lift them mid-run: a directory
/// that starts answering again is how abandoned ground heals, and that needs one
/// backend that says two different things over its life.
pub(super) struct RefusesToList {
    inner: InMemoryVolume,
    refused: Mutex<Vec<PathBuf>>,
    /// The one listing that blocks until the test lets it go, same as
    /// [`Instrumented`]'s. Here it's what makes a cancel land at a KNOWN point in
    /// the walk, so "the walk had already proved this give-up" is a fact rather
    /// than a race.
    gate: Mutex<Option<PathBuf>>,
    gate_reached: AtomicBool,
    gate_released: tokio::sync::Notify,
}

impl RefusesToList {
    /// Answer everything from here on, as a share that woke up does.
    pub(super) fn answer_everything(&self) {
        self.refused.lock_ignore_poison().clear();
    }

    /// Hold the listing of `path` open until [`release_the_gate`](Self::release_the_gate).
    pub(super) fn gate_at(&self, path: &str) {
        *self.gate.lock_ignore_poison() = Some(PathBuf::from(path));
    }

    /// Block until the walk has reached the gated listing.
    pub(super) fn wait_for_the_gate(&self) {
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walk to reach the gated dir",
            || self.gate_reached.load(Ordering::SeqCst),
        );
    }

    pub(super) fn release_the_gate(&self) {
        self.gate_released.notify_one();
    }
}

impl Volume for RefusesToList {
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
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        if self.refused.lock_ignore_poison().iter().any(|refused| refused == path) {
            return Box::pin(async { Err(VolumeError::PermissionDenied("test: listing refused".into())) });
        }
        let gated = self.gate.lock_ignore_poison().as_deref() == Some(path);
        Box::pin(async move {
            if gated {
                self.gate_reached.store(true, Ordering::SeqCst);
                self.gate_released.notified().await;
            }
            self.inner.list_directory(path, on_progress).await
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        self.inner.is_directory(path)
    }
}

impl Share {
    /// A share that refuses to list the paths `refuse` names, relative to its
    /// mount root.
    pub(super) fn refusing(
        volume_id: &'static str,
        build: impl FnOnce(&Tree) -> Vec<FileEntry>,
        refuse: &[&str],
    ) -> Self {
        Self::refusing_for_now(volume_id, build, refuse).0
    }

    /// The same, handing the backend back so a test can lift the refusals partway
    /// through and watch the ground heal.
    pub(super) fn refusing_for_now(
        volume_id: &'static str,
        build: impl FnOnce(&Tree) -> Vec<FileEntry>,
        refuse: &[&str],
    ) -> (Self, Arc<RefusesToList>) {
        let refuse: Vec<String> = refuse.iter().map(|r| r.to_string()).collect();
        let mut backend = None;
        let share = Self::with_volume(volume_id, |root| {
            let tree = Tree(root.to_string());
            let volume = Arc::new(RefusesToList {
                refused: Mutex::new(refuse.iter().map(|r| PathBuf::from(tree.path(r))).collect()),
                inner: InMemoryVolume::with_entries("Share", build(&tree)).with_root(root),
                gate: Mutex::new(None),
                gate_reached: AtomicBool::new(false),
                gate_released: tokio::sync::Notify::new(),
            });
            backend = Some(Arc::clone(&volume));
            volume as Arc<dyn Volume>
        });
        (share, backend.expect("the backend is built while registering"))
    }

    /// A share that answers `answers` listings and then goes away, failing every
    /// one after that with the UNTYPED shape a dropped SMB session actually
    /// produces.
    ///
    /// ⚠️ Untyped on purpose. `DeviceDisconnected` has its own arm in the walk and
    /// stops it on the first failure, so a test built on it would prove nothing
    /// about the arm that matters: a connection reset arrives as a plain `IoError`,
    /// reaches the ordinary skip-this-directory branch, and is indistinguishable
    /// there from one directory that won't list. That's the whole reason the
    /// consecutive-failure backstop exists (~6,475 directories churned into empty
    /// rows in about a second, in the reported bug).
    pub(super) fn going_away_after(
        volume_id: &'static str,
        build: impl FnOnce(&Tree) -> Vec<FileEntry>,
        answers: i64,
    ) -> Self {
        Self::with_volume(volume_id, |root| {
            let tree = Tree(root.to_string());
            Arc::new(GoesAway {
                inner: InMemoryVolume::with_entries("Share", build(&tree)).with_root(root),
                answers_left: AtomicI64::new(answers),
            })
        })
    }
}

/// A backend that answers a fixed number of listings and then stops being there.
struct GoesAway {
    inner: InMemoryVolume,
    answers_left: AtomicI64,
}

impl Volume for GoesAway {
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
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        if self.answers_left.fetch_sub(1, Ordering::SeqCst) <= 0 {
            return Box::pin(async {
                Err(VolumeError::IoError {
                    message: "test: connection reset by peer".into(),
                    raw_os_error: None,
                })
            });
        }
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        self.inner.is_directory(path)
    }
}
