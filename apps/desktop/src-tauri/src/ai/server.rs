//! Starting, health-checking, and stopping the local llama-server.
//!
//! The stateful layer above the stateless [`super::process`] syscalls: it spawns the
//! server inside the `MANAGER` lock (so `child_pid` is tracked with no race window),
//! health-checks it asynchronously, and keeps the `child_pid` / `start_cancel` /
//! `server_starting` fields coherent across rapid provider switches. The
//! quiet-stop-vs-real-failure protocol ([`StartupOutcome`] + the cancel token) lives
//! here. See `ai/DETAILS.md` § Startup flow.

use super::extract::{LLAMA_SERVER_BINARY, extract_bundled_llama_server};
use super::process::{
    SERVER_LOG_FILENAME, find_available_port, is_process_alive, kill_and_reap_in_background, kill_stale_llama_servers,
    log_diagnostics, read_log_tail, spawn_llama_server,
};
use super::state::{MANAGER, ManagerState, get_ai_dir, is_fully_installed, save_state};
use super::{AiServerReady, AiStarting, get_default_model, get_model_by_id, is_local_ai_supported};
use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;
use std::path::Path;
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;
use tokio_util::sync::CancellationToken;

/// Stops the local llama-server without uninstalling.
#[tauri::command]
#[specta::specta]
pub fn stop_ai_server() {
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager {
        // Cancel an in-flight startup first, so its waiter sees an intentional stop
        // rather than a crash.
        if let Some(token) = m.start_cancel.take() {
            token.cancel();
        }
        if let Some(pid) = m.child_pid.take() {
            log::info!("AI: stopping server (PID {pid})");
            kill_and_reap_in_background(pid);
            m.state.port = None;
            m.state.pid = None;
            save_state(&m.ai_dir, &m.state);
        }
    }
}

/// Starts the local llama-server with the given context size.
/// Spawns the server in a background task and returns immediately.
#[tauri::command]
#[specta::specta]
pub fn start_ai_server<R: Runtime>(app: AppHandle<R>, ctx_size: u32) -> Result<(), String> {
    if !is_local_ai_supported() {
        return Err(String::from("Local AI not supported on this hardware"));
    }

    // Recovery: re-extract binary if missing (before acquiring lock)
    let ai_dir = get_ai_dir(&app);
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    if !binary_path.exists() {
        log::debug!("AI manager: binary missing, attempting re-extraction...");
        extract_bundled_llama_server(&app, &ai_dir)?;
    }

    let spawn_result;
    {
        let mut manager = MANAGER.lock_ignore_poison();
        let Some(ref mut m) = *manager else {
            return Err(String::from("AI manager not initialized"));
        };
        m.context_size = ctx_size;

        spawn_result = if is_fully_installed(m) && m.child_pid.is_none() {
            match spawn_and_track_server(m) {
                Ok((pid, port, cancel)) => {
                    m.server_starting = true;
                    Some((pid, port, cancel))
                }
                Err(e) => return Err(e),
            }
        } else {
            None
        };
    }

    if let Some((pid, port, cancel)) = spawn_result {
        let _ = AiStarting.emit(&app);
        tauri::async_runtime::spawn(async move {
            handle_startup_outcome(wait_for_server_health(&ai_dir, pid, port, cancel).await, pid, &app);
        });
    }

    Ok(())
}

/// Logs a startup outcome at the right severity, emits `AiServerReady` only on success, and
/// clears `server_starting` unless a newer startup has taken over the slot. A `Cancelled`
/// outcome (provider switched away, server stopped, or superseded) is deliberately quiet:
/// no ERROR, no event. That's the normal case when someone toggles local AI on and off.
pub(super) fn handle_startup_outcome<R: Runtime>(outcome: StartupOutcome, pid: u32, app: &AppHandle<R>) {
    match outcome {
        StartupOutcome::Ready => {
            log::info!("AI: server ready");
            let _ = AiServerReady.emit(app);
        }
        StartupOutcome::Cancelled => {
            log::debug!("AI server: startup cancelled (provider switched or server stopped)");
        }
        StartupOutcome::Failed(e) => crate::log_error!("AI manager: server didn't start: {e}"),
    }

    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager
        && startup_task_owns_slot(m.child_pid, pid)
    {
        m.server_starting = false;
    }
}

