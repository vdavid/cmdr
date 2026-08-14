//! Phased cover walks against one bulk build, over a real `/`.
//!
//! A throwaway measurement harness, not a shape any product code should copy. It
//! answers one question before the phase machine gets written: does covering a
//! drive as a sequence of stitched cover walks cost so much more wall clock than
//! today's truncate-and-bulk-build that the whole idea should be dropped, and does
//! it buy the thing it is for, which is `~/Downloads` being searchable in seconds
//! rather than minutes.
//!
//! One arm per `#[ignore]`d test, run one process at a time so the memory
//! high-water mark belongs to that arm alone. The first four are the comparison;
//! the rest exist to attribute the difference to a cause.
//!
//! - [`bulk_build`] — `scan_volume` from `/`, which is what ships today.
//! - [`phased_stitch_depth_1`] / [`phased_stitch_depth_2`] — the stitch plus
//!   priority roots, `$HOME`, then the `/` frontier, walked one frontier root at a
//!   time. The two differ only in how deep the stitch goes under the `$HOME` and
//!   `/` phase roots, which is what decides how long a newly-queued root waits.
//! - [`phased_while_browsing`] — depth 2 with a pane reading listings mid-walk and
//!   a second, search-shaped cover walk on disjoint ground.
//! - [`phased_marking_ground_the_walk_gave_up_on`],
//!   [`phased_marking_and_draining_once_per_phase`],
//!   [`phased_draining_once_per_phase`], [`phased_four_roots_at_a_time`] — one
//!   suspected cost removed at a time, so the note can say where the wall clock
//!   goes rather than guess.
//!
//! Each arm runs twice: once with `CMDR_PHASE_BENCH_NO_PROBE=1` for a wall clock
//! with no instrument in it, once without for the coverage timestamps. Run one at
//! a time, in release:
//!
//! ```sh
//! cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
//!   indexing::lifecycle::phased_bench::bulk_build
//! ```
//!
//! Results and the call they back: `docs/notes/phased-vs-bulk-index-2026-08-14.md`.
//!
//! ## The stitch, which is the whole reason this harness is not trivial
//!
//! A cover walk marks only the directories it reads, so after `~/Downloads` is
//! covered the frontier for `$HOME` is still `["$HOME"]` and a `cover` over it
//! would hit `ScanError::NotVirgin` and fall back to the serial repair. Each phase
//! is therefore preceded by a shallow stitch ([`Bench::stitch_dir`]): read one
//! directory, upsert its children (files included), flush, and mark that one
//! directory listed at the CURRENT epoch. After it, the coverage descent walks
//! through the stitched ancestors and cuts at each genuinely unlisted child, so
//! every frontier root handed to the walker is virgin and the big phases become
//! many small walks.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use crate::NoopEventSink;
use crate::indexing::IndexPathSpace;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::read::coverage::{CoverageDimension, coverage_for_scope};
use crate::indexing::scanner::{
    ScanConfig, WalkHeartbeat, cover_subtree, exclusion_policy_stamp_message, scan_volume, should_exclude,
};
use crate::indexing::store::{IndexStore, ROOT_ID, UnreadableCause, resolve_path};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

/// How often the sampler asks the index what it covers and the process how much
/// memory it holds. The coverage question costs a recursive query per target, so
/// this is both the granularity every coverage timestamp is reported at and a cost
/// the measurement imposes on what it measures — which is why the sampler reports
/// how long its own queries took.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// How long a browsing pane sits on a folder before the harness moves it on.
const BROWSE_DWELL: Duration = Duration::from_secs(3);

/// When the browsing arm starts its second, search-shaped walk. Late enough that
/// the phase machine is well inside `$HOME` and the two are genuinely disjoint.
const SEARCH_WALK_AT: Duration = Duration::from_secs(30);

/// The ground that arm's search-shaped walk takes: a top-level tree the `/` phase
/// reaches last, so at [`SEARCH_WALK_AT`] it is still virgin.
const SEARCH_WALK_ROOT: &str = "/Applications";

// ── The arms ─────────────────────────────────────────────────────────

#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn bulk_build() {
    run_bulk("bulk build (today's `scan_volume` from `/`)", false);
}

/// The bulk build plus every other thing `start_scan` and the completion handler
/// put on the SAME writer, so the comparison isn't the phased arms paying for
/// aggregation while the baseline skips it.
///
/// Adds `set_expected_total_entries`, `BackfillMissingDirStats`, and a
/// `ScanProgressReporter`-shaped 500 ms tick that fires `ComputePartialAggregates`
/// every tenth pass with `AggSource::Maps` over the hot paths a pane would be
/// showing — the pump the phase machine would have to run too.
#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn bulk_build_with_the_full_post_scan_sequence() {
    run_bulk("bulk build, with the whole post-scan sequence `start_scan` runs", true);
}

fn run_bulk(label: &'static str, full_sequence: bool) {
    let bench = Bench::new(label);
    let sampler = bench.start_sampling();
    let pump = full_sequence.then(|| bench.start_partial_aggregation_pump());

    let config = ScanConfig {
        root: PathBuf::from("/"),
        ..Default::default()
    };
    let (_handle, thread) = scan_volume(config, &bench.writer, CancellationToken::new()).expect("start the scan");
    let summary = thread.join().expect("scan thread").expect("scan completed");
    if let Some(pump) = pump {
        pump.stop();
    }
    if full_sequence {
        // `scan_completion.rs`: the denominator the writer reports progress
        // against, then the backfill that catches directories created by events
        // that landed mid-walk.
        bench.writer.set_expected_total_entries(summary.total_entries);
        bench
            .writer
            .send(WriteMessage::BackfillMissingDirStats)
            .expect("backfill");
    }
    bench.writer.flush_blocking().expect("flush");

    let mut report = bench.finish(sampler);
    report.walked_entries = summary.total_entries;
    report.print();
}

#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_stitch_depth_1() {
    run_phased("phased, stitch depth 1", Phased::depth(1));
}

#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_stitch_depth_2() {
    run_phased("phased, stitch depth 2", Phased::depth(2));
}

#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_while_browsing() {
    run_phased(
        "phased, stitch depth 2, while browsing and searching",
        Phased {
            browsing: true,
            ..Phased::depth(2)
        },
    );
}

