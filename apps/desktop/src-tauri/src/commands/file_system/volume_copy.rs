//! Tauri commands for cross-volume copy/move operations.
//!
//! These are pass-throughs: they build the `TauriEventSink` at the edge and hand
//! the whole routed transfer to `write_operations::routing`, which owns the volume
//! and destination-path resolution and the archive forks. That split is what lets
//! a backend caller start the same transfer with its own injected sink.

use crate::file_system::{
    OperationEventSink, ScanConflict, TauriEventSink, VolumeCopyConfig, VolumeCopyScanResult, WriteOperationError,
    WriteOperationStartResult, resolve_dest_path, resolve_source_volume,
    scan_for_volume_copy as ops_scan_for_volume_copy, start_volume_compress, start_volume_copy, start_volume_move,
    transfer_would_land_on_its_source,
};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{Volume, VolumeError};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::Duration;

use crate::commands::util::{Deadline, timeout_detached_typed, timeout_detached_within};
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
        // No source binding: the user picked these in the pane they are looking at.
        None,
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
        // No source binding: the user picked these in the pane they are looking at.
        None,
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
) -> Result<VolumeCopyScanResult, VolumeScanError> {
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);

    // Resolve both so an archive-inner source scans through its ArchiveVolume
    // (sizing an extract-out) and the dest routes consistently with the copy op.
    let (source_volume, _) = resolve_source_volume(&source_volume_id, source_paths.first())
        .await
        .ok_or(VolumeScanError::SourceVolumeNotFound {
            volume_id: source_volume_id,
        })?;

    let dest_volume = get_volume_manager()
        .resolve(&dest_volume_id, &dest_path)
        .await
        .volume
        .ok_or(VolumeScanError::DestinationVolumeNotFound {
            volume_id: dest_volume_id,
        })?;

    let max_conflicts = max_conflicts.unwrap_or(100);
    // Same anchoring the copy op applies, so the scan sizes and counts conflicts
    // at the folder the copy will actually write to.
    let dest_path = resolve_dest_path(&dest_volume, dest_path.to_string_lossy().into_owned());

    // Run scan (now async). Detached: a copy scan of an MTP source is a recursive
    // listing that outlives 30 s on any photo-heavy folder, and dropping it
    // mid-`GetObjectInfo` wedges the phone.
    timeout_detached_typed(
        Duration::from_secs(30),
        || VolumeScanError::TimedOut,
        |detail| VolumeScanError::Unexpected { detail },
        async move {
            ops_scan_for_volume_copy(&*source_volume, &source_paths, &*dest_volume, &dest_path, max_conflicts)
                .await
                .map_err(|error| VolumeScanError::Volume { error })
        },
    )
    .await
}

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

