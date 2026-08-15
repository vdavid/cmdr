//! What resuming an interrupted first index costs, against the same ground
//! covered in one go.
//!
//! `#[ignore]`d: it builds a six-figure directory tree and drives the REAL machine
//! over it twice, which takes minutes and prints numbers rather than asserting.
//! The machine is what's being measured, so this goes through the `Drive` fixture
//! next door — the handle, the activation, the registry, the phase driver, the
//! progress reporter, the event sink — and not through a hand-rolled model of it.
//!
//! Two arms over identical trees, in one process so they share a machine and a
//! page-cache state:
//!
//! - **Uninterrupted**: start the machine, let it finish. The reference.
//! - **Interrupted**: start it, stop it partway (a quit), start it again, and time
//!   only the second run. Its per-root numbers are the ones the fix is about.
//!
//! ```sh
//! CMDR_PHASES_TEST_TREE_DIR=/private/tmp \
//!   cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
//!   indexing::lifecycle::phases::tests::resume_bench::resume_cost
//! ```
//!
//! Results and the call they back: `docs/notes/phased-vs-bulk-index-2026-08-14.md`
//! § "Resuming an interrupted run".

use std::io::Write;
use std::time::{Duration, Instant};

use super::*;
use crate::indexing::events::IndexEvent;

/// How many directories the synthetic tree holds. Big enough that the walk takes
/// tens of seconds (so an interruption has somewhere to land) and small enough
/// that building it takes a few.
fn dir_budget() -> usize {
    std::env::var("CMDR_RESUME_BENCH_DIRS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(150_000)
}

/// How much of the tree is indexed when the quit lands, as a share of what the
/// uninterrupted arm ended with.
///
/// Late, because that is the expensive end: the more of the tree is already
/// listed, the deeper and tinier the frontier a quit leaves behind. Measured in
/// ROWS rather than seconds so the same amount of work is left to every run of the
/// bench — a wall-clock quit moves with the machine's mood, and the arm it is meant
/// to compare against is the one whose speed just changed.
const QUIT_AT: f64 = 0.6;

/// How long the machine may take before the bench gives up on it.
const PATIENCE: Duration = Duration::from_secs(600);

#[test]
#[ignore = "benchmark over a six-figure synthetic tree; run manually with --nocapture"]
fn resume_cost() {
    let mut out = std::io::stderr();
    let dirs = dir_budget();

    let uninterrupted = uninterrupted_arm(dirs);
    uninterrupted.print("uninterrupted: the machine runs to the end");

    let resumed = interrupted_arm(dirs, (uninterrupted.entries as f64 * QUIT_AT) as u64);
    resumed.print(&format!(
        "resumed: the same tree, quit at {:.0}% of its rows and started again",
        QUIT_AT * 100.0
    ));

    let _ = writeln!(
        &mut out,
        "\n  the comparison: {:.1?} to cover {} entries uninterrupted ({:.0} entries/s), \
         against {:.1?} for the {} entries the resume had left ({:.0} entries/s) \
         over {} frontier roots in {} cover calls",
        uninterrupted.elapsed,
        uninterrupted.entries,
        uninterrupted.entries as f64 / uninterrupted.elapsed.as_secs_f64(),
        resumed.elapsed,
        resumed.entries_added,
        resumed.entries_added as f64 / resumed.elapsed.as_secs_f64(),
        resumed.roots,
        resumed.cover_calls,
    );
}

// ── The arms ─────────────────────────────────────────────────────────

/// The machine over a cold index, start to finish.
fn uninterrupted_arm(dirs: usize) -> Arm {
    let drive = Drive::new("phased-resume-bench-whole", |root| build_tree(root, dirs), &[]);
    let entries_before = 0;
    let started = Instant::now();
    drive.start();
    wait_for_the_machine(&drive);
    let elapsed = started.elapsed();
    Arm::taken(&drive, elapsed, entries_before, None)
}

/// The machine over the same tree, stopped once `quit_at` rows are indexed and
/// started again. Only the second run is timed.
fn interrupted_arm(dirs: usize, quit_at: u64) -> Arm {
    let drive = Drive::new("phased-resume-bench-resumed", |root| build_tree(root, dirs), &[]);
    drive.start();
    cmdr_fs::testing::wait_until(PATIENCE, "the index to fill up to the quit", || {
        drive.entry_count() >= quit_at
    });
    drive.stop();

    // What the quit left behind, before anything walks again: how many frontier
    // roots the next run has to take, and what one coverage query over the volume
    // costs against a mostly-covered index — the machine asks that question after
    // every single root.
    //
    // ⚠️ Straight off the database, ❌ never through `Index::coverage`: the volume
    // is stopped here, so it has no read pool, and the handle answers a volume it
    // holds no index for with "the whole scope is frontier". That reads as one
    // frontier root and a query that costs nothing.
    let frontier_at_resume = frontier_off_the_database(&drive).len();
    let coverage_query = {
        let started = Instant::now();
        let _ = frontier_off_the_database(&drive);
        started.elapsed()
    };
    let entries_before = drive.entry_count();

    let started = Instant::now();
    drive.start();
    wait_for_the_machine(&drive);
    let elapsed = started.elapsed();
    Arm::taken(
        &drive,
        elapsed,
        entries_before,
        Some(Interruption {
            frontier_at_resume,
            coverage_query,
        }),
    )
}

/// The volume's frontier, read off its database file rather than through the
/// handle, so a stopped volume answers honestly.
fn frontier_off_the_database(drive: &Drive) -> Vec<String> {
    let conn = IndexStore::open_read_connection(&drive.db_path()).expect("read connection");
    let root = drive.path("");
    coverage_for_scope(&conn, "/", &root, CoverageDimension::Listing)
        .expect("coverage")
        .frontier
}

/// Wait for the machine to report it has nothing left to do, with a benchmark's
/// patience rather than a unit test's.
fn wait_for_the_machine(drive: &Drive) {
    cmdr_fs::testing::wait_until(PATIENCE, "the phases to finish", || {
        !drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning)
    });
}

