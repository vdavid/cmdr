//! Dev-only importance tuning surface (plan Decision 6, §18.3).
//!
//! Reads a volume's `importance.db` through the same [`ImportanceIndex`] read API
//! consumers use, re-scores every stored folder's raw signals under a chosen
//! [`Weights`], and prints the ranked folders WITH their per-signal `explain`
//! breakdowns — so David can eyeball the ranking against his real home directory
//! and tune the weights. No write path: it reads stored signals and re-scores;
//! nothing is persisted.
//!
//! Usage:
//!   cargo run -p index-query --bin importance-tune -- <importance.db> [top_n]
//!
//! Find the DB under the app data dir as `importance-root.db` (beside
//! `index-root.db`). `top_n` defaults to 30.

use cmdr_lib::importance::{ImportanceIndex, SignalSet};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} <importance.db> [top_n]", args[0]);
        std::process::exit(1);
    }
    let db_path = std::path::PathBuf::from(&args[1]);
    let top_n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);

    // Local macOS availability (both optional signals producible). The tuning tool
    // runs against a local home, so this matches how the recompute wrote the rows.
    let index = ImportanceIndex::open_at(db_path, SignalSet::all());

    let generation = index.recompute_generation().unwrap_or(0);
    println!("importance.db as-of generation {generation}\n");

    let ranked = match index.top_n(top_n) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Couldn't read importance.db: {e}");
            std::process::exit(1);
        }
    };
    if ranked.is_empty() {
        println!("No scored folders (has the volume been indexed and recomputed yet?).");
        return;
    }

    // "Now" for the recency signals: the current wall clock, so the printed
    // breakdown matches what a live consumer would compute.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    for w in &ranked {
        println!("{:>6.3}  {}", w.score.value(), w.path);
        if let Ok(Some(explanation)) = index.explain(&w.path, now) {
            if explanation.floored {
                println!("        (floored: denylisted or hidden/system)");
            }
            for c in &explanation.contributions {
                // Only print signals that carry weight (skip redistributed-to-zero).
                if c.weight > 0.0 {
                    println!(
                        "        {:<20} weight {:>5.3}  raw {:>5.3}  = {:>6.3}",
                        format!("{:?}", c.signal),
                        c.weight,
                        c.raw,
                        c.contribution,
                    );
                }
            }
        }
        println!();
    }
}
