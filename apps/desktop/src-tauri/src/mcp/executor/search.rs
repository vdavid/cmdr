//! Search tool handlers (search, ai_search).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::search::PatternType;
use crate::search::{
    self, DIALOG_OPEN, SEARCH_INDEX, SearchIndexState, SearchQuery, SearchResult, fill_directory_sizes, format_size,
    format_timestamp, summarize_query,
};

/// Ensure the search index is loaded. Returns the index or an error.
async fn ensure_search_index() -> Result<Arc<search::SearchIndex>, ToolError> {
    // Check if already loaded
    {
        let guard = SEARCH_INDEX.lock().map_err(|e| ToolError::internal(format!("{e}")))?;
        if let Some(ref state) = *guard {
            if state.index.entries.is_empty() && state.index.generation == 0 {
                // Loading sentinel — wait briefly then check again
                log::warn!("MCP ai_search: search index is in loading sentinel state (empty, gen=0), will reload");
            } else {
                log::debug!(
                    "MCP ai_search: search index already loaded, {} entries, gen={}",
                    state.index.entries.len(),
                    state.index.generation
                );
                return Ok(state.index.clone());
            }
        } else {
            log::debug!("MCP ai_search: search index not loaded, will load now");
        }
    }

    // Not loaded — load synchronously via spawn_blocking
    let pool = crate::indexing::get_read_pool().ok_or_else(|| {
        log::error!("MCP ai_search: drive index not available (no read pool)");
        ToolError::internal(
            "Drive index not available. Make sure indexing is enabled and the initial scan has completed.",
        )
    })?;

    DIALOG_OPEN.store(false, Ordering::Relaxed);

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    log::debug!("MCP ai_search: loading search index from DB...");
    let index = tokio::task::spawn_blocking(move || search::load_search_index(&pool, &cancel_clone))
        .await
        .map_err(|e| {
            log::error!("MCP ai_search: search index load spawn_blocking failed: {e}");
            ToolError::internal(format!("Search index load failed: {e}"))
        })?
        .map_err(|e| {
            log::error!("MCP ai_search: search index load failed: {e}");
            ToolError::internal(format!("Search index load failed: {e}"))
        })?;

    log::debug!(
        "MCP ai_search: search index loaded from DB, {} entries",
        index.entries.len()
    );
    let index = Arc::new(index);

    // Store it for reuse (no timers for MCP — one-shot)
    {
        let mut guard = SEARCH_INDEX.lock().map_err(|e| ToolError::internal(format!("{e}")))?;
        *guard = Some(SearchIndexState {
            index: index.clone(),
            idle_timer: None,
            backstop_timer: None,
            load_cancel: Some(cancel),
        });
    }

    Ok(index)
}

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

/// Run search and post-process (fill dir sizes, post-filter, truncate).
fn run_search_and_postprocess(index: &search::SearchIndex, query: &SearchQuery) -> Result<SearchResult, ToolError> {
    let mut result = search::search(index, query).map_err(ToolError::internal)?;

    // Fill directory sizes from the DB
    if result.entries.iter().any(|e| e.is_directory)
        && let Some(pool) = crate::indexing::get_read_pool()
    {
        fill_directory_sizes(&mut result, &pool);
    }

    // Post-filter: remove directories that don't match size criteria
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    if has_size_filter {
        result.entries.retain(|e| {
            if !e.is_directory {
                return true;
            }
            if let Some(min) = query.min_size {
                match e.size {
                    Some(s) if s >= min => {}
                    _ => return false,
                }
            }
            if let Some(max) = query.max_size {
                match e.size {
                    Some(s) if s <= max => {}
                    _ => return false,
                }
            }
            true
        });
        result.total_count = result.entries.len() as u32;
    }

    // Truncate to limit
    let limit = query.limit.min(1000) as usize;
    if result.entries.len() > limit {
        result.entries.truncate(limit);
    }

    Ok(result)
}

