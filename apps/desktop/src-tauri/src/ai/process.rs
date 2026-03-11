//! llama-server process lifecycle management.

use super::extract::LLAMA_SERVER_BINARY;
use std::fs;
use std::path::Path;

/// Log file name for llama-server output (useful for debugging startup issues).
pub const SERVER_LOG_FILENAME: &str = "llama-server.log";

/// Spawns the llama-server process and returns its PID.
///
/// The caller is responsible for health checking and state management.
pub fn spawn_llama_server(ai_dir: &Path, model_filename: &str, port: u16, ctx_size: u32) -> Result<u32, String> {
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    let model_path = ai_dir.join(model_filename);

    log::debug!("AI server: checking files at {:?}", ai_dir);
    log::debug!(
        "AI server: binary exists={}, model exists={}",
        binary_path.exists(),
        model_path.exists(),
    );

    if !binary_path.exists() || !model_path.exists() {
        return Err(String::from("AI files not found"));
    }

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
        .current_dir(ai_dir)
        .arg("-m")
        .arg(&model_path)
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("-c")
        .arg(ctx_size.to_string())
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

    Ok(pid)
}

/// Reads the last N lines from the llama-server log file for diagnostics.
pub fn read_log_tail(ai_dir: &Path, lines: usize) -> String {
    let log_path = ai_dir.join(SERVER_LOG_FILENAME);
    let log_content = fs::read_to_string(&log_path).unwrap_or_default();
    log_content
        .lines()
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns some diagnostic info about the log file (line count, last line).
pub fn log_diagnostics(ai_dir: &Path) -> Option<(usize, String)> {
    let log_path = ai_dir.join(SERVER_LOG_FILENAME);
    let log_content = fs::read_to_string(&log_path).ok()?;
    let line_count = log_content.lines().count();
    let last_line = log_content.lines().last()?.to_string();
    Some((line_count, last_line))
}

/// Finds an available TCP port on localhost.
pub fn find_available_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
}

/// Sends SIGKILL to a process. Returns immediately (~0.5ms).
///
/// Use for fire-and-forget scenarios (app quit, orphan cleanup from previous sessions).
/// llama-server is stateless — SIGKILL is safe. macOS reclaims all GPU/Metal/mmap resources
/// on process death regardless of signal. The llama.cpp test suite itself uses SIGKILL.
pub fn kill_process(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

/// Sends SIGKILL and reaps the zombie in a background thread.
///
/// Use during normal operation (settings switch, explicit stop) so zombies don't
/// accumulate over long-running sessions.
pub fn kill_and_reap_in_background(pid: u32) {
    kill_process(pid);
    #[cfg(unix)]
    std::thread::spawn(move || unsafe {
        libc::waitpid(pid as i32, std::ptr::null_mut(), 0);
    });
}

/// Returns true if the process with the given PID is still running.
pub fn is_process_alive(pid: u32) -> bool {
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
