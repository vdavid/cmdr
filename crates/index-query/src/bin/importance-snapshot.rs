//! Snapshot a real drive index into an anonymized eval scenario + a labels
//! template (the corpus tool, plan open-question 1 tuning loop).
//!
//! Reads a volume's `index-{volume_id}.db` READ-ONLY, derives each folder's signals
//! through the SAME production assembly the scheduler uses, ANONYMIZES every folder
//! name (keeping only classification-relevant names — see the privacy contract in
//! `importance/evals/corpus.rs`), and writes two files into the gitignored corpus
//! dir:
//!
//! - `<name>.scenario.json` — the anonymized folders + their signals.
//! - `<name>.labels.json` — a template where David marks his genuinely-important
//!   folders (the ground truth for personalized soft constraints).
//!
//! Nothing personal is written: the scenario holds counts, flags, bucketed
//! timestamps, and placeholder names, never a real folder name or file content.
//!
//! IMPORTANT: point this at the REAL index DB (`index-root.db`,
//! `index-smb-….db`), NOT `importance.db`. Copy the DB to a temp path first if the
//! app might be writing it (this tool opens read-only, but a copy avoids reading a
//! half-written page). Usage:
//!
//!   cargo run -p index-query --bin importance-snapshot -- \
//!     <index.db> <home-or-mount-root> <local|listing-only> <scenario-name> <out-dir>
//!
//! Example (David's local root):
//!   cargo run -p index-query --bin importance-snapshot -- \
//!     /tmp/index-root-copy.db "$HOME" local root apps/desktop/src-tauri/tests/importance-corpus

use std::path::PathBuf;

use cmdr_lib::importance::evals::corpus::{labels_template_for, snapshot_index_to_scenario};
use cmdr_lib::importance::evals::scenario::Availability;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 6 {
        eprintln!(
            "Usage: {} <index.db> <home-or-mount-root> <local|listing-only> <scenario-name> <out-dir>",
            args[0]
        );
        std::process::exit(1);
    }
    let index_db = PathBuf::from(&args[1]);
    let home = &args[2];
    let availability = match args[3].as_str() {
        "local" => Availability::Local,
        "listing-only" => Availability::ListingOnly,
        other => {
            eprintln!("availability must be 'local' or 'listing-only', got '{other}'");
            std::process::exit(1);
        }
    };
    let name = &args[4];
    let out_dir = PathBuf::from(&args[5]);

    if !index_db.exists() {
        eprintln!("index DB not found: {}", index_db.display());
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("couldn't create out dir {}: {e}", out_dir.display());
        std::process::exit(1);
    }

    // "Now" for the recency signals: the current wall clock, matching what a live
    // recompute would use.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let scenario = match snapshot_index_to_scenario(&index_db, home, availability, name, now) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("snapshot failed: {e}");
            std::process::exit(1);
        }
    };

    let scenario_path = out_dir.join(format!("{name}.scenario.json"));
    let labels_path = out_dir.join(format!("{name}.labels.json"));

    let scenario_json = match scenario.to_json() {
        Ok(j) => j,
        Err(e) => {
            eprintln!("couldn't serialize scenario: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = std::fs::write(&scenario_path, scenario_json) {
        eprintln!("couldn't write {}: {e}", scenario_path.display());
        std::process::exit(1);
    }

    // Don't clobber an existing labels file — David may have already filled it in.
    if labels_path.exists() {
        println!("Kept existing labels file: {}", labels_path.display());
    } else {
        let template = labels_template_for(name);
        match template.to_json() {
            Ok(j) => {
                if let Err(e) = std::fs::write(&labels_path, j) {
                    eprintln!("couldn't write {}: {e}", labels_path.display());
                    std::process::exit(1);
                }
                println!("Wrote labels template: {}", labels_path.display());
            }
            Err(e) => {
                eprintln!("couldn't serialize labels template: {e}");
                std::process::exit(1);
            }
        }
    }

    println!(
        "Snapshotted {} folders into {} (anonymized: personal names stripped).",
        scenario.folders.len(),
        scenario_path.display()
    );
    println!("Next: open the labels file and mark your important folders, then run the eval suite.");
}