impl std::fmt::Display for VolumeScanError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceVolumeNotFound { volume_id } => write!(f, "source volume not found: {volume_id}"),
            Self::DestinationVolumeNotFound { volume_id } => {
                write!(f, "destination volume not found: {volume_id}")
            }
            Self::Volume { error } => write!(f, "volume: {error}"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for VolumeScanError {}

/// Checks which source items already exist at the destination. Returns conflict details for UI.
///
/// When `source_volume_id` and `source_paths` are both provided, each item's
/// `is_directory` and `size` are resolved authoritatively on the source volume
/// via one `get_metadata` per top-level path (see `stat_source_paths`: strictly
/// O(top-level items), never a subtree walk), overriding whatever the caller
/// passed in `source_items`. This lets the dialog classify dir-vs-dir collisions
/// as silent merges without the FE having to plumb per-item types. Callers that
/// don't pass the source volume keep the legacy name-only behavior.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
) -> Result<Vec<ScanConflict>, VolumeScanError> {
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
    .inspect_err(|e| log_conflict_outcome(&deadline, "couldn't reach the destination volume", e))?
    .ok_or(VolumeScanError::DestinationVolumeNotFound { volume_id })?;

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
const CONFLICT_CHECK_BUDGET: Duration = Duration::from_secs(30);

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
    use super::{
        Deadline, SourceItemInput, VolumeScanError, merge_source_types_from_stats, scan_volume_for_conflicts_within,
    };
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use crate::file_system::{InMemoryVolume, LocalPosixVolume, SourceItemInfo};
    use crate::test_support::WedgedVolume;
    use cmdr_fs::entry::FileEntry;
    use cmdr_fs::volume::Volume;
    use std::path::{Path, PathBuf};
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
        assert!(
            matches!(err, VolumeScanError::TimedOut),
            "and it says WHY by variant, so the dialog can offer a retry without reading a sentence"
        );
        assert!(
            started.elapsed() < budget * 4,
            "the whole check owes one budget, not one per leg: took {:?}",
            started.elapsed()
        );
    }

    /// A source that won't answer costs its OWN leg, never the destination's.
    ///
    /// The source stat is an optional refinement — the code says so, and falls
    /// back to the caller's name-only items when it fails. But it used to share
    /// one budget with the destination scan, so a source that never answered
    /// spent all 30 s and the destination scan then failed instantly, reporting
    /// `couldn't read the destination (timed out)` for a destination it had
    /// never asked. That is what a user saw on 2026-08-27 (`ERR-AYVM4`): the
    /// source was one 119k-file folder, and the recursive walk the scan used to
    /// run could never have finished inside any budget.
    #[tokio::test]
    async fn a_source_that_never_answers_still_lets_the_destination_answer() {
        let dest_dir = tempfile::tempdir().expect("dest dir");
        std::fs::write(dest_dir.path().join("photo.jpg"), b"theirs").expect("write dest");
        let _source = TestVolumeRegistration::install(
            "starving-source",
            Arc::new(WedgedVolume::new("WedgedSource")) as Arc<dyn Volume>,
        );
        let _dest = TestVolumeRegistration::install(
            "starving-dest",
            Arc::new(LocalPosixVolume::new("Dest", dest_dir.path())) as Arc<dyn Volume>,
        );

        let budget = Duration::from_millis(900);
        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(budget),
            String::from("starving-dest"),
            vec![input("photo.jpg")],
            dest_dir.path().to_string_lossy().into_owned(),
            Some(String::from("starving-source")),
            Some(vec![String::from("/media/photo.jpg")]),
        )
        .await
        .expect("the destination is fine, so the check answers rather than blaming it");

        assert_eq!(
            conflicts.len(),
            1,
            "the clash is real and the dialog still has to say so, name-only hints or not"
        );
    }

    /// A source pasted into the folder it already lives in is a request to
    /// DUPLICATE it, and both engines answer it by silently auto-renaming
    /// (`write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in
    /// place)"). The pre-flight matches destination entries by NAME, so without
    /// the filter every source of a same-folder copy comes back as its own
    /// conflict, and the dialog announces a conflict count, shows the
    /// overwrite/skip/rename radios, and hands the backend a pre-known-conflict
    /// list naming every source.
    #[tokio::test]
    async fn a_same_folder_copy_finds_no_conflicts() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("photo.jpg"), b"bytes").expect("write source");
        std::fs::create_dir(dir.path().join("docs")).expect("create source dir");
        let _volume = TestVolumeRegistration::install(
            "self-collision-scan",
            Arc::new(LocalPosixVolume::new("Duplicates", dir.path())) as Arc<dyn Volume>,
        );

        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(Duration::from_secs(5)),
            String::from("self-collision-scan"),
            vec![input("photo.jpg"), input("docs")],
            dir.path().to_string_lossy().into_owned(),
            Some(String::from("self-collision-scan")),
            Some(vec![
                dir.path().join("photo.jpg").to_string_lossy().into_owned(),
                dir.path().join("docs").to_string_lossy().into_owned(),
            ]),
        )
        .await
        .expect("the scan answers");

        assert!(
            conflicts.is_empty(),
            "an item landing on itself is a duplicate, not a conflict, but the scan reported {conflicts:?}"
        );
    }

    /// The other half of the same rule: a DIFFERENT file of the same name is
    /// still in the way, and the dialog still has to say so.
    #[tokio::test]
    async fn a_different_file_of_the_same_name_is_still_a_conflict() {
        let source_dir = tempfile::tempdir().expect("source dir");
        let dest_dir = tempfile::tempdir().expect("dest dir");
        std::fs::write(source_dir.path().join("photo.jpg"), b"mine").expect("write source");
        std::fs::write(dest_dir.path().join("photo.jpg"), b"theirs").expect("write dest");
        let _source = TestVolumeRegistration::install(
            "self-collision-scan-source",
            Arc::new(LocalPosixVolume::new("Source", source_dir.path())) as Arc<dyn Volume>,
        );
        let _dest = TestVolumeRegistration::install(
            "self-collision-scan-dest",
            Arc::new(LocalPosixVolume::new("Dest", dest_dir.path())) as Arc<dyn Volume>,
        );

        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(Duration::from_secs(5)),
            String::from("self-collision-scan-dest"),
            vec![input("photo.jpg")],
            dest_dir.path().to_string_lossy().into_owned(),
            Some(String::from("self-collision-scan-source")),
            Some(vec![source_dir.path().join("photo.jpg").to_string_lossy().into_owned()]),
        )
        .await
        .expect("the scan answers");

        assert_eq!(conflicts.len(), 1, "a real clash still reaches the dialog");
        assert_eq!(conflicts[0].source_path, "photo.jpg");
    }

    /// A source reached through a symlinked parent is the same file, and the
    /// LOCAL engine (which a both-local transfer routes to) settles it with
    /// `dev+ino`. A folded-path comparison would miss it and the dialog would
    /// invent a conflict the engine then silently duplicates past.
    #[tokio::test]
    async fn a_source_reached_through_a_symlinked_parent_finds_no_conflicts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("create real dir");
        std::fs::write(real.join("photo.jpg"), b"bytes").expect("write source");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink the parent");
        let _volume = TestVolumeRegistration::install(
            "self-collision-scan-symlink",
            Arc::new(LocalPosixVolume::new("Duplicates", dir.path())) as Arc<dyn Volume>,
        );

        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(Duration::from_secs(5)),
            String::from("self-collision-scan-symlink"),
            vec![input("photo.jpg")],
            real.to_string_lossy().into_owned(),
            Some(String::from("self-collision-scan-symlink")),
            Some(vec![link.join("photo.jpg").to_string_lossy().into_owned()]),
        )
        .await
        .expect("the scan answers");

        assert!(
            conflicts.is_empty(),
            "the symlinked route names the same file, so it is a duplicate: got {conflicts:?}"
        );
    }

    /// The cross-volume arm of the same rule. No backend out here offers an
    /// inode, so identity is one volume, the same parent, and a folded LEAF, and
    /// that fold is what makes a case-differing name (an SMB share, a macOS
    /// volume) count. A case-differing PARENT deliberately does not: see
    /// `transfer/volume/conflict.rs::is_the_same_volume_path`.
    #[tokio::test]
    async fn a_same_folder_copy_on_a_remote_volume_finds_no_conflicts() {
        let volume = Arc::new(InMemoryVolume::new("Device")) as Arc<dyn Volume>;
        volume.create_directory(Path::new("/photos")).await.expect("create dir");
        volume
            .create_file(Path::new("/photos/photo.jpg"), b"pixels")
            .await
            .expect("create file");
        let _registered = TestVolumeRegistration::install("self-collision-scan-device", Arc::clone(&volume));

        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(Duration::from_secs(5)),
            String::from("self-collision-scan-device"),
            vec![input("photo.jpg")],
            String::from("/photos"),
            Some(String::from("self-collision-scan-device")),
            Some(vec![String::from("/photos/Photo.JPG")]),
        )
        .await
        .expect("the scan answers");

        assert!(
            conflicts.is_empty(),
            "the folded leaf names the same item, so it is a duplicate: got {conflicts:?}"
        );
    }

    /// The dialog and the engine have to agree, so the pre-flight draws the
    /// parent line in the same place: a source from a differently-cased folder is
    /// a real clash, not a duplicate. Dropping it here would announce "no
    /// conflicts" for a transfer the engine then prompts about.
    #[tokio::test]
    async fn a_source_from_a_case_differing_folder_is_still_a_conflict() {
        let volume = Arc::new(InMemoryVolume::new("Device")) as Arc<dyn Volume>;
        for dir in ["/photos", "/PHOTOS"] {
            volume.create_directory(Path::new(dir)).await.expect("create dir");
            volume
                .create_file(&Path::new(dir).join("photo.jpg"), b"pixels")
                .await
                .expect("create file");
        }
        let _registered = TestVolumeRegistration::install("self-collision-scan-case-parent", Arc::clone(&volume));

        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(Duration::from_secs(5)),
            String::from("self-collision-scan-case-parent"),
            vec![input("photo.jpg")],
            String::from("/photos"),
            Some(String::from("self-collision-scan-case-parent")),
            Some(vec![String::from("/PHOTOS/photo.jpg")]),
        )
        .await
        .expect("the scan answers");

        assert_eq!(
            conflicts.len(),
            1,
            "a differently-cased parent is another folder as far as we may say: got {conflicts:?}"
        );
    }

    /// Two volumes that happen to spell a path the same way hold two different
    /// items, so the clash is real and the dialog still has to say so.
    #[tokio::test]
    async fn the_same_path_on_two_volumes_is_still_a_conflict() {
        let source = Arc::new(InMemoryVolume::new("Source")) as Arc<dyn Volume>;
        let dest = Arc::new(InMemoryVolume::new("Dest")) as Arc<dyn Volume>;
        for volume in [&source, &dest] {
            volume.create_directory(Path::new("/photos")).await.expect("create dir");
            volume
                .create_file(Path::new("/photos/photo.jpg"), b"pixels")
                .await
                .expect("create file");
        }
        let _source_reg = TestVolumeRegistration::install("self-collision-scan-two-source", source);
        let _dest_reg = TestVolumeRegistration::install("self-collision-scan-two-dest", dest);

        let conflicts = scan_volume_for_conflicts_within(
            Deadline::new(Duration::from_secs(5)),
            String::from("self-collision-scan-two-dest"),
            vec![input("photo.jpg")],
            String::from("/photos"),
            Some(String::from("self-collision-scan-two-source")),
            Some(vec![String::from("/photos/photo.jpg")]),
        )
        .await
        .expect("the scan answers");

        assert_eq!(conflicts.len(), 1, "two volumes, two items, one real clash");
    }

    fn input(name: &str) -> SourceItemInput {
        SourceItemInput {
            name: name.to_string(),
            size: 0,
            modified: None,
            is_directory: false,
        }
    }

    fn stat(path: &str, is_dir: bool, bytes: u64) -> (PathBuf, FileEntry) {
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        (
            PathBuf::from(path),
            FileEntry {
                size: Some(bytes),
                ..FileEntry::new(name, path.to_string(), is_dir, false)
            },
        )
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
        let stats = vec![stat("/src/photos", true, 999_999), stat("/src/readme.txt", false, 42)];

        merge_source_types_from_stats(&mut items, &stats);

        // The directory item is now flagged as such; whatever size the stat
        // reported for it is deliberately NOT copied (a dir's conflict size is
        // meaningless).
        assert!(items[0].is_directory);
        assert_eq!(items[0].size, 0);
        // The file item gets its real size.
        assert!(!items[1].is_directory);
        assert_eq!(items[1].size, 42);
    }

    #[test]
    fn keeps_caller_values_when_no_stat_hit() {
        let mut items = vec![SourceItemInfo {
            name: "ghost".to_string(),
            size: 7,
            modified: Some(123),
            is_directory: true,
        }];
        let stats = vec![stat("/src/other", false, 1)];

        merge_source_types_from_stats(&mut items, &stats);

        // No matching name → the caller's values survive untouched.
        assert!(items[0].is_directory);
        assert_eq!(items[0].size, 7);
        assert_eq!(items[0].modified, Some(123));
    }
}
