//! The query surface: OCR keyword search, tag-score filtering, CLIP semantic search,
//! find-similar, and near-duplicate clustering.
//!
//! Every one is thin (see [`super`]): resolve the data dir, open the
//! [`MediaIndex`] read API for the volume, and hand
//! off the query on a blocking worker. They answer from `media.db`, so an offline volume
//! (a NAS unplugged) still returns results.

use tauri::AppHandle;

use super::resolve_limit;
use cmdr_index::media_index::clip;
use cmdr_index::media_index::gate;
use cmdr_index::media_index::read::{MediaIndex, OcrHit, SemanticHit, TagHit};
use cmdr_index::media_index::vector::{DedupCluster, SimilarImage};

/// Search a volume's OCR text for `query`, returning up to `limit` hits (default
/// [`DEFAULT_LIMIT`](super::DEFAULT_LIMIT), capped at [`MAX_LIMIT`](super::MAX_LIMIT)),
/// each with a highlighted `snippet` — the "why matched" reason the results grid shows.
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
    // Feature off ⇒ no volume is enriched, so there's nothing to search; skip opening
    // `media.db` entirely (defense in depth — the frontend also hides the OCR section when
    // off, so this command never fires from there).
    if !gate::is_enabled() {
        return Ok(Vec::new());
    }
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

/// Find the images most similar to the one at `source_path` on `volume_id` (by
/// feature-print cosine), highest first, excluding the source (plan "find
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

/// Natural-language semantic image search (plan M3): encode `query` with the CLIP text
/// tower and return the up-to-`limit` images whose CLIP embeddings are closest by cosine —
/// the headline "search photos by description". Each hit is a snippet-less tile with a
/// "matched description" reason (the match is on the whole-image embedding, not text).
///
/// Runs OFF the IPC thread (`spawn_blocking`): the tokenize + warm-text-tower encode hops to
/// the CLIP worker thread, then a brute-force top-k over the resident CLIP cache. Returns an
/// empty list (never an error) when image indexing is off, semantic search is turned off,
/// no CLIP model is installed, or the volume has no CLIP embeddings — so the UI voices
/// coverage rather than failing.
#[tauri::command]
#[specta::specta]
pub async fn media_index_search_semantic(
    app: AppHandle,
    volume_id: String,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SemanticHit>, String> {
    if !gate::is_enabled() || !gate::semantic_search_enabled() || query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let limit = resolve_limit(limit);
    tauri::async_runtime::spawn_blocking(move || {
        // Encode the query to a CLIP text vector; a missing/unavailable model yields no hits.
        let Ok(query_vec) = clip::encode_text_query(&query) else {
            return Ok(Vec::new());
        };
        Ok(MediaIndex::open(&data_dir, &volume_id).search_semantic(&query_vec, limit))
    })
    .await
    .map_err(|e| format!("semantic search task panicked: {e}"))?
}
