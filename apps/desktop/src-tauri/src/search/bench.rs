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
//!
//! # The unscoped fan-out: several volumes' arenas, one after another vs all at once.
//! CMDR_SEARCH_BENCH_DBS="/path/index-root.db,/path/index-smb-a.db,/path/index-smb-b.db" \
//!   cargo test -p cmdr --lib search::bench::bench_volume_fanout -- --ignored --nocapture
//!
//! # What a loaded arena actually costs, in heap bytes and in process RSS.
//! # (`--exact`: a sibling bench's arena would land in the same RSS reading.)
//! CMDR_SEARCH_BENCH_DB="/path/index-root.db" \
//!   cargo test -p cmdr --lib -- --ignored --nocapture --exact search::bench::bench_arena_bytes
//!
//! # The arena pass alone, best-of-N, with and without size/date filters.
//! CMDR_SEARCH_BENCH_DB="/path/index-root.db" \
//!   cargo test -p cmdr --lib search::bench::bench_arena_scan -- --ignored --nocapture
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

use crate::pluralize::{pluralize, pluralize_with};
use cmdr_index::ReadPool;
use cmdr_index::store::ROOT_ID;

use super::engine::search_ranked;
use super::index::{OptU64, SearchEntry, SearchIndex, load_search_index};
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
            size: OptU64::new(if is_dir {
                None
            } else {
                Some((i as u64 * 7919) % 50_000_000)
            }),
            modified_at: OptU64::new(Some(1_600_000_000 + (i as u64 * 613) % 60_000_000)),
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
        sort_by: None,
    }
}

/// Run one pattern through the three phase-isolating variants and print a row.
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn measure(label: &str, index: &SearchIndex, weights: &ImportanceWeights, pattern: &str, pattern_type: PatternType) {
    let empty = ImportanceWeights::empty();

    let t = Instant::now();
    let total = search_ranked(index, &query(pattern, pattern_type.clone(), true), &empty, "", None)
        .expect("count-only search should succeed")
        .total_count;
    let scan = t.elapsed();

    let t = Instant::now();
    let _ = search_ranked(index, &query(pattern, pattern_type.clone(), false), &empty, "", None)
        .expect("unweighted search should succeed");
    let unweighted = t.elapsed();

    let t = Instant::now();
    let _ =
        search_ranked(index, &query(pattern, pattern_type, false), weights, "", None).expect("search should succeed");
    let weighted = t.elapsed();

    eprintln!(
        "  {label:<28} matches {total:>9}  scan {scan:>10.2?}  +rank {:>10.2?}  +importance {:>10.2?}  = {weighted:.2?}",
        unweighted.saturating_sub(scan),
        weighted.saturating_sub(unweighted),
    );
}

// ── Benchmarks ───────────────────────────────────────────────────────

#[test]
#[ignore = "benchmark; run explicitly to collect numbers"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_synthetic() {
    let n: usize = std::env::var("CMDR_SEARCH_BENCH_ENTRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3_000_000);

    let t = Instant::now();
    let index = build_synthetic_index(n);
    eprintln!(
        "\nsynthetic index: {} entries, built in {:.2?}",
        index.entries.len(),
        t.elapsed()
    );
    let t = Instant::now();
    let weights = synthetic_weights(&index);
    eprintln!(
        "importance map: {} folders, built in {:.2?}\n",
        weights.len(),
        t.elapsed()
    );

    measure("rare literal", &index, &weights, "report-1234567", PatternType::Glob);
    measure("common word", &index, &weights, "report", PatternType::Glob);
    measure("extension glob", &index, &weights, "*.pdf", PatternType::Glob);
    measure("one letter", &index, &weights, "e", PatternType::Glob);
    eprintln!();
}

