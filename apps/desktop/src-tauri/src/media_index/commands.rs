//! The media-index IPC commands: the OCR-search read surface (plan Decision 8).
//!
//! Thin per the commands-layer rule: resolve the app data dir, open the
//! [`MediaIndex`](super::read::MediaIndex) read API for the volume, and hand off the
//! query. `search/` reaches `media.db` ONLY through `MediaIndex` — this command is
//! that door, so no consumer takes a raw `rusqlite` dep on `media.db`.
//!
//! Query-time DB work runs OFF the synchronous IPC thread (`spawn_blocking`), since a
//! sync `#[tauri::command]` blocks the IPC handler (`src-tauri/CLAUDE.md`). The read
//! API answers from `media.db` directly, so it still returns results when the volume
//! is offline (a NAS unplugged) — proven by the read-API tests.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::coverage;
use super::gate;
use super::network::config as network_config;
use super::read::{MediaIndex, OcrHit, TagHit};
use super::scheduler::{self, MediaScheduler};
use super::vector::{DedupCluster, SimilarImage};

/// The default hit cap when the caller doesn't specify one, and the hard ceiling on
/// any caller-supplied limit (a photo-search grid never needs more, and it bounds the
/// query's work + payload).
const DEFAULT_LIMIT: u32 = 200;
const MAX_LIMIT: u32 = 1000;