/// The same walk with the writer drain moved from every root to every phase.
///
/// Not a candidate shape — a phase machine that only flushes per phase can't
/// report a root as covered when it finishes — but the one measurement that
/// separates "the walks cost this much" from "the walk and the writer stopped
/// overlapping", which is the difference between a design problem and a
/// batch-size problem.
#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_draining_once_per_phase() {
    run_phased(
        "phased, stitch depth 2, writer drained once per phase",
        Phased {
            drain_per_root: false,
            ..Phased::depth(2)
        },
    );
}

/// Walking several frontier roots at once, to find out whether a phased arm is
/// slow because of the work it does or because of how little of the machine one
/// small subtree can keep busy.
///
/// ❌ Not a candidate shape either: the plan's join rule is one walk at a time, so
/// a machine built this way would need a different answer for cancellation and for
/// the claim. It is here to attribute the cost.
#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_four_roots_at_a_time() {
    run_phased(
        "phased, stitch depth 2, four frontier roots walked at once",
        Phased {
            concurrent_roots: 4,
            ..Phased::depth(2)
        },
    );
}

/// What the phased shape costs once a walk that gave up on ground says so, so no
/// later phase's frontier offers that ground again.
///
/// The mechanism already exists — `MarkDirsUnreadable` is what a denied or declined
/// directory gets — and nothing sends it for a directory the walker ABANDONED. On a
/// machine with a stalled File Provider domain that turns every later phase into a
/// re-run of the same timeouts.
#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_marking_ground_the_walk_gave_up_on() {
    run_phased(
        "phased, stitch depth 2, abandoned ground marked so no later phase re-offers it",
        Phased {
            mark_abandoned: true,
            ..Phased::depth(2)
        },
    );
}

/// Both fixes at once: mark abandoned ground AND drain the writer once per phase
/// rather than once per frontier root.
#[test]
#[ignore = "benchmark over the real boot volume; run manually with --nocapture"]
fn phased_marking_and_draining_once_per_phase() {
    run_phased(
        "phased, stitch depth 2, abandoned ground marked and the writer drained once per phase",
        Phased {
            mark_abandoned: true,
            drain_per_root: false,
            ..Phased::depth(2)
        },
    );
}

/// How one phased arm differs from the next.
#[derive(Clone, Copy)]
struct Phased {
    /// How deep the stitch goes under the `$HOME` and `/` phase roots. 1 makes
    /// their children the frontier roots, 2 their grandchildren.
    depth: usize,
    /// Whether a pane reads listings and a search-shaped walk runs alongside.
    browsing: bool,
    /// Whether each frontier root ends with a blocking writer drain, which is what
    /// one `cover()` call per root does today.
    drain_per_root: bool,
    /// How many frontier roots are walked at the same time. 1 is the plan's shape.
    concurrent_roots: usize,
    /// Whether a walk that gave up on ground records that, so a later phase's
    /// frontier stops offering it.
    mark_abandoned: bool,
}

impl Phased {
    fn depth(depth: usize) -> Self {
        Self {
            depth,
            browsing: false,
            drain_per_root: true,
            concurrent_roots: 1,
            mark_abandoned: false,
        }
    }
}

/// The phased arm: priority roots in order, then `$HOME`, then the `/` frontier,
/// one frontier root per walk with the queue check the real machine would do in
/// between (here: nothing to check, so the loop just continues).
fn run_phased(label: &'static str, config: Phased) {
    let bench = Bench::new(label);
    bench.drain_per_root.store(config.drain_per_root, Ordering::Relaxed);
    bench
        .concurrent_roots
        .store(config.concurrent_roots as u64, Ordering::Relaxed);
    bench.mark_abandoned.store(config.mark_abandoned, Ordering::Relaxed);
    let sampler = bench.start_sampling();
    let browser = config.browsing.then(|| bench.start_browsing());

    // Rank 0: the folders the app would name. Each is a frontier root taken
    // whole, so only its ancestors get stitched.
    for root in priority_roots() {
        bench.stitch_ancestors_of(&root);
        bench.cover_frontier_root(&root);
        // ⚠️ Marked under the ROOT that was just walked, ❌ never under `$HOME`:
        // at this point everything else in home is merely not walked YET, and
        // condemning it would cut it out of the phase that was going to cover it.
        bench.drain();
        bench.mark_ground_the_walk_gave_up_on(&root);
    }
    bench.drain();
    // Rank 2 and 3. Both are phase roots rather than targets, so the stitch goes
    // through the root itself: its children are what the walker takes.
    bench.run_phase(&home_dir(), config.depth);
    bench.run_phase(Path::new("/"), config.depth);

    if let Some(browser) = browser {
        browser.stop();
    }
    let mut report = bench.finish(sampler);
    report.walked_entries = report.total_entries;
    report.print();
}

// ── What the app would answer ────────────────────────────────────────

/// This machine's home directory.
fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME"))
}

/// The priority roots, hardcoded in the order `apps/desktop`'s ranking produces
/// (tabs, favorites, the standard home folders that hold something, then the cloud
/// roots). The real seam lives app-side and this crate can't reach it; the walk
/// order is the only payload, so a hardcoded stand-in measures the same thing.
///
/// `CMDR_PHASE_BENCH_ROOTS` overrides it with a colon-separated list.
fn priority_roots() -> Vec<PathBuf> {
    if let Ok(raw) = std::env::var("CMDR_PHASE_BENCH_ROOTS") {
        return raw.split(':').filter(|p| !p.is_empty()).map(PathBuf::from).collect();
    }
    let home = home_dir();
    ["Downloads", "Documents", "Desktop", "Pictures", "Movies", "Music"]
        .iter()
        .map(|name| home.join(name))
        .chain([home.join("Library/CloudStorage"), home.join("Dropbox")])
        .filter(|path| path.is_dir())
        .collect()
}

/// The paths whose coverage timestamp the note reports: every priority root, plus
/// `~/Library` (it gates the early media kick), plus `$HOME`.
///
/// ❌ Not `/`: the whole volume's answer is the arm's own wall clock, and probing
/// a six-million-entry subtree once a second would charge the thing being measured
/// for measuring it.
fn coverage_targets() -> Vec<PathBuf> {
    let home = home_dir();
    let mut targets = priority_roots();
    targets.push(home.join("Library"));
    targets.push(home);
    targets
}

