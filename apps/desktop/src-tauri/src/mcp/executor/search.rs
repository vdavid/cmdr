//! Search tool handlers (search, ai_search).
//!
//! Both are a thin wrapper on the same path a person's search takes
//! (`docs/specs/unindexed-search-plan.md` Decision 10): `search::run_live_collected`,
//! which walks whatever the index can't answer for. There is no
//! walk-versus-don't parameter and no agent-specific policy — an agent's search
//! walks exactly like a person's, and the only thing the transport changes is
//! that the answer arrives once instead of in batches (`search/live/collect.rs`).
//!
//! This file parses the call and runs it. The typed JSON both tools answer with,
//! and every rule about its counts and its coverage, live in `search/result.rs`.

use std::time::Duration;

use serde_json::Value;

mod result;

use result::{AiSearchResult, shape_answer};

use super::{ToolError, ToolResult};
use crate::search::PatternType;
use crate::search::{self, AnswerEnding, LiveAnswer, SearchQuery, summarize_query};

/// The least time worth starting `ai_search`'s widened retry with. Below it the
/// retry would report "still walking" over ground it barely touched, which is
/// noise on top of an answer that already said it found nothing.
const FALLBACK_FLOOR: Duration = Duration::from_secs(2);

/// The row cap when the caller doesn't ask for one, and the ceiling on any value
/// they do ask for — the house cap `inspect_file`, `image_facts`, and
/// `list_pane_files` all use.
///
/// It bounds the ROWS. `result::shape_answer` still cuts the page to what one
/// tool result may carry, because 200 rows of long paths isn't a bounded payload
/// either.
const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 200;

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

/// Run a search over its one target volume and wait for the answer.
///
/// Shared by `search` and `ai_search`, and the SAME path the dialog takes: an
/// agent's search walks what the index can't answer for, exactly like a
/// person's. Waits up to `budget`; past that the walk carries on and the reply
/// says so.
async fn run_search(query: SearchQuery, budget: Duration) -> Result<LiveAnswer, ToolError> {
    let answer = tokio::task::spawn_blocking(move || search::run_live_collected(query, budget))
        .await
        .map_err(|e| ToolError::internal(format!("Search couldn't run: {e}")))?
        .map_err(ToolError::invalid_params)?;

    // A run that couldn't run at all is the caller's problem to fix, so it comes
    // back as an error rather than as an empty list with a note. Branch on the
    // typed cause: a query the walk refuses is the caller's to narrow, an index
    // that won't open is not.
    if let AnswerEnding::Failed { error, message } = &answer.ending {
        return Err(match error {
            search::SearchRunError::Query => ToolError::invalid_params(message.clone()),
            search::SearchRunError::IndexUnreadable => ToolError::internal(message.clone()),
        });
    }
    Ok(answer)
}

/// How many rows the caller asked for, clamped to [`MAX_LIMIT`].
///
/// Clamped rather than refused: a caller asking for 5,000 wants "as many as you
/// can", and 200 IS as many as one tool result can carry. `returned` and
/// `truncated` in the reply say what they actually got.
fn requested_limit(params: &Value) -> u32 {
    params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map_or(DEFAULT_LIMIT, |n| n.clamp(1, u64::from(MAX_LIMIT)) as u32)
}

/// How long this call may wait for its answer, from the caller's `maxWaitSeconds`.
///
/// ❌ Not a walk-versus-don't switch: the walk happens either way, and it keeps
/// going after the wait runs out. This only says how much of it to wait for.
fn wait_budget(params: &Value) -> Duration {
    params
        .get("maxWaitSeconds")
        .and_then(|v| v.as_u64())
        .map(Duration::from_secs)
        .unwrap_or(search::AGENT_WAIT_DEFAULT)
        .clamp(Duration::from_secs(1), search::AGENT_WAIT_MAX)
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
    let limit = requested_limit(params);

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
    let sort_by = match params.get("sortBy").and_then(|v| v.as_str()) {
        None | Some("relevance") => None,
        Some("size") => Some(search::SearchSort::Size),
        Some("modified") => Some(search::SearchSort::Modified),
        // Refused, not defaulted: a caller who asked for the biggest matches and
        // silently got the best-ranked ones would read the top row as the biggest.
        Some(other) => {
            return Err(ToolError::invalid_params(format!(
                "Unknown sortBy '{other}'. Use 'relevance', 'size', or 'modified'."
            )));
        }
    };
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
        sort_by,
    };

    let answer = run_search(query, wait_budget(params)).await?;

    // Count-only needs no branch: the run returns no rows, so `entries` is empty
    // and `matchCount` carries the answer. The coverage still rides along, so the
    // count is never misread as complete.
    shape(shape_answer(answer, exclude_system_dirs != Some(false)))
}

