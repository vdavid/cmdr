//! Convergence: what a walk leaves behind when it doesn't finish.
//!
//! Every search is supposed to shrink the uncovered frontier durably, so that
//! repeated searching over an area trends toward instant and a refined query
//! walks less than the first one did. Two things have to hold for that, and both
//! are data-safety properties rather than performance ones:
//!
//! 1. A cancelled walk keeps the coverage it earned, with honest lower-bound
//!    sizes over the part it covered and nothing claiming the part it didn't.
//! 2. A walk never removes a row it did not write, so an interrupted walk can
//!    only ever leave the index better-informed than it found it.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use super::test_fixtures::{self, MockChild, MockTree, dir, file, setup_writer};
use super::*;
use crate::indexing::IndexPathSpace;
use crate::indexing::read::coverage::{CoverageDimension, coverage_for_scope};
use crate::indexing::store::{EXCLUSION_POLICY_KEY, IndexStore, UnreadableCause};

// ── Fixture ──────────────────────────────────────────────────────────

/// The tree David's case is written against: `/A/B/C`, ten files of ten bytes
/// each, and a walk that is cancelled the moment it reaches C.
const TREE_ROOT: &str = "/cmdr-convergence-test";

const FILE_NAMES: [&str; 10] = [
    "f0.txt", "f1.txt", "f2.txt", "f3.txt", "f4.txt", "f5.txt", "f6.txt", "f7.txt", "f8.txt", "f9.txt",
];

/// Ten ten-byte files plus an optional subdirectory, which is what each level of
/// the fixture tree holds.
fn level(subdir: Option<&'static str>) -> Vec<MockChild> {
    let mut children: Vec<_> = FILE_NAMES.iter().map(|name| file(name, 10)).collect();
    if let Some(name) = subdir {
        children.push(dir(name));
    }
    children
}

/// Seed the DB with the ancestor chain down to `path`, so a subtree scan can
/// resolve its root, and stamp the exclusion policy this build applies so the
/// coverage query answers about the descent rather than about the stamp.
fn seed_chain(db_path: &Path, path: &Path, writer: &IndexWriter) -> i64 {
    test_fixtures::ensure_path_in_db(db_path, path, writer);
    let conn = IndexStore::open_write_connection(db_path).expect("write connection");
    IndexStore::update_meta(&conn, EXCLUSION_POLICY_KEY, &exclusion_policy_fingerprint()).expect("stamp policy");
    crate::indexing::store::resolve_path(&conn, &path.to_string_lossy())
        .expect("resolve")
        .expect("seeded path")
}

