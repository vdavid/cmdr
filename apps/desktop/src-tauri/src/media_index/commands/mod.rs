//! The media-index IPC commands: the read/query surface (plan Decision 8), plus the
//! coverage-CHANGING setters in [`policy`].
//!
//! Thin per the commands-layer rule: resolve the app data dir, open the
//! [`MediaIndex`](super::read::MediaIndex) read API for the volume, and hand off the
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
//! This file keeps only what more than one of them needs: the hit-limit clamp and the
//! ONE enabled-volume rule.

mod clip_model;
mod file_status;
pub mod policy;
mod reclaim;
mod search;
mod state;
mod thumbnail;

// Glob re-exports so every command keeps its `media_index::commands::<name>` path in
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
    use super::network::config as network_config;
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

#[cfg(test)]
mod tests;
