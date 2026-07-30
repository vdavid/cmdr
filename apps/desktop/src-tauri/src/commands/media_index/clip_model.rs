//! Managing the CLIP model on disk: the settings install state, the on-demand download,
//! and the delete-and-reclaim. Semantic search itself is in [`search`].

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::kick_all_ready_passes_for;
use crate::media_index::clip;
use crate::media_index::gate;
use crate::media_index::scheduler::MediaScheduler;

/// The CLIP model's install state, for the settings download affordance. Crosses the IPC
/// boundary, so it derives `Serialize` + `specta::Type` (camelCase).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipModelStatus {
    /// Whether the device can run CLIP at all (Apple Silicon — the Neural Engine path).
    /// The download affordance hides on unsupported hardware.
    pub supported: bool,
    /// Whether both towers are installed on disk (ready for semantic search).
    pub installed: bool,
    /// Whether a real artifact is configured (a pinned, non-placeholder checksum). `false`
    /// means the model isn't published yet, so the UI shows "coming soon", not a download.
    pub configured: bool,
    /// The total download size in bytes, for the honest "~X MB" copy.
    pub download_bytes: u64,
}

/// Report the CLIP model install state for the settings download affordance. Cheap (a few
/// `is_dir` checks); still hops off the IPC thread to be safe.
#[tauri::command]
#[specta::specta]
pub async fn media_index_clip_model_status(app: AppHandle) -> Result<ClipModelStatus, String> {
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let supported = crate::ai::is_local_ai_supported();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(ClipModelStatus {
            supported,
            installed: clip::install::is_installed(&data_dir),
            configured: clip::install::CLIP_TOWERS
                .iter()
                .all(|t| t.sha256 != clip::install::PLACEHOLDER_SHA),
            download_bytes: clip::install::total_download_bytes(),
        })
    })
    .await
    .map_err(|e| format!("clip status task panicked: {e}"))?
}

/// Download + checksum-verify + install the CLIP towers on demand (plan M3, Decision 9),
/// then kick a pass so already-enriched images gain CLIP embeddings. Each tower is fetched
/// via the shared resumable HTTP GET (`ai::download`), verified against its pinned SHA-256
/// BEFORE unpacking (a truncated download never installs), and unzipped into the model dir.
/// The intermediate zip is removed after a successful unpack.
#[tauri::command]
#[specta::specta]
pub async fn media_index_download_clip_model(app: AppHandle) -> Result<(), String> {
    if !crate::ai::is_local_ai_supported() {
        return Err("CLIP semantic search needs Apple Silicon".to_string());
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let model_dir = clip::install::clip_model_dir(&data_dir);
    std::fs::create_dir_all(&model_dir).map_err(|e| format!("create model dir: {e}"))?;

    for tower in clip::install::CLIP_TOWERS {
        if tower.sha256 == clip::install::PLACEHOLDER_SHA {
            return Err("The CLIP model isn't published yet".to_string());
        }
        let zip_path = model_dir.join(tower.artifact);
        // Fetch (resumable); the shared GET emits generic download-progress events.
        crate::ai::download::download_file(&app, tower.url, &zip_path, || false).await?;
        // Verify + unzip OFF the IPC thread (a blocking hash + extract).
        let (zip, sha, mdir) = (zip_path.clone(), tower.sha256, model_dir.clone());
        tauri::async_runtime::spawn_blocking(move || {
            clip::install::install_tower(&zip, sha, &mdir).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("clip install task panicked: {e}"))??;
        let _ = std::fs::remove_file(&zip_path);
    }

    // Newly installed ⇒ every already-enriched image is CLIP-stale: kick the ready passes so
    // they embed CLIP now (Vision stays current — two-part staleness), like a threshold drop.
    if gate::is_enabled() {
        kick_all_ready_passes_for(&app);
    }
    Ok(())
}

/// Delete the installed CLIP model and reclaim its disk: remove the on-disk model
/// artifacts, then prune every enriched volume's `media_clip_embedding` rows (resetting
/// each `clip_stamp` so a later re-download re-embeds) and `VACUUM` to free the pages.
/// Vision data (OCR, tags, feature print) is untouched — semantic search and Vision are
/// independent halves, so this returns the CLIP model status to `configured`/`supported`
/// (installed → false) while keeping keyword + tag search working. Runs OFF the IPC
/// thread (it blocks on each volume's writer). Idempotent: with nothing installed and
/// nothing enriched it removes any stray artifacts and returns.
#[tauri::command]
#[specta::specta]
pub async fn media_index_delete_clip_model(app: AppHandle) -> Result<(), String> {
    // No scheduler yet (nothing enriched) ⇒ just remove any on-disk artifacts.
    let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>() else {
        let data_dir = crate::config::resolved_app_data_dir(&app)?;
        let model_dir = clip::install::clip_model_dir(&data_dir);
        if model_dir.exists() {
            std::fs::remove_dir_all(&model_dir).map_err(|e| format!("delete clip model: {e}"))?;
        }
        return Ok(());
    };
    let scheduler = Arc::clone(scheduler.inner());
    tauri::async_runtime::spawn_blocking(move || {
        scheduler.delete_clip_model();
    })
    .await
    .map_err(|e| format!("delete-clip-model task panicked: {e}"))
}
