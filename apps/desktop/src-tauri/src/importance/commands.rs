//! The `record_visit` IPC command: the navigation-visit signal feeder (plan
//! Decision 3).
//!
//! The frontend's navigation-commit point calls this fire-and-forget when a pane
//! settles on a folder. It persists a compact per-volume visit count + last-visit
//! timestamp in `importance.db` (counts and timestamps only — no content,
//! local-only; privacy posture in `docs/security.md`). The scorer's visit-activity
//! signal reads this on the next recompute.
//!
//! Thin per the commands-layer rule: resolve the DB path, hand off to the writer.
//! Failure-silent by contract — a visit that can't be recorded must never break or
//! block navigation, so the command returns `Ok(())` even on a write hiccup (it
//! logs at debug). Local volumes only in M2 (a non-`root` volume id is ignored).

use tauri::AppHandle;

use crate::indexing::ROOT_VOLUME_ID;
use crate::location::Location;

use super::store::importance_db_path;
use super::writer::ImportanceWriter;

/// Record that the user navigated into `location`. Fire-and-forget and
/// failure-silent: never blocks or breaks navigation.
///
/// M2 records only local (`root`) visits; SMB/MTP visit recording lands with
/// their scoring in M4. The write goes through a short-lived [`ImportanceWriter`]
/// so it honors the one-writer-per-DB invariant even though the recompute
/// scheduler may hold its own writer at other times (each opens its own thread on
/// the same WAL DB; the busy-timeout absorbs brief contention).
#[tauri::command]
#[specta::specta]
pub async fn record_visit(app: AppHandle, location: Location) -> Result<(), String> {
    // Local only in M2. A non-root volume id is silently ignored (its scoring,
    // and thus its visit signal, arrives in M4).
    if location.volume_id != ROOT_VOLUME_ID {
        return Ok(());
    }

    let data_dir = match crate::config::resolved_app_data_dir(&app) {
        Ok(d) => d,
        Err(e) => {
            log::debug!(target: "importance", "record_visit skipped (no data dir): {e}");
            return Ok(());
        }
    };
    let db_path = importance_db_path(&data_dir, &location.volume_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // The write is quick, but do it off the IPC thread (it opens a DB + spawns a
    // short-lived writer thread). A failure is swallowed to `Ok(())`: the visit
    // signal is best-effort, and navigation must never depend on it.
    let path = location.path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let writer = ImportanceWriter::spawn(&db_path)?;
        writer.record_visit(&path, now)?;
        writer.flush_blocking()?;
        writer.shutdown();
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