/// Execute the `search` tool.
pub async fn execute_search(params: &Value) -> ToolResult {
    let pattern = params.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    let pattern_type = match params.get("pattern_type").and_then(|v| v.as_str()) {
        Some("regex") => PatternType::Regex,
        _ => PatternType::Glob,
    };
    let min_size = params
        .get("min_size")
        .and_then(|v| v.as_str())
        .map(parse_human_size)
        .transpose()?;
    let max_size = params
        .get("max_size")
        .and_then(|v| v.as_str())
        .map(parse_human_size)
        .transpose()?;
    let modified_after = params
        .get("modified_after")
        .and_then(|v| v.as_str())
        .map(search::ai::iso_date_to_timestamp)
        .transpose()
        .map_err(ToolError::invalid_params)?;
    let modified_before = params
        .get("modified_before")
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

    let index = ensure_search_index().await?;

    // Parse scope if provided
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

    let mut query = SearchQuery {
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
        limit,
        case_sensitive,
        exclude_system_dirs,
    };

    // Resolve include paths to entry IDs via SQLite
    if query.include_paths.as_ref().is_some_and(|p| !p.is_empty())
        && let Some(pool) = crate::indexing::get_read_pool()
    {
        search::resolve_include_paths(&mut query, &pool);
    }

    let query_clone = query.clone();
    let index_clone = index.clone();
    let result = tokio::task::spawn_blocking(move || run_search_and_postprocess(&index_clone, &query_clone))
        .await
        .map_err(|e| ToolError::internal(format!("Search failed: {e}")))??;

    Ok(json!(format_search_results(&result, limit)))
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

    log::debug!("MCP ai_search: loading search index...");
    let index = match ensure_search_index().await {
        Ok(idx) => {
            log::debug!("MCP ai_search: search index loaded, {} entries", idx.entries.len());
            idx
        }
        Err(e) => {
            log::error!("MCP ai_search: search index load failed: {}", e.message);
            return Err(e);
        }
    };

    // ── Translate query ──────────────────────────────────────────────
    log::debug!("MCP ai_search: calling translate_search_query for query={natural_query:?}");
    let t = std::time::Instant::now();
    let translate_result = match crate::commands::search::translate_search_query(natural_query.to_string()).await {
        Ok(tr) => {
            log::info!(
                "MCP ai_search: translate_search_query succeeded in {:.1}s, pattern={:?}",
                t.elapsed().as_secs_f64(),
                tr.query.name_pattern
            );
            tr
        }
        Err(e) => {
            log::warn!("MCP ai_search: LLM call failed for query={natural_query:?}: {e}");
            return Err(ToolError::internal(format!("AI translation failed: {e}")));
        }
    };

    let mut query = build_search_query_from_translate(&translate_result, scope_str, limit);

    // Resolve include paths to entry IDs via SQLite
    if query.include_paths.as_ref().is_some_and(|p| !p.is_empty())
        && let Some(pool) = crate::indexing::get_read_pool()
    {
        search::resolve_include_paths(&mut query, &pool);
    }

    log::debug!("MCP ai_search: running search...");
    let t = std::time::Instant::now();
    let query_clone = query.clone();
    let index_clone = index.clone();
    let result = match tokio::task::spawn_blocking(move || run_search_and_postprocess(&index_clone, &query_clone)).await
    {
        Ok(Ok(result)) => {
            log::info!(
                "MCP ai_search: search completed in {:.1}s, {} results (total_count={})",
                t.elapsed().as_secs_f64(),
                result.entries.len(),
                result.total_count
            );
            result
        }
        Ok(Err(e)) => {
            log::error!("MCP ai_search: search failed (postprocess): {}", e.message);
            return Err(e);
        }
        Err(e) => {
            log::error!("MCP ai_search: spawn_blocking failed (task join): {e}");
            return Err(ToolError::internal(format!("Search failed: {e}")));
        }
    };

    // ── Fallback: if 0 results and LLM suggested searchPaths, retry without them ──
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
        let fallback_query_clone = fallback_query.clone();
        let index_clone = index.clone();
        let t = std::time::Instant::now();
        match tokio::task::spawn_blocking(move || run_search_and_postprocess(&index_clone, &fallback_query_clone)).await
        {
            Ok(Ok(result)) => {
                log::info!(
                    "MCP ai_search: fallback full-drive search completed in {:.1}s, {} results",
                    t.elapsed().as_secs_f64(),
                    result.total_count
                );
                (result, fallback_query)
            }
            Ok(Err(e)) => {
                log::error!("MCP ai_search: fallback search failed: {}", e.message);
                return Err(e);
            }
            Err(e) => {
                log::error!("MCP ai_search: fallback spawn_blocking failed: {e}");
                return Err(ToolError::internal(format!("Search failed: {e}")));
            }
        }
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
    let output = format!(
        "{} hits\n\nInterpreted query: {interpreted}\n{caveat_line}\n{formatted}",
        result.total_count
    );
    log::info!(
        "MCP ai_search: completed in {:.1}s, output length={}",
        total_t.elapsed().as_secs_f64(),
        output.len()
    );
    Ok(json!(output))
}