/// The folders the browsing arm's pane walks through, ahead of the walker.
fn browse_path_ring() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        home.join("Library"),
        PathBuf::from("/Applications"),
        home.join("projects-git"),
        home.clone(),
        PathBuf::from("/usr/share"),
        home.join("Library/Application Support"),
    ]
}

// ── The harness ──────────────────────────────────────────────────────

/// One arm's fresh index, writer, and clock. The temp dir owns the database, so
/// nothing here can reach a real Cmdr data directory.
struct Bench {
    label: &'static str,
    _dir: tempfile::TempDir,
    db_path: PathBuf,
    writer: IndexWriter,
    space: IndexPathSpace,
    started_at: Instant,
    /// Per frontier root, in walk order. The longest walk IS the worst-case wait a
    /// newly-queued root would see.
    roots: Mutex<Vec<RootWalk>>,
    /// Frontier roots the walker refused as non-virgin. Zero is the stitch working.
    not_virgin: AtomicU64,
    /// Whether each root ends with a blocking writer drain.
    drain_per_root: AtomicBool,
    /// How many frontier roots a phase walks at the same time.
    concurrent_roots: AtomicU64,
    /// Whether a walk that gave up records it, so later phases stop re-offering
    /// that ground.
    mark_abandoned: AtomicBool,
    /// Directories marked as ground no walk will read, by that mechanism.
    marked_abandoned: AtomicU64,
    /// What the machinery around the walks costs: stitching directories, asking
    /// for a frontier, and draining the writer.
    stitched_dirs: AtomicU64,
    stitch_nanos: AtomicU64,
    frontier_query_nanos: AtomicU64,
    drain_nanos: AtomicU64,
}

/// One frontier root's walk.
#[derive(Clone)]
struct RootWalk {
    path: String,
    walk: Duration,
    entries: u64,
    /// When it ended, measured from the arm's start. What makes a phased arm's
    /// coverage timestamps derivable exactly and for free: a folder is covered
    /// once the last frontier root at or under it has finished.
    finished_at: Duration,
}

impl Bench {
    fn new(label: &'static str) -> Self {
        Self::over(label, IndexPathSpace::root())
    }

    fn over(label: &'static str, space: IndexPathSpace) -> Self {
        let dir = tempfile::tempdir().expect("temp db dir");
        let db_path = dir.path().join("phased-bench.db");
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, NoopEventSink::shared()).expect("spawn writer");

        // What `prepare_database_for_a_walk` does for a `WriterOnly` start, through
        // writer messages instead of a second write connection: seed the epoch, and
        // stamp the exclusion policy while the index is empty. Without the stamp,
        // `index_predates_exclusion_policy` answers yes and every coverage query
        // short-circuits to "walk the whole scope", so the frontier never shrinks.
        writer.send(WriteMessage::BumpCurrentEpoch).expect("seed the epoch");
        writer.send(exclusion_policy_stamp_message()).expect("stamp the policy");
        writer.flush_blocking().expect("flush the preparation");

