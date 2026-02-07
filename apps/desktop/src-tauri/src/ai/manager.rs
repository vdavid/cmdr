//! AI model download manager and llama-server process lifecycle.
//!
//! The llama-server binary is bundled with the app (no runtime download needed).
//! Only the AI model (~4.3 GB) is downloaded on first use.
//!
//! Uses runtime check `use_real_ai()` to enable/disable real AI features.
//! In dev mode without `CMDR_REAL_AI=1`, all AI features return Unavailable.

use super::download::{cleanup_partial, download_file};
use super::extract::{LLAMA_SERVER_BINARY, REQUIRED_DYLIB, extract_bundled_llama_server};
use super::process::{
    SERVER_LOG_FILENAME, find_available_port, is_process_alive, log_diagnostics, read_log_tail, spawn_llama_server,
    stop_process,
};
use super::{AiState, AiStatus, ModelInfo, get_default_model, get_model_by_id, use_real_ai};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, Runtime};

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
}

const STATE_FILENAME: &str = "ai-state.json";
const DISMISS_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days in seconds
/// Stale partial downloads older than this are cleaned up at app start.
const STALE_PARTIAL_SECONDS: u64 = 24 * 60 * 60; // 24 hours

/// Initializes the AI manager. Called once on app startup.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let ai_dir = get_ai_dir(app);
    let state = load_state(&ai_dir);

    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    *manager = Some(ManagerState {
        ai_dir,
        state,
        child_pid: None,
        cancel_requested: false,
        download_in_progress: false,
    });

    // Only run real AI initialization if enabled
    if use_real_ai() {
        log::debug!("AI manager: real AI enabled, checking installation...");

        // Clean up stale PID from a previous crash
        if let Some(ref mut m) = *manager
            && let Some(pid) = m.state.pid
            && !is_process_alive(pid)
        {
            log::debug!("AI manager: cleaning up stale PID {pid} from previous session");
            m.state.pid = None;
            m.state.port = None;
            save_state(&m.ai_dir, &m.state);
        }

        // Clean up stale partial downloads (older than 24 hours)
        if let Some(ref mut m) = *manager {
            cleanup_stale_partial_download(m);
        }

        // Only consider installed if model download is verified complete
        let mut is_ready = is_fully_installed(manager.as_ref().unwrap());
        log::debug!("AI manager: ready={is_ready}");

        // Recovery: if state says installed but files are missing, try to recover
        if !is_ready
            && let Some(ref mut m) = *manager
            && m.state.installed
        {
            let model = get_model_by_id(&m.state.installed_model_id).unwrap_or_else(get_default_model);
            let model_path = m.ai_dir.join(model.filename);
            let binary_path = m.ai_dir.join(LLAMA_SERVER_BINARY);

            // Check if model is complete but binary is missing (can re-extract)
            let model_complete = model_path.exists()
                && fs::metadata(&model_path)
                    .map(|meta| meta.len() >= model.size_bytes)
                    .unwrap_or(false);

            if model_complete && !binary_path.exists() {
                log::debug!("AI manager: model exists but binary missing, re-extracting...");
                match extract_bundled_llama_server(app, &m.ai_dir) {
                    Ok(()) => {
                        log::debug!("AI manager: binary re-extracted successfully");
                        is_ready = true;
                    }
                    Err(e) => {
                        log::error!("AI manager: failed to re-extract binary: {e}");
                    }
                }
            } else if !model_complete {
                // Model is missing or incomplete - reset installed state
                log::debug!("AI manager: model missing or incomplete, resetting installed state");
                m.state.installed = false;
                m.state.model_download_complete = false;
                save_state(&m.ai_dir, &m.state);
            }
        }

        if is_ready {
            let app_clone = app.clone();
            // Use tauri's runtime spawn instead of tokio::spawn since init() is called
            // during Tauri setup before the tokio runtime is fully available
            log::debug!("AI manager: spawning server start task...");
            // Emit starting event so frontend can show "AI starting..." notification
            let _ = app.emit("ai-starting", ());
            tauri::async_runtime::spawn(async move {
                match start_server_inner(&app_clone).await {
                    Ok(()) => {
                        log::info!("AI: server ready");
                        let _ = app_clone.emit("ai-server-ready", ());
                    }
                    Err(e) => log::error!("AI manager: failed to start server: {e}"),
                }
            });
        }
    } else {
        log::debug!("AI manager: real AI disabled (dev mode without CMDR_REAL_AI=1)");
    }
}

/// Shuts down the AI server. Called on app quit.
pub fn shutdown() {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut m) = *manager
        && let Some(pid) = m.child_pid.take()
    {
        stop_process(pid);
    }
}

/// Returns the current AI status.
#[tauri::command]
pub fn get_ai_status() -> AiStatus {
    // If real AI is not enabled (dev mode without env var), return Unavailable
    if !use_real_ai() {
        return AiStatus::Unavailable;
    }

    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    match &*manager {
        Some(m) if m.state.opted_out => AiStatus::Unavailable,
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
    if !use_real_ai() {
        return None;
    }
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.as_ref().and_then(|m| m.state.port)
}

/// Starts the AI download (binary + model).
#[tauri::command]
pub async fn start_ai_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if !use_real_ai() {
        return Err(String::from("AI features not enabled"));
    }

    // Check if download is already in progress
    {
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut m) = *manager {
            m.download_in_progress = false;
        }
    }

    result
}

/// Cancels an in-progress download.
#[tauri::command]
pub fn cancel_ai_download() {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut m) = *manager {
        m.cancel_requested = true;
    }
}

/// Uninstalls the AI model and binary, resets state.
#[tauri::command]
pub fn uninstall_ai() {
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut m) = *manager {
        m.state.opted_out = false;
        save_state(&m.ai_dir, &m.state);
    }
}

/// Returns whether the user has opted out of AI features.
#[tauri::command]
pub fn is_ai_opted_out() -> bool {
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
    }
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
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref m) = *manager
        && let Some(model) = get_model_by_id(&m.state.installed_model_id)
    {
        return model;
    }
    get_default_model()
}

fn get_ai_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    app.path()
        .app_data_dir()
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
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.as_ref().is_some_and(|m| m.cancel_requested)
}

async fn start_server_inner<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let ai_dir = get_ai_dir(app);
    let model = get_current_model();

    // Find an available port
    let port = find_available_port().ok_or("No available port")?;
    log::debug!("AI server: starting llama-server on port {port}");

    // Spawn the server process
    let pid = spawn_llama_server(&ai_dir, model.filename, port)?;

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
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
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
    fn test_get_ai_status_dev_mode() {
        // In dev mode without CMDR_REAL_AI env var, status is Unavailable
        let status = get_ai_status();
        // Note: This test will return Unavailable in dev mode (no env var)
        // or the actual status in release mode
        if !use_real_ai() {
            assert_eq!(status, AiStatus::Unavailable);
        }
    }
}
