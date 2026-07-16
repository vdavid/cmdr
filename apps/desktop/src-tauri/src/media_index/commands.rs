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
    /// (plan M1: the residual risk must be VISIBLE, never silent).
    pub waiting_for_importance: bool,
    /// How many drive-index qualifying images fall in the folders COVERED at the
    /// current slider threshold — the honest denominator for the settings progress line
    /// "N of M in your covered folders", which can reach done at any slider position
    /// (unlike `qualifying_count`, the full volume total). `None` when importance hasn't
    /// scored the volume yet (the same `stored_coverage` single source as M4's reclaim
    /// numbers, so they never disagree; plan M5).
    pub covered_qualifying_count: Option<u64>,
    /// How many STORED rows fall OUTSIDE current coverage — indexed under a broader past
    /// setting and kept searchable (the slider is forward-only). Drives the quiet
    /// kept-rows line "K more indexed from broader settings — still searchable", which
    /// composes with M4's reclaim line as one narrative. `None` when importance is
    /// unscored (plan M5).
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
    let enabled = gate::is_enabled();
    // The scheduler is `app.manage`d only once `media_index::scheduler::start` ran; a
    // missing state (e.g. an early call) honestly reads as "not enriching".
    let scheduler = app.try_state::<Arc<MediaScheduler>>().map(|s| Arc::clone(s.inner()));
    let indexing = scheduler.as_ref().is_some_and(|s| s.is_enriching(&volume_id));

    let data_dir = crate::config::resolved_app_data_dir(&app)?;
    let threshold = gate::importance_threshold();
    let vid = volume_id.clone();
    // The threshold-aware stored-coverage split (`covered_qualifying_count` + `kept_count`,
    // plan M5) needs the volume's OS mount root to map override/exclude config; resolving
    // it here (a reclaim-eligible enabled volume only) keeps the split `None` for a
    // volume that isn't background-enriched.
    let mount_root = resolve_reclaim_volumes(std::slice::from_ref(&volume_id))
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
            // The threshold-aware split (`None` unless the volume is reclaim-eligible AND
            // importance has scored it — the SAME single source as M4's reclaim numbers,
            // via `stored_coverage_counts`, so they never disagree).
            let coverage_counts = match (&scheduler, &mount_root) {
                (Some(scheduler), Some(mount)) => scheduler.stored_coverage_counts(&vid, mount, threshold),
                _ => None,
            };
            Ok::<_, String>((enriched, qualifying, importance_scored, coverage_counts))
        })
        .await
        .map_err(|e| format!("media volume state task panicked: {e}"))??;

    // Deferred-on-importance: enabled, the index is ready (a real qualifying count),
    // but importance has no data yet, so enrichment waits on the recompute.
    let waiting_for_importance = enabled && qualifying_count.is_some() && !importance_scored;

    Ok(MediaIndexVolumeState {
        enabled,
        indexing,
        enriched_count,
        qualifying_count,
        network_opt_in: network_config::is_opted_in(&volume_id),
        always_indexed: network_config::snapshot().always_index_volumes.contains(&volume_id),
        paused: network_config::is_paused(&volume_id),
        waiting_for_importance,
        covered_qualifying_count: coverage_counts.as_ref().map(|c| c.covered_qualifying),
        kept_count: coverage_counts.as_ref().map(|c| c.doomed_stored),
    })
}

/// Set (or clear) a volume's opt-in for background network (SMB) image enrichment
/// (network enrichment). Off by default: turning on the master toggle does NOT auto-enrich
/// network volumes. Enabling kicks an immediate pass so the user sees progress without
/// waiting for the next scan completion. Live-applied (no restart); the frontend
/// persists `mediaIndex.networkVolumes` and calls this on change (network-enrichment UI).
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
/// (an absolute OS path) enriches (the privacy complement to the opt-in — plan §
/// Privacy). A hard veto that beats any "always index" override.
///
/// EXCLUDING retro-deletes existing rows at or under the folder across the reachable
/// volumes, so already-extracted OCR text stops being searchable at once (privacy is a
/// hard requirement, not "eventually on the next GC"). The sequence is deliberate:
///
/// 1. set the live veto FIRST, so any in-flight pass re-checks against the excluded
///    state and can't re-insert rows behind the delete (the pre-upsert TOCTOU close);
/// 2. THEN retro-delete (a double-tap through each volume's one writer thread, so a
///    straggler upsert that squeezed in is swept), off the IPC thread.
///
/// Un-EXCLUDING only clears the veto: NO re-delete and NO auto re-enrich — the next
/// natural pass picks the folder up again. An offline network volume is skipped by the
/// retro-delete (no mount root) and re-fires on reconnect via the registration bus.
/// Live-applied; the frontend persists `mediaIndex.excludedFolders` and calls this on
/// change (rolling the persisted value back if this rejects).
#[tauri::command]
#[specta::specta]
pub async fn media_index_set_excluded_folder(app: AppHandle, folder: String, excluded: bool) -> Result<(), String> {
    // Live state FIRST (step 1): the veto must precede any delete.
    network_config::set_excluded_folder(&folder, excluded);

    if !excluded {
        return Ok(());
    }
    // Step 2: retro-delete across reachable volumes. Skip cleanly if the scheduler isn't
    // managed yet (nothing has been enriched, so there's nothing to purge).
    let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>() else {
        return Ok(());
    };
    let scheduler = Arc::clone(scheduler.inner());
    // The reachable volumes + their mount roots. An unmounted volume isn't listed, so
    // its retro-delete re-fires on reconnect (`wire_volume`).
    let mounts: Vec<(String, String)> = crate::file_system::get_volume_manager()
        .list_volumes_with_handles()
        .into_iter()
        .map(|(id, vol)| (id, vol.root().to_string_lossy().into_owned()))
        .collect();
    // The prune blocks on the writer thread, so run it off the IPC thread.
    tauri::async_runtime::spawn_blocking(move || {
        scheduler.retro_delete_excluded_folder(&folder, &mounts);
    })
    .await
    .map_err(|e| format!("retro-delete task panicked: {e}"))
}

