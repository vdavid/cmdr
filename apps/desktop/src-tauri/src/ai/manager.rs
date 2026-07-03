//! AI manager facade: lifecycle, status, provider config, and backend resolution.
//!
//! The thin coordinator over the AI subsystem's concern modules. It owns the
//! cross-cutting commands (`init` / `shutdown`, `get_ai_status`, `configure_ai`,
//! `get_ai_runtime_status`) and the provider-routing decision (`resolve_backend`),
//! and delegates the rest:
//!
//! - shared state + persistence + model info → [`super::state`]
//! - model download / uninstall → [`super::install`]
//! - llama-server process lifecycle → [`super::server`]
//! - cloud-endpoint probing + URL safety → [`super::connection_check`]
//! - streaming-cancellation registry → [`super::stream_registry`]
//!
//! Each concern module's Tauri commands are registered directly from there (`ipc.rs`):
//! the `#[tauri::command]` macro's generated helper items don't survive a `pub use`, so
//! re-exporting wouldn't make the IPC path work. The handful of plain-fn callers that
//! still reach in via `ai::manager::…` (`get_provider`, the stream-cancel registry) are
//! covered by the re-exports just below.
//!
//! Uses `is_local_ai_supported()` to gate local-only operations (requires Apple Silicon).

use super::process::{kill_process, kill_stale_llama_servers};
use super::state::{
    MANAGER, get_ai_dir, get_cloud_config, get_current_model, get_port, is_fully_installed, load_state,
    new_manager_state, save_state,
};
use super::{AiStarting, AiStatus, AiTranslateError, AiTranslateErrorKind, is_local_ai_supported};
use crate::ignore_poison::IgnorePoison;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;

// --- Re-exports keeping the `ai::manager::…` path stable for the facade's non-command
// callers. The Tauri command fns live in their concern modules and are registered from
// there (`ipc.rs`), because the `#[tauri::command]` macro's generated helper items don't
// travel through a `pub use`. These plain re-exports cover the rest:
// `get_provider` (commands/selection.rs) and the stream-cancellation registry (suggestions.rs).

pub(super) use super::install::cleanup_stale_partial_download;
pub(super) use super::server::{handle_startup_outcome, spawn_and_track_server, wait_for_server_health};
pub use super::state::get_provider;
pub use super::stream_registry::cancel_stream;
pub(super) use super::stream_registry::{register_stream, unregister_stream};

/// Initializes the AI manager. Called once on app startup.
///
/// Only sets up directories and cleans stale PIDs. Does NOT start the server.
/// Server start is triggered later by `configure_ai` when the frontend pushes settings.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let ai_dir = get_ai_dir(app);
    let state = load_state(&ai_dir);

    let mut manager = MANAGER.lock_ignore_poison();
    *manager = Some(new_manager_state(ai_dir, state));

    // Belt-and-suspenders: stop ALL llama-server processes from our AI directory,
    // not just the tracked PID. Catches orphans from race conditions or crashes.
    if let Some(ref m) = *manager {
        kill_stale_llama_servers(&m.ai_dir);
    }

    // Clean up tracked PID from a previous session
    if let Some(ref mut m) = *manager
        && m.state.pid.is_some()
    {
        m.state.pid = None;
        m.state.port = None;
        save_state(&m.ai_dir, &m.state);
    }

    // Clean up stale partial downloads (older than 24 hours)
    if let Some(ref mut m) = *manager {
        cleanup_stale_partial_download(m);
    }

    log::debug!("AI manager: initialized (server start deferred until configure_ai)");
}

/// Shuts down the AI server. Called on app quit.
/// Fire-and-forget SIGKILL; the app is exiting so no need to reap the zombie.
pub fn shutdown() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        if let Some(token) = m.start_cancel.take() {
            token.cancel();
        }
        if let Some(pid) = m.child_pid.take() {
            kill_process(pid);
        }
    }
}

/// Returns the current AI status.
#[tauri::command]
#[specta::specta]
pub fn get_ai_status() -> AiStatus {
    let manager = MANAGER.lock_ignore_poison();
    let Some(m) = manager.as_ref() else {
        return AiStatus::Unavailable;
    };
    compute_ai_status(
        &m.provider,
        m.state.installed,
        m.child_pid.is_some(),
        m.state.dismissed_until,
        is_local_ai_supported(),
        current_unix_seconds(),
    )
}

