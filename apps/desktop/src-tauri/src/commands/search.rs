//! IPC commands for drive search.
//!
//! Thin wrappers around `indexing::search` module functions, exposed to the frontend via Tauri commands.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::ai::client::{AiBackend, ChatCompletionOptions};
use crate::indexing::get_read_pool;
use crate::indexing::search::{
    self, DIALOG_OPEN, SEARCH_INDEX, SearchIndexState, SearchQuery, SearchResult, drop_search_index,
    fill_directory_sizes, start_backstop_timer, start_idle_timer, touch_activity,
};
use crate::indexing::writer::WRITER_GENERATION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareResult {
    pub ready: bool,
    pub entry_count: u64,
}

/// Called when the search dialog opens. Starts loading the index in the background.
/// Returns immediately with `{ ready, entryCount }`.
#[tauri::command]
pub async fn prepare_search_index(app: tauri::AppHandle) -> Result<PrepareResult, String> {
    touch_activity();
    DIALOG_OPEN.store(true, Ordering::Relaxed);

    // Check if already loaded and fresh
    {
        let mut guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;
        if let Some(ref mut state) = *guard {
            let current_gen = WRITER_GENERATION.load(Ordering::Relaxed);
            if state.index.generation == current_gen {
                // Cancel any pending idle timer
                if let Some(ref h) = state.idle_timer {
                    h.abort();
                }
                state.idle_timer = None;
                // Reset backstop timer — the previous session's timer may still
                // be ticking and could fire while the dialog is open.
                if let Some(ref h) = state.backstop_timer {
                    h.abort();
                }
                state.backstop_timer = Some(start_backstop_timer());
                return Ok(PrepareResult {
                    ready: true,
                    entry_count: state.index.entries.len() as u64,
                });
            }
            // Stale — drop and reload below
        }
    }

    // Drop stale index if any
    drop_search_index();

    let pool = get_read_pool().ok_or_else(|| "Index not available".to_string())?;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    // Store a "loading" sentinel with the cancel flag BEFORE spawning the task.
    // This closes the race window where release_search_index can't cancel the
    // load between checking the lock and the background task starting.
    {
        let mut guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;
        if guard.is_some() {
            return Ok(PrepareResult {
                ready: false,
                entry_count: 0,
            });
        }
        *guard = Some(SearchIndexState {
            index: Arc::new(search::SearchIndex {
                names: String::new(),
                entries: Vec::new(),
                id_to_index: HashMap::new(),
                generation: 0, // sentinel: never matches a real generation
            }),
            idle_timer: None,
            backstop_timer: None,
            load_cancel: Some(cancel.clone()),
        });
    }

    // Spawn the load in a background task
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = tokio::task::spawn_blocking(move || search::load_search_index(&pool, &cancel_clone)).await;

        match result {
            Ok(Ok(index)) => {
                let entry_count = index.entries.len() as u64;
                let backstop = start_backstop_timer();
                let mut guard = match SEARCH_INDEX.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                *guard = Some(SearchIndexState {
                    index: Arc::new(index),
                    idle_timer: None,
                    backstop_timer: Some(backstop),
                    load_cancel: Some(cancel),
                });
                log::debug!("Search index ready: {entry_count} entries");
                // Emit event to frontend
                use tauri::Emitter;
                let _ = app_clone.emit(
                    "search-index-ready",
                    serde_json::json!({
                        "entryCount": entry_count,
                    }),
                );
            }
            Ok(Err(e)) => {
                if e.contains("cancelled") {
                    log::debug!("Search index load cancelled");
                } else {
                    log::warn!("Search index load failed: {e}");
                }
            }
            Err(e) => {
                log::warn!("Search index load task panicked: {e}");
            }
        }
    });

    Ok(PrepareResult {
        ready: false,
        entry_count: 0,
    })
}

