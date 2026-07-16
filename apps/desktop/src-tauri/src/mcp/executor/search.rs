//! Search tool handlers (search, ai_search).

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::search::PatternType;
use crate::search::{self, SearchQuery, SearchResult, format_size, format_timestamp, summarize_query};

/// Parse a human-readable size string into bytes.
/// Supports B, KB, MB, GB, TB (case-insensitive, with or without space).
pub fn parse_human_size(s: &str) -> Result<u64, ToolError> {
    let s = s.trim();
    // Find where the numeric part ends and the unit begins
    let s_upper = s.to_uppercase();
    let (num_str, unit) = if let Some(pos) = s_upper.find("TB") {
        (&s[..pos], "TB")
    } else if let Some(pos) = s_upper.find("GB") {
        (&s[..pos], "GB")
    } else if let Some(pos) = s_upper.find("MB") {
        (&s[..pos], "MB")
    } else if let Some(pos) = s_upper.find("KB") {
        (&s[..pos], "KB")
    } else if let Some(pos) = s_upper.find('B') {
        (&s[..pos], "B")
    } else {
        // Try parsing as pure number (bytes)
        let n: u64 = s.trim().parse().map_err(|_| {
            ToolError::invalid_params(format!(
                "Couldn't parse size: \"{s}\". Use a format like \"1 MB\" or \"500 KB\"."
            ))
        })?;
        return Ok(n);
    };

    let num: f64 = num_str.trim().parse().map_err(|_| {
        ToolError::invalid_params(format!(
            "Couldn't parse size: \"{s}\". Use a format like \"1 MB\" or \"500 KB\"."
        ))
    })?;

    let multiplier: u64 = match unit {
        "B" => 1,
        "KB" => 1_024,
        "MB" => 1_024 * 1_024,
        "GB" => 1_024 * 1_024 * 1_024,
        "TB" => 1_024 * 1_024 * 1_024 * 1_024,
        _ => unreachable!(),
    };

    Ok((num * multiplier as f64) as u64)
}

/// Format search results as a human-readable table.
pub fn format_search_results(result: &SearchResult, limit: u32) -> String {
    if result.entries.is_empty() {
        return "No files found matching the query.".to_string();
    }

    let shown = result.entries.len().min(limit as usize);
    let entries = &result.entries[..shown];

    // Compute column widths
    let max_name = entries
        .iter()
        .map(|e| {
            let display_name = if e.is_directory {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            };
            display_name.len()
        })
        .max()
        .unwrap_or(0)
        .max(4);

    let max_parent = entries.iter().map(|e| e.parent_path.len()).max().unwrap_or(0).max(4);

    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(format!("{} of {} results:", shown, result.total_count));

    for entry in entries {
        let display_name = if entry.is_directory {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };

        let size_str = match entry.size {
            Some(s) => format_size(s),
            None => String::new(),
        };

        let date_str = match entry.modified_at {
            Some(ts) => format_timestamp(ts),
            None => String::new(),
        };

        lines.push(format!(
            "  {:<name_w$}  {:<parent_w$}  {:>8}  {}",
            display_name,
            entry.parent_path,
            size_str,
            date_str,
            name_w = max_name,
            parent_w = max_parent,
        ));
    }

    lines.join("\n")
}

/// An honest one-line note naming any scopes the search couldn't cover (a scope
/// pointing at a volume with no search index), or `None` when coverage is complete.
/// Rendered so an agent learns its NAS/ejected-drive scope was skipped rather than
/// reading an empty result as "no matches".
fn coverage_note(result: &SearchResult) -> Option<String> {
    let mut notes = Vec::new();
    if !result.uncovered_scopes.is_empty() {
        notes.push(format!(
            "Note: Cmdr hasn't indexed {} yet, so it isn't searchable. Skipped it.",
            result.uncovered_scopes.join(", ")
        ));
    }
    if !result.unresolved_scopes.is_empty() {
        notes.push(format!(
            "Note: couldn't find {} in the index (a typo, or not indexed yet).",
            result.unresolved_scopes.join(", ")
        ));
    }
    (!notes.is_empty()).then(|| notes.join("\n"))
}

/// Run a routed multi-volume search on a blocking thread (route → load → scan →
/// merge). Shared by both `search` and `ai_search`.
async fn run_search(query: SearchQuery) -> Result<SearchResult, ToolError> {
    tokio::task::spawn_blocking(move || search::run_blocking(query))
        .await
        .map_err(|e| ToolError::internal(format!("Search failed: {e}")))?
        .map_err(ToolError::internal)
}

