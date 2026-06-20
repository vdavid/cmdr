//! Acquiring and removing the on-disk AI model + binary.
//!
//! Drives the download → verify → first-launch install sequence (`start_ai_download` /
//! `do_download`), its cancellation, the stale-partial cleanup, and `uninstall_ai`.
//! The llama-server *process* lifecycle lives in [`super::server`]; this module only
//! puts the files in place (or removes them) and then hands off to the server module
//! for the initial health-checked launch.

use super::download::{cleanup_partial, download_file};
use super::extract::{LLAMA_SERVER_BINARY, extract_bundled_llama_server};
use super::process::kill_and_reap_in_background;
use super::server::{StartupOutcome, spawn_and_track_server, wait_for_server_health};
use super::state::{MANAGER, ManagerState, get_ai_dir, get_current_model, save_state};
use super::{
    AiExtracting, AiInstallComplete, AiInstalling, AiVerifying, get_default_model, get_model_by_id,
    is_local_ai_supported,
};
use crate::ignore_poison::IgnorePoison;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;

/// Stale partial downloads older than this are cleaned up at app start.
const STALE_PARTIAL_SECONDS: u64 = 24 * 60 * 60; // 24 hours

/// Starts the AI download (binary + model).
#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub fn cancel_ai_download() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        m.cancel_requested = true;
    }
}

/// Uninstalls the AI model and binary, resets state.
/// Async because file deletion may block briefly.
#[tauri::command]
#[specta::specta]
pub async fn uninstall_ai() {
    tauri::async_runtime::spawn_blocking(uninstall_ai_sync).await.ok();
}

fn uninstall_ai_sync() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        // Stop server if running
        if let Some(pid) = m.child_pid.take() {
            kill_and_reap_in_background(pid);
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

/// Cleans up stale partial downloads older than 24 hours.
pub(super) fn cleanup_stale_partial_download(m: &mut ManagerState) {
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
        let _ = AiExtracting.emit(app);
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

    // Step 3: Verify download integrity by checking file size
    let _ = AiVerifying.emit(app);
    let actual_size = fs::metadata(&model_path)
        .map(|m| m.len())
        .map_err(|e| format!("Failed to read downloaded model file: {e}"))?;

    if actual_size < model.size_bytes {
        crate::log_error!(
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
    let _ = AiInstalling.emit(app);

    // Start the server FIRST, then emit install complete.
    // Spawn synchronously so PID is tracked immediately, then health-check async.
    let (pid, port, cancel) = {
        let mut manager = MANAGER.lock_ignore_poison();
        let Some(ref mut m) = *manager else {
            return Err(String::from("AI manager not initialized"));
        };
        spawn_and_track_server(m)?
    };
    match wait_for_server_health(&ai_dir, pid, port, cancel).await {
        StartupOutcome::Ready => {}
        // The user switched away mid-install. The model is installed and state saved;
        // the server start was deliberately abandoned, so this isn't an install failure.
        StartupOutcome::Cancelled => return Ok(()),
        StartupOutcome::Failed(e) => return Err(e),
    }

    // Emit install complete only after server is healthy
    let _ = AiInstallComplete.emit(app);

    Ok(())
}

fn is_cancel_requested() -> bool {
    let manager = MANAGER.lock_ignore_poison();
    manager.as_ref().is_some_and(|m| m.cancel_requested)
}
