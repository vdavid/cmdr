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
use crate::managed_policy::{AiDestination, ManagedAiRefusal, ManagedPolicy};
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
        &crate::managed_policy::current(),
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
    policy: &ManagedPolicy,
    provider: &str,
    installed: bool,
    server_running: bool,
    dismissed_until: Option<u64>,
    local_ai_supported: bool,
    now_secs: u64,
) -> AiStatus {
    if provider == "off" || policy.ai_destination(&AiDestination::LocalServer).is_err() {
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

/// Resolves the configured AI provider into either a ready-to-use [`AiBackend`](super::client::AiBackend) or
/// a reason why one couldn't be built.
///
/// Centralizes the provider-routing logic so callers (`suggestions.rs`,
/// `commands/search.rs`) just match on the variants and decide whether the case
/// should be hard-error or graceful-empty.
pub enum BackendResolution {
    /// `provider = "off"`: AI features are turned off.
    Off,
    /// `provider = "cloud"`, but the user hasn't turned on "Allow cloud AI" (or turned it off).
    /// Decided before any key or endpoint check: consent precedes setup.
    NoCloudConsent,
    /// Provider is set but missing config (e.g. local server not running, cloud key blank).
    /// Includes a human-readable reason suitable for error toasts.
    NotConfigured(&'static str),
    /// Backend is ready to use.
    Ready(super::client::AiBackend),
    /// Provider value isn't recognized.
    UnknownProvider(String),
    /// The organization's managed policy refuses this provider or endpoint. Decided before
    /// consent, key, and endpoint, so the user sees the reason they can't change themselves.
    Managed(ManagedAiRefusal),
}

/// The ONE place an LLM backend comes from, and so the one place the managed policy's typed
/// reason is produced (the client re-checks per request) and cloud consent is enforced
/// (`super::cloud_consent`). Taking the app handle is what makes that structural: no caller can
/// resolve a backend without the consent read, and `AiBackend::remote` is private to `ai/`.
/// Consent is read only when the provider is cloud, and outside the `MANAGER` lock.
pub fn resolve_backend<R: Runtime>(app: &AppHandle<R>) -> BackendResolution {
    let provider = get_provider();
    let cloud_consent = provider == "cloud" && super::cloud_consent::cloud_consent_from_app(app);
    let (api_key, base_url, model) = get_cloud_config();
    resolve_backend_inner(
        &crate::managed_policy::current(),
        &provider,
        get_port(),
        api_key,
        base_url,
        model,
        super::state::get_cloud_requires_api_key(),
        cloud_consent,
    )
}

/// Like [`resolve_backend`], but substitutes a dedicated `model_override` onto the
/// resolved CLOUD backend — the Ask Cmdr interactive slot's own model choice. `None` (or a
/// local provider, whose model is fixed) resolves exactly like [`resolve_backend`]. The
/// slot layers a model choice OVER the shared `ai/` provider config (readiness, keys, base
/// URL, consent, on/off all come from `resolve_backend`), so it never forks provider management
/// (agent decision D49).
pub fn resolve_backend_with_model<R: Runtime>(app: &AppHandle<R>, model_override: Option<&str>) -> BackendResolution {
    let base = resolve_backend(app);
    match model_override {
        Some(model) if !model.is_empty() && get_provider() == "cloud" => match base {
            // Rebuild the ready cloud backend with the slot's model; the adapter is still
            // chosen from the model name inside `AiBackend::remote`.
            BackendResolution::Ready(_) => {
                let (api_key, base_url, _model) = get_cloud_config();
                BackendResolution::Ready(super::client::AiBackend::remote(api_key, base_url, model.to_string()))
            }
            other => other,
        },
        _ => base,
    }
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
            BackendResolution::NoCloudConsent => Err(AiTranslateError::new(
                K::NoCloudConsent,
                "Cloud AI isn't allowed in Settings > AI.",
            )),
            BackendResolution::NotConfigured(reason) => Err(AiTranslateError::new(K::NotConfigured, reason)),
            BackendResolution::UnknownProvider(p) => Err(AiTranslateError::new(
                K::UnknownProvider,
                format!("Unknown AI provider: {p}"),
            )),
            BackendResolution::Managed(refusal) => Err(AiTranslateError::from_refusal(refusal)),
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
            BackendResolution::NoCloudConsent => {
                log::debug!("{context}: cloud AI isn't allowed, returning empty");
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
            BackendResolution::Managed(refusal) => {
                log::debug!("{context}: the organization's policy refuses it ({refusal:?}), returning empty");
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
/// [`BackendResolution::into_translate_result`] with `Ready`/`NoCloudConsent`/`NotConfigured`/`Managed`.
pub fn resolve_translate_backend<R: Runtime>(
    app: &AppHandle<R>,
    cloud_only: bool,
) -> Result<super::client::AiBackend, AiTranslateError> {
    if cloud_only && get_provider() != "cloud" {
        return Err(cloud_only_refusal(&crate::managed_policy::current()));
    }
    resolve_backend(app).into_translate_result()
}

/// Why a cloud-only feature can't run on a non-cloud provider: the organization's reason when its
/// policy rules out every cloud host (so nobody is told to pick a provider they can't), else the
/// setup hint.
fn cloud_only_refusal(policy: &ManagedPolicy) -> AiTranslateError {
    match policy.any_cloud_refusal() {
        Some(refusal) => AiTranslateError::from_refusal(refusal),
        None => AiTranslateError::new(
            AiTranslateErrorKind::NotConfigured,
            "AI selection needs a cloud provider. Set one in Settings > AI.",
        ),
    }
}

/// Pure provider-resolution decision, split out so the global `MANAGER` lock doesn't have to
/// participate in tests (mirrors `compute_ai_status`).
///
/// The managed policy is checked FIRST, for every provider (`DisableAI` refuses even `off`, so a
/// caller names the organization's reason), then for cloud against the configured endpoint. Cloud
/// consent follows, before the key and endpoint: nothing about the service matters until the user
/// allowed sending to it. Local and off ignore it.
///
/// The empty-key → `NotConfigured` gate applies ONLY when the provider needs a key
/// (`requires_api_key`). Keyless OpenAI-compatible endpoints (Ollama, LM Studio, a custom
/// endpoint) legitimately have no key, so they resolve to `Ready` on a non-empty base URL.
#[allow(
    clippy::too_many_arguments,
    reason = "a pure decision over every input it reads, for tests"
)]
fn resolve_backend_inner(
    policy: &ManagedPolicy,
    provider: &str,
    port: Option<u16>,
    api_key: String,
    base_url: String,
    model: String,
    requires_api_key: bool,
    cloud_consent: bool,
) -> BackendResolution {
    // The organization's answer comes first: nothing about consent or setup matters when the
    // policy refuses, and the user should see the reason they can't change themselves.
    if let Err(refusal) = policy.ai_destination(&AiDestination::LocalServer) {
        return BackendResolution::Managed(refusal);
    }
    match provider {
        "off" => BackendResolution::Off,
        "local" => match port {
            Some(port) => BackendResolution::Ready(super::client::AiBackend::local(port)),
            None => BackendResolution::NotConfigured("Local AI server isn't running. Start it in settings."),
        },
        "cloud" => {
            if let Err(refusal) = policy.ai_destination(&super::client::remote_destination(&base_url)) {
                BackendResolution::Managed(refusal)
            } else if !cloud_consent {
                BackendResolution::NoCloudConsent
            } else if requires_api_key && api_key.is_empty() {
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
            download_in_progress: false,
            local_ai_supported: is_local_ai_supported(),
            kv_bytes_per_token: model.kv_bytes_per_token,
            base_overhead_bytes: model.base_overhead_bytes,
        },
    }
}

/// Outcome of a [`configure_ai`] call. Carries the secret-store failure (if any) rather than
/// failing the whole call: the rest of the config still applies, and the frontend needs the typed
/// error to tell the user their keyring is locked.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureAiOutcome {
    /// Set when the provider's key exists but couldn't be read from the OS secret store.
    pub secret_store_error: Option<super::api_keys::AiApiKeyError>,
}

/// Stores provider + context size + OpenAI config in manager state.
/// If provider is `local` and model is installed and hardware is supported, starts the server
/// in a background task. If provider is NOT `local` and a server is running, stops it.
/// Returns immediately.
///
/// Takes a provider ID, NOT a key: the BYOK key is read here from the OS secret store, so it never
/// travels through a webview. See `api_keys.rs`.
#[tauri::command]
#[specta::specta]
pub fn configure_ai<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    context_size: u32,
    cloud_provider_id: String,
    cloud_base_url: String,
    cloud_model: String,
    cloud_requires_api_key: bool,
) -> Result<ConfigureAiOutcome, String> {
    log::debug!(
        "AI configure: provider={provider}, context_size={context_size}, cloud_provider={cloud_provider_id}, base_url={cloud_base_url}, model={cloud_model}, requires_api_key={cloud_requires_api_key}"
    );

    let (cloud_api_key, secret_store_error) = super::api_keys::read_for_backend(&cloud_provider_id);

    // Guard the BYOK key against plaintext exfiltration before we store config that
    // suggestions.rs / search will later send with an Authorization header. Only
    // enforced for the cloud provider with a non-empty base URL.
    if provider == "cloud" && !cloud_base_url.is_empty() {
        super::connection_check::validate_ai_base_url(&cloud_base_url, &cloud_api_key)?;
    }

    // Under the organization's `DisableAI` the local server never runs, whatever the provider:
    // treated exactly like switching away from local. Cached read: this must not block.
    let local_allowed =
        provider == "local" && super::managed::local_ai_allowed(&crate::managed_policy::current()).is_ok();

    // Single lock: decide, stop, spawn (no race window for orphan processes)
    let spawn_result;
    {
        let mut manager = MANAGER.lock_ignore_poison();
        let Some(ref mut m) = *manager else {
            return Ok(ConfigureAiOutcome { secret_store_error });
        };

        // Switching away from local: cancel any in-flight startup (so its waiter exits
        // quietly instead of reporting the deliberate stop as a failure) and stop a
        // running server.
        if !local_allowed {
            if let Some(token) = m.start_cancel.take() {
                token.cancel();
            }
            if let Some(pid) = m.child_pid.take() {
                log::info!(
                    "AI configure: provider changed away from local (or local AI is managed off), stopping server"
                );
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
        spawn_result = if local_allowed && is_local_ai_supported() && is_fully_installed(m) && m.child_pid.is_none() {
            match spawn_and_track_server(m) {
                Ok((pid, port, cancel)) => {
                    m.server_starting = true;
                    Some((pid, port, cancel))
                }
                Err(super::server::LocalAiError::Managed { refusal }) => {
                    log::info!("AI configure: the organization's policy refuses local AI ({refusal:?}), not starting");
                    None
                }
                Err(e) => {
                    crate::log_error!("AI configure: couldn't spawn server: {e:?}");
                    None
                }
            }
        } else {
            None
        };
    }

    // The wake loop gates on whether a provider can answer, and it caches that rather than
    // asking per live batch. This is the one place the answer can move.
    crate::agent::wake::refresh_readiness(&app);

    // Health check asynchronously (the slow part, up to 60s)
    if let Some((pid, port, cancel)) = spawn_result {
        let _ = AiStarting.emit(&app);
        let ai_dir = get_ai_dir(&app);
        tauri::async_runtime::spawn(async move {
            handle_startup_outcome(wait_for_server_health(&ai_dir, pid, port, cancel).await, pid, &app);
        });
    }

    Ok(ConfigureAiOutcome { secret_store_error })
}

#[cfg(test)]
#[path = "manager_test.rs"]
mod tests;
