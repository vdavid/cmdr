//! AI model download manager and llama-server process lifecycle.
//!
//! The llama-server binary is bundled with the app (no runtime download needed).
//! Only the AI model (~4.3 GB) is downloaded on first use.
//!
//! Uses runtime check `use_real_ai()` to enable/disable real AI features.
//! In dev mode without `CMDR_REAL_AI=1`, all AI features return Unavailable.

use super::{AiState, AiStatus, DownloadProgress, ModelInfo, get_default_model, get_model_by_id, use_real_ai};
use std::fs;
use std::io::Write;
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

/// Binary filename for the llama-server executable.
const LLAMA_SERVER_BINARY: &str = "llama-server";

/// Bundled llama-server archive (included in app bundle).
const BUNDLED_LLAMA_ARCHIVE: &str = "resources/llama-server.tar.gz";
/// Path of the llama-server binary inside the tar.gz archive (version-prefixed directory).
const LLAMA_ARCHIVE_BINARY_SUFFIX: &str = "llama-server";

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
    /// Human-readable size (e.g., "4.3 GB")
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

/// Formats bytes as GB with one decimal place (e.g., "4.3 GB").
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

/// A required shared library that must exist for llama-server to run.
const REQUIRED_DYLIB: &str = "libllama.dylib";

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

    download_file(app, model.url, &model_path).await?;

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

/// Extracts llama-server and its dylibs from the bundled archive.
fn extract_bundled_llama_server<R: Runtime>(app: &AppHandle<R>, ai_dir: &Path) -> Result<(), String> {
    log::debug!("AI: extracting bundled llama-server runtime...");

    // Get path to bundled resource
    let resource_path = app
        .path()
        .resolve(BUNDLED_LLAMA_ARCHIVE, tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve bundled archive path: {e}"))?;

    if !resource_path.exists() {
        return Err(format!("Bundled llama-server archive not found at: {resource_path:?}"));
    }

    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    extract_llama_server(&resource_path, &binary_path)?;

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&binary_path, perms).map_err(|e| format!("Failed to set permissions: {e}"))?;
    }

    log::debug!("AI: llama-server runtime extracted successfully");
    Ok(())
}

/// Extracts the llama-server binary and required shared libraries from the tar.gz archive.
fn extract_llama_server(archive_path: &Path, dest_path: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::{Archive, EntryType};

    let dest_dir = dest_path.parent().ok_or("Invalid destination path")?;
    let file = fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    let mut found_binary = false;
    let mut extracted_libs = Vec::new();
    let mut symlinks_to_create: Vec<(String, String)> = Vec::new();

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read archive entry: {e}"))?;

        // Get entry type and file info before any operations
        let entry_type = entry.header().entry_type();

        // Get file name and convert to owned String to release the borrow on entry
        let file_name = {
            let path = entry.path().map_err(|e| format!("Failed to get entry path: {e}"))?;
            path.file_name().and_then(|n| n.to_str()).map(String::from)
        };

        let Some(file_name) = file_name else {
            continue;
        };

        // Handle symlinks (common for versioned dylibs like libfoo.dylib -> libfoo.0.dylib)
        if entry_type == EntryType::Symlink && file_name.ends_with(".dylib") {
            let link_target = entry
                .link_name()
                .map_err(|e| format!("Failed to get symlink target for {file_name}: {e}"))?
                .ok_or_else(|| format!("Symlink {file_name} has no target"))?;
            let target_name = link_target
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid symlink target for {file_name}"))?
                .to_string();
            // Defer symlink creation until after all files are extracted
            symlinks_to_create.push((file_name, target_name));
            continue;
        }

        // Extract the llama-server binary
        if file_name == LLAMA_ARCHIVE_BINARY_SUFFIX {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|e| format!("Failed to extract llama-server: {e}"))?;
            fs::write(dest_path, &contents).map_err(|e| format!("Failed to write llama-server binary: {e}"))?;
            found_binary = true;
            log::debug!("AI extract: extracted llama-server binary");
        }
        // Extract all .dylib files (shared libraries required by llama-server)
        else if file_name.ends_with(".dylib") {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|e| format!("Failed to extract {file_name}: {e}"))?;
            let lib_dest = dest_dir.join(&file_name);
            fs::write(&lib_dest, &contents).map_err(|e| format!("Failed to write {file_name}: {e}"))?;
            extracted_libs.push(file_name);
        }
    }

    if !found_binary {
        return Err(String::from("llama-server binary not found in downloaded archive"));
    }

    // Create symlinks after all regular files are extracted
    #[cfg(unix)]
    for (link_name, target_name) in &symlinks_to_create {
        let link_path = dest_dir.join(link_name);
        // Remove existing file/symlink if present (from previous extraction)
        let _ = fs::remove_file(&link_path);
        std::os::unix::fs::symlink(target_name, &link_path)
            .map_err(|e| format!("Failed to create symlink {link_name} -> {target_name}: {e}"))?;
        log::debug!("AI extract: created symlink {link_name} -> {target_name}");
    }

    log::debug!(
        "AI extract: extracted {} libraries, {} symlinks",
        extracted_libs.len(),
        symlinks_to_create.len()
    );
    Ok(())
}

