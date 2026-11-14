//! What the "Tell me about this" card's two digest searches cost on a real arena:
//! the largest files under a folder (size-sorted, eight rows) and its untouched
//! files (modified before a year ago, size-sorted, 1,000 rows), the queries
//! `about/digest.rs::files_under` asks, through the same seam a desktop search
//! runs (`IndexSource` + `scope_pairs` + `search_covered`).
//!
//! Run with a release build against a real index:
//! `CMDR_SEARCH_BENCH_DB=…/drive-index/index-root.db CMDR_SEARCH_BENCH_SCOPES=/Users/me,/Users/me/projects
//! cargo nextest run --release -p cmdr --lib --run-ignored only bench_card_digest_searches --no-capture`.
//! Findings: `apps/desktop/src-tauri/src/about/DETAILS.md` § A folder's digest and an app's data.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use cmdr_index::ReadPool;

use crate::pluralize::pluralize;

use super::context::SearchContext;
use super::index::load_search_index;
use super::index_source::{IndexSource, PathNamespace, scope_pairs};
use super::ranking::ImportanceWeights;
use super::seam::search_covered;
use super::types::{PatternType, SearchQuery, SearchSort};
use super::volumes::LoadedVolume;

fn card_query(scope: &str, modified_before: Option<u64>, limit: u32) -> SearchQuery {
    SearchQuery {
        name_pattern: None,
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before,
        is_directory: Some(false),
        include_paths: Some(vec![scope.to_string()]),
        exclude_dir_names: None,
        include_path_ids: None,
        limit,
        case_sensitive: None,
        sort_by: Some(SearchSort::Size),
        exclude_system_dirs: Some(false),
        count_only: false,
    }
}

#[test]
#[ignore = "benchmark; needs CMDR_SEARCH_BENCH_DB pointing at a real index-*.db"]
#[allow(
    clippy::print_stderr,
    reason = "an ignored measurement harness prints its table to stderr for `--nocapture`; it never runs in the app or CI"
)]
fn bench_card_digest_searches() {
    let Ok(db) = std::env::var("CMDR_SEARCH_BENCH_DB") else {
        eprintln!("CMDR_SEARCH_BENCH_DB not set; skipping");
        return;
    };
    let scopes = std::env::var("CMDR_SEARCH_BENCH_SCOPES").unwrap_or_else(|_| "/".to_string());
    let repeats = 7;
    let pool = ReadPool::new(db.clone().into()).expect("open index DB");
    let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load index");
    let rows = index.len();
    let loaded = LoadedVolume::for_test(index, pool, None);
    let source = IndexSource(&loaded);
    let empty = ImportanceWeights::empty();
    let ctx = SearchContext::for_test(&empty);
    let a_year_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
        .saturating_sub(365 * 86_400);
    eprintln!(
        "\n{db}\n  {}, best and median of {repeats}",
        pluralize(rows as u64, "row")
    );
    for scope in scopes.split(',') {
        let pairs = scope_pairs(&[scope.to_string()], None, PathNamespace::Os);
        for (label, query) in [
            ("largest 8", card_query(scope, None, 8)),
            ("untouched 1,000", card_query(scope, Some(a_year_ago), 1_000)),
        ] {
            let mut runs: Vec<Duration> = Vec::with_capacity(repeats);
            let mut total = 0;
            for _ in 0..repeats {
                let started = Instant::now();
                total = search_covered(&source, &source, &query, &pairs, &ctx)
                    .expect("the card's search succeeds")
                    .total;
                runs.push(started.elapsed());
            }
            runs.sort_unstable();
            eprintln!(
                "  {scope:<40} {label:<16} {:>15}  min {:>9.2?}  median {:>9.2?}",
                pluralize(total as u64, "file"),
                runs[0],
                runs[repeats / 2]
            );
        }
    }
    eprintln!();
}
