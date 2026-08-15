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

/// The two drive-menu actions a user can reach a half-covered volume with.
/// Their own file, over this file's `Drive` fixture.
mod menu_actions;

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

/// The other half of the stamp's story. A build whose exclusion policy changed
/// can't trust a row in the index, and during the phased window nothing else would
/// ever repair that: every phase would re-walk the whole scope and never re-stamp,
/// so the volume would never converge again.
#[test]
fn a_changed_exclusion_fingerprint_rebuilds_a_phased_index() {
    let drive = Drive::new(
        "phased-fingerprint",
        |root| {
            std::fs::create_dir_all(root.join("kept")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    assert!(drive.meta("scan_completed_at").is_some(), "precondition: it completed");

    // A build with a different exclusion policy, as the index records it.
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::update_meta(&conn, crate::indexing::store::EXCLUSION_POLICY_KEY, "some-older-policy")
            .expect("stamp an older policy");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("a partial index, as a relaunch would find it");
    }

    drive.restart();
    drive.wait_for_the_machine();

    assert_eq!(
        drive.meta(crate::indexing::store::EXCLUSION_POLICY_KEY).as_deref(),
        Some(crate::indexing::scanner::exclusion_policy_fingerprint().as_str()),
        "the rebuild re-stamps, so coverage answers mean something again"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the rebuilt index converges"
    );
}

/// A truncating rescan under the machine would blank rows it is still writing and
/// make the sizes the user has been watching appear vanish again. `start_scan`'s
/// own reconcile-or-truncate predicate is what makes it a TRUNCATE: the index has
/// rows and no `scan_completed_at`, which is "a partial that never finished".
///
/// ⚠️ The guard has to hold BETWEEN frontier roots too, where no walk is running —
/// the stitch produces 50–150 roots per phase, so those gaps are most of the run.
/// That is why it asks whether the machine has WORK, not whether a walk is live.
#[test]
fn start_scan_refuses_while_a_phase_is_active() {
    let drive = Drive::new(
        "phased-refuses-rescan",
        |root| {
            // Enough ground that the machine is provably still working a moment
            // after it is handed the volume; the assertion below says so out loud
            // rather than passing vacuously if that ever stops being true.
            for outer in 0..20 {
                for inner in 0..20 {
                    std::fs::create_dir_all(root.join(format!("d{outer}/e{inner}"))).expect("dirs");
                }
            }
        },
        &[],
    );
    drive.start();

    // The machine holds the volume from the moment it is handed over until it has
    // nothing queued, so this is a refusal rather than a race.
    assert!(
        drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning),
        "precondition: the machine has work the moment the volume is handed to it"
    );
    let epoch = drive.current_epoch();
    let outcome = crate::indexing::lifecycle::state::force_scan(drive.volume_id);
    assert!(outcome.is_ok(), "the request is answered, not an error: {outcome:?}");
    // The durable evidence, and the one that doesn't race the machine's own
    // event: every `start_scan` bumps `current_epoch` before it walks, so an
    // unchanged epoch means no second run got past the guard.
    assert_eq!(
        drive.current_epoch(),
        epoch,
        "❌ a second run started over the top of the machine, which would have truncated the index"
    );

    drive.wait_for_the_machine();
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the machine finished what it was doing"
    );
    assert_eq!(drive.scans_started(), 1, "one run, start to finish");
}

/// The bounded-progress rule, and the reason completion can be a pure function of
/// the database. A directory a walk gave up on is never marked listed, so on its
/// own it would sit in the frontier forever and NOTHING hanging off completion
/// would ever happen: the stamps, the ledger heal, the sweep keys, the branch
/// collapse, the media kick, freshness. The cause is what takes it out.
///
/// Staged the way a walk stages it — the same `MarkDirsUnreadable` message the
/// walker sends — because what is under test is the completion rule, not the three
/// producers of the cause.
#[test]
fn a_permanently_timing_out_directory_still_lets_completion_happen() {
    completion_survives("phased-timed-out", crate::indexing::store::UnreadableCause::Abandoned);
}

/// The same rule for the other shape it arrives in: a subtree the walker's
/// consecutive-failure budget pruned without reading. It carries the same cause
/// and has to behave the same way.
#[test]
fn a_subtree_pruned_by_the_failure_budget_still_lets_completion_happen() {
    completion_survives("phased-pruned", crate::indexing::store::UnreadableCause::Abandoned);
}

