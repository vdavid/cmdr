//! Tauri commands for cross-volume copy/move operations.
//!
//! These are pass-throughs: they build the `TauriEventSink` at the edge and hand
//! the whole routed transfer to `write_operations::routing`, which owns the volume
//! and destination-path resolution and the archive forks. That split is what lets
//! a backend caller start the same transfer with its own injected sink.

use crate::file_system::{
    OperationEventSink, ScanConflict, TauriEventSink, VolumeCopyConfig, VolumeCopyScanResult, WriteOperationError,
    WriteOperationStartResult, resolve_dest_path, resolve_source_volume, scan_for_volume_copy as ops_scan_for_volume_copy,
    start_volume_compress, start_volume_copy, start_volume_move,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;

use crate::commands::util::{Deadline, IpcError, timeout_detached, timeout_detached_within};
use crate::file_system::volume::manager::get_volume_manager;
use crate::operation_log::types::Initiator;

/// Unified copy across volume types (local, MTP, extract out of a `.zip`).
/// Same events as `copy_files`.
#[tauri::command]
#[specta::specta]
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    start_volume_copy(
        events,
        source_volume_id,
        source_paths.iter().map(PathBuf::from).collect(),
        dest_volume_id,
        dest_path,
        config.unwrap_or_default(),
        initiator.unwrap_or(Initiator::User),
    )
    .await
}

/// Unified move across volume types. Handles same-volume (native rename/move),
/// both-local (native move), cross-volume (copy+delete), and both directions
/// across a `.zip` boundary.
#[tauri::command]
#[specta::specta]
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    start_volume_move(
        events,
        source_volume_id,
        source_paths.iter().map(PathBuf::from).collect(),
        dest_volume_id,
        dest_path,
        config.unwrap_or_default(),
        initiator.unwrap_or(Initiator::User),
    )
    .await
}

/// Compresses `source_paths` into a NEW zip at `dest_zip_path` on `dest_volume_id`.
/// Same events as `copy_between_volumes`. The destination may be LOCAL or REMOTE
/// (SMB/MTP).
#[tauri::command]
#[specta::specta]
pub async fn compress_files(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_zip_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    start_volume_compress(
        events,
        source_volume_id,
        source_paths.iter().map(PathBuf::from).collect(),
        dest_volume_id,
        dest_zip_path,
        config.unwrap_or_default(),
        initiator.unwrap_or(Initiator::User),
    )
    .await
}

/// Pre-flight scan: total count/bytes, available space, conflicts. Doesn't copy anything.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_copy(
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    max_conflicts: Option<usize>,
) -> Result<VolumeCopyScanResult, IpcError> {
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);

    // Resolve both so an archive-inner source scans through its ArchiveVolume
    // (sizing an extract-out) and the dest routes consistently with the copy op.
    let (source_volume, _) = resolve_source_volume(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| IpcError::from_err(format!("Source volume '{}' not found", source_volume_id)))?;

    let dest_volume = get_volume_manager()
        .resolve(&dest_volume_id, &dest_path)
        .await
        .volume
        .ok_or_else(|| IpcError::from_err(format!("Destination volume '{}' not found", dest_volume_id)))?;

    let max_conflicts = max_conflicts.unwrap_or(100);
    // Same anchoring the copy op applies, so the scan sizes and counts conflicts
    // at the folder the copy will actually write to.
    let dest_path = resolve_dest_path(&dest_volume, dest_path.to_string_lossy().into_owned());

    // Run scan (now async). Detached: a copy scan of an MTP source is a recursive
    // listing that outlives 30 s on any photo-heavy folder, and dropping it
    // mid-`GetObjectInfo` wedges the phone.
    timeout_detached(Duration::from_secs(30), async move {
        ops_scan_for_volume_copy(&*source_volume, &source_paths, &*dest_volume, &dest_path, max_conflicts)
            .await
            .map_err(|e| e.to_string())
    })
    .await
}