        let bench = Self {
            label,
            _dir: dir,
            db_path,
            writer,
            space,
            started_at: Instant::now(),
            roots: Mutex::new(Vec::new()),
            not_virgin: AtomicU64::new(0),
            drain_per_root: AtomicBool::new(true),
            concurrent_roots: AtomicU64::new(1),
            mark_abandoned: AtomicBool::new(false),
            marked_abandoned: AtomicU64::new(0),
            stitched_dirs: AtomicU64::new(0),
            stitch_nanos: AtomicU64::new(0),
            frontier_query_nanos: AtomicU64::new(0),
            drain_nanos: AtomicU64::new(0),
        };
        assert!(
            !bench.coverage_short_circuits(),
            "the exclusion-policy stamp didn't land, so every coverage answer would be `walk everything`"
        );
        bench
    }

    fn read_conn(&self) -> Connection {
        IndexStore::open_read_connection(&self.db_path).expect("read connection")
    }

    /// Whether a coverage query would refuse to trust any row. Pins the plan's
    /// claim that an unstamped index makes the frontier permanent.
    fn coverage_short_circuits(&self) -> bool {
        let conn = self.read_conn();
        crate::indexing::scanner::index_predates_exclusion_policy(&conn)
    }

    // ── The stitch ───────────────────────────────────────────────────

    /// Read one directory, upsert everything in it, and mark that directory alone
    /// listed. No descent, no deletion.
    ///
    /// Files go in as well as directories: `listed_children_on` serves a
    /// directory's rows as its full contents the moment `listed_epoch` is non-zero,
    /// so a directories-only stitch would report a folder as holding no files to a
    /// user-visible consumer that same instant.
    fn stitch_dir(&self, dir: &Path) -> Option<i64> {
        let started = Instant::now();
        let id = self.stitch_dir_inner(dir);
        self.stitch_nanos
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        id
    }

    fn stitch_dir_inner(&self, dir: &Path) -> Option<i64> {
        let conn = self.read_conn();
        let id = self.resolve(&conn, dir)?;
        if IndexStore::get_listed_epoch_by_id(&conn, id)
            .ok()
            .flatten()
            .unwrap_or(0)
            > 0
        {
            return Some(id); // Already stitched, or already covered by a walk.
        }
        drop(conn);

        let Ok(entries) = std::fs::read_dir(dir) else {
            return Some(id);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if should_exclude(&path.to_string_lossy(), self.space.exclusion_scope()) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            let is_symlink = meta.file_type().is_symlink();
            let is_directory = meta.is_dir();
            let snapshot = extract_metadata(&meta, is_directory, is_symlink);
            let send = self.writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: id,
                name: entry.file_name().to_string_lossy().into_owned(),
                is_directory,
                is_symlink,
                logical_size: snapshot.logical_size,
                physical_size: snapshot.physical_size,
                modified_at: snapshot.modified_at,
                inode: snapshot.inode,
                nlink: snapshot.nlink,
            });
            send.expect("upsert a stitched child");
        }

        // Mandatory, not an optimization: `MarkDirsListed` is a PK-keyed UPDATE, so
        // marking a row still pending in an unflushed batch would leave it at
        // `listed_epoch = 0` forever.
        self.writer.flush_blocking().expect("flush the stitched children");
        let conn = self.read_conn();
        let epoch = IndexStore::read_current_epoch(&conn).expect("read the current epoch");
        drop(conn);
        self.writer
            .send(WriteMessage::MarkDirsListed { ids: vec![id], epoch })
            .expect("mark the stitched directory listed");
        self.writer.flush_blocking().expect("flush the mark");
        self.stitched_dirs.fetch_add(1, Ordering::Relaxed);
        Some(id)
    }

    /// Stitch every ancestor of `path`, from the volume root down to its parent.
    fn stitch_ancestors_of(&self, path: &Path) {
        let mut chain: Vec<&Path> = path.ancestors().skip(1).collect();
        chain.reverse();
        for ancestor in chain {
            self.stitch_dir(ancestor);
        }
    }

    /// Resolve a path to its entry id, the space's own root being the sentinel.
    ///
    /// ❌ The `/` shortcut can't come before `index_relative`: on a mount-rooted
    /// space that would root a path from outside the volume at `ROOT_ID`, and the
    /// stitch would then invent the boot disk's top level inside a temp tree.
    fn resolve(&self, conn: &Connection, path: &Path) -> Option<i64> {
        let index_path = self.space.index_relative(&path.to_string_lossy())?;
        if index_path == "/" || index_path.is_empty() {
            return Some(ROOT_ID);
        }
        resolve_path(conn, &index_path).ok().flatten()
    }

    // ── The phases ───────────────────────────────────────────────────

    /// One phase: stitch down to `depth` under `root`, then walk what's left of its
    /// frontier, one root at a time.
    fn run_phase(&self, root: &Path, depth: usize) {
        self.stitch_ancestors_of(root);
        self.stitch_dir(root);
        if depth >= 2 {
            for child in self.child_dirs(root) {
                self.stitch_dir(&child);
            }
        }
        let frontier = self.frontier_under(root);
        let width = self.concurrent_roots.load(Ordering::Relaxed).max(1) as usize;
        for group in frontier.chunks(width) {
            std::thread::scope(|scope| {
                for frontier_root in group {
                    scope.spawn(|| self.cover_frontier_root(Path::new(frontier_root)));
                }
            });
        }
        self.drain();
        self.mark_ground_the_walk_gave_up_on(root);
    }

    /// Record the directories a finished phase left unlisted under `root`, so the
    /// coverage descent stops offering them to every phase that comes after.
    ///
    /// The phase's walks have listed everything they could reach, so what is still
    /// unlisted under the phase root is exactly what they could not read.
    ///
    /// ❌ The signal is NOT `heartbeat.abandoned_count()`. That counts stall
    /// timeouts and consecutive-failure pruning; a directory whose `readdir` fails
    /// with anything other than permission-denied is left plain unlisted with no
    /// cause and no give-up (`insert_visitor.rs:406-428`, deliberately, so a
    /// transient error gets retried), and those are the ones that pile up.
    ///
    /// ⚠️ Runs only AFTER the phase's drain: a walk sends its `MarkDirsListed` last,
    /// so querying before that commits reads thousands of perfectly good
    /// directories as unlisted. `Denied` stands in for the cause; the real fix wants
    /// its own variant, since "the walk could not read it" is neither a permission
    /// the user can grant nor a refusal Cmdr chose.
    fn mark_ground_the_walk_gave_up_on(&self, root: &Path) {
        if !self.mark_abandoned.load(Ordering::Relaxed) {
            return;
        }
        let conn = self.read_conn();
        let Some(root_id) = self.resolve(&conn, root) else {
            return;
        };
        let Ok(mut statement) = conn.prepare(
            "WITH RECURSIVE sub(id) AS ( \
               SELECT ?1 \
               UNION ALL \
               SELECT e.id FROM entries e JOIN sub s ON e.parent_id = s.id WHERE e.is_directory = 1 \
             ) \
             SELECT s.id FROM sub s JOIN entries e ON e.id = s.id \
             WHERE e.is_directory = 1 AND e.listed_epoch = 0 AND e.unreadable_cause = 0",
        ) else {
            return;
        };
        let Ok(rows) = statement.query_map([root_id], |row| row.get::<_, i64>(0)) else {
            return;
        };
        let ids: Vec<i64> = rows.filter_map(Result::ok).collect();
        if ids.is_empty() {
            return;
        }
        self.marked_abandoned.fetch_add(ids.len() as u64, Ordering::Relaxed);
        let _ = self.writer.send(WriteMessage::MarkDirsUnreadable {
            ids,
            cause: UnreadableCause::Denied,
        });
    }

    /// Wait for the writer to commit everything the walks have sent it.
    fn drain(&self) {
        let started = Instant::now();
        self.writer.flush_blocking().expect("drain the writer");
        self.drain_nanos
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }

    /// The directories directly under `path` that the index now holds a row for.
    fn child_dirs(&self, path: &Path) -> Vec<PathBuf> {
        let conn = self.read_conn();
        let Some(id) = self.resolve(&conn, path) else {
            return Vec::new();
        };
        let mut statement = conn
            .prepare("SELECT name FROM entries WHERE parent_id = ?1 AND is_directory = 1 AND is_symlink = 0")
            .expect("prepare the child query");

        statement
            .query_map([id], |row| row.get::<_, String>(0))
            .expect("query children")
            .filter_map(Result::ok)
            .map(|name| path.join(name))
            .collect()
    }

    /// What still needs walking under `path`, as the product's own coverage query
    /// answers it.
    fn frontier_under(&self, path: &Path) -> Vec<String> {
        let started = Instant::now();
        let conn = self.read_conn();
        let display = path.to_string_lossy().into_owned();
        let Some(index_path) = self.space.index_relative(&display) else {
            return Vec::new();
        };
        let frontier = coverage_for_scope(&conn, &index_path, &display, CoverageDimension::Listing)
            .map(|map| map.frontier)
            .unwrap_or_default();
        self.frontier_query_nanos
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        frontier
    }

    /// Walk one frontier root the way the phase machine would: the parallel walker,
    /// then a flush, so the coverage the walk earned is readable before the next
    /// root starts.
    fn cover_frontier_root(&self, root: &Path) {
        let started = Instant::now();
        let heartbeat = WalkHeartbeat::new();
        let cancel = CancellationToken::new();
        let entries = match cover_subtree(root, &self.space, &self.writer, None, &cancel, &heartbeat) {
            Ok(summary) => summary.total_entries,
            Err(e) => {
                if matches!(e, crate::indexing::scanner::ScanError::NotVirgin) {
                    self.not_virgin.fetch_add(1, Ordering::Relaxed);
                }
                0
            }
        };
        let walk = started.elapsed();
        if self.drain_per_root.load(Ordering::Relaxed) {
            self.drain();
        }
        self.roots.lock_ignore_poison().push(RootWalk {
            path: root.to_string_lossy().into_owned(),
            walk,
            entries,
            finished_at: self.started_at.elapsed(),
        });
    }

    // ── Instruments ──────────────────────────────────────────────────

    fn start_sampling(&self) -> Sampler {
        Sampler::spawn(self.db_path.clone(), self.started_at)
    }

    /// The `ScanProgressReporter`'s 500 ms tick, reduced to the only part of it
    /// that touches this writer: a `ComputePartialAggregates` every tenth pass, the
    /// same 5 s cadence `PARTIAL_AGG_TICK_INTERVAL` sets.
    fn start_partial_aggregation_pump(&self) -> Pump {
        Pump::spawn(self.writer.clone())
    }

    fn start_browsing(&self) -> Browser {
        Browser::spawn(self.db_path.clone(), self.space.clone(), self.writer.clone())
    }

    fn finish(self, sampler: Sampler) -> Report {
        // The clock stops BEFORE the sampler is joined: the join waits out up to a
        // whole sampling interval, which would land in the arm's headline number.
        let elapsed = self.started_at.elapsed();
        let samples = sampler.stop();
        let conn = self.read_conn();
        let total_entries = count(&conn, "SELECT COUNT(*) FROM entries");
        // What each arm actually left behind, so "the same work" is shown rather
        // than asserted: a `dir_stats` row per directory, a non-zero
        // `min_subtree_epoch` on the ones that are genuinely covered, and the
        // volume's recursive size at the root.
        let dirs = count(&conn, "SELECT COUNT(*) FROM entries WHERE is_directory = 1");
        let dir_stats = count(&conn, "SELECT COUNT(*) FROM dir_stats");
        let covered_dirs = count(&conn, "SELECT COUNT(*) FROM dir_stats WHERE min_subtree_epoch > 0");
        let root_bytes = count(
            &conn,
            "SELECT COALESCE(recursive_physical_size, 0) FROM dir_stats WHERE entry_id = 1",
        );
        let denied = count(&conn, "SELECT COUNT(*) FROM entries WHERE unreadable_cause = 1");
        let declined = count(&conn, "SELECT COUNT(*) FROM entries WHERE unreadable_cause = 2");
        // The sampler's cheap probe against the product's own coverage query, so
        // the note can say the two agree rather than hope they do.
        let disagreements = coverage_targets()
            .into_iter()
            .filter(|target| {
                let cheap = subtree_fully_listed(&conn, self.resolve(&conn, target));
                let real = self.frontier_under(target).is_empty();
                cheap != real
            })
            .count();
        drop(conn);

        let roots = std::mem::take(&mut *self.roots.lock_ignore_poison());
        self.writer.shutdown();
        let nanos = |counter: &AtomicU64| Duration::from_nanos(counter.load(Ordering::Relaxed));
        Report {
            label: self.label,
            elapsed,
            total_entries,
            walked_entries: 0,
            denied,
            declined,
            dirs,
            dir_stats,
            covered_dirs,
            root_bytes,
            not_virgin: self.not_virgin.load(Ordering::Relaxed),
            marked_abandoned: self.marked_abandoned.load(Ordering::Relaxed),
            roots,
            samples,
            disagreements,
            stitched_dirs: self.stitched_dirs.load(Ordering::Relaxed),
            stitch_time: nanos(&self.stitch_nanos),
            frontier_query_time: nanos(&self.frontier_query_nanos),
            drain_time: nanos(&self.drain_nanos),
        }
    }
}

