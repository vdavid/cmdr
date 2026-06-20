//! The shared AI-manager singleton, its on-disk persistence, and derived model facts.
//!
//! [`ManagerState`] is the one mutable struct the AI subsystem coordinates around; it
//! lives behind the [`MANAGER`] global `Mutex`. Owning it (and the lock) in a single
//! module keeps the shared state coherent: the sibling concern modules (`install`,
//! `server`, `manager`) borrow `&mut ManagerState` through the lock rather than each
//! holding their own copy. This module also owns reading/writing `ai-state.json` and
//! the install-status / model-info facts derived from disk.

use super::extract::{LLAMA_SERVER_BINARY, REQUIRED_DYLIB};
use super::{AiState, ModelInfo, get_default_model, get_model_by_id};
use crate::ignore_poison::IgnorePoison;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime};

/// Global manager state, accessible from Tauri commands.
pub(super) static MANAGER: Mutex<Option<ManagerState>> = Mutex::new(None);

pub(super) struct ManagerState {
    pub(super) ai_dir: PathBuf,
    pub(super) state: AiState,
    /// PID of the running llama-server process
    pub(super) child_pid: Option<u32>,
    /// Flag to cancel an in-progress download
    pub(super) cancel_requested: bool,
    /// Flag to prevent multiple concurrent downloads
    pub(super) download_in_progress: bool,
    /// True while the server is starting up (health check polling)
    pub(super) server_starting: bool,
    /// Cancels the in-flight startup health-check when the server is intentionally stopped
    /// or superseded, so a deliberate stop isn't reported as a startup failure.
    pub(super) start_cancel: Option<tokio_util::sync::CancellationToken>,
    /// AI provider mode: "off", "cloud", or "local"
    pub(super) provider: String,
    /// Context size for local llama-server
    pub(super) context_size: u32,
    /// Cloud-AI provider API key (stored here so suggestions.rs can read without settings files)
    pub(super) cloud_api_key: String,
    /// Cloud-AI provider base URL (e.g. `https://api.openai.com/v1`, `https://api.anthropic.com/v1/`)
    pub(super) cloud_base_url: String,
    /// Cloud-AI provider model name (e.g. `gpt-4o-mini`, `claude-sonnet-4-5`, `gemini-2.5-flash`)
    pub(super) cloud_model: String,
    /// Whether the selected cloud provider needs an API key. The frontend owns this fact
    /// (`requiresApiKey` per provider preset); keyless endpoints (Ollama, LM Studio, a custom
    /// OpenAI-compatible endpoint) set it `false` so an empty key isn't treated as "not configured".
    pub(super) cloud_requires_api_key: bool,
}

const STATE_FILENAME: &str = "ai-state.json";

/// Builds a fresh `ManagerState` for `ai_dir` with the given persisted `state` and the
/// default in-memory config (provider `local`, default cloud endpoint). `init` mutates
/// it further (stale-PID cleanup) before storing it in `MANAGER`.
pub(super) fn new_manager_state(ai_dir: PathBuf, state: AiState) -> ManagerState {
    ManagerState {
        ai_dir,
        state,
        child_pid: None,
        cancel_requested: false,
        download_in_progress: false,
        server_starting: false,
        start_cancel: None,
        provider: String::from("local"),
        context_size: 4096,
        cloud_api_key: String::new(),
        cloud_base_url: String::from("https://api.openai.com/v1"),
        cloud_model: String::from("gpt-4o-mini"),
        cloud_requires_api_key: false,
    }
}

/// Returns the port the llama-server is listening on, if running.
pub fn get_port() -> Option<u16> {
    let manager = MANAGER.lock_ignore_poison();
    manager.as_ref().and_then(|m| m.state.port)
}

/// Returns the current AI provider stored in manager state.
pub fn get_provider() -> String {
    let manager = MANAGER.lock_ignore_poison();
    manager
        .as_ref()
        .map(|m| m.provider.clone())
        .unwrap_or_else(|| String::from("off"))
}

/// Returns the cloud-AI config (api_key, base_url, model) stored in manager state.
pub fn get_cloud_config() -> (String, String, String) {
    let manager = MANAGER.lock_ignore_poison();
    manager
        .as_ref()
        .map(|m| (m.cloud_api_key.clone(), m.cloud_base_url.clone(), m.cloud_model.clone()))
        .unwrap_or_default()
}

