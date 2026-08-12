//! Difference the SCOPED incremental walk against the full O(dirs) walk over a REAL
//! index, and time both.
//!
//! The correctness harness for `docs/specs/scoped-incremental-walk.md`: for a sample
//! of real directories, the rows a scoped walk would write for a subtree have to be
//! exactly the rows the full walk would write for that same subtree — same paths,
//! same scores, same signal blobs. Both sides take the SAME fixed `now_secs`, since
//! the recency signal moves scores with the wall clock.
//!
//! Opens the index READ-ONLY (WAL gives a consistent snapshot) and writes nothing.
//! Reports counts and timings only — never a folder name.
//!
//! ```text
//! cargo run --release -p index-query --bin importance-diff -- \
//!   <index.db> <home-or-mount-root> [origin-count | comma-separated origin paths]
//! ```

use std::path::PathBuf;

use cmdr_index::importance::tooling::{compare_walks_for_incremental, sample_origins};

/// A fixed clock for both sides, so a score difference can only be a signal
/// difference.
const NOW_SECS: u64 = 1_700_000_000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        eprintln!("Usage: {} <index.db> <home-or-mount-root> [origin-count]", args[0]);
        std::process::exit(1);
    }
    let index_db = PathBuf::from(&args[1]);
    let home = &args[2];
    // A number samples that many origins across the index; a comma-separated list
    // compares exactly those (for a deliberately deep or wide subtree).
    let explicit: Option<Vec<String>> = match args.get(3) {
        Some(a) if a.parse::<usize>().is_err() => Some(a.split(',').map(|p| p.trim().to_string()).collect()),
        _ => None,
    };
    let wanted: usize = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(200);

    if !index_db.exists() {
        eprintln!("index DB not found: {}", index_db.display());
        std::process::exit(1);
    }

    let origins = match explicit {
        Some(paths) => paths,
        None => match sample_origins(&index_db, home, wanted) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("sampling origins failed: {e}");
                std::process::exit(1);
            }
        },
    };
    println!("comparing {} origins", origins.len());

    let full_started = std::time::Instant::now();
    let comparisons = match compare_walks_for_incremental(&index_db, home, &origins, NOW_SECS) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("differential failed: {e}");
            std::process::exit(1);
        }
    };
    let total = full_started.elapsed();

    let compared: Vec<_> = comparisons.iter().filter(|c| !c.fell_back).collect();
    let fell_back = comparisons.len() - compared.len();
    let disagreed = compared.iter().filter(|c| !c.agrees()).count();

    println!(
        "compared {} origins, {fell_back} fell back to the full walk",
        compared.len()
    );
    println!(
        "  rows: {} scoped against {} oracle",
        compared.iter().map(|c| c.scoped_rows).sum::<usize>(),
        compared.iter().map(|c| c.oracle_rows).sum::<usize>()
    );
    println!(
        "  disagreements: {disagreed} origins ({} missing/extra rows, {} score, {} signal)",
        compared.iter().map(|c| c.missing_or_extra).sum::<usize>(),
        compared.iter().map(|c| c.score_mismatches).sum::<usize>(),
        compared.iter().map(|c| c.signal_mismatches).sum::<usize>()
    );
    // The over-budget origins: each is rescored ALONE, and each is one the pass would
    // have paid a full walk for before the bound existed.
    println!(
        "  demoted (subtree past the budget): {} origins",
        comparisons.iter().filter(|c| c.demoted).count()
    );

    let mut walks: Vec<_> = compared.iter().map(|c| c.scoped_walk).collect();
    walks.sort_unstable();
    if let (Some(min), Some(max)) = (walks.first(), walks.last()) {
        let sum: std::time::Duration = walks.iter().sum();
        println!(
            "  scoped walk per origin: median {:.2?}, min {min:.2?}, max {max:.2?}, mean {:.2?}",
            walks[walks.len() / 2],
            sum / walks.len() as u32
        );
    }
    let mut folders: Vec<_> = compared.iter().map(|c| c.scoped_folders).collect();
    folders.sort_unstable();
    if !folders.is_empty() {
        println!(
            "  folders read per origin: median {}, max {}",
            folders[folders.len() / 2],
            folders[folders.len() - 1]
        );
    }
    let mut probes: Vec<_> = comparisons
        .iter()
        .filter(|c| c.fell_back)
        .map(|c| c.scoped_walk)
        .collect();
    probes.sort_unstable();
    if let (Some(min), Some(max)) = (probes.first(), probes.last()) {
        println!(
            "  fallback probe before giving up: median {:.2?}, min {min:.2?}, max {max:.2?}",
            probes[probes.len() / 2]
        );
    }
    println!("whole differential (one full walk plus every scoped walk): {total:.2?}");

    if disagreed > 0 {
        eprintln!("THE WALKS DISAGREE — the scoped walk is not a drop-in for the full one");
        std::process::exit(2);
    }
}