/// One count off an open connection.
fn count(conn: &Connection, sql: &str) -> u64 {
    conn.query_row(sql, [], |row| row.get(0)).unwrap_or(0)
}

/// Whether every directory in `root_id`'s subtree has been listed.
///
/// The same predicate the coverage descent answers (`frontier.is_empty()`) at a
/// fraction of the cost: one recursive query instead of a round-trip per
/// directory. `Bench::finish` cross-checks the two.
fn subtree_fully_listed(conn: &Connection, root_id: Option<i64>) -> bool {
    unlisted_dirs_under(conn, root_id).is_some_and(|count| count == 0)
}

/// How many directories under `root_id` nothing has listed yet.
///
/// A count rather than a boolean because "the frontier is empty" never becomes
/// true on this machine: a handful of File Provider domains stall, the walker
/// gives up on them, and a subtree it gave up on is left honestly unlisted with
/// no `unreadable_cause` to exclude it by. So the timestamp worth reporting is
/// when the count SETTLES, next to what it settled at.
fn unlisted_dirs_under(conn: &Connection, root_id: Option<i64>) -> Option<u64> {
    let root_id = root_id?;
    conn.query_row(
        "WITH RECURSIVE sub(id) AS ( \
           SELECT ?1 \
           UNION ALL \
           SELECT e.id FROM entries e JOIN sub s ON e.parent_id = s.id WHERE e.is_directory = 1 \
         ) \
         SELECT COUNT(*) FROM sub s JOIN entries e ON e.id = s.id \
         WHERE e.is_directory = 1 AND e.listed_epoch = 0 AND e.unreadable_cause = 0",
        [root_id],
        |row| row.get(0),
    )
    .ok()
}

// ── The sampler ──────────────────────────────────────────────────────

/// What the sampler collected: per target, when its unlisted-directory count
/// stopped falling and what it stopped at, plus the memory high-water marks.
struct Samples {
    settled: Vec<Settled>,
    peak_resident: u64,
    peak_footprint: u64,
    /// What the sampler's own queries cost, so the note can say how much of an
    /// arm's wall clock the instrument is responsible for.
    probe_time: Duration,
}

/// One coverage target's answer.
struct Settled {
    path: PathBuf,
    /// When the unlisted count last changed. `None` if it never moved.
    at: Option<Duration>,
    /// What it settled at. `0` is genuinely fully covered; anything else is
    /// ground no walk in this arm could read.
    left_unlisted: u64,
}

/// A thread asking, every [`SAMPLE_INTERVAL`], what the index covers and what the
/// process holds.
struct Sampler {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<Samples>,
}

