//! The cover driver over a real filesystem: what it emits, what it fills in, and
//! what it refuses to touch.
//!
//! Everything here runs against an index that already exists, over a temp tree the
//! LOCAL walker reads off the disk. The cold-drive half (no index at all, driven
//! through the public handle) is `cold_drive_tests.rs`; the `Volume`-trait half is
//! `network_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::test_support::drain;
use super::*;
use crate::indexing::store::ROOT_ID;
use crate::indexing::writer::WriteMessage;

// ── Fixture ──────────────────────────────────────────────────────────

/// A temp tree plus an index over it, with the ancestor chain down to the tree
/// root already seeded so a frontier path resolves.
struct Fixture {
    tree: tempfile::TempDir,
    _db_dir: tempfile::TempDir,
    db_path: PathBuf,
    writer: IndexWriter,
    /// A volume id of its own, because the in-flight frontier claims
    /// (`live.rs`) are keyed by one and these tests run in parallel over paths
    /// that would otherwise look like each other's ground.
    volume_id: String,
}

impl Fixture {
    fn new() -> Self {
        // In the CWD rather than `/tmp`: `/tmp` is excluded on Linux and is a
        // symlink on macOS, and both would fight the path space.
        let tree = tempfile::Builder::new()
            .prefix("cmdr-cover-test-")
            .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .expect("temp tree");
        let db_dir = tempfile::tempdir().expect("temp db dir");
        let db_path = db_dir.path().join("cover-test-index.db");
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

        let fixture = Self {
            tree,
            _db_dir: db_dir,
            db_path,
            writer,
            volume_id: format!("cover-fixture-{}", next_fixture_id()),
        };
        fixture.seed_chain(fixture.tree.path());
        fixture
    }

    /// Insert the ancestor chain down to `path`, and sync the writer's id counter.
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

    fn context(&self) -> CoverContext {
        CoverContext {
            volume_id: self.volume_id.clone(),
            writer: self.writer.clone(),
            space: IndexPathSpace::root(),
            kind: IndexVolumeKind::Local,
            flush: FlushOnFinish::default(),
        }
    }

    fn path(&self, relative: &str) -> String {
        self.tree.path().join(relative).to_string_lossy().to_string()
    }

    fn id_of(&self, path: &str) -> i64 {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        crate::indexing::store::resolve_path(&conn, path)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{path} should have a row"))
    }

    fn child_ids(&self, path: &str) -> Vec<i64> {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        let Some(id) = crate::indexing::store::resolve_path(&conn, path).expect("resolve") else {
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

    fn listed_epoch(&self, path: &str) -> u64 {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        IndexStore::get_listed_epoch_by_id(&conn, self.id_of(path))
            .expect("listed epoch")
            .expect("row")
    }
}

/// A fresh volume id per fixture, so parallel tests never look like each other's
/// in-flight walk.
fn next_fixture_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ── What a walk delivers ─────────────────────────────────────────────

/// The batched channel Decision 3 asks for: a walk hands its consumer every
/// entry it discovers, while it's still running, and fills the index with the
/// same rows.
#[test]
fn a_walk_emits_what_it_writes() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("shallow/deep")).expect("dirs");
    std::fs::write(root.join("shallow/one.txt"), "aaaa").expect("file");
    std::fs::write(root.join("shallow/deep/two.txt"), "bb").expect("file");
    // A frontier node always has its own row: `coverage` found it by descending
    // into its parent's listing. (A path the index has never seen at all is
    // the cold-bootstrap case, not this one.)
    f.seed_chain(&root.join("shallow"));

    let frontier = vec![f.path("shallow")];
    let walk = start(
        f.context(),
        frontier,
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    let (entries, outcome) = drain(walk);
    f.writer.flush_blocking().expect("flush");

    assert!(!outcome.cancelled, "the walk ran to the end");
    assert_eq!(outcome.roots_covered, 1);
    assert_eq!(outcome.entries_found, 3, "one.txt, deep/, deep/two.txt");
    assert_eq!(outcome.dirs_found, 1, "deep/ is the only directory among them");
    assert_eq!(entries.len(), 3, "every written entry reaches the consumer");

    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        vec![
            f.path("shallow/deep"),
            f.path("shallow/deep/two.txt"),
            f.path("shallow/one.txt")
        ]
    );
    let one = entries
        .iter()
        .find(|e| e.path.ends_with("one.txt"))
        .expect("one.txt emitted");
    assert_eq!(one.logical_size, Some(4), "with the size a result row shows");
    assert!(!one.is_directory);

