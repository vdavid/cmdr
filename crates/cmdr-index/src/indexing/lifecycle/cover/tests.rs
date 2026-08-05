//! The cover driver over a real filesystem: what it emits, what it fills in, and
//! what it refuses to touch.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::*;
use crate::indexing::store::ROOT_ID;
use crate::indexing::writer::WriteMessage;
use cmdr_fs::pluralize::{pluralize, pluralize_with};

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

// ── The primitive choice, measured ────────────────────────────────────

/// Which primitive should cover a frontier: the parallel walker or the serial
/// reconcile? Measured on a REAL tree rather than trusted from the in-tree
/// full-volume numbers, because a frontier is a different workload (all-new
/// ground, a bulk add, never an incremental diff).
///
/// Both are pointed at the same directory with the same fresh empty index, and
/// the time includes draining the writer, because both are writing the same rows
/// and a walk that just fills a queue faster hasn't covered anything yet. Each
/// runs after a warm-up pass over the tree, so neither pays for the other's cold
/// page cache.
///
/// Not a correctness check, so it's `#[ignore]`d. Run it in RELEASE, which is
/// where the difference is honest:
///
/// ```sh
/// CMDR_COVER_BENCH_ROOT=/Applications \
///   cargo test -p cmdr-index --release --lib -- --ignored --nocapture measure_cover_primitives
/// ```
///
/// Results and the call they back: `docs/notes/cover-walk-primitive-2026-08-05.md`.
#[test]
#[ignore = "benchmark over a real tree; run manually with --nocapture"]
fn measure_cover_primitives() {
    let root = PathBuf::from(
        std::env::var("CMDR_COVER_BENCH_ROOT").expect("set CMDR_COVER_BENCH_ROOT to a real directory to walk"),
    );
    assert!(root.is_dir(), "{} isn't a directory", root.display());

    // Emit via a stderr handle rather than `println!` (crate-banned), same as the
    // `bulk_read` bench next door.
    use std::io::Write;
    let mut out = std::io::stderr();

    // Warm the page cache so the first primitive doesn't pay for the second.
    let warm = std::time::Instant::now();
    let warmed = walkdir_count(&root);
    let _ = writeln!(
        out,
        "warm-up: {} under {} in {:?}",
        pluralize_with(warmed, "entry", "entries"),
        root.display(),
        warm.elapsed()
    );

    let parallel = measure_one(&root, Primitive::Parallel);
    let serial = measure_one(&root, Primitive::Serial);

    let _ = writeln!(out, "\n── {} ──", root.display());
    for (name, (elapsed, rows, dirs)) in [("parallel walker", parallel), ("serial reconcile", serial)] {
        let _ = writeln!(
            out,
            "{name:>17}: {elapsed:>10.2?}  {}, {} listed",
            pluralize(rows, "row"),
            pluralize(dirs, "dir"),
        );
    }
    let (p_elapsed, p_rows, _) = parallel;
    let (s_elapsed, s_rows, _) = serial;
    let _ = writeln!(
        out,
        "{:>17}: {:.1}x, and the parallel walk wrote {:.2}% of the serial walk's rows",
        "verdict",
        s_elapsed.as_secs_f64() / p_elapsed.as_secs_f64(),
        100.0 * p_rows as f64 / s_rows.max(1) as f64,
    );
}

#[derive(Clone, Copy)]
enum Primitive {
    Parallel,
    Serial,
}

/// One primitive over `root` into a fresh index: wall clock including the writer
/// drain, the row count it produced, and how many directories it marked listed.
fn measure_one(root: &Path, primitive: Primitive) -> (std::time::Duration, u64, u64) {
    let db_dir = tempfile::tempdir().expect("temp db dir");
    let db_path = db_dir.path().join("bench-index.db");
    IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

    // Seed the chain down to the walk root, exactly as a real frontier node has it.
    {
        let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
        let mut parent_id = ROOT_ID;
        for component in root.to_string_lossy().split('/').filter(|c| !c.is_empty()) {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None)
                    .expect("seed"),
            };
        }
        let next_id = IndexStore::get_next_id(&conn).expect("next id");
        writer.next_id().fetch_max(next_id, Ordering::Relaxed);
    }

    let space = IndexPathSpace::root();
    let cancel = CancellationToken::new();
    let start = std::time::Instant::now();
    match primitive {
        Primitive::Parallel => {
            cover_subtree(root, &space, &writer, None, &cancel).expect("parallel walk");
        }
        Primitive::Serial => {
            let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
            crate::indexing::reconcile::reconciler::reconcile_subtree(root, &space, &conn, &writer, &cancel)
                .expect("serial reconcile");
        }
    }
    writer.flush_blocking().expect("flush");
    let elapsed = start.elapsed();
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read connection");
    let rows: u64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .expect("count rows");
    let dirs: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE is_directory = 1 AND listed_epoch > 0",
            [],
            |r| r.get(0),
        )
        .expect("count listed dirs");
    (elapsed, rows, dirs)
}

/// A plain recursive count, used only to warm the page cache.
fn walkdir_count(root: &Path) -> u64 {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            count += 1;
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(entry.path());
            }
        }
    }
    count
}

// ── Cancellation ─────────────────────────────────────────────────────

/// A walk stopped partway reports what it DID cover, not zero.
///
/// The totals are what M5's "walked 40,000 folders" line reads, and a cancelled
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
