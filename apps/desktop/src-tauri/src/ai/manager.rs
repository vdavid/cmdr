//! AI model download manager and llama-server process lifecycle.
//!
//! The llama-server binary is bundled with the app (no runtime download needed).
//! Only the AI model (~4.3 GB) is downloaded on first use.
//!
//! Uses `is_local_ai_supported()` to gate local-only operations (requires Apple Silicon).

use super::download::{cleanup_partial, download_file};
use super::extract::{LLAMA_SERVER_BINARY, REQUIRED_DYLIB, extract_bundled_llama_server};
use super::process::{
    SERVER_LOG_FILENAME, find_available_port, is_process_alive, log_diagnostics, read_log_tail, spawn_llama_server,
    stop_process,
};
use super::{AiState, AiStatus, ModelInfo, get_default_model, get_model_by_id, is_local_ai_supported};
use crate::ignore_poison::IgnorePoison;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Runtime};

/// Global manager state, accessible from Tauri commands.
static MANAGER: Mutex<Option<ManagerState>> = Mutex::new(None);

struct ManagerState {
    ai_dir: PathBuf,
    state: AiState,
    /// PID of the running llama-server process
    child_pid: Option<u32>,
    /// Flag to cancel an in-progress download
    cancel_requested: bool,
    /// Flag to prevent multiple concurrent downloads
    download_in_progress: bool,
    /// True while the server is starting up (health check polling)
    server_starting: bool,
    /// AI provider mode: "off", "openai-compatible", or "local"
    provider: String,
    /// Context size for local llama-server
    context_size: u32,
    /// OpenAI-compatible API key (stored here so suggestions.rs can read without settings files)
    openai_api_key: String,
    /// OpenAI-compatible base URL
    openai_base_url: String,
    /// OpenAI-compatible model name
    openai_model: String,
}

const STATE_FILENAME: &str = "ai-state.json";
const DISMISS_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days in seconds
/// Stale partial downloads older than this are cleaned up at app start.
const STALE_PARTIAL_SECONDS: u64 = 24 * 60 * 60; // 24 hours

/// Initializes the AI manager. Called once on app startup.
///
/// Only sets up directories and cleans stale PIDs. Does NOT start the server.
/// Server start is triggered later by `configure_ai` when the frontend pushes settings.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let ai_dir = get_ai_dir(app);
    let state = load_state(&ai_dir);

    let mut manager = MANAGER.lock_ignore_poison();
    *manager = Some(ManagerState {
        ai_dir,
        state,
        child_pid: None,
        cancel_requested: false,
        download_in_progress: false,
        server_starting: false,
        provider: String::from("local"),
        context_size: 4096,
        openai_api_key: String::new(),
        openai_base_url: String::from("https://api.openai.com/v1"),
        openai_model: String::from("gpt-4o-mini"),
    });

    // Clean up stale PID from a previous session (crash, force-quit, or normal restart)
    if let Some(ref mut m) = *manager
        && let Some(pid) = m.state.pid
    {
        if is_process_alive(pid) {
            log::info!("AI manager: stopping orphaned llama-server (PID {pid}) from previous session");
            stop_process(pid);
        } else {
            log::debug!("AI manager: clearing dead PID {pid} from previous session");
        }
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
pub fn shutdown() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager
        && let Some(pid) = m.child_pid.take()
    {
        stop_process(pid);
    }
}

/// Returns the current AI status.
#[tauri::command]
pub fn get_ai_status() -> AiStatus {
    let manager = MANAGER.lock_ignore_poison();
    match &*manager {
        Some(m) if m.provider == "off" => AiStatus::Unavailable,
        Some(m) if m.state.installed && m.child_pid.is_some() => AiStatus::Available,
        Some(m) if m.state.installed => AiStatus::Unavailable, // installed but server not running
        Some(m) => {
            // Check if dismissed
            if let Some(until) = m.state.dismissed_until
                && is_still_dismissed(until)
            {
                return AiStatus::Unavailable;
            }
            AiStatus::Offer
        }
        None => AiStatus::Unavailable,
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

/// Returns the OpenAI config stored in manager state.
pub fn get_openai_config() -> (String, String, String) {
    let manager = MANAGER.lock_ignore_poison();
    manager
        .as_ref()
        .map(|m| {
            (
                m.openai_api_key.clone(),
                m.openai_base_url.clone(),
                m.openai_model.clone(),
            )
        })
        .unwrap_or_default()
}

/// Starts the AI download (binary + model).
#[tauri::command]
pub async fn start_ai_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if !is_local_ai_supported() {
        return Err(String::from("Local AI not supported on this hardware"));
    }

    // Check if download is already in progress
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            if m.download_in_progress {
                log::warn!("AI download: already in progress, ignoring duplicate request");
                return Ok(());
            }
            m.download_in_progress = true;
        }
    }

    let result = do_download(&app).await;

    // Clear in-progress flag
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            m.download_in_progress = false;
        }
    }

    result
}

