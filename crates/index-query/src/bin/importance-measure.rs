//! Measure `importance.db` size + row count for a real index, exercising the SAME
//! walk + score + write path a live recompute uses (so the floored-skip and
//! trimmed-JSON storage shape are measured faithfully).
//!
//! Points at a REAL index DB READ-ONLY (WAL gives a consistent snapshot), writes a
//! fresh scratch `importance.db`, and prints the row count + file size. Reports
//! sizes only — never a folder name.
//!
//!   cargo run -p index-query --bin importance-measure -- \
//!     <index.db> <home-or-mount-root> <local|listing-only> <out-importance.db>

use std::path::PathBuf;

use cmdr_lib::importance::SignalSet;
use cmdr_lib::importance::scheduler::recompute_index_to_db;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!(
            "Usage: {} <index.db> <home-or-mount-root> <local|listing-only> <out-importance.db>",
            args[0]
        );
        std::process::exit(1);
    }
    let index_db = PathBuf::from(&args[1]);
    let home = &args[2];
    let available = match args[3].as_str() {
        "local" => SignalSet::all(),
        "listing-only" => SignalSet::listing_only(),
        other => {
            eprintln!("availability must be 'local' or 'listing-only', got '{other}'");
            std::process::exit(1);
        }
    };
    let out_db = PathBuf::from(&args[4]);

    if !index_db.exists() {
        eprintln!("index DB not found: {}", index_db.display());
        std::process::exit(1);
    }
    // Start from a clean scratch store so the size reflects only this pass.
    for p in [&out_db, &out_db.with_extension("db-wal"), &out_db.with_extension("db-shm")] {
        let _ = std::fs::remove_file(p);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let outcome = match recompute_index_to_db(&index_db, &out_db, home, available, now) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("recompute failed: {e}");
            std::process::exit(1);
        }
    };

    // Checkpoint the WAL into the main file so the size is the whole store.
    if let Ok(conn) = rusqlite::Connection::open(&out_db) {
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    let main_bytes = std::fs::metadata(&out_db).map(|m| m.len()).unwrap_or(0);
    let wal_bytes = std::fs::metadata(out_db.with_extension("db-wal"))
        .map(|m| m.len())
        .unwrap_or(0);

    let total = outcome.walk_and_score + outcome.write_and_flush;
    println!(
        "rows written (floored omitted): {} of {} folders walked",
        outcome.rows_written, outcome.folders_walked
    );
    println!("importance.db size: {main_bytes} bytes ({:.1} MB)", main_bytes as f64 / 1e6);
    println!("  wal remaining: {wal_bytes} bytes");
    println!(
        "full-pass wall clock: {:.2?} (walk+score {:.2?}, write+flush {:.2?})",
        total, outcome.walk_and_score, outcome.write_and_flush
    );
}
