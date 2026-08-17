//! What a claim costs at frontier scale.
//!
//! `#[ignore]`d: it prints numbers over synthetic path sets rather than
//! asserting. Nothing here touches a disk or a writer — the claim table is pure
//! in-memory bookkeeping, so the measurement is the bookkeeping and nothing else.
//!
//! Two arms, matching the two ways a claim gets asked a big question:
//!
//! - **Taking a free frontier**: one `Claim::take` over a whole frontier on a
//!   volume nobody holds, plus the release its drop does. What a phased group
//!   start and a cold-drive search both pay, on the thread that asked for the
//!   walk, before any disk is read.
//! - **Taking it again under a live one**: the same frontier against a volume
//!   already holding all of it, so every root defers. The Decision 11 case (a
//!   refined query re-asking while the first walk runs), and the arm that scans
//!   the fullest table.
//!
//! ```sh
//! cargo test -p cmdr-index --release --lib -- --ignored --nocapture claim_cost
//! ```
//!
//! Results and the call they back: `docs/notes/claim-table-cost-2026-08-17.md`.

use std::io::Write;
use std::time::{Duration, Instant};

use super::live::{Claim, Mode};

/// Frontier widths to measure at. The real number that prompted this was 2,503
/// roots on one cold-drive search; the rest bracket it so the growth curve is
/// visible rather than inferred from one point. Same ladder as the branch set's
/// bench, so the two are readable side by side.
const WIDTHS: &[usize] = &[100, 500, 1_000, 2_500, 5_000];

#[test]
#[ignore = "benchmark over synthetic path sets; run manually with --nocapture"]
fn claim_cost() {
    let mut out = std::io::stderr();

    let _ = writeln!(&mut out, "\n── taking a free frontier ──");
    let _ = writeln!(
        &mut out,
        "{:>7}  {:>12}  {:>12}  {:>12}  {:>10}",
        "roots", "take", "release", "total", "µs/root"
    );
    for &width in WIDTHS {
        let (take, release) = free_frontier_cost(width);
        let total = take + release;
        let _ = writeln!(
            &mut out,
            "{width:>7}  {take:>12.2?}  {release:>12.2?}  {total:>12.2?}  {:>10.1}",
            total.as_secs_f64() * 1e6 / width as f64,
        );
    }

    let _ = writeln!(&mut out, "\n── taking it again under a live walk ──");
    let _ = writeln!(&mut out, "{:>7}  {:>12}  {:>10}", "roots", "take", "µs/root");
    for &width in WIDTHS {
        let take = contended_cost(width);
        let _ = writeln!(
            &mut out,
            "{width:>7}  {take:>12.2?}  {:>10.1}",
            take.as_secs_f64() * 1e6 / width as f64,
        );
    }
}

/// One `take` over a frontier nobody holds, and the release its drop does. The
/// take grows as it goes: every root is checked against the ones already taken.
fn free_frontier_cost(width: usize) -> (Duration, Duration) {
    let volume_id = format!("claim-bench-free-{width}");
    let paths = frontier(width);

    let start = Instant::now();
    let claim = Claim::take(&volume_id, paths, Mode::Additive);
    let take = start.elapsed();
    assert_eq!(claim.mine().len(), width, "a free volume grants the whole frontier");

    let start = Instant::now();
    drop(claim);
    let release = start.elapsed();

    (take, release)
}

/// A second `take` over ground a live walk holds in full, so every root defers
/// against a table already at its widest.
fn contended_cost(width: usize) -> Duration {
    let volume_id = format!("claim-bench-contended-{width}");
    let paths = frontier(width);
    let held = Claim::take(&volume_id, paths.clone(), Mode::Additive);
    assert_eq!(held.mine().len(), width, "the first walk takes it all");

    let start = Instant::now();
    let second = Claim::take(&volume_id, paths, Mode::Additive);
    let take = start.elapsed();

    assert!(second.mine().is_empty(), "and the second takes none of it");
    assert_eq!(second.deferred().len(), width);
    take
}

/// `count` disjoint frontier roots, deep and sharing long prefixes: the shape a
/// resumed phased index leaves behind, where what's left to walk is a scatter of
/// small directories far down one user's tree. Long shared prefixes are the
/// honest case for a table that compares paths component by component. Same
/// generator as the branch set's bench, so the two numbers are comparable.
fn frontier(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            format!(
                "/Users/someone/Library/Application Support/project-{}/node_modules/package-{}/dist/chunk-{i}",
                i % 32,
                i % 64,
            )
        })
        .collect()
}