/// Spawns llama-server and immediately tracks its PID in manager state.
/// Must be called while holding the MANAGER lock.
/// Returns (pid, port) for the caller to health-check asynchronously.
pub(super) fn spawn_and_track_server(m: &mut ManagerState) -> Result<(u32, u16, CancellationToken), String> {
    let model = get_model_by_id(&m.state.installed_model_id).unwrap_or_else(get_default_model);
    let port = find_available_port().ok_or("No available port")?;

    log::debug!(
        "AI server: starting llama-server on port {port} with context size {}",
        m.context_size
    );

    // Supersede any previous in-flight startup: its health-check waiter should exit
    // quietly rather than report the now-orphaned PID's death as a failure.
    if let Some(token) = m.start_cancel.take() {
        token.cancel();
    }

    // Belt-and-suspenders: stop any stale llama-servers before spawning a new one
    kill_stale_llama_servers(&m.ai_dir);

    let pid = spawn_llama_server(&m.ai_dir, model.filename, port, m.context_size)?;

    // Track PID immediately (no race window where a process exists untracked)
    let cancel = CancellationToken::new();
    m.start_cancel = Some(cancel.clone());
    m.child_pid = Some(pid);
    m.state.port = Some(port);
    m.state.pid = Some(pid);
    save_state(&m.ai_dir, &m.state);

    Ok((pid, port, cancel))
}

/// Outcome of waiting for a freshly-spawned llama-server to become healthy.
pub(super) enum StartupOutcome {
    /// The server passed its health check and is ready.
    Ready,
    /// Startup was intentionally cancelled: the provider was switched away, the server was
    /// stopped, or a newer spawn superseded this one. Not a failure: the caller logs it
    /// quietly and emits no "ready" event.
    Cancelled,
    /// The server genuinely failed to start (crashed, or never became healthy in time).
    Failed(String),
}

/// A finished startup task should clear `server_starting` only if it still owns the slot:
/// either it's the current child, or no child is tracked (the server was stopped or the
/// process failed and was cleaned up). If a newer startup has taken over (`child_pid` now
/// points at a different process), that task owns the flag and this one must leave it alone.
fn startup_task_owns_slot(current_child: Option<u32>, my_pid: u32) -> bool {
    current_child.is_none_or(|current| current == my_pid)
}

/// Waits for the server to become healthy (polls every 500ms, up to 60s). Returns early and
/// quietly if `cancel` fires (an intentional stop or supersede). On genuine failure or
/// timeout, kills the process and clears state.
pub(super) async fn wait_for_server_health(
    ai_dir: &Path,
    pid: u32,
    port: u16,
    cancel: CancellationToken,
) -> StartupOutcome {
    // Brief pause to let the process initialize. A `biased` select checks cancellation first,
    // so an intentional stop during this window never gets misread as a crash below.
    tokio::select! {
        biased;
        () = cancel.cancelled() => return StartupOutcome::Cancelled,
        () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
    }
    if !is_process_alive(pid) {
        cleanup_failed_server(pid);
        let last_lines = read_log_tail(ai_dir, 20);
        crate::log_error!("AI server: process died immediately. Last log lines:\n{last_lines}");
        let log_path = ai_dir.join(SERVER_LOG_FILENAME);
        return StartupOutcome::Failed(format!("llama-server crashed on startup. Check log at: {log_path:?}"));
    }

    log::debug!("AI server: waiting for health check on port {port}...");
    for i in 0..120 {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return StartupOutcome::Cancelled,
            () = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
        }

        if !is_process_alive(pid) {
            // A genuine death: an intentional stop would have cancelled the token, and the
            // biased select above would have returned `Cancelled` before reaching here.
            cleanup_failed_server(pid);
            let last_lines = read_log_tail(ai_dir, 20);
            crate::log_error!("AI server: process died during startup. Last log lines:\n{last_lines}");
            return StartupOutcome::Failed(format!("llama-server process (PID {pid}) died during startup"));
        }

        if super::client::health_check(port).await {
            log::debug!("AI server: healthy on port {port} after {}s", (i + 1) / 2);
            return StartupOutcome::Ready;
        }

        if i % 10 == 9 {
            log::debug!("AI server: still waiting for health check ({}s)...", (i + 1) / 2);
            if let Some((line_count, last_line)) = log_diagnostics(ai_dir) {
                log::debug!(
                    "AI server: log has {}, last: {last_line}",
                    pluralize(line_count as u64, "line")
                );
            }
        }
    }

    // Timed out; kill the process instead of leaving it orphaned
    cleanup_failed_server(pid);
    let last_lines = read_log_tail(ai_dir, 20);
    crate::log_error!("AI server: health check timed out. Last log lines:\n{last_lines}");
    StartupOutcome::Failed(String::from("llama-server failed to become healthy within 60s"))
}

