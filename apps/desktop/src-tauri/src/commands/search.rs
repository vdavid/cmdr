//! IPC commands for drive search.
//!
//! Thin wrappers around `search` module functions, exposed to the frontend via Tauri commands.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::ai::client::{AiBackend, ChatCompletionOptions};
use crate::indexing::get_read_pool;
use crate::search::{
    self, DIALOG_OPEN, ParsedScope, SEARCH_INDEX, SearchIndexState, SearchQuery, SearchResult, drop_search_index,
    fill_directory_sizes, start_backstop_timer, start_idle_timer, touch_activity,
};

use crate::indexing::writer::WRITER_GENERATION;
use crate::search::ai::{self, query_builder as ai_query_builder};

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
    let system_prompt = ai::build_classification_prompt();

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
    let parsed = ai::parse_llm_response(&response);

    // Fallback: if parser returned nothing useful, use raw query keywords
    let parsed = if parsed.is_empty() {
        log::info!("AI search: LLM returned empty/garbage response, falling back to raw keywords");
        let fallback_kw = ai::fallback_keywords(&natural_query);
        if fallback_kw.is_empty() {
            parsed
        } else {
            ai::ParsedLlmResponse {
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
}