#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_DB pointing at a real index-*.db"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_real_index() {
    let Ok(db) = std::env::var("CMDR_SEARCH_BENCH_DB") else {
        eprintln!("CMDR_SEARCH_BENCH_DB not set; skipping");
        return;
    };
    let pool = ReadPool::new(db.clone().into()).expect("open index DB");

    let t = Instant::now();
    let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load index");
    let load = t.elapsed();
    eprintln!("\n{db}\n  arena load: {} entries in {load:.2?}", index.entries.len());

    let weights = match std::env::var("CMDR_SEARCH_BENCH_IMPORTANCE_DB") {
        Ok(path) => {
            let t = Instant::now();
            let w = load_weights_from(&path);
            eprintln!("  importance map: {} folders in {:.2?}", w.len(), t.elapsed());
            w
        }
        Err(_) => ImportanceWeights::empty(),
    };
    eprintln!();

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
    eprintln!();
}

/// Time the unscoped fan-out's LOAD phase: every listed volume's arena one after
/// another (what a serial `ensure_volume` loop costs) versus all at once.
///
/// Runs a warm-up pass first so both variants see the same page-cache state; the
/// warm-up's own timings print too, as the closest thing here to a cold read.
#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_DBS listing real index-*.db paths"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_volume_fanout() {
    let Ok(list) = std::env::var("CMDR_SEARCH_BENCH_DBS") else {
        eprintln!("CMDR_SEARCH_BENCH_DBS not set; skipping");
        return;
    };
    let paths: Vec<String> = list.split(',').map(|s| s.trim().to_string()).collect();

    let load_one = |path: &str| -> (usize, std::time::Duration) {
        let t = Instant::now();
        let pool = ReadPool::new(path.into()).expect("open index DB");
        let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load index");
        (index.entries.len(), t.elapsed())
    };

    eprintln!("\nfirst pass (page cache as found):");
    for path in &paths {
        let (n, took) = load_one(path);
        eprintln!(
            "  {path}\n    {} in {took:.2?}",
            pluralize_with(n as u64, "entry", "entries")
        );
    }

    let t = Instant::now();
    let mut serial_total = 0usize;
    for path in &paths {
        serial_total += load_one(path).0;
    }
    let serial = t.elapsed();

    let t = Instant::now();
    let parallel_total: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = paths.iter().map(|p| scope.spawn(|| load_one(p).0)).collect();
        handles.into_iter().map(|h| h.join().unwrap_or(0)).sum()
    });
    let parallel = t.elapsed();

    eprintln!(
        "\n{}, {}\n  serial   {serial:.2?}\n  parallel {parallel:.2?} (parallel_total {parallel_total})\n",
        pluralize(paths.len() as u64, "volume"),
        pluralize_with(serial_total as u64, "entry", "entries"),
    );
}

/// Time the ARENA PASS alone, best-of-N, with and without the filters that read an
/// entry's size and date.
///
/// The one instrument for "did a change to the row's shape make scanning slower".
/// `bench_real_index` can't answer that: its queries set no size or date bound, so the
/// matcher skips those predicates entirely, and its single run per pattern is swamped by
/// machine noise on a busy box.
///
/// Two shapes, because they cost differently:
///
/// - **name only** — every row still pays the `Candidate` build, which decodes both
///   optional fields whether or not the query asks about them.
/// - **+ size and date bounds** — the predicates actually read the decoded values, on
///   every row the name pattern let through.
///
/// Count-only, so the number is the rayon scan with no ranking or path materialization
/// in it. Reported as min and median over the repeats: the minimum is the closest thing
/// to an uncontended run, and on a machine running other work it's the only figure two
/// builds can be compared on. Comparing two builds means comparing two BINARIES —
/// `cargo test --release` overwrites the same one, so copy it aside between builds and
/// run them alternately, or the machine's mood ends up in the delta.
#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_DB pointing at a real index-*.db"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_arena_scan() {
    let Ok(db) = std::env::var("CMDR_SEARCH_BENCH_DB") else {
        eprintln!("CMDR_SEARCH_BENCH_DB not set; skipping");
        return;
    };
    let repeats: usize = std::env::var("CMDR_SEARCH_BENCH_REPEATS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9);

    let pool = ReadPool::new(db.clone().into()).expect("open index DB");
    let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load index");
    let empty = ImportanceWeights::empty();
    eprintln!(
        "\n{db}\n  {}, size_of::<SearchEntry>() = {} B, best of {repeats}",
        pluralize_with(index.entries.len() as u64, "entry", "entries"),
        size_of::<SearchEntry>(),
    );

    // A one-letter pattern, so the name filter passes millions of rows through to the
    // size and date predicates rather than rejecting them on the regex.
    let name_only = query("e", PatternType::Glob, true);
    let filtered = SearchQuery {
        min_size: Some(1_000_000),
        modified_after: Some(1_600_000_000),
        ..query("e", PatternType::Glob, true)
    };

    for (label, q) in [("name only", &name_only), ("+ size and date bounds", &filtered)] {
        let mut runs: Vec<std::time::Duration> = Vec::with_capacity(repeats);
        let mut total = 0;
        for _ in 0..repeats {
            let t = Instant::now();
            total = search_ranked(&index, q, &empty, "", None)
                .expect("count-only search should succeed")
                .total_count;
            runs.push(t.elapsed());
        }
        runs.sort_unstable();
        eprintln!(
            "  {label:<24} matches {total:>9}  min {:>9.2?}  median {:>9.2?}",
            runs[0],
            runs[repeats / 2],
        );
    }
    eprintln!();
}

