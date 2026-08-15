//! What the phase machine has to get right, over a real temp tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tokio_util::sync::CancellationToken;

use super::{HOME_COVERED_AT_KEY, VisitLog, stitch};
use crate::indexing::IndexPathSpace;
use crate::indexing::lifecycle::cover::{self, CoverContext};
use crate::indexing::read::coverage::{CoverageDimension, CoverageMap, coverage_for_scope};
use crate::indexing::scanner::exclusion_policy_stamp_message;
use crate::indexing::store::{IndexStore, ROOT_ID};
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Fixture ──────────────────────────────────────────────────────────

/// A temp tree plus an index prepared exactly as a phased start prepares one:
/// the epoch seeded and the exclusion policy stamped, so a coverage answer means
/// something. Without both, every query short-circuits to "walk the whole scope".
struct Tree {
    tree: tempfile::TempDir,
    _db_dir: tempfile::TempDir,
    db_path: PathBuf,
    writer: IndexWriter,
    space: IndexPathSpace,
    /// A volume id of its own, because the in-flight frontier claims are keyed by
    /// one and these tests run in parallel over paths that look alike.
    volume_id: String,
}

impl Tree {
    fn new() -> Self {
        // In the CWD rather than `/tmp`: `/tmp` is excluded on Linux and is a
        // symlink on macOS, and both would fight the path space.
        let tree = tempfile::Builder::new()
            .prefix("cmdr-phases-test-")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("temp tree");
        let db_dir = tempfile::tempdir().expect("temp db dir");
        let db_path = db_dir.path().join("phases-test-index.db");
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
        writer.send(WriteMessage::BumpCurrentEpoch).expect("seed the epoch");
        writer.send(exclusion_policy_stamp_message()).expect("stamp the policy");
        writer.flush_blocking().expect("flush the preparation");

        let fixture = Self {
            tree,
            _db_dir: db_dir,
            db_path,
            writer,
            space: IndexPathSpace::root(),
            volume_id: format!("phases-fixture-{}", next_fixture_id()),
        };
        fixture.seed_chain(fixture.tree.path());
        fixture
    }

    /// Insert the ancestor chain down to `path`, and sync the writer's id counter.
    /// The temp tree sits many levels below `/`, and the phases under test are
    /// about what happens BELOW it, so the chain above is scaffolding.
    fn seed_chain(&self, path: &Path) -> i64 {
        let conn = IndexStore::open_write_connection(&self.db_path).expect("write connection");
        let path_str = path.to_string_lossy();
        let mut parent_id = ROOT_ID;
        for component in path_str.split('/').filter(|c| !c.is_empty()) {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None)
                    .expect("insert chain component"),
            };
        }
        let next_id = IndexStore::get_next_id(&conn).expect("next id");
        self.writer.next_id().fetch_max(next_id, Ordering::Relaxed);
        parent_id
    }

    fn root(&self) -> &Path {
        self.tree.path()
    }

    fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    fn make(&self, dirs: &[&str], files: &[&str]) {
        for dir in dirs {
            std::fs::create_dir_all(self.tree.path().join(dir)).expect("dirs");
        }
        for file in files {
            std::fs::write(self.tree.path().join(file), "x").expect("file");
        }
    }

    fn context(&self) -> CoverContext {
        CoverContext {
            volume_id: self.volume_id.clone(),
            writer: self.writer.clone(),
            space: self.space.clone(),
            kind: IndexVolumeKind::Local,
            flush: Default::default(),
        }
    }

    /// Walk one frontier root to the end, the way the machine does.
    fn cover(&self, root: &str) {
        let walk = cover::start(
            self.context(),
            vec![root.to_string()],
            CoverageDimension::Listing,
            CancellationToken::new(),
        );
        while walk.next_batch().is_some() {}
        walk.finish();
        self.writer.flush_blocking().expect("flush the walk");
    }

    fn coverage(&self, scope: &str) -> CoverageMap {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        coverage_for_scope(&conn, scope, scope, CoverageDimension::Listing).expect("coverage")
    }

    fn frontier(&self, scope: &str) -> Vec<String> {
        let mut frontier = self.coverage(scope).frontier;
        frontier.sort();
        frontier
    }

    /// The names the index holds under a directory, which is what a listing
    /// consumer is served the moment that directory reads as listed.
    fn indexed_children(&self, path: &str) -> Vec<String> {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        let Some(id) = crate::indexing::store::resolve_path(&conn, path).expect("resolve") else {
            return Vec::new();
        };
        let mut names: Vec<String> = IndexStore::list_children_on(id, &conn)
            .expect("list children")
            .iter()
            .map(|row| row.name.clone())
            .collect();
        names.sort();
        names
    }
}