impl Sampler {
    fn spawn(db_path: PathBuf, started_at: Instant) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("phase-bench-sampler".into())
            .spawn(move || {
                let conn = IndexStore::open_read_connection(&db_path).expect("sampler connection");
                // The coverage probe is a recursive query over a subtree that grows
                // to millions of rows, and running it once a second costs the arm
                // real wall clock. So the numbers come from two passes: one with it
                // on for the timestamps, one with it off for the clock and the
                // memory. Memory sampling costs nothing and stays on in both.
                let targets = if probing() { coverage_targets() } else { Vec::new() };
                let mut last: Vec<Option<u64>> = vec![None; targets.len()];
                let mut settled: Vec<Settled> = targets
                    .iter()
                    .map(|path| Settled {
                        path: path.clone(),
                        at: None,
                        left_unlisted: 0,
                    })
                    .collect();
                let mut peak_resident = 0;
                let mut peak_footprint = 0;
                let mut probe_time = Duration::ZERO;
                loop {
                    if let Some(basic) = cmdr_fs::process_memory::query_basic_info() {
                        peak_resident = peak_resident.max(basic.resident_size_max);
                    }
                    if let Some(footprint) = cmdr_fs::process_memory::current_phys_footprint() {
                        peak_footprint = peak_footprint.max(footprint);
                    }
                    let elapsed = started_at.elapsed();
                    let probe_started = Instant::now();
                    for (index, target) in targets.iter().enumerate() {
                        let id = resolve_for_sampler(&conn, target);
                        let Some(count) = unlisted_dirs_under(&conn, id) else {
                            continue;
                        };
                        if last[index] != Some(count) {
                            last[index] = Some(count);
                            settled[index].at = Some(elapsed);
                            settled[index].left_unlisted = count;
                        }
                    }
                    probe_time += probe_started.elapsed();
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    // allowed-test-sleep: the sampling interval IS the instrument;
                    // it sets the granularity every coverage timestamp is reported at.
                    std::thread::sleep(SAMPLE_INTERVAL);
                }
                Samples {
                    settled,
                    peak_resident,
                    peak_footprint,
                    probe_time,
                }
            })
            .expect("spawn the sampler");
        Self { stop, thread }
    }

    fn stop(self) -> Samples {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().expect("sampler thread")
    }
}

/// Whether this run probes coverage. Off (`CMDR_PHASE_BENCH_NO_PROBE=1`) gives an
/// arm's wall clock without the instrument in it.
fn probing() -> bool {
    std::env::var("CMDR_PHASE_BENCH_NO_PROBE").is_err()
}

/// The sampler's own path resolve, without a `Bench` to borrow.
fn resolve_for_sampler(conn: &Connection, path: &Path) -> Option<i64> {
    if path == Path::new("/") {
        return Some(ROOT_ID);
    }
    resolve_path(conn, &path.to_string_lossy()).ok().flatten()
}

// ── The browsing arm's second and third walkers ──────────────────────

/// The mid-scan partial-aggregation pump, so a baseline arm carries the same
/// writer load a real scan does.
struct Pump {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<u64>,
}

impl Pump {
    fn spawn(writer: IndexWriter) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("phase-bench-partial-agg".into())
            .spawn(move || {
                let mut tick: u64 = 0;
                let mut passes = 0;
                while !thread_stop.load(Ordering::Relaxed) {
                    // allowed-test-sleep: this IS the reporter's 500 ms tick, the
                    // cadence being reproduced.
                    std::thread::sleep(Duration::from_millis(500));
                    tick += 1;
                    if !tick.is_multiple_of(10) {
                        continue;
                    }
                    let hot_paths = browse_path_ring()
                        .into_iter()
                        .take(2)
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    if writer
                        .send(WriteMessage::ComputePartialAggregates {
                            hot_paths,
                            source: AggSource::Maps,
                        })
                        .is_ok()
                    {
                        passes += 1;
                    }
                }
                passes
            })
            .expect("spawn the partial-aggregation pump");
        Self { stop, thread }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let passes = self.thread.join().unwrap_or(0);
        let mut out = std::io::stderr();
        let _ = writeln!(out, "  partial-aggregation passes fired: {passes}");
    }
}

/// A pane reading listings ahead of the walker, plus one search-shaped cover walk
/// on disjoint ground: the two things the phase machine doesn't control.
struct Browser {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

impl Browser {
    fn spawn(db_path: PathBuf, space: IndexPathSpace, writer: IndexWriter) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("phase-bench-browser".into())
            .spawn(move || {
                let conn = IndexStore::open_read_connection(&db_path).expect("browser connection");
                let started = Instant::now();
                let mut searched = false;
                for path in browse_path_ring().into_iter().cycle() {
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    open_listing(&conn, &path);
                    if !searched && started.elapsed() >= SEARCH_WALK_AT {
                        searched = true;
                        search_shaped_walk(&space, &writer);
                    }
                    // allowed-test-sleep: the dwell IS the scenario, a person
                    // looking at a folder before moving to the next one.
                    std::thread::sleep(BROWSE_DWELL);
                }
            })
            .expect("spawn the browser");
        Self { stop, thread }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().expect("browser thread");
    }
}

/// What showing a folder costs the disk and the index: the real `readdir` a pane
/// does, plus the enrichment read of that folder's rows and their sizes.
fn open_listing(conn: &Connection, path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let _ = entry.metadata();
        }
    }
    let Some(id) = resolve_for_sampler(conn, path) else {
        return;
    };
    let Ok(mut statement) = conn.prepare(
        "SELECT e.name, e.is_directory, d.recursive_physical_size \
         FROM entries e LEFT JOIN dir_stats d ON d.entry_id = e.id WHERE e.parent_id = ?1",
    ) else {
        return;
    };
    if let Ok(rows) = statement.query_map([id], |row| row.get::<_, String>(0)) {
        for row in rows {
            let _ = row;
        }
    }
}

/// A second walker the phase machine doesn't own, over ground it hasn't reached.
/// A live search calls `Index::cover` on the user's behalf, deliberately carved out
/// of both indexing switches, and only overlapping ground defers.
fn search_shaped_walk(space: &IndexPathSpace, writer: &IndexWriter) {
    let root = Path::new(SEARCH_WALK_ROOT);
    let heartbeat = WalkHeartbeat::new();
    let cancel = CancellationToken::new();
    let writer = writer.clone();
    let space = space.clone();
    std::thread::Builder::new()
        .name("phase-bench-search".into())
        .spawn(move || {
            let result = cover_subtree(root, &space, &writer, None, &cancel, &heartbeat);
            let _ = writer.flush_blocking();
            let mut out = std::io::stderr();
            let _ = match result {
                Ok(summary) => writeln!(
                    out,
                    "  search-shaped walk of {SEARCH_WALK_ROOT}: {} entries",
                    summary.total_entries
                ),
                Err(e) => writeln!(out, "  search-shaped walk of {SEARCH_WALK_ROOT} didn't run: {e:?}"),
            };
        })
        .expect("spawn the search-shaped walk");
}

