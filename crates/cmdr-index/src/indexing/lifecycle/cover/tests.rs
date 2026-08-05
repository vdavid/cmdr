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
    assert_eq!(entries.len(), 1, "and delivered what it found there");
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
            bootstrap::ensure_walkable(&f.context(), &root.join("mixed/inner")),
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
            bootstrap::ensure_walkable(&f.context(), &root.join("gone")),
            Err(bootstrap::NotWalkable::NotADirectoryOnDisk(_))
        ),
        "a path that isn't there can't be walked"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).expect("symlink");
        assert!(
            matches!(
                bootstrap::ensure_walkable(&f.context(), &root.join("link")),
                Err(bootstrap::NotWalkable::NotADirectoryOnDisk(_))
            ),
            "a symlink is not a directory to descend into"
        );
    }
    assert!(f.child_ids(&f.path("")).is_empty(), "and neither one left a row behind");
}

// ── The cold volume ──────────────────────────────────────────────────

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
    events: std::sync::Arc<crate::indexing::events::RecordingSink>,
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

    /// The same, with the registered volume shaped by `describe` — a share, a
    /// phone, whatever the refusal under test needs.
    fn with_volume(
        volume_id: &'static str,
        describe: impl FnOnce(cmdr_fs::volume::InMemoryVolume) -> cmdr_fs::volume::InMemoryVolume,
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
            std::sync::Arc::new(describe(
                cmdr_fs::volume::InMemoryVolume::new("Cold").with_root(tree.path()),
            )),
        );
        let events = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
        let (index, installed) = crate::indexing::handle::Index::builder()
            .data_dir(data.path())
            .volumes(std::sync::Arc::clone(&volumes) as std::sync::Arc<_>)
            .events(std::sync::Arc::clone(&events) as std::sync::Arc<dyn crate::indexing::events::EventSink>)
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

    fn coverage(&self, path: &str) -> crate::indexing::read::coverage::CoverageMap {
        self.index
            .coverage(self.volume_id, path, CoverageDimension::Listing)
            .expect("the volume answers for its own coverage")
    }

    /// Walk one scope to the end, waiting for the rows to land.
    fn cover(&self, scope: &str) -> CoverOutcome {
        let walk = self
            .index
            .cover(self.volume_id, vec![scope.to_string()], CoverageDimension::Listing)
            .expect("the drive is walkable");
        let (_, outcome) = drain(walk);
        cmdr_fs::testing::wait_until(
            std::time::Duration::from_secs(10),
            "the walked scope to read as covered",
            || {
                let covered = self.coverage(scope);
                covered.frontier.is_empty() && covered.unreadable.is_empty()
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
    assert_eq!(outcome.entries_found, 2, "inner/ and inner/found.txt");

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
        crate::indexing::volume::IndexVolumeKind::Local,
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

    cmdr_fs::testing::wait_until_async(
        std::time::Duration::from_secs(20),
        "the full scan to cover the whole drive",
        || drive.coverage(&volume_root).frontier.is_empty(),
    )
    .await;
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
                CoverageDimension::Listing
            ),
            Err(crate::indexing::handle::IndexError::NotIndexed { .. })
        ),
        "an unmounted drive can't be bootstrapped"
    );
}

/// A share and a phone are refused rather than walked locally.
///
/// The LOCAL guarded walker must never be pointed at a network mount — it would
/// traverse the share over syscalls that block for minutes, and the rows it wrote
/// would fight the trait scanner's. Their scoped walk is the `Volume`-trait one
/// (M3d); until it exists, a search over them is honestly index-only. Classified
/// by typed facts, never by the volume id.
#[test]
fn a_share_or_a_phone_is_not_walked_locally() {
    {
        let share = ColdDrive::with_volume("cover-cold-share-test", |volume| {
            volume
                .with_local_fs_access()
                .with_smb_connection_state(cmdr_fs::volume::SmbConnectionState::Direct)
        });
        assert!(
            matches!(
                share
                    .index
                    .cover(share.volume_id, vec![share.path("x")], CoverageDimension::Listing),
                Err(crate::indexing::handle::IndexError::NotIndexed { .. })
            ),
            "a share is not local ground"
        );
    }
    // A phone's files exist only over PTP: no local path to walk at all.
    let phone = ColdDrive::with_volume("cover-cold-phone-test", |volume| volume);
    assert!(
        matches!(
            phone
                .index
                .cover(phone.volume_id, vec![phone.path("x")], CoverageDimension::Listing),
            Err(crate::indexing::handle::IndexError::NotIndexed { .. })
        ),
        "a volume with no local filesystem access is not local ground"
    );
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