/// Where a fixture's temp tree is built.
///
/// The CWD by default: `/tmp` is excluded on Linux and is a symlink on macOS, and
/// both would fight the path space. `CMDR_PHASES_TEST_TREE_DIR` moves it, which is
/// what the resume benchmark next door needs — its tree holds six figures of
/// directories, and building that inside the repository would hand every watcher
/// and indexer on the machine a tree the size of the repo many times over. ⚠️ Point
/// it at a path with no symlink in it (`/private/tmp`, ❌ not `/tmp`).
fn tree_parent() -> PathBuf {
    std::env::var("CMDR_PHASES_TEST_TREE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// A fresh volume id per fixture, so parallel tests never look like each other's
/// in-flight walk.
fn next_fixture_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ── The test files, by subject ───────────────────────────────────────

/// Whether a phase covers ground at all: frontier, stitch, exclusion stamp, and
/// the refusal that keeps a truncating scan out.
mod coverage;

/// What a covered volume owes, and the order the terminal reports go out in.
mod completion;

/// What a second launch finds, and what the two switches do to a half-covered
/// drive.
mod relaunch;

/// Staying responsive while walking: grouping, visits, and the progress pump.
mod interleaving;

/// The two drive-menu actions a user can reach a half-covered volume with.
mod menu_actions;

/// What resuming an interrupted run costs, measured against covering the same
/// ground in one go. `#[ignore]`d.
mod resume_bench;

/// What covering a REAL home folder costs. `#[ignore]`d.
mod home_bench;

// ── A drive the machine covers, end to end ───────────────────────────

/// A local drive with no index, driven through the PUBLIC handle so the whole
/// real path runs: the activation, the database preparation, the branch-watch
/// resume, and the machine itself.
///
/// Everything behind the handle is process-wide, so this holds the test lock for
/// its whole life and forgets the volume on the way out. ⚠️ Field order IS
/// teardown order: the seam guard drops before the lock guard, or it restores the
/// previous data directory over the top of whichever test took the lock next.
struct Drive {
    _installed: crate::indexing::handle::TestInstallGuard,
    data: tempfile::TempDir,
    tree: tempfile::TempDir,
    index: crate::indexing::handle::Index,
    events: std::sync::Arc<crate::indexing::events::RecordingSink>,
    volume_id: &'static str,
    _serialized: std::sync::MutexGuard<'static, ()>,
}

impl Drive {
    /// A drive whose contents come off a real temp tree (the LOCAL walker reads
    /// the disk, not the volume), with `priority` as the folders its owner cares
    /// about, relative to the tree root.
    fn new(volume_id: &'static str, build: impl FnOnce(&Path), priority: &[&str]) -> Self {
        Self::with_host(volume_id, build, |_, _| {}, priority, true)
    }

    /// The same, with `host` given the fake host policy and the tree root so a
    /// test can say where the user is looking.
    fn with_host(
        volume_id: &'static str,
        build: impl FnOnce(&Path),
        host_says: impl FnOnce(&crate::indexing::host::policy::FakeHostPolicy, &Path),
        priority: &[&str],
        indexing_enabled: bool,
    ) -> Self {
        let recorder = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
        let sink = std::sync::Arc::clone(&recorder) as std::sync::Arc<dyn crate::indexing::events::EventSink>;
        Self::assembled(volume_id, build, host_says, priority, indexing_enabled, sink, recorder)
    }

    /// The whole fixture, with the sink the volume reports THROUGH given apart
    /// from the recorder a test reads back. Everything above hands in the recorder
    /// for both; a test that needs to act from inside an `emit` wraps it.
    #[allow(
        clippy::too_many_arguments,
        reason = "the fixture's whole surface in one place; the constructors above are the ones tests call"
    )]
    fn assembled(
        volume_id: &'static str,
        build: impl FnOnce(&Path),
        host_says: impl FnOnce(&crate::indexing::host::policy::FakeHostPolicy, &Path),
        priority: &[&str],
        indexing_enabled: bool,
        sink: std::sync::Arc<dyn crate::indexing::events::EventSink>,
        events: std::sync::Arc<crate::indexing::events::RecordingSink>,
    ) -> Self {
        let serialized = crate::indexing::handle::test_lock();
        let data = tempfile::tempdir().expect("index data dir");
        let tree = tempfile::Builder::new()
            .prefix("cmdr-phased-drive-")
            .tempdir_in(tree_parent())
            .expect("temp tree");
        build(tree.path());

        let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
        volumes.register(
            volume_id,
            std::sync::Arc::new(
                cmdr_fs::volume::InMemoryVolume::new("Phased")
                    .with_root(tree.path())
                    .with_local_fs_access(),
            ),
        );
        let host = crate::indexing::host::policy::FakeHostPolicy::shared();
        for root in priority {
            host.note_priority_root(volume_id, tree.path().join(root));
        }
        host_says(&host, tree.path());
        let (index, installed) = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(std::sync::Arc::clone(&volumes) as std::sync::Arc<_>)
            .host(host as std::sync::Arc<_>)
            .events(sink)
            .indexing_enabled(Some(indexing_enabled))
            .install_for_test();

        Self {
            _installed: installed,
            data,
            tree,
            index,
            events,
            volume_id,
            _serialized: serialized,
        }
    }

    /// Turn indexing on for the drive, which is what hands it to the machine.
    fn start(&self) {
        crate::indexing::host::runtime::block_on(self.index.start_volume(self.volume_id))
            .expect("the drive starts indexing");
    }

    fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    fn db_path(&self) -> PathBuf {
        self.data.path().join(format!("index-{}.db", self.volume_id))
    }

    fn frontier(&self, scope: &str) -> Vec<String> {
        self.index
            .coverage(self.volume_id, scope, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
            .frontier
    }

    /// The phases this drive announced, in order. The order IS the feature, so
    /// most phase tests assert on this sequence rather than on event kinds.
    fn announced_phases(&self) -> Vec<crate::indexing::events::CoveragePhase> {
        self.events
            .events()
            .into_iter()
            .filter_map(|event| match event {
                crate::indexing::events::IndexEvent::CoveragePhaseStarted { volume_id, phase, .. }
                    if volume_id == self.volume_id =>
                {
                    Some(phase)
                }
                _ => None,
            })
            .collect()
    }

    fn meta(&self, key: &str) -> Option<String> {
        let conn = IndexStore::open_read_connection(&self.db_path()).ok()?;
        IndexStore::get_meta(&conn, key).ok().flatten()
    }

    /// Wait for the machine to report it has nothing left to do.
    fn wait_for_the_machine(&self) {
        cmdr_fs::testing::wait_until(std::time::Duration::from_secs(30), "the phases to finish", || {
            !self.index.status(self.volume_id).is_ok_and(|status| status.scanning)
        });
    }

    fn scans_started(&self) -> usize {
        self.events
            .kinds_for(self.volume_id)
            .iter()
            .filter(|kind| **kind == crate::indexing::events::IndexEventKind::ScanStarted)
            .count()
    }

    /// How many activity phases this drive announced. One per phase the machine
    /// runs, plus the one it ends on.
    fn phase_changes(&self) -> usize {
        self.events
            .kinds_for(self.volume_id)
            .iter()
            .filter(|kind| **kind == crate::indexing::events::IndexEventKind::PhaseChanged)
            .count()
    }

    /// Take the volume down: the instance goes, the database stays. What a quit is.
    fn stop(&self) {
        crate::indexing::lifecycle::state::stop_indexing(self.volume_id).expect("the drive stops indexing");
    }

    /// Take the volume down and bring it back, which is what a relaunch is: the
    /// instance goes, the database stays.
    fn restart(&self) {
        self.stop();
        self.start();
    }

    /// Write a row for a FILE that isn't on disk, deep inside ground the last run
    /// covered. Call it with the volume stopped, so no live writer is allocating
    /// ids behind this one's back.
    ///
    /// It is how a launch says which of two things it did. Nothing re-lists a
    /// covered directory on a resume — its frontier is empty, and the stitch only
    /// touches the ancestors of a phase root — so the ghost is exactly "the rows
    /// the last session wrote": still there if the index was resumed, gone if it
    /// was thrown away and rebuilt. ❌ A row count can't say this, because a
    /// rebuild re-walks the same tree and lands on the same count.
    fn plant_a_ghost(&self, parent: &str, name: &str) {
        let conn = IndexStore::open_write_connection(&self.db_path()).expect("write connection");
        let parent_id = self.id_of(&self.path(parent)).expect("the ghost's parent is indexed");
        IndexStore::insert_entry_v2(&conn, parent_id, name, false, false, None, None, None, None)
            .expect("insert the ghost");
    }

    /// Whether the ghost this drive planted is still there.
    fn ghost_survived(&self, parent: &str, name: &str) -> bool {
        self.id_of(&self.path(&format!("{parent}/{name}"))).is_some()
    }

    /// Forget which ground this index's rows cover, which is what an interrupted
    /// BULK scan leaves behind: `start_scan` clears the branch set before it walks.
    fn forget_the_covered_branches(&self) {
        let conn = IndexStore::open_write_connection(&self.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, crate::indexing::watch::branches::COVERED_BRANCHES_KEY)
            .expect("clear the branch set");
    }

    /// Drop the completion marker, which is what a quit mid-coverage leaves: rows,
    /// and nothing saying the drive is done.
    fn forget_the_completion_marker(&self) {
        let conn = IndexStore::open_write_connection(&self.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }

    fn id_of(&self, path: &str) -> Option<i64> {
        let conn = IndexStore::open_read_connection(&self.db_path()).ok()?;
        let relative = IndexPathSpace::mount_rooted(self.path("")).index_relative(path)?;
        crate::indexing::store::resolve_path(&conn, &relative).ok().flatten()
    }

    /// The epoch this drive's rows are written against. A truncating rescan bumps
    /// it, so it reads as "something blanked this index" from outside.
    fn current_epoch(&self) -> u64 {
        let conn = IndexStore::open_read_connection(&self.db_path()).expect("read connection");
        IndexStore::read_current_epoch(&conn).expect("current epoch")
    }

    fn entry_count(&self) -> u64 {
        let conn = IndexStore::open_read_connection(&self.db_path()).expect("read connection");
        IndexStore::get_entry_count(&conn).expect("entry count")
    }

    /// Send one message through this drive's own writer and wait for it to land.
    fn write(&self, message: WriteMessage) {
        let (writer, _) =
            crate::indexing::lifecycle::state::get_writer_and_scanning_for(self.volume_id).expect("a running writer");
        writer.send(message).expect("the writer takes it");
        writer.flush_blocking().expect("and commits it");
    }
}

impl Drop for Drive {
    fn drop(&mut self) {
        let _ = self.index.forget_volume(self.volume_id);
    }
}

// ── The home a test drives ───────────────────────────────────────────

/// The synthetic home the machine reads instead of the real one, while a fixture
/// holds it. Process-global, which is safe because every test that sets it holds
/// the handle test lock for its whole life.
static HOME_OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub(super) fn home_override() -> Option<PathBuf> {
    use cmdr_fs::ignore_poison::IgnorePoison;
    HOME_OVERRIDE.lock_ignore_poison().clone()
}

/// Point the machine's home phase at `home` until the guard drops.
fn set_home_override(home: PathBuf) -> HomeOverrideGuard {
    use cmdr_fs::ignore_poison::IgnorePoison;
    *HOME_OVERRIDE.lock_ignore_poison() = Some(home);
    HomeOverrideGuard
}

struct HomeOverrideGuard;

impl Drop for HomeOverrideGuard {
    fn drop(&mut self) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        *HOME_OVERRIDE.lock_ignore_poison() = None;
    }
}
