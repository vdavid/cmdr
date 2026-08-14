//! What a branch-watched volume does with an event, from the admission rule up
//! to the rows a real batch writes.
//!
//! The two anchors are `an_event_inside_a_covered_branch_reaches_the_index_and_one_outside_does_not`
//! and `an_event_that_lands_mid_walk_is_not_lost`: the first is the promise
//! Decision 9 makes, the second is the failure nobody would notice.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::*;
use crate::indexing::reconcile::reconciler::EventReconciler;
use crate::indexing::store::ROOT_ID;
use crate::indexing::watch::churn_monitor::ChurnObserver;
use crate::indexing::watch::event_loop::{drain_promoted, process_live_batch, queue_admitted};
use crate::indexing::watch::watcher::FsEventFlags;
use tokio_util::sync::CancellationToken;

// ── Fixtures ─────────────────────────────────────────────────────────

/// A real tree on disk, a real index over it, and a real writer, so an admitted
/// event writes rows and a discarded one provably doesn't.
struct Fixture {
    tree: tempfile::TempDir,
    _db_dir: tempfile::TempDir,
    db_path: PathBuf,
    writer: IndexWriter,
}

impl Fixture {
    fn new() -> Self {
        // Under the crate rather than `/tmp`: Linux's exclusion policy blocks
        // `/tmp`, and on macOS it's a symlink the path space would rewrite.
        let tree = tempfile::Builder::new()
            .prefix("cmdr-branch-test-")
            .tempdir_in(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .expect("temp tree");
        let db_dir = tempfile::tempdir().expect("temp db dir");
        let db_path = db_dir.path().join("branch-test-index.db");
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
        let fixture = Self {
            tree,
            _db_dir: db_dir,
            db_path,
            writer,
        };
        fixture.seed_chain(fixture.tree.path());
        fixture
    }

    /// Insert every component of `path` as a directory row, and sync the writer's
    /// id counter so its own inserts don't collide.
    fn seed_chain(&self, path: &Path) -> i64 {
        let conn = IndexStore::open_write_connection(&self.db_path).expect("write connection");
        let mut parent_id = ROOT_ID;
        for component in path.to_string_lossy().split('/').filter(|c| !c.is_empty()) {
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

    fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    /// Create a directory (with its chain seeded into the index) and a file in
    /// it, and hand back the file's path.
    fn create_file_in(&self, dir_relative: &str, name: &str) -> String {
        let dir = self.tree.path().join(dir_relative);
        std::fs::create_dir_all(&dir).expect("create dir");
        self.seed_chain(&dir);
        let file = dir.join(name);
        std::fs::write(&file, "contents").expect("write file");
        file.to_string_lossy().to_string()
    }

    fn is_indexed(&self, path: &str) -> bool {
        self.writer.flush_blocking().expect("flush");
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        crate::indexing::store::resolve_path(&conn, path)
            .expect("resolve")
            .is_some()
    }

    /// Run one batch of events through the same admission + processing path the
    /// live loop runs them through.
    fn run_batch(&self, scope: &WatchScope, events: Vec<FsChangeEvent>) {
        let space = IndexPathSpace::root();
        let mut reconciler =
            EventReconciler::new_for("branch-test".to_string(), space.clone(), CancellationToken::new());
        reconciler.switch_to_live();
        let mut pending: HashMap<String, FsChangeEvent> = HashMap::new();
        // Through the loop's own promotion path, not a hand-rolled copy of it: the
        // release of a held event is exactly what these tests are for.
        drain_promoted(scope, &mut pending, &mut reconciler, &self.writer);
        for event in events {
            if let Admission::Process(admitted) = scope.admit(event) {
                for event in admitted {
                    queue_admitted(event, &mut pending);
                }
            }
        }
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        let mut origins = HashSet::new();
        let mut churn = ChurnObserver::disabled();
        process_live_batch(
            &mut pending,
            &mut reconciler,
            &space,
            &conn,
            &self.writer,
            &mut origins,
            &mut churn,
        );
        self.writer.flush_blocking().expect("flush");
    }
}

fn created_file(path: &str, event_id: u64) -> FsChangeEvent {
    FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags: FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        },
    }
}

fn must_scan(path: &str, event_id: u64) -> FsChangeEvent {
    FsChangeEvent {
        path: path.to_string(),
        event_id,
        flags: FsEventFlags {
            must_scan_sub_dirs: true,
            item_is_dir: true,
            ..Default::default()
        },
    }
}

/// A watch over branches that are already covered (no walk in flight).
fn live_watch(paths: &[&str]) -> WatchScope {
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let owned: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    watch.begin_covering(&owned);
    watch.finish_covering(&owned, AfterWalk::Watch);
    WatchScope::Branches(watch)
}

// ── The two anchors ──────────────────────────────────────────────────

/// Decision 9's promise, end to end: a walked branch is as live as an indexed
/// one, and the ground beside it is still nobody's business.
#[test]
fn an_event_inside_a_covered_branch_reaches_the_index_and_one_outside_does_not() {
    let f = Fixture::new();
    let inside = f.create_file_in("covered", "new.txt");
    let outside = f.create_file_in("untouched", "new.txt");
    let scope = live_watch(&[&f.path("covered")]);

    f.run_batch(&scope, vec![created_file(&inside, 10), created_file(&outside, 11)]);

    assert!(
        f.is_indexed(&inside),
        "a change inside the walked branch updates the index"
    );
    assert!(
        !f.is_indexed(&outside),
        "a change outside every walked branch is not this index's business: writing it would \
         put rows under ground nothing ever listed"
    );
}

/// The silent-corruption case. A change that lands while the walk is still
/// covering its branch must not be dropped (the branch's aggregate would drift
/// with nothing to signal it) and must not be written underneath the walk
/// (fresh ids from the parallel walker collide and orphan a subtree). It waits,
/// and lands when the walk ends.
#[test]
fn an_event_that_lands_mid_walk_is_not_lost() {
    let f = Fixture::new();
    let inside = f.create_file_in("covered", "arrived-mid-walk.txt");
    let branch = vec![f.path("covered")];
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    watch.begin_covering(&branch);
    let scope = WatchScope::Branches(Arc::clone(&watch));

    f.run_batch(&scope, vec![created_file(&inside, 20)]);
    assert!(
        !f.is_indexed(&inside),
        "while the walk covers the branch the event waits: writing it now races the walk's own ids"
    );

    watch.finish_covering(&branch, AfterWalk::Watch);
    f.run_batch(&scope, vec![]);

    assert!(
        f.is_indexed(&inside),
        "and the moment the walk ends it lands, so the branch's rows and sizes are whole"
    );
}

// ── The admission rule ───────────────────────────────────────────────

#[test]
fn a_coalesced_sweep_above_a_branch_is_re_anchored_onto_it() {
    // FSEvents reports "a lot changed under here" at a shallower path than the
    // branch. A plain prefix test would discard it and lose every change inside
    // the covered ground.
    let scope = live_watch(&["/a/b/covered", "/a/b/also-covered"]);

    let Admission::Process(events) = scope.admit(must_scan("/a", 1)) else {
        panic!("a sweep above the branches must not be discarded");
    };
    let mut paths: Vec<String> = events.into_iter().map(|e| e.path).collect();
    paths.sort();
    assert_eq!(paths, ["/a/b/also-covered", "/a/b/covered"]);
    assert!(
        events_are_discarded(&scope, must_scan("/elsewhere", 2)),
        "a sweep over unrelated ground is still nobody's business"
    );
}

#[test]
fn a_pocket_walked_inside_a_live_branch_buffers_and_is_absorbed_when_it_ends() {
    // A cancelled walk leaves unwalked pockets inside a branch, so a later walk
    // covers one while the branch around it is live. Its events must buffer
    // against the POCKET, not process because the branch around it says live.
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branch = vec!["/vol/covered".to_string()];
    watch.begin_covering(&branch);
    watch.finish_covering(&branch, AfterWalk::Watch);
    let pocket = vec!["/vol/covered/pocket".to_string()];
    watch.begin_covering(&pocket);

    assert!(
        matches!(
            watch.admit(created_file("/vol/covered/pocket/deep.txt", 1), Reach::CoveredBranches),
            Admission::Buffered
        ),
        "the deepest branch decides, so ground under walk buffers even inside a live branch"
    );
    assert!(
        matches!(
            watch.admit(created_file("/vol/covered/other.txt", 2), Reach::CoveredBranches),
            Admission::Process(_)
        ),
        "and the rest of the live branch keeps flowing"
    );

    watch.finish_covering(&pocket, AfterWalk::Watch);
    assert_eq!(
        watch.branch_paths(),
        ["/vol/covered"],
        "a branch inside a live branch is redundant once its walk ends"
    );
    assert_eq!(watch.take_promoted().events.len(), 1, "and its held event is released");
}

#[test]
fn an_overflowing_buffer_asks_for_a_relist_instead_of_a_replay() {
    // Past the cap the buffer is no longer a complete record of what changed, so
    // replaying it would leave the branch subtly wrong with no signal. Re-listing
    // the branch is the honest recovery.
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branch = vec!["/vol/covered".to_string()];
    watch.begin_covering(&branch);
    for id in 0..(BRANCH_BUFFER_CAP as u64 + 5) {
        let _ = watch.admit(
            created_file(&format!("/vol/covered/f{id}.txt"), id + 1),
            Reach::CoveredBranches,
        );
    }
    watch.finish_covering(&branch, AfterWalk::Watch);

    let promoted = watch.take_promoted();
    assert!(promoted.events.is_empty(), "a partial record is not replayed");
    assert_eq!(promoted.relist, ["/vol/covered"], "the branch is re-listed instead");
}

#[test]
fn escalation_is_confined_to_the_covered_branches() {
    // The walk owns coverage growth; the watcher only keeps covered ground
    // current. Without this a create in unwalked ground escalates into a subtree
    // rescan of a drive whose owner may have indexing turned off entirely.
    let scope = live_watch(&["/vol/covered"]);
    let watch = scope.branches();

    assert!(watch.covers(Path::new("/vol/covered/inner")));
    assert!(watch.covers(Path::new("/vol/covered")));
    assert!(!watch.covers(Path::new("/vol/elsewhere")));
    assert!(!watch.covers(Path::new("/vol")), "never an ancestor of a branch");
}

#[test]
fn the_journal_position_never_advances_past_a_held_event() {
    // The stored position is what a restart replays from, so advancing it past
    // an event we're still holding would drop that event on the floor.
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branch = vec!["/vol/covered".to_string()];
    watch.begin_covering(&branch);

    let _ = watch.admit(created_file("/vol/elsewhere/x.txt", 40), Reach::CoveredBranches);
    assert_eq!(
        watch.safe_event_id(),
        Some(40),
        "a discarded event still moves the stream position: the journal is about the stream"
    );

    let _ = watch.admit(created_file("/vol/covered/held.txt", 41), Reach::CoveredBranches);
    assert_eq!(watch.safe_event_id(), None, "and a held event pins it until it lands");

    watch.finish_covering(&branch, AfterWalk::Watch);
    assert_eq!(watch.safe_event_id(), Some(41));
}

/// Two searches can walk overlapping frontiers, so a branch counts its walks:
/// the first one finishing must not un-buffer the second's ground.
#[test]
fn a_branch_two_walks_are_covering_stays_held_until_the_last_one_ends() {
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branch = vec!["/vol/covered".to_string()];
    watch.begin_covering(&branch);
    watch.begin_covering(&branch);

    let _ = watch.admit(created_file("/vol/covered/held.txt", 1), Reach::CoveredBranches);
    watch.finish_covering(&branch, AfterWalk::Watch);
    assert!(
        watch.take_promoted().events.is_empty(),
        "one walk ending is not the ground going quiet"
    );
    assert!(
        matches!(
            watch.admit(created_file("/vol/covered/second.txt", 2), Reach::CoveredBranches),
            Admission::Buffered
        ),
        "and the branch is still under walk"
    );

    watch.finish_covering(&branch, AfterWalk::Watch);
    assert_eq!(
        watch.take_promoted().events.len(),
        2,
        "released when the last walk ends"
    );
}

/// On a scanned volume the branch set does one job — hold events for a walk's
/// duration — and it has to do it without swallowing everything else.
#[test]
fn a_whole_watched_volume_holds_only_the_ground_its_walk_is_covering() {
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branch = vec!["/vol/hole".to_string()];
    watch.begin_covering(&branch);
    let scope = WatchScope::WholeVolume(Arc::clone(&watch));

    assert!(
        matches!(scope.admit(created_file("/vol/hole/inner.txt", 1)), Admission::Buffered),
        "the walk's ground waits for it, on an indexed drive exactly as on a walked one"
    );
    assert!(
        matches!(
            scope.admit(created_file("/vol/elsewhere.txt", 2)),
            Admission::Process(_)
        ),
        "and everything else is still this loop's business"
    );

    // A coalesced sweep ABOVE the walk waits too: reconciling it would walk a
    // subtree straight through the ground the walk is covering.
    assert!(
        matches!(scope.admit(must_scan("/vol", 3)), Admission::Buffered),
        "a sweep over the walk's ground waits for the walk"
    );

    watch.finish_covering(&branch, AfterWalk::Forget);
    assert!(
        watch.branch_paths().is_empty(),
        "and a scanned volume keeps no branch bookkeeping once the walk is done"
    );
    assert_eq!(watch.take_promoted().events.len(), 3, "everything it held is released");

    // With no walk in flight, a sweep above is processed AND re-anchored onto
    // nothing (there are no branches left), so it flows unchanged.
    assert!(matches!(scope.admit(must_scan("/vol", 4)), Admission::Process(_)));
}

/// A sweep above a LIVE branch on a scanned volume keeps both halves: the branch
/// gets its own anchored copy, and the original still reconciles the rest of the
/// subtree.
#[test]
fn a_whole_watched_sweep_above_a_live_branch_keeps_both_halves() {
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branch = vec!["/vol/covered".to_string()];
    watch.begin_covering(&branch);
    watch.finish_covering(&branch, AfterWalk::Watch);
    let scope = WatchScope::WholeVolume(watch);

    let Admission::Process(events) = scope.admit(must_scan("/vol", 1)) else {
        panic!("a whole-watched volume answers for the sweep it was handed");
    };
    let mut paths: Vec<String> = events.into_iter().map(|e| e.path).collect();
    paths.sort();
    assert_eq!(paths, ["/vol", "/vol/covered"]);
}

/// A plain event outside every branch is DISCARDED, not held: holding it would
/// leak, and a branch-watched loop has nothing to hold it against.
#[test]
fn an_ordinary_event_outside_the_branches_is_dropped_not_held() {
    let scope = live_watch(&["/vol/covered"]);
    assert!(events_are_discarded(&scope, created_file("/vol/elsewhere/x.txt", 1)));
    assert!(events_are_discarded(&scope, must_scan("/vol/elsewhere", 2)));
}

// ── Absorption ───────────────────────────────────────────────────────

/// A branch that arrives over existing ones RETIRES them. Watching `/vol/a`
/// covers `/vol/a/one`, so keeping both makes every event pay a longer
/// `deepest_containing` scan for an answer that can't differ — and the set is
/// scanned once per event on the live hot path.
#[test]
fn a_branch_absorbs_the_ones_it_covers_and_leaves_a_live_walk_alone() {
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let settled = vec!["/vol/a/one".to_string(), "/vol/a/two".to_string()];
    watch.begin_covering(&settled);
    watch.finish_covering(&settled, AfterWalk::Watch);
    let live = vec!["/vol/a/three".to_string()];
    watch.begin_covering(&live);

    let over_them = vec!["/vol/a".to_string()];
    watch.begin_covering(&over_them);

    assert_eq!(
        watch.branch_paths(),
        ["/vol/a", "/vol/a/three"],
        "the settled ones are absorbed; the one a walk is covering keeps its entry, because its \
         buffer belongs to that walk"
    );
    assert!(
        matches!(
            watch.admit(created_file("/vol/a/one/new.txt", 1), Reach::CoveredBranches),
            Admission::Buffered
        ),
        "and ground the absorbing walk is covering now waits for it, rather than being written \
         underneath it through a settled entry"
    );

    watch.finish_covering(&live, AfterWalk::Watch);
    watch.finish_covering(&over_them, AfterWalk::Watch);
    assert_eq!(
        watch.branch_paths(),
        ["/vol/a"],
        "and a walk that ends under a settled branch is absorbed by the same rule"
    );
}

/// A collapse mutates the set the running loop is READING, which is the whole
/// reason it exists as an operation.
///
/// The live loop and its reconciler each capture an `Arc<BranchWatch>` when the
/// watch starts (`ensure_branch_watch`). ❌ A collapse built out of
/// `branches::clear` plus a begin/finish pair would drop the map entry, so
/// `live_for` mints a brand-new set nobody is reading: the database would say
/// `["/vol"]` while the loop kept filtering against the old entries for the rest
/// of the session, and `is_branch_confined` would read that same stale set.
#[test]
fn the_branch_collapse_is_visible_to_the_running_live_loop() {
    let f = Fixture::new();
    let space = IndexPathSpace::root();
    let volume_id = "branch-collapse-test";
    let root = f.path("collapsing");

    let watch = live_for(volume_id);
    let covered = vec![f.path("collapsing/one"), f.path("collapsing/two")];
    watch.begin_covering(&covered);
    watch.finish_covering(&covered, AfterWalk::Watch);
    // What the live loop and its reconciler hold: a clone taken when the watch
    // started, never re-resolved.
    let scope = WatchScope::Branches(Arc::clone(&watch));

    // And what a caller that knows the whole subtree is covered does, resolving
    // the set the way any caller would.
    live_for(volume_id).collapse_to(&root, &space, &f.writer);

    assert_eq!(
        scope.branches().branch_paths(),
        [root.as_str()],
        "the loop's own set is the one that collapsed"
    );
    assert!(
        matches!(
            scope.admit(created_file(&f.path("collapsing/three/new.txt"), 1)),
            Admission::Process(_)
        ),
        "so the loop admits the whole collapsed branch, including ground no entry named before"
    );

    f.writer.flush_blocking().expect("flush");
    let conn = IndexStore::open_read_connection(&f.db_path).expect("read connection");
    assert_eq!(
        BranchWatch::with_branches(load_branches(&space, &conn)).branch_paths(),
        [root],
        "and the database says the same thing the loop is reading"
    );
    forget(volume_id);
}

// ── Persistence ──────────────────────────────────────────────────────

#[test]
fn the_branch_set_survives_a_restart_through_the_volumes_own_database() {
    let f = Fixture::new();
    let space = IndexPathSpace::root();
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branches = vec![f.path("one"), f.path("two")];
    watch.begin_covering(&branches);
    watch.finish_covering(&branches, AfterWalk::Watch);
    watch.persist(&space, &f.writer);
    f.writer.flush_blocking().expect("flush");

    let conn = IndexStore::open_read_connection(&f.db_path).expect("read connection");
    let reloaded = BranchWatch::with_branches(load_branches(&space, &conn));

    assert_eq!(reloaded.branch_paths(), branches, "the covered ground comes back");
}

#[test]
fn a_mount_rooted_drive_persists_branches_relative_to_its_mount() {
    // A drive that comes back at `/Volumes/Backup 1` must still find the branches
    // it covered at `/Volumes/Backup`, so the stored form is index-relative.
    let f = Fixture::new();
    let watch = Arc::new(BranchWatch::with_branches(Vec::new()));
    let branches = vec!["/Volumes/Backup/photos".to_string()];
    watch.begin_covering(&branches);
    watch.finish_covering(&branches, AfterWalk::Watch);
    watch.persist(&IndexPathSpace::mount_rooted("/Volumes/Backup"), &f.writer);
    f.writer.flush_blocking().expect("flush");

    let conn = IndexStore::open_read_connection(&f.db_path).expect("read connection");
    let stored = IndexStore::get_meta(&conn, COVERED_BRANCHES_KEY)
        .expect("meta")
        .expect("a stored branch set");
    assert_eq!(stored, "/photos", "stored without the mount root");

    let remounted = IndexPathSpace::mount_rooted("/Volumes/Backup 1");
    let reloaded = BranchWatch::with_branches(load_branches(&remounted, &conn));
    assert_eq!(reloaded.branch_paths(), ["/Volumes/Backup 1/photos"]);
}

// ── Helpers ──────────────────────────────────────────────────────────

fn events_are_discarded(scope: &WatchScope, event: FsChangeEvent) -> bool {
    matches!(scope.admit(event), Admission::Discarded)
}