// ── What an arm reports ──────────────────────────────────────────────

/// One arm's numbers.
struct Arm {
    elapsed: Duration,
    entries: u64,
    entries_added: u64,
    /// How many times the machine started a walk. One per `cover()` round trip,
    /// counted off the branch events the walk brackets itself with.
    cover_calls: usize,
    /// How many frontier roots those calls carried between them.
    roots: usize,
    /// What the index still says it hasn't covered when the machine stops. Zero on
    /// a healthy arm; anything else means the run ended without covering the tree.
    frontier_at_end: usize,
    /// Directories the index holds a "nothing is coming for this" mark on. A quit
    /// must not manufacture these: ground the walk never reached is uncovered, ❌
    /// not unreadable.
    unreadable_at_end: usize,
    interruption: Option<Interruption>,
}

/// What the quit left for the run that followed it.
struct Interruption {
    frontier_at_resume: usize,
    coverage_query: Duration,
}

impl Arm {
    fn taken(drive: &Drive, elapsed: Duration, entries_before: u64, interruption: Option<Interruption>) -> Self {
        // Only the events this run emitted: the interrupted arm's recorder also
        // holds the first run's, and counting those would halve every rate here.
        let branches: Vec<Vec<String>> = drive
            .events
            .events()
            .into_iter()
            .filter_map(|event| match event {
                IndexEvent::CoverageBranchStarted { volume_id, roots } if volume_id == drive.volume_id => Some(roots),
                _ => None,
            })
            .collect();
        let entries = drive.entry_count();
        let coverage = drive
            .index
            .coverage(drive.volume_id, &drive.path(""), CoverageDimension::Listing)
            .expect("the volume answers for its own coverage");
        Self {
            elapsed,
            entries,
            entries_added: entries.saturating_sub(entries_before),
            cover_calls: branches.len(),
            roots: branches.iter().map(Vec::len).sum(),
            frontier_at_end: coverage.frontier.len(),
            unreadable_at_end: coverage.abandoned.len() + coverage.permission_denied.len() + coverage.declined.len(),
            interruption,
        }
    }

