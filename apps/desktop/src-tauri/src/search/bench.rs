//! Measurement harness for the search hot path. `#[ignore]`d: these print numbers,
//! they don't assert, and the real-index one needs a multi-GB DB on disk.
//!
//! ```bash
//! # Synthetic index (deterministic, runs anywhere):
//! cargo test -p cmdr --lib search::bench::bench_synthetic -- --ignored --nocapture
//!
//! # A real index DB (field-accurate; point it at a copy or the live file — the read is read-only):
//! CMDR_SEARCH_BENCH_DB="$HOME/Library/Application Support/com.veszelovszki.cmdr/index-root.db" \
//! CMDR_SEARCH_BENCH_IMPORTANCE_DB="$HOME/Library/Application Support/com.veszelovszki.cmdr/importance-root.db" \
//!   cargo test -p cmdr --lib search::bench::bench_real_index -- --ignored --nocapture
//! ```
//!
//! Phases are isolated by differencing whole `search_ranked` runs rather than by
//! instrumenting the engine (which would put timing code on the production path):
//!
//! - `count_only` run ⇒ the SCAN alone (rayon filter, no ranking, no materialization).
//! - full run with an EMPTY weight map ⇒ scan + rank + materialize, no importance.
//! - full run with a REAL weight map ⇒ the above plus the per-candidate parent-path
//!   reconstruction the importance blend needs.
//!
//! Findings from these runs live in `docs/notes/search-latency-2026-07-28.md`.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use crate::indexing::ReadPool;
use crate::indexing::store::ROOT_ID;

use super::engine::search_ranked;
use super::index::{SearchEntry, SearchIndex, load_search_index};
use super::ranking::ImportanceWeights;
use super::types::{PatternType, SearchQuery};

// ── Fixtures ─────────────────────────────────────────────────────────

/// Word pool for synthetic filenames. 20 words ⇒ a plain-word query matches
/// roughly 1/20 of the files, the "a common word, lots of hits" shape that makes
/// the ranking phase visible.
const WORDS: [&str; 20] = [
    "report",
    "invoice",
    "photo",
    "screenshot",
    "notes",
    "budget",
    "resume",
    "draft",
    "backup",
    "export",
    "meeting",
    "recipe",
    "receipt",
    "sketch",
    "logo",
    "banner",
    "config",
    "readme",
    "license",
    "changelog",
];

const EXTS: [&str; 8] = ["pdf", "jpg", "png", "txt", "md", "rs", "json", "zip"];

/// Build a synthetic index of roughly `n` entries shaped like a real home dir:
/// a fanned-out tree ~8 levels deep, ~12% directories, mixed sizes and mtimes.
fn build_synthetic_index(n: usize) -> SearchIndex {
    const FANOUT: usize = 6;
    const MAX_DEPTH: usize = 8;

    let mut names = String::with_capacity(n * 20);
    let mut entries: Vec<SearchEntry> = Vec::with_capacity(n + 1);

    let push = |names: &mut String, entries: &mut Vec<SearchEntry>, id, parent_id, name: &str, is_dir, i: usize| {
        let name_offset = names.len() as u32;
        let name_len = name.len() as u16;
        names.push_str(name);
        entries.push(SearchEntry {
            id,
            parent_id,
            name_offset,
            name_len,
            is_directory: is_dir,
            size: if is_dir {
                None
            } else {
                Some((i as u64 * 7919) % 50_000_000)
            },
            modified_at: Some(1_600_000_000 + (i as u64 * 613) % 60_000_000),
        });
    };

    push(&mut names, &mut entries, ROOT_ID, 0, "", true, 0);

    // Directories first, breadth-first, until we've spent ~12% of the budget.
    let dir_budget = n / 8;
    let mut next_id: i64 = ROOT_ID + 1;
    let mut level: Vec<i64> = vec![ROOT_ID];
    let mut all_dirs: Vec<i64> = Vec::with_capacity(dir_budget);
    for depth in 0..MAX_DEPTH {
        let mut next_level = Vec::with_capacity(level.len() * FANOUT);
        for &parent in &level {
            for k in 0..FANOUT {
                if all_dirs.len() >= dir_budget {
                    break;
                }
                let name = format!("{}-{}-{}", WORDS[(next_id as usize + k) % WORDS.len()], depth, next_id);
                push(&mut names, &mut entries, next_id, parent, &name, true, next_id as usize);
                all_dirs.push(next_id);
                next_level.push(next_id);
                next_id += 1;
            }
        }
        if all_dirs.len() >= dir_budget || next_level.is_empty() {
            break;
        }
        level = next_level;
    }

    // Files spread evenly across the directories.
    let mut i = 0usize;
    while entries.len() < n {
        let parent = all_dirs[i % all_dirs.len()];
        let name = format!(
            "{}-{}.{}",
            WORDS[i % WORDS.len()],
            i,
            EXTS[(i / WORDS.len()) % EXTS.len()]
        );
        push(&mut names, &mut entries, next_id, parent, &name, false, i);
        next_id += 1;
        i += 1;
    }

    let mut id_to_index = HashMap::with_capacity(entries.len());
    for (idx, e) in entries.iter().enumerate() {
        id_to_index.insert(e.id, idx);
    }
    SearchIndex {
        names,
        entries,
        id_to_index,
        generation: 1,
    }
}

