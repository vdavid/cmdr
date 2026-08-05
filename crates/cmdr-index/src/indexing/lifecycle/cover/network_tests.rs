//! The cover driver over a volume the index can only reach through the `Volume`
//! trait: a share, a phone, and whatever backend comes next.
//!
//! The local half lives in `tests.rs` and reads a real temp tree, because the
//! guarded walker reads the disk. Nothing here touches a disk at all: the ground
//! is an `InMemoryVolume`, which is exactly the shape a future backend arrives in.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError};

use super::*;
use crate::indexing::read::coverage::CoverageMap;

// ── Fixture ──────────────────────────────────────────────────────────

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
struct Share {
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
    fn new(volume_id: &'static str, build: impl FnOnce(&Tree) -> Vec<FileEntry>) -> Self {
        Self::with_volume(volume_id, |root| {
            let entries = build(&Tree(root.to_string()));
            Arc::new(InMemoryVolume::with_entries("Share", entries).with_root(root))
        })
    }

    /// The same share behind an [`Instrumented`] wrapper, handed back alongside it
    /// so a test can read what the walk did to the backend. `gate` names the one
    /// listing that blocks until the test releases it, relative to the mount root.
    fn instrumented(
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
    fn with_volume(volume_id: &'static str, describe: impl FnOnce(&str) -> Arc<dyn Volume>) -> Self {
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

    fn path(&self, relative: &str) -> String {
        Tree(self.root.clone()).path(relative)
    }

    fn coverage(&self, path: &str) -> CoverageMap {
        self.index
            .coverage(self.volume_id, path, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
    }

    /// The ids of everything the index holds under an absolute path, sorted. Read
    /// straight off the database, because the point of the tests that use it is
    /// which ROWS survived a second walk.
    fn child_ids(&self, absolute: &str) -> Vec<i64> {
        let db_path = self._data.path().join(format!("index-{}.db", self.volume_id));
        let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
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

    /// Start a walk over one scope.
    fn walk(&self, scope: &str) -> CoverWalk {
        self.index
            .cover(self.volume_id, vec![scope.to_string()], CoverageDimension::Listing)
            .expect("the share is walkable")
    }

    /// Walk one scope to the end, waiting for the rows to land.
    fn cover(&self, scope: &str) -> (Vec<CoveredEntry>, CoverOutcome) {
        let (entries, outcome) = drain(self.walk(scope));
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walked scope to read as covered",
            || {
                let covered = self.coverage(scope);
                covered.frontier.is_empty() && covered.unreadable.is_empty()
            },
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
struct Tree(String);

impl Tree {
    fn path(&self, relative: &str) -> String {
        if relative.is_empty() {
            self.0.clone()
        } else {
            format!("{}/{relative}", self.0)
        }
    }

    fn dir(&self, relative: &str) -> FileEntry {
        FileEntry::new(leaf(relative), self.path(relative), true, false)
    }

    fn file(&self, relative: &str, size: u64) -> FileEntry {
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
struct Instrumented {
    inner: InMemoryVolume,
    sessions_begun: AtomicU64,
    sessions_ended: AtomicU64,
    in_flight: AtomicU64,
    max_in_flight: AtomicU64,
    /// The one listing that blocks, if any.
    gate: Option<PathBuf>,
    gate_reached: AtomicBool,
    gate_released: AtomicBool,
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
            gate_released: AtomicBool::new(false),
        }
    }

    fn gated_at(mut self, path: impl Into<PathBuf>) -> Self {
        self.gate = Some(path.into());
        self
    }

    fn sessions(&self) -> (u64, u64) {
        (
            self.sessions_begun.load(Ordering::SeqCst),
            self.sessions_ended.load(Ordering::SeqCst),
        )
    }

    /// Block until the walk has reached the gated listing.
    fn wait_for_the_gate(&self) {
        cmdr_fs::testing::wait_until(std::time::Duration::from_secs(10), "the walk to reach the gated dir", || {
            self.gate_reached.load(Ordering::SeqCst)
        });
    }

    fn release_the_gate(&self) {
        self.gate_released.store(true, Ordering::SeqCst);
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
                while !self.gate_released.load(Ordering::SeqCst) {
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
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

/// Drain a walk, collecting every entry it emitted.
fn drain(walk: CoverWalk) -> (Vec<CoveredEntry>, CoverOutcome) {
    let mut entries = Vec::new();
    while let Some(batch) = walk.next_batch() {
        entries.extend(batch);
    }
    (entries, walk.finish())
}

// ── The scoped walk ──────────────────────────────────────────────────

/// A walk over a share covers the folder it was pointed at, hands every entry to
/// its consumer, and claims nothing else on the volume.
///
/// The second half is what "scoped" means, and it's the whole milestone: the only
/// walk `network_scanner` had was the whole volume, so a search of one folder on a
/// 10 TB NAS would have walked the NAS.
#[test]
fn a_walk_over_a_share_covers_the_folder_it_was_pointed_at() {
    let share = Share::new("cover-share-scoped-test", |t| {
        vec![
            t.dir("scope"),
            t.dir("elsewhere"),
            t.file("scope/one.txt", 4),
            t.dir("scope/inner"),
            t.file("scope/inner/two.txt", 2),
            t.file("elsewhere/other.txt", 9),
        ]
    });
    let scope = share.path("scope");

    let cold = share.coverage(&scope);
    assert_eq!(cold.frontier, vec![scope.clone()], "nothing is covered yet");

    let (entries, outcome) = share.cover(&scope);

    assert!(!outcome.cancelled, "the walk ran to the end");
    assert_eq!(outcome.roots_covered, 1);
    assert_eq!(outcome.entries_found, 3, "one.txt, inner/, inner/two.txt");
    assert_eq!(outcome.dirs_found, 1, "inner/ is the only directory among them");

    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        vec![
            share.path("scope/inner"),
            share.path("scope/inner/two.txt"),
            share.path("scope/one.txt")
        ],
        "every entry the walk wrote reached the consumer"
    );
    let one = entries
        .iter()
        .find(|e| e.path.ends_with("one.txt"))
        .expect("one.txt emitted");
    assert_eq!(one.logical_size, Some(4), "with the size a result row shows");

    assert_eq!(
        share.coverage(&share.path("")).frontier,
        vec![share.path("")],
        "and the rest of the share is untouched: nothing may claim coverage it didn't earn"
    );
}

// ── Cancellation ─────────────────────────────────────────────────────

/// A walk over a share stopped partway KEEPS every directory it read.
///
/// This is convergence, on the far side of a network round trip, and it's the
/// property the whole coverage concept rests on: eight minutes of walking a NAS
/// that someone then cancels has to leave the frontier genuinely smaller, or no
/// amount of searching ever shrinks it. ⚠️ It is the OPPOSITE of what the
/// whole-volume scan on the same transport does — that one discards its partial,
/// because a half-built index of a share is not an index of the share.
#[test]
fn a_cancelled_walk_over_a_share_keeps_the_ground_it_covered() {
    let (share, volume) = Share::instrumented(
        "cover-share-cancel-test",
        |t| {
            vec![
                t.dir("scope"),
                t.dir("scope/inner"),
                t.file("scope/inner/deep.txt", 3),
                t.file("scope/top.txt", 1),
            ]
        },
        // The scope's OWN listing blocks, so exactly one round trip is in flight
        // when the cancel lands and the walk stops in the same place every run.
        Some("scope"),
    );
    let scope = share.path("scope");

    let walk = share.walk(&scope);
    volume.wait_for_the_gate();
    walk.cancel();
    volume.release_the_gate();

    let (entries, outcome) = drain(walk);
    assert!(outcome.cancelled, "it was stopped, and says so");
    assert!(
        outcome.entries_found >= 2,
        "the totals carry what it read, not zero (got {outcome:?})"
    );
    assert!(!entries.is_empty(), "and the consumer got them");

    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the covered half of the scope to become durable",
        || share.coverage(&scope).frontier == [share.path("scope/inner")],
    );
    assert_eq!(
        share.coverage(&scope).frontier,
        [share.path("scope/inner")],
        "the directory the walk read is covered, and only what it never reached is left"
    );
}

/// The backend's scan session is opened once per walk and closed on every
/// outcome, cancel included.
///
/// Over SMB that session is a pool of extra TCP connections. An unpaired open
/// leaves them standing for the life of the app, and a walk somebody cancels is
/// exactly the case where nothing else would ever close them.
#[test]
fn the_scan_session_is_paired_on_the_completed_and_the_cancelled_walk() {
    let (share, volume) = Share::instrumented(
        "cover-share-session-test",
        |t| {
            vec![
                t.dir("first"),
                t.file("first/a.txt", 1),
                t.dir("second"),
                t.dir("second/inner"),
                t.file("second/inner/b.txt", 1),
            ]
        },
        Some("second"),
    );

    share.cover(&share.path("first"));
    assert_eq!(volume.sessions(), (1, 1), "a completed walk opens one session and closes it");

    let walk = share.walk(&share.path("second"));
    volume.wait_for_the_gate();
    walk.cancel();
    volume.release_the_gate();
    let (_, outcome) = drain(walk);

    assert!(outcome.cancelled);
    assert_eq!(
        volume.sessions(),
        (2, 2),
        "and so does a cancelled one: the pool never outlives the walk that opened it"
    );
}

// ── Pacing ───────────────────────────────────────────────────────────

/// The walk overlaps its round trips, and never past the pacer's budget.
///
/// Directory listing over a share is latency-bound, so a serial scoped walk would
/// be an order of magnitude slower than it needs to be — and the ceiling is the
/// same per-volume budget the background scan yields with, so a search walking a
/// share the user is also browsing drops to one listing in flight instead of
/// burying their navigation behind 64.
#[test]
fn the_walk_overlaps_its_listings_within_the_pacer_budget() {
    let subdirs = 20;
    let (share, volume) = Share::instrumented(
        "cover-share-pacing-test",
        |t| {
            let mut entries = vec![t.dir("scope")];
            for i in 0..subdirs {
                entries.push(t.dir(&format!("scope/d{i}")));
                entries.push(t.file(&format!("scope/d{i}/f.txt"), 1));
            }
            entries
        },
        None,
    );

    let (_, outcome) = share.cover(&share.path("scope"));
    assert_eq!(outcome.dirs_found, subdirs, "every subdirectory was walked");

    let max_in_flight = volume.max_in_flight.load(Ordering::SeqCst);
    assert!(
        max_in_flight > 1,
        "the walk overlaps its listings rather than going one at a time (max was {max_in_flight})"
    );
    assert!(
        max_in_flight <= crate::indexing::network_scanner::scan_pace::FULL_LISTING_BUDGET as u64,
        "and never past the budget the pacer hands out (max was {max_in_flight})"
    );
}

// ── Ground somebody already touched ──────────────────────────────────

/// A walk over a frontier node the index already holds rows under keeps them: the
/// pre-existing rows keep their ids, the new siblings arrive, and nothing is
/// deleted.
///
/// The shape takes one earlier walk to produce — cover a deep folder, and the
/// ancestor it had to materialize is a frontier node with a covered child under
/// it. The local walker refuses this case and hands it to the serial reconcile;
/// over the trait the walk simply takes it, because comparing a directory's names
/// against the index costs nothing next to the round trip that listed them.
#[test]
fn a_walk_over_ground_an_earlier_walk_touched_keeps_its_rows() {
    let share = Share::new("cover-share-existing-rows-test", |t| {
        vec![
            t.dir("F"),
            t.dir("F/G"),
            t.file("F/G/kept.txt", 4),
            t.file("F/new.txt", 3),
        ]
    });

    // The first walk covers G and materializes F on the way, without listing it.
    share.cover(&share.path("F/G"));
    let g_rows = share.child_ids(&share.path("F/G"));
    assert_eq!(g_rows.len(), 1, "precondition: G holds kept.txt");
    assert_eq!(
        share.coverage(&share.path("F")).frontier,
        [share.path("F")],
        "precondition: F itself is a frontier node"
    );

    let (entries, outcome) = share.cover(&share.path("F"));

    assert_eq!(outcome.roots_covered, 1);
    assert_eq!(
        share.child_ids(&share.path("F/G")),
        g_rows,
        "the rows this walk did not write keep their ids"
    );
    assert_eq!(share.child_ids(&share.path("F")).len(), 2, "G, plus the new sibling");
    let emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    assert_eq!(
        emitted,
        [share.path("F/new.txt")],
        "and only the row it actually wrote is emitted, so a live search sees no duplicates"
    );
}

// ── Same-name siblings (MTP) ─────────────────────────────────────────

/// A backend that reports every child of one directory TWICE, which is what a
/// phone does: MTP objects are handles, not names, so one folder can genuinely
/// hold two children called the same thing.
struct SameNameSiblings {
    inner: InMemoryVolume,
    doubled: PathBuf,
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

/// Two children with one name: the walk keeps the first and says so, instead of
/// allocating a second id whose rows vanish.
///
/// The index can hold one row per `(parent, folded name)`, and `insert_entries_v2_batch`
/// is `INSERT OR IGNORE` — so without the per-directory name check the second
/// `dup` would get an id, be queued as a directory of its own, have its children
/// written under that id, and then lose the row that id belonged to. Everything
/// below it would be orphaned: rows in the database that no path resolves to,
/// invisible to search and counted in nobody's size.
#[test]
fn a_same_name_sibling_keeps_the_first_row_rather_than_orphaning_a_subtree() {
    let share = Share::with_volume("cover-share-mtp-siblings-test", |root| {
        let tree = Tree(root.to_string());
        let inner = InMemoryVolume::with_entries(
            "Phone",
            vec![tree.dir("scope"), tree.dir("scope/dup"), tree.file("scope/dup/child.txt", 7)],
        )
        .with_root(root);
        Arc::new(SameNameSiblings {
            inner,
            doubled: PathBuf::from(tree.path("scope")),
        })
    });
    let scope = share.path("scope");

    let (entries, outcome) = share.cover(&scope);

    assert_eq!(outcome.entries_found, 2, "dup/ once, and its child once");
    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(emitted, [share.path("scope/dup"), share.path("scope/dup/child.txt")]);
    assert_eq!(
        share.child_ids(&scope).len(),
        1,
        "one row for the name, and no second id pointing at nothing"
    );
    assert_eq!(
        share.child_ids(&share.path("scope/dup")).len(),
        1,
        "and the subtree below it is attributed to the row that survived"
    );
}