// ── Reporting ────────────────────────────────────────────────────────

/// One arm's numbers, printed for the note to quote.
struct Report {
    label: &'static str,
    elapsed: Duration,
    total_entries: u64,
    walked_entries: u64,
    denied: u64,
    declined: u64,
    dirs: u64,
    dir_stats: u64,
    covered_dirs: u64,
    root_bytes: u64,
    not_virgin: u64,
    marked_abandoned: u64,
    roots: Vec<RootWalk>,
    samples: Samples,
    disagreements: usize,
    stitched_dirs: u64,
    stitch_time: Duration,
    frontier_query_time: Duration,
    drain_time: Duration,
}

impl Report {
    fn print(&self) {
        let mut out = std::io::stderr();
        let _ = writeln!(out, "\n══ {} ══", self.label);
        let _ = writeln!(out, "  full disk access: {}", if has_fda() { "yes" } else { "NO" });
        let _ = writeln!(out, "  wall clock to full coverage: {:.1?}", self.elapsed);
        let _ = writeln!(
            out,
            "  entries in the index: {}  (the walks reported {})",
            self.total_entries, self.walked_entries
        );
        let _ = writeln!(
            out,
            "  dirs the walk was refused: {} denied, {} declined",
            self.denied, self.declined
        );
        let _ = writeln!(
            out,
            "  peak resident: {:.0} MB   peak phys footprint: {:.0} MB",
            self.samples.peak_resident as f64 / 1_048_576.0,
            self.samples.peak_footprint as f64 / 1_048_576.0,
        );
        let _ = writeln!(
            out,
            "  aggregation left behind: {} dir_stats rows for {} dirs, {} of them covered, {} bytes at the root",
            self.dir_stats, self.dirs, self.covered_dirs, self.root_bytes
        );
        let _ = writeln!(out, "  frontier roots refused as non-virgin: {}", self.not_virgin);
        let _ = writeln!(
            out,
            "  cheap-probe vs coverage-query disagreements: {}",
            self.disagreements
        );
        let _ = writeln!(
            out,
            "  the sampler's own queries cost {:.1?} of that wall clock",
            self.samples.probe_time
        );

        let _ = writeln!(
            out,
            "  covered at, meaning the moment the unlisted count stopped moving (±{SAMPLE_INTERVAL:?}):"
        );
        for settled in &self.samples.settled {
            let at = match settled.at {
                Some(at) => format!("{at:.1?}"),
                None => "never moved".to_string(),
            };
            let left = match settled.left_unlisted {
                0 => String::new(),
                n => format!("  ({n} dirs no walk could read)"),
            };
            let _ = writeln!(out, "    {:>8}  {}{}", at, settled.path.display(), left);
        }

        if self.roots.is_empty() {
            return;
        }
        let walk_total: Duration = self.roots.iter().map(|root| root.walk).sum();
        let entries: u64 = self.roots.iter().map(|root| root.entries).sum();
        let _ = writeln!(
            out,
            "  where the wall clock went: {:.1?} walking {} frontier roots ({entries} entries), \
             {:.1?} stitching {} dirs, {:.1?} draining the writer, {:.1?} asking for frontiers",
            walk_total,
            self.roots.len(),
            self.stitch_time,
            self.stitched_dirs,
            self.drain_time,
            self.frontier_query_time,
        );

        // A directory an earlier walk gave up on is never marked listed, so every
        // LATER phase's frontier offers it again and pays its stall timeout again.
        // The bulk build reads it once. Separated out because it is the cost of a
        // machine's dead mounts rather than of the tree's size, and the two scale
        // with completely different things.
        let barren: Vec<&RootWalk> = self.roots.iter().filter(|root| root.entries == 0).collect();
        let barren_time: Duration = barren.iter().map(|root| root.walk).sum();
        let _ = writeln!(
            out,
            "  of that walking, {:.1?} went to {} frontier roots that yielded NOTHING \
             (ground an earlier walk gave up on, re-offered by a later phase)",
            barren_time,
            barren.len(),
        );
        if self.marked_abandoned > 0 {
            let _ = writeln!(
                out,
                "  dirs marked as ground no walk will read: {}",
                self.marked_abandoned
            );
        }
        for root in barren.iter().take(4) {
            let _ = writeln!(out, "    yielded nothing, {:>7.1?}: {}", root.walk, root.path);
        }

        let _ = writeln!(
            out,
            "  covered at, derived from walk order rather than probed (exact, and free):"
        );
        for target in coverage_targets() {
            let prefix = format!("{}/", target.to_string_lossy());
            let own = target.to_string_lossy().into_owned();
            let last = self
                .roots
                .iter()
                .filter(|root| root.path == own || root.path.starts_with(&prefix))
                .map(|root| root.finished_at)
                .max();
            match last {
                Some(at) => {
                    let _ = writeln!(out, "    {:>8.1?}  {}", at, target.display());
                }
                None => {
                    let _ = writeln!(out, "    {:>8}  {}", "no walk", target.display());
                }
            }
        }

        let mut roots = self.roots.clone();
        roots.sort_by_key(|root| std::cmp::Reverse(root.walk));
        let _ = writeln!(out, "  the wait a newly-queued root would see, worst first:");
        for root in roots.iter().take(12) {
            let _ = writeln!(
                out,
                "    {:>7.1?}  {:>9} entries  {}",
                root.walk, root.entries, root.path
            );
        }
    }
}

/// Whether this process can read a TCC-protected directory. A run without Full
/// Disk Access reads less of the drive and finishes faster, so every number here
/// has to be read next to this answer.
fn has_fda() -> bool {
    std::fs::read_dir(home_dir().join("Library/Application Support/com.apple.TCC")).is_ok()
}

// ── Guarding the harness itself ──────────────────────────────────────

/// The arms above take twenty minutes over a real drive, so the properties they
/// depend on are pinned here instead, over a tree that takes milliseconds. Every
/// one of them is a way the benchmark could report a fast, wrong number.
#[cfg(test)]
mod tests {
    use super::*;