/// An importance map covering every directory in `index`, like a real
/// `importance-root.db` (a weight on most folders the user has touched).
fn synthetic_weights(index: &SearchIndex) -> ImportanceWeights {
    let mut weights = ImportanceWeights::empty();
    for (i, e) in index.entries.iter().enumerate() {
        if e.is_directory && e.id != ROOT_ID {
            let path = super::engine::reconstruct_path_from_index(index, e.id);
            weights.insert(&path, ((i % 100) as f64) / 100.0);
        }
    }
    weights
}

fn query(pattern: &str, pattern_type: PatternType, count_only: bool) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(pattern.to_string()),
        pattern_type,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only,
        limit: 30,
        case_sensitive: None,
        // Excludes off: the synthetic tree has no `node_modules`, and this keeps the
        // scan cost comparable between runs.
        exclude_system_dirs: Some(false),
    }
}

/// Run one pattern through the three phase-isolating variants and print a row.
fn measure(label: &str, index: &SearchIndex, weights: &ImportanceWeights, pattern: &str, pattern_type: PatternType) {
    let empty = ImportanceWeights::empty();

    let t = Instant::now();
    let (_, total) = search_ranked(index, &query(pattern, pattern_type.clone(), true), &empty, "")
        .expect("count-only search should succeed");
    let scan = t.elapsed();

    let t = Instant::now();
    let _ = search_ranked(index, &query(pattern, pattern_type.clone(), false), &empty, "")
        .expect("unweighted search should succeed");
    let unweighted = t.elapsed();

    let t = Instant::now();
    let _ = search_ranked(index, &query(pattern, pattern_type, false), weights, "").expect("search should succeed");
    let weighted = t.elapsed();

    println!(
        "  {label:<28} matches {total:>9}  scan {scan:>10.2?}  +rank {:>10.2?}  +importance {:>10.2?}  = {weighted:.2?}",
        unweighted.saturating_sub(scan),
        weighted.saturating_sub(unweighted),
    );
}

// ── Benchmarks ───────────────────────────────────────────────────────

#[test]
#[ignore = "benchmark; run explicitly to collect numbers"]
fn bench_synthetic() {
    let n: usize = std::env::var("CMDR_SEARCH_BENCH_ENTRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000_000);

    let t = Instant::now();
    let index = build_synthetic_index(n);
    println!(
        "\nsynthetic index: {} entries, built in {:.2?}",
        index.entries.len(),
        t.elapsed()
    );
    let t = Instant::now();
    let weights = synthetic_weights(&index);
    println!(
        "importance map: {} folders, built in {:.2?}\n",
        weights.len(),
        t.elapsed()
    );

    measure("rare literal", &index, &weights, "report-1234567", PatternType::Glob);
    measure("common word", &index, &weights, "report", PatternType::Glob);
    measure("extension glob", &index, &weights, "*.pdf", PatternType::Glob);
    measure("one letter", &index, &weights, "e", PatternType::Glob);
    println!();
}

#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_DB pointing at a real index-*.db"]
fn bench_real_index() {
    let Ok(db) = std::env::var("CMDR_SEARCH_BENCH_DB") else {
        println!("CMDR_SEARCH_BENCH_DB not set; skipping");
        return;
    };
    let pool = ReadPool::new(db.clone().into()).expect("open index DB");

    let t = Instant::now();
    let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load index");
    let load = t.elapsed();
    println!("\n{db}\n  arena load: {} entries in {load:.2?}", index.entries.len());

    let weights = match std::env::var("CMDR_SEARCH_BENCH_IMPORTANCE_DB") {
        Ok(path) => {
            let t = Instant::now();
            let w = load_weights_from(&path);
            println!("  importance map: {} folders in {:.2?}", w.len(), t.elapsed());
            w
        }
        Err(_) => ImportanceWeights::empty(),
    };
    println!();

    let pattern = std::env::var("CMDR_SEARCH_BENCH_PATTERN").unwrap_or_else(|_| "report".to_string());
    measure(
        "rare literal",
        &index,
        &weights,
        "xyzzy-no-such-file",
        PatternType::Glob,
    );
    measure(
        &format!("pattern {pattern:?}"),
        &index,
        &weights,
        &pattern,
        PatternType::Glob,
    );
    measure("extension glob", &index, &weights, "*.pdf", PatternType::Glob);
    measure("one letter", &index, &weights, "e", PatternType::Glob);
    println!();
}

/// Load an importance weight map straight from an `importance-*.db` file (the
/// bench reads a path, while production reads a data dir + volume id).
fn load_weights_from(path: &str) -> ImportanceWeights {
    let mut weights = ImportanceWeights::empty();
    let Ok(conn) = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return weights;
    };
    let Ok(mut stmt) = conn.prepare("SELECT path, score FROM weights WHERE score > 0") else {
        return weights;
    };
    let Ok(mut rows) = stmt.query([]) else {
        return weights;
    };
    while let Ok(Some(row)) = rows.next() {
        let (Ok(path), Ok(score)) = (row.get::<_, String>(0), row.get::<_, f64>(1)) else {
            continue;
        };
        weights.insert(&path, score);
    }
    weights
}
