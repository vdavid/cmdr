//! Search tool handlers (search, ai_search).
//!
//! Both are a thin wrapper on the same path a person's search takes
//! (`docs/specs/unindexed-search-plan.md` Decision 10): `search::run_live_collected`,
//! which walks whatever the index can't answer for. There is no
//! walk-versus-don't parameter and no agent-specific policy — an agent's search
//! walks exactly like a person's, and the only thing the transport changes is
//! that the answer arrives once instead of in batches (`search/live/collect.rs`).

use std::time::Duration;

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::search::PatternType;
use crate::search::{
    self, AnswerEnding, LiveAnswer, SearchQuery, SearchResultEntry, format_size, format_timestamp, summarize_query,
};

/// The least time worth starting `ai_search`'s widened retry with. Below it the
/// retry would report "still walking" over ground it barely touched, which is
/// noise on top of an answer that already said it found nothing.
const FALLBACK_FLOOR: Duration = Duration::from_secs(2);

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
pub fn format_search_results(rows: &[SearchResultEntry], total_count: u32, limit: u32) -> String {
    if rows.is_empty() {
        return "No files found matching the query.".to_string();
    }

    let shown = rows.len().min(limit as usize);
    let entries = &rows[..shown];

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
    lines.push(format!(
        "{shown} of {}:",
        crate::pluralize::pluralize(u64::from(total_count), "result")
    ));

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

/// Everything the run couldn't answer for, as lines above the results.
///
/// Every one of these comes off a TYPED field, never a message match.
/// It is the one thing MCP has always
/// rendered that the dialog didn't, and the reason an agent can't read an empty
/// list as "there's nothing there": each line says which ground the answer
/// doesn't speak for, and what would open it.
fn coverage_note(answer: &LiveAnswer, system_dirs_excluded: bool) -> Option<String> {
    let mut notes = Vec::new();

    if let AnswerEnding::Settled(coverage) = &answer.ending {
        if !coverage.unresolved_scopes.is_empty() {
            notes.push(format!(
                "Note: Cmdr couldn't resolve {}: a typo, or a folder nothing has walked yet.",
                coverage.unresolved_scopes.join(", ")
            ));
        }
        if !coverage.permission_denied.is_empty() {
            notes.push(refusal_note(&coverage.permission_denied, full_disk_access_would_help()));
        }
        if !coverage.declined.is_empty() {
            notes.push(format!(
                "Note: Cmdr never reads snapshot folders (each one is a hardlinked copy of the whole share), so it skipped {}.",
                coverage.declined.join(", ")
            ));
        }
        if !coverage.still_covering.is_empty() {
            notes.push(format!(
                "Note: another search is already walking {}, so this run left it alone. Those results land in the index; run this search again to pick them up.",
                coverage.still_covering.join(", ")
            ));
        }
        if coverage.hidden_by_excludes > 0 {
            // The count is filtered and nothing else on the wire says so. It's the
            // difference between "27 files match" and "27, plus 400 inside caches" —
            // and for a disk-space question the hidden ones ARE usually the answer.
            // The advice only fits while the default tier is still on: with it
            // already off, everything hidden came from the caller's own `!` excludes.
            let (matches, are) = if coverage.hidden_by_excludes == 1 {
                ("match", "is")
            } else {
                ("matches", "are")
            };
            notes.push(if system_dirs_excluded {
                format!(
                    "Note: {} more {} {} inside excluded folders and NOT in the count above: the system, cache, and build tier (node_modules, .git, Caches, …) that's hidden by default, plus any ! excludes in the scope. Pass excludeSystemDirs: false to include the default tier — do that when you're asking where disk space is going, because those folders are usually the answer.",
                    coverage.hidden_by_excludes, matches, are
                )
            } else {
                format!(
                    "Note: {} more {} {} inside the ! excludes in the scope, and NOT in the count above.",
                    coverage.hidden_by_excludes, matches, are
                )
            });
        }
        if coverage.abandoned_ground {
            // The count is places, not folders: a mount that went to sleep marks
            // every directory the walk had reached inside it, and an agent reading
            // "1,497 folders" would conclude the drive is broken.
            notes.push(if coverage.abandoned_locations > 0 {
                format!(
                    "Note: the walk gave up on {} that stopped responding, so this list is a lower bound. Cmdr retries them on its own.",
                    cmdr_fs::pluralize::pluralize(u64::from(coverage.abandoned_locations), "place"),
                )
            } else {
                "Note: the walk gave up on folders that stopped responding, so this list is a lower bound. Cmdr retries them on its own.".to_string()
            });
        }
        match coverage.walk {
            search::WalkEnding::Interrupted => notes.push(
                "Note: the walk stopped before covering everything (the drive went away, or a folder wouldn't read), so this list is a lower bound. Running the search again picks up the rest."
                    .to_string(),
            ),
            search::WalkEnding::Cancelled => notes.push(
                "Note: the search was stopped before it finished, so this list is a lower bound.".to_string(),
            ),
            // Nothing to say: the index covered the whole scope, or the walk
            // covered everything it took. `dirs_found` still reports the work.
            search::WalkEnding::NothingToWalk | search::WalkEnding::Completed => {}
        }
        if answer.dirs_found > 0 && coverage.walk == search::WalkEnding::Completed {
            notes.push(format!(
                "Cmdr walked {} folders it hadn't indexed yet, so the next search over them is instant.",
                answer.dirs_found
            ));
        }
    }

    if matches!(answer.ending, AnswerEnding::StillWalking) {
        notes.push(format!(
            "Note: Cmdr is still walking {} ({} folders so far), so this list and count are a lower bound. The walk keeps filling the index, so running this search again picks up where it left off, or pass a bigger maxWaitSeconds to wait it out.",
            answer.target_volume_id, answer.dirs_found
        ));
    }

    (!notes.is_empty()).then(|| notes.join("\n"))
}

/// Whether pointing at Full Disk Access would actually open a refused folder:
/// on macOS, and only while Cmdr doesn't already hold it. With FDA granted the
/// refusal is something else (a locked folder), and the advice would do nothing.
/// The dialog gates its offer on the same conditions
/// (`coverage-note.ts::offersFullDiskAccess`).
///
/// ⚠️ The platform half is a `#[cfg]`, ❌ never `cfg!(target_os = "macos") && …`:
/// `cfg!` is a runtime value, so the `crate::permissions` call still has to COMPILE
/// on Linux, where that module doesn't exist. It didn't.
#[cfg(target_os = "macos")]
fn full_disk_access_would_help() -> bool {
    !crate::permissions::check_full_disk_access_quiet()
}

/// No such permission exists off macOS: a refusal there is ordinary file
/// permissions, which Cmdr can't grant itself either.
#[cfg(not(target_os = "macos"))]
fn full_disk_access_would_help() -> bool {
    false
}

/// The line for folders the OS refused a walk: the only half of the unreadable
/// ground somebody can act on, so it offers the fix when there is one.
fn refusal_note(paths: &[String], offer_full_disk_access: bool) -> String {
    let refused = format!("Note: the OS refused to let Cmdr read {}.", paths.join(", "));
    if offer_full_disk_access {
        return format!(
            "{refused} Granting Cmdr Full Disk Access in System Settings > Privacy & Security opens them, and the next search covers them."
        );
    }
    refused
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

    // Count-only replaces the table with a bare count; the coverage note (if the
    // run couldn't speak for some ground) still rides along so the count isn't
    // misread as complete.
    let body = if count_only {
        format_match_count(answer.match_count, is_directory)
    } else {
        format_search_results(&answer.entries, answer.match_count, limit)
    };
    let output = match coverage_note(&answer, exclude_system_dirs != Some(false)) {
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

    let interpreted = summarize_query(&query);
    let formatted = format_search_results(&answer.entries, answer.match_count, limit);
    let caveat_line = translate_result
        .caveat
        .as_deref()
        .map(|c| format!("Note: {c}\n"))
        .unwrap_or_default();
    let coverage_line = coverage_note(&answer, translate_result.query.exclude_system_dirs != Some(false))
        .map(|n| format!("{n}\n"))
        .unwrap_or_default();
    let output = format!(
        "{} hits\n\nInterpreted query: {interpreted}\n{caveat_line}{coverage_line}\n{formatted}",
        answer.match_count
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
    use crate::search::WalkEnding;
    use crate::search::live::{CoverageKind, SearchRunCoverage};

    /// A run that covered everything, over `volume`.
    fn covered(volume: &str) -> SearchRunCoverage {
        SearchRunCoverage {
            walk: WalkEnding::NothingToWalk,
            kind: CoverageKind::Covered,
            permission_denied: Vec::new(),
            declined: Vec::new(),
            still_covering: Vec::new(),
            unresolved_scopes: Vec::new(),
            abandoned_ground: false,
            abandoned_locations: 0,
            capped: false,
            target_volume_id: volume.to_string(),
            hidden_by_excludes: 0,
        }
    }

    fn answer(ending: AnswerEnding, dirs_found: u64) -> LiveAnswer {
        LiveAnswer {
            target_volume_id: "naspi".to_string(),
            entries: Vec::new(),
            match_count: 0,
            dirs_found,
            ending,
        }
    }

    #[test]
    fn a_run_that_covered_its_scope_from_the_index_says_nothing() {
        // The note exists to name ground the answer doesn't speak for. A complete
        // answer has none, and a line per search would train an agent to skip
        // them all.
        let settled = answer(AnswerEnding::Settled(Box::new(covered("naspi"))), 0);
        assert_eq!(coverage_note(&settled, true), None);
    }

    #[test]
    fn matches_hidden_by_the_default_exclusions_are_never_silent() {
        // The failure this prevents: "27 files match" over a machine where 400 more
        // sit in node_modules and Caches. Silently filtering a COUNT is how an agent
        // states a wrong conclusion confidently, and a disk-space question is
        // answered mostly by the folders the defaults hide.
        let coverage = SearchRunCoverage {
            hidden_by_excludes: 400,
            ..covered("root")
        };
        let note = coverage_note(&answer(AnswerEnding::Settled(Box::new(coverage)), 0), true)
            .expect("a filtered count always says so");
        assert!(note.contains("400"), "{note}");
        assert!(
            note.contains("excludeSystemDirs"),
            "the way to see them is named: {note}"
        );
    }

    #[test]
    fn with_the_default_tier_already_off_the_note_stops_advising_it() {
        // Everything hidden then came from the caller's own `!` excludes, and
        // telling them to pass a flag they already passed is noise.
        let coverage = SearchRunCoverage {
            hidden_by_excludes: 3,
            ..covered("root")
        };
        let note = coverage_note(&answer(AnswerEnding::Settled(Box::new(coverage)), 0), false)
            .expect("hidden matches are still reported");
        assert!(note.contains('3'), "{note}");
        assert!(!note.contains("excludeSystemDirs"), "{note}");
    }

    #[test]
    fn the_two_unreadable_lists_get_two_different_sentences() {
        // The typed unreadable cause, end to end: one half is a permission somebody can
        // grant, the other is ground Cmdr declines to read. ❌ Never one list and
        // never one sentence — offering Full Disk Access over a snapshot folder
        // is advice that does nothing.
        let coverage = SearchRunCoverage {
            walk: WalkEnding::Completed,
            kind: CoverageKind::Live,
            permission_denied: vec!["/Users/dave/Documents".to_string()],
            declined: vec!["/Volumes/naspi/@eaDir".to_string()],
            ..covered("naspi")
        };
        let note = coverage_note(&answer(AnswerEnding::Settled(Box::new(coverage)), 12), true)
            .expect("unreadable ground is always reported");
        assert!(note.contains("/Users/dave/Documents"), "{note}");
        assert!(note.contains("/Volumes/naspi/@eaDir"), "{note}");
        assert!(note.contains("snapshot folders"), "{note}");
        assert!(
            note.lines().count() >= 3,
            "each cause gets its own sentence, plus the walk's own line: {note}"
        );
    }

    #[test]
    fn full_disk_access_is_offered_only_when_granting_it_would_help() {
        let refused = vec!["/Users/dave/Downloads".to_string()];
        let offered = refusal_note(&refused, true);
        assert!(offered.contains("/Users/dave/Downloads"));
        assert!(offered.contains("Full Disk Access"));
        // Cmdr already has it (or this isn't macOS): the folder is still named,
        // and no advice that would do nothing.
        let plain = refusal_note(&refused, false);
        assert!(plain.contains("/Users/dave/Downloads"));
        assert!(!plain.contains("Full Disk Access"));
    }

    #[test]
    fn a_walk_still_running_says_so_and_says_what_to_do_about_it() {
        // The one thing an agent must not do with a partial answer is read it as
        // complete. It names the drive, the work so far, and the two ways on.
        let note =
            coverage_note(&answer(AnswerEnding::StillWalking, 480), true).expect("a partial answer always says so");
        assert!(note.contains("still walking"), "{note}");
        assert!(note.contains("naspi") && note.contains("480"), "{note}");
        assert!(note.contains("again") && note.contains("maxWaitSeconds"), "{note}");
    }

    #[test]
    fn an_interrupted_walk_says_the_list_is_a_lower_bound() {
        let coverage = SearchRunCoverage {
            walk: WalkEnding::Interrupted,
            kind: CoverageKind::Live,
            ..covered("naspi")
        };
        let note =
            coverage_note(&answer(AnswerEnding::Settled(Box::new(coverage)), 3), true).expect("a short answer says so");
        assert!(note.contains("lower bound"), "{note}");
    }

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
