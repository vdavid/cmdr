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

use super::clip;
use super::coverage;
use super::gate;
use super::network::config as network_config;
use super::read::{MediaIndex, OcrHit, SemanticHit, TagHit};
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

/// Resolve the effective hit cap: a caller `None` takes [`DEFAULT_LIMIT`], and any
/// caller value is clamped to [`MAX_LIMIT`].
fn resolve_limit(limit: Option<u32>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize
}

/// The minimal, honest per-volume enrichment state the search UI reads to voice its
/// own coverage (plan § Coverage honesty + per-volume state). Deliberately NOT a
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
    /// the honest denominator behind "12,000 of 38,900 images indexed" (plan §
    /// Honest progress). `None` when the volume's index isn't ready (offline / still
    /// scanning), so the UI voices that rather than a fabricated total. ETA math lives
    /// UI-side off `(enriched_count, qualifying_count)`.
    pub qualifying_count: Option<u64>,
    /// Whether this volume is opted into background network (SMB) enrichment. Only
    /// meaningful for network volumes; a local volume enriches by default when
    /// `enabled`, so the UI shows the opt-in toggle only for network volumes (network-enrichment UI).
    pub network_opt_in: bool,
    /// Whether this volume is marked "always index" (enrich regardless of the
    /// importance threshold). The per-folder overrides aren't summarized here.
    pub always_indexed: bool,
    /// Whether enrichment is paused because the volume disconnected mid-pass. Its
    /// coverage is intact and resumes on reconnect (never GC'd, never marked failed).
    pub paused: bool,
    /// Whether image indexing is DEFERRED on this volume because importance hasn't
    /// scored its folders yet: the master toggle is on, the drive index is ready, but
    /// importance has no data (fresh or a recompute still running). The scheduler
    /// enriches only override-covered folders until importance lands, then the
    /// unscored → scored bridge kicks the rest. The settings UI voices this honestly
    /// ("Working out which folders matter — image indexing starts right after")
    /// instead of the generic covered-count spinner, so a persistently-failing
    /// importance recompute surfaces as a visible wait rather than a silent "0 of N"
    /// (defer-until-scored: the residual risk must be VISIBLE, never silent).
    pub waiting_for_importance: bool,
    /// How many drive-index qualifying images fall in the folders COVERED at the
    /// current slider threshold — the honest denominator for the settings progress line
    /// "N of M in your covered folders", which can reach done at any slider position
    /// (unlike `qualifying_count`, the full volume total). `None` when importance hasn't
    /// scored the volume yet (the same `stored_coverage` single source as the reclaim
    /// numbers, so they never disagree).
    pub covered_qualifying_count: Option<u64>,
    /// How many STORED rows fall OUTSIDE current coverage — indexed under a broader past
    /// setting and kept searchable (the slider is forward-only). Drives the quiet
    /// kept-rows line "K more indexed from broader settings — still searchable", which
    /// composes with the reclaim line as one narrative. `None` when importance is
    /// unscored.
    pub kept_count: Option<u64>,
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
    volume_state(&app, &volume_id).await
}