/// Checks which source items already exist at the destination. Returns conflict details for UI.
///
/// When `source_volume_id` and `source_paths` are both provided, each item's
/// `is_directory` and `size` are resolved authoritatively on the source volume
/// via ONE batched stat (`scan_for_copy_batch`, strictly O(top-level items),
/// never a subtree walk), overriding whatever the caller passed in `source_items`.
/// This lets the dialog classify dir-vs-dir collisions as silent merges without
/// the FE having to plumb per-item types. Callers that don't pass the source
/// volume keep the legacy name-only behavior.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
) -> Result<Vec<ScanConflict>, IpcError> {
    scan_volume_for_conflicts_within(
        Deadline::new(CONFLICT_CHECK_BUDGET),
        volume_id,
        source_items,
        dest_path,
        source_volume_id,
        source_paths,
    )
    .await
}

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
) -> Result<Vec<ScanConflict>, IpcError> {
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
    let volume = timeout_detached_within(&deadline, async move {
        Ok::<_, String>(get_volume_manager().resolve(&resolve_id, &resolve_path).await.volume)
    })
    .await
    .inspect_err(|e| log_conflict_outcome(&deadline, "couldn't reach the destination volume", e))?
    .ok_or_else(|| IpcError::from_err(format!("Volume '{}' not found", volume_id)))?;

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

    // Resolve real per-item types and sizes from the source volume when the
    // caller supplied it. One batched stat, O(top-level items). `resolve_source`
    // routes an archive-inner source through its ArchiveVolume.
    if let (Some(src_volume_id), Some(src_paths)) = (source_volume_id, source_paths) {
        let paths: Vec<PathBuf> = src_paths.iter().map(PathBuf::from).collect();
        let first = paths.first().cloned();
        let resolved = timeout_detached_within(&deadline, async move {
            Ok::<_, String>(resolve_source_volume(&src_volume_id, first.as_ref()).await)
        })
        .await;
        if let Ok(Some((src_volume, _))) = resolved {
            // Detached (see `timeout_detached`): the batch stat reaches the
            // source device, so the deadline must not drop it.
            let batch = timeout_detached_within(&deadline, async move {
                src_volume.scan_for_copy_batch(&paths).await.map_err(|e| e.to_string())
            })
            .await;
            match batch {
                Ok(batch) => merge_source_types_from_batch(&mut source_items, &batch),
                // A source-side stat that doesn't come back is non-fatal: fall
                // back to the name-only items the caller sent. Conflict
                // detection still works by name; only the dir/size hints
                // degrade, so the check can still answer.
                Err(e) => {
                    log::debug!(
                        target: CONFLICT_LOG_TARGET,
                        "source batch stat unavailable after {:.0}s, using name-only items: {}",
                        deadline.elapsed().as_secs_f64(),
                        e.message
                    );
                }
            }
        }
    }

    // Run conflict scan (now async), detached so the destination device isn't
    // left mid-transaction if the scan overruns.
    let found = timeout_detached_within(&deadline, async move {
        volume
            .scan_for_conflicts(&source_items, &dest_path)
            .await
            .map_err(|e| e.to_string())
    })
    .await;
    match &found {
        Ok(conflicts) => log::debug!(
            target: CONFLICT_LOG_TARGET,
            "found {} collision(s) in {:.1}s",
            conflicts.len(),
            deadline.elapsed().as_secs_f64()
        ),
        Err(e) => log_conflict_outcome(&deadline, "couldn't read the destination", e),
    }
    found
}

/// The one line a check that couldn't answer leaves behind. WARN, because the
/// dialog is about to tell the user it doesn't know what's at their destination,
/// and that's exactly the state worth noticing in a log.
fn log_conflict_outcome(deadline: &Deadline, what: &str, e: &IpcError) {
    log::warn!(
        target: CONFLICT_LOG_TARGET,
        "conflict check gave up after {:.1}s: {} ({})",
        deadline.elapsed().as_secs_f64(),
        what,
        e.message
    );
}

/// Overlays authoritative `is_directory` + `size` from a source-volume batch
/// stat onto the caller-supplied `source_items`, matched by base filename.
///
/// The match key is the path's final component, which is exactly the `name`
/// the FE derives for each `SourceItemInput`. An item with no batch hit keeps
/// the values the caller sent (the safe fallback). For a top-level directory
/// the batch's `total_bytes` is the recursive size, which we deliberately do
/// NOT copy into `size` — a directory's conflict-UI size is meaningless and the
/// dir-dir case never renders a size. Only files get their real size.
fn merge_source_types_from_batch(
    source_items: &mut [crate::file_system::SourceItemInfo],
    batch: &crate::file_system::BatchScanResult,
) {
    use std::collections::HashMap;
    let by_name: HashMap<&str, &crate::file_system::CopyScanResult> = batch
        .per_path
        .iter()
        .filter_map(|(path, scan)| path.file_name().and_then(|n| n.to_str()).map(|n| (n, scan)))
        .collect();
    for item in source_items.iter_mut() {
        if let Some(scan) = by_name.get(item.name.as_str()) {
            item.is_directory = scan.top_level_is_directory;
            if !scan.top_level_is_directory {
                item.size = scan.total_bytes;
            }
        }
    }
}