fn completion_survives(volume_id: &'static str, cause: crate::indexing::store::UnreadableCause) {
    let drive = Drive::new(
        volume_id,
        |root| {
            std::fs::create_dir_all(root.join("readable")).expect("dirs");
            std::fs::create_dir_all(root.join("unreadable/inside")).expect("dirs");
        },
        &[],
    );
    // Stitch the tree root so `unreadable` has a row, then condemn it exactly as a
    // walk that couldn't read it would.
    drive.start();
    drive.wait_for_the_machine();
    let id = drive
        .id_of(&drive.path("unreadable"))
        .expect("the walk wrote a row for it");
    drive.write(WriteMessage::MarkDirsUnreadable { ids: vec![id], cause });
    drive.write(WriteMessage::MarkDirsListed {
        ids: vec![id],
        epoch: 0,
    });
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("a partial index, as a relaunch would find it");
    }

    drive.restart();
    drive.wait_for_the_machine();

    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "ground no walk can read is not frontier, so it doesn't hold completion open"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and the volume completes with a hole in it, which is the honest answer"
    );
}

/// Everything a completed volume owes that nothing else would ever do. The
/// `dir_stats` ledger heal is armed on every launch and disarmed only by a full
/// `ComputeAllAggregates`, which cover walks never send; the sweep ledger is seeded
/// from meta only at launch, so without these keys the first shallow anchor after
/// completion triggers a full sweep nobody asked for.
#[test]
fn completion_pays_the_ledger_and_seeds_the_sweep_keys() {
    let drive = Drive::new(
        "phased-completion-owes",
        |root| {
            std::fs::create_dir_all(root.join("one")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive
            .meta(crate::indexing::reconcile::reconciler::SHALLOW_SWEEP_AT_KEY)
            .is_some(),
        "the sweep window starts now, or the next shallow anchor sweeps the whole drive"
    );
    assert_eq!(
        drive
            .meta(crate::indexing::reconcile::reconciler::SHALLOW_COALESCED_KEY)
            .as_deref(),
        Some("0"),
        "and the coalesced count starts over"
    );
    let conn = IndexStore::open_read_connection(&drive.db_path()).expect("read connection");
    assert!(
        IndexStore::ledger_heal_done(&conn).expect("the ledger marker"),
        "`PayLedgerIfUnpaid` is the only thing that ever pays the armed heal"
    );
    assert!(
        drive.meta("total_entries").is_some_and(|value| value != "0"),
        "and the calibration a later run's ETA is built from is recorded"
    );
}

/// The sequence fires on the absent→present transition and never again. The
/// machine takes stock after EVERY drain, so a missing guard would re-run it
/// several times in one run — rewriting the sweep keys and pushing the 24-hour
/// window forward each time, which is the mirror of the bug the sweep ledger
/// exists to prevent.
#[test]
fn the_completion_sequence_runs_once_however_often_the_machine_takes_stock() {
    let drive = Drive::new(
        "phased-completes-once",
        |root| {
            // Several frontier roots, so the run drains and takes stock repeatedly.
            for name in ["a", "b", "c", "d"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    drive.meta("scan_completed_at").expect("it completed");

    assert_eq!(
        drive
            .events
            .kinds_for(drive.volume_id)
            .iter()
            .filter(|kind| **kind == crate::indexing::events::IndexEventKind::ScanComplete)
            .count(),
        1,
        "one completion per run, however many times the machine asked the question"
    );

    // And a relaunch of a COMPLETED volume never reaches the machine at all: it
    // takes today's reconcile-in-place path, which leaves the rows standing.
    //
    // ⚠️ Waited on the DURABLE marker, ❌ not on `scanning`. That path clears the
    // marker before it walks and re-stamps at the end, and its completion handler
    // drops the flag BEFORE the meta write reaches the writer — a window Linux
    // loses regularly.
    let rows = drive.entry_count();
    drive.restart();
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(30),
        "the relaunch to record its own completion",
        || drive.meta("scan_completed_at").is_some(),
    );
    assert!(drive.entry_count() >= rows, "and it never blanks the index");
}

/// **What the user is left looking at.** The completion sequence queues the
/// `dir_stats` ledger heal, and its full `ComputeAllAggregates` streams progress
/// for as long as it runs (18.8 s over a real `/`). A status surface reopens on a
/// progress tick and only a terminal event closes it, so a terminal fired BEFORE
/// that aggregate leaves the corner hourglass, every folder row's size hourglass,
/// and the step checklist lit for the rest of the session.
#[test]
fn nothing_aggregates_after_the_volume_says_aggregation_is_done() {
    use crate::indexing::events::IndexEventKind;

    let drive = Drive::new(
        "phased-terminal-aggregation",
        |root| {
            for name in ["a", "b"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    let kinds = drive.events.kinds_for(drive.volume_id);
    let terminal = kinds
        .iter()
        .position(|kind| *kind == IndexEventKind::AggregationComplete)
        .expect("a completed volume reports aggregation as done");
    let last_tick = kinds
        .iter()
        .rposition(|kind| *kind == IndexEventKind::AggregationProgress)
        .expect("the ledger heal really does stream progress, or this test proves nothing");

    assert!(
        last_tick < terminal,
        "the terminal event is the LAST word on aggregation: a tick after it reopens a step \
         nothing closes again (last tick at {last_tick}, terminal at {terminal})"
    );
}

/// The phase header is the ORDER made visible, and the order is the whole
/// feature. A folder the user opens mid-run is covered as its own phase and
/// announces itself, so without a re-assertion afterwards the header names that
/// interlude for the rest of the run — "Indexing the folders you use most" while
/// the machine is actually walking the whole drive.
#[test]
fn the_outer_phase_says_so_again_after_a_visited_root_interrupts_it() {
    use crate::indexing::events::IndexEventKind;

    let drive = Drive::with_host(
        "phased-phase-reasserts",
        |root| {
            for name in ["a", "b", "zzz-visited"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |host, root| {
            host.note_open_listing("phased-phase-reasserts", root.join("zzz-visited"));
        },
        &[],
        true,
    );
    drive.start();
    drive.wait_for_the_machine();

    let announced: Vec<IndexEventKind> = drive
        .events
        .kinds_for(drive.volume_id)
        .into_iter()
        .filter(|kind| {
            matches!(
                kind,
                IndexEventKind::PriorityCoverageStarted
                    | IndexEventKind::HomeCoverageStarted
                    | IndexEventKind::WholeVolumeCoverageStarted
            )
        })
        .collect();

    assert!(
        announced.contains(&IndexEventKind::PriorityCoverageStarted),
        "the folder the user is looking at really did earn a phase of its own, \
         or there is no interlude here to re-assert after ({announced:?})"
    );
    assert_eq!(
        announced.last(),
        Some(&IndexEventKind::WholeVolumeCoverageStarted),
        "and the last thing announced is what the machine is actually walking ({announced:?})"
    );
}

/// The early signal, and its blast radius. Photo search and folder importance only
/// need `$HOME`; waiting for the rest of the drive is minutes of the most visibly
/// valuable feature sitting idle on a first run. ⚠️ It says nothing about
/// freshness: the volume genuinely isn't covered yet.
#[test]
fn home_coverage_fires_the_early_media_signal_without_claiming_fresh() {
    let drive = Drive::new(
        "phased-home-signal",
        |root| {
            std::fs::create_dir_all(root.join("home/Documents")).expect("dirs");
            // The rest of the drive, which the signal must not wait for.
            std::fs::create_dir_all(root.join("zzz-elsewhere/deep")).expect("dirs");
        },
        &[],
    );
    let _home = set_home_override(drive.tree.path().join("home"));
    let mut signal = crate::indexing::lifecycle::lifecycle_bus::subscribe_home_covered(drive.volume_id);

    drive.start();
    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(30), "home to read as covered", || {
        drive.meta(super::HOME_COVERED_AT_KEY).is_some()
    });

    assert!(*signal.borrow_and_update(), "the one subscriber hears about it");
    drive.wait_for_the_machine();
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and the drive still completes on its own rule afterwards"
    );
}

/// The admission the early kick lives or dies on. `ready_volumes_with_kind` only
/// WIRES subscriptions, and a relaunch mid-coverage finds a volume that is
/// home-covered but not `Fresh` — so without this the early kick works on the first
/// run and never again.
#[test]
fn a_relaunch_mid_coverage_still_wires_the_media_subscriptions() {
    let drive = Drive::new(
        "phased-relaunch-wires",
        |root| {
            std::fs::create_dir_all(root.join("home/Documents")).expect("dirs");
        },
        &[],
    );
    let _home = set_home_override(drive.tree.path().join("home"));
    drive.start();
    drive.wait_for_the_machine();

    // What a relaunch mid-coverage looks like: home covered, the drive not.
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }
    crate::indexing::lifecycle::state::apply_freshness_event(
        drive.volume_id,
        crate::indexing::lifecycle::freshness::FreshnessEvent::ScanStarted,
    );

    let ready = crate::indexing::lifecycle::state::ready_volumes_with_kind();
    assert!(
        ready.iter().any(|(vid, _)| vid == drive.volume_id),
        "a home-covered volume is admitted even though it is honestly not Fresh: {ready:?}"
    );
}

/// Both switches keep outranking everything, and they are asked per phase and per
/// root rather than only at launch — so turning drive indexing off stops the
/// walking rather than the next launch.
#[test]
fn master_off_runs_nothing() {
    // The handle's own guard puts the process-wide switch back when the fixture
    // drops, so this can't leak into another test.
    let drive = Drive::with_host(
        "phased-master-off",
        |root| {
            std::fs::create_dir_all(root.join("one")).expect("dirs");
        },
        |_, _| {},
        &[],
        false,
    );

    drive.start();

    assert!(
        drive.meta("scan_completed_at").is_none(),
        "nothing indexes when the master switch is off"
    );
    assert_eq!(drive.scans_started(), 0, "and no run is announced");
}

/// A stopped machine leaves what it covered covered, and a restart adds to it
/// rather than starting over. This is the property today's truncate-and-rebuild
/// first scan can't have, and the first reason the whole design exists.
#[test]
fn rows_survive_a_stopped_and_restarted_machine() {
    let drive = Drive::new(
        "phased-survives-restart",
        |root| {
            for name in ["a", "b", "c"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    let after_first = drive.entry_count();
    let epoch = drive.current_epoch();
    assert!(after_first > 1, "precondition: the first run wrote rows");

    // What a quit mid-coverage leaves behind: rows, and no completion marker.
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }
    drive.restart();
    drive.wait_for_the_machine();

    assert!(
        drive.entry_count() >= after_first,
        "a restart adds to the index; ❌ it never blanks it"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "the resumed run confirms what is covered and stamps it"
    );
    let _ = epoch;
}

// ── What a launch does with the index it finds ────────────────────────

/// The routing table, end to end, over the two shapes that look identical from a
/// row count: rows and no completion marker, told apart by whether anything
/// records which ground those rows cover.
///
/// The pure table itself is `manager/launch_route.rs`; these two run the real
/// path and check what actually happened to the database.
#[test]
fn a_stopped_phased_index_comes_back_with_its_rows() {
    let drive = Drive::new(
        "phased-resume-keeps-rows",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ a partially covered volume must come back as a partially covered volume: the machine ADDS \
         to what the last session bought, it never throws it away"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "and the resumed run confirms what is covered and stamps it"
    );
}

/// Its opposite number. The same rows, minus the record of which ground they
/// cover, is a first BULK scan somebody interrupted — and nothing can watch that
/// ground or mark it stale, so resuming into it would render last session's sizes
/// as CURRENT with nothing having verified them. It goes.
#[test]
fn a_legacy_interrupted_partial_is_thrown_away_and_rebuilt() {
    let drive = Drive::new(
        "phased-legacy-partial",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();
    drive.forget_the_covered_branches();
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        !drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ rows nothing can account for are rebuilt, not walked on top of"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the rebuilt index converges, by the same machine"
    );
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "all the way to its completion marker"
    );
}

/// Turning drive indexing off and back on used to be a truncate door: the resume
/// always went to `start_scan`, which reads rows-without-a-completion-marker as a
/// partial that never finished and blanks it.
///
/// ⚠️ Only the BOOT DISK comes back through `drives_to_resume` while its first
/// index is unfinished; an external drive's per-drive intent is
/// `persisted_scan_completed`, which a drive mid-first-index doesn't have yet.
/// That gap predates the phases (the same drive interrupted mid-bulk-scan was
/// forgotten identically) and is called out in `../DETAILS.md`. What is under test
/// here is the routing a resume takes once it happens.
#[test]
fn a_master_switch_cycle_resumes_the_phases_instead_of_rebuilding() {
    let drive = Drive::new(
        "phased-master-cycle",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    // Off stops every running index, exactly as the settings switch does.
    drive.index.set_indexing_enabled(false);
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();

    drive.index.set_indexing_enabled(true);
    drive.start();
    drive.wait_for_the_machine();

    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ the master switch coming back on must not cost the drive what it already covered"
    );
}

/// The AUTOMATIC rescan door: a coalesced shallow `MustScanSubDirs`, a replay that
/// couldn't roll forward, an ingestion backlog. All three land on
/// `perform_registry_rescan`, which used to call `start_scan` unconditionally — so
/// any of them could blank a half-built index without a person involved.
///
/// (A branch-confined volume never routes a shallow anchor here at all, which is
/// its own guard and its own test in `../../reconcile/reconciler/tests/`. This one
/// says the door is closed even when something does reach it.)
#[test]
fn an_automatic_rescan_restarts_the_phases_instead_of_truncating() {
    let drive = Drive::new(
        "phased-automatic-rescan",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    // Cleared before the restart too, or the resume takes the COMPLETED route —
    // a reconcile in place, which re-lists the tree and would take the ghost with
    // it before this test got to its own question.
    drive.forget_the_completion_marker();
    drive.start();
    drive.wait_for_the_machine();

    // A volume the machine hasn't finished, which is the state the door is
    // dangerous in: rows, and no completion marker to stop `start_scan` reading
    // them as a partial to blank.
    drive.forget_the_completion_marker();
    let epoch = drive.current_epoch();
    crate::indexing::host::runtime::block_on(crate::indexing::lifecycle::manager::perform_registry_rescan(
        drive.volume_id,
        "ingestion backlog",
    ));
    drive.wait_for_the_machine();

    assert_eq!(
        drive.current_epoch(),
        epoch,
        "❌ no truncating (re)scan ran: every one of them bumps the epoch before it walks"
    );
    assert!(
        drive.ghost_survived("covered/inner", "last-session.txt"),
        "❌ and the rows the machine had already covered are still there"
    );
    // The door stops the watcher and the live loop on its way in, expecting the
    // full scan it used to reach to start fresh ones. This volume's frontier is
    // already empty, so no walk runs and nothing else would put one back.
    assert!(
        crate::indexing::lifecycle::state::is_watching_for_test(drive.volume_id),
        "❌ covered ground the drive still serves must not be left with nothing watching it"
    );
}

/// The escape hatch's own row, which is the one nobody would write down. With the
/// switch off there is no phase machine to resume into, so a phased partial takes
/// today's truncating rebuild — self-healing, and what the person who flipped it
/// asked for.
#[test]
fn the_kill_switch_gives_a_phased_partial_back_to_the_bulk_scan() {
    let drive = Drive::new(
        "phased-kill-switch",
        |root| {
            std::fs::create_dir_all(root.join("covered/inner")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();

    drive.stop();
    drive.plant_a_ghost("covered/inner", "last-session.txt");
    drive.forget_the_completion_marker();

    // Off, exactly as a launch would find it. Restored when the guard drops.
    let _killed = crate::indexing::lifecycle::phases::install_for_test(false);
    drive.start();
    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(30), "the bulk scan to finish", || {
        drive.meta("scan_completed_at").is_some()
    });

    assert!(
        !drive.ghost_survived("covered/inner", "last-session.txt"),
        "the bulk build truncates first, which is the behavior the switch restores"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the drive is indexed, by the path that shipped before the phases"
    );
}

/// The resume-honesty property, and the reason the machine's first walk waits for
/// `resume_branch_watch`. A session that can't replay the gap since the last one
/// doesn't know what happened to the ground it covered — so the rows stay
/// (Decision 5: a covered-but-stale subtree is not re-walked) and the epoch bump is
/// what makes the read side RENDER them as stale instead of confidently current.
///
/// ⚠️ Start the machine before that resume and the bump never fires: its first walk
/// starts a watcher, and `ensure_branch_watch` returns early when one is already
/// running.
#[test]
fn a_relaunch_with_no_replayable_journal_bumps_the_epoch() {
    let drive = Drive::new(
        "phased-resume-honesty",
        |root| {
            std::fs::create_dir_all(root.join("covered")).expect("dirs");
        },
        &[],
    );
    drive.start();
    drive.wait_for_the_machine();
    let before = drive.current_epoch();
    let rows = drive.entry_count();

    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::delete_meta(&conn, "scan_completed_at").expect("clear the completion marker");
    }
    drive.restart();
    drive.wait_for_the_machine();

    assert!(
        drive.current_epoch() > before,
        "a session that can't replay the gap says so, rather than claiming the rows are current"
    );
    assert!(
        drive.entry_count() >= rows,
        "and it says it by bumping the epoch, ❌ never by throwing the rows away"
    );
}

/// A folder the user has open when indexing starts is covered as its own phase,
/// ahead of the rest of the drive. The rank ORDER itself is pinned by the queue's
/// own tests; this is the wiring: the poll reaches the machine, and the machine
/// gives what it finds a turn.
#[test]
fn a_visited_root_is_taken_between_frontier_roots() {
    let build = |root: &Path| {
        for name in ["a", "b", "zzz-visited"] {
            std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
        }
    };
    let phases_without_a_visit = {
        let drive = Drive::new("phased-no-visit", build, &[]);
        drive.start();
        drive.wait_for_the_machine();
        drive.phase_changes()
    };

    let drive = Drive::with_host(
        "phased-with-visit",
        build,
        |host, root| {
            host.note_open_listing("phased-with-visit", root.join("zzz-visited"));
        },
        &[],
        true,
    );
    drive.start();
    drive.wait_for_the_machine();

    assert_eq!(
        drive.phase_changes(),
        phases_without_a_visit + 1,
        "the folder the user is looking at earns a phase of its own"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the rest of the drive still gets covered"
    );
}

/// The pump the machine owns for its whole lifetime, doing both its jobs: it
/// reports progress, and it is the one legal place the machine hears where the
/// user is looking (the seam's contract is "the 500 ms tick, ❌ nothing faster",
/// and frontier-root boundaries are far faster than that).
#[test]
fn the_progress_pump_reports_and_polls_where_the_user_is_looking() {
    let _serialized = crate::indexing::handle::test_lock();
    let dir = tempfile::tempdir().expect("temp db dir");
    let db_path = dir.path().join("pump-test.db");
    IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

    let host = crate::indexing::host::policy::FakeHostPolicy::shared();
    host.note_open_listing("pump-volume", "/somewhere/the/user/is");
    let (_index, _installed) = crate::indexing::handle::Index::builder()
        .data_dir(dir.path())
        .host(std::sync::Arc::clone(&host) as std::sync::Arc<_>)
        .install_for_test();

    let events = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
    let progress = std::sync::Arc::new(crate::indexing::scanner::ScanProgress::new());
    progress.entries_scanned.fetch_add(7, Ordering::Relaxed);
    let visits = std::sync::Arc::new(super::VisitLog::new());
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    crate::indexing::lifecycle::progress_reporter::ScanProgressReporter::new(
        std::sync::Arc::clone(&progress),
        writer.clone(),
        std::sync::Arc::clone(&events) as std::sync::Arc<dyn crate::indexing::events::EventSink>,
        "pump-volume".to_string(),
        crate::indexing::writer::AggSource::Sql,
    )
    .noting_visits(std::sync::Arc::clone(&visits))
    .spawn(std::sync::Arc::clone(&done));

    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(5), "the pump to tick", || {
        !events.kinds_for("pump-volume").is_empty()
    });
    done.store(true, Ordering::Relaxed);

    assert!(
        events
            .kinds_for("pump-volume")
            .contains(&crate::indexing::events::IndexEventKind::ScanProgress),
        "the progress stream is alive"
    );
    assert_eq!(
        visits.take(),
        Some(PathBuf::from("/somewhere/the/user/is")),
        "and the machine hears where the user is looking"
    );
    writer.shutdown();
}

/// The same pump, over a REAL machine run, answering the question the isolated
/// test above can't: whose lifetime is it?
///
/// It is the machine's, and everything riding the 500 ms tick depends on that —
/// the progress stream, the `open_listings` poll, and mid-scan partial
/// aggregation, which is what makes a size appear for the folder somebody is
/// looking at while the walker is deep inside a different frontier root. One
/// reporter per walk would die and restart 50–150 times a phase and tick almost
/// never: a walk over a frontier root usually finishes in milliseconds, well
/// inside the reporter's first sleep.
///
/// The gap between two frontier roots is where that difference shows, so the test
/// holds one open from inside the sink and watches what still arrives.
#[test]
fn the_progress_pump_outlives_the_walks_it_reports_on() {
    let recorder = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
    let watcher = std::sync::Arc::new(PauseBetweenWalks::new(
        "phased-pump-outlives",
        std::sync::Arc::clone(&recorder),
    ));
    let drive = Drive::assembled(
        "phased-pump-outlives",
        |root| {
            // Three frontier roots under the volume root, so there are two gaps
            // between walks and the first one has walks after it.
            for name in ["a", "b", "c"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |_, _| {},
        &[],
        true,
        std::sync::Arc::clone(&watcher) as std::sync::Arc<dyn crate::indexing::events::EventSink>,
        recorder,
    );

    drive.start();
    drive.wait_for_the_machine();

    assert!(
        watcher.held_a_gap_open(),
        "precondition: a walk ended and this test held the moment after it open"
    );
    assert!(
        watcher.ticks_in_the_gap() > 0,
        "❌ the pump died with the walk it was reporting on: nothing ticked in {:?} between frontier roots, \
         so nothing would refresh the size of the folder the user is looking at until the run ends",
        THE_GAP
    );
    assert!(
        watcher.walks_after_the_gap() > 0,
        "and the gap was BETWEEN frontier roots, not after the last one"
    );
}

/// How long one between-walks gap is held open. Three of the reporter's 500 ms
/// ticks fit, so a machine-lifetime pump lands at least two of them here however
/// the sleeps happen to line up.
const THE_GAP: std::time::Duration = std::time::Duration::from_millis(1_500);

/// Holds the machine still in the gap after its FIRST walk, and counts what
/// arrives while it waits.
///
/// The machine emits on its own thread, synchronously, so blocking inside `emit`
/// IS a between-frontier-roots moment: the walk has finished, `walking` is already
/// false, and the next walk can't start until this returns. Anything whose
/// lifetime was that walk is gone by now; anything whose lifetime is the machine
/// keeps going, and that is what the counters below tell apart.
struct PauseBetweenWalks {
    volume_id: &'static str,
    recorder: std::sync::Arc<crate::indexing::events::RecordingSink>,
    /// Set while the gap is being held open, so a tick landing in it is counted.
    holding: std::sync::atomic::AtomicBool,
    /// One gap is enough, and holding every one of them would only make the test
    /// slower.
    held: std::sync::atomic::AtomicBool,
    ticks: std::sync::atomic::AtomicUsize,
    walks_after: std::sync::atomic::AtomicUsize,
}

impl PauseBetweenWalks {
    fn new(volume_id: &'static str, recorder: std::sync::Arc<crate::indexing::events::RecordingSink>) -> Self {
        Self {
            volume_id,
            recorder,
            holding: std::sync::atomic::AtomicBool::new(false),
            held: std::sync::atomic::AtomicBool::new(false),
            ticks: std::sync::atomic::AtomicUsize::new(0),
            walks_after: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn held_a_gap_open(&self) -> bool {
        self.held.load(Ordering::Relaxed)
    }

    /// Progress events that arrived while no walk was running.
    fn ticks_in_the_gap(&self) -> usize {
        self.ticks.load(Ordering::Relaxed)
    }

    /// Walks that started after the gap, which is what makes it a gap rather than
    /// the end of the run.
    fn walks_after_the_gap(&self) -> usize {
        self.walks_after.load(Ordering::Relaxed)
    }
}

impl crate::indexing::events::EventSink for PauseBetweenWalks {
    fn emit(&self, event: crate::indexing::events::IndexEvent) {
        use crate::indexing::events::IndexEventKind;
        let mine = event.volume_id() == Some(self.volume_id);
        let kind = event.kind();
        self.recorder.emit(event);
        if !mine {
            return;
        }
        match kind {
            IndexEventKind::ScanProgress if self.holding.load(Ordering::Relaxed) => {
                self.ticks.fetch_add(1, Ordering::Relaxed);
            }
            IndexEventKind::CoverageBranchStarted if self.held.load(Ordering::Relaxed) => {
                self.walks_after.fetch_add(1, Ordering::Relaxed);
            }
            IndexEventKind::CoverageBranchEnded if !self.held.swap(true, Ordering::Relaxed) => {
                self.holding.store(true, Ordering::Relaxed);
                // allowed-test-sleep: the gap IS the thing under test. Nothing the
                // machine does between two frontier roots can be waited on, because
                // the property is that something keeps happening while it does
                // nothing at all.
                std::thread::sleep(THE_GAP);
                self.holding.store(false, Ordering::Relaxed);
            }
            _ => {}
        }
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

// ── Measuring a real home folder ─────────────────────────────────────

/// What covering a REAL home folder costs, and when the early signal fires inside
/// that. `#[ignore]`d: it walks the machine's actual `$HOME` (into a temp index),
/// takes minutes, and prints numbers rather than asserting any.
///
/// It exists to answer one question the design rests on: whether `~/Library` is
/// enough of home's wall clock that the early media kick has to skip it. Run it
/// with `CMDR_PHASE_HOME` to point it somewhere smaller.
///
/// ```sh
/// cargo test -p cmdr-index --release --lib -- --ignored --nocapture \
///   indexing::lifecycle::phases::tests::how_long_home_takes
/// ```
#[test]
#[ignore = "walks a real home folder; run it explicitly"]
fn how_long_home_takes() {
    let home = std::env::var("CMDR_PHASE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().expect("a home directory"));

    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
    volumes.register(
        "phased-measure",
        std::sync::Arc::new(
            cmdr_fs::volume::InMemoryVolume::new("Measured")
                .with_root(&home)
                .with_local_fs_access(),
        ),
    );
    let (index, _installed) = crate::indexing::handle::Index::builder()
        .data_dir(data.path())
        .volumes(std::sync::Arc::clone(&volumes) as std::sync::Arc<_>)
        .host(crate::indexing::host::policy::FakeHostPolicy::shared() as std::sync::Arc<_>)
        .indexing_enabled(Some(true))
        .install_for_test();
    let _home_override = set_home_override(home.clone());

    let db_path = data.path().join("index-phased-measure.db");
    // Written the way `phased_bench` writes its numbers: `writeln!` to a stderr
    // handle, so a measurement harness can report without the `print_stdout` lint
    // that keeps production code on the logger.
    use std::io::Write;
    let mut out = std::io::stderr();
    let started = std::time::Instant::now();
    crate::indexing::host::runtime::block_on(index.start_volume("phased-measure")).expect("indexing starts");

    let mut home_covered = None;
    loop {
        let meta = |key: &str| {
            IndexStore::open_read_connection(&db_path)
                .ok()
                .and_then(|conn| IndexStore::get_meta(&conn, key).ok().flatten())
        };
        if home_covered.is_none() && meta(super::HOME_COVERED_AT_KEY).is_some() {
            home_covered = Some(started.elapsed());
            let _ = writeln!(
                out,
                "home covered (minus the deferred folder) after {:?}",
                started.elapsed()
            );
        }
        if meta("scan_completed_at").is_some() {
            let _ = writeln!(out, "all of {} covered after {:?}", home.display(), started.elapsed());
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(600) {
            let _ = writeln!(out, "gave up after 10 minutes");
            break;
        }
        // allowed-test-sleep: the sampler IS the measurement. It watches two markers
        // land at different moments over minutes, which is the number this prints;
        // a wait-on-one-condition helper can't see the first one go by.
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let entries = IndexStore::open_read_connection(&db_path)
        .ok()
        .and_then(|conn| IndexStore::get_entry_count(&conn).ok())
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "{}; the early signal arrived {}",
        cmdr_fs::pluralize::pluralize_with(entries, "entry", "entries"),
        match home_covered {
            Some(at) => format!("{at:?} in"),
            None => "never".to_string(),
        }
    );
    let _ = index.forget_volume("phased-measure");
}