    assert!(f.listed_epoch(&f.path("shallow")) > 0, "and the index now covers it");
    assert!(f.listed_epoch(&f.path("shallow/deep")) > 0);
}

/// A consumer that goes away doesn't stop the walk (Decision 11): the ground it
/// covers is worth covering regardless of who was watching, and the rows are
/// there for the next query.
#[test]
fn dropping_the_consumer_leaves_the_walk_running() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("wide")).expect("dirs");
    for i in 0..50 {
        std::fs::write(root.join("wide").join(format!("f{i}.txt")), "x").expect("file");
    }
    f.seed_chain(&root.join("wide"));

    let walk = start(
        f.context(),
        vec![f.path("wide")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    // Never read a batch; `finish` drops the channel and waits it out.
    let outcome = walk.finish();
    f.writer.flush_blocking().expect("flush");

    assert!(!outcome.cancelled, "nobody cancelled it");
    assert_eq!(outcome.entries_found, 50, "it walked the whole thing anyway");
    assert!(f.listed_epoch(&f.path("wide")) > 0, "and the coverage is durable");
}

// ── The repair path ──────────────────────────────────────────────────

/// A frontier node the index already holds rows under is repaired by the serial
/// reconcile, which compares by name: the pre-existing rows keep their ids, the
/// new siblings arrive, and nothing is deleted.
///
/// The other half of
/// `scanner::convergence_tests::covering_a_frontier_node_never_removes_a_row_it_did_not_write`,
/// which pins that the parallel walker refuses this case rather than corrupting it.
#[test]
fn a_non_virgin_frontier_node_is_repaired_without_losing_rows() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    std::fs::write(root.join("F/G/kept.txt"), "kept").expect("file");
    std::fs::write(root.join("F/new.txt"), "new").expect("file");

    // What FSEvents verification leaves behind: G's row under an unlisted F.
    let f_id = f.seed_chain(&root.join("F"));
    f.writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: f_id,
            name: "G".to_string(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .expect("upsert G");
    f.writer.flush_blocking().expect("flush");
    // … and then G itself gets walked, so it holds rows F has no claim on.
    let g_walk = start(
        f.context(),
        vec![f.path("F/G")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    drain(g_walk);
    f.writer.flush_blocking().expect("flush");

    let g_rows = f.child_ids(&f.path("F/G"));
    assert_eq!(g_rows.len(), 1, "precondition: G holds kept.txt");
    assert_eq!(f.listed_epoch(&f.path("F")), 0, "precondition: F is a frontier node");

    let walk = start(
        f.context(),
        vec![f.path("F")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    let (_, outcome) = drain(walk);
    f.writer.flush_blocking().expect("flush");

    assert_eq!(outcome.roots_covered, 1, "the repair path covered it");
    assert_eq!(
        f.child_ids(&f.path("F/G")),
        g_rows,
        "the rows the walk did not write keep their ids"
    );
    assert!(f.listed_epoch(&f.path("F")) > 0, "and F is covered now");
    assert!(
        f.child_ids(&f.path("F")).len() >= 2,
        "with the sibling the repair discovered, alongside G"
    );
}

// ── Ground the index has no row for ──────────────────────────────────

/// A frontier path with no `entries` row is walkable, and the chain the walk
/// needs to get there is materialized on the way.
///
/// This is NOT only a cold-volume story: a folder created since the last listing
/// has no row on a fully indexed volume either, and its parent is exactly the
/// frontier node a coverage answer names. Without this, the walk resolves its
/// root to nothing and the frontier never shrinks.
#[test]
fn a_frontier_path_with_no_row_is_materialized_and_walked() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("fresh/deeper")).expect("dirs");
    std::fs::write(root.join("fresh/deeper/found.txt"), "x").expect("file");
    // Deliberately NO `seed_chain` for `fresh`: nothing has listed the tree root,
    // so neither `fresh` nor `fresh/deeper` has a row.

    let walk = start(
        f.context(),
        vec![f.path("fresh/deeper")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    let (entries, outcome) = drain(walk);
    f.writer.flush_blocking().expect("flush");

    assert_eq!(outcome.roots_covered, 1, "the walk reached ground it had no row for");
    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        [f.path("fresh/deeper"), f.path("fresh/deeper/found.txt")],
        "and delivered what it found there, plus the folder it had to materialize: \
         nothing else will ever report that row to whoever asked for the walk"
    );
    assert!(
        f.listed_epoch(&f.path("fresh/deeper")) > 0,
        "the walked node is covered"
    );
}

/// The chain a walk had to materialize claims nothing: an ancestor row exists so
/// the walk could resolve its root, and its `listed_epoch` stays zero because
/// nobody read it. A stamped ancestor would mark the whole tree covered off the
/// back of one walked folder.
#[test]
fn a_materialized_ancestor_claims_no_listing() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("fresh/deeper")).expect("dirs");
    std::fs::create_dir_all(root.join("fresh/untouched")).expect("dirs");

    let walk = start(
        f.context(),
        vec![f.path("fresh/deeper")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    drain(walk);
    f.writer.flush_blocking().expect("flush");

    assert_eq!(
        f.listed_epoch(&f.path("fresh")),
        0,
        "the ancestor was materialized, never listed"
    );
    assert!(
        f.child_ids(&f.path("fresh")).len() == 1,
        "and only the child the walk needed exists under it, not the sibling nobody read"
    );
}

/// A chain that runs through a FILE row is declined rather than parented under.
///
/// The stale file→dir type change the reconciler escalates on
/// (`reconcile_subtree`'s "parent of X is not a directory row"): upserting
/// children under a file id orphans every one of them, so the walk leaves the
/// root in the frontier and lets the reconcile that heals type changes have it.
#[test]
fn a_chain_running_through_a_file_row_is_declined() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("mixed/inner")).expect("dirs");
    // The index believes `mixed` is a file — what a file replaced by a directory
    // leaves behind until something re-lists the parent.
    let parent_id = f.seed_chain(root);
    {
        let conn = IndexStore::open_write_connection(&f.db_path).expect("write connection");
        IndexStore::insert_entry_v2(&conn, parent_id, "mixed", false, false, Some(1), None, None, None)
            .expect("insert the file row");
    }

    let walk = start(
        f.context(),
        vec![f.path("mixed/inner")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    let (entries, outcome) = drain(walk);

    assert_eq!(outcome.roots_covered, 0, "the walk declined the broken chain");
    assert!(entries.is_empty());
    assert!(
        matches!(
            bootstrap::ensure_walkable(&f.context(), &Ground::Local, &root.join("mixed/inner")),
            Err(bootstrap::NotWalkable::FileRowInTheChain(_))
        ),
        "and says which kind of broken"
    );
}

/// A frontier path that isn't a directory on disk — deleted between the coverage
/// answer and the walk, or a symlink the index stores but never descends into —
/// gets no row at all. Materializing one would leave a directory in the index
/// that nothing can ever list.
#[test]
fn a_frontier_path_that_is_not_a_directory_on_disk_is_declined() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("real")).expect("dirs");

    assert!(
        matches!(
            bootstrap::ensure_walkable(&f.context(), &Ground::Local, &root.join("gone")),
            Err(bootstrap::NotWalkable::NotADirectoryOnDisk(_))
        ),
        "a path that isn't there can't be walked"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).expect("symlink");
        assert!(
            matches!(
                bootstrap::ensure_walkable(&f.context(), &Ground::Local, &root.join("link")),
                Err(bootstrap::NotWalkable::NotADirectoryOnDisk(_))
            ),
            "a symlink is not a directory to descend into"
        );
    }
    assert!(f.child_ids(&f.path("")).is_empty(), "and neither one left a row behind");
}

/// Two walks over overlapping ground don't both walk it: the second leaves the
/// shared roots to the first, walks the rest, and says which it left.
///
/// This is the case Decision 11 creates — a refined query re-asks `coverage`
/// while the first query's walk is still running — and it's a data-safety rule,
/// not a performance one. The second search loses nothing durable: the first
/// walk's rows go into the same index, which is where Decision 11 already says a
/// superseded query recovers its predecessor's ground.
///
/// The live walk is stood in for by its `Claim`, deliberately: a real first walk
/// over a small fixture can finish before the second one starts, and a test that
/// races its own precondition would go green on a broken implementation about
/// half the time.
#[test]
fn a_walk_leaves_ground_another_walk_is_covering_to_it() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("shared/inner")).expect("dirs");
    std::fs::create_dir_all(root.join("mine")).expect("dirs");
    std::fs::write(root.join("mine/f.txt"), "x").expect("file");
    std::fs::write(root.join("shared/inner/theirs.txt"), "x").expect("file");
    f.seed_chain(&root.join("shared"));
    f.seed_chain(&root.join("mine"));

    let first = Claim::take(&f.volume_id, vec![f.path("shared")], Mode::Additive);
    let second = start(
        f.context(),
        vec![f.path("shared/inner"), f.path("mine")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );

    assert_eq!(
        second.covered_by_another_walk(),
        [f.path("shared/inner")],
        "ground inside a live walk is left to it"
    );
    let (entries, outcome) = drain(second);
    f.writer.flush_blocking().expect("flush");
    assert_eq!(outcome.roots_covered, 1, "and the rest is walked normally");
    assert_eq!(entries.len(), 1, "only `mine`'s file, none of the shared ground");
    assert!(
        f.child_ids(&f.path("shared")).is_empty(),
        "and this walk wrote nothing at all under the deferred root, rather than \
         half-covering ground the other walk is mid-way through"
    );

    drop(first);
}

