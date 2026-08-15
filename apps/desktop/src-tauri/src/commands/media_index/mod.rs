//! The media-index IPC commands: the read/query surface (plan Decision 8), plus the
//! coverage-CHANGING setters in [`policy`].
//!
//! Thin per the commands-layer rule: resolve the app data dir, open the
//! [`MediaIndex`](cmdr_index::media_index::read::MediaIndex) read API for the volume, and hand off the
//! query. `search/` reaches `media.db` ONLY through `MediaIndex` — this command layer is
//! that door, so no consumer takes a raw `rusqlite` dep on `media.db`.
//!
//! Query-time DB work runs OFF the synchronous IPC thread (`spawn_blocking`), since a
//! sync `#[tauri::command]` blocks the IPC handler (`src-tauri/CLAUDE.md`). The read
//! API answers from `media.db` directly, so it still returns results when the volume
//! is offline (a NAS unplugged) — proven by the read-API tests.
//!
//! ## The modules
//!
//! - [`search`]: OCR, tag, and semantic search, find-similar, dedup clusters.
//! - [`state`]: the honest per-volume enrichment state + the covered-count slider preview.
//! - [`reclaim`]: the outside-the-setting preview and the user-explicit prune.
//! - [`file_status`]: the per-file overlay and per-folder coverage badge.
//! - [`clip_model`]: the CLIP model's install state, download, and delete.
//! - [`thumbnail`]: `cmdr-media://` tokens for the results grid.
//! - [`policy`]: the setters that change WHAT gets indexed (scope, overrides, threshold).
//!
//! This file keeps only what more than one of them needs: the hit-limit clamp, the ONE
//! enabled-volume rule, and the [`OVERLAY_QUERIES`] blocking budget.

use cmdr_index::host::config::{IndexConfig, MediaConfig};
use cmdr_index::media_index::network::config::NetworkEnrichConfig;

use crate::commands::util::BlockingBudget;

/// The shared blocking-pool budget for the queries the FRONTEND re-issues on its own
/// schedule: the per-file badge, the per-folder coverage badge, the per-volume state
/// poll, and the covered-count preview.
///
/// They're on ONE budget because they contend for the same thing — `media.db`,
/// `importance.db`, and the drive index behind them — so a cap on each separately
/// would still let their sum take the pool. ❌ Don't give one of them its own budget
/// to "unblock" it; that reopens the hole.
///
/// **Four permits**, because these queries serialize on SQLite anyway: more
/// concurrency buys no throughput and costs contention, while four covers both panes
/// asking at once with room to spare. A burst past that queues (cheap async futures,
/// in order) instead of taking pool threads other subsystems need.
///
/// This is defense in depth, not the fix for any one pileup: a query that's slow
/// enough to need the cap should also be made cheap. The badge query's own fix is the
/// score cache in `cmdr_index::media_index::coverage`.
static OVERLAY_QUERIES: BlockingBudget = BlockingBudget::new(4);

mod clip_model;
mod file_status;
pub mod policy;
mod reclaim;
mod search;
mod state;
mod thumbnail;

// Glob re-exports so every command keeps its `commands::media_index::<name>` path in
// `ipc.rs` / `ipc_collectors.rs`. They're globs on purpose: `#[tauri::command]` also
// generates two hidden macros per function (`__cmd__*`, `__tauri_command_name_*`) that
// `generate_handler!` resolves through the SAME path, so naming the items one by one
// would mean listing those macros too.
pub use clip_model::*;
pub use file_status::*;
pub use reclaim::*;
pub use search::*;
pub use state::*;
pub use thumbnail::*;

/// Kick a coalesced enrichment pass for every ready volume, resolving the managed
/// scheduler first.
///
/// The app-side half of `MediaScheduler::kick_all_ready_passes`: the scheduler lives in
/// Tauri state, which the subsystem can't reach. A no-op before the scheduler is managed
/// (an early call at startup).
pub(crate) fn kick_all_ready_passes_for(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(scheduler) = app.try_state::<std::sync::Arc<cmdr_index::media_index::scheduler::MediaScheduler>>() {
        scheduler.inner().kick_all_ready_passes();
    }
}

/// Turn the app's stored settings into the index's [`IndexConfig`].
///
/// The one place settings become index policy. Every default and every migration
/// fallback lives here rather than inside the index, which is what "the crate never
/// reads settings" means in practice. Called at startup and again by the policy
/// setters below whenever the user changes one of these.
pub(crate) fn index_config_from(
    data_dir: std::path::PathBuf,
    settings: &crate::settings::loader::Settings,
) -> IndexConfig {
    use cmdr_index::media_index::gate;
    IndexConfig {
        data_dir,
        // Not a settings key: the escape hatch for the phased first index is a user
        // default, read once per launch (`crate::index_host::phased_first_index`).
        phased_first_index: crate::index_host::phased_first_index(),
        media: MediaConfig {
            enabled: settings.image_index_enabled == Some(true),
            // The scope, with the pre-setting fallback applied: an install that already
            // had image indexing on keeps the automatic behavior even on the launch
            // before the frontend migration writes the key.
            scope: gate::scope_from_settings(settings.media_index_scope.as_deref(), settings.image_index_enabled),
            importance_threshold: settings
                .media_index_importance_threshold
                .unwrap_or(gate::DEFAULT_IMPORTANCE_THRESHOLD),
            // Absent means the default 1; the index clamps to `1..=CPU count`, so a
            // persisted or hand-edited value can't over-provision.
            parallelism: settings
                .media_index_parallelism
                .map_or(gate::DEFAULT_PARALLELISM, usize::from),
            // ON unless explicitly turned off (absent means on — inert with no model
            // installed anyway). Gates both the CLIP write path and `search_semantic`.
            semantic_search_enabled: settings.media_index_semantic_search_enabled.unwrap_or(true),
            network: NetworkEnrichConfig {
                opted_in_volumes: settings.media_index_network_volumes.iter().cloned().collect(),
                always_index_volumes: settings.media_index_always_index_volumes.iter().cloned().collect(),
                always_index_folders: settings.media_index_always_index_folders.iter().cloned().collect(),
                excluded_folders: settings.media_index_excluded_folders.iter().cloned().collect(),
            },
        },
    }
}

/// The default hit cap when the caller doesn't specify one, and the hard ceiling on
/// any caller-supplied limit (a photo-search grid never needs more, and it bounds the
/// query's work + payload).
const DEFAULT_LIMIT: u32 = 200;
const MAX_LIMIT: u32 = 1000;

/// Resolve the effective hit cap: a caller `None` takes [`DEFAULT_LIMIT`], and any
/// caller value is clamped to [`MAX_LIMIT`].
fn resolve_limit(limit: Option<u32>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize
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
    use cmdr_index::IndexVolumeKind;
    use cmdr_index::media_index::network::config as network_config;
    let kinds: std::collections::HashMap<String, IndexVolumeKind> =
        crate::index_host::index().ready_volumes().into_iter().collect();
    let mounts: std::collections::HashMap<String, String> = crate::file_system::volume::manager::get_volume_manager()
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

#[cfg(test)]
mod tests;