/// This process's resident set in bytes, as the kernel reports it.
fn process_rss_bytes() -> u64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|kib| kib * 1024)
        .unwrap_or(0)
}

/// What one loaded arena costs, measured rather than derived from `size_of`.
///
/// Two numbers, because neither alone is the whole answer:
///
/// - **Heap bytes held** ([`heap_bytes_held`]) is exact and reproducible: it's the
///   requested `Layout` sizes the built `SearchIndex` still holds, so a rerun on the
///   same DB gives the same figure. Compare THIS across a change to the row's shape.
/// - **Process RSS with the arena resident** confirms the heap figure reaches real
///   memory. Read the ABSOLUTE number, not the Δ: a warm-up load runs and is dropped
///   first (otherwise the delta is mostly the DB's page cache), so by the measured load
///   the allocator is reusing pages it already holds and the Δ reads as ~0. The absolute
///   figure is reproducible to 0.1 MB across runs.
///
/// ⚠️ Both are measured under the test binary's `System`-backed counting allocator, not
/// the mimalloc the shipping app uses, so ❌ don't quote either as the app's footprint.
/// A DIFFERENCE between two builds carries; an absolute figure doesn't.
///
/// ❌ **Run this one ALONE** (`--exact search::bench::bench_arena_bytes`). The harness
/// runs `#[test]`s in parallel inside one process, so a sibling bench holding its own
/// arena lands in the same RSS reading. The heap figure is thread-local and unaffected.
///
/// Point it at a SNAPSHOT of a real index rather than the live file if you're comparing
/// two builds: the boot volume's index gains rows by the second, and a row count that
/// moved between runs is a difference the arena shape didn't cause. Comparing two builds
/// means comparing two BINARIES — see [`bench_arena_scan`] on copying one aside.
#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_DB pointing at a real index-*.db"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_arena_bytes() {
    use crate::test_support::heap_bytes_held;

    let Ok(db) = std::env::var("CMDR_SEARCH_BENCH_DB") else {
        eprintln!("CMDR_SEARCH_BENCH_DB not set; skipping");
        return;
    };
    let pool = ReadPool::new(db.clone().into()).expect("open index DB");

    // A throwaway load first, then drop it, so the DB's page cache and the allocator's
    // first-touch growth are already paid for and the figure below is one arena's worth
    // of resident memory rather than everything this process has ever touched.
    drop(load_search_index(&pool, &AtomicBool::new(false)).expect("load index"));

    let rss_empty = process_rss_bytes();
    let t = Instant::now();
    let (index, heap) = heap_bytes_held(|| load_search_index(&pool, &AtomicBool::new(false)).expect("load index"));
    let load = t.elapsed();
    let rss_loaded = process_rss_bytes();

    let rows = index.entries.len() as f64;
    let mb = |bytes: i64| bytes as f64 / (1024.0 * 1024.0);
    let entries_bytes = (index.entries.capacity() * size_of::<SearchEntry>()) as i64;
    eprintln!(
        "\n{db}\n  {} loaded in {load:.2?}\n  \
         size_of::<SearchEntry>() = {} B\n  \
         arena heap held:      {:>9.1} MB  ({:.1} B a row)\n    \
         of which entries:     {:>9.1} MB\n    \
         names + id map:       {:>9.1} MB\n  \
         process RSS, arena resident: {:.1} MB (empty {:.1} MB)\n",
        pluralize_with(index.entries.len() as u64, "entry", "entries"),
        size_of::<SearchEntry>(),
        mb(heap),
        heap as f64 / rows,
        mb(entries_bytes),
        mb(heap - entries_bytes),
        rss_loaded as f64 / (1024.0 * 1024.0),
        rss_empty as f64 / (1024.0 * 1024.0),
    );

    // Keep the arena alive past the RSS read above.
    assert!(!index.entries.is_empty(), "the index should hold rows");
}

