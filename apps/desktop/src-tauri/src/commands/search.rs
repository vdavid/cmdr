//! IPC commands for drive search.
//!
//! Thin wrappers around `search` module functions, exposed to the frontend via Tauri commands.

use std::sync::atomic::Ordering;

use serde::Serialize;

use genai::chat::ChatOptions;

use crate::ai::AiTranslateError;
use crate::search::{self, ParsedScope, SearchQuery, SearchResult, VolumeLoad};

use crate::search::ai::{self, query_builder as ai_query_builder};
use crate::search::history::{self, HistoryEntry};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PrepareResult {
    pub ready: bool,
    pub entry_count: u64,
}

/// Emitted once the in-memory search index finishes loading, so the dialog can
/// flip from "loading" to ready and show the indexed entry count.
#[derive(Debug, Clone, serde::Deserialize, Serialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "search-index-ready")]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexReadyEvent {
    pub entry_count: u64,
}

/// Called when the search dialog opens. Pre-loads the ROOT index in the background
/// (the common case; scoped volumes load lazily on their first query). Returns
/// immediately with `{ ready, entryCount }`; the dialog flips to ready on the
/// emitted `search-index-ready` event.
#[tauri::command]
#[specta::specta]
pub async fn prepare_search_index(app: tauri::AppHandle) -> Result<PrepareResult, String> {
    use crate::indexing::ROOT_VOLUME_ID;

    search::touch_activity();
    search::DIALOG_OPEN.store(true, Ordering::Relaxed);
    search::cancel_idle_timer();

    // Fast path: root already warm and fresh.
    if let Some(v) = search::get_loaded(ROOT_VOLUME_ID) {
        // A prior session's backstop timer may still be ticking; reset it so it
        // can't fire while the dialog is open.
        search::reset_backstop_timer();
        return Ok(PrepareResult {
            ready: true,
            entry_count: v.index.entries.len() as u64,
        });
    }

    // Load root in the background so the dialog doesn't block on a multi-second scan.
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        match tokio::task::spawn_blocking(|| search::ensure_volume(ROOT_VOLUME_ID)).await {
            Ok(VolumeLoad::Loaded(v)) => {
                let entry_count = v.index.entries.len() as u64;
                use tauri_specta::Event;
                let _ = SearchIndexReadyEvent { entry_count }.emit(&app_clone);
            }
            Ok(VolumeLoad::NotIndexed) => log::debug!("prepare_search_index: root index not available yet"),
            Ok(VolumeLoad::Failed(e)) => log::warn!("prepare_search_index: root load failed: {e}"),
            Err(e) => log::warn!("prepare_search_index: load task panicked: {e}"),
        }
    });

    Ok(PrepareResult {
        ready: false,
        entry_count: 0,
    })
}

/// Search across the scoped volume(s), or every indexed volume when unscoped.
/// Returns empty (no coverage gaps) when nothing is indexed yet.
#[tauri::command]
#[specta::specta]
pub async fn search_files(query: SearchQuery) -> Result<SearchResult, String> {
    search::touch_activity();
    search::cancel_idle_timer();

    // Route + load + scan + merge on a blocking thread (opens DBs, rayon scan).
    tokio::task::spawn_blocking(move || search::run_blocking(query))
        .await
        .map_err(|e| format!("Search task failed: {e}"))?
}

/// Called when the search dialog closes. Starts the idle timer and cancels any
/// in-progress index load.
#[tauri::command]
#[specta::specta]
pub async fn release_search_index() -> Result<(), String> {
    search::DIALOG_OPEN.store(false, Ordering::Relaxed);
    search::cancel_active_loads();
    search::start_idle_timer();
    Ok(())
}

/// Parse a scope string into structured include/exclude data.
#[tauri::command]
#[specta::specta]
pub fn parse_search_scope(scope: String) -> ParsedScope {
    search::parse_scope(&scope)
}

/// Returns the list of system/build/cache directory names excluded by default,
/// for display in the UI tooltip.
#[tauri::command]
#[specta::specta]
pub fn get_system_dir_excludes() -> &'static [&'static str] {
    search::SYSTEM_DIR_EXCLUDES
}

// ============================================================================
// AI search query translation
// ============================================================================

