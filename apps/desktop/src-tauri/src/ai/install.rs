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
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;
use tokio::sync::Notify;

/// Stale partial downloads older than this are cleaned up at app start.
const STALE_PARTIAL_SECONDS: u64 = 24 * 60 * 60; // 24 hours

/// Wakes every start waiting for a cancelled download to wind down (see [`StartDecision`]).
static DOWNLOAD_WOUND_DOWN: LazyLock<Notify> = LazyLock::new(Notify::new);

/// What a start request does, given the manager's download bookkeeping.
///
/// ⚠️ **The latest choice wins.** Picking Local, then Off, then Local again quickly used to end
/// with NO download: the re-pick arrived while the cancelled download was still winding down,
/// answered "already in progress", and the cancelled one then stopped. Now the re-pick waits for
/// the wind-down and runs its own download, unless a newer cancel lands while it waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartDecision {
    /// Nothing is running: this call runs the download (and has claimed it).
    Run,
    /// A download is running and nobody cancelled it: this call has nothing to add.
    AlreadyRunning,
    /// A cancelled download is still winding down: wait for it, then decide again.
    WaitForWindDown,
    /// A cancel arrived after this call did: the latest choice is no download.
    CancelledWhileWaiting,
}

/// Decides a start request against the manager, claiming the download when it runs.
/// `cancels_seen` is [`ManagerState::download_cancels`] as it was when the request arrived.
fn decide_start(m: &mut ManagerState, cancels_seen: u64) -> StartDecision {
    if m.download_cancels != cancels_seen {
        return StartDecision::CancelledWhileWaiting;
    }
    if !m.download_in_progress {
        // Claimed and un-cancelled under one lock, so a cancel landing right after this is honored
        // by the download it's aimed at rather than wiped by a later reset.
        m.download_in_progress = true;
        m.cancel_requested = false;
        return StartDecision::Run;
    }
    if m.cancel_requested {
        StartDecision::WaitForWindDown
    } else {
        StartDecision::AlreadyRunning
    }
}

/// Records a cancel. Counted, so a start waiting on a wind-down can tell a newer cancel from the
/// one it's waiting out.
fn request_download_cancel(m: &mut ManagerState) {
    m.cancel_requested = true;
    m.download_cancels += 1;
}

/// Records that a download ended (finished, failed, or cancelled), so the next start may run.
fn finish_download(m: &mut ManagerState) {
    m.download_in_progress = false;
}

/// Starts the AI download (binary + model).
#[tauri::command]
#[specta::specta]
pub async fn start_ai_download<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if !is_local_ai_supported() {
        return Err(String::from("Local AI not supported on this hardware"));
    }

    let cancels_seen = MANAGER.lock_ignore_poison().as_ref().map_or(0, |m| m.download_cancels);
    loop {
        // Registered before the state is read, so a wind-down between the two can't be missed.
        let wound_down = DOWNLOAD_WOUND_DOWN.notified();
        tokio::pin!(wound_down);
        wound_down.as_mut().enable();

        let decision = match *MANAGER.lock_ignore_poison() {
            Some(ref mut m) => decide_start(m, cancels_seen),
            None => StartDecision::Run,
        };
        match decision {
            StartDecision::Run => break,
            StartDecision::AlreadyRunning => {
                log::warn!("AI download: already in progress, ignoring duplicate request");
                return Ok(());
            }
            StartDecision::WaitForWindDown => {
                log::info!(
                    "AI download: Local was picked again while a cancelled download winds down; starting once it stops"
                );
                wound_down.await;
            }
            StartDecision::CancelledWhileWaiting => return Err(String::from("Download cancelled")),
        }
    }

    let result = do_download(&app).await;

    // Clear in-progress flag, then let a start waiting on this wind-down decide again.
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
            finish_download(m);
        }
    }
    DOWNLOAD_WOUND_DOWN.notify_waiters();

    result
}

/// Cancels an in-progress download.
#[tauri::command]
#[specta::specta]
pub fn cancel_ai_download() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        request_download_cancel(m);
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

    // Set the model ID we're installing. The cancel flag was already reset by `decide_start`, under
    // the same lock that claimed this download: resetting it here would wipe a cancel that landed
    // in between.
    {
        let mut manager = MANAGER.lock_ignore_poison();
        if let Some(ref mut m) = *manager {
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

    // Emit installing event so UI shows "Setting up AI…" while server starts
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

#[cfg(test)]
mod tests {
    use super::super::AiState;
    use super::super::state::new_manager_state;
    use super::*;
    use std::path::PathBuf;

    fn manager() -> ManagerState {
        new_manager_state(PathBuf::from("/nonexistent/ai"), AiState::default())
    }

    #[test]
    fn a_start_with_nothing_running_claims_the_download() {
        let mut m = manager();
        let seen = m.download_cancels;
        assert_eq!(decide_start(&mut m, seen), StartDecision::Run);
        assert!(m.download_in_progress);
    }

    #[test]
    fn a_duplicate_start_while_a_download_runs_has_nothing_to_add() {
        let mut m = manager();
        let seen = m.download_cancels;
        assert_eq!(decide_start(&mut m, seen), StartDecision::Run);
        assert_eq!(decide_start(&mut m, seen), StartDecision::AlreadyRunning);
    }

    /// Local, then Off, then Local again: the re-pick arrives while the cancelled download still
    /// winds down. Pre-fix it answered "already running" and returned, the cancelled download then
    /// stopped, and nothing ran at all.
    #[test]
    fn local_off_local_waits_out_the_cancelled_download_then_runs_its_own() {
        let mut m = manager();
        let first = m.download_cancels;
        assert_eq!(decide_start(&mut m, first), StartDecision::Run);
        request_download_cancel(&mut m);

        let repick = m.download_cancels;
        assert_eq!(decide_start(&mut m, repick), StartDecision::WaitForWindDown);

        finish_download(&mut m);
        assert_eq!(decide_start(&mut m, repick), StartDecision::Run);
        assert!(
            !m.cancel_requested,
            "the old cancel must not stop the download the re-pick just started"
        );
    }

    /// Local, Off, Local, Off: the second Off lands while the re-pick waits, and it's the newest
    /// choice, so the waiting start gives up without claiming anything.
    #[test]
    fn a_cancel_while_waiting_is_the_newer_choice() {
        let mut m = manager();
        let first = m.download_cancels;
        assert_eq!(decide_start(&mut m, first), StartDecision::Run);
        request_download_cancel(&mut m);

        let repick = m.download_cancels;
        assert_eq!(decide_start(&mut m, repick), StartDecision::WaitForWindDown);
        request_download_cancel(&mut m);

        finish_download(&mut m);
        assert_eq!(decide_start(&mut m, repick), StartDecision::CancelledWhileWaiting);
        assert!(!m.download_in_progress, "a start that gives up claims nothing");
    }
}