/// Kills a server process and clears its tracking state.
/// Only clears state if the tracked PID still matches (avoids clobbering a newer spawn).
fn cleanup_failed_server(pid: u32) {
    kill_and_reap_in_background(pid);
    let mut manager = MANAGER.lock_ignore_poison();
    if let Some(ref mut m) = *manager
        && m.child_pid == Some(pid)
    {
        m.child_pid = None;
        m.state.port = None;
        m.state.pid = None;
        save_state(&m.ai_dir, &m.state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PID above macOS's default `PID_MAX` (99999), so it's never a live process. Positive
    /// as an `i32`, so `kill(pid, 0)` reports "no such process" rather than the `-1` broadcast.
    const DEAD_PID: u32 = 999_999;
    const UNUSED_DIR: &str = "/nonexistent-cmdr-startup-test-dir";

    #[tokio::test]
    async fn wait_for_server_health_reports_cancelled_not_failed_when_token_fired() {
        // The race this guards: switching the AI provider off mid-startup kills the server
        // process, so the waiter sees a dead PID. Because the startup was cancelled it must
        // report `Cancelled` (silent), NOT `Failed` (which logs an ERROR and auto-sends a
        // report). DEAD_PID is already gone, so without the cancel check the death branch
        // would fire and return `Failed`.
        let cancel = CancellationToken::new();
        cancel.cancel();
        let outcome = wait_for_server_health(Path::new(UNUSED_DIR), DEAD_PID, 1, cancel).await;
        assert!(
            matches!(outcome, StartupOutcome::Cancelled),
            "an intentional stop must be reported quietly, not as a failure"
        );
    }

    #[tokio::test]
    async fn wait_for_server_health_reports_failed_on_genuine_death() {
        // A real crash (process gone, startup NOT cancelled) must still surface as `Failed`,
        // so the cancel fix doesn't suppress genuine startup failures.
        let cancel = CancellationToken::new(); // never cancelled
        let outcome = wait_for_server_health(Path::new(UNUSED_DIR), DEAD_PID, 1, cancel).await;
        assert!(
            matches!(outcome, StartupOutcome::Failed(_)),
            "a genuine death must still be reported as a failure"
        );
    }

    #[test]
    fn startup_task_owns_slot_only_when_current_or_unset() {
        // No child tracked (server stopped, or failed and cleaned up): the finishing task
        // owns the slot and may clear `server_starting`.
        assert!(startup_task_owns_slot(None, 42));
        // Still the current child: we own it.
        assert!(startup_task_owns_slot(Some(42), 42));
        // A newer startup took over (different PID): that task owns the flag, not us.
        assert!(!startup_task_owns_slot(Some(43), 42));
    }
}
