//! The `record_visit` IPC command: the navigation-visit signal feeder (plan
//! Decision 3).
//!
//! The frontend's navigation-commit point calls this fire-and-forget when a pane
//! settles on a folder. It persists a compact per-volume visit count + last-visit
//! timestamp in `importance.db` (counts and timestamps only — no content,
//! local-only; privacy posture in `docs/security.md`). The scorer's visit-activity
//! signal reads this on the next recompute.
//!
//! Thin per the commands-layer rule: resolve the shared writer, hand off the
//! visit. Failure-silent by contract — a visit that can't be recorded must never
//! break or block navigation, so the command returns `Ok(())` even on a write
//! hiccup (it logs at debug). Local volumes only in M2 (a non-`root` volume id is
//! ignored).

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::indexing::ROOT_VOLUME_ID;
use crate::location::Location;

use super::scheduler::ImportanceScheduler;

/// Record that the user navigated into `location`. Fire-and-forget and
/// failure-silent: never blocks or breaks navigation.
///
/// M2 records only local (`root`) visits; SMB/MTP visit recording lands with
/// their scoring in M4. The write goes through the scheduler's SHARED long-lived
/// writer for the volume (one writer thread per DB — the subsystem invariant held
/// in spirit, not absorbed by WAL busy-timeouts), reached through Tauri managed
/// state. If the scheduler isn't managed yet (startup raced ahead of `start`),
/// the visit is silently dropped — the next navigation records it.
#[tauri::command]
#[specta::specta]
pub async fn record_visit(app: AppHandle, location: Location) -> Result<(), String> {
    // Local only in M2. A non-root volume id is silently ignored (its scoring,
    // and thus its visit signal, arrives in M4).
    if location.volume_id != ROOT_VOLUME_ID {
        return Ok(());
    }

    let Some(scheduler) = app.try_state::<Arc<ImportanceScheduler>>().map(|s| Arc::clone(&s)) else {
        log::debug!(target: "importance", "record_visit skipped (scheduler not managed yet)");
        return Ok(());
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Do the enqueue off the IPC thread (the shared writer's channel send is quick
    // but resolving/creating the writer can open a DB). A failure is swallowed to
    // `Ok(())`: the visit signal is best-effort, navigation never depends on it.
    let path = location.path.clone();
    let volume_id = location.volume_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let writer = scheduler.writer_for(&volume_id)?;
        writer.record_visit(&path, now)?;
        Ok::<(), super::store::ImportanceStoreError>(())
    })
    .await;

    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::debug!(target: "importance", "record_visit write failed: {e}"),
        Err(e) => log::debug!(target: "importance", "record_visit task panicked: {e}"),
    }
    Ok(())
}
