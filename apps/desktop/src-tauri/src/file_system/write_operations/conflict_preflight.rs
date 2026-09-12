//! The transfer dialog's pre-flight conflict check: which destination items
//! `source_items` would already collide with, budgeted so a volume that has
//! stopped answering reports "I couldn't check" instead of hanging the dialog.
//!
//! Distinct from `conflict.rs`, which resolves a clash a write has ALREADY hit
//! mid-copy; this is the BEFORE-the-write scan the transfer dialog runs to
//! decide whether to show the overwrite/skip/rename radios at all. Also
//! distinct from `scan.rs` / `scan_preview.rs`, which size a transfer (byte and
//! file totals) rather than check for name collisions.
//!
//! `commands/file_system/volume_copy.rs::scan_volume_for_conflicts` is the thin
//! `#[tauri::command]` wrapper that calls [`scan_volume_for_conflicts_within`]
//! with the production budget.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::time::Duration;

use crate::deadline::{Deadline, timeout_detached_within};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{ScanConflict, Volume, VolumeError};
use crate::unregistered_volumes::{Unregistered, why_unregistered};

use super::routing::{resolve_dest_path, resolve_source_volume, transfer_would_land_on_its_source};

/// Why a pre-flight scan or a conflict check couldn't answer.
///
/// ❌ Not prose: the transfer dialog shows its own "couldn't check" state and
/// logs the variant. `VolumeError` is the wire type the frontend already words,
/// so a scan that fails on the device says exactly what the device said.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum VolumeScanError {
    /// The source volume isn't registered (a race: it was ejected mid-dialog).
    SourceVolumeNotFound {
        /// The id that no longer resolves.
        volume_id: String,
    },
    /// The destination volume isn't registered.
    DestinationVolumeNotFound {
        /// The id that no longer resolves.
        volume_id: String,
    },
    /// The source is a listed phone or a saved server that nothing has
    /// connected yet (`crate::unregistered_volumes`).
    SourceVolumeNotConnected {
        /// The id nothing has connected.
        volume_id: String,
    },
    /// The destination is a listed phone or a saved server that nothing has
    /// connected yet.
    DestinationVolumeNotConnected {
        /// The id nothing has connected.
        volume_id: String,
    },
    /// The volume refused, and said why in its own vocabulary.
    Volume {
        /// The backend's typed answer.
        error: VolumeError,
    },
    /// The scan didn't finish inside the command's budget. ❗ It was NOT
    /// cancelled: the deadline bounds the dialog's wait, not the scan.
    TimedOut,
    /// The scan task panicked, so no answer is coming.
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

impl VolumeScanError {
    /// The refusal for a source id the registry had nothing for: not connected
    /// yet, or gone (`crate::unregistered_volumes`).
    pub(crate) async fn source_missing(volume_id: String) -> Self {
        match why_unregistered(&volume_id).await {
            Unregistered::NotConnected => Self::SourceVolumeNotConnected { volume_id },
            Unregistered::Gone => Self::SourceVolumeNotFound { volume_id },
        }
    }

    /// The same, for the destination.
    pub(crate) async fn destination_missing(volume_id: String) -> Self {
        match why_unregistered(&volume_id).await {
            Unregistered::NotConnected => Self::DestinationVolumeNotConnected { volume_id },
            Unregistered::Gone => Self::DestinationVolumeNotFound { volume_id },
        }
    }
}

