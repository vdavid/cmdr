//! A self-test of the search core (`cmdr-search`) on the real `.wasm`, exported to
//! the smoke test (`smoke.mjs`): what native tests can't see. The scan's `par_chunks`
//! has to fall back to the calling thread (rayon on a thread-less target), and the
//! engine's timing log has to read the page's clock, never `std`'s, which panics
//! here. So it runs a glob, a scoped query, and a folder-size filter over a small
//! in-memory tree, with `debug` logging on so the timing lines format too.
//!
//! The demo's ⌘F host replaces this once it lands; until then this keeps the core
//! linked and executed in the browser build.

use std::collections::HashMap;

use cmdr_search::{
    DirSizeSource, MemoryRow, MemorySource, NameRules, PatternType, SearchContext, SearchQuery, SearchRows,
    SourceError, ranking::ImportanceWeights, search_covered,
};
use wasm_bindgen::prelude::*;

/// Runs the queries and answers a one-line summary, or throws naming the first one
/// that answered wrong.
#[wasm_bindgen(js_name = searchSelfTest)]
pub fn search_self_test() -> Result<String, JsValue> {
    let level = log::max_level();
    log::set_max_level(log::LevelFilter::Debug);
    let outcome = run();
    log::set_max_level(level);
    outcome.map_err(|e| JsValue::from_str(&e))
}

/// Folder sizes by id, the shape a host's own fold answers in.
struct Sizes(HashMap<i64, u64>);

impl DirSizeSource for Sizes {
    fn in_range(&self, min: Option<u64>, max: Option<u64>) -> Result<HashMap<i64, u64>, SourceError> {
        Ok(self
            .0
            .iter()
            .filter(|&(_, &size)| min.is_none_or(|min| size >= min) && max.is_none_or(|max| size <= max))
            .map(|(&id, &size)| (id, size))
            .collect())
    }

    fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError> {
        Ok(ids.iter().map(|id| self.0.get(id).copied()).collect())
    }
}

fn row(path: &str, is_directory: bool, size: Option<u64>) -> MemoryRow {
    MemoryRow {
        path: path.to_string(),
        is_directory,
        size,
        modified_at: Some(1_700_000_000),
    }
}

fn query(pattern: Option<&str>) -> SearchQuery {
    SearchQuery {
        name_pattern: pattern.map(str::to_string),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: None,
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

fn run() -> Result<String, String> {
    let rows = vec![
        row("/Documents", true, None),
        row("/Documents/Report.pdf", false, Some(50_000)),
        row("/Documents/Taxes", true, None),
        row("/Documents/Taxes/report-2025.pdf", false, Some(2_000_000)),
        row("/Pictures", true, None),
        row("/Pictures/beach.jpg", false, Some(3_000_000)),
    ];
    let source = MemorySource::new(rows, "/Users/demo", NameRules::MACOS).map_err(|e| e.to_string())?;
    let id = |path: &str| source.id_of(path).ok_or_else(|| format!("{path} isn't in the tree"));
    let sizes = Sizes(HashMap::from([
        (id("/Documents")?, 2_050_000),
        (id("/Documents/Taxes")?, 2_000_000),
        (id("/Pictures")?, 3_000_000),
    ]));
    let weights = ImportanceWeights::empty();
    let ctx = SearchContext {
        rules: NameRules::MACOS,
        home_dir: Some("/Users/demo"),
        weights: &weights,
    };
    let paths_of = |q: &SearchQuery, scope: &[&str]| -> Result<Vec<String>, String> {
        let pairs: Vec<(String, String)> = scope.iter().map(|p| (p.to_string(), p.to_string())).collect();
        let half = search_covered(&source, &sizes, q, &pairs, &ctx).map_err(|e| e.to_string())?;
        let mut paths: Vec<String> = half.entries.into_iter().map(|e| e.path).collect();
        paths.sort_unstable();
        Ok(paths)
    };

    let everywhere = paths_of(&query(Some("report")), &[])?;
    let want = [
        "/Users/demo/Documents/Report.pdf",
        "/Users/demo/Documents/Taxes/report-2025.pdf",
    ];
    if everywhere != want {
        return Err(format!("a glob found {everywhere:?}, want {want:?}"));
    }
    let scoped = paths_of(&query(Some("report")), &["/Users/demo/documents/taxes"])?;
    if scoped != ["/Users/demo/Documents/Taxes/report-2025.pdf"] {
        return Err(format!("a scoped glob found {scoped:?}"));
    }
    let big_folders = SearchQuery {
        is_directory: Some(true),
        min_size: Some(2_500_000),
        ..query(None)
    };
    let big = paths_of(&big_folders, &[])?;
    if big != ["/Users/demo/Pictures"] {
        return Err(format!("a folder-size filter found {big:?}"));
    }
    Ok(format!(
        "search OK ({} rows: a glob, a scope, and a folder size)",
        source.arena().len()
    ))
}
