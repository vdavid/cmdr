//! IPC commands for drive search.
//!
//! Thin wrappers around `indexing::search` module functions, exposed to the frontend via Tauri commands.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::ai::client::{AiBackend, ChatCompletionOptions};
use crate::indexing::get_read_pool;
use crate::indexing::search::{
    self, DIALOG_OPEN, ParsedScope, SEARCH_INDEX, SearchIndexState, SearchQuery, SearchResult, drop_search_index,
    fill_directory_sizes, start_backstop_timer, start_idle_timer, touch_activity,
};

use super::ai_query_builder;
use super::ai_response_parser;
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
            index: Arc::new(search::SearchIndex::empty()),
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
pub async fn search_files(mut query: SearchQuery) -> Result<SearchResult, String> {
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

    // Resolve include paths to entry IDs via SQLite (microseconds, not 20s)
    if query.include_paths.as_ref().is_some_and(|p| !p.is_empty())
        && let Some(pool) = get_read_pool()
    {
        search::resolve_include_paths(&mut query, &pool);
    }

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

/// Parse a scope string into structured include/exclude data.
#[tauri::command]
pub fn parse_search_scope(scope: String) -> ParsedScope {
    search::parse_scope(&scope)
}

/// Returns the list of system/build/cache directory names excluded by default,
/// for display in the UI tooltip.
#[tauri::command]
pub fn get_system_dir_excludes() -> &'static [&'static str] {
    search::SYSTEM_DIR_EXCLUDES
}

// ============================================================================
// AI search query translation
// ============================================================================

/// Human-readable field values returned alongside the structured query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub query: TranslatedQuery,
    pub display: TranslateDisplay,
    pub caveat: Option<String>,
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
    pub include_paths: Option<Vec<String>>,
    pub exclude_dir_names: Option<Vec<String>>,
    pub case_sensitive: Option<bool>,
    pub exclude_system_dirs: Option<bool>,
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
    pub include_paths: Option<Vec<String>>,
    pub exclude_dir_names: Option<Vec<String>>,
    pub case_sensitive: Option<bool>,
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

// ── AI search classification prompt ──────────────────────────────────

/// Classification prompt for the LLM. The LLM classifies intent into predefined
/// enums and extracts filename keywords. Rust handles all structural/technical work.
/// `{TODAY}` is replaced at runtime.
const CLASSIFICATION_PROMPT: &str = "\
Extract search parameters from the user's file search query.
Return one field per line. Omit fields that don't apply.

keywords:  filename words, space-separated, in the user's language
type:      photos|screenshots|videos|documents|presentations|archives|music|\
code|rust|python|javascript|typescript|go|java|config|logs|fonts|\
databases|xcode|shell-scripts|ssh-keys|docker-compose|env-files|none
time:      today|yesterday|this_week|last_week|this_month|last_month|\
this_quarter|last_quarter|this_year|last_year|last_3_months|last_6_months|\
recent|old|YYYY|YYYY..YYYY
size:      empty|tiny|small|large|huge|>NUMBERmb|>NUMBERgb|<NUMBERmb
scope:     downloads|documents|desktop|dotfiles|PATH
exclude:   dirname1 dirname2
folders:   yes|no
note:      brief limitation caveat if query involves unfilterable concepts

