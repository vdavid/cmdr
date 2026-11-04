use super::*;
use crate::test_alloc::allocations_on_pool;
use crate::testing::{build_synthetic_index, synthetic_weights};
use crate::types::PatternType;

// ── The scan doesn't allocate per entry ─────────────────────────
//
// A real arena is 6–7 M rows, so one allocation per row is millions per query, and
// under the system allocator it made even a query that matches nothing 30–45% slower
// (`docs/notes/performance/search-loop-allocations-2026-09-27.md`). These pin the
// count per QUERY: the difference between a big arena and a tiny one is what the rows
// cost, and it has to stay far below the row count.

/// Rows in the big arena. Big enough that per-row allocation dwarfs the budget.
const ROWS: usize = 200_000;

/// Allocations the big arena may cost over the tiny one: one regex cache per scan
/// chunk (seven here, ~30 allocations each), rayon's bookkeeping, and the result
/// `Vec`s, plus the ranking memos and 30 result rows for a full search. ~225 with no
/// match, ~360 with 11,250 matches, and ~625 ranking them against weights when this
/// was written; per-ancestor folding was 316,000 and a path `Vec` per folder 26,000.
///
/// ⚠️ The regex half of the fix (a clone per chunk, `engine.rs`) is NOT reliably
/// pinned here: in a debug build the regex pool's lock is rarely contended, so the
/// shared-`Regex` shape measured 231–1,388. Its evidence is the release bench
/// (`bench::bench_query_allocations`), where it was ~380,000.
const BUDGET: u64 = 1_000;

/// More threads than the regex crate's eight cache stacks, so two workers share one,
/// the way they do on any 10+-core Mac.
const THREADS: usize = 16;

fn count_only(pattern: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(pattern.to_string()),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: true,
        limit: 30,
        // Case-insensitive, the macOS default, on every platform: that's the path that
        // folds names.
        case_sensitive: Some(false),
        // The default: the system/cache tier is on, so every match walks its ancestors.
        exclude_system_dirs: None,
        sort_by: None,
    }
}

/// Allocations one query costs on `index`, and its match count.
fn measure(index: &SearchIndex, query: &SearchQuery, weights: &ImportanceWeights) -> (u64, u32) {
    let (result, allocations) = allocations_on_pool(THREADS, || {
        search(index, query, weights).expect("search should succeed")
    });
    (allocations, result.total_count)
}

/// The per-row cost of `query`: its allocations on a big arena minus those on a tiny
/// one. `with_weights` ranks the big one against an importance weight on every folder.
fn per_arena_cost(query: &SearchQuery, with_weights: bool) -> (u64, u32) {
    let big = build_synthetic_index(ROWS);
    let weights = if with_weights {
        synthetic_weights(&big)
    } else {
        ImportanceWeights::empty()
    };
    let tiny = make_test_index();
    let (tiny_allocations, _) = measure(&tiny, query, &ImportanceWeights::empty());
    let (big_allocations, matches) = measure(&big, query, &weights);
    assert!(
        tiny_allocations > 0,
        "the counter measured nothing; compiling a regex alone allocates, so the allocator isn't counting"
    );
    (big_allocations.saturating_sub(tiny_allocations), matches)
}

#[test]
fn a_query_that_matches_nothing_does_not_allocate_per_row() {
    let (cost, matches) = per_arena_cost(&count_only("no-such-name-anywhere"), false);
    assert_eq!(matches, 0);
    assert!(cost < BUDGET, "{cost} allocations over {ROWS} rows (budget {BUDGET})"); // allowed-pluralize-noun: a measurement message whose counts are thousands, never one
}

#[test]
fn matches_under_the_default_excludes_do_not_allocate_per_ancestor() {
    // One word in 20: ~10,000 matches, each walking ~8 ancestors past the excludes.
    let (cost, matches) = per_arena_cost(&count_only("report"), false);
    assert!(matches > 5_000, "the fixture should match plenty, got {matches}");
    assert!(
        cost < BUDGET,
        "{cost} allocations for {matches} matches (budget {BUDGET})" // allowed-pluralize-noun: a measurement message whose counts are thousands, never one
    );
}

#[test]
fn ranking_against_importance_weights_does_not_allocate_per_folder() {
    // The full search, ranked the way production ranks it: every match looks up its
    // folder's weight, which means hashing the folder's path off the parent chain.
    // The synthetic tree spreads the ~10,000 matches over thousands of folders.
    let query = SearchQuery {
        count_only: false,
        ..count_only("report")
    };
    let (cost, matches) = per_arena_cost(&query, true);
    assert!(matches > 5_000, "the fixture should match plenty, got {matches}");
    assert!(
        cost < BUDGET,
        "{cost} allocations ranking {matches} matches (budget {BUDGET})" // allowed-pluralize-noun: a measurement message whose counts are thousands, never one
    );
}