/// Human-readable field values returned alongside the structured query.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub query: TranslatedQuery,
    pub display: TranslateDisplay,
    pub caveat: Option<String>,
    /// Short, breadcrumb-friendly title for this search (max 40 chars, sentence
    /// case). The LLM produces it; the frontend stores it on the snapshot and
    /// renders it in the search-results pane breadcrumb. `None` when the LLM
    /// omitted the label or the fallback path ran (raw-keywords retry); the
    /// frontend falls back to the original natural-language prompt.
    pub label: Option<String>,
}

/// The structured query with unix timestamps, ready for `search_files`.
#[derive(Debug, Clone, Serialize, specta::Type)]
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
#[derive(Debug, Clone, Serialize, specta::Type)]
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

/// Translates a natural language search query into structured filters using the configured LLM.
///
/// Single-pass flow: call LLM with classification prompt → parse key-value response →
/// build deterministic SearchQuery via `ai_query_builder`.
/// `current_type` is the dialog's `Both | Files | Folders` toggle as context (`Some(true)` =
/// folders, `Some(false)` = files, `None` = both). The model maps it to the `folders: yes|no`
/// field, or omits the field to keep the user's current choice. First step toward the
/// "agent sees app state" model; structured to grow into the full filter set later.
#[tauri::command]
#[specta::specta]
pub async fn translate_search_query(
    natural_query: String,
    current_type: Option<bool>,
) -> Result<TranslateResult, AiTranslateError> {
    let backend = crate::ai::manager::resolve_translate_backend(false)?
        .with_log_context(crate::ai::llm_log::LlmLogContext::translate_search());
    let system_prompt = ai::build_classification_prompt(current_type);

    log::debug!(
        "AI search: classification prompt ({} chars), query={natural_query:?}",
        system_prompt.len()
    );

    // 300 tokens (not 200): reasoning models spend the budget thinking before any visible
    // answer, so a tight cap returns an empty response. See ai/CLAUDE.md § reasoning-model
    // token budget.
    let options = ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(300)
        .with_top_p(0.9);

    let response =
        crate::ai::translate::translate_once(&backend, &system_prompt, &natural_query, &options, "AI search").await?;

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
    let label = ai_query_builder::build_label(&parsed);
    let translated_query = ai_query_builder::build_translated_query(&query);

    Ok(TranslateResult {
        query: translated_query,
        display,
        caveat,
        label,
    })
}

// ============================================================================
// Recent searches (history) IPC
// ============================================================================

/// Reads the latest persisted recent-searches entries. `limit = None` returns all.
#[tauri::command]
#[specta::specta]
pub fn get_recent_searches(limit: Option<u32>) -> Vec<HistoryEntry> {
    history::list_entries(limit.map(|n| n as usize))
}

/// Adds a recent-search entry. Dedupes against existing entries by canonical key,
/// moves the matching one to the top, and trims to `max_count`.
#[tauri::command]
#[specta::specta]
pub fn add_recent_search(app: tauri::AppHandle, entry: HistoryEntry, max_count: Option<u32>) -> Result<(), String> {
    let cap = max_count.map(|n| n as usize).unwrap_or_else(history::default_max_count);
    history::add_entry(&app, entry, cap);
    Ok(())
}

/// Removes a recent-search entry by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub fn remove_recent_search(app: tauri::AppHandle, id: String) -> Result<(), String> {
    history::remove_entry(&app, &id);
    Ok(())
}

/// Clears every recent-search entry.
#[tauri::command]
#[specta::specta]
pub fn clear_recent_searches(app: tauri::AppHandle) -> Result<(), String> {
    history::clear_entries(&app);
    Ok(())
}

/// Live-applies a new `search.recentSearches.maxCount` value. Trims the in-memory
/// store and rewrites disk only when entries actually drop.
#[tauri::command]
#[specta::specta]
pub fn apply_recent_searches_max_count(app: tauri::AppHandle, max_count: u32) -> Result<(), String> {
    history::apply_max_count(&app, max_count as usize);
    Ok(())
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
            label: Some("Big PDFs from 2025".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("namePattern"));
        assert!(json.contains("patternType"));
        assert!(json.contains("2025-01-01"));
        assert!(json.contains("\"label\":\"Big PDFs from 2025\""));
    }
}