/// Pure decision function for [`get_ai_status`]. Split out so the global `MANAGER` lock
/// and the compile-time `cfg!(target_arch)` gate don't have to participate in tests.
fn compute_ai_status(
    provider: &str,
    installed: bool,
    server_running: bool,
    dismissed_until: Option<u64>,
    local_ai_supported: bool,
    now_secs: u64,
) -> AiStatus {
    if provider == "off" {
        return AiStatus::Unavailable;
    }
    if installed && server_running {
        return AiStatus::Available;
    }
    if installed {
        return AiStatus::Unavailable; // installed but server not running
    }
    // Not installed. Only offer the local-model download if the hardware can run it;
    // otherwise the user sees the toast, clicks Download, and only then discovers
    // `start_ai_download` rejects with "Local AI not supported on this hardware".
    // Cloud AI is unaffected: the frontend short-circuits this status path when
    // `ai.provider === "cloud"` (see `ai-state.svelte.ts::initAiState`).
    if !local_ai_supported {
        return AiStatus::Unavailable;
    }
    if let Some(until) = dismissed_until
        && now_secs < until
    {
        return AiStatus::Unavailable;
    }
    AiStatus::Offer
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Resolves the configured AI provider into either a ready-to-use [`AiBackend`] or
/// a reason why one couldn't be built.
///
/// Centralizes the provider-routing logic so callers (`suggestions.rs`,
/// `commands/search.rs`) just match on the variants and decide whether the case
/// should be hard-error or graceful-empty.
pub enum BackendResolution {
    /// `provider = "off"`: AI features are turned off.
    Off,
    /// Provider is set but missing config (e.g. local server not running, cloud key blank).
    /// Includes a human-readable reason suitable for error toasts.
    NotConfigured(&'static str),
    /// Backend is ready to use.
    Ready(super::client::AiBackend),
    /// Provider value isn't recognized.
    UnknownProvider(String),
}

pub fn resolve_backend() -> BackendResolution {
    let (api_key, base_url, model) = get_cloud_config();
    resolve_backend_inner(
        &get_provider(),
        get_port(),
        api_key,
        base_url,
        model,
        super::state::get_cloud_requires_api_key(),
    )
}

impl BackendResolution {
    /// Maps a resolution onto the translate-command surface, where every
    /// non-ready case is a typed [`AiTranslateError`] the dialog toasts (the
    /// `kind` is what the frontend branches on). Used via
    /// [`resolve_translate_backend`].
    pub fn into_translate_result(self) -> Result<super::client::AiBackend, AiTranslateError> {
        use AiTranslateErrorKind as K;
        match self {
            BackendResolution::Ready(b) => Ok(b),
            BackendResolution::Off => Err(AiTranslateError::new(
                K::Off,
                "AI is not configured. Enable an AI provider in settings.",
            )),
            BackendResolution::NotConfigured(reason) => Err(AiTranslateError::new(K::NotConfigured, reason)),
            BackendResolution::UnknownProvider(p) => {
                Err(AiTranslateError::new(K::UnknownProvider, format!("Unknown AI provider: {p}")))
            }
        }
    }

    /// Maps a resolution onto the graceful-empty surface used by nice-to-have
    /// features (folder suggestions): the backend when ready, else `None` after
    /// logging the reason at debug. `context` labels the log line.
    pub fn ready_or_log(self, context: &str) -> Option<super::client::AiBackend> {
        match self {
            BackendResolution::Ready(b) => Some(b),
            BackendResolution::Off => {
                log::debug!("{context}: provider is off, returning empty");
                None
            }
            BackendResolution::NotConfigured(reason) => {
                log::debug!("{context}: backend not configured ({reason}), returning empty");
                None
            }
            BackendResolution::UnknownProvider(p) => {
                log::debug!("{context}: unknown provider '{p}', returning empty");
                None
            }
        }
    }
}

/// Resolves the AI backend for a translate command, mapping every non-ready case
/// to a typed [`AiTranslateError`] the dialog can toast.
///
/// When `cloud_only`, rejects any provider other than `cloud` up front: selection
/// AI needs a cloud model (small local models can't reliably handle a 200+-name
/// folder sample plus the structured prompt). The frontend hides the AI chip when
/// `ai.provider !== 'cloud'`, so this gate is the belt-and-braces check for a
/// misconfigured frontend or an automation caller. Because a non-cloud provider
/// (including `off`) is rejected here, the cloud path only ever reaches
/// [`BackendResolution::into_translate_result`] with `Ready`/`NotConfigured`.
pub fn resolve_translate_backend(cloud_only: bool) -> Result<super::client::AiBackend, AiTranslateError> {
    if cloud_only && get_provider() != "cloud" {
        return Err(AiTranslateError::new(
            AiTranslateErrorKind::NotConfigured,
            "AI selection needs a cloud provider. Set one in Settings > AI.",
        ));
    }
    resolve_backend().into_translate_result()
}

/// Pure provider-resolution decision, split out so the global `MANAGER` lock doesn't have to
/// participate in tests (mirrors `compute_ai_status`).
///
/// The empty-key → `NotConfigured` gate applies ONLY when the provider needs a key
/// (`requires_api_key`). Keyless OpenAI-compatible endpoints (Ollama, LM Studio, a custom
/// endpoint) legitimately have no key, so they resolve to `Ready` on a non-empty base URL.
fn resolve_backend_inner(
    provider: &str,
    port: Option<u16>,
    api_key: String,
    base_url: String,
    model: String,
    requires_api_key: bool,
) -> BackendResolution {
    match provider {
        "off" => BackendResolution::Off,
        "local" => match port {
            Some(port) => BackendResolution::Ready(super::client::AiBackend::local(port)),
            None => BackendResolution::NotConfigured("Local AI server isn't running. Start it in settings."),
        },
        "cloud" => {
            if requires_api_key && api_key.is_empty() {
                BackendResolution::NotConfigured("Cloud AI API key not configured. Add it in settings.")
            } else if base_url.is_empty() {
                BackendResolution::NotConfigured("Cloud AI endpoint not configured. Add it in settings.")
            } else {
                BackendResolution::Ready(super::client::AiBackend::remote(api_key, base_url, model))
            }
        }
        other => BackendResolution::UnknownProvider(other.to_string()),
    }
}

/// Runtime status of the AI subsystem, returned to frontend.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeStatus {
    pub server_running: bool,
    pub server_starting: bool,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub model_installed: bool,
    pub model_name: String,
    pub model_size_bytes: u64,
    pub model_size_formatted: String,
    pub download_in_progress: bool,
    pub local_ai_supported: bool,
    pub kv_bytes_per_token: u64,
    pub base_overhead_bytes: u64,
}

/// Returns the full runtime status of the AI subsystem.
#[tauri::command]
#[specta::specta]
pub fn get_ai_runtime_status() -> AiRuntimeStatus {
    let model = get_current_model();
    let manager = MANAGER.lock_ignore_poison();
    match &*manager {
        Some(m) => AiRuntimeStatus {
            server_running: m.child_pid.is_some() && !m.server_starting,
            server_starting: m.server_starting,
            pid: m.child_pid,
            port: m.state.port,
            model_installed: is_fully_installed(m),
            model_name: model.display_name.to_string(),
            model_size_bytes: model.size_bytes,
            model_size_formatted: super::state::format_bytes_gb(model.size_bytes),
            download_in_progress: m.download_in_progress,
            local_ai_supported: is_local_ai_supported(),
            kv_bytes_per_token: model.kv_bytes_per_token,
            base_overhead_bytes: model.base_overhead_bytes,
        },
        None => AiRuntimeStatus {
            server_running: false,
            server_starting: false,
            pid: None,
            port: None,
            model_installed: false,
            model_name: model.display_name.to_string(),
            model_size_bytes: model.size_bytes,
            model_size_formatted: super::state::format_bytes_gb(model.size_bytes),
            download_in_progress: false,
            local_ai_supported: is_local_ai_supported(),
            kv_bytes_per_token: model.kv_bytes_per_token,
            base_overhead_bytes: model.base_overhead_bytes,
        },
    }
}

/// Stores provider + context size + OpenAI config in manager state.
/// If provider is `local` and model is installed and hardware is supported, starts the server
/// in a background task. If provider is NOT `local` and a server is running, stops it.
/// Returns immediately.
#[tauri::command]
#[specta::specta]
pub fn configure_ai<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    context_size: u32,
    cloud_api_key: String,
    cloud_base_url: String,
    cloud_model: String,
    cloud_requires_api_key: bool,
) -> Result<(), String> {
    log::debug!(
        "AI configure: provider={provider}, context_size={context_size}, base_url={cloud_base_url}, model={cloud_model}, requires_api_key={cloud_requires_api_key}"
    );

    // Guard the BYOK key against plaintext exfiltration before we store config that
    // suggestions.rs / search will later send with an Authorization header. Only
    // enforced for the cloud provider with a non-empty base URL.
    if provider == "cloud" && !cloud_base_url.is_empty() {
        super::connection_check::validate_ai_base_url(&cloud_base_url, &cloud_api_key)?;
    }

    // Single lock: decide, stop, spawn (no race window for orphan processes)
    let spawn_result;
    {
        let mut manager = MANAGER.lock_ignore_poison();
        let Some(ref mut m) = *manager else { return Ok(()) };

        // Switching away from local: cancel any in-flight startup (so its waiter exits
        // quietly instead of reporting the deliberate stop as a failure) and stop a
        // running server.
        if provider != "local" {
            if let Some(token) = m.start_cancel.take() {
                token.cancel();
            }
            if let Some(pid) = m.child_pid.take() {
                log::info!("AI configure: provider changed away from local, stopping server");
                super::process::kill_and_reap_in_background(pid);
                m.state.port = None;
                m.state.pid = None;
                save_state(&m.ai_dir, &m.state);
            }
        }

        m.provider = provider.clone();
        m.context_size = context_size;
        m.cloud_api_key = cloud_api_key;
        m.cloud_base_url = cloud_base_url;
        m.cloud_model = cloud_model;
        m.cloud_requires_api_key = cloud_requires_api_key;

        // Spawn server synchronously so child_pid is set before the lock is released.
        // Only the health check (up to 60s) runs async.
        spawn_result =
            if provider == "local" && is_local_ai_supported() && is_fully_installed(m) && m.child_pid.is_none() {
                match spawn_and_track_server(m) {
                    Ok((pid, port, cancel)) => {
                        m.server_starting = true;
                        Some((pid, port, cancel))
                    }
                    Err(e) => {
                        crate::log_error!("AI configure: couldn't spawn server: {e}");
                        None
                    }
                }
            } else {
                None
            };
    }

    // Health check asynchronously (the slow part, up to 60s)
    if let Some((pid, port, cancel)) = spawn_result {
        let _ = AiStarting.emit(&app);
        let ai_dir = get_ai_dir(&app);
        tauri::async_runtime::spawn(async move {
            handle_startup_outcome(wait_for_server_health(&ai_dir, pid, port, cancel).await, pid, &app);
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_off_and_unknown_provider() {
        assert!(matches!(
            resolve_backend_inner("off", None, String::new(), String::new(), String::new(), true),
            BackendResolution::Off
        ));
        assert!(matches!(
            resolve_backend_inner("bogus", None, String::new(), String::new(), String::new(), true),
            BackendResolution::UnknownProvider(p) if p == "bogus"
        ));
    }

    #[test]
    fn resolve_local_needs_a_running_port() {
        assert!(matches!(
            resolve_backend_inner("local", None, String::new(), String::new(), String::new(), false),
            BackendResolution::NotConfigured(_)
        ));
        assert!(matches!(
            resolve_backend_inner("local", Some(8080), String::new(), String::new(), String::new(), false),
            BackendResolution::Ready(_)
        ));
    }

    #[test]
    fn resolve_cloud_key_required_provider_needs_a_key() {
        // A key-requiring provider (OpenAI etc.) with no key stays NotConfigured (friendly hint).
        assert!(matches!(
            resolve_backend_inner(
                "cloud",
                None,
                String::new(),
                String::from("https://api.openai.com/v1"),
                String::from("gpt-4o-mini"),
                true,
            ),
            BackendResolution::NotConfigured(_)
        ));
        // Same provider, key present → Ready.
        assert!(matches!(
            resolve_backend_inner(
                "cloud",
                None,
                String::from("sk-key"),
                String::from("https://api.openai.com/v1"),
                String::from("gpt-4o-mini"),
                true,
            ),
            BackendResolution::Ready(_)
        ));
    }

    #[test]
    fn resolve_cloud_keyless_local_endpoint_is_ready() {
        // Ollama / LM Studio / custom: `requires_api_key = false`, no key, but a real endpoint +
        // model. This is the bug from issue #29 — it must resolve to Ready, not NotConfigured.
        assert!(matches!(
            resolve_backend_inner(
                "cloud",
                None,
                String::new(),
                String::from("http://localhost:11434/v1"),
                String::from("llama3.2"),
                false,
            ),
            BackendResolution::Ready(_)
        ));
        // A keyless *remote* custom endpoint is equally valid (custom shows no key field).
        assert!(matches!(
            resolve_backend_inner(
                "cloud",
                None,
                String::new(),
                String::from("https://my-proxy.example.com/v1"),
                String::from("some-model"),
                false,
            ),
            BackendResolution::Ready(_)
        ));
    }

    #[test]
    fn resolve_cloud_without_an_endpoint_is_not_configured() {
        // Keyless provider but no base URL yet (e.g. custom before the user types one): there's
        // nothing to connect to, so it's genuinely not configured.
        assert!(matches!(
            resolve_backend_inner("cloud", None, String::new(), String::new(), String::new(), false),
            BackendResolution::NotConfigured(_)
        ));
    }

    #[test]
    fn test_get_ai_status_no_manager() {
        // When manager is not initialized, status is Unavailable
        let status = get_ai_status();
        assert_eq!(status, AiStatus::Unavailable);
    }

    // --- compute_ai_status: pure decision function ---

    const NOW: u64 = 1_700_000_000;

    #[test]
    fn compute_ai_status_provider_off_is_unavailable() {
        let s = compute_ai_status("off", true, true, None, true, NOW);
        assert_eq!(s, AiStatus::Unavailable);
    }

    #[test]
    fn compute_ai_status_installed_and_running_is_available() {
        let s = compute_ai_status("local", true, true, None, true, NOW);
        assert_eq!(s, AiStatus::Available);
    }

    #[test]
    fn compute_ai_status_installed_but_server_down_is_unavailable() {
        let s = compute_ai_status("local", true, false, None, true, NOW);
        assert_eq!(s, AiStatus::Unavailable);
    }

    #[test]
    fn compute_ai_status_not_installed_offers_on_apple_silicon() {
        let s = compute_ai_status("local", false, false, None, true, NOW);
        assert_eq!(s, AiStatus::Offer);
    }

    #[test]
    fn compute_ai_status_not_installed_does_not_offer_on_intel() {
        // The bug this guard fixes: Intel users with default provider="local" used to see
        // the AI download toast, only to be rejected by `start_ai_download` on click.
        let s = compute_ai_status("local", false, false, None, false, NOW);
        assert_eq!(s, AiStatus::Unavailable);
    }

    #[test]
    fn compute_ai_status_intel_with_installed_state_still_unavailable() {
        // Defense in depth: even if state somehow says installed on Intel (e.g. user copied
        // their data dir across machines), we still don't claim Available because the binary
        // is ARM64-only and won't run.
        let s = compute_ai_status("local", true, false, None, false, NOW);
        assert_eq!(s, AiStatus::Unavailable);
    }

    #[test]
    fn compute_ai_status_dismissed_offer_is_hidden() {
        let s = compute_ai_status("local", false, false, Some(NOW + 60), true, NOW);
        assert_eq!(s, AiStatus::Unavailable);
    }

    #[test]
    fn compute_ai_status_expired_dismissal_offers_again() {
        let s = compute_ai_status("local", false, false, Some(NOW - 60), true, NOW);
        assert_eq!(s, AiStatus::Offer);
    }
}