/// Execute the `search` tool.
pub async fn execute_search(params: &Value) -> ToolResult {
    let pattern = params.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    let pattern_type = match params.get("patternType").and_then(|v| v.as_str()) {
        Some("regex") => PatternType::Regex,
        _ => PatternType::Glob,
    };
    let min_size = params
        .get("sizeMin")
        .and_then(|v| v.as_str())
        .map(parse_human_size)
        .transpose()?;
    let max_size = params
        .get("sizeMax")
        .and_then(|v| v.as_str())
        .map(parse_human_size)
        .transpose()?;
    let modified_after = params
        .get("modifiedAfter")
        .and_then(|v| v.as_str())
        .map(search::ai::iso_date_to_timestamp)
        .transpose()
        .map_err(ToolError::invalid_params)?;
    let modified_before = params
        .get("modifiedBefore")
        .and_then(|v| v.as_str())
        .map(search::ai::iso_date_to_timestamp)
        .transpose()
        .map_err(ToolError::invalid_params)?;
    let is_directory = match params.get("type").and_then(|v| v.as_str()) {
        Some("file") => Some(false),
        Some("dir") => Some(true),
        _ => None,
    };
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

    // Parse scope if provided (routing to the owning volume(s) happens in the runner).
    let scope_str = params.get("scope").and_then(|v| v.as_str());
    let (include_paths, exclude_dir_names) = if let Some(scope) = scope_str {
        let parsed = search::parse_scope(scope);
        let inc = if parsed.include_paths.is_empty() {
            None
        } else {
            Some(parsed.include_paths)
        };
        let exc = if parsed.exclude_patterns.is_empty() {
            None
        } else {
            Some(parsed.exclude_patterns)
        };
        (inc, exc)
    } else {
        (None, None)
    };

    let case_sensitive = params.get("caseSensitive").and_then(|v| v.as_bool());
    let exclude_system_dirs = params.get("excludeSystemDirs").and_then(|v| v.as_bool());
    let count_only = params.get("countOnly").and_then(|v| v.as_bool()).unwrap_or(false);

    let query = SearchQuery {
        name_pattern: pattern,
        pattern_type,
        min_size,
        max_size,
        modified_after,
        modified_before,
        is_directory,
        include_paths,
        exclude_dir_names,
        include_path_ids: None,
        count_only,
        limit,
        case_sensitive,
        exclude_system_dirs,
    };

    let result = run_search(query).await?;

    // Count-only replaces the table with a bare count; the coverage note (if any
    // scope was unindexed) still rides along so the count isn't misread as complete.
    let body = if count_only {
        format_match_count(result.total_count, is_directory)
    } else {
        format_search_results(&result, limit)
    };
    let output = match coverage_note(&result) {
        Some(note) => format!("{note}\n\n{body}"),
        None => body,
    };
    Ok(json!(output))
}

/// Concise count-only response, e.g. "1,234 files match". The noun reflects the
/// type filter (files / folders / items); singular for a count of one.
fn format_match_count(count: u32, is_directory: Option<bool>) -> String {
    let (singular, plural) = match is_directory {
        Some(false) => ("file", "files"),
        Some(true) => ("folder", "folders"),
        None => ("item", "items"),
    };
    if count == 1 {
        format!("1 {singular} matches")
    } else {
        format!("{count} {plural} match")
    }
}

/// Build a `SearchQuery` from a `TranslateResult`, merging in caller-provided scope
/// and the LLM-suggested scope, then applying system directory exclusions.
fn build_search_query_from_translate(
    translate_result: &crate::commands::search::TranslateResult,
    scope_str: Option<&str>,
    limit: u32,
) -> SearchQuery {
    // Start with LLM-suggested scope
    let mut include_paths: Option<Vec<String>> = translate_result.query.include_paths.clone();
    let mut exclude_dir_names: Option<Vec<String>> = translate_result.query.exclude_dir_names.clone();

    // Merge caller-provided scope (the explicit `scope` parameter from the MCP request)
    if let Some(scope) = scope_str {
        let parsed = search::parse_scope(scope);
        if !parsed.include_paths.is_empty() {
            include_paths.get_or_insert_with(Vec::new).extend(parsed.include_paths);
        }
        if !parsed.exclude_patterns.is_empty() {
            exclude_dir_names
                .get_or_insert_with(Vec::new)
                .extend(parsed.exclude_patterns);
        }
    }

    SearchQuery {
        name_pattern: translate_result.query.name_pattern.clone(),
        pattern_type: if translate_result.query.pattern_type == "regex" {
            PatternType::Regex
        } else {
            PatternType::Glob
        },
        min_size: translate_result.query.min_size,
        max_size: translate_result.query.max_size,
        modified_after: translate_result.query.modified_after,
        modified_before: translate_result.query.modified_before,
        is_directory: translate_result.query.is_directory,
        include_path_ids: None,
        count_only: false,
        include_paths,
        exclude_dir_names,
        limit,
        case_sensitive: translate_result.query.case_sensitive,
        exclude_system_dirs: translate_result.query.exclude_system_dirs,
    }
}