/// Serialize a result DTO to the tool's JSON value.
fn shape<T: serde::Serialize>(result: T) -> ToolResult {
    serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))
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
        // The AI translator shapes filters, not ordering: an ai_search answer is
        // ranked by relevance like the dialog's.
        sort_by: None,
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
    let limit = requested_limit(params);
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

    // One budget for the whole call, however many searches it takes: the caller
    // is waiting on ONE tool call, and a fallback that got its own full budget
    // could double the wait it asked for.
    let budget = wait_budget(params);
    let deadline = std::time::Instant::now() + budget;

    log::debug!("MCP ai_search: running search...");
    let t = std::time::Instant::now();
    let answer = run_search(query.clone(), budget).await.inspect_err(|e| {
        crate::log_error!("MCP ai_search: the search couldn't run: {}", e.message);
    })?;
    log::info!(
        "MCP ai_search: search completed in {:.1}s, {} results (match_count={})",
        t.elapsed().as_secs_f64(),
        answer.entries.len(),
        answer.match_count
    );

    // ── Fallback: if 0 results and the LLM suggested searchPaths, retry without them ──
    // Only once the first run SETTLED: a run still walking hasn't finished
    // answering, so widening the scope on it would trade a partial answer for a
    // second walk over even more ground.
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    let (answer, query) = if answer.match_count == 0
        && matches!(answer.ending, AnswerEnding::Settled(_))
        && remaining >= FALLBACK_FLOOR
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
        let answer = run_search(fallback_query.clone(), remaining).await.inspect_err(|e| {
            crate::log_error!("MCP ai_search: the fallback search couldn't run: {}", e.message);
        })?;
        log::info!(
            "MCP ai_search: fallback full-drive search completed in {:.1}s, {} results",
            t.elapsed().as_secs_f64(),
            answer.match_count
        );
        (answer, fallback_query)
    } else {
        (answer, query)
    };

    let match_count = answer.match_count;
    let mut search = shape_answer(answer, translate_result.query.exclude_system_dirs != Some(false));
    // The translator's caveat leads the notes: it says the query may not be the
    // one the caller meant, which every other note is downstream of.
    if let Some(caveat) = translate_result.caveat.as_deref() {
        search.notes.insert(0, format!("Note: {caveat}"));
    }
    log::info!(
        "MCP ai_search: completed in {:.1}s, {} of {match_count} rows returned",
        total_t.elapsed().as_secs_f64(),
        search.returned
    );
    shape(AiSearchResult {
        interpreted_query: summarize_query(&query),
        search,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_wait_budget_defaults_and_clamps() {
        // The transport knob, not a policy one: it can't turn the walk off, and
        // it can't hold a caller's turn open indefinitely either.
        assert_eq!(wait_budget(&json!({})), search::AGENT_WAIT_DEFAULT);
        assert_eq!(wait_budget(&json!({ "maxWaitSeconds": 45 })), Duration::from_secs(45));
        assert_eq!(wait_budget(&json!({ "maxWaitSeconds": 0 })), Duration::from_secs(1));
        assert_eq!(wait_budget(&json!({ "maxWaitSeconds": 9_000 })), search::AGENT_WAIT_MAX);
    }

    #[test]
    fn the_row_limit_defaults_and_clamps_to_the_house_cap() {
        // `limit: 5000` used to reach the engine untouched and serialize every
        // row it found, which is the payload that pushed a caller's turn out of
        // its own prompt.
        assert_eq!(requested_limit(&json!({})), DEFAULT_LIMIT);
        assert_eq!(requested_limit(&json!({ "limit": 5 })), 5);
        assert_eq!(requested_limit(&json!({ "limit": 5_000 })), MAX_LIMIT);
        assert_eq!(requested_limit(&json!({ "limit": u64::MAX })), MAX_LIMIT);
        assert_eq!(requested_limit(&json!({ "limit": 0 })), 1);
    }
}
