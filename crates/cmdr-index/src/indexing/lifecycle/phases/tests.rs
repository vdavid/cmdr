//! What the phase machine has to get right, over a real temp tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tokio_util::sync::CancellationToken;

use super::stitch;
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

/// A fresh volume id per fixture, so parallel tests never look like each other's
/// in-flight walk.
fn next_fixture_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

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
        let serialized = crate::indexing::handle::test_lock();
        let data = tempfile::tempdir().expect("index data dir");
        let tree = tempfile::Builder::new()
            .prefix("cmdr-phased-drive-")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
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
        let events = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
        let (index, installed) = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(std::sync::Arc::clone(&volumes) as std::sync::Arc<_>)
            .host(host as std::sync::Arc<_>)
            .events(std::sync::Arc::clone(&events) as std::sync::Arc<dyn crate::indexing::events::EventSink>)
            .indexing_enabled(Some(true))
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
}

impl Drop for Drive {
    fn drop(&mut self) {
        let _ = self.index.forget_volume(self.volume_id);
    }
}

/// **The one that makes every other test here meaningful.** A phased start never
/// goes through `start_scan`, so nothing else writes the exclusion-policy stamp —
/// and without it `index_predates_exclusion_policy` answers yes, every coverage
/// query short-circuits to "the whole scope is frontier", the frontier never
/// shrinks, and each later root lands on the serial repair. The product would look
/// like it was working while never converging.
#[test]
fn a_fresh_phased_volume_s_frontier_shrinks_after_one_walk() {
    let drive = Drive::new(
        "phased-converges",
        |root| {
            std::fs::create_dir_all(root.join("one/inside")).expect("dirs");
            std::fs::create_dir_all(root.join("two")).expect("dirs");
            std::fs::write(root.join("one/file.txt"), "x").expect("file");
        },
        &[],
    );

    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "the whole drive reads as covered once the machine has been over it"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and an empty frontier is what completion means"
    );
    assert_eq!(
        drive.scans_started(),
        1,
        "the machine announces one run; ❌ no truncating full scan ever ran"
    );
}

// ── The stitch ───────────────────────────────────────────────────────

/// The finding that broke the first draft of the design: a cover walk marks only
/// the directories it READS, so covering one child leaves the parent's frontier
/// saying "walk the parent whole" — the later phase would re-walk everything the
/// earlier one covered, and hit the serial repair path doing it.
///
/// The stitch is what makes an ancestor scope's frontier shrink, and every root
/// it leaves has to be virgin, or the parallel walker refuses it.
#[test]
fn frontier_excludes_covered_ground_after_a_stitch() {
    let t = Tree::new();
    t.make(
        &["covered/inside", "untouched/inside"],
        &["covered/one.txt", "loose.txt"],
    );
    let root = t.root().to_string_lossy().to_string();

    // A priority phase covers one child of the tree root.
    t.seed_chain(&t.tree.path().join("covered"));
    t.cover(&t.path("covered"));

    // The later phase stitches the tree root before asking what is left.
    stitch::directory(&t.space, &t.writer, t.root());

    assert_eq!(
        t.frontier(&root),
        vec![t.path("untouched")],
        "the covered child is gone from the frontier and the untouched one is offered whole"
    );
    for frontier_root in t.frontier(&root) {
        let conn = IndexStore::open_read_connection(&t.db_path).expect("read connection");
        let id = crate::indexing::store::resolve_path(&conn, &frontier_root)
            .expect("resolve")
            .expect("a frontier root has a row");
        assert_eq!(
            IndexStore::count_children_capped(id, &conn, 1).expect("count"),
            0,
            "{frontier_root} must be virgin, or the parallel walker refuses it and the serial repair takes over"
        );
    }
}

/// `listed_children_on` serves a directory's rows as its FULL contents the moment
/// its `listed_epoch` is non-zero, and the MCP `list_dir` tool reads exactly that.
/// So a stitch that upserted only subdirectories would tell a user-visible
/// consumer that a folder holds no files, that same instant.
#[test]
fn a_stitched_directory_lists_its_files_not_only_its_subdirectories() {
    let t = Tree::new();
    t.make(&["sub"], &["one.txt", "two.txt"]);

    stitch::directory(&t.space, &t.writer, t.root());

    assert_eq!(
        t.indexed_children(&t.root().to_string_lossy()),
        vec!["one.txt".to_string(), "sub".to_string(), "two.txt".to_string()],
        "a stitched directory's rows are its whole listing, files included"
    );
}