/// Search a volume's OCR text for `query`, returning up to `limit` hits (default
/// [`DEFAULT_LIMIT`], capped at [`MAX_LIMIT`]), each with a highlighted `snippet` —
/// the "why matched" reason the results grid shows.
///
/// An empty/whitespace query, an un-enriched volume, or an offline/purged `media.db`
/// returns an empty list rather than erroring.
#[tauri::command]
#[specta::specta]
pub async fn media_index_search_ocr(
    app: AppHandle,
    volume_id: String,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<OcrHit>, String> {
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let limit = resolve_limit(limit);

    // Do the DB work off the IPC thread.
    tauri::async_runtime::spawn_blocking(move || {
        MediaIndex::open(&data_dir, &volume_id)
            .search_ocr(&query, limit)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("media OCR search task panicked: {e}"))?
}

/// Resolve the effective hit cap: a caller `None` takes [`DEFAULT_LIMIT`], and any
/// caller value is clamped to [`MAX_LIMIT`].
fn resolve_limit(limit: Option<u32>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize
}

/// The minimal, honest per-volume enrichment state the search UI reads to voice its
/// own coverage (plan M1 § Coverage honesty + per-volume state). Deliberately NOT a
/// progress percentage or ETA — those are a later milestone; this only lets the UI
/// tell apart "indexing is off", "still indexing", "indexed but empty result", and
/// "not indexed yet". Crosses the IPC boundary, so it derives `Serialize` +
/// `specta::Type` (camelCase).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaIndexVolumeState {
    /// Whether image indexing is enabled at all (the master toggle / gate). When
    /// `false`, no volume is enriched and the UI hints the user to turn it on.
    pub enabled: bool,
    /// Whether an enrichment pass is running for this volume right now. Drives the
    /// "still indexing images, results may be incomplete" honesty line.
    pub indexing: bool,
    /// How many images are already enriched (stored OCR rows) for this volume. `0`
    /// with `indexing == false` and `enabled == true` reads as "not indexed yet",
    /// distinct from a genuinely empty search result over a populated index.
    pub enriched_count: u64,
    /// How many images the drive index says QUALIFY for enrichment on this volume —
    /// the honest denominator behind "12,000 of 38,900 images indexed" (plan M2 §
    /// Honest progress). `None` when the volume's index isn't ready (offline / still
    /// scanning), so the UI voices that rather than a fabricated total. ETA math lives
    /// UI-side off `(enriched_count, qualifying_count)`.
    pub qualifying_count: Option<u64>,
    /// Whether this volume is opted into background network (SMB) enrichment. Only
    /// meaningful for network volumes; a local volume enriches by default when
    /// `enabled`, so the UI shows the opt-in toggle only for network volumes (M1.5b).
    pub network_opt_in: bool,
    /// Whether this volume is marked "always index" (enrich regardless of the
    /// importance threshold). The per-folder overrides aren't summarized here.
    pub always_indexed: bool,
    /// Whether enrichment is paused because the volume disconnected mid-pass. Its
    /// coverage is intact and resumes on reconnect (never GC'd, never marked failed).
    pub paused: bool,
}

/// Report the honest per-volume enrichment state for `volume_id`: the master toggle,
/// whether a pass is running now, and how many images are already enriched. The search
/// UI reads this to voice its own coverage rather than showing a confident-looking
/// empty result that's really "not indexed yet".
///
/// The count read runs off the IPC thread (`spawn_blocking`); the running-pass flag is
/// a cheap in-memory snapshot off the scheduler's coalescing coordinator. A volume with
/// no `media.db` (never enriched / offline) reports `enriched_count: 0`, never an error.
#[tauri::command]
#[specta::specta]
pub async fn media_index_volume_state(app: AppHandle, volume_id: String) -> Result<MediaIndexVolumeState, String> {
    let enabled = gate::is_enabled();
    // The scheduler is `app.manage`d only once `media_index::scheduler::start` ran; a
    // missing state (e.g. an early call) honestly reads as "not enriching".
    let indexing = app
        .try_state::<Arc<MediaScheduler>>()
        .is_some_and(|scheduler| scheduler.is_enriching(&volume_id));

    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let vid = volume_id.clone();
    let (enriched_count, qualifying_count) = tauri::async_runtime::spawn_blocking(move || {
        let enriched = MediaIndex::open(&data_dir, &vid)
            .enriched_count()
            .map_err(|e| e.to_string())?;
        // The honest denominator: how many images qualify per the drive index. `None`
        // when the index isn't registered (offline / still scanning).
        let qualifying = coverage::get_or_build(&vid).map(|c| c.total);
        Ok::<_, String>((enriched, qualifying))
    })
    .await
    .map_err(|e| format!("media volume state task panicked: {e}"))??;

    Ok(MediaIndexVolumeState {
        enabled,
        indexing,
        enriched_count,
        qualifying_count,
        network_opt_in: network_config::is_opted_in(&volume_id),
        always_indexed: network_config::snapshot().always_index_volumes.contains(&volume_id),
        paused: network_config::is_paused(&volume_id),
    })
}

/// Set (or clear) a volume's opt-in for background network (SMB) image enrichment
/// (plan M1.5). Off by default: turning on the master toggle does NOT auto-enrich
/// network volumes. Enabling kicks an immediate pass so the user sees progress without
/// waiting for the next scan completion. Live-applied (no restart); the frontend
/// persists `mediaIndex.networkVolumes` and calls this on change (M1.5b UI).
#[tauri::command]
#[specta::specta]
pub fn media_index_set_network_volume_enabled(app: AppHandle, volume_id: String, enabled: bool) {
    network_config::set_opted_in(&volume_id, enabled);
    if enabled
        && gate::is_enabled()
        && let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>()
    {
        scheduler::kick_network_pass(Arc::clone(scheduler.inner()), volume_id);
    }
}

/// Set (or clear) a whole-volume "always index" override: enrich regardless of the
/// importance threshold (a rarely-browsed NAS scores low, so without this its photos
/// defer forever — plan Decision 6). Enabling kicks an immediate pass. Live-applied;
/// the frontend persists `mediaIndex.alwaysIndexVolumes` and calls this on change.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_always_index_volume(app: AppHandle, volume_id: String, always: bool) {
    network_config::set_always_index_volume(&volume_id, always);
    if always
        && gate::is_enabled()
        && network_config::is_opted_in(&volume_id)
        && let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>()
    {
        scheduler::kick_network_pass(Arc::clone(scheduler.inner()), volume_id);
    }
}

/// Set (or clear) a folder "always index" override: every image at or under `folder`
/// (an absolute OS-mount path) enriches regardless of importance. Live-applied; the
/// frontend persists `mediaIndex.alwaysIndexFolders` and calls this on change.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_always_index_folder(folder: String, always: bool) {
    network_config::set_always_index_folder(&folder, always);
}

/// Set (or clear) a per-folder photo-search EXCLUSION: no image at or under `folder`
/// (an absolute path) is enriched (the privacy complement to the opt-in — plan M2 §
/// Privacy). A hard veto that beats any "always index" override. Live-applied; the
/// frontend persists `mediaIndex.excludedFolders` and calls this on change. Existing
/// rows for the folder stay until the next GC/rescan; the veto stops FUTURE enrichment.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_excluded_folder(folder: String, excluded: bool) {
    network_config::set_excluded_folder(&folder, excluded);
}