    fn print(&self, label: &str) {
        let mut out = std::io::stderr();
        let seconds = self.elapsed.as_secs_f64().max(f64::MIN_POSITIVE);
        let _ = writeln!(&mut out, "\n══ {label} ══");
        if let Some(interruption) = &self.interruption {
            let _ = writeln!(
                &mut out,
                "  the quit left {} frontier roots, and one coverage query over the volume costs {:.1?}",
                interruption.frontier_at_resume, interruption.coverage_query,
            );
        }
        let _ = writeln!(
            &mut out,
            "  {:.1?} for {} entries added ({} in the index at the end)",
            self.elapsed, self.entries_added, self.entries,
        );
        let _ = writeln!(
            &mut out,
            "  it ends with {} frontier roots left and {} dirs marked unreadable",
            self.frontier_at_end, self.unreadable_at_end,
        );
        let _ = writeln!(
            &mut out,
            "  {} cover calls over {} frontier roots: {:.1} roots/s, {:.1} ms per root, {:.1} roots per call",
            self.cover_calls,
            self.roots,
            self.roots as f64 / seconds,
            1000.0 * seconds / self.roots.max(1) as f64,
            self.roots as f64 / self.cover_calls.max(1) as f64,
        );
    }
}

// ── The tree ─────────────────────────────────────────────────────────

/// Build a tree shaped like somebody's disk rather than a lattice: breadth-first,
/// with a branching factor and a file count that vary per directory, so an
/// interruption leaves frontier roots of genuinely different sizes the way a real
/// one does.
///
/// Deterministic (a fixed-seed LCG), so both arms walk the same shape and two runs
/// of the bench are comparable.
///
/// Shared with `churn_bench` next door, which measures a different question over
/// the same shape.
pub(super) fn build_tree(root: &Path, dir_budget: usize) {
    let started = Instant::now();
    let mut random = Lcg::seeded(0x5EED);
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut dirs = 0usize;
    let mut files = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        if dirs >= dir_budget {
            break;
        }
        for index in 0..random.upto(4) {
            let path = dir.join(format!("file-{index}.txt"));
            if std::fs::write(&path, b"x").is_ok() {
                files += 1;
            }
        }
        if depth >= 14 {
            continue;
        }
        // The budget runs out mid-level, so the deepest directories the queue
        // reaches stay childless — which is exactly the shape a late quit leaves
        // behind, and the shape this bench is about.
        for index in 0..random.upto(4) {
            let path = dir.join(format!("dir-{index}"));
            if std::fs::create_dir(&path).is_ok() {
                dirs += 1;
                queue.push_back((path, depth + 1));
            }
        }
    }
    let mut out = std::io::stderr();
    let _ = writeln!(
        &mut out,
        "  built a tree of {} and {} in {:.1?}",
        cmdr_fs::pluralize::pluralize(dirs as u64, "dir"),
        cmdr_fs::pluralize::pluralize(files as u64, "file"),
        started.elapsed()
    );
}

/// A tiny linear congruential generator, so the tree's shape is reproducible
/// without a dependency.
struct Lcg(u64);

impl Lcg {
    fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    /// A number in `0..=max`, weighted towards the wide end so a breadth-first
    /// tree keeps growing to its budget instead of dying out at depth three.
    fn upto(&mut self, max: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let roll = (self.0 >> 33) % (max + 1);
        // Two rolls, taking the larger: same range, weighted towards the wide end,
        // which is what keeps a breadth-first tree growing to its budget instead of
        // dying out at depth three.
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        roll.max((self.0 >> 33) % (max + 1))
    }
}
