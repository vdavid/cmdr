//! Reclaiming disk: the settings preview of what falls outside the current setting, and
//! the user-explicit prune that deletes it. Both read the scheduler's single-source
//! `stored_coverage`, so the preview always counts exactly what the prune removes.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::resolve_enabled_volumes;
use crate::media_index::gate;
use crate::media_index::scheduler::MediaScheduler;

/// The reclaim-space preview behind the settings "delete the extra entries" line:
/// across the ENABLED volumes in `volume_ids`, how many stored image rows fall
/// inside the current setting vs outside it, and the bytes the outside set would free.
/// `totalStored = coveredStored + doomedCount` (the single-source partition invariant),
/// so the copy's "you have N indexed; your setting covers M; delete the extra K" always
/// adds up. `pending` is `true` when a requested enabled volume isn't ready (still
/// scanning, or importance hasn't scored it), so the UI hides the reclaim line rather
/// than proposing a destructive count off a lower bound.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimPreview {
    /// All stored image rows across the enabled volumes (`coveredStored + doomedCount`).
    pub total_stored: u64,
    /// Stored rows inside the current setting — they stay searchable.
    pub covered_stored: u64,
    /// Stored rows outside the current setting — what a prune would delete.
    pub doomed_count: u64,
    /// The content bytes the doomed rows hold (an honest "about" — `VACUUM` reclaims at
    /// least this on disk).
    pub estimated_bytes: u64,
    /// Whether some enabled requested volume's count is unknown (scanning / not yet
    /// scored), so the totals are a lower bound the UI must not act on.
    pub pending: bool,
}

/// What a reclaim prune freed: the rows deleted and the bytes reclaimed.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimResult {
    /// The image rows deleted across the enabled volumes.
    pub deleted_rows: u64,
    /// The content bytes freed (an "about" estimate; the toast voices it).
    pub freed_bytes: u64,
}

/// Preview the reclaim-space split across `volume_ids` at the CURRENT `threshold`.
/// Thin: resolves the enabled volumes and aggregates the scheduler's single-source
/// `stored_coverage` per volume (the doomed-row SELECTION is Rust-side, the same
/// precedence enrichment uses; only the byte SUM over the chosen set is a `media.db`
/// query). Runs OFF the IPC thread; answers offline from `media.db`.
#[tauri::command]
#[specta::specta]
pub async fn media_index_reclaim_preview(
    app: AppHandle,
    threshold: f64,
    volume_ids: Vec<String>,
) -> Result<ReclaimPreview, String> {
    let empty = ReclaimPreview {
        total_stored: 0,
        covered_stored: 0,
        doomed_count: 0,
        estimated_bytes: 0,
        pending: false,
    };
    // Feature off ⇒ nothing is enriched, so there's nothing to reclaim.
    if !gate::is_enabled() {
        return Ok(empty);
    }
    // The scheduler owns the data dir + the writer/read paths; a missing state (an early
    // call before `start`) honestly reads as pending (nothing enriched yet).
    let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>().map(|s| Arc::clone(s.inner())) else {
        return Ok(ReclaimPreview { pending: true, ..empty });
    };
    // The scope isn't a hypothetical the UI previews (unlike `threshold`, which the
    // slider passes at its live position), so it's read from the gate here.
    let scope = gate::scope();
    tauri::async_runtime::spawn_blocking(move || {
        let (volumes, mut pending) = resolve_enabled_volumes(&volume_ids);
        let mut total_stored = 0u64;
        let mut covered_stored = 0u64;
        let mut doomed_count = 0u64;
        let mut estimated_bytes = 0u64;
        for (vid, mount) in &volumes {
            match scheduler.stored_coverage(vid, mount, threshold, scope) {
                Some(cov) => {
                    total_stored += cov.surviving_stored + cov.doomed_stored;
                    covered_stored += cov.surviving_stored;
                    doomed_count += cov.doomed_stored;
                    estimated_bytes += scheduler.estimate_doomed_bytes(vid, &cov.doomed_paths);
                }
                // Importance hasn't scored this volume yet ⇒ can't partition safely.
                None => pending = true,
            }
        }
        Ok(ReclaimPreview {
            total_stored,
            covered_stored,
            doomed_count,
            estimated_bytes,
            pending,
        })
    })
    .await
    .map_err(|e| format!("reclaim-preview task panicked: {e}"))?
}

/// Prune the stored image rows OUTSIDE the current `threshold` across `volume_ids`
/// (reclaim). Thin: delegates to the scheduler's `prune_below_threshold` per volume,
/// which selects the doomed set Rust-side, deletes it through the volume's ONE writer
/// thread (the serialization guarantee), `VACUUM`s, and drops the vector + coverage
/// caches. A USER-EXPLICIT deletion (derives only from settings state), so it needs no
/// completed-scan edge. Runs OFF the IPC thread. Returns the rows deleted and bytes freed.
#[tauri::command]
#[specta::specta]
pub async fn media_index_prune_below_threshold(
    app: AppHandle,
    threshold: f64,
    volume_ids: Vec<String>,
) -> Result<ReclaimResult, String> {
    let empty = ReclaimResult {
        deleted_rows: 0,
        freed_bytes: 0,
    };
    if !gate::is_enabled() {
        return Ok(empty);
    }
    let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>().map(|s| Arc::clone(s.inner())) else {
        return Ok(empty);
    };
    // Same live-gate read as the preview, so the prune deletes exactly the set the
    // preview counted.
    let scope = gate::scope();
    tauri::async_runtime::spawn_blocking(move || {
        let (volumes, _pending) = resolve_enabled_volumes(&volume_ids);
        let mut deleted_rows = 0u64;
        let mut freed_bytes = 0u64;
        for (vid, mount) in &volumes {
            let outcome = scheduler.prune_below_threshold(vid, mount, threshold, scope);
            deleted_rows += outcome.deleted_rows;
            freed_bytes += outcome.freed_bytes;
        }
        Ok(ReclaimResult {
            deleted_rows,
            freed_bytes,
        })
    })
    .await
    .map_err(|e| format!("reclaim-prune task panicked: {e}"))?
}