    /// A small tree with one big-ish child, one small one, and files at every
    /// level, in a mount-rooted space so the boot-disk exclusions leave it alone.
    fn fixture() -> (tempfile::TempDir, IndexPathSpace) {
        let dir = tempfile::tempdir().expect("temp tree");
        let root = dir.path();
        std::fs::write(root.join("a-file-at-the-root.txt"), b"x").expect("write");
        for branch in ["big", "small"] {
            std::fs::create_dir(root.join(branch)).expect("mkdir");
            std::fs::write(root.join(branch).join("leaf.txt"), b"x").expect("write");
            for n in 0..3 {
                let deep = root.join(branch).join(format!("deep-{n}"));
                std::fs::create_dir(&deep).expect("mkdir");
                std::fs::write(deep.join("leaf.txt"), b"x").expect("write");
            }
        }
        let space = IndexPathSpace::mount_rooted(root.to_string_lossy().into_owned());
        (dir, space)
    }

    /// The whole reason the stitch exists: after it, a phase root's CHILDREN are
    /// the frontier, each one virgin, so the parallel walker takes them. Without
    /// it the frontier is the phase root itself and the walk hits `NotVirgin`.
    #[test]
    fn the_stitch_turns_a_phase_root_into_a_list_of_virgin_frontier_roots() {
        let (dir, space) = fixture();
        let root = dir.path();
        let bench = Bench::over("stitch", space);

        assert_eq!(
            bench.frontier_under(root),
            vec![root.to_string_lossy().into_owned()],
            "before the stitch the phase root itself is the whole frontier"
        );

        bench.stitch_dir(root).expect("stitch the phase root");

        let mut frontier = bench.frontier_under(root);
        frontier.sort();
        assert_eq!(
            frontier,
            vec![
                root.join("big").to_string_lossy().into_owned(),
                root.join("small").to_string_lossy().into_owned(),
            ],
            "after the stitch the children are the frontier and the root is not"
        );
    }

    /// Rule 1 of the stitch. `listed_children_on` serves a directory's rows as its
    /// full contents the moment `listed_epoch` is non-zero, so a directories-only
    /// stitch would tell a user-visible consumer the folder holds no files.
    #[test]
    fn the_stitch_upserts_files_not_only_directories() {
        let (dir, space) = fixture();
        let bench = Bench::over("stitch files", space);
        bench.stitch_dir(dir.path()).expect("stitch");

        let conn = bench.read_conn();
        let id = bench.resolve(&conn, dir.path()).expect("the root resolves");
        let files: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE parent_id = ?1 AND is_directory = 0",
                [id],
                |row| row.get(0),
            )
            .expect("count files");
        assert_eq!(files, 1, "the file sitting next to the stitched directories is indexed");
        assert!(
            IndexStore::get_listed_epoch_by_id(&conn, id)
                .expect("read")
                .unwrap_or(0)
                > 0,
            "the stitched directory is marked listed, at the current epoch"
        );
    }

    /// The convergence property every other number rests on: one pass of stitch
    /// plus walks leaves nothing on the frontier, and no walk was refused.
    #[test]
    fn a_stitched_phase_converges_in_one_pass_at_either_depth() {
        for depth in [1, 2] {
            let (dir, space) = fixture();
            let bench = Bench::over("convergence", space);
            bench.run_phase(dir.path(), depth);

            assert_eq!(
                bench.frontier_under(dir.path()),
                Vec::<String>::new(),
                "depth {depth}: the frontier is empty after one pass"
            );
            assert_eq!(
                bench.not_virgin.load(Ordering::Relaxed),
                0,
                "depth {depth}: every frontier root the stitch produced was virgin"
            );
        }
    }

    /// Depth 2 is the interleaving knob: it has to hand the walker MORE and
    /// SMALLER roots than depth 1, or the wait it claims to cut isn't cut.
    #[test]
    fn depth_2_hands_the_walker_smaller_roots_than_depth_1() {
        let counts: Vec<usize> = [1, 2]
            .into_iter()
            .map(|depth| {
                let (dir, space) = fixture();
                let bench = Bench::over("depth", space);
                bench.run_phase(dir.path(), depth);

                bench.roots.lock().expect("roots").len()
            })
            .collect();
        assert!(
            counts[1] > counts[0],
            "depth 2 split the phase into more frontier roots than depth 1 ({counts:?})"
        );
    }

    /// Marking ground a walk could not read must not cost the arm a single ROW.
    ///
    /// The mark cuts the coverage descent, so a mark placed over ground that is
    /// merely not walked YET deletes that ground from every later phase's frontier
    /// and the arm finishes early with a short index — which reads exactly like a
    /// win. Pinned here because it already happened once, and the only evidence in
    /// the output was an entry count 21% below the bulk build's.
    #[test]
    fn marking_unreadable_ground_costs_no_coverage() {
        let counts: Vec<u64> = [false, true]
            .into_iter()
            .map(|mark_abandoned| {
                let (dir, space) = fixture();
                let bench = Bench::over("marking", space);
                bench.mark_abandoned.store(mark_abandoned, Ordering::Relaxed);
                bench.run_phase(dir.path(), 2);
                let conn = bench.read_conn();
                count(&conn, "SELECT COUNT(*) FROM entries")
            })
            .collect();
        assert_eq!(
            counts[0], counts[1],
            "marking abandoned ground indexed a different number of entries ({counts:?})"
        );
    }

    /// The sampler's cheap probe and the product's coverage query have to answer
    /// the same question, or every coverage timestamp in the note is measuring
    /// something the product doesn't believe.
    #[test]
    fn the_cheap_coverage_probe_agrees_with_the_coverage_query() {
        let (dir, space) = fixture();
        let root = dir.path();
        let bench = Bench::over("probe", space);

        let agree = |bench: &Bench| {
            let conn = bench.read_conn();
            let id = bench.resolve(&conn, root);
            subtree_fully_listed(&conn, id) == bench.frontier_under(root).is_empty()
        };
        assert!(agree(&bench), "they agree on a cold index");
        bench.stitch_dir(root);
        assert!(agree(&bench), "they agree on a stitched but unwalked phase root");
        bench.run_phase(root, 1);
        assert!(agree(&bench), "they agree once the phase has converged");
    }
}