/// Search the in-memory index. Returns empty if not loaded yet.
#[tauri::command]
pub async fn search_files(query: SearchQuery) -> Result<SearchResult, String> {
    touch_activity();

    let index = {
        let guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;
        match guard.as_ref() {
            Some(state) => {
                // Cancel any idle timer since we're actively searching
                if let Some(ref h) = state.idle_timer {
                    h.abort();
                }
                state.index.clone()
            }
            None => {
                return Ok(SearchResult {
                    entries: Vec::new(),
                    total_count: 0,
                });
            }
        }
    };

    // Run search on a blocking thread (rayon parallel scan)
    let query_clone = query.clone();
    let index_clone = index.clone();
    let mut result = tokio::task::spawn_blocking(move || search::search(&index_clone, &query_clone))
        .await
        .map_err(|e| format!("Search task failed: {e}"))??;

    // Fill directory sizes from the DB
    if result.entries.iter().any(|e| e.is_directory)
        && let Some(pool) = get_read_pool()
    {
        fill_directory_sizes(&mut result, &pool);
    }

    // Post-filter: remove directories that don't match size criteria.
    // Directory sizes come from dir_stats (not the entries table), so the
    // main search() can't filter them. We over-fetch candidates and trim here.
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    if has_size_filter {
        result.entries.retain(|e| {
            if !e.is_directory {
                return true; // files already filtered in search()
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
        // total_count is approximate after post-filtering — the true count
        // would require fetching dir_stats for ALL matching directories, which
        // is too expensive. The displayed count may overestimate slightly.
        result.total_count = result.entries.len() as u32;
    }

    // Truncate to the originally requested limit
    let limit = query.limit.min(1000) as usize;
    if result.entries.len() > limit {
        result.entries.truncate(limit);
    }

    // Check generation staleness — trigger background reload if needed
    let current_gen = WRITER_GENERATION.load(Ordering::Relaxed);
    if index.generation != current_gen {
        log::debug!(
            "Search index stale (gen {} vs {}), will reload on next prepare",
            index.generation,
            current_gen
        );
    }

    Ok(result)
}

/// Called when the search dialog closes. Starts the idle timer and
/// cancels any in-progress load.
#[tauri::command]
pub async fn release_search_index() -> Result<(), String> {
    DIALOG_OPEN.store(false, Ordering::Relaxed);
    let mut guard = SEARCH_INDEX.lock().map_err(|e| format!("{e}"))?;

    // Set cancellation flag on any in-progress load
    if let Some(ref state) = *guard
        && let Some(ref cancel) = state.load_cancel
    {
        cancel.store(true, Ordering::Relaxed);
    }

    // Start idle timer
    if guard.is_some() {
        let idle_handle = start_idle_timer();
        if let Some(ref mut state) = *guard {
            // Cancel previous idle timer if any
            if let Some(ref h) = state.idle_timer {
                h.abort();
            }
            state.idle_timer = Some(idle_handle);
        }
    }

    Ok(())
}

// ============================================================================
// AI search query translation
// ============================================================================

/// Intermediate struct for LLM output — uses ISO date strings.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSearchQuery {
    pub(crate) name_pattern: Option<String>,
    pub(crate) pattern_type: Option<String>,
    pub(crate) min_size: Option<u64>,
    pub(crate) max_size: Option<u64>,
    pub(crate) modified_after: Option<String>,
    pub(crate) modified_before: Option<String>,
    pub(crate) is_directory: Option<bool>,
}

/// Human-readable field values returned alongside the structured query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub query: TranslatedQuery,
    pub display: TranslateDisplay,
}

/// The structured query with unix timestamps, ready for `search_files`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslatedQuery {
    pub name_pattern: Option<String>,
    pub pattern_type: String,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<u64>,
    pub modified_before: Option<u64>,
    pub is_directory: Option<bool>,
}

/// Human-readable values so the frontend can populate filter UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateDisplay {
    pub name_pattern: Option<String>,
    pub pattern_type: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub modified_after: Option<String>,
    pub modified_before: Option<String>,
    pub is_directory: Option<bool>,
}

/// Converts an ISO date string (YYYY-MM-DD) to a unix timestamp (seconds since epoch).
pub(crate) fn iso_date_to_timestamp(date_str: &str) -> Result<u64, String> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let date = time::Date::parse(date_str, &format).map_err(|e| format!("Invalid date '{date_str}': {e}"))?;
    let datetime = date.with_hms(0, 0, 0).expect("midnight is always valid");
    let timestamp = datetime.assume_utc().unix_timestamp();
    if timestamp < 0 {
        return Err(format!("Date '{date_str}' is before unix epoch"));
    }
    Ok(timestamp as u64)
}