/// A walk holds its ground only while it runs, so the next search over the same
/// folder walks rather than deferring forever.
#[test]
fn a_finished_walk_releases_the_ground_it_held() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.tree.path().join("once")).expect("dirs");
    f.seed_chain(&f.tree.path().join("once"));

    let walk = start(
        f.context(),
        vec![f.path("once")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    assert!(walk.covered_by_another_walk().is_empty(), "nobody else holds it");
    drain(walk);

    let again = start(
        f.context(),
        vec![f.path("once")],
        CoverageDimension::Listing,
        CancellationToken::new(),
    );
    assert!(
        again.covered_by_another_walk().is_empty(),
        "a finished walk holds nothing"
    );
    drain(again);
}

// ── Cancellation ─────────────────────────────────────────────────────

/// A walk stopped partway reports what it DID cover, not zero.
///
/// The totals are what the dialog's "walked 40,000 folders" line reads, and a cancelled
/// walk that reported nothing would tell the user their eight minutes bought
/// them nothing — when in fact every folder it read is now in the index.
#[test]
fn a_walk_cancelled_partway_reports_the_ground_it_covered() {
    let f = Fixture::new();
    let root = f.tree.path();
    // Several roots, so cancelling between them is deterministic: the first is
    // walked in full, then the token fires, then the loop stops at the second.
    for name in ["one", "two"] {
        std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
        std::fs::write(root.join(name).join("f.txt"), "xx").expect("file");
        f.seed_chain(&root.join(name));
    }

    let cancel = CancellationToken::new();
    let walk = start(
        f.context(),
        vec![f.path("one"), f.path("two")],
        CoverageDimension::Listing,
        cancel.clone(),
    );
    // The first root's batch is in hand, so its walk is over; stop before the second.
    let first = walk.next_batch().expect("the first root's entries");
    cancel.cancel();
    let (mut entries, outcome) = drain(walk);
    entries.extend(first);
    f.writer.flush_blocking().expect("flush");

    assert!(outcome.cancelled, "it was stopped, and says so");
    assert!(
        outcome.entries_found >= 2,
        "the totals carry what it read, not zero (got {outcome:?})"
    );
    assert!(
        outcome.dirs_found >= 1,
        "including the directories among them (got {outcome:?})"
    );
    assert!(!entries.is_empty(), "and the consumer got them");
    assert!(
        f.listed_epoch(&f.path("one")) > 0,
        "the coverage the walk earned is durable"
    );
}

/// The repair path reports a cancellation as one, rather than as a covered node.
///
/// `reconcile_subtree` breaks out of its walk on cancel and returns `Ok`, so
/// without `ReconcileSummary.cancelled` this arm would count a stopped repair as
/// a finished one and the frontier would look smaller than it is.
#[test]
fn a_cancelled_repair_is_reported_as_cancelled_not_covered() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    f.seed_chain(&root.join("F/G"));

    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        repair_non_virgin(&f.context(), &root.join("F"), &cancel),
        RootOutcome::Cancelled,
        "a repair whose token had already fired covered nothing"
    );
}

/// Cancelling before the walk starts leaves the frontier where it was, and says
/// so.
#[test]
fn a_walk_cancelled_up_front_covers_nothing_and_admits_it() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.tree.path().join("untouched")).expect("dirs");
    f.seed_chain(&f.tree.path().join("untouched"));

    let cancel = CancellationToken::new();
    cancel.cancel();
    let walk = start(
        f.context(),
        vec![f.path("untouched")],
        CoverageDimension::Listing,
        cancel,
    );
    let (entries, outcome) = drain(walk);
    f.writer.flush_blocking().expect("flush");

    assert!(outcome.cancelled);
    assert_eq!(outcome.roots_covered, 0);
    assert!(entries.is_empty());
    assert_eq!(
        f.listed_epoch(&f.path("untouched")),
        0,
        "nothing may claim coverage it didn't earn"
    );
}