/// Cancels an in-progress download.
#[tauri::command]
pub fn cancel_ai_download() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        m.cancel_requested = true;
    }
}

/// Uninstalls the AI model and binary, resets state.
#[tauri::command]
pub fn uninstall_ai() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        // Stop server if running
        if let Some(pid) = m.child_pid.take() {
            stop_process(pid);
        }

        // Delete files
        let model = get_model_by_id(&m.state.installed_model_id).unwrap_or_else(get_default_model);
        let _ = fs::remove_file(m.ai_dir.join(LLAMA_SERVER_BINARY));
        let _ = fs::remove_file(m.ai_dir.join(model.filename));

        // Reset state
        m.state.installed = false;
        m.state.port = None;
        m.state.pid = None;
        m.state.model_download_complete = false;
        save_state(&m.ai_dir, &m.state);
    }
}

/// Dismisses the AI offer notification for 7 days.
#[tauri::command]
pub fn dismiss_ai_offer() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        m.state.dismissed_until = Some(now + DISMISS_SECONDS);
        save_state(&m.ai_dir, &m.state);
    }
}

/// Permanently opts out of AI features.
/// Can be re-enabled later via settings.
/// Also cleans up any partial downloads to avoid wasting disk space.
#[tauri::command]
pub fn opt_out_ai() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        // Clean up partial model download if exists
        let model = get_model_by_id(&m.state.installed_model_id).unwrap_or_else(get_default_model);
        let model_path = m.ai_dir.join(model.filename);
        if model_path.exists() && !m.state.model_download_complete {
            log::info!("AI opt-out: removing partial model download");
            let _ = fs::remove_file(&model_path);
        }

        m.state.opted_out = true;
        m.state.dismissed_until = None; // Clear temporary dismiss
        m.state.partial_download_started = None;
        save_state(&m.ai_dir, &m.state);
    }
}

/// Re-enables AI features after opting out.
#[tauri::command]
pub fn opt_in_ai() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        m.state.opted_out = false;
        save_state(&m.ai_dir, &m.state);
    }
}

/// Returns whether the user has opted out of AI features.
#[tauri::command]
pub fn is_ai_opted_out() -> bool {
    let manager = MANAGER.lock_ignore_poison();
    manager.as_ref().is_some_and(|m| m.state.opted_out)
}

/// Model info returned to frontend.
#[derive(Debug, Clone, serde::Serialize)]
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

/// Runtime status of the AI subsystem, returned to frontend.
#[derive(Debug, Clone, serde::Serialize)]
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
            model_size_formatted: format_bytes_gb(model.size_bytes),
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
            model_size_formatted: format_bytes_gb(model.size_bytes),
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
pub fn configure_ai<R: Runtime>(
    app: AppHandle<R>,
    provider: String,
    context_size: u32,
    openai_api_key: String,
    openai_base_url: String,
    openai_model: String,
) {
    log::debug!(
        "AI configure: provider={provider}, context_size={context_size}, base_url={openai_base_url}, model={openai_model}"
    );

    let should_start_local;
    let should_stop;
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            // Stop server if switching away from local while one is running
            should_stop = provider != "local" && m.child_pid.is_some();

            m.provider = provider.clone();
            m.context_size = context_size;
            m.openai_api_key = openai_api_key;
            m.openai_base_url = openai_base_url;
            m.openai_model = openai_model;

            should_start_local =
                provider == "local" && is_local_ai_supported() && is_fully_installed(m) && m.child_pid.is_none();
        } else {
            return;
        }
    }

    // Stop local server if provider changed away from local
    if should_stop {
        log::info!("AI configure: provider changed away from local, stopping server");
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager
            && let Some(pid) = m.child_pid.take()
        {
            stop_process(pid);
            m.state.port = None;
            m.state.pid = None;
            save_state(&m.ai_dir, &m.state);
        }
    }

    if should_start_local {
        let app_clone = app.clone();
        let _ = app.emit("ai-starting", ());
        {
            let mut manager = MANAGER.lock_ignore_poison();
            if let Some(ref mut m) = *manager {
                m.server_starting = true;
            }
        }
        tauri::async_runtime::spawn(async move {
            match start_server_inner(&app_clone).await {
                Ok(()) => {
                    log::info!("AI: server ready");
                    let _ = app_clone.emit("ai-server-ready", ());
                }
                Err(e) => log::error!("AI manager: failed to start server: {e}"),
            }
            let mut manager = MANAGER.lock_ignore_poison();
            if let Some(ref mut m) = *manager {
                m.server_starting = false;
            }
        });
    }
}