pub(crate) fn build_search_system_prompt() -> String {
    let today = time::OffsetDateTime::now_utc().date();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let today_str = today.format(&format).expect("date format always succeeds");

    format!(
        "You translate natural language file search queries into structured JSON filters.\n\
         \n\
         Return ONLY a JSON object with these optional fields:\n\
         - \"namePattern\": a filename pattern. Use glob (*, ?) for simple cases, regex for complex ones.\n\
         - \"patternType\": \"glob\" or \"regex\" — specify which format you used for namePattern.\n\
         - \"minSize\": size in bytes (e.g., 1048576 for 1 MB)\n\
         - \"maxSize\": size in bytes\n\
         - \"modifiedAfter\": ISO date string (e.g., \"2025-01-01\")\n\
         - \"modifiedBefore\": ISO date string\n\
         - \"isDirectory\": true for folders only, false for files only, omit for both\n\
         \n\
         For regex patterns, use Rust `regex` crate syntax (PCRE-like but no lookahead/lookbehind, \
         no backreferences, no \\d — use [0-9] instead). All regex is case-insensitive and unanchored \
         (partial match) unless you add ^ or $.\n\
         \n\
         Examples:\n\
         \"large pdfs\" → {{\"namePattern\": \"*.pdf\", \"patternType\": \"glob\", \"minSize\": 10485760}}\n\
         \"quarterly reports\" → {{\"namePattern\": \"(Q[1-4]|quarterly).*\\.pdf\", \"patternType\": \"regex\"}}\n\
         \"photos from last month\" → {{\"namePattern\": \"*.jpg\", \"patternType\": \"glob\", \"modifiedAfter\": \"2026-02-15\"}}\n\
         \"folders bigger than 1gb\" → {{\"isDirectory\": true, \"minSize\": 1073741824}}\n\
         \n\
         Today's date is {today_str}. Return ONLY the JSON, no explanation."
    )
}

/// Strips markdown code fences from an LLM response and parses it as JSON.
pub(crate) fn parse_ai_response(response: &str) -> Result<AiSearchQuery, String> {
    let json_str = response.trim();
    let json_str = json_str
        .strip_prefix("```json")
        .or_else(|| json_str.strip_prefix("```"))
        .unwrap_or(json_str);
    let json_str = json_str.strip_suffix("```").unwrap_or(json_str).trim();

    serde_json::from_str(json_str)
        .map_err(|_| "Couldn't understand that query. Try rephrasing or use the manual filters.".to_string())
}

/// Converts a parsed `AiSearchQuery` into the final `TranslateResult`.
pub(crate) fn build_translate_result(ai_query: AiSearchQuery) -> Result<TranslateResult, String> {
    let modified_after_ts = ai_query
        .modified_after
        .as_deref()
        .map(iso_date_to_timestamp)
        .transpose()?;
    let modified_before_ts = ai_query
        .modified_before
        .as_deref()
        .map(iso_date_to_timestamp)
        .transpose()?;

    let pattern_type = ai_query.pattern_type.clone().unwrap_or_else(|| "glob".to_string());

    Ok(TranslateResult {
        query: TranslatedQuery {
            name_pattern: ai_query.name_pattern.clone(),
            pattern_type: pattern_type.clone(),
            min_size: ai_query.min_size,
            max_size: ai_query.max_size,
            modified_after: modified_after_ts,
            modified_before: modified_before_ts,
            is_directory: ai_query.is_directory,
        },
        display: TranslateDisplay {
            name_pattern: ai_query.name_pattern,
            pattern_type: Some(pattern_type),
            min_size: ai_query.min_size,
            max_size: ai_query.max_size,
            modified_after: ai_query.modified_after,
            modified_before: ai_query.modified_before,
            is_directory: ai_query.is_directory,
        },
    })
}

/// If the AI returned a regex pattern, validates it against the `regex` crate.
/// Returns `Ok(())` if valid or not a regex, `Err(message)` with the compile error otherwise.
pub(crate) fn validate_regex_pattern(ai_query: &AiSearchQuery) -> Result<(), String> {
    let is_regex = ai_query.pattern_type.as_deref().is_some_and(|t| t == "regex");
    if !is_regex {
        return Ok(());
    }
    if let Some(ref pattern) = ai_query.name_pattern {
        regex::Regex::new(pattern).map_err(|e| format!("{e}"))?;
    }
    Ok(())
}

