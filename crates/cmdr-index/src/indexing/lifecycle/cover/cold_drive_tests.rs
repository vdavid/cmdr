//! The cover driver over a drive nobody has ever indexed, driven through the
//! PUBLIC handle rather than the internals.
//!
//! Everything here runs the real activation: the walk stands the database, epoch,
//! writer, and read handles up for itself, and the assertions are about what a
//! caller can observe afterwards (coverage, freshness, branches, whether a scan
//! ever ran). `tests.rs` next door drives the same walk over an index that already
//! exists.

use std::path::PathBuf;
use std::sync::Arc;

use super::test_support::drain;
use super::*;

/// A drive with no index, as the host reports one, plus the handle to reach it
/// through.
///
/// Everything behind the handle is process-wide, so this holds the test lock for
/// its whole life and forgets the volume on the way out; a leaked registry entry
/// would follow the next test into its own drive.
/// ⚠️ Field order IS the teardown order: struct fields drop in declaration
/// order, so the seam guard has to come before the lock guard. The other way
/// round, this restores the previous data directory AFTER releasing the lock —
/// over the top of whichever test took it next, which then fails with "no index
/// data directory configured".
struct ColdDrive {
    _installed: crate::indexing::handle::TestInstallGuard,
    data: tempfile::TempDir,
    tree: tempfile::TempDir,
    index: crate::indexing::handle::Index,
    events: Arc<crate::indexing::events::RecordingSink>,
    volume_id: &'static str,
    _serialized: std::sync::MutexGuard<'static, ()>,
}

impl ColdDrive {
    /// A local drive as the host reports one: readable through the local
    /// filesystem, no smb2 session, a local mount. Its contents come off the real
    /// temp tree, because the LOCAL walker reads the disk rather than the volume.
    fn new(volume_id: &'static str) -> Self {
        Self::with_volume(volume_id, |volume| volume.with_local_fs_access())
    }

    /// The same drive with drive indexing turned OFF in settings, which is the
    /// master switch's `false`. The handle's own guard puts the process-wide
    /// atomic back when the fixture drops.
    fn with_indexing_disabled(volume_id: &'static str) -> Self {
        Self::build(volume_id, |volume| volume.with_local_fs_access(), Some(false))
    }

    /// The same, with the registered volume shaped by `describe` — a share, a
    /// phone, whatever the refusal under test needs.
    fn with_volume(
        volume_id: &'static str,
        describe: impl FnOnce(cmdr_fs::volume::InMemoryVolume) -> cmdr_fs::volume::InMemoryVolume,
    ) -> Self {
        Self::build(volume_id, describe, None)
    }

