//! The honesty surface: the per-volume enrichment state the search UI voices, and the
//! covered-count preview behind the importance slider. Both answer "how much of this
//! volume is (or will be) indexed", from the same cached counts, so they can't disagree.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::resolve_enabled_volumes;
use cmdr_index::media_index::coverage;
use cmdr_index::media_index::gate;
use cmdr_index::media_index::network::config as network_config;
use cmdr_index::media_index::read::MediaIndex;
use cmdr_index::media_index::scheduler::MediaScheduler;

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
    /// Honest progress). `None` when there's no honest number YET: the volume's index
    /// isn't ready (offline / still scanning), OR nothing has computed the counts (this
    /// command reads the coverage cache and never builds it — a cold build is a
    /// whole-index walk). Either way the UI voices the wait rather than a fabricated
    /// total. ETA math lives UI-side off `(enriched_count, qualifying_count)`.
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
    // The scheduler is `app.manage`d only once `MediaScheduler::start` ran; a
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
            // CACHED only — this is a poll that also runs at launch, and a cold build is a
            // whole-index O(entries) walk (the 50 GB launch runaway). `None` therefore means
            // "no honest number yet": the index isn't registered (offline / still scanning),
            // no pass has walked it, and nobody has asked for the count. The settings panel
            // asks via `media_index_covered_count` when it opens, which warms this.
            let qualifying = coverage::cached(&vid).map(|c| c.total);
            // Whether importance has data for this volume — the same "has it scored?"
            // check the scheduler gates enrichment on (live weight rows OR a stamped
            // generation), so the deferred state can't disagree with the scheduler.
            let importance_scored = {
                use cmdr_index::importance::{ImportanceIndex, SignalSet};
                ImportanceIndex::open(&data_dir, &vid, SignalSet::all()).is_scored()
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
        covered_qualifying_count: coverage_counts.as_ref().and_then(|c| c.covered_qualifying),
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
                None if !scope.consults_importance() => std::sync::Arc::new(std::collections::HashMap::new()),
                None => {
                    pending = true;
                    continue;
                }
            };
            // Override coverage is OS-path keyed; map each folder into OS space, as the
            // enrichment gate and the reclaim partition both do.
            let config = network_config::snapshot();
            let is_override = |folder: &str| {
                config.covers(
                    vid,
                    &cmdr_index::media_index::network::fetch::os_join(mount_root, folder),
                )
            };
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
