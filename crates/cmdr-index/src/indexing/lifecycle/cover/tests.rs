//! The cover driver over a real filesystem: what it emits, what it fills in, and
//! what it refuses to touch.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

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
            writer: self.writer.clone(),
            space: IndexPathSpace::root(),
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

/// Drain a walk, collecting every entry it emitted.
fn drain(walk: CoverWalk) -> (Vec<CoveredEntry>, CoverOutcome) {
    let mut entries = Vec::new();
    while let Some(batch) = walk.next_batch() {
        entries.extend(batch);
    }
    (entries, walk.finish())
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
    // M3b's cold-bootstrap case, not this one.)
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

// ── Cancellation ─────────────────────────────────────────────────────

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