Rules:
- \"keywords\" = words likely in FILENAMES. Not descriptions.
- Use singular forms for keywords (contract, not contracts).
- \"I name them X\" / \"I mark them as X\" → keywords: X (not the descriptive words)
- Only set `time` when the user explicitly mentions a time period (yesterday, last week, recent, 2024, etc.). Never default to recent/today.
- Prefer `type` over `keywords` for well-known file categories. Don't put the type name in keywords.
- Don't put the file format in keywords when using a type. \"PDF documents\" → type: documents. \"sqlite databases\" → type: databases.
- If the user wants ONLY a specific format (not all files of that category), use the format as keyword without type: \"HEIC photos I haven't converted\" → keywords: .heic / note: can't determine conversion status
- \"not in X\" / \"but not in X\" / \"excluding X\" / \"except in X\" → ALWAYS use exclude: X
- \"ssh keys\"/\"env files\"/\"docker compose\"/\"shell scripts\" → type handles this, no keywords needed
- For content/semantic queries (\"photos of my cat\"), set type + add a note

Examples:
\"recent invoices, I mark them rymd\" → keywords: rymd / type: documents / time: recent
\"\u{5927}\u{304d}\u{306a}\u{52d5}\u{753b}\u{3092}\u{524a}\u{9664}\u{3057}\u{305f}\u{3044}\" → type: videos / size: large / note: can't determine safe to delete
\"node_modules folders taking up space\" → keywords: node_modules / folders: yes / size: large
\"screenshots from this week\" → type: screenshots / time: this_week
\"package.json not in node_modules\" → keywords: package.json / exclude: node_modules
\"empty folders\" → folders: yes / size: empty
\"ssh keys\" → type: ssh-keys
\"foton fr\u{00e5}n f\u{00f6}rra veckan\" → type: photos / time: last_week
\"that rust file with the websocket server\" → keywords: websocket / type: rust
\"old xcode projects\" → type: xcode / time: old
\"contracts I signed in the last 6 months\" → keywords: contract / type: documents / time: last_6_months / note: \"signed\" is not filterable
\"shell scripts in my dotfiles\" → type: shell-scripts / scope: dotfiles
\"HEIC photos I haven't converted\" → keywords: .heic / note: can't determine conversion status

Today: {TODAY}.";

fn build_classification_prompt() -> String {
    let today = time::OffsetDateTime::now_utc().date();
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let today_str = today.format(&format).expect("date format always succeeds");
    CLASSIFICATION_PROMPT.replace("{TODAY}", &today_str)
}

/// Resolves the AI backend from the current provider configuration.
fn resolve_ai_backend() -> Result<AiBackend, String> {
    let provider = crate::ai::manager::get_provider();
    match provider.as_str() {
        "off" => Err("AI is not configured. Enable an AI provider in settings.".to_string()),
        "local" => {
            let port = crate::ai::manager::get_port()
                .ok_or_else(|| "Local AI server isn't running. Start it in settings.".to_string())?;
            Ok(AiBackend::Local { port })
        }
        "openai-compatible" => {
            let (api_key, base_url, model) = crate::ai::manager::get_openai_config();
            if api_key.is_empty() {
                return Err("OpenAI API key not configured. Add it in settings.".to_string());
            }
            Ok(AiBackend::OpenAi {
                api_key,
                base_url,
                model,
            })
        }
        _ => Err(format!("Unknown AI provider: {provider}")),
    }
}

/// Translates a natural language search query into structured filters using the configured LLM.
///
/// Single-pass flow: call LLM with classification prompt → parse key-value response →
/// build deterministic SearchQuery via `ai_query_builder`.
#[tauri::command]
pub async fn translate_search_query(natural_query: String) -> Result<TranslateResult, String> {
    let backend = resolve_ai_backend()?;
    let system_prompt = build_classification_prompt();

    log::debug!(
        "AI search: classification prompt ({} chars), query={natural_query:?}",
        system_prompt.len()
    );

    let options = ChatCompletionOptions {
        system_prompt,
        temperature: 0.3,
        max_tokens: 200,
        top_p: 0.9,
    };

    let t0 = std::time::Instant::now();
    let response = crate::ai::client::chat_completion(&backend, &natural_query, &options)
        .await
        .map_err(|e| {
            log::warn!(
                "AI search: chat_completion failed after {:.1}s for query={natural_query:?}: {e}",
                t0.elapsed().as_secs_f64()
            );
            format!("{e}")
        })?;

    log::info!(
        "AI search: chat_completion returned {} chars in {:.1}s",
        response.len(),
        t0.elapsed().as_secs_f64()
    );
    log::debug!("AI search: raw response: {response:?}");

    // Parse key-value response
    let parsed = ai_response_parser::parse_llm_response(&response);

    // Fallback: if parser returned nothing useful, use raw query keywords
    let parsed = if parsed.is_empty() {
        log::info!("AI search: LLM returned empty/garbage response, falling back to raw keywords");
        let fallback_kw = ai_response_parser::fallback_keywords(&natural_query);
        if fallback_kw.is_empty() {
            parsed
        } else {
            ai_response_parser::ParsedLlmResponse {
                keywords: Some(fallback_kw),
                ..Default::default()
            }
        }
    } else {
        parsed
    };

    // Build deterministic query
    let query = ai_query_builder::build_search_query(&parsed);
    let display = ai_query_builder::build_translate_display(&parsed, &query);
    let caveat = ai_query_builder::generate_caveat(&parsed, &query);
    let translated_query = ai_query_builder::build_translated_query(&query);

    Ok(TranslateResult {
        query: translated_query,
        display,
        caveat,
    })
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
                include_paths: None,
                exclude_dir_names: None,
                case_sensitive: None,
                exclude_system_dirs: None,
            },
            display: TranslateDisplay {
                name_pattern: Some("*.pdf".to_string()),
                pattern_type: Some("glob".to_string()),
                min_size: Some(1_048_576),
                max_size: None,
                modified_after: Some("2025-01-01".to_string()),
                modified_before: None,
                is_directory: None,
                include_paths: None,
                exclude_dir_names: None,
                case_sensitive: None,
            },
            caveat: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("2025-01-01"));
    }

    #[test]
    fn test_classification_prompt_contains_date() {
        let prompt = build_classification_prompt();
        assert!(prompt.contains("Today:"));
        assert!(prompt.contains("Extract search parameters"));
        // Should contain a date in YYYY-MM-DD format
        assert!(prompt.contains("20")); // Year starts with 20
    }

    #[test]
    fn test_classification_prompt_contains_type_enums() {
        let prompt = build_classification_prompt();
        assert!(prompt.contains("photos|screenshots|videos"));
        assert!(prompt.contains("shell-scripts|ssh-keys|docker-compose|env-files"));
    }

    #[test]
    fn test_classification_prompt_contains_time_enums() {
        let prompt = build_classification_prompt();
        assert!(prompt.contains("last_3_months|last_6_months"));
    }
}