/// Set the folder-importance threshold the scheduler enriches by — the M2 settings
/// slider's typed value (`0.0..=1.0`, clamped), never a string (`no-string-matching`).
/// Below-threshold folders are deferred; an override still forces enrichment. Live-
/// applied; the frontend persists `mediaIndex.importanceThreshold` and calls this.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_importance_threshold(threshold: f64) {
    gate::set_importance_threshold(threshold);
}

/// The live preview behind the M2 importance slider: across the ENABLED volumes in
/// `volume_ids`, how many folders score at or above `threshold` and how many images
/// they hold ((importance ≥ `threshold`) AND volume opted-in — never a non-opted-in
/// SMB/MTP volume). `pending` is `true` when any requested enabled volume isn't ready
/// (still scanning, or importance hasn't scored it), so the UI voices "naspi still
/// scanning" instead of a confident wrong number. Debounce-friendly: the per-folder
/// image counts are cached, so a drag only re-runs the cheap importance filter.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CoveredCount {
    /// Folders scoring at or above the threshold across the enabled volumes.
    pub folders: u64,
    /// Qualifying images in those folders across the enabled volumes.
    pub images: u64,
    /// Whether some enabled volume's count is unknown (scanning / not yet scored), so
    /// the total is a lower bound the UI must caveat.
    pub pending: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn media_index_covered_count(
    app: AppHandle,
    threshold: f64,
    volume_ids: Vec<String>,
) -> Result<CoveredCount, String> {
    // Feature off ⇒ nothing is covered (the slider is disabled anyway).
    if !gate::is_enabled() {
        return Ok(CoveredCount {
            folders: 0,
            images: 0,
            pending: false,
        });
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;

    tauri::async_runtime::spawn_blocking(move || {
        use crate::indexing::IndexVolumeKind;
        // Typed kind per ready volume (never an id-string branch); an unlisted volume
        // is offline / not scanned.
        let kinds: std::collections::HashMap<String, IndexVolumeKind> =
            crate::indexing::ready_volumes_with_kind().into_iter().collect();

        let mut folders = 0u64;
        let mut images = 0u64;
        let mut pending = false;

        for vid in &volume_ids {
            // Enabled = master on (checked above) AND the scheduler would enrich this
            // volume: a local volume always, an SMB volume only when opted in, MTP
            // never (it's on-demand, not previewed).
            let enabled = match kinds.get(vid) {
                Some(IndexVolumeKind::Local) => true,
                Some(IndexVolumeKind::Smb) => network_config::is_opted_in(vid),
                Some(IndexVolumeKind::Mtp) | None => false,
            };
            if !enabled {
                // An offline / not-ready requested volume that the user expects to
                // count is pending; a genuinely-disabled one just contributes nothing.
                if !kinds.contains_key(vid) {
                    pending = true;
                }
                continue;
            }
            let (Some(counts), Some(scores)) =
                (coverage::get_or_build(vid), coverage::importance_scores(&data_dir, vid))
            else {
                // Index not ready or importance not scored yet ⇒ unknown for now.
                pending = true;
                continue;
            };
            let (f, i) = coverage::covered_for_volume(&counts, &scores, threshold);
            folders += f;
            images += i;
        }

        Ok(CoveredCount {
            folders,
            images,
            pending,
        })
    })
    .await
    .map_err(|e| format!("covered-count task panicked: {e}"))?
}

/// Find the images most similar to the one at `source_path` on `volume_id` (by
/// feature-print cosine), highest first, excluding the source (plan M2 "find
/// similar"). Runs OFF the IPC thread; answers from `media.db` + the resident vector
/// cache even when the volume is offline.
#[tauri::command]
#[specta::specta]
pub async fn media_index_find_similar(
    app: AppHandle,
    volume_id: String,
    source_path: String,
    limit: Option<u32>,
) -> Result<Vec<SimilarImage>, String> {
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let k = resolve_limit(limit);
    tauri::async_runtime::spawn_blocking(move || {
        MediaIndex::open(&data_dir, &volume_id)
            .find_similar(&source_path, k)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("find-similar task panicked: {e}"))?
}

/// The default and hard-cap cosine thresholds for the near-duplicate grouping.
const DEFAULT_DEDUP_THRESHOLD: f32 = 0.9;

/// Group `volume_id`'s images into near-duplicate clusters (feature-print cosine at or
/// above `threshold`, default [`DEFAULT_DEDUP_THRESHOLD`]). Runs OFF the IPC thread
/// over the resident vector cache.
#[tauri::command]
#[specta::specta]
pub async fn media_index_dedup_clusters(
    app: AppHandle,
    volume_id: String,
    threshold: Option<f32>,
) -> Result<Vec<DedupCluster>, String> {
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let threshold = threshold.unwrap_or(DEFAULT_DEDUP_THRESHOLD).clamp(-1.0, 1.0);
    tauri::async_runtime::spawn_blocking(move || Ok(MediaIndex::open(&data_dir, &volume_id).dedup_clusters(threshold)))
        .await
        .map_err(|e| format!("dedup task panicked: {e}"))?
}

/// The images on `volume_id` tagged `label` at or above `min_score` (default `0.0` =
/// any confidence), highest first — the structured tag-score filter alongside the FTS
/// keyword search. Runs OFF the IPC thread; answers offline from `media.db`.
#[tauri::command]
#[specta::specta]
pub async fn media_index_search_tag(
    app: AppHandle,
    volume_id: String,
    label: String,
    min_score: Option<f32>,
) -> Result<Vec<TagHit>, String> {
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let min_score = min_score.unwrap_or(0.0);
    tauri::async_runtime::spawn_blocking(move || {
        MediaIndex::open(&data_dir, &volume_id)
            .images_with_tag(&label, min_score)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("tag-search task panicked: {e}"))?
}

/// Mint a `cmdr-media://` token so the search-results grid can render an image's
/// thumbnail through the EXISTING viewer preview scheme (plan Decision 5 — reuse the
/// preview path, never a media_index-produced thumbnail file). Returns `None` when the
/// path isn't a renderable image (the grid then falls back to a plain tile).
///
/// Token lifetime is the CALLER's here: a viewer session drops its token at the
/// window-close choke point, but the grid has none, so the frontend MUST drop every
/// token via [`media_index_drop_thumbnail_tokens`] when it re-renders or closes, or the
/// token map leaks path mappings. Runs off the IPC thread (a small header read + magic-
/// byte classification).
#[tauri::command]
#[specta::specta]
pub async fn media_index_thumbnail_token(path: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || mint_image_thumbnail_token(&path))
        .await
        .map_err(|e| format!("thumbnail token task panicked: {e}"))
}

/// Drop `cmdr-media://` tokens the grid minted via [`media_index_thumbnail_token`], once
/// they're no longer displayed. Idempotent; an unknown token is a no-op. Sync + trivial
/// (a map removal per token), so it needn't hop off the IPC thread.
#[tauri::command]
#[specta::specta]
pub fn media_index_drop_thumbnail_tokens(tokens: Vec<String>) {
    for token in &tokens {
        crate::file_viewer::media::drop_token(token);
    }
}

/// Classify `path` by magic bytes and, if it's an image, mint a `cmdr-media://` token
/// for it (reusing the viewer's token registry + scheme). `None` for a non-image or an
/// unreadable file. Local-only: M1 enriches local volumes, so classification trusts the
/// local extension fast-path the viewer uses.
fn mint_image_thumbnail_token(path: &str) -> Option<String> {
    use crate::file_viewer::content_kind::{CLASSIFY_HEAD_LEN, ViewerContentKind, classify_viewer_content, media_mime};
    use crate::file_viewer::media::{self, MediaEntry};

    let p = std::path::Path::new(path);
    let head = read_head_bytes(p, CLASSIFY_HEAD_LEN);
    let ext = p.extension().and_then(|e| e.to_str());
    let kind = classify_viewer_content(&head, ext, true);
    if kind != ViewerContentKind::Image {
        return None;
    }
    let mime = media_mime(&head, kind)
        .unwrap_or("application/octet-stream")
        .to_string();
    Some(media::mint_token(MediaEntry {
        canonical_path: p.to_path_buf(),
        kind,
        mime,
    }))
}

/// Read up to `max` leading bytes for magic-byte classification. Best-effort: an
/// unreadable file yields an empty slice (classified as non-image, so no token).
fn read_head_bytes(path: &std::path::Path, max: usize) -> Vec<u8> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = vec![0u8; max];
    match file.read(&mut buf) {
        Ok(n) => {
            buf.truncate(n);
            buf
        }
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
