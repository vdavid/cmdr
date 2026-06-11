//! IPC commands for the Selection dialog.
//!
//! Thin wrappers around `crate::selection`. The selection AI translation is
//! cloud-only: small local models can't reliably handle a 200+-name folder sample
//! plus the structured prompt and response. `translate_selection_query` returns a
//! hard error when the configured provider isn't `cloud` so the UI can toast the
//! reason; the frontend hides the AI chip when the provider isn't cloud so we
//! never reach this path in normal use.

use genai::chat::ChatOptions;

use crate::ai::client::AiBackend;
use crate::ai::manager::BackendResolution;
use crate::ai::{AiTranslateError, AiTranslateErrorKind};

use crate::selection::ai::{self, SelectionTranslateResult, query_builder};
use crate::selection::history::{self, SelectionHistoryEntry};

/// Resolves the AI backend, requiring a cloud provider.
///
/// Mirrors `commands::search::resolve_ai_backend` but adds the cloud-only gate. The
/// frontend hides the AI chip when `ai.provider !== 'cloud'`; this gate is the
/// belt-and-braces check so a misconfigured frontend (or an MCP caller in the
/// future) can't drive the local model with a prompt it can't handle.
fn resolve_cloud_ai_backend() -> Result<AiBackend, AiTranslateError> {
    use AiTranslateErrorKind as K;
    let provider = crate::ai::manager::get_provider();
    if provider != "cloud" {
        return Err(AiTranslateError::new(
            K::NotConfigured,
            "AI selection needs a cloud provider. Set one in Settings > AI.",
        ));
    }
    match crate::ai::manager::resolve_backend() {
        BackendResolution::Ready(b) => Ok(b),
        BackendResolution::Off => Err(AiTranslateError::new(
            K::Off,
            "AI is not configured. Enable a cloud provider in settings.",
        )),
        BackendResolution::NotConfigured(reason) => Err(AiTranslateError::new(K::NotConfigured, reason)),
        BackendResolution::UnknownProvider(p) => Err(AiTranslateError::new(
            K::UnknownProvider,
            format!("Unknown AI provider: {p}"),
        )),
    }
}

/// Translates a natural-language selection request into a glob/regex plus optional
/// size and date filters.
///
/// The `sample_names` argument is the focused folder's filename listing (already
/// sampled on the frontend; see `lib/selection-dialog/folder-sampler.ts` for the
/// sampling strategy). It grounds the prompt in what's actually in the folder.
///
/// `current_type` is the dialog's current `Both | Files | Folders` toggle as context
/// (`Some(true)` = folders, `Some(false)` = files, `None` = both). The model may change
/// it or leave it; when it returns no `type`, the frontend keeps the user's choice. This
/// is the first step toward the "agent sees app state" model: structured to grow into the
/// broader current filter set later without another IPC reshape.
#[tauri::command]
#[specta::specta]
pub async fn translate_selection_query(
    prompt: String,
    sample_names: Vec<String>,
    current_type: Option<bool>,
) -> Result<SelectionTranslateResult, AiTranslateError> {
    let backend = resolve_cloud_ai_backend()?;
    let system_prompt = ai::build_classification_prompt(&sample_names, current_type);

    log::debug!(
        target: "selection::ai",
        "translate_selection_query: prompt={prompt:?}, sample_count={}, system_prompt_chars={}",
        sample_names.len(),
        system_prompt.len()
    );

    let options = ChatOptions::default()
        .with_temperature(0.2)
        .with_max_tokens(300)
        .with_top_p(0.9);

    let t0 = std::time::Instant::now();
    let response = crate::ai::client::chat_completion_with_empty_retry(&backend, &system_prompt, &prompt, &options)
        .await
        .map_err(|e| {
            log::warn!(
                target: "selection::ai",
                "chat_completion failed after {:.1}s for prompt={prompt:?}: {e}",
                t0.elapsed().as_secs_f64()
            );
            AiTranslateError::from(e)
        })?;

    log::info!(
        target: "selection::ai",
        "translate_selection_query: response {} chars in {:.1}s",
        response.len(),
        t0.elapsed().as_secs_f64()
    );
    log::debug!(target: "selection::ai", "translate_selection_query raw response: {response:?}");

    let parsed = ai::parse_selection_response(&response);
    Ok(query_builder::build_selection_translate_result(&parsed))
}

// ============================================================================
// Recent selections (history) IPC
// ============================================================================

/// Returns the persisted recent-selections entries (newest first). `limit = None`
/// returns all.
#[tauri::command]
#[specta::specta]
pub fn get_recent_selections(limit: Option<u32>) -> Vec<SelectionHistoryEntry> {
    history::list_entries(limit.map(|n| n as usize))
}

/// Adds a recent-selection entry. Dedupes against existing entries by canonical
/// key, moves the matching one to the top, and trims to `max_count`.
#[tauri::command]
#[specta::specta]
pub fn add_recent_selection(
    app: tauri::AppHandle,
    entry: SelectionHistoryEntry,
    max_count: Option<u32>,
) -> Result<(), String> {
    let cap = max_count.map(|n| n as usize).unwrap_or_else(history::default_max_count);
    history::add_entry(&app, entry, cap);
    Ok(())
}

/// Removes a recent-selection entry by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub fn remove_recent_selection(app: tauri::AppHandle, id: String) -> Result<(), String> {
    history::remove_entry(&app, &id);
    Ok(())
}

/// Clears every recent-selection entry.
#[tauri::command]
#[specta::specta]
pub fn clear_recent_selections(app: tauri::AppHandle) -> Result<(), String> {
    history::clear_entries(&app);
    Ok(())
}

/// Live-applies a new `selection.recentSelections.maxCount` value. Trims the
/// in-memory store and rewrites disk only when entries actually drop.
#[tauri::command]
#[specta::specta]
pub fn apply_recent_selections_max_count(app: tauri::AppHandle, max_count: u32) -> Result<(), String> {
    history::apply_max_count(&app, max_count as usize);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_result_serialization_round_trips() {
        let r = SelectionTranslateResult {
            pattern: Some("*.log".to_string()),
            kind: Some("glob".to_string()),
            is_directory: Some(false),
            size_min: Some(1024),
            size_max: None,
            modified_after: Some("2026-01-01".to_string()),
            modified_before: None,
            caveat: None,
            label: Some("Log files".to_string()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"pattern\":\"*.log\""));
        assert!(json.contains("\"isDirectory\":false"));
        assert!(json.contains("\"sizeMin\":1024"));
        assert!(json.contains("\"modifiedAfter\":\"2026-01-01\""));
        assert!(json.contains("\"label\":\"Log files\""));
    }
}