/// Whether the configured cloud provider needs an API key (frontend-owned `requiresApiKey`).
pub fn get_cloud_requires_api_key() -> bool {
    let manager = MANAGER.lock_ignore_poison();
    manager.as_ref().map(|m| m.cloud_requires_api_key).unwrap_or(false)
}

/// Model info returned to frontend.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiModelInfo {
    pub id: String,
    pub display_name: String,
    pub size_bytes: u64,
    /// Human-readable size (like "4.3 GB")
    pub size_formatted: String,
    /// Bytes per token for KV cache (used for memory estimation)
    pub kv_bytes_per_token: u64,
    /// Base memory overhead in bytes (model weights + compute buffers)
    pub base_overhead_bytes: u64,
}

/// Returns information about the current AI model.
#[tauri::command]
#[specta::specta]
pub fn get_ai_model_info() -> AiModelInfo {
    let model = get_current_model();
    AiModelInfo {
        id: model.id.to_string(),
        display_name: model.display_name.to_string(),
        size_bytes: model.size_bytes,
        size_formatted: format_bytes_gb(model.size_bytes),
        kv_bytes_per_token: model.kv_bytes_per_token,
        base_overhead_bytes: model.base_overhead_bytes,
    }
}

/// Formats bytes as GB with one decimal place (like "4.3 GB").
pub(super) fn format_bytes_gb(bytes: u64) -> String {
    let gb = bytes as f64 / 1_000_000_000.0;
    format!("{gb:.1} GB")
}

/// Returns the model info for the currently selected/installed model.
/// Falls back to default if the stored model ID is not in the registry.
pub(super) fn get_current_model() -> &'static ModelInfo {
    let manager = MANAGER.lock_ignore_poison();
    if let Some(ref m) = *manager
        && let Some(model) = get_model_by_id(&m.state.installed_model_id)
    {
        return model;
    }
    get_default_model()
}

pub(super) fn get_ai_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    crate::config::resolved_app_data_dir(app)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("ai")
}

pub(super) fn load_state(ai_dir: &Path) -> AiState {
    let path = ai_dir.join(STATE_FILENAME);
    fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

pub(super) fn save_state(ai_dir: &Path, state: &AiState) {
    let _ = fs::create_dir_all(ai_dir);
    let path = ai_dir.join(STATE_FILENAME);
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json);
    }
}

/// Returns true if AI is fully installed and ready to run.
/// Requires binary, model, AND shared libraries to exist.
pub(super) fn is_fully_installed(m: &ManagerState) -> bool {
    let binary_exists = m.ai_dir.join(LLAMA_SERVER_BINARY).exists();
    let dylib_exists = m.ai_dir.join(REQUIRED_DYLIB).exists();

    // Get model info based on installed model ID
    let model = get_model_by_id(&m.state.installed_model_id).unwrap_or_else(get_default_model);
    let model_path = m.ai_dir.join(model.filename);
    let model_exists = model_path.exists();

    if !binary_exists || !dylib_exists {
        if binary_exists && !dylib_exists {
            log::debug!("AI: binary exists but shared libraries missing, need re-extraction");
        }
        return false;
    }

    // Model must exist AND be verified complete (not a partial download)
    let model_complete = model_exists && m.state.model_download_complete;

    if model_exists && !m.state.model_download_complete {
        // Double-check by file size in case state is stale
        if let Ok(meta) = fs::metadata(&model_path)
            && meta.len() >= model.size_bytes
        {
            log::debug!("AI: model file size matches expected, marking as complete");
            return true; // Binary, dylibs, and model all present
        }
        log::debug!("AI: model file exists but download not verified complete");
    }

    model_complete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ai_state() {
        let state = AiState::default();
        assert!(!state.installed);
        assert_eq!(state.port, None);
        assert_eq!(state.pid, None);
        assert_eq!(state.installed_model_id, "ministral-3b-instruct-q4km");
        assert_eq!(state.dismissed_until, None);
    }

    #[test]
    fn test_state_serialization() {
        let state = AiState {
            installed: true,
            port: Some(52847),
            pid: Some(12345),
            installed_model_id: String::from("ministral-3b-instruct-q4km"),
            dismissed_until: None,
            model_download_complete: true,
            partial_download_started: None,
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: AiState = serde_json::from_str(&json).unwrap();
        assert!(parsed.installed);
        assert_eq!(parsed.port, Some(52847));
        assert_eq!(parsed.pid, Some(12345));
        assert!(parsed.model_download_complete);
    }
}