/// Set the folder-importance threshold the scheduler enriches by — the importance settings
/// slider's typed value (`0.0..=1.0`, clamped), never a string (`no-string-matching`).
/// Below-threshold folders are deferred; an override still forces enrichment. Live-
/// applied; the frontend persists `mediaIndex.importanceThreshold` and calls this.
///
/// A DECREASE broadens coverage, so newly-covered folders should start enriching now
/// rather than waiting for the next scan — this kicks a pass. A RAISE only defers
/// future work (forward-only semantics: nothing to enrich now, and the deferred rows
/// persist), so kicking on a raise would re-walk the index for nothing. The comparison
/// reads the stored value BEFORE and AFTER the (clamped) set, so a clamp can't
/// misclassify the direction (plan M1).
#[tauri::command]
#[specta::specta]
pub fn media_index_set_importance_threshold(app: AppHandle, threshold: f64) {
    let previous = gate::importance_threshold();
    gate::set_importance_threshold(threshold);
    let next = gate::importance_threshold();
    if gate::threshold_decreased(previous, next) && gate::is_enabled() {
        scheduler::kick_all_ready_passes(&app);
    }
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
            // volume: a local volume always, an SMB volume only when opted in, MTP and
            // LocalExternal never (MTP is on-demand; a LocalExternal drive's index paths
            // are mount-relative, so it's skipped until mapped — see `wire_volume`).
            let enabled = match kinds.get(vid) {
                Some(IndexVolumeKind::Local) => true,
                Some(IndexVolumeKind::Smb) => network_config::is_opted_in(vid),
                Some(IndexVolumeKind::Mtp | IndexVolumeKind::LocalExternal) | None => false,
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

/// The reclaim-space preview behind the settings "delete the extra entries" line (plan
/// M4): across the ENABLED volumes in `volume_ids`, how many stored image rows fall
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

/// What a reclaim prune freed (plan M4): the rows deleted and the bytes reclaimed.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimResult {
    /// The image rows deleted across the enabled volumes.
    pub deleted_rows: u64,
    /// The content bytes freed (an "about" estimate; the toast voices it).
    pub freed_bytes: u64,
}

/// Resolve the enabled media-index volumes to reclaim over from `volume_ids`, each with
/// the OS mount root the stored (index) paths map into: a local volume (mount `/`), or an
/// opted-in SMB volume (its mount root). MTP and non-opted-in SMB are dropped. The
/// `pending` flag is set when a requested volume the user expects isn't ready (offline /
/// not scanned, or opted-in-but-unmounted SMB), so the caller can caveat the totals.
fn resolve_reclaim_volumes(volume_ids: &[String]) -> (Vec<(String, String)>, bool) {
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

/// Preview the reclaim-space split across `volume_ids` at the CURRENT `threshold` (plan
/// M4). Thin: resolves the enabled volumes and aggregates the scheduler's single-source
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
    tauri::async_runtime::spawn_blocking(move || {
        let (volumes, mut pending) = resolve_reclaim_volumes(&volume_ids);
        let mut total_stored = 0u64;
        let mut covered_stored = 0u64;
        let mut doomed_count = 0u64;
        let mut estimated_bytes = 0u64;
        for (vid, mount) in &volumes {
            match scheduler.stored_coverage(vid, mount, threshold) {
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

/// Prune the stored image rows OUTSIDE the current `threshold` across `volume_ids` (plan
/// M4 reclaim). Thin: delegates to the scheduler's `prune_below_threshold` per volume,
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
    tauri::async_runtime::spawn_blocking(move || {
        let (volumes, _pending) = resolve_reclaim_volumes(&volume_ids);
        let mut deleted_rows = 0u64;
        let mut freed_bytes = 0u64;
        for (vid, mount) in &volumes {
            let outcome = scheduler.prune_below_threshold(vid, mount, threshold);
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

#[cfg(test)]
mod tests;