/// Load an importance weight map straight from an `importance-*.db` file (the
/// bench reads a path, while production reads a data dir + volume id).
fn load_weights_from(path: &str) -> ImportanceWeights {
    let mut weights = ImportanceWeights::empty();
    let Ok(conn) = crate::sqlite_util::open_read_only(std::path::Path::new(path)) else {
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

/// Time the two ways root's importance weight map is kept current: the FULL reload
/// (stream every row out of `importance-root.db` and rehash every path) against the
/// in-place patch an incremental pass's delta allows.
///
/// This is the every-60s cost the incremental rescore pays, so the ratio is what
/// decides whether the throttle window can come down. The delta is synthetic — a
/// handful of upserts plus one removal, the shape a real listing change produces —
/// and the patch is measured BOTH ways: with the `Arc` unshared (nobody searching,
/// the overwhelmingly common case, where `Arc::make_mut` mutates in place) and with a
/// reader holding it (a search in flight, where it clones the map first).
#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_IMPORTANCE_DB pointing at a real importance-*.db"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_weight_reload() {
    use std::sync::Arc;

    let Ok(db) = std::env::var("CMDR_SEARCH_BENCH_IMPORTANCE_DB") else {
        eprintln!("set CMDR_SEARCH_BENCH_IMPORTANCE_DB to an importance-*.db path");
        return;
    };

    // The full reload, three times: it's what every recompute notification cost.
    let mut reload_runs = Vec::new();
    let mut weights = ImportanceWeights::empty();
    for _ in 0..3 {
        let t = Instant::now();
        weights = load_weights_from(&db);
        reload_runs.push(t.elapsed());
    }
    eprintln!(
        "\n{db}\n  {} scored folders\n  full reload: {:.2?} / {:.2?} / {:.2?}",
        weights.len(),
        reload_runs[0],
        reload_runs[1],
        reload_runs[2],
    );

    // A typical incremental delta: one changed folder plus a few descendants, and one
    // folder that left (renamed away, deleted, or newly floored).
    let upserted: Vec<(String, f64)> = (0..8)
        .map(|i| (format!("/Users/bench/proj/sub{i}"), 0.4 + i as f64 / 100.0))
        .collect();
    let removed = vec!["/Users/bench/proj/gone".to_string()];

    let patch = |shared: bool| {
        let mut map = Arc::new(weights.clone());
        // A search in flight holds a second handle, which makes `make_mut` clone.
        let reader = shared.then(|| Arc::clone(&map));
        let t = Instant::now();
        let patched = Arc::make_mut(&mut map);
        for path in &removed {
            patched.remove(path);
        }
        for (path, score) in &upserted {
            patched.insert(path, *score);
        }
        let elapsed = t.elapsed();
        drop(reader);
        elapsed
    };

    // Warm up (the first clone touches pages the timed runs then find resident).
    patch(true);
    eprintln!(
        "  delta patch ({} upserts, {} removal): {:.2?} unshared, {:.2?} while a search holds the map\n",
        upserted.len(),
        removed.len(),
        patch(false),
        patch(true),
    );
}