/// The honest per-volume enrichment state — the shared derivation behind both the
/// `media_index_volume_state` command (the search UI) and the Ask Cmdr / MCP
/// `search_photos` tool (`mcp::executor::photos`). Generic over the Tauri runtime so
/// the agent tool dispatch (also generic) reuses this ONE source rather than deriving
/// coverage a second time (the reuse-the-core rule).
pub(crate) async fn volume_state<R: tauri::Runtime>(
    app: &AppHandle<R>,
    volume_id: &str,
) -> Result<MediaIndexVolumeState, String> {
    let enabled = gate::is_enabled();
    // The scheduler is `app.manage`d only once `media_index::scheduler::start` ran; a
    // missing state (e.g. an early call) honestly reads as "not enriching".
    let scheduler = app.try_state::<Arc<MediaScheduler>>().map(|s| Arc::clone(s.inner()));
    let indexing = scheduler.as_ref().is_some_and(|s| s.is_enriching(volume_id));

    let data_dir = crate::config::resolved_app_data_dir(app)?;
    let threshold = gate::importance_threshold();
    let scope = gate::scope();
    let vid = volume_id.to_string();
    // The threshold-aware stored-coverage split (`covered_qualifying_count` + `kept_count`)
    // needs the volume's OS mount root to map override/exclude config; resolving
    // it here (a reclaim-eligible enabled volume only) keeps the split `None` for a
    // volume that isn't background-enriched.
    let mount_root = resolve_enabled_volumes(std::slice::from_ref(&vid))
        .0
        .into_iter()
        .next()
        .map(|(_, mount)| mount);
    let (enriched_count, qualifying_count, importance_scored, coverage_counts) =
        tauri::async_runtime::spawn_blocking(move || {
            let enriched = MediaIndex::open(&data_dir, &vid)
                .enriched_count()
                .map_err(|e| e.to_string())?;
            // The honest denominator: how many images qualify per the drive index.
            // `None` when the index isn't registered (offline / still scanning).
            let qualifying = coverage::get_or_build(&vid).map(|c| c.total);
            // Whether importance has data for this volume — the same "has it scored?"
            // check the scheduler gates enrichment on (live weight rows OR a stamped
            // generation), so the deferred state can't disagree with the scheduler.
            let importance_scored = {
                use crate::importance::{ImportanceIndex, SignalSet};
                let index = ImportanceIndex::open(&data_dir, &vid, SignalSet::all());
                coverage::importance_scored(&index)
            };
            // The scope- and threshold-aware split (`None` unless the volume is
            // reclaim-eligible AND the partition is safe — the SAME single source as the
            // reclaim numbers, via `stored_coverage_counts`, so they never disagree).
            let coverage_counts = match (&scheduler, &mount_root) {
                (Some(scheduler), Some(mount)) => scheduler.stored_coverage_counts(&vid, mount, threshold, scope),
                _ => None,
            };
            Ok::<_, String>((enriched, qualifying, importance_scored, coverage_counts))
        })
        .await
        .map_err(|e| format!("media volume state task panicked: {e}"))??;

    // Deferred-on-importance: enabled, the index is ready (a real qualifying count), but
    // importance has no data yet, so enrichment waits on the recompute. Only in the
    // automatic scope — the narrow one never consults importance, so reporting a wait
    // there would voice a wait that isn't happening.
    let waiting_for_importance =
        enabled && scope.consults_importance() && qualifying_count.is_some() && !importance_scored;

    Ok(MediaIndexVolumeState {
        enabled,
        indexing,
        enriched_count,
        qualifying_count,
        network_opt_in: network_config::is_opted_in(volume_id),
        always_indexed: network_config::snapshot().always_index_volumes.contains(volume_id),
        paused: network_config::is_paused(volume_id),
        waiting_for_importance,
        covered_qualifying_count: coverage_counts.as_ref().map(|c| c.covered_qualifying),
        kept_count: coverage_counts.as_ref().map(|c| c.doomed_stored),
    })
}