/// Execute the `ai_search` tool.
///
/// Single-pass flow: translate natural language → structured query → search.
pub async fn execute_ai_search(params: &Value) -> ToolResult {
    let natural_query = params.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
        log::warn!("MCP ai_search: missing 'query' parameter, returning error");
        ToolError::invalid_params("Missing 'query' parameter")
    })?;
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
    let scope_str = params.get("scope").and_then(|v| v.as_str());
    let total_t = std::time::Instant::now();
    log::info!("MCP ai_search: handler entered, query={natural_query:?}, limit={limit}, scope={scope_str:?}");

    // ── Translate query ──────────────────────────────────────────────
    log::debug!("MCP ai_search: calling translate_search_query for query={natural_query:?}");
    let t = std::time::Instant::now();
    // MCP has no dialog type-toggle context; pass `None` (both files and folders).
    let translate_result = match crate::commands::search::translate_search_query(natural_query.to_string(), None).await
    {
        Ok(tr) => {
            log::info!(
                "MCP ai_search: translate_search_query succeeded in {:.1}s, pattern={:?}",
                t.elapsed().as_secs_f64(),
                tr.query.name_pattern
            );
            tr
        }
        Err(e) => {
            log::warn!(
                "MCP ai_search: translate returned {:?} for query={natural_query:?}: {e}",
                e.kind
            );
            // Branch on the TYPED kind (no string-matching): the not-set-up cases get a
            // clear, actionable message instead of the error-copy-rule-banned "failed".
            use crate::ai::translate_error::AiTranslateErrorKind as K;
            return match e.kind {
                K::Off | K::NotConfigured => Err(ToolError::invalid_params(
                    "AI isn't set up yet. Configure an AI provider in Settings > AI, then run ai_search again."
                        .to_string(),
                )),
                _ => Err(ToolError::internal(format!("AI search couldn't run: {}", e.message))),
            };
        }
    };

    let query = build_search_query_from_translate(&translate_result, scope_str, limit);

    log::debug!("MCP ai_search: running search...");
    let t = std::time::Instant::now();
    let result = run_search(query.clone()).await.inspect_err(|e| {
        crate::log_error!("MCP ai_search: search failed: {}", e.message);
    })?;
    log::info!(
        "MCP ai_search: search completed in {:.1}s, {} results (total_count={})",
        t.elapsed().as_secs_f64(),
        result.entries.len(),
        result.total_count
    );

    // ── Fallback: if 0 results and the LLM suggested searchPaths, retry without them ──
    let (result, query) = if result.total_count == 0
        && translate_result
            .query
            .include_paths
            .as_ref()
            .is_some_and(|p| !p.is_empty())
    {
        log::info!(
            "MCP ai_search: returned 0 results with searchPaths {:?}, retrying full-drive search",
            translate_result.query.include_paths
        );
        let mut fallback_query = query;
        fallback_query.include_paths = None;
        fallback_query.include_path_ids = None;
        let t = std::time::Instant::now();
        let result = run_search(fallback_query.clone()).await.inspect_err(|e| {
            crate::log_error!("MCP ai_search: fallback search failed: {}", e.message);
        })?;
        log::info!(
            "MCP ai_search: fallback full-drive search completed in {:.1}s, {} results",
            t.elapsed().as_secs_f64(),
            result.total_count
        );
        (result, fallback_query)
    } else {
        (result, query)
    };

    let interpreted = summarize_query(&query);
    let formatted = format_search_results(&result, limit);
    let caveat_line = translate_result
        .caveat
        .as_deref()
        .map(|c| format!("Note: {c}\n"))
        .unwrap_or_default();
    let coverage_line = coverage_note(&result).map(|n| format!("{n}\n")).unwrap_or_default();
    let output = format!(
        "{} hits\n\nInterpreted query: {interpreted}\n{caveat_line}{coverage_line}\n{formatted}",
        result.total_count
    );
    log::info!(
        "MCP ai_search: completed in {:.1}s, output length={}",
        total_t.elapsed().as_secs_f64(),
        output.len()
    );
    Ok(json!(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_match_count_reflects_type_and_plurality() {
        assert_eq!(format_match_count(1234, Some(false)), "1234 files match");
        assert_eq!(format_match_count(1, Some(false)), "1 file matches");
        assert_eq!(format_match_count(3, Some(true)), "3 folders match");
        assert_eq!(format_match_count(1, Some(true)), "1 folder matches");
        assert_eq!(format_match_count(42, None), "42 items match");
        assert_eq!(format_match_count(1, None), "1 item matches");
        assert_eq!(format_match_count(0, None), "0 items match");
    }
}