    fn build(
        volume_id: &'static str,
        describe: impl FnOnce(cmdr_fs::volume::InMemoryVolume) -> cmdr_fs::volume::InMemoryVolume,
        indexing_enabled: Option<bool>,
    ) -> Self {
        let serialized = crate::indexing::handle::test_lock();
        let data = tempfile::tempdir().expect("index data dir");
        // In the CWD rather than `/tmp`, for the reasons `Fixture` names.
        let tree = tempfile::Builder::new()
            .prefix("cmdr-cold-cover-")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("temp tree");

        let volumes = crate::indexing::host::volumes::FakeVolumeProvider::shared();
        volumes.register(
            volume_id,
            Arc::new(describe(
                cmdr_fs::volume::InMemoryVolume::new("Cold").with_root(tree.path()),
            )),
        );
        let events = Arc::new(crate::indexing::events::RecordingSink::new());
        let mut builder = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(Arc::clone(&volumes) as Arc<_>)
            .events(Arc::clone(&events) as Arc<dyn crate::indexing::events::EventSink>);
        if let Some(enabled) = indexing_enabled {
            builder = builder.indexing_enabled(Some(enabled));
        }
        let (index, installed) = builder.install_for_test();

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

    fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    /// This drive's index database, whether or not anything has created it yet.
    fn db_path(&self) -> PathBuf {
        self.data.path().join(format!("index-{}.db", self.volume_id))
    }

    /// How many full scans this drive has announced.
    fn scans_started(&self) -> usize {
        self.events
            .kinds_for(self.volume_id)
            .iter()
            .filter(|kind| **kind == crate::indexing::events::IndexEventKind::ScanStarted)
            .count()
    }

    /// Whether the drive's index holds a row for this absolute path.
    fn is_indexed(&self, path: &str) -> bool {
        let Ok(conn) = IndexStore::open_read_connection(&self.db_path()) else {
            return false;
        };
        let Some(relative) = IndexPathSpace::mount_rooted(self.path("")).index_relative(path) else {
            return false;
        };
        crate::indexing::store::resolve_path(&conn, &relative)
            .ok()
            .flatten()
            .is_some()
    }

    /// The epoch the drive's rows are being written against. A truncating rescan
    /// bumps it, so it reads as "something blanked this index" from outside.
    fn current_epoch(&self) -> u64 {
        let conn = IndexStore::open_read_connection(&self.db_path()).expect("read connection");
        IndexStore::read_current_epoch(&conn).expect("current epoch")
    }

    /// What the drive's own database says it walked, as stored (index-relative).
    fn persisted_branches(&self) -> Option<String> {
        let conn = IndexStore::open_read_connection(&self.db_path()).ok()?;
        IndexStore::get_meta(&conn, crate::indexing::watch::branches::COVERED_BRANCHES_KEY)
            .ok()
            .flatten()
            .filter(|stored| !stored.is_empty())
    }

    fn coverage(&self, path: &str) -> crate::indexing::read::coverage::CoverageMap {
        self.index
            .coverage(self.volume_id, path, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
    }

    /// Walk one scope to the end, waiting for the rows to land.
    fn cover(&self, scope: &str) -> CoverOutcome {
        let walk = self
            .index
            .cover(
                self.volume_id,
                vec![scope.to_string()],
                CoverageDimension::Listing,
                CancellationToken::new(),
            )
            .expect("the drive is walkable");
        let (_, outcome) = drain(walk);
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walked scope to read as covered",
            || {
                let covered = self.coverage(scope);
                covered.frontier.is_empty() && covered.permission_denied.is_empty() && covered.declined.is_empty()
            },
        );
        outcome
    }
}

impl Drop for ColdDrive {
    fn drop(&mut self) {
        let _ = self.index.forget_volume(self.volume_id);
    }
}

/// A drive nobody ever indexed, driven through the public handle: the walk
/// stands the whole index up (database, epoch, writer, the chain down to the
/// scope), covers exactly the folder it was pointed at, and claims NOTHING
/// else.
///
/// The second half is the load-bearing one. A bootstrap that claimed anything
/// beyond what it read would make the very next search skip ground nobody has
/// walked, and a search that quietly omits a folder is the bug this whole effort
/// exists to remove. So the volume root — the ancestor the bootstrap had to
/// materialize to reach the scope — must still read as uncovered afterwards.
#[test]
fn a_cold_volume_bootstraps_and_claims_only_what_it_walked() {
    let drive = ColdDrive::new("cover-cold-bootstrap-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    std::fs::create_dir_all(drive.tree.path().join("elsewhere")).expect("dirs");
    let scope = drive.path("scope");
    let volume_root = drive.path("");

    let cold = drive.coverage(&scope);
    assert_eq!(cold.frontier, vec![scope.clone()], "nothing is covered yet");
    assert_eq!(cold.token, crate::indexing::read::coverage::CoverageToken::UNINDEXED);

    let outcome = drive.cover(&scope);
    assert!(!outcome.cancelled);
    assert_eq!(outcome.roots_covered, 1, "the scope was covered");
    assert_eq!(outcome.entries_found, 3, "scope/ itself, inner/, and inner/found.txt");

    let whole_volume = drive.coverage(&volume_root);
    assert_eq!(
        whole_volume.frontier,
        vec![volume_root],
        "the volume root was materialized, not listed: nothing may claim coverage it didn't earn"
    );
    assert_ne!(
        whole_volume.token,
        crate::indexing::read::coverage::CoverageToken::UNINDEXED,
        "and the volume now has an index to answer from"
    );
}

/// A walk over a drive whose index is left over from an earlier session reads it
/// as Stale, never Fresh.
///
/// Fresh-on-launch is what a journal REPLAY earns, and a writer-only start
/// doesn't replay: nothing has been watching this volume, so its rows are
/// stale-but-visible. Claiming Fresh would make the badge say "authoritative"
/// over an index nobody has verified since the app was last open.
#[test]
fn a_walk_on_a_left_over_index_reads_it_as_stale() {
    let drive = ColdDrive::new("cover-cold-leftover-test");
    {
        // A local index a previous session completed, with nothing running it now.
        drop(IndexStore::open(&drive.db_path()).expect("open store"));
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write connection");
        IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp a completed scan");
    }

    // Started as the JOURNALED kind on purpose: the boot disk is the only kind
    // that can load Fresh at all, so it's the only one where this can go wrong.
    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::Local,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index up for a walk");

    assert_eq!(
        crate::indexing::lifecycle::state::get_freshness(drive.volume_id),
        Some(crate::indexing::lifecycle::freshness::Freshness::Stale),
        "a walk replays no journal, so it verifies nothing and may not claim Fresh"
    );
}

/// The second walk on a bootstrapped drive reuses the writer the first one stood
/// up, and the coverage the first one earned is still there.
///
/// One writer per database is the invariant: a second would allocate ids from its
/// own counter and inflate `dir_stats`, and a second bootstrap that re-prepared
/// the database would throw the first walk's ground away.
#[test]
fn a_second_walk_reuses_the_index_the_first_one_stood_up() {
    let drive = ColdDrive::new("cover-cold-second-walk-test");
    for name in ["first", "second"] {
        std::fs::create_dir_all(drive.tree.path().join(name)).expect("dirs");
        std::fs::write(drive.tree.path().join(name).join("f.txt"), "x").expect("file");
    }

    drive.cover(&drive.path("first"));
    drive.cover(&drive.path("second"));

    assert!(
        drive.coverage(&drive.path("first")).frontier.is_empty(),
        "the first walk's ground survived the second walk"
    );
}

/// Turning indexing on for a drive a search already walked runs the full scan
/// the person asked for, instead of no-opping against the index the walk left.
///
/// A walk registers an instance with no scan and no watcher behind it, so a bare
/// "this volume is already active" would swallow the request for exactly those.
/// A first scan someone stopped leaves the same shape, and had the same problem.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn turning_indexing_on_after_a_walk_still_scans_the_drive() {
    let drive = ColdDrive::new("cover-cold-then-enable-test");
    std::fs::create_dir_all(drive.tree.path().join("walked")).expect("dirs");
    std::fs::create_dir_all(drive.tree.path().join("never-walked")).expect("dirs");
    let volume_root = drive.path("");

    drive.cover(&drive.path("walked"));
    assert!(
        !drive.coverage(&volume_root).frontier.is_empty(),
        "precondition: one walked folder leaves the rest of the drive uncovered"
    );

    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("the drive starts indexing");

    // Waited on the DURABLE completion marker, not on the coverage answer: a walk
    // marks its directories listed well before `scan_completed_at` reaches the
    // database, so "the drive reads as covered" would let the second enable below
    // find a scan that hasn't finished recording itself yet (it did, on Linux).
    cmdr_fs::testing::wait_until_async(
        std::time::Duration::from_secs(20),
        "the full scan to finish and record itself",
        || drive.index.volume_status(drive.volume_id).scan_completed_at.is_some(),
    )
    .await;
    assert!(
        drive.coverage(&volume_root).frontier.is_empty(),
        "and the whole drive is covered now"
    );
    assert_eq!(drive.scans_started(), 1, "exactly one scan, not one per call");

    // And the other side of the same gate: a drive that HAS been indexed must not
    // be rescanned by an enable. On a real drive that's a full re-walk of
    // everything — minutes on a NAS — off one stray click.
    drive
        .index
        .start_volume(drive.volume_id)
        .await
        .expect("a second enable is a no-op");
    assert_eq!(drive.scans_started(), 1, "an indexed drive is left alone");
}

// ── Both indexing switches govern background work only ───────────────

/// ⚠️ A search walks a drive with drive indexing turned OFF, and that is
/// DELIBERATE (Decision 13). ❌ Don't "fix" it back into a refusal.
///
/// The master switch means "don't index anything on your own": no launch
/// auto-start, no per-drive enable, no reconnect resume. Searching is none of
/// those — it's a read the person in front of the app just asked for, and a walk
/// is what reading a folder Cmdr hasn't indexed IS. Refusing here wouldn't save
/// the user any work; it would only make the search return a wrong answer
/// silently, which is the exact bug this whole effort removes.
///
/// The switch keeps its teeth: nothing schedules a scan, nothing starts a
/// watcher, and the walk covers only the folder it was pointed at.
#[test]
fn a_search_walks_a_drive_with_the_master_switch_off() {
    let drive = ColdDrive::with_indexing_disabled("cover-master-switch-off-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    std::fs::create_dir_all(drive.tree.path().join("elsewhere")).expect("dirs");
    let scope = drive.path("scope");

    assert!(
        !crate::indexing::lifecycle::master::master_enabled(),
        "precondition: drive indexing is off in settings"
    );

    let outcome = drive.cover(&scope);
    assert!(!outcome.cancelled);
    assert_eq!(outcome.roots_covered, 1, "the search got the walk it asked for");
    assert_eq!(outcome.entries_found, 3, "scope/ itself, inner/, and inner/found.txt");

    assert_eq!(
        drive.scans_started(),
        0,
        "and the switch keeps its teeth: nothing indexed the drive uninvited"
    );
    assert!(
        !drive.coverage(&drive.path("")).frontier.is_empty(),
        "only the searched folder was walked, not the drive"
    );
}

/// The sticky per-drive veto reads the same way: it stops background indexing of
/// that drive, not a read someone asked for.
///
/// Its teeth are elsewhere — a vetoed drive gets no watcher (M11), so its walked
/// branches go stale and re-walk instead of staying live. Turning a search on
/// that drive into a wrong answer was never the point.
#[test]
fn a_search_walks_a_drive_the_user_turned_indexing_off_for() {
    let drive = ColdDrive::new("cover-user-disabled-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    drop(IndexStore::open(&drive.db_path()).expect("open store"));
    IndexStore::set_user_disabled(&drive.db_path(), true).expect("mark user_disabled");

    let outcome = drive.cover(&drive.path("scope"));
    assert_eq!(outcome.roots_covered, 1, "the search still got its answer");
    assert_eq!(outcome.entries_found, 2, "scope/ itself and found.txt");
    assert_eq!(drive.scans_started(), 0, "with no scan of the drive behind it");
}

// ── What the walk leaves watched ─────────────────────────────────────

/// The walk's other half of Decision 9: it writes down the ground it covered, on
/// the volume's own database, so the branch survives the app.
///
/// Without this the plan needs the expiry it replaced — a walked branch nothing
/// watches is a snapshot of a folder taken once, and the next session would have
/// to either re-walk it or serve rows it can't vouch for.
#[test]
fn a_walk_leaves_the_ground_it_covered_watched_and_written_down() {
    let drive = ColdDrive::new("cover-branch-watch-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);

    assert_eq!(
        crate::indexing::watch::branches::live_for(drive.volume_id).branch_paths(),
        [scope.as_str()],
        "the walked branch is the volume's to keep current now"
    );
    // `persist` hands the meta row to the async `IndexWriter` and `cover` only waits for
    // the COVERAGE to read as walked, so reading the database the instant the walk returns
    // races the writer thread. Wait for the row, then assert its exact contents.
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's branch to reach the database",
        || drive.persisted_branches().is_some(),
    );
    assert_eq!(
        drive.persisted_branches(),
        Some("/scope".to_string()),
        "and it's on the drive's own database, index-relative so a remount still finds it"
    );
}

/// The whole promise, through the real watcher: change a file inside walked
/// ground and the index follows; change one beside it and the index doesn't move.
///
/// Every other test here drives the admission rule directly. This one starts a
/// real `DriveWatcher` on a real drive and touches real files, which is the only
/// way to prove `ensure_branch_watch` starts something that works — the failure it
/// catches is a walked branch that reads live and is quietly frozen.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the fixture holds the process-wide seams for the whole test; holding it across the await IS the point"
)]
async fn a_change_inside_a_walked_branch_reaches_the_index_and_one_beside_it_does_not() {
    let drive = ColdDrive::new("cover-branch-live-test");
    std::fs::create_dir_all(drive.tree.path().join("walked")).expect("dirs");
    std::fs::create_dir_all(drive.tree.path().join("beside")).expect("dirs");
    std::fs::write(drive.tree.path().join("walked/already-there.txt"), "x").expect("file");
    let branch = drive.path("walked");

    drive.cover(&branch);
    assert!(
        !crate::indexing::watch::branches::live_for(drive.volume_id)
            .branch_paths()
            .is_empty(),
        "precondition: the walk left a branch to watch"
    );

    // Both drives created AFTER the walk, so neither is in the index yet. The
    // watcher is the only thing that can put either one there.
    std::fs::write(drive.tree.path().join("walked/appeared.txt"), "new").expect("file");
    std::fs::write(drive.tree.path().join("beside/appeared.txt"), "new").expect("file");

    let inside = drive.path("walked/appeared.txt");
    let outside = drive.path("beside/appeared.txt");
    cmdr_fs::testing::wait_until_async(
        std::time::Duration::from_secs(30),
        "the watcher to index the change inside the walked branch",
        || drive.is_indexed(&inside),
    )
    .await;
    assert!(
        !drive.is_indexed(&outside),
        "and the folder beside it stays this index's business only once a search walks it"
    );
}

/// A walk registers its ground when it starts and releases it when it ends, and
/// the release can't depend on what the registry is doing minutes later.
///
/// `force_scan` and `perform_registry_rescan` publish `ShuttingDown` for the whole
/// of a scan start, so a walk ending inside that window used to leave its branch
/// at `walks > 0` forever: `may_walk` false for that ground permanently, every
/// event for it buffered and never promoted, and the branch never absorbed.
#[test]
fn a_walk_that_finishes_while_the_manager_is_shutting_down_still_releases_its_branch() {
    use crate::indexing::watch::branches::{self, WatchScope};
    use crate::indexing::watch::watcher::{FsChangeEvent, FsEventFlags};

    let drive = ColdDrive::new("cover-branch-shutdown-finish-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    let scope = drive.path("scope");
    let branch = vec![scope.clone()];
    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::LocalExternal,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index up");

    crate::indexing::lifecycle::state::begin_branch_coverage(drive.volume_id, &branch);
    let watch = branches::live_for(drive.volume_id);
    assert!(
        watch.is_being_walked(Path::new(&scope)),
        "precondition: the ground is registered and its events wait for the walk"
    );
    let held = FsChangeEvent {
        path: drive.path("scope/arrived-mid-walk.txt"),
        event_id: 1,
        flags: FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    };
    let _ = WatchScope::Branches(Arc::clone(&watch)).admit(held);

    crate::indexing::lifecycle::state::while_shutting_down_for_test(drive.volume_id, || {
        crate::indexing::lifecycle::state::finish_branch_coverage(drive.volume_id, &branch);
    });

    assert!(
        !watch.is_being_walked(Path::new(&scope)),
        "the hold goes with the walk, whatever phase the registry was in"
    );
    assert_eq!(
        watch.take_promoted().events.len(),
        1,
        "and what it held is released, rather than buffering for the rest of the session"
    );
}

/// Clearing a drive's index takes its branches with it, because they describe
/// coverage that no longer exists.
///
/// No code does this: the set lives on the database the clear deletes, which is
/// why it lives there. The test is here so a later change that moves it somewhere
/// else has to notice.
#[test]
fn clearing_a_drives_index_drops_the_branches_with_the_coverage() {
    let drive = ColdDrive::new("cover-branch-clear-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");

    drive.cover(&drive.path("scope"));
    // `BranchWatch::persist` hands the meta row to the async `IndexWriter`, and
    // `cover` only waits for the COVERAGE to read as walked. Wait for the row itself:
    // reading the database the instant the walk returns races the writer thread, which
    // is a race only a slow host loses (it did, on the Docker Linux lane).
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's branch to reach the database",
        || drive.persisted_branches().is_some(),
    );

    drive.index.forget_volume(drive.volume_id).expect("clear the index");

    assert_eq!(
        drive.persisted_branches(),
        None,
        "the database went, and the set with it"
    );
    assert!(
        crate::indexing::watch::branches::live_for(drive.volume_id)
            .branch_paths()
            .is_empty(),
        "and nothing in memory still claims to watch it"
    );
}

/// The per-drive veto's teeth, and the whole of them: a drive the user turned
/// indexing off for is still walked for a search (Decision 13), and is left
/// unwatched afterwards.
///
/// So its walked ground stays covered and served, and stops being kept current
/// the moment the app stops. ❌ Not "re-walked" — the walk marked those
/// directories listed, so the frontier never offers them again; they are
/// covered-but-stale, which Decision 5 trusts.
///
/// The veto takes the WATCHER, not the record. What a walk covered is a fact
/// about the index, and dropping it would leave the next session unable to tell
/// walked ground from ground nobody ever asked about.
#[test]
fn a_vetoed_drive_is_walked_and_left_unwatched() {
    let drive = ColdDrive::new("cover-branch-veto-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    drop(IndexStore::open(&drive.db_path()).expect("open store"));
    IndexStore::set_user_disabled(&drive.db_path(), true).expect("mark user_disabled");
    let scope = drive.path("scope");

    let outcome = drive.cover(&scope);
    assert_eq!(outcome.roots_covered, 1, "the search still got its answer");

    assert!(
        !crate::indexing::lifecycle::state::is_watching_for_test(drive.volume_id),
        "and nothing is watching it: the veto stops everything that runs uninvited"
    );
    assert_eq!(
        crate::indexing::watch::branches::live_for(drive.volume_id).branch_paths(),
        [scope],
        "while what the walk covered is still written down"
    );
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the walk's branch to reach the database",
        || drive.persisted_branches().is_some(),
    );
    assert_eq!(
        drive.persisted_branches(),
        Some("/scope".to_string()),
        "on the drive's own database, so a later session knows this ground was walked"
    );
}

/// A branch survives the volume going away and coming back, which is what
/// "persisted" has to mean.
///
/// Nothing at LAUNCH resumes it, deliberately: an unregistered volume answers
/// neither sizes nor coverage questions, so the first moment that coverage can be
/// read is the moment its index comes up — and that's the moment the watch
/// returns.
#[test]
fn a_branch_comes_back_when_the_volume_does() {
    let drive = ColdDrive::new("cover-branch-resume-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    crate::indexing::lifecycle::state::stop_indexing(drive.volume_id).expect("stop the volume");
    assert!(
        crate::indexing::watch::branches::live_for(drive.volume_id)
            .branch_paths()
            .is_empty(),
        "precondition: the set goes with the instance that watched it"
    );

    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::LocalExternal,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index back up");

    assert_eq!(
        crate::indexing::watch::branches::live_for(drive.volume_id).branch_paths(),
        [scope],
        "and the volume comes back watching what it walked last time"
    );
}

/// A drive that isn't mounted has nothing to walk and nothing to root an index
/// at, so it reads as what it is: not indexed.
#[test]
fn an_unmounted_volume_is_not_walkable() {
    let drive = ColdDrive::new("cover-cold-unmounted-test");
    assert!(
        matches!(
            drive.index.cover(
                "nothing-is-mounted-here",
                vec![drive.path("x")],
                CoverageDimension::Listing,
                CancellationToken::new(),
            ),
            Err(crate::indexing::handle::IndexError::NotIndexed { .. })
        ),
        "an unmounted drive can't be bootstrapped"
    );
}

/// A share and a phone are walked over the `Volume` trait, and the LOCAL guarded
/// walker is never pointed at one.
///
/// That half is the data-safety rule: walking a network mount locally traverses a
/// share over syscalls that block for minutes, and the rows it wrote would fight
/// the trait scanner's. What decides it is typed facts — a live smb2 session, a
/// network filesystem, MTP's own id vocabulary — never a path substring. The walk
/// itself is `network_tests.rs`.
#[test]
fn a_share_or_a_phone_walks_over_the_trait_and_never_locally() {
    let walks_over_the_trait = |drive: &ColdDrive| {
        bootstrap::walkable_volume(drive.volume_id)
            .expect("a registered volume is walkable")
            .kind
            .is_trait_scanned()
    };

    {
        let share = ColdDrive::with_volume("cover-cold-share-test", |volume| {
            volume
                .with_local_fs_access()
                .with_smb_connection_state(cmdr_fs::volume::SmbConnectionState::Direct)
        });
        assert!(walks_over_the_trait(&share), "a live smb2 session is not local ground");
    }
    {
        // A phone's files exist only over PTP: no local path to walk at all.
        let phone = ColdDrive::with_volume("cover-cold-phone-test", |volume| volume);
        assert!(
            walks_over_the_trait(&phone),
            "a volume with no local filesystem access is not local ground"
        );
    }
    // And a phone by its own id vocabulary, which is what routes MTP everywhere
    // else. It's asked FIRST, before any mount probe: `mtp://…` is not a path a
    // `statfs` can answer for.
    let phone = ColdDrive::with_volume("mtp-serial:1", |volume| volume.with_local_fs_access());
    assert_eq!(
        bootstrap::walkable_volume(phone.volume_id)
            .expect("a phone is walkable")
            .kind,
        IndexVolumeKind::Mtp,
    );
}

// ── One writer per database ──────────────────────────────────────────

/// The other direction of the same rule, and the sharper one: a volume a walk is
/// covering isn't TRUNCATED under it.
///
/// `start_scan`'s single-flight guard reads `mgr.scanning`, which a search-driven
/// walk never sets — it holds a claim instead. Left there, a rescan through any
/// door (the manual button, a journal-gap fallback, a coalesced shallow anchor)
/// sends `TruncateData` + `BumpCurrentEpoch` while the walk is still writing:
/// the walk's rows land in a database that was blanked underneath them, and
/// everything it attributed to an id the truncate dropped is orphaned.
#[test]
fn a_truncating_rescan_refuses_while_a_search_cover_walk_is_live() {
    let drive = ColdDrive::new("cover-truncate-guard-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    assert!(
        drive.is_indexed(&drive.path("scope/found.txt")),
        "precondition: the walk's rows are in"
    );
    let epoch = drive.current_epoch();

    // The hold a walk keeps for as long as it runs, taken directly so the window
    // is deterministic rather than a race against a walk of two files.
    let walking = Claim::take(drive.volume_id, vec![scope.clone()]);

    assert!(
        crate::indexing::lifecycle::state::force_scan(drive.volume_id).is_err(),
        "a rescan refuses while a walk holds ground on the volume"
    );
    assert_eq!(
        drive.scans_started(),
        0,
        "so no scan announced itself, which is also where the truncate would have been sent from"
    );
    assert_eq!(
        drive.current_epoch(),
        epoch,
        "and the epoch the walk's rows carry is untouched: a bump under a live walk renders \
         everything it just wrote as stale"
    );

    drop(walking);
    crate::indexing::lifecycle::state::force_scan(drive.volume_id)
        .expect("and the moment the walk ends the rescan runs");
}

/// A volume whose own full scan is running isn't walked at all.
///
/// Two reasons, and either alone would be enough: the scan already covers
/// everything a search would want walked, and a walk beside it allocates fresh
/// ids for names the scan is inserting under its own — `INSERT OR IGNORE` drops
/// whichever loses and orphans everything below it.
#[test]
fn a_volume_mid_full_scan_is_not_walked() {
    let drive = ColdDrive::new("cover-scan-in-progress-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");

    // A writer-only start is the shape a walk leaves: Running, nothing scanning.
    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::Local,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index up");
    assert!(
        context_for_walk(drive.volume_id).is_ok(),
        "precondition: with no scan running, the walk reuses this writer"
    );

    crate::indexing::lifecycle::state::set_scanning_for_test(drive.volume_id, true);
    assert!(
        matches!(context_for_walk(drive.volume_id), Err(NoCoverContext::ScanInProgress)),
        "a scan owns the volume while it runs"
    );

    crate::indexing::lifecycle::state::set_scanning_for_test(drive.volume_id, false);
    assert!(
        context_for_walk(drive.volume_id).is_ok(),
        "and hands it back when it's done"
    );
}

/// An index whose rows predate this build's exclusion policy is DROPPED before
/// the next walk, not walked on top of (`docs/specs/unindexed-search-plan.md`
/// Decision 17).
///
/// A stamp mismatch means nothing in the index counts as covered, so every
/// search hands the whole scope back to the walk. On a scanned drive the next
/// full scan truncates and re-stamps; on a walk-built one no scan is coming, and
/// the walk can't re-stamp a database that already holds rows — so without the
/// eviction that drive re-walks its whole scope on every search, forever, each
/// root landing on the slow repair path. Evicting costs one walk and gets
/// convergence back.
#[test]
fn an_index_that_predates_the_exclusion_policy_is_dropped_before_the_next_walk() {
    let drive = ColdDrive::new("cover-stale-policy-test");
    std::fs::create_dir_all(drive.tree.path().join("scope/inner")).expect("dirs");
    std::fs::write(drive.tree.path().join("scope/inner/found.txt"), "x").expect("file");
    let scope = drive.path("scope");

    drive.cover(&scope);
    assert!(
        drive.is_indexed(&drive.path("scope/inner/found.txt")),
        "the walk landed"
    );

    // The drive stops being indexed (the user turned it off, or the app simply
    // restarted and nothing registered a drive nobody indexes), and a release
    // edits the exclusion lists behind its back.
    drive.index.disable_volume(drive.volume_id).expect("disable");
    {
        let conn = IndexStore::open_write_connection(&drive.db_path()).expect("write conn");
        IndexStore::update_meta(&conn, crate::indexing::store::EXCLUSION_POLICY_KEY, "0000000000000000")
            .expect("stamp a policy this build doesn't apply");
    }

    // The next search's walk converges again, which it can only do on a database
    // it was allowed to stamp — an empty one.
    let outcome = drive.cover(&scope);
    assert!(!outcome.cancelled);
    assert!(
        outcome.entries_found >= 2,
        "the walk rebuilt what the dropped index held, {} entries",
        outcome.entries_found
    );
    let conn = IndexStore::open_read_connection(&drive.db_path()).expect("read conn");
    assert!(
        !crate::indexing::scanner::index_predates_exclusion_policy(&conn),
        "the rebuilt index carries this build's policy, so its coverage counts"
    );
}