/// The live preview behind the importance slider: across the ENABLED volumes in
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
    let scope = gate::scope();

    tauri::async_runtime::spawn_blocking(move || {
        // The enabled volumes + their OS mount roots, resolved by the ONE shared rule
        // (local always, SMB only when opted in, MTP / LocalExternal never); a requested
        // volume that isn't ready comes back `pending`.
        let (volumes, mut pending) = resolve_enabled_volumes(&volume_ids);

        let mut folders = 0u64;
        let mut images = 0u64;

        for (vid, mount_root) in &volumes {
            let Some(counts) = coverage::get_or_build(vid) else {
                // The drive index isn't ready ⇒ unknown for now.
                pending = true;
                continue;
            };
            // The automatic scope needs importance; the narrow one counts the chosen
            // folders alone, so an unscored volume is answerable there.
            let scores = match coverage::importance_scores(&data_dir, vid) {
                Some(scores) => scores,
                None if !scope.consults_importance() => std::collections::HashMap::new(),
                None => {
                    pending = true;
                    continue;
                }
            };
            // Override coverage is OS-path keyed; map each folder into OS space, as the
            // enrichment gate and the reclaim partition both do.
            let config = network_config::snapshot();
            let is_override = |folder: &str| config.covers(vid, &super::network::fetch::os_join(mount_root, folder));
            let (f, i) = coverage::covered_in_scope(&counts, &scores, threshold, scope, &is_override);
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

/// Resolve the ENABLED media-index volumes from `volume_ids`, each with the OS mount root
/// the stored (index) paths map into: a local volume (mount `/`), or an opted-in SMB volume
/// (its mount root). MTP and non-opted-in SMB are dropped. The `pending` flag is set when a
/// requested volume the user expects isn't ready (offline / not scanned, or
/// opted-in-but-unmounted SMB), so the caller can caveat the totals.
///
/// The ONE enabled-volume rule, shared by the covered-count preview, the reclaim preview,
/// the prune, and the per-volume state — so none of them can disagree about which volumes
/// count, or map a stored path into OS space differently.
fn resolve_enabled_volumes(volume_ids: &[String]) -> (Vec<(String, String)>, bool) {
    use crate::indexing::IndexVolumeKind;
    let kinds: std::collections::HashMap<String, IndexVolumeKind> =
        crate::indexing::ready_volumes_with_kind().into_iter().collect();
    let mounts: std::collections::HashMap<String, String> = crate::file_system::get_volume_manager()
        .list_volumes_with_handles()
        .into_iter()
        .map(|(id, vol)| (id, vol.root().to_string_lossy().into_owned()))
        .collect();
    let mut enabled = Vec::new();
    let mut pending = false;
    for vid in volume_ids {
        match kinds.get(vid) {
            // A local volume's index path == its OS path, so the mount root is `/`.
            Some(IndexVolumeKind::Local) => {
                let mount = mounts.get(vid).cloned().unwrap_or_else(|| "/".to_string());
                enabled.push((vid.clone(), mount));
            }
            // An opted-in SMB volume: needs its live mount root to map index paths back to
            // OS space; opted-in-but-unmounted is pending (its rows are reachable only on
            // reconnect).
            Some(IndexVolumeKind::Smb) if network_config::is_opted_in(vid) => match mounts.get(vid) {
                Some(mount) => enabled.push((vid.clone(), mount.clone())),
                None => pending = true,
            },
            // Not opted-in SMB / MTP / LocalExternal: never reclaimed here (nothing was
            // enriched — LocalExternal is skipped by the passes since its index paths are
            // mount-relative, so it has no stored rows to reclaim).
            Some(IndexVolumeKind::Smb) | Some(IndexVolumeKind::Mtp) | Some(IndexVolumeKind::LocalExternal) => {}
            // Requested but offline / not scanned: the user expects it, so it's pending.
            None => pending = true,
        }
    }
    (enabled, pending)
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
/// unreadable file. Local-only: enrichment covers local volumes, so classification trusts the
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
        scheduler::kick_all_ready_passes(&app);
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

// ── Per-file + per-folder index status (the quiet glanceable indicators) ─────

/// The index status of ONE file, as the file-icon overlay reads it. Serialized
/// camelCase across the IPC boundary; classification is entirely backend-side (the
/// frontend renders an icon per state, never re-deriving from mtime/size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FileIndexState {
    /// A `done` row whose `(mtime, size)` + analyze stamp are current — fully indexed.
    Indexed,
    /// A stored row whose live file changed since indexing (or the analyze engine
    /// bumped): it needs re-enrichment. The same `needs_enrichment` predicate a pass uses.
    Stale,
    /// A `failed` row (a broken/undecodable file). Won't progress on its own.
    Failed,
    /// An indexable image that the coverage gate would enrich, but which has no stored
    /// row yet, and NO pass is running for the volume — genuinely queued, indexing hasn't
    /// reached it.
    Pending,
    /// An indexable image with no stored row yet WHILE a pass is actively running for the
    /// volume: it's being worked on right now, not merely queued. Distinct from `Pending`
    /// so a file whose row was transiently GC'd during a re-enriching pass (a move/rename)
    /// reads "indexing now" rather than a false "never indexed."
    Indexing,
    /// An indexable image the coverage gate would NOT enrich: out of scope, below the
    /// importance threshold, or under an excluded folder.
    Excluded,
    /// Not an indexable media type (a video, a document, a folder, a RAW deferring to a
    /// JPEG sibling, or a file the index no longer holds) — the frontend renders no badge.
    NotApplicable,
}

/// One file's path + its index status. `path` echoes the request exactly (the frontend
/// keys its per-path map by it).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileIndexStatus {
    pub path: String,
    pub state: FileIndexState,
}

/// Classify the index status of each file in `paths` (in request order) on `volume_id`.
///
/// **Paths are in the volume's INDEX-path space** — for a local volume that equals the
/// OS path, which is what the file lists show; a network volume's mount-root mapping is a
/// later slice (the file overlay ships local-first). The classification is bounded to the
/// paths passed (the visible rows), so a per-path drive-index lookup is cheap.
///
/// Backend-owned classification (smart-backend/thin-frontend): the drive index supplies
/// each path's live `(mtime, size)` and whether it qualifies as an image (sibling-aware
/// `qualify_dir`), `media.db` supplies the stored row, and the coverage gate
/// (`local_should_enrich` + the live exclusion veto) decides `pending` vs `excluded` for
/// an un-enriched image. A stored row wins over the gate: an already-indexed image reads
/// `indexed`/`stale`/`failed` even if the current setting no longer covers it (the rows
/// stay searchable, forward-only).
#[tauri::command]
#[specta::specta]
pub async fn media_index_file_status(
    app: AppHandle,
    volume_id: String,
    paths: Vec<String>,
) -> Result<Vec<FileIndexStatus>, String> {
    // Feature off ⇒ no badges. The frontend gates the overlay on the setting; this is
    // defense in depth (mirrors the other commands' feature-off short-circuit).
    if !gate::is_enabled() {
        return Ok(paths
            .into_iter()
            .map(|path| FileIndexStatus {
                path,
                state: FileIndexState::NotApplicable,
            })
            .collect());
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let scope = gate::scope();
    let threshold = gate::importance_threshold();
    // The scheduler owns the backend that supplies the analyze provenance stamp (for the
    // `stale` check). A missing one (an early call before `start`) leaves the stamp `None`,
    // and classification falls back to comparing only `(mtime, size)` per row — an honest
    // degrade. The stamp itself is fetched INSIDE `spawn_blocking` (it can touch the Vision
    // framework), never on the IPC/async thread.
    let scheduler = app.try_state::<Arc<MediaScheduler>>().map(|s| Arc::clone(s.inner()));
    let vid = volume_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let stamp = scheduler.as_ref().map(|s| s.current_analysis_stamp());
        // A pass running for this volume ⇒ an un-rowed covered image is being worked on
        // NOW (`indexing`), not merely queued (`pending`). Cheap in-memory snapshot.
        let is_enriching = scheduler.as_ref().is_some_and(|s| s.is_enriching(&vid));
        classify_file_statuses(&data_dir, &vid, paths, scope, threshold, stamp.as_deref(), is_enriching)
    })
    .await
    .map_err(|e| format!("file-status task panicked: {e}"))
}

/// The coverage `scores` map the file-status gate consults, mirroring `pass_coverage`:
/// the automatic scope loads importance and filters to folders scoring at or above
/// `threshold` (what `local_should_enrich` checks membership against); the narrow scope
/// reads no importance at all (override-only, so `None`). An unscored volume in the
/// automatic scope is `None` too, matching the pass's defer-to-override-only behavior.
fn coverage_scores(
    data_dir: &std::path::Path,
    volume_id: &str,
    scope: gate::IndexScope,
    threshold: f64,
) -> Option<std::collections::HashMap<String, f64>> {
    if !scope.consults_importance() {
        return None;
    }
    coverage::importance_scores(data_dir, volume_id)
        .map(|scores| scores.into_iter().filter(|(_, score)| *score >= threshold).collect())
}

/// Resolve the inputs (qualifying-image walk + stored rows + coverage scores) and hand
/// off to the pure [`classify_all`]. Runs on the blocking worker.
fn classify_file_statuses(
    data_dir: &std::path::Path,
    volume_id: &str,
    paths: Vec<String>,
    scope: gate::IndexScope,
    threshold: f64,
    stamp: Option<&str>,
    is_enriching: bool,
) -> Vec<FileIndexStatus> {
    use super::scheduler::enrich::{ImageEntry, parent_dir, walk_image_entries_in_dirs};
    use super::store::{MediaStatusRow, media_db_path, open_read_connection, read_status};
    use std::collections::{HashMap, HashSet};

    // The qualifying images (sibling-aware, with live `(mtime, size)`) for exactly the
    // dirs the requested paths live in — a bounded, scoped index walk.
    let dirs: HashSet<String> = paths.iter().map(|p| parent_dir(p).to_string()).collect();
    let qualifying: HashMap<String, ImageEntry> = match crate::indexing::get_read_pool_for(volume_id) {
        Some(pool) => match pool.with_conn(|conn| walk_image_entries_in_dirs(conn, &dirs)) {
            Ok(Ok(entries)) => entries.into_iter().map(|e| (e.path.clone(), e)).collect(),
            _ => HashMap::new(),
        },
        None => HashMap::new(),
    };

    // Stored rows for exactly the requested paths (bounded; a per-path point lookup).
    let db_path = media_db_path(data_dir, volume_id);
    let stored: HashMap<String, MediaStatusRow> = if db_path.exists() {
        match open_read_connection(&db_path) {
            Ok(conn) => paths
                .iter()
                .filter_map(|p| read_status(&conn, p).ok().flatten().map(|row| (p.clone(), row)))
                .collect(),
            Err(_) => HashMap::new(),
        }
    } else {
        HashMap::new()
    };

    // Coverage inputs for the pending/excluded split (only consulted for un-enriched
    // images), threshold-filtered exactly as the enrichment gate sees them.
    let scores = coverage_scores(data_dir, volume_id, scope, threshold);
    let config = network_config::snapshot();

    classify_all(
        &paths,
        &qualifying,
        &stored,
        stamp,
        scores.as_ref(),
        &config,
        volume_id,
        is_enriching,
    )
}

/// The PURE classification core: one [`FileIndexStatus`] per input path, in order,
/// decided from already-resolved inputs (no index, DB, or app), so every state is
/// directly unit-testable.
#[allow(
    clippy::too_many_arguments,
    reason = "the classification inputs are all distinct and resolved once"
)]
fn classify_all(
    paths: &[String],
    qualifying: &std::collections::HashMap<String, scheduler::enrich::ImageEntry>,
    stored: &std::collections::HashMap<String, super::store::MediaStatusRow>,
    stamp: Option<&str>,
    scores: Option<&std::collections::HashMap<String, f64>>,
    config: &super::network::config::NetworkEnrichConfig,
    volume_id: &str,
    is_enriching: bool,
) -> Vec<FileIndexStatus> {
    paths
        .iter()
        .map(|path| FileIndexStatus {
            path: path.clone(),
            state: classify_one(
                path,
                qualifying.get(path),
                stored.get(path),
                stamp,
                scores,
                config,
                volume_id,
                is_enriching,
            ),
        })
        .collect()
}