/// Translates a natural language search query into structured filters using the configured LLM.
#[tauri::command]
pub async fn translate_search_query(natural_query: String) -> Result<TranslateResult, String> {
    let provider = crate::ai::manager::get_provider();

    let backend = match provider.as_str() {
        "off" => return Err("AI is not configured. Enable an AI provider in settings.".to_string()),
        "local" => {
            let port = crate::ai::manager::get_port()
                .ok_or_else(|| "Local AI server isn't running. Start it in settings.".to_string())?;
            AiBackend::Local { port }
        }
        "openai-compatible" => {
            let (api_key, base_url, model) = crate::ai::manager::get_openai_config();
            if api_key.is_empty() {
                return Err("OpenAI API key not configured. Add it in settings.".to_string());
            }
            AiBackend::OpenAi {
                api_key,
                base_url,
                model,
            }
        }
        _ => return Err(format!("Unknown AI provider: {provider}")),
    };

    let options = ChatCompletionOptions {
        system_prompt: build_search_system_prompt(),
        temperature: 0.3,
        max_tokens: 200,
        top_p: 0.9,
    };

    let response = crate::ai::client::chat_completion(&backend, &natural_query, &options)
        .await
        .map_err(|e| format!("{e}"))?;

    let mut ai_query = parse_ai_response(&response)?;

    // Validate regex patterns — retry once if invalid
    if let Err(regex_error) = validate_regex_pattern(&ai_query) {
        let pattern = ai_query.name_pattern.as_deref().unwrap_or("");
        let retry_prompt = format!(
            "You gave me this pattern: `{pattern}`, but it's not valid regex: {regex_error}. \
             Please fix it using Rust `regex` crate syntax (no lookahead/lookbehind, no backreferences). \
             Return the same JSON object with the corrected pattern."
        );

        log::debug!("AI regex validation failed, retrying: {regex_error}");

        let retry_response = crate::ai::client::chat_completion(&backend, &retry_prompt, &options)
            .await
            .map_err(|e| format!("{e}"))?;

        ai_query = parse_ai_response(&retry_response)?;

        // Validate the retry — if still invalid, return the error
        if let Err(retry_error) = validate_regex_pattern(&ai_query) {
            return Err(format!("AI generated invalid regex pattern: {retry_error}"));
        }
    }

    build_translate_result(ai_query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso_date_to_timestamp() {
        // 2025-01-01 00:00:00 UTC = 1735689600
        let ts = iso_date_to_timestamp("2025-01-01").unwrap();
        assert_eq!(ts, 1_735_689_600);
    }

    #[test]
    fn test_iso_date_to_timestamp_mid_year() {
        // 2026-06-15 00:00:00 UTC = 1781481600
        let ts = iso_date_to_timestamp("2026-06-15").unwrap();
        assert_eq!(ts, 1_781_481_600);
    }

    #[test]
    fn test_iso_date_to_timestamp_invalid() {
        assert!(iso_date_to_timestamp("not-a-date").is_err());
        assert!(iso_date_to_timestamp("2025-13-01").is_err());
        assert!(iso_date_to_timestamp("2025-01-32").is_err());
    }

    #[test]
    fn test_ai_search_query_deserialization_all_fields() {
        let json = r#"{
            "namePattern": "*.pdf",
            "patternType": "glob",
            "minSize": 1048576,
            "maxSize": 10485760,
            "modifiedAfter": "2025-01-01",
            "modifiedBefore": "2025-12-31",
            "isDirectory": false
        }"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.pdf"));
        assert_eq!(q.pattern_type.as_deref(), Some("glob"));
        assert_eq!(q.min_size, Some(1_048_576));
        assert_eq!(q.max_size, Some(10_485_760));
        assert_eq!(q.modified_after.as_deref(), Some("2025-01-01"));
        assert_eq!(q.modified_before.as_deref(), Some("2025-12-31"));
        assert_eq!(q.is_directory, Some(false));
    }

    #[test]
    fn test_ai_search_query_deserialization_minimal() {
        let json = r#"{"namePattern": "*.jpg"}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.jpg"));
        assert!(q.pattern_type.is_none());
        assert!(q.min_size.is_none());
        assert!(q.max_size.is_none());
        assert!(q.modified_after.is_none());
        assert!(q.modified_before.is_none());
        assert!(q.is_directory.is_none());
    }

    #[test]
    fn test_ai_search_query_deserialization_empty_object() {
        let json = r#"{}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert!(q.name_pattern.is_none());
        assert!(q.pattern_type.is_none());
    }

    #[test]
    fn test_ai_search_query_deserialization_regex_type() {
        let json = r#"{"namePattern": "Q[1-4].*\\.pdf", "patternType": "regex"}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.pattern_type.as_deref(), Some("regex"));
    }

    #[test]
    fn test_ai_search_query_deserialization_directory_only() {
        let json = r#"{"isDirectory": true, "minSize": 1073741824}"#;
        let q: AiSearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.is_directory, Some(true));
        assert_eq!(q.min_size, Some(1_073_741_824));
    }

    #[test]
    fn test_translate_result_serialization() {
        let result = TranslateResult {
            query: TranslatedQuery {
                name_pattern: Some("*.pdf".to_string()),
                pattern_type: "glob".to_string(),
                min_size: Some(1_048_576),
                max_size: None,
                modified_after: Some(1_735_689_600),
                modified_before: None,
                is_directory: None,
            },
            display: TranslateDisplay {
                name_pattern: Some("*.pdf".to_string()),
                pattern_type: Some("glob".to_string()),
                min_size: Some(1_048_576),
                max_size: None,
                modified_after: Some("2025-01-01".to_string()),
                modified_before: None,
                is_directory: None,
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("2025-01-01"));
    }

    #[test]
    fn test_build_search_system_prompt_contains_date() {
        let prompt = build_search_system_prompt();
        assert!(prompt.contains("Today's date is"));
        assert!(prompt.contains("Return ONLY a JSON object"));
        // Should contain a date in YYYY-MM-DD format
        assert!(prompt.contains("20")); // Year starts with 20
    }

    #[test]
    fn test_build_search_system_prompt_contains_regex_flavor() {
        let prompt = build_search_system_prompt();
        assert!(prompt.contains("Rust `regex` crate syntax"));
        assert!(prompt.contains("no lookahead/lookbehind"));
    }

    #[test]
    fn test_parse_ai_response_plain_json() {
        let response = r#"{"namePattern": "*.pdf", "patternType": "glob"}"#;
        let q = parse_ai_response(response).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.pdf"));
        assert_eq!(q.pattern_type.as_deref(), Some("glob"));
    }

    #[test]
    fn test_parse_ai_response_with_code_fences() {
        let response = "```json\n{\"namePattern\": \"*.txt\"}\n```";
        let q = parse_ai_response(response).unwrap();
        assert_eq!(q.name_pattern.as_deref(), Some("*.txt"));
    }

    #[test]
    fn test_parse_ai_response_invalid_json() {
        assert!(parse_ai_response("not json at all").is_err());
    }

    #[test]
    fn test_validate_regex_pattern_valid() {
        let q = AiSearchQuery {
            name_pattern: Some("[0-9]+\\.pdf".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
        };
        assert!(validate_regex_pattern(&q).is_ok());
    }

    #[test]
    fn test_validate_regex_pattern_invalid() {
        let q = AiSearchQuery {
            name_pattern: Some("[unclosed".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
        };
        assert!(validate_regex_pattern(&q).is_err());
    }

    #[test]
    fn test_validate_regex_pattern_glob_skips_validation() {
        let q = AiSearchQuery {
            name_pattern: Some("[unclosed".to_string()),
            pattern_type: Some("glob".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
        };
        // Glob patterns aren't validated as regex
        assert!(validate_regex_pattern(&q).is_ok());
    }

    #[test]
    fn test_validate_regex_pattern_none_skips_validation() {
        let q = AiSearchQuery {
            name_pattern: None,
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
        };
        assert!(validate_regex_pattern(&q).is_ok());
    }

    #[test]
    fn test_build_translate_result_with_regex() {
        let q = AiSearchQuery {
            name_pattern: Some("Q[1-4].*\\.pdf".to_string()),
            pattern_type: Some("regex".to_string()),
            min_size: None,
            max_size: None,
            modified_after: None,
            modified_before: None,
            is_directory: None,
        };
        let result = build_translate_result(q).unwrap();
        assert_eq!(result.query.pattern_type, "regex");
        assert_eq!(result.display.pattern_type.as_deref(), Some("regex"));
    }
}