/// Stops the local llama-server without uninstalling.
#[tauri::command]
pub fn stop_ai_server() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager
        && let Some(pid) = m.child_pid.take()
    {
        log::info!("AI: stopping server (PID {pid})");
        stop_process(pid);
        m.state.port = None;
        m.state.pid = None;
        save_state(&m.ai_dir, &m.state);
    }
}

/// Starts the local llama-server with the given context size.
/// Spawns the server in a background task and returns immediately.
#[tauri::command]
pub fn start_ai_server<R: Runtime>(app: AppHandle<R>, ctx_size: u32) -> Result<(), String> {
    if !is_local_ai_supported() {
        return Err(String::from("Local AI not supported on this hardware"));
    }

    let should_start;
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            m.context_size = ctx_size;
            should_start = is_fully_installed(m) && m.child_pid.is_none();
        } else {
            return Err(String::from("AI manager not initialized"));
        }
    }

    if should_start {
        let app_clone = app.clone();
        let _ = app.emit("ai-starting", ());
        {
            let mut manager = MANAGER.lock_ignore_poison();
            if let Some(ref mut m) = *manager {
                m.server_starting = true;
            }
        }
        tauri::async_runtime::spawn(async move {
            match start_server_inner(&app_clone).await {
                Ok(()) => {
                    log::info!("AI: server ready");
                    let _ = app_clone.emit("ai-server-ready", ());
                }
                Err(e) => log::error!("AI manager: failed to start server: {e}"),
            }
            let mut manager = MANAGER.lock_ignore_poison();
            if let Some(ref mut m) = *manager {
                m.server_starting = false;
            }
        });
    }

    Ok(())
}

/// Formats bytes as GB with one decimal place (like "4.3 GB").
fn format_bytes_gb(bytes: u64) -> String {
    let gb = bytes as f64 / 1_000_000_000.0;
    format!("{gb:.1} GB")
}

// --- Internal helpers ---

/// Returns the model info for the currently selected/installed model.
/// Falls back to default if the stored model ID is not in the registry.
fn get_current_model() -> &'static ModelInfo {
    let manager = MANAGER.lock_ignore_poison();
    if let Some(ref m) = *manager
        && let Some(model) = get_model_by_id(&m.state.installed_model_id)
    {
        return model;
    }
    get_default_model()
}

fn get_ai_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    crate::config::resolved_app_data_dir(app)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("ai")
}

fn load_state(ai_dir: &Path) -> AiState {
    let path = ai_dir.join(STATE_FILENAME);
    fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_state(ai_dir: &Path, state: &AiState) {
    let _ = fs::create_dir_all(ai_dir);
    let path = ai_dir.join(STATE_FILENAME);
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json);
    }
}

/// Returns true if AI is fully installed and ready to run.
/// Requires binary, model, AND shared libraries to exist.
fn is_fully_installed(m: &ManagerState) -> bool {
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

/// Cleans up stale partial downloads older than 24 hours.
fn cleanup_stale_partial_download(m: &mut ManagerState) {
    // Only cleanup if there's a partial download (not complete) with a start timestamp
    if m.state.model_download_complete {
        return;
    }

    let Some(started) = m.state.partial_download_started else {
        return;
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now.saturating_sub(started) >= STALE_PARTIAL_SECONDS {
        let model = get_model_by_id(&m.state.installed_model_id).unwrap_or_else(get_default_model);
        let model_path = m.ai_dir.join(model.filename);
        if model_path.exists() {
            log::debug!(
                "AI: cleaning up stale partial download (started {} hours ago)",
                (now - started) / 3600
            );
            let _ = fs::remove_file(&model_path);
            m.state.partial_download_started = None;
            save_state(&m.ai_dir, &m.state);
        }
    }
}

fn is_still_dismissed(until_timestamp: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now < until_timestamp
}

async fn do_download<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let ai_dir = get_ai_dir(app);
    fs::create_dir_all(&ai_dir).map_err(|e| format!("Failed to create AI directory: {e}"))?;

    // Get the model to download (use default for new installs)
    let model = get_current_model();
    log::debug!("AI download: using model {} ({})", model.id, model.display_name);

    // Reset cancel flag and set the model ID we're installing
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            m.cancel_requested = false;
            m.state.installed_model_id = model.id.to_string();
        }
    }

    // Step 1: Extract llama-server from bundled archive (instant, no download needed)
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    if !binary_path.exists() {
        extract_bundled_llama_server(app, &ai_dir)?;
    }

    // Check if cancelled before starting big download
    if is_cancel_requested() {
        cleanup_partial(&ai_dir, model);
        return Err(String::from("Download cancelled"));
    }

    // Step 2: Download GGUF model - this is the only network download
    let model_path = ai_dir.join(model.filename);

    // Track when this partial download started (for stale cleanup)
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            m.state.partial_download_started = Some(now);
            save_state(&m.ai_dir, &m.state);
        }
    }

    download_file(app, model.url, &model_path, is_cancel_requested).await?;

    // Verify download integrity by checking file size
    let actual_size = fs::metadata(&model_path)
        .map(|m| m.len())
        .map_err(|e| format!("Failed to read downloaded model file: {e}"))?;

    if actual_size < model.size_bytes {
        log::error!(
            "AI download: model file size mismatch. Expected {} bytes, got {} bytes",
            model.size_bytes,
            actual_size
        );
        return Err(format!(
            "Download incomplete: expected {} bytes, got {} bytes",
            model.size_bytes, actual_size
        ));
    }

    log::debug!("AI download: model verified, {} bytes", actual_size);

    // Mark download as complete and update state
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            m.state.installed = true;
            m.state.model_download_complete = true;
            m.state.partial_download_started = None; // Clear partial marker
            save_state(&m.ai_dir, &m.state);
        }
    }

    // Emit installing event so UI shows "Setting up AI..." while server starts
    let _ = app.emit("ai-installing", ());

    // Start the server FIRST, then emit install complete
    // This ensures the server is healthy before showing "AI ready"
    start_server_inner(app).await?;

    // Emit install complete only after server is healthy
    let _ = app.emit("ai-install-complete", ());

    Ok(())
}