/// One directory's coverage columns, straight off the DB.
fn coverage_columns(db_path: &Path, path: &str) -> (u64, u64) {
    let conn = IndexStore::open_read_connection(db_path).expect("read connection");
    let id = crate::indexing::store::resolve_path(&conn, path)
        .expect("resolve")
        .unwrap_or_else(|| panic!("{path} should have a row"));
    let listed: u64 = conn
        .query_row("SELECT listed_epoch FROM entries WHERE id = ?1", [id], |r| r.get(0))
        .expect("listed_epoch");
    let min_subtree: u64 = conn
        .query_row(
            "SELECT COALESCE(min_subtree_epoch, 0) FROM dir_stats WHERE entry_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    (listed, min_subtree)
}

/// The stored recursive physical size for a directory, or `None` when it has no
/// `dir_stats` row at all.
fn recursive_physical_size(db_path: &Path, path: &str) -> Option<u64> {
    let conn = IndexStore::open_read_connection(db_path).expect("read connection");
    let id = crate::indexing::store::resolve_path(&conn, path).expect("resolve")?;
    IndexStore::get_dir_stats_by_id(&conn, id)
        .expect("read dir_stats")
        .map(|s| s.recursive_physical_size)
}

/// The frontier a search would be handed for `scope`, sorted.
fn frontier(db_path: &Path, scope: &str) -> Vec<String> {
    let conn = IndexStore::open_read_connection(db_path).expect("read connection");
    let mut map = coverage_for_scope(&conn, scope, scope, CoverageDimension::Listing).expect("coverage");
    map.frontier.sort();
    map.frontier
}

// ── 1. A cancelled walk keeps the coverage it earned ─────────────────

/// David's case, and the reason incremental marking exists: `/A/B/C` with ten files each, a walk
/// cancelled before it reaches C.
///
/// A and B were read, so their contents are in the index and their sizes are a
/// true lower bound (`recursive_size_complete = false` with a non-zero size,
/// which the listing renders as "≥ 100 bytes"). C was never read, so it keeps
/// the honest `<dir>` placeholder — and, crucially, it is the ONLY thing the
/// next search has to walk.
///
/// Before incremental marking a cancelled walk stamped ZERO coverage: A and B held rows nothing
/// had marked listed, so the whole subtree re-entered the frontier on every
/// later search and nothing ever converged.
#[test]
fn a_cancelled_walk_leaves_durable_partial_coverage() {
    let (writer, db_path, _db_dir) = setup_writer();
    let root = PathBuf::from(TREE_ROOT);
    let a = root.join("A");
    let b = a.join("B");
    let c = b.join("C");
    seed_chain(&db_path, &a, &writer);

    let cancel = CancellationToken::new();
    let reader = MockTree::new()
        .dir_at(a.clone(), level(Some("B")))
        .dir_at(b.clone(), level(Some("C")))
        .dir_at(c.clone(), level(None))
        .cancel_when_reading(c.clone())
        .reader(&cancel);

    let result = cover_subtree_with_reader(&a, &IndexPathSpace::root(), &writer, None, &cancel, reader, None);
    assert!(
        matches!(result, Err(ScanError::Cancelled(_))),
        "a cancelled walk must surface the typed cancellation, got {result:?}"
    );
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let a_str = a.to_string_lossy().to_string();
    let b_str = b.to_string_lossy().to_string();
    let c_str = c.to_string_lossy().to_string();

    let (a_listed, a_min) = coverage_columns(&db_path, &a_str);
    let (b_listed, b_min) = coverage_columns(&db_path, &b_str);
    let (c_listed, c_min) = coverage_columns(&db_path, &c_str);

    assert!(a_listed > 0, "A was read, so the cancelled walk must leave it listed");
    assert!(b_listed > 0, "B was read, so the cancelled walk must leave it listed");
    assert_eq!(c_listed, 0, "C was never read, so nothing may claim it was");

    assert_eq!(
        a_min, 0,
        "something under A is unlisted, so A's size stays a lower bound"
    );
    assert_eq!(
        b_min, 0,
        "something under B is unlisted, so B's size stays a lower bound"
    );
    assert_eq!(c_min, 0, "C is unknown ground");

    // A holds 10 files, B holds 10 more; C's ten were never read.
    assert_eq!(
        recursive_physical_size(&db_path, &a_str),
        Some(200),
        "A's lower bound must count everything the walk actually read"
    );
    assert_eq!(
        recursive_physical_size(&db_path, &b_str),
        Some(100),
        "B's lower bound must count its own ten files"
    );
    assert_eq!(
        recursive_physical_size(&db_path, &c_str),
        Some(0),
        "C has no size to claim, which is what renders the `<dir>` placeholder"
    );

    assert_eq!(
        frontier(&db_path, &a_str),
        vec![c_str],
        "the next search over A must walk exactly C, and nothing it already covered"
    );
}

/// Convergence has a second failure mode, and it needs no cancellation: a folder
/// the walk CAN'T READ stays `listed_epoch = 0`, so it re-enters the frontier on
/// every single search and that part of the scope never converges.
///
/// The schema carries the column for exactly this, and the walk is its only writer.
/// Every failed read gets a cause, split by WHOSE problem it is: permission denied
/// is the durable, user-fixable refusal; any other errno is ground Cmdr gave up on
/// and will retry on a backoff.
///
/// ⚠️ Leaving the non-permission case uncaused is what made a wedged mount cost a
/// full stall timeout on every search, forever: 1,497 `ETIMEDOUT` directories
/// inside one disconnected phone mount on David's machine
/// (`docs/notes/phased-vs-bulk-index-2026-08-14.md`). "It might heal" was true and
/// still left the retry re-paying full price and never converging; the backoff in
/// `writer/abandoned_retry.rs` is what buys the retry back at a price worth paying.
#[test]
fn a_folder_the_walk_cannot_read_stops_re_entering_the_frontier() {
    let (writer, db_path, _db_dir) = setup_writer();
    let root = PathBuf::from(TREE_ROOT);
    let a = root.join("A");
    let denied = a.join("denied");
    let flaky = a.join("flaky");
    let healthy = a.join("healthy");
    seed_chain(&db_path, &a, &writer);

    let cancel = CancellationToken::new();
    let reader = MockTree::new()
        // `flaky` is declared as a child but has no listing, so its read fails
        // with a plain not-found — the non-permission errno shape.
        .dir_at(a.clone(), vec![dir("denied"), dir("flaky"), dir("healthy")])
        .dir_at(healthy.clone(), level(None))
        .denied_at(denied.clone())
        .reader(&cancel);

    cover_subtree_with_reader(&a, &IndexPathSpace::root(), &writer, None, &cancel, reader, None).expect("walk A");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
    let unreadable_flag = |path: &Path| {
        let id = crate::indexing::store::resolve_path(&conn, &path.to_string_lossy())
            .expect("resolve")
            .expect("row");
        IndexStore::get_unreadable_cause_by_id(&conn, id)
            .expect("the unreadable cause")
            .expect("row")
    };
    assert_eq!(
        unreadable_flag(&denied),
        Some(UnreadableCause::Denied),
        "permission denied is durable, so mark it — and as a refusal, which is the half a user can act on"
    );
    assert_eq!(
        unreadable_flag(&flaky),
        Some(UnreadableCause::Abandoned),
        "any other errno is ground Cmdr gave up on: recorded, so it stops costing a read per search"
    );
    assert_eq!(
        unreadable_flag(&healthy),
        None,
        "❌ and a folder the walk actually read is never condemned"
    );

    let mut map = coverage_for_scope(
        &conn,
        &a.to_string_lossy(),
        &a.to_string_lossy(),
        CoverageDimension::Listing,
    )
    .expect("coverage");
    map.frontier.sort();
    map.permission_denied.sort();
    map.abandoned.sort();
    assert!(
        map.frontier.is_empty(),
        "nothing is offered to the next walk: every folder here was either read or recorded, got {:?}",
        map.frontier
    );
    assert_eq!(
        map.permission_denied,
        vec![denied.to_string_lossy().to_string()],
        "and the refusal is reported to the user instead of walked again"
    );
    assert_eq!(
        map.abandoned,
        vec![flaky.to_string_lossy().to_string()],
        "❌ never in `permission_denied`: offering Full Disk Access for a dead mount is advice that does nothing"
    );
}

/// The regression anchor for the trap that made a benchmark arm look 2× faster
/// while silently indexing 1.28M fewer entries: a mark computed BEFORE the walk's
/// own `MarkDirsListed` has landed condemns everything the walk listed but hasn't
/// stamped yet. It reads exactly like a win, and the only evidence is an entry
/// count that is quietly short.
///
/// So: over a tree where one folder fails and the rest are fine, recording the
/// failure must cost the walk NOTHING. Every folder that was read stays listed at
/// the current epoch with no cause, every file it holds is in the index, and the
/// coverage answer is complete except for the one folder that really did fail.
///
/// What makes that structurally true rather than lucky: the marks ride the writer
/// channel behind the rows and the `MarkDirsListed` that stamp them
/// (`insert_visitor.rs`'s `Pending`, then `send_unreadable_marks` after
/// `visitor.finish()`), and the condemned ids are only ever the ones a read failed
/// on — never "whatever is still unlisted".
#[test]
fn marking_abandoned_ground_costs_no_coverage() {
    let (writer, db_path, _db_dir) = setup_writer();
    let root = PathBuf::from(TREE_ROOT);
    let a = root.join("A");
    let b = a.join("B");
    let c = b.join("C");
    let wedged = b.join("wedged");
    seed_chain(&db_path, &a, &writer);

    let cancel = CancellationToken::new();
    let reader = MockTree::new()
        .dir_at(a.clone(), level(Some("B")))
        // `wedged` has no listing, so its read fails with a non-permission errno.
        .dir_at(b.clone(), {
            let mut children = level(Some("C"));
            children.push(dir("wedged"));
            children
        })
        .dir_at(c.clone(), level(None))
        .reader(&cancel);

    cover_subtree_with_reader(&a, &IndexPathSpace::root(), &writer, None, &cancel, reader, None).expect("walk A");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
    let epoch = IndexStore::read_current_epoch(&conn).expect("epoch");
    let row = |path: &Path| {
        let id = crate::indexing::store::resolve_path(&conn, &path.to_string_lossy())
            .expect("resolve")
            .expect("row");
        let listed: u64 = conn
            .query_row("SELECT listed_epoch FROM entries WHERE id = ?1", [id], |r| r.get(0))
            .expect("listed_epoch");
        let cause = IndexStore::get_unreadable_cause_by_id(&conn, id)
            .expect("cause")
            .expect("row");
        (listed, cause)
    };

    for read_folder in [&a, &b, &c] {
        assert_eq!(
            row(read_folder),
            (epoch, None),
            "{} was read, so the mark must leave it listed and uncondemned",
            read_folder.display()
        );
    }
    assert_eq!(row(&wedged), (0, Some(UnreadableCause::Abandoned)));

    // 30 files across A, B, and C — the count that would have gone quietly short.
    let files: u64 = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE is_directory = 0", [], |r| r.get(0))
        .expect("count files");
    assert_eq!(files, 30, "every file the walk read is still in the index");

    let map = coverage_for_scope(
        &conn,
        &a.to_string_lossy(),
        &a.to_string_lossy(),
        CoverageDimension::Listing,
    )
    .expect("coverage");
    assert!(map.frontier.is_empty(), "nothing to re-walk, got {:?}", map.frontier);
    assert_eq!(map.abandoned, vec![wedged.to_string_lossy().to_string()]);
}

// ── 2. A walk never removes a row it did not write ───────────────────

/// A frontier node CAN hold a listed descendant, so the destructive delete that
/// `scan_subtree` opens with is a real hazard rather than a theoretical one.
///
/// The reachable sequence needs no cancellation at all: FSEvents verification
/// upserts newly-seen children under a directory WITHOUT marking that directory
/// listed (`watch/event_loop/verification.rs` sends `UpsertEntryV2`, never
/// `MarkDirsListed`), and then scans each new child directory, which does mark
/// it. The parent is left at `listed_epoch = 0` — a frontier node by the descent
/// rule — sitting above ground the index genuinely knows.
///
/// This is what makes `cover` refuse to delete: it would be deleting rows it did
/// not write, on a node search picked precisely because it looked empty.
#[test]
fn a_frontier_node_can_hold_a_listed_descendant() {
    let (writer, db_path, _db_dir) = setup_writer();
    let root = PathBuf::from(TREE_ROOT);
    let f = root.join("F");
    let g = f.join("G");
    seed_chain(&db_path, &f, &writer);

    // What verification does: G's row appears under the unlisted F …
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: crate::indexing::store::resolve_path(
                &IndexStore::open_read_connection(&db_path).unwrap(),
                &f.to_string_lossy(),
            )
            .unwrap()
            .unwrap(),
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
    writer.flush_blocking().expect("flush");

    // … and then G itself gets walked and marked.
    let cancel = CancellationToken::new();
    let reader = MockTree::new().dir_at(g.clone(), level(None)).reader(&cancel);
    cover_subtree_with_reader(&g, &IndexPathSpace::root(), &writer, None, &cancel, reader, None).expect("walk G");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let (f_listed, _) = coverage_columns(&db_path, &f.to_string_lossy());
    let (g_listed, _) = coverage_columns(&db_path, &g.to_string_lossy());
    assert_eq!(f_listed, 0, "F was never listed, so a search sees it as frontier");
    assert!(g_listed > 0, "G was walked, so the index genuinely knows its contents");
    assert_eq!(
        frontier(&db_path, &f.to_string_lossy()),
        vec![f.to_string_lossy().to_string()],
        "the frontier cuts at F and never looks below it, which is what puts G in the blast radius"
    );
}

/// The data-safety anchor: covering a frontier node must never remove a row it
/// did not write.
///
/// `scan_subtree` opens with `DeleteDescendantsById(root)` because it rebuilds a
/// subtree it already indexed. A search-driven walk is the opposite case — it is
/// handed ground the index has no claim on — so the same delete would throw away
/// whatever a verification pass or an earlier interrupted walk had already
/// learned, and leave the index worse than it found it.
///
/// The refusal is the mechanism: an add-only walk over pre-existing rows isn't
/// safe either (its fresh ids collide, `INSERT OR IGNORE` drops them, and the
/// subtree below a dropped id is orphaned), so `cover_subtree` checks and hands
/// the case to the serial reconcile instead —
/// `lifecycle::cover::tests::a_non_virgin_frontier_node_is_repaired_without_losing_rows`
/// is the other half of this.
#[test]
fn covering_a_frontier_node_never_removes_a_row_it_did_not_write() {
    let (writer, db_path, _db_dir) = setup_writer();
    let root = PathBuf::from(TREE_ROOT);
    let f = root.join("F");
    let g = f.join("G");
    seed_chain(&db_path, &f, &writer);

    let f_id = crate::indexing::store::resolve_path(
        &IndexStore::open_read_connection(&db_path).unwrap(),
        &f.to_string_lossy(),
    )
    .unwrap()
    .unwrap();
    writer
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
    writer.flush_blocking().expect("flush");

    let cancel = CancellationToken::new();
    let reader = MockTree::new().dir_at(g.clone(), level(None)).reader(&cancel);
    cover_subtree_with_reader(&g, &IndexPathSpace::root(), &writer, None, &cancel, reader, None).expect("walk G");
    writer.flush_blocking().expect("flush");

    let g_rows_before = child_ids(&db_path, &g.to_string_lossy());
    assert_eq!(g_rows_before.len(), 10, "precondition: G's ten files are in the index");

    // Now cover F, the frontier node above it. G is unreadable this time round,
    // so anything the walk removes there is gone for good: a delete-then-rewalk
    // can't hide behind re-discovering the same names.
    let cancel = CancellationToken::new();
    let reader = MockTree::new()
        .dir_at(f.clone(), level(Some("G")))
        .denied_at(g.clone())
        .reader(&cancel);
    let outcome = cover_subtree_with_reader(&f, &IndexPathSpace::root(), &writer, None, &cancel, reader, None);
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    assert!(
        matches!(outcome, Err(ScanError::NotVirgin)),
        "F isn't virgin ground, so the add-only walk must refuse it, got {outcome:?}"
    );
    assert_eq!(
        child_ids(&db_path, &g.to_string_lossy()),
        g_rows_before,
        "covering F must leave every row it did not write exactly where it was"
    );
    let (g_listed, _) = coverage_columns(&db_path, &g.to_string_lossy());
    assert!(
        g_listed > 0,
        "and G must keep the coverage an earlier pass earned for it"
    );
}

/// The ids of a directory's children in the index, sorted, so a test can tell
/// "still there" from "deleted and re-inserted under new ids".
fn child_ids(db_path: &Path, path: &str) -> Vec<i64> {
    let conn = IndexStore::open_read_connection(db_path).expect("read connection");
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