/// Classify ONE path. Priority: a non-qualifying entry is `notApplicable`; otherwise a
/// stored row decides `failed`/`stale`/`indexed` (an indexed image reads as such even if
/// the current setting no longer covers it — forward-only); with no row, the coverage
/// gate splits a covered image into `indexing` (a pass is running now) vs `pending` (none
/// is), and an uncovered one into `excluded`.
#[allow(
    clippy::too_many_arguments,
    reason = "the classification inputs are all distinct and resolved once"
)]
fn classify_one(
    path: &str,
    entry: Option<&scheduler::enrich::ImageEntry>,
    stored: Option<&super::store::MediaStatusRow>,
    stamp: Option<&str>,
    scores: Option<&std::collections::HashMap<String, f64>>,
    config: &super::network::config::NetworkEnrichConfig,
    volume_id: &str,
    is_enriching: bool,
) -> FileIndexState {
    use super::store::{EnrichmentState, needs_enrichment};

    let Some(entry) = entry else {
        // Not a qualifying image (a video, document, folder, RAW+JPEG dup, or a path the
        // index no longer holds) — no badge.
        return FileIndexState::NotApplicable;
    };
    match stored {
        Some(row) => match row.state {
            EnrichmentState::Failed => FileIndexState::Failed,
            EnrichmentState::Done => {
                // The engine stamp defaults to the row's own when the scheduler is
                // unavailable, so a `None` stamp flags only `(mtime, size)` changes.
                let engine = stamp.unwrap_or(&row.engine_version);
                if needs_enrichment(Some(row), entry.mtime, entry.size, engine) {
                    FileIndexState::Stale
                } else {
                    FileIndexState::Indexed
                }
            }
        },
        None => {
            // No row yet: would a pass enrich it? The live exclusion veto beats coverage.
            let covered = !config.is_excluded(path) && scheduler::local_should_enrich(path, scores, config, volume_id);
            if !covered {
                FileIndexState::Excluded
            } else if is_enriching {
                // A pass is running for the volume, so this covered-but-unrowed image is
                // being worked on now (or its row was transiently GC'd mid-reenrich by a
                // move/rename), not merely queued.
                FileIndexState::Indexing
            } else {
                FileIndexState::Pending
            }
        }
    }
}

