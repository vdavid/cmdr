//! AI model download manager and llama-server process lifecycle.
//!
//! In dev mode (`cfg(debug_assertions)`), all operations are mocked.

#[cfg(not(debug_assertions))]
use super::DownloadProgress;
use super::{AiState, AiStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(not(debug_assertions))]
use tauri::Emitter;
use tauri::{AppHandle, Manager, Runtime};

/// Global manager state, accessible from Tauri commands.
static MANAGER: Mutex<Option<ManagerState>> = Mutex::new(None);

struct ManagerState {
    ai_dir: PathBuf,
    state: AiState,
    /// PID of the running llama-server process (release only)
    #[cfg(not(debug_assertions))]
    child_pid: Option<u32>,
    /// Flag to cancel an in-progress download
    cancel_requested: bool,
}

/// Binary filename for the llama-server executable.
#[cfg(not(debug_assertions))]
const LLAMA_SERVER_BINARY: &str = "llama-server";
/// GGUF model filename.
#[cfg(not(debug_assertions))]
const MODEL_FILENAME: &str = "falcon-h1r-7b-q4km.gguf";

/// Temporary filename for the downloaded tar.gz archive.
#[cfg(not(debug_assertions))]
const LLAMA_ARCHIVE_FILENAME: &str = "llama-server.tar.gz";
/// Path of the llama-server binary inside the tar.gz archive (version-prefixed directory).
#[cfg(not(debug_assertions))]
const LLAMA_ARCHIVE_BINARY_SUFFIX: &str = "llama-server";

/// HuggingFace URL for the GGUF model.
#[cfg(not(debug_assertions))]
const MODEL_URL: &str = "https://huggingface.co/tiiuae/Falcon-H1R-7B-GGUF/resolve/main/Falcon-H1R-7B-Q4_K_M.gguf";

/// GitHub releases URL for the llama-server macOS ARM64 archive.
/// Pinned to a known stable release (b7815).
#[cfg(not(debug_assertions))]
const LLAMA_SERVER_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b7815/llama-b7815-bin-macos-arm64.tar.gz";

const STATE_FILENAME: &str = "ai-state.json";
const DISMISS_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days in seconds

/// Initializes the AI manager. Called once on app startup.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    let ai_dir = get_ai_dir(app);
    let state = load_state(&ai_dir);

    let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    *manager = Some(ManagerState {
        ai_dir,
        state,
        #[cfg(not(debug_assertions))]
        child_pid: None,
        cancel_requested: false,
    });

    // In release mode, clean up stale PID and start the server if installed
    #[cfg(not(debug_assertions))]
    {
        // Clean up stale PID from a previous crash
        if let Some(ref mut m) = *manager {
            if let Some(pid) = m.state.pid {
                if !is_process_alive(pid) {
                    log::info!("Cleaning up stale PID {pid} from previous session");
                    m.state.pid = None;
                    m.state.port = None;
                    save_state(&m.ai_dir, &m.state);
                }
            }
        }

        if is_installed_inner(manager.as_ref().unwrap()) {
            let app_clone = app.clone();
            tokio::spawn(async move {
                let _ = start_server_inner(&app_clone).await;
            });
        }
    }
}

/// Shuts down the AI server. Called on app quit.
pub fn shutdown() {
    #[cfg(not(debug_assertions))]
    {
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut m) = *manager {
            if let Some(pid) = m.child_pid.take() {
                stop_process(pid);
            }
        }
    }
}

/// Returns the current AI status.
#[tauri::command]
pub fn get_ai_status() -> AiStatus {
    #[cfg(debug_assertions)]
    {
        AiStatus::Available
    }

    #[cfg(not(debug_assertions))]
    {
        let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        match &*manager {
            Some(m) if m.state.installed && m.child_pid.is_some() => AiStatus::Available,
            Some(m) if m.state.installed => AiStatus::Unavailable, // installed but server not running
            Some(m) => {
                // Check if dismissed
                if let Some(until) = m.state.dismissed_until {
                    if is_still_dismissed(until) {
                        return AiStatus::Unavailable;
                    }
                }
                AiStatus::Offer
            }
            None => AiStatus::Unavailable,
        }
    }
}

/// Returns the port the llama-server is listening on, if running.
#[cfg(not(debug_assertions))]
pub fn get_port() -> Option<u16> {
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.as_ref().and_then(|m| m.state.port)
}

/// Starts the AI download (binary + model).
#[tauri::command]
pub async fn start_ai_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        let _ = app;
        Ok(())
    }

    #[cfg(not(debug_assertions))]
    {
        do_download(&app).await
    }
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
        #[cfg(not(debug_assertions))]
        if let Some(pid) = m.child_pid.take() {
            stop_process(pid);
        }

        // Delete files
        #[cfg(not(debug_assertions))]
        {
            let _ = fs::remove_file(m.ai_dir.join(LLAMA_SERVER_BINARY));
            let _ = fs::remove_file(m.ai_dir.join(MODEL_FILENAME));
        }

        // Reset state
        m.state.installed = false;
        m.state.port = None;
        m.state.pid = None;
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

// --- Internal helpers ---

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

#[cfg(not(debug_assertions))]
fn is_installed_inner(m: &ManagerState) -> bool {
    let binary_exists = m.ai_dir.join(LLAMA_SERVER_BINARY).exists();
    let model_exists = m.ai_dir.join(MODEL_FILENAME).exists();
    binary_exists && model_exists
}

#[cfg(not(debug_assertions))]
fn is_still_dismissed(until_timestamp: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now < until_timestamp
}

