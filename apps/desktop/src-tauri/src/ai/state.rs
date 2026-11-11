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
    /// How many cancels were ever requested. A start that waits for a cancelled download to
    /// wind down gives up when this moves, because a cancel after it is the newer choice.
    pub(super) download_cancels: u64,
    /// True while the server is starting up (health check polling)
    pub(super) server_starting: bool,
    /// Cancels the in-flight startup health-check when the server is intentionally stopped
    /// or superseded, so a deliberate stop isn't reported as a startup failure.
    pub(super) start_cancel: Option<tokio_util::sync::CancellationToken>,
    /// AI provider mode: "off", "cloud", or "local"
    pub(super) provider: String,
    /// Context size for local llama-server
    pub(super) context_size: u32,
    /// The cloud provider preset id (`ai.cloudProvider`, e.g. `anthropic`): what the API key is
    /// stored under, and which model-list protocol and capability catalog apply
    /// (`model_capabilities`).
    pub(super) cloud_provider_id: String,
    /// Cloud-AI provider API key (stored here so suggestions.rs can read without settings files)
    pub(super) cloud_api_key: String,
    /// Cloud-AI provider base URL (e.g. `https://api.openai.com/v1`, `https://api.anthropic.com/v1/`)
    pub(super) cloud_base_url: String,
    /// Cloud-AI provider model name (e.g. `gpt-4o-mini`, `claude-sonnet-5-5`, `gemini-2.5-flash`)
    pub(super) cloud_model: String,
    /// Whether the selected cloud provider needs an API key. The frontend owns this fact
    /// (`requiresApiKey` per provider preset); keyless endpoints (Ollama, LM Studio, a custom
    /// OpenAI-compatible endpoint) set it `false` so an empty key isn't treated as "not configured".
    pub(super) cloud_requires_api_key: bool,
}

const STATE_FILENAME: &str = "ai-state.json";

/// The local llama-server context size before the frontend pushes the user's setting
/// (`ai.localContextSize`, whose own default matches this). It is also the floor Ask Cmdr
/// needs for one working turn (`agent::chat::budget::MIN_LOCAL_CONTEXT_TOKENS`): a smaller
/// window can't hold the prefix plus a paged tool result, so a turn against it is refused
/// rather than assembled.
const DEFAULT_CONTEXT_SIZE: u32 = crate::agent::chat::budget::MIN_LOCAL_CONTEXT_TOKENS;

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
        download_cancels: 0,
        server_starting: false,
        start_cancel: None,
        provider: String::from("local"),
        context_size: DEFAULT_CONTEXT_SIZE,
        cloud_provider_id: String::new(),
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

/// Returns the context size the local llama-server runs with (the `ai.localContextSize`
/// setting, pushed in by `configure_ai`), or the built-in default when the manager isn't up.
/// The agent's prompt budget derives from it, so a prompt can't overrun the local window.
pub fn get_local_context_size() -> u32 {
    let manager = MANAGER.lock_ignore_poison();
    manager.as_ref().map(|m| m.context_size).unwrap_or(DEFAULT_CONTEXT_SIZE)
}

/// Returns the cloud-AI config (api_key, base_url, model) stored in manager state.
pub fn get_cloud_config() -> (String, String, String) {
    let manager = MANAGER.lock_ignore_poison();
    manager
        .as_ref()
        .map(|m| (m.cloud_api_key.clone(), m.cloud_base_url.clone(), m.cloud_model.clone()))
        .unwrap_or_default()
}

/// The configured cloud provider preset id (`ai.cloudProvider`), empty before `configure_ai`.
pub fn get_cloud_provider_id() -> String {
    let manager = MANAGER.lock_ignore_poison();
    manager
        .as_ref()
        .map(|m| m.cloud_provider_id.clone())
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
        kv_bytes_per_token: model.kv_bytes_per_token,
        base_overhead_bytes: model.base_overhead_bytes,
    }
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

/// Reads `ai-state.json`. A missing file is the defaults; one that can't be read or parsed is set
/// aside first (`crate::state_recovery::read_state_file`), so the next [`save_state`] can't write
/// the defaults over the install record, the running server's pid, and a dismissal.
pub(super) fn load_state(ai_dir: &Path) -> AiState {
    let path = ai_dir.join(STATE_FILENAME);
    crate::state_recovery::read_state_file(&path, crate::state_recovery::StateFile::LocalAi, |bytes| {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    })
    .into_value()
    .unwrap_or_default()
}

/// Writes `ai-state.json` durably (temp + rename), never over a file that didn't load.
pub(super) fn save_state(ai_dir: &Path, state: &AiState) {
    let path = ai_dir.join(STATE_FILENAME);
    let written = serde_json::to_string_pretty(state)
        .map_err(|e| crate::state_recovery::StateWriteError::Io(std::io::Error::other(e)))
        .and_then(|json| crate::state_recovery::write_state_file(&path, &json));
    if let Err(e) = written {
        log::warn!(target: "ai", "Couldn't save the local AI state: {e}");
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

    /// An `ai-state.json` that's there but can't be read (mode 000) is set aside before the next
    /// state change writes the defaults over the install record and the dismissal.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_state_file_survives_the_next_save() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let _notices = crate::state_recovery::testing::capture_recoveries();
        let dir = crate::test_support::TestDir::new("ai_state_unreadable");
        let installed = AiState {
            installed: true,
            dismissed_until: Some(1_900_000_000),
            ..AiState::default()
        };
        save_state(&dir, &installed);
        let path = dir.join(STATE_FILENAME);
        let original = fs::read(&path).expect("the state exists");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("make it unreadable");

        let loaded = load_state(&dir);
        assert!(!loaded.installed);
        save_state(&dir, &loaded);

        let kept = crate::state_recovery::testing::kept_copies(&path);
        assert_eq!(kept.len(), 1, "set aside: {kept:?}");
        fs::set_permissions(&kept[0], fs::Permissions::from_mode(0o644)).expect("read the copy");
        assert_eq!(fs::read(&kept[0]).expect("read the copy"), original);
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