/// One folder's index coverage: the eligible denominator and accounted numerator, each a
/// subtree total (the folder plus all its descendants). Serialized camelCase; the
/// frontend derives the two-state badge (`accounted == eligible` vs `accounted <
/// eligible`, no badge when `eligible == 0`) and the `accounted/eligible` tooltip.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderCoverage {
    pub path: String,
    /// Images under this folder (subtree) the drive index says qualify for indexing.
    pub eligible: u64,
    /// Of those, how many have a stored `done`/`failed` row (both count as accounted).
    pub accounted: u64,
}

/// The eligible + accounted subtree counts for each folder in `folder_paths` (in request
/// order) on `volume_id`. Reads the per-volume rollups the aggregate maintains
/// ([`coverage::folder_coverage`]); never scans `media.db` per query. Folder paths are in
/// the volume's INDEX-path space (== OS path for a local volume), matching the stored
/// rows and the eligible cache.
#[tauri::command]
#[specta::specta]
pub async fn media_index_folder_coverage(
    app: AppHandle,
    volume_id: String,
    folder_paths: Vec<String>,
) -> Result<Vec<FolderCoverage>, String> {
    // Feature off ⇒ nothing is indexed, so no folder badges.
    if !gate::is_enabled() {
        return Ok(folder_paths
            .into_iter()
            .map(|path| FolderCoverage {
                path,
                eligible: 0,
                accounted: 0,
            })
            .collect());
    }
    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let vid = volume_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let db_path = super::store::media_db_path(&data_dir, &vid);
        // Seed the accounted aggregate in case the writer hasn't spawned this session.
        coverage::ensure_accounted_seeded(&vid, &db_path);
        let counts = coverage::folder_coverage(&vid, &folder_paths);
        folder_paths
            .into_iter()
            .zip(counts)
            .map(|(path, c)| FolderCoverage {
                path,
                eligible: c.eligible,
                accounted: c.accounted,
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("folder-coverage task panicked: {e}"))
}

pub mod policy;

#[cfg(test)]
mod tests;
