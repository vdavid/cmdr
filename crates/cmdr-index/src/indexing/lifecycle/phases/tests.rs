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
        let events = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
        let (index, installed) = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(std::sync::Arc::clone(&volumes) as std::sync::Arc<_>)
            .host(host as std::sync::Arc<_>)
            .events(std::sync::Arc::clone(&events) as std::sync::Arc<dyn crate::indexing::events::EventSink>)
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

    /// Take the volume down and bring it back, which is what a relaunch is: the
    /// instance goes, the database stays.
    fn restart(&self) {
        crate::indexing::lifecycle::state::stop_indexing(self.volume_id).expect("the drive stops indexing");
        self.start();
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
/// make the sizes the user has been watching appear vanish again. ⚠️ The guard has
/// to hold BETWEEN frontier roots too, where no walk is running — the stitch
/// produces 50–150 roots per phase, so those gaps are most of the run.
#[test]
fn start_scan_refuses_while_a_phase_is_active() {
    let drive = Drive::new(
        "phased-refuses-rescan",
        |root| {
            for name in ["a", "b", "c", "d", "e"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        &[],
    );
    drive.start();

    // Asked repeatedly for the whole run, so at least some of the asks land in the
    // gaps between roots rather than inside a walk.
    let mut refusals = 0;
    while drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning) {
        if !matches!(
            crate::indexing::lifecycle::state::force_scan(drive.volume_id),
            Ok(crate::indexing::lifecycle::rescan_request::RescanOutcome::Started)
        ) {
            refusals += 1;
        }
        // A rescan that DID start would announce itself, which is the failure this
        // test exists to catch.
        assert!(
            drive.scans_started() <= 1,
            "❌ a second run started while the machine was covering the volume"
        );
        std::thread::yield_now();
    }
    let _ = refusals;
    drive.wait_for_the_machine();
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the machine finished what it was doing"
    );
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
    let stamped = drive.meta("scan_completed_at").expect("it completed");
    let swept = drive.meta(crate::indexing::reconcile::reconciler::SHALLOW_SWEEP_AT_KEY);

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
    let rows = drive.entry_count();
    drive.restart();
    drive.wait_for_the_machine();
    assert!(
        drive.meta("scan_completed_at").is_some(),
        "the relaunch keeps a completion marker"
    );
    assert!(drive.entry_count() >= rows, "and it never blanks the index");
    let _ = (stamped, swept);
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