/// How long the whole conflict check may take before it answers "I couldn't".
///
/// One budget for every leg, not one per leg. 30 s is the tier this codebase
/// already gives a recursive scan over IPC, and it's what the dialog's spinner
/// is sized against.
const CONFLICT_CHECK_BUDGET: Duration = Duration::from_secs(30);

/// The log target every conflict-check line carries.
const CONFLICT_LOG_TARGET: &str = "conflict_scan";

/// Input type for source item information (used by scan_volume_for_conflicts).
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
mod tests {
    use super::{Deadline, SourceItemInput, merge_source_types_from_batch, scan_volume_for_conflicts_within};
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use crate::file_system::{BatchScanResult, CopyScanResult, SourceItemInfo};
    use crate::test_support::WedgedVolume;
    use cmdr_fs::volume::Volume;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    /// The check must answer, or say it couldn't, within its budget — including
    /// when the volumes it asks have stopped answering entirely.
    ///
    /// A user watching `Checking for conflicts...` over a share whose connection
    /// had dropped waited it out for minutes. Each leg owning its own timeout is
    /// not enough: the legs run in sequence, so the promise is their SUM, and a
    /// leg with no timeout at all (the volume resolves) makes it unbounded.
    #[tokio::test]
    async fn a_check_against_volumes_that_never_answer_gives_up_inside_its_budget() {
        let budget = Duration::from_millis(300);
        let _source = TestVolumeRegistration::install(
            "wedged-source",
            Arc::new(WedgedVolume::new("WedgedSource")) as Arc<dyn Volume>,
        );
        let _dest = TestVolumeRegistration::install(
            "wedged-dest",
            Arc::new(WedgedVolume::new("WedgedDest")) as Arc<dyn Volume>,
        );

        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            scan_volume_for_conflicts_within(
                Deadline::new(budget),
                String::from("wedged-dest"),
                vec![SourceItemInput {
                    name: String::from("holiday.mov"),
                    size: 0,
                    modified: None,
                    is_directory: false,
                }],
                String::from("/incoming"),
                Some(String::from("wedged-source")),
                Some(vec![String::from("/media/holiday.mov")]),
            ),
        )
        .await
        .expect("the check must come back on its own, not be cut off by the test");

        let err = outcome.expect_err("a check that never reached either volume cannot report 'no conflicts'");
        assert!(err.timed_out, "and it says WHY, so the dialog can offer a retry");
        assert!(
            started.elapsed() < budget * 4,
            "the whole check owes one budget, not one per leg: took {:?}",
            started.elapsed()
        );
    }

    fn scan(is_dir: bool, bytes: u64) -> CopyScanResult {
        CopyScanResult {
            file_count: if is_dir { 0 } else { 1 },
            dir_count: if is_dir { 1 } else { 0 },
            total_bytes: bytes,
            dedup_bytes: bytes,
            top_level_is_directory: is_dir,
        }
    }

    fn item(name: &str) -> SourceItemInfo {
        SourceItemInfo {
            name: name.to_string(),
            size: 0,
            modified: None,
            is_directory: false,
        }
    }

    #[test]
    fn overlays_real_directory_flag_onto_placeholder_items() {
        let mut items = vec![item("photos"), item("readme.txt")];
        let batch = BatchScanResult {
            aggregate: scan(false, 0),
            per_path: vec![
                (PathBuf::from("/src/photos"), scan(true, 999_999)),
                (PathBuf::from("/src/readme.txt"), scan(false, 42)),
            ],
        };

        merge_source_types_from_batch(&mut items, &batch);

        // The directory item is now flagged as such; its recursive byte total
        // is deliberately NOT copied into `size` (a dir's conflict size is
        // meaningless).
        assert!(items[0].is_directory);
        assert_eq!(items[0].size, 0);
        // The file item gets its real size.
        assert!(!items[1].is_directory);
        assert_eq!(items[1].size, 42);
    }

    #[test]
    fn keeps_caller_values_when_no_batch_hit() {
        let mut items = vec![SourceItemInfo {
            name: "ghost".to_string(),
            size: 7,
            modified: Some(123),
            is_directory: true,
        }];
        let batch = BatchScanResult {
            aggregate: scan(false, 0),
            per_path: vec![(PathBuf::from("/src/other"), scan(false, 1))],
        };

        merge_source_types_from_batch(&mut items, &batch);

        // No matching name → the caller's values survive untouched.
        assert!(items[0].is_directory);
        assert_eq!(items[0].size, 7);
        assert_eq!(items[0].modified, Some(123));
    }
}