#[cfg(not(debug_assertions))]
async fn do_download<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let ai_dir = get_ai_dir(app);
    fs::create_dir_all(&ai_dir).map_err(|e| format!("Failed to create AI directory: {e}"))?;

    // Reset cancel flag
    {
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut m) = *manager {
            m.cancel_requested = false;
        }
    }

    // Download llama-server archive (tar.gz)
    let archive_path = ai_dir.join(LLAMA_ARCHIVE_FILENAME);
    download_file(app, LLAMA_SERVER_URL, &archive_path, "llama-server").await?;

    // Extract llama-server binary from the archive
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    extract_llama_server(&archive_path, &binary_path)?;

    // Clean up the archive
    let _ = fs::remove_file(&archive_path);

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&binary_path, perms).map_err(|e| format!("Failed to set permissions: {e}"))?;
    }

    // Check if cancelled
    if is_cancel_requested() {
        cleanup_partial(&ai_dir);
        return Err(String::from("Download cancelled"));
    }

    // Download GGUF model
    let model_path = ai_dir.join(MODEL_FILENAME);
    download_file(app, MODEL_URL, &model_path, "model").await?;

    // Update state
    {
        let mut manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut m) = *manager {
            m.state.installed = true;
            save_state(&m.ai_dir, &m.state);
        }
    }

    // Emit install complete
    let _ = app.emit("ai-install-complete", ());

    // Start the server
    start_server_inner(app).await?;

    Ok(())
}

/// Extracts the llama-server binary from the downloaded tar.gz archive.
#[cfg(not(debug_assertions))]
fn extract_llama_server(archive_path: &Path, dest_path: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::Archive;

    let file = fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read archive entry: {e}"))?;
        let path = entry.path().map_err(|e| format!("Failed to get entry path: {e}"))?;

        // Look for the entry ending with "llama-server" (inside a versioned directory like "llama-b7815/")
        if path.file_name().is_some_and(|name| name == LLAMA_ARCHIVE_BINARY_SUFFIX) {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|e| format!("Failed to extract llama-server: {e}"))?;
            fs::write(dest_path, &contents).map_err(|e| format!("Failed to write llama-server binary: {e}"))?;
            return Ok(());
        }
    }

    Err(String::from("llama-server binary not found in downloaded archive"))
}

#[cfg(not(debug_assertions))]
async fn download_file<R: Runtime>(app: &AppHandle<R>, url: &str, dest: &Path, label: &str) -> Result<(), String> {
    use futures_util::StreamExt;

    let client = reqwest::Client::new();

    // Check for resume (existing partial file)
    let existing_size = dest.metadata().map(|m| m.len()).unwrap_or(0);

    let mut request = client.get(url);
    if existing_size > 0 {
        request = request.header("Range", format!("bytes={existing_size}-"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Download failed ({label}): {e}"))?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("Download failed ({label}): HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().map(|cl| cl + existing_size).unwrap_or(0);

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dest)
        .map_err(|e| format!("Failed to open file ({label}): {e}"))?;

    let mut stream = response.bytes_stream();
    let mut downloaded = existing_size;
    let start_time = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        // Check cancel
        if is_cancel_requested() {
            return Err(String::from("Download cancelled"));
        }

        let chunk = chunk.map_err(|e| format!("Download error ({label}): {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Write error ({label}): {e}"))?;
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

#[cfg(not(debug_assertions))]
fn is_cancel_requested() -> bool {
    let manager = MANAGER.lock().unwrap_or_else(|e| e.into_inner());
    manager.as_ref().is_some_and(|m| m.cancel_requested)
}

#[cfg(not(debug_assertions))]
fn cleanup_partial(ai_dir: &Path) {
    let _ = fs::remove_file(ai_dir.join(LLAMA_SERVER_BINARY));
    let _ = fs::remove_file(ai_dir.join(LLAMA_ARCHIVE_FILENAME));
    let _ = fs::remove_file(ai_dir.join(MODEL_FILENAME));
}

#[cfg(not(debug_assertions))]
async fn start_server_inner<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let ai_dir = get_ai_dir(app);
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    let model_path = ai_dir.join(MODEL_FILENAME);

    if !binary_path.exists() || !model_path.exists() {
        return Err(String::from("AI files not found"));
    }

    // Find an available port
    let port = find_available_port().ok_or("No available port")?;

    // Spawn llama-server
    let child = std::process::Command::new(&binary_path)
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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start llama-server: {e}"))?;

    let pid = child.id();

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
    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if super::client::health_check(port).await {
            log::info!("llama-server healthy on port {port}");
            return Ok(());
        }
    }

    Err(String::from("llama-server failed to become healthy within 60s"))
}

#[cfg(not(debug_assertions))]
fn find_available_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

#[cfg(not(debug_assertions))]
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
}

#[cfg(not(debug_assertions))]
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
        assert_eq!(state.model_version, "falcon-h1r-7b-q4km");
        assert_eq!(state.dismissed_until, None);
    }

    #[test]
    fn test_state_serialization() {
        let state = AiState {
            installed: true,
            port: Some(52847),
            pid: Some(12345),
            model_version: String::from("falcon-h1r-7b-q4km"),
            dismissed_until: None,
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: AiState = serde_json::from_str(&json).unwrap();
        assert!(parsed.installed);
        assert_eq!(parsed.port, Some(52847));
        assert_eq!(parsed.pid, Some(12345));
    }

    #[test]
    fn test_get_ai_status_dev_mode() {
        // In dev mode (debug_assertions), status is always Available
        let status = get_ai_status();
        assert_eq!(status, AiStatus::Available);
    }
}