impl std::fmt::Display for VolumeScanError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceVolumeNotFound { volume_id } => write!(f, "source volume not found: {volume_id}"),
            Self::DestinationVolumeNotFound { volume_id } => {
                write!(f, "destination volume not found: {volume_id}")
            }
            Self::SourceVolumeNotConnected { volume_id } => write!(f, "source volume not connected yet: {volume_id}"),
            Self::DestinationVolumeNotConnected { volume_id } => {
                write!(f, "destination volume not connected yet: {volume_id}")
            }
            Self::Volume { error } => write!(f, "volume: {error}"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for VolumeScanError {}

/// The whole conflict check, under ONE wall-clock budget.
///
/// The dialog says "Checking for conflicts..." while this runs, so what it owes
/// the user is a knowable wait: an answer, or an honest "couldn't check", within
/// `deadline`. Every leg below can reach a device that has stopped answering —
/// two volume resolves (a `.zip`-crossing path probes the network), the source
/// batch stat, and the destination listing — and each takes what's LEFT rather
/// than a fresh 30 s, because four legs with their own timeouts add up to a
/// two-minute promise nobody ever wrote down.
///
/// Split out from the command so a test can hand it a budget it can wait out.
pub(crate) async fn scan_volume_for_conflicts_within(
    deadline: Deadline,
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
) -> Result<Vec<ScanConflict>, VolumeScanError> {
    let dest_path = PathBuf::from(dest_path);
    log::debug!(
        target: CONFLICT_LOG_TARGET,
        "checking {} item(s) against {} on volume {}",
        source_items.len(),
        dest_path.display(),
        volume_id
    );

    // Resolve the destination so a conflict scan against an archive-inner dest
    // routes to its ArchiveVolume (consistent with the copy op's routing).
    let resolve_id = volume_id.clone();
    let resolve_path = dest_path.clone();
    let volume = timeout_detached_within(
        &deadline,
        || VolumeScanError::TimedOut,
        |detail| VolumeScanError::Unexpected { detail },
        async move { Ok::<_, VolumeScanError>(get_volume_manager().resolve(&resolve_id, &resolve_path).await.volume) },
    )
    .await
    .inspect_err(|e| log_conflict_outcome(&deadline, "couldn't reach the destination volume", e))?;
    let Some(volume) = volume else {
        return Err(VolumeScanError::destination_missing(volume_id).await);
    };

    // Same anchoring the copy op applies: the dialog's box is volume-relative,
    // so without it the scan asks a share for a path outside its mount and
    // reports "no conflicts" for a folder full of them.
    let dest_path = resolve_dest_path(&volume, dest_path.to_string_lossy().into_owned());

    let mut source_items: Vec<crate::file_system::SourceItemInfo> = source_items
        .into_iter()
        .map(|item| crate::file_system::SourceItemInfo {
            name: item.name,
            size: item.size,
            modified: item.modified,
            is_directory: item.is_directory,
        })
        .collect();

    // The source side of the self-collision filter below. Both halves come from
    // the same resolve the batch stat pays for, and it stays `None` when the
    // caller sent no source volume: the filter is then inert and the scan keeps
    // its name-only behavior.
    let mut resolved_source: Option<(Arc<dyn Volume>, Vec<PathBuf>)> = None;

    // Resolve real per-item types and sizes from the source volume when the
    // caller supplied it. One `get_metadata` per top-level path, O(top-level
    // items) and never recursive. `resolve_source` routes an archive-inner
    // source through its ArchiveVolume.
    if let (Some(src_volume_id), Some(src_paths)) = (source_volume_id, source_paths) {
        let paths: Vec<PathBuf> = src_paths.iter().map(PathBuf::from).collect();
        let first = paths.first().cloned();
        let resolved = timeout_detached_within(
            &deadline,
            || VolumeScanError::TimedOut,
            |detail| VolumeScanError::Unexpected { detail },
            async move { Ok::<_, VolumeScanError>(resolve_source_volume(&src_volume_id, first.as_ref()).await) },
        )
        .await;
        if let Ok(Some((src_volume, _))) = resolved {
            resolved_source = Some((Arc::clone(&src_volume), paths.clone()));
            // A sub-budget, because this leg is OPTIONAL (see the fallback
            // below) and the destination scan after it is not. Detached (see
            // `timeout_detached`): the stats reach the source device, so the
            // deadline must not drop them mid-request.
            let stat_deadline = deadline.fraction(SOURCE_STAT_BUDGET_DIVISOR);
            let stats = timeout_detached_within(
                &stat_deadline,
                || VolumeScanError::TimedOut,
                |detail| VolumeScanError::Unexpected { detail },
                async move { Ok::<_, VolumeScanError>(stat_source_paths(&src_volume, &paths).await) },
            )
            .await;
            match stats {
                Ok(stats) => merge_source_types_from_stats(&mut source_items, &stats),
                // A source-side stat that doesn't come back is non-fatal: fall
                // back to the name-only items the caller sent. Conflict
                // detection still works by name; only the dir/size hints
                // degrade, so the check can still answer.
                Err(e) => {
                    log::debug!(
                        target: CONFLICT_LOG_TARGET,
                        "source stats unavailable after {:.1}s of their {:.1}s share, using name-only items: {}",
                        stat_deadline.elapsed().as_secs_f64(),
                        stat_deadline.total().as_secs_f64(),
                        e
                    );
                }
            }
        }
    }

    // Run conflict scan (now async), detached so the destination device isn't
    // left mid-transaction if the scan overruns.
    let dest_volume = Arc::clone(&volume);
    let found = timeout_detached_within(
        &deadline,
        || VolumeScanError::TimedOut,
        |detail| VolumeScanError::Unexpected { detail },
        async move {
            volume
                .scan_for_conflicts(&source_items, &dest_path)
                .await
                .map_err(|error| VolumeScanError::Volume { error })
        },
    )
    .await;
    match found {
        Ok(conflicts) => {
            let total = conflicts.len();
            let conflicts = drop_self_collisions(conflicts, resolved_source.as_ref(), &dest_volume);
            log::debug!(
                target: CONFLICT_LOG_TARGET,
                "found {} collision(s) in {:.1}s, {} of them the sources themselves",
                total,
                deadline.elapsed().as_secs_f64(),
                total - conflicts.len()
            );
            Ok(conflicts)
        }
        Err(e) => {
            log_conflict_outcome(&deadline, "couldn't read the destination", &e);
            Err(e)
        }
    }
}

/// Drops the collisions that name a source itself, which the engines duplicate
/// silently instead of asking about.
///
/// The per-backend `scan_for_conflicts` can't do this: it gets `SourceItemInfo`,
/// a name and a size with no source path in it, so widening it would mean
/// touching three backends and every test double for a question one place can
/// answer. `transfer_would_land_on_its_source` is that one place, and it gives
/// the answer the engine that will actually run gives.
///
/// Every source is tried against every collision, rather than paired by the name
/// each collision carries. Identity is what the engines redirect on, and a batch
/// whose sources share a basename lands in ONE redirected destination, so a
/// collision is gone the moment ANY source turns out to be the item sitting
/// there. The cost is bounded by `max_conflicts` (100 by default) times the
/// batch, and the expensive `dev+ino` arm only runs when both sides are local.
fn drop_self_collisions(
    conflicts: Vec<ScanConflict>,
    resolved_source: Option<&(Arc<dyn Volume>, Vec<PathBuf>)>,
    dest_volume: &Arc<dyn Volume>,
) -> Vec<ScanConflict> {
    let Some((source_volume, source_paths)) = resolved_source else {
        return conflicts;
    };
    conflicts
        .into_iter()
        .filter(|conflict| {
            let dest_path = Path::new(&conflict.dest_path);
            !source_paths.iter().any(|source_path| {
                transfer_would_land_on_its_source(source_volume, source_path, dest_volume, dest_path)
            })
        })
        .collect()
}

/// The one line a check that couldn't answer leaves behind. WARN, because the
/// dialog is about to tell the user it doesn't know what's at their destination,
/// and that's exactly the state worth noticing in a log.
fn log_conflict_outcome(deadline: &Deadline, what: &str, e: &VolumeScanError) {
    log::warn!(
        target: CONFLICT_LOG_TARGET,
        "conflict check gave up after {:.1}s: {} ({})",
        deadline.elapsed().as_secs_f64(),
        what,
        e
    );
}

/// Stats each top-level source path, concurrently, one `get_metadata` apiece.
///
/// ❗ Deliberately NOT `scan_for_copy_batch`: that walks a directory source's
/// whole subtree to produce a recursive `total_bytes`, and the only two fields
/// the conflict check wants — `is_directory`, and a FILE's size — need a plain
/// stat. On a single directory source the SMB and SFTP backends take their
/// `paths.len() == 1` fast path straight into `scan_recursive`, so a 119k-file
/// folder spent the entire conflict budget on a number the merge below throws
/// away (a real user's copy, 2026-08-27, `ERR-AYVM4`).
///
/// A path the source can't stat is simply absent from the result; the merge
/// leaves the caller's values in place for it.
async fn stat_source_paths(source_volume: &Arc<dyn Volume>, paths: &[PathBuf]) -> Vec<(PathBuf, FileEntry)> {
    use futures_util::stream::{self, StreamExt};

    stream::iter(paths.iter().cloned())
        .map(|path| async move {
            let entry = source_volume.get_metadata(&path).await.ok()?;
            Some((path, entry))
        })
        .buffer_unordered(SOURCE_STAT_CONCURRENCY)
        .filter_map(|hit| async move { hit })
        .collect()
        .await
}

/// Overlays authoritative `is_directory` + `size` from the source-volume stats
/// onto the caller-supplied `source_items`, matched by base filename.
///
/// The match key is the path's final component, which is exactly the `name`
/// the FE derives for each `SourceItemInput`. An item with no stat hit keeps
/// the values the caller sent (the safe fallback). A directory keeps the
/// caller's `size` too: a directory's conflict-UI size is meaningless and the
/// dir-dir case never renders one.
fn merge_source_types_from_stats(
    source_items: &mut [crate::file_system::SourceItemInfo],
    stats: &[(PathBuf, FileEntry)],
) {
    use std::collections::HashMap;
    let by_name: HashMap<&str, &FileEntry> = stats
        .iter()
        .filter_map(|(path, entry)| path.file_name().and_then(|n| n.to_str()).map(|n| (n, entry)))
        .collect();
    for item in source_items.iter_mut() {
        if let Some(entry) = by_name.get(item.name.as_str()) {
            item.is_directory = entry.is_directory;
            if !entry.is_directory
                && let Some(size) = entry.size
            {
                item.size = size;
            }
        }
    }
}

/// How long the whole conflict check may take before it answers "I couldn't".
///
/// One budget for every leg, not one per leg. 30 s is the tier this codebase
/// already gives a recursive scan over IPC, and it's what the dialog's spinner
/// is sized against.
pub(crate) const CONFLICT_CHECK_BUDGET: Duration = Duration::from_secs(30);

/// What share of the budget the OPTIONAL source-stat leg may spend: `1/N` of it.
///
/// The leg after it is the destination scan, which is the one that actually
/// answers "does this clash?". Letting an optional leg run to the shared
/// deadline leaves that one with zero budget, so it fails instantly and the
/// dialog reports a destination timeout for a destination it never asked.
const SOURCE_STAT_BUDGET_DIVISOR: u32 = 3;

/// How many top-level source paths to stat at once.
///
/// Each is one round trip on a remote backend, so a wide selection wants
/// overlap; the cap keeps a 10k-item selection from opening 10k of them.
const SOURCE_STAT_CONCURRENCY: usize = 16;

/// The log target every conflict-check line carries.
const CONFLICT_LOG_TARGET: &str = "conflict_scan";

/// Input type for source item information (used by `scan_volume_for_conflicts`).
#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceItemInput {
    /// File/directory name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix timestamp in seconds).
    pub modified: Option<i64>,
    /// `true` when the source item is a directory. The FE has this from the
    /// `FileEntry` it already holds; it lets `scan_for_conflicts` flag a
    /// dir-vs-dir collision the FE can classify as a silent merge.
    #[serde(default)]
    pub is_directory: bool,
}

#[cfg(test)]
#[path = "conflict_preflight_tests.rs"]
mod tests;
