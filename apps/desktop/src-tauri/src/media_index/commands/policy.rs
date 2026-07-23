//! The commands that change WHAT gets indexed: the scope, the per-folder and per-volume
//! "always index" overrides, the network opt-in, the importance threshold, and the
//! privacy exclusion.
//!
//! Split out of [`super`] (the read/query surface) because they share one shape and one
//! hazard: each mutates live `gate` / `network::config` state, and each has to decide
//! whether the change BROADENS coverage and therefore needs an immediate pass. Each of
//! those decisions is a small pure `*_should_kick` fn covered in `commands/tests.rs`,
//! since the commands themselves need an `AppHandle`.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::media_index::gate;
use crate::media_index::network::config as network_config;
use crate::media_index::scheduler::{self, MediaScheduler};

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
/// (an absolute OS-mount path) enriches regardless of importance, in EITHER scope —
/// in the narrow one ("only folders I choose") these overrides are the whole coverage.
/// Live-applied; the frontend persists `mediaIndex.alwaysIndexFolders` and calls this
/// on change.
///
/// ADDING a folder kicks an immediate pass, the same way opting a network volume in
/// does: the folder is the user asking for these photos NOW, and waiting for the next
/// scan completion (which on a quiet local drive may be hours) would make the feature
/// look inert. Every ready volume is kicked because the folder can sit on any of them
/// and the path alone doesn't say which; a pass on a volume the folder isn't under is a
/// fast staleness no-op, and the coordinator folds a kick that races a running pass.
/// REMOVING one kicks nothing: coverage only narrows, and the rows persist
/// (forward-only) until the user reclaims them.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_always_index_folder(app: AppHandle, folder: String, always: bool) {
    network_config::set_always_index_folder(&folder, always);
    if folder_override_should_kick(always, gate::is_enabled()) {
        scheduler::kick_all_ready_passes(&app);
    }
}

/// Whether committing a folder-override change should kick an immediate pass: only
/// ADDING one (it broadens coverage), and only while the feature is enabled (a disabled
/// feature has no pass to run). Extracted from [`media_index_set_always_index_folder`]
/// so the decide-then-kick decision is testable without an `AppHandle`.
pub(super) fn folder_override_should_kick(always: bool, enabled: bool) -> bool {
    always && enabled
}

/// Set the indexing SCOPE: index only the folders the user chose, or index
/// automatically by folder importance. The typed token (`no-string-matching`: an
/// unknown one falls back to the narrow default rather than branching on wording).
/// Live-applied; the frontend persists `mediaIndex.scope` and calls this on change.
///
/// BROADENING (narrow → automatic) kicks a pass so the newly-covered folders start
/// enriching now. NARROWING never deletes: the rows outside the new scope stay
/// searchable and surface as the existing kept-rows / reclaim offer, so a scope switch
/// can't silently destroy an index the user spent hours building.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_scope(app: AppHandle, scope: String) {
    let previous = gate::scope();
    let next = gate::IndexScope::from_token(&scope);
    gate::set_scope(next);
    if scope_change_should_kick(previous, next, gate::is_enabled()) {
        scheduler::kick_all_ready_passes(&app);
    }
}

/// Whether committing a scope change from `previous` to `next` should kick an immediate
/// pass: a BROADENING while the feature is enabled. Extracted from
/// [`media_index_set_scope`] so the decide-then-kick decision is testable without an
/// `AppHandle`.
pub(super) fn scope_change_should_kick(previous: gate::IndexScope, next: gate::IndexScope, enabled: bool) -> bool {
    gate::scope_broadened(previous, next) && enabled
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

/// Turn CLIP semantic search on or off (the "search photos by description" feature).
/// Live-applied; the frontend persists `mediaIndex.semanticSearch.enabled` and calls this
/// on change. The one atomic gates BOTH sides: `search_semantic` returns nothing when off,
/// and `clip::current_stamp` returns `None` when off so no pass embeds CLIP.
///
/// Turning it OFF stops future CLIP work without deleting anything (a running pass simply
/// stops embedding CLIP; existing embeddings stay searchable until the user turns it back
/// on or deletes the model). Turning it ON while a model is installed makes every image
/// CLIP-stale again, so it kicks the ready passes to embed now (like a fresh model
/// install); with no model installed there's nothing to embed, so no kick.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_semantic_search_enabled(app: AppHandle, enabled: bool) {
    gate::set_semantic_search_enabled(enabled);
    if !enabled || !gate::is_enabled() {
        return;
    }
    // Only worth a pass if a model is actually installed (else `current_stamp` is `None`
    // and the pass would walk the index to embed nothing).
    let model_installed = crate::config::resolved_app_data_dir(&app)
        .map(|dir| crate::media_index::clip::install::is_installed(&dir))
        .unwrap_or(false);
    if model_installed {
        scheduler::kick_all_ready_passes(&app);
    }
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
/// misclassify the direction.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_importance_threshold(app: AppHandle, threshold: f64) {
    let previous = gate::importance_threshold();
    gate::set_importance_threshold(threshold);
    let next = gate::importance_threshold();
    if threshold_change_should_kick(previous, next, gate::is_enabled()) {
        scheduler::kick_all_ready_passes(&app);
    }
}

/// Whether committing a threshold change from `previous` to `next` should kick an
/// immediate pass: a DECREASE (broader coverage) while the feature is enabled. A raise
/// only defers future work (forward-only semantics — nothing to enrich now, and the
/// deferred rows persist), and a disabled feature has no pass to run. Extracted from
/// [`media_index_set_importance_threshold`] so the decide-then-kick decision is testable
/// without an `AppHandle`.
pub(super) fn threshold_change_should_kick(previous: f64, next: f64, enabled: bool) -> bool {
    gate::threshold_decreased(previous, next) && enabled
}

/// Set how many parallel enrichment workers to run (the `mediaIndex.parallelism` slider).
/// Clamped to `1..=CPU-count` by the gate. Live-applied: a RUNNING pass re-reads the count
/// between images and resizes its worker pool within about one image, so no pass restart
/// and no kick is needed (unlike a coverage change, this only changes HOW FAST the current
/// work runs, never WHICH images are covered). The frontend persists `mediaIndex.parallelism`
/// and calls this.
#[tauri::command]
#[specta::specta]
pub fn media_index_set_parallelism(parallelism: u32) {
    gate::set_parallelism(parallelism as usize);
}

/// The hardware ceiling for the parallelism slider: this machine's logical CPU count. The
/// slider reads it for its max (the backend clamps to it independently, so an out-of-range
/// value can't over-provision). A plain read, no side effects.
#[tauri::command]
#[specta::specta]
pub fn media_index_max_parallelism() -> u32 {
    gate::max_parallelism() as u32
}