fn is_cancel_requested() -> bool {
    let manager = MANAGER.lock_ignore_poison();
    manager.as_ref().is_some_and(|m| m.cancel_requested)
}

async fn start_server_inner<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let ai_dir = get_ai_dir(app);
    let model = get_current_model();

    // Recovery: if model exists but binary is missing, try re-extraction
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    if !binary_path.exists() {
        log::debug!("AI manager: binary missing, attempting re-extraction...");
        extract_bundled_llama_server(app, &ai_dir)?;
    }

    // Read context size from manager state
    let ctx_size = {
        let manager = MANAGER.lock_ignore_poison();
        manager.as_ref().map(|m| m.context_size).unwrap_or(4096)
    };

    // Find an available port
    let port = find_available_port().ok_or("No available port")?;
    log::debug!("AI server: starting llama-server on port {port} with context size {ctx_size}");

    // Spawn the server process
    let pid = spawn_llama_server(&ai_dir, model.filename, port, ctx_size)?;

    // Brief pause to let the process initialize, then check if it's still alive
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if !is_process_alive(pid) {
        let last_lines = read_log_tail(&ai_dir, 20);
        log::error!("AI server: process died immediately. Last log lines:\n{last_lines}");
        let log_path = ai_dir.join(SERVER_LOG_FILENAME);
        return Err(format!("llama-server crashed on startup. Check log at: {log_path:?}"));
    }

    // Update state
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            m.state.port = Some(port);
            m.state.pid = Some(pid);
            m.child_pid = Some(pid);
            save_state(&m.ai_dir, &m.state);
        }
    }

    // Wait for health check (poll every 500ms, up to 60s)
    log::debug!("AI server: waiting for health check on port {port}...");
    for i in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Check if process is still alive
        if !is_process_alive(pid) {
            let last_lines = read_log_tail(&ai_dir, 20);
            log::error!("AI server: process died during startup. Last log lines:\n{last_lines}");
            return Err(format!("llama-server process (PID {pid}) died during startup"));
        }

        if super::client::health_check(port).await {
            log::debug!("AI server: healthy on port {port} after {}s", (i + 1) / 2);
            return Ok(());
        }

        // Log progress every 5 seconds
        if i % 10 == 9 {
            log::debug!("AI server: still waiting for health check ({}s)...", (i + 1) / 2);
            if let Some((line_count, last_line)) = log_diagnostics(&ai_dir) {
                log::debug!("AI server: log has {line_count} lines, last: {last_line}");
            }
        }
    }

    // Timed out - read the log for diagnostics
    let last_lines = read_log_tail(&ai_dir, 20);
    log::error!("AI server: health check timed out. Last log lines:\n{last_lines}");

    Err(String::from("llama-server failed to become healthy within 60s"))
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
        assert!(!state.opted_out);
    }

    #[test]
    fn test_state_serialization() {
        let state = AiState {
            installed: true,
            port: Some(52847),
            pid: Some(12345),
            installed_model_id: String::from("ministral-3b-instruct-q4km"),
            dismissed_until: None,
            opted_out: false,
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

    #[test]
    fn test_state_with_opted_out() {
        let state = AiState {
            opted_out: true,
            ..Default::default()
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: AiState = serde_json::from_str(&json).unwrap();
        assert!(parsed.opted_out);
    }

    #[test]
    fn test_get_ai_status_no_manager() {
        // When manager is not initialized, status is Unavailable
        let status = get_ai_status();
        assert_eq!(status, AiStatus::Unavailable);
    }
}
