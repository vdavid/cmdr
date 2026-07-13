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

use super::gate;
use super::read::{MediaIndex, OcrHit};
use super::scheduler::MediaScheduler;

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
    let enriched_count = tauri::async_runtime::spawn_blocking(move || {
        MediaIndex::open(&data_dir, &vid)
            .enriched_count()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("media volume state task panicked: {e}"))??;

    Ok(MediaIndexVolumeState {
        enabled,
        indexing,
        enriched_count,
    })
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