/// Downloads the AI model with progress reporting and resume support.
async fn download_file<R: Runtime>(app: &AppHandle<R>, url: &str, dest: &Path) -> Result<(), String> {
    use futures_util::StreamExt;

    let client = reqwest::Client::new();

    // Check for resume (existing partial file)
    let existing_size = dest.metadata().map(|m| m.len()).unwrap_or(0);
    if existing_size > 0 {
        log::debug!("AI download: resuming from {} bytes", existing_size);
    }

    let mut request = client.get(url);
    if existing_size > 0 {
        request = request.header("Range", format!("bytes={existing_size}-"));
    }

    let response = request.send().await.map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().map(|cl| cl + existing_size).unwrap_or(0);

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dest)
        .map_err(|e| format!("Failed to open file: {e}"))?;

    let mut stream = response.bytes_stream();
    let mut downloaded = existing_size;
    let start_time = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        // Check cancel
        if is_cancel_requested() {
            return Err(String::from("Download cancelled"));
        }

        let chunk = chunk.map_err(|e| format!("Download error: {e}"))?;
        file.write_all(&chunk).map_err(|e| format!("Write error: {e}"))?;
        downloaded += chunk.len() as u64;

        // Emit progress at most every 200ms
        if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                ((downloaded - existing_size) as f64 / elapsed) as u64
            } else {
                0
            };
            let eta_seconds = if speed > 0 {
                (total_bytes.saturating_sub(downloaded)) / speed
            } else {
                0
            };

            let progress = DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes,
                speed,
                eta_seconds,
            };
            let _ = app.emit("ai-download-progress", &progress);
            last_emit = std::time::Instant::now();
        }
    }

    // Final progress emit
    let _ = app.emit(
        "ai-download-progress",
        &DownloadProgress {
            bytes_downloaded: downloaded,
            total_bytes: downloaded,
            speed: 0,
            eta_seconds: 0,
        },
    );

    Ok(())
}

fn is_cancel_requested() -> bool {
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.as_ref().is_some_and(|m| m.cancel_requested)
}

fn cleanup_partial(ai_dir: &Path, model: &ModelInfo) {
    let _ = fs::remove_file(ai_dir.join(LLAMA_SERVER_BINARY));
    let _ = fs::remove_file(ai_dir.join(model.filename));
    // Also remove any dylibs that were extracted
    if let Ok(entries) = fs::read_dir(ai_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "dylib") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// Log file name for llama-server output (useful for debugging startup issues).
const SERVER_LOG_FILENAME: &str = "llama-server.log";

async fn start_server_inner<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let ai_dir = get_ai_dir(app);
    let model = get_current_model();
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    let model_path = ai_dir.join(model.filename);

    log::debug!("AI server: checking files at {:?}", ai_dir);
    log::debug!(
        "AI server: binary exists={}, model exists={} ({})",
        binary_path.exists(),
        model_path.exists(),
        model.id
    );

    if !binary_path.exists() || !model_path.exists() {
        return Err(String::from("AI files not found"));
    }

    // Find an available port
    let port = find_available_port().ok_or("No available port")?;
    log::debug!("AI server: starting llama-server on port {port}");

    // Create log file for llama-server output (helps debug startup issues)
    let log_path = ai_dir.join(SERVER_LOG_FILENAME);
    let log_file = fs::File::create(&log_path).map_err(|e| format!("Failed to create llama-server log file: {e}"))?;
    let log_file_stderr = log_file
        .try_clone()
        .map_err(|e| format!("Failed to clone log file handle: {e}"))?;

    log::debug!("AI server: logging output to {:?}", log_path);

    // Spawn llama-server with DYLD_LIBRARY_PATH set to find the shared libraries
    // The @rpath in the binary points to the directory where the dylibs are located
    let ai_dir_str = ai_dir.to_string_lossy();
    log::debug!("AI server: setting DYLD_LIBRARY_PATH to {}", ai_dir_str);

    let child = std::process::Command::new(&binary_path)
        .env("DYLD_LIBRARY_PATH", &*ai_dir_str)
        .current_dir(&ai_dir)
        .arg("-m")
        .arg(&model_path)
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--temp")
        .arg("0.6")
        .arg("--top-p")
        .arg("0.95")
        .arg("-n")
        .arg("4096")
        .arg("--jinja")
        .arg("-ngl")
        .arg("99")
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(log_file_stderr))
        .spawn()
        .map_err(|e| format!("Failed to start llama-server: {e}"))?;

    let pid = child.id();
    log::debug!("AI server: spawned llama-server with PID {pid}");

    // Brief pause to let the process initialize, then check if it's still alive
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    if !is_process_alive(pid) {
        // Process died immediately - read the log to see why
        let log_content = fs::read_to_string(&log_path).unwrap_or_default();
        let last_lines: String = log_content
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        log::error!("AI server: process died immediately. Last log lines:\n{last_lines}");
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
            // Process died - read the log to see why
            let log_content = fs::read_to_string(&log_path).unwrap_or_default();
            let last_lines: String = log_content
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
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
            // Also peek at the log file to see what llama-server is doing
            if let Ok(log_content) = fs::read_to_string(&log_path) {
                let line_count = log_content.lines().count();
                if let Some(last_line) = log_content.lines().last() {
                    log::debug!("AI server: log has {line_count} lines, last: {last_line}");
                }
            }
        }
    }

    // Timed out - read the log for diagnostics
    let log_content = fs::read_to_string(&log_path).unwrap_or_default();
    let last_lines: String = log_content
        .lines()
        .rev()
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    log::error!("AI server: health check timed out. Last log lines:\n{last_lines}");

    Err(String::from("llama-server failed to become healthy within 60s"))
}

fn find_available_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

fn stop_process(pid: u32) {
    #[cfg(unix)]
    {
        use std::time::Duration;

        // Send SIGTERM
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }

        // Wait up to 5s for graceful shutdown
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            // Check if process is still alive
            let result = unsafe { libc::kill(pid as i32, 0) };
            if result != 0 {
                return; // Process is gone
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        // Force kill
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) checks if a process exists without sending a signal
        let result = unsafe { libc::kill(pid as i32, 0) };
        result == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
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
