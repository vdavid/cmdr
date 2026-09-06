//! Phase 4 of the cross-filesystem move: removing the originals.
//!
//! The sweep deletes a LEDGER, never a tree. Its input is the set of source
//! files the staging copy actually landed at the destination, plus the
//! directories the scan saw; it removes exactly those, closing every directory
//! with `fs::remove_dir`. Anything else in the source — a download that finished
//! while the move ran, an original a Skip left standing — turns that call into an
//! `ENOTEMPTY` the sweep honors: the directory stays, the item stays, and the
//! operation reports what it left.
//!
//! ❌ Never reach for `remove_dir_all` here. The source is the only other copy of
//! the data until Phase 3 lands and Phase 4 runs, and a recursive delete acts on
//! what is on disk NOW rather than on what this move carried.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::file_system::write_operations::error_classification::IoResultExt;
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::state::ScanResult;
use crate::file_system::write_operations::state::{WriteOperationState, update_operation_status};
use crate::file_system::write_operations::types::{
    AppearedDuringMove, CancelRollback, SourceItemOutcome, WriteCancelledEvent, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};

/// What the sweep may remove under one top-level source.
pub(super) struct SweptSource {
    /// The source files this move staged and landed. Removed with
    /// `remove_file`, which unlinks a symlink rather than following it.
    landed_files: Vec<PathBuf>,
    /// The scanned directories under this source, deepest first, the source
    /// itself last. Each goes through `remove_dir`.
    scanned_dirs: Vec<PathBuf>,
}

/// Exactly what the source sweep may remove, and how it tells a leftover from
/// something it kept on purpose.
pub(super) struct SourceSweep {
    /// One entry per top-level source, in the caller's `sources` order.
    per_source: Vec<SweptSource>,
    /// Every path the scan saw. What the sweep meets that is NOT in here
    /// arrived after the scan, and is what the completion event counts.
    scanned_paths: HashSet<PathBuf>,
    /// Originals a Skip left standing: whole top-level sources, and per-child
    /// paths inside a directory merge. Never in `landed_files` either, so this
    /// set only decides what the sweep SAYS about a source, not what it removes.
    skipped: HashSet<PathBuf>,
}

impl SourceSweep {
    /// Plans the sweep from what Phase 2 staged (`landed_files`, already free of
    /// anything a later Skip discarded), the scan behind it, and the Skips both
    /// phases recorded.
    pub(super) fn plan(
        sources: &[PathBuf],
        landed_files: Vec<PathBuf>,
        scan_result: &ScanResult,
        skipped: HashSet<PathBuf>,
    ) -> Self {
        // Deepest first, so a parent is only tried once its children are gone.
        // `ScanResult.dirs` comes out of the walker in discovery order, and a
        // seeded preview can carry any order at all, so the sweep sorts rather
        // than trusting one.
        let mut scanned_dirs = scan_result.dirs.clone();
        scanned_dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));

        let mut per_source: Vec<SweptSource> = sources
            .iter()
            .map(|source| SweptSource {
                landed_files: Vec::new(),
                scanned_dirs: scanned_dirs
                    .iter()
                    .filter(|dir| dir.starts_with(source))
                    .cloned()
                    .collect(),
            })
            .collect();
        for (index, source) in sources.iter().enumerate() {
            // The source itself closes its own list. An ordinary scan already
            // names it; a seeded preview may not, and a directory that never
            // gets the chance to go would report itself left behind.
            if source.is_dir() && !per_source[index].scanned_dirs.iter().any(|dir| dir == source) {
                per_source[index].scanned_dirs.push(source.clone());
            }
        }
        for file in landed_files {
            if let Some(index) = sources.iter().position(|source| file.starts_with(source)) {
                per_source[index].landed_files.push(file);
            }
        }

        let scanned_paths = scan_result
            .files
            .iter()
            .map(|file| file.path.clone())
            .chain(scan_result.dirs.iter().cloned())
            .collect();

        Self {
            per_source,
            scanned_paths,
            skipped,
        }
    }
}

/// What the sweep left in the source because this move never carried it there.
///
// DEFAULT-OK: the zero value is the claim "the sweep has found nothing left
// behind", which is exactly true of a sweep that hasn't run yet and stays true
// of one that took every source it was given.
#[derive(Default)]
pub(super) struct SweepLeftovers {
    /// Items the scan never saw, counted once per unknown subtree.
    item_count: u32,
    /// The names of the top-level sources that kept one, in sweep order.
    folders: Vec<String>,
}

impl SweepLeftovers {
    /// The completion event's typed field, or `None` when the move took
    /// everything it was asked to take (the ordinary case).
    pub(super) fn appeared_during_move(&self) -> Option<AppearedDuringMove> {
        let folder_name = self.folders.first()?.clone();
        Some(AppearedDuringMove {
            item_count: self.item_count,
            folder_name,
            folder_count: self.folders.len() as u32,
        })
    }
}

/// Deletes the originals after a successful cross-FS copy+rename, removing only
/// what the move actually landed at the destination.
///
/// A whole top-level source in the skip set (single-file / type-mismatch Skip)
/// is left untouched. Everything else is swept from the ledger: the landed files
/// go, then the scanned directories under the source, deepest first, each via
/// `remove_dir`. A source that survives the sweep — because a Skip kept a child,
/// or because something arrived after the scan — ends on `Skipped` with
/// `source_removed: false`, so the pane keeps it selected and the search
/// snapshot keeps its row.
///
/// ## Why this phase reports progress
///
/// It is the last real work of the move and it is unbounded: however many files
/// and directories the move carried, over however large a tree. Running it
/// silently left the frontend on the copy phase's last tick, `files_done ==
/// files_total`, so the dialog read 100% (and "Paused" over a full bar if the
/// user parked it here) with the whole sweep still ahead — the exact "looks
/// finished when it isn't" the honest-progress principle forbids.
///
/// The denominator is the TOP-LEVEL sources, because that's what the loop
/// iterates. Bytes stay zero throughout — nothing is transferred — which is how
/// the readout knows to drop its size bar rather than freeze it at whatever the
/// copy left.
///
/// ## What a stop here reports
///
/// Nothing is reversed and nothing can be: the copy is across a filesystem
/// boundary already, so `outcome` is `NotRolledBack`. But the state this leaves
/// is worth a sentence — the whole copy landed and was flushed before the sweep
/// began, some originals are gone for good, and the rest are duplicates of files
/// that now live at the destination. So the cancel carries
/// `originals_still_in_place`: the sources the sweep never reached, plus the
/// Skipped ones it walked past on purpose, both of which the user still has in
/// the source folder. Counted in the same top-level items as the bar above.
pub(super) fn delete_sources_after_move(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    files_done: usize,
    sweep: &SourceSweep,
) -> Result<SweepLeftovers, WriteOperationError> {
    let sources_total = sources.len();
    let mut sources_done = 0usize;
    // Originals the sweep has walked past and deliberately left where they are.
    // A stop has to count these alongside the ones it never reached: both are
    // still sitting in the user's source folder.
    let mut originals_spared = 0usize;
    let mut leftovers = SweepLeftovers::default();
    let mut last_progress_time = Instant::now();

    // The opening tick, unthrottled: it's what flips the frontend off the copy's
    // full bar and onto this phase's own, and the first source can take minutes.
    emit_source_sweep_progress(events, state, operation_id, None, 0, sources_total);

    for (index, source) in sources.iter().enumerate() {
        // The cooperative boundary, like every other loop in the engine. The
        // destination is already durable by now, so parking here holds the
        // originals in place, which is exactly what a paused move should look
        // like.
        if state.stop_or_park_sync() {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                files_processed: files_done,
                // No reversal, and none is possible: the copy is already across
                // a filesystem boundary. What the report CAN say is where the
                // user's files are — the whole copy landed and was flushed
                // before this phase began, and these originals are still in
                // their old place. ❌ Never emit a bare `none()` here: the
                // readout renders that as SILENCE, and every original the sweep
                // already reached is gone for good.
                rollback: CancelRollback::none()
                    .with_originals_still_in_place((sources_total - sources_done + originals_spared) as u32),
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // A whole top-level source skipped on a file / type-mismatch conflict:
        // leave the original exactly where it is, and say so. The staging phase
        // already reported this source as `Done` (it staged fine); this later
        // event is the operation's real verdict on it, which is why the LAST
        // event a source gets is the one that counts (`SourceItemOutcome`).
        if sweep.skipped.contains(source) {
            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
                source_removed: false,
                outcome: SourceItemOutcome::Skipped,
            });
            // Counted anyway: the bar measures how far through the sources the
            // sweep has got, and a deliberate skip is as finished with as a
            // deletion. Leaving it out would strand the bar short of full.
            sources_done += 1;
            originals_spared += 1;
            continue;
        }

        if fs::symlink_metadata(source).is_ok() {
            remove_landed(&sweep.per_source[index])?;

            // Whatever is still standing here was never carried to the
            // destination. `source_removed` is the vanished-path contract, so it
            // reports what the disk says rather than what the move intended.
            let source_removed = fs::symlink_metadata(source).is_err();
            if !source_removed {
                let appeared = count_unscanned_entries(source, &sweep.scanned_paths);
                if appeared > 0 {
                    leftovers.item_count += appeared;
                    leftovers.folders.push(
                        source
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| source.display().to_string()),
                    );
                }
            }
            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
                source_removed,
                outcome: if source_removed {
                    SourceItemOutcome::Done
                } else {
                    SourceItemOutcome::Skipped
                },
            });
        }

        sources_done += 1;
        if last_progress_time.elapsed() >= state.progress_interval {
            let name = source.file_name().map(|n| n.to_string_lossy().into_owned());
            emit_source_sweep_progress(events, state, operation_id, name, sources_done, sources_total);
            last_progress_time = Instant::now();
        }
    }

    // The closing tick, unthrottled for the same reason as the opening one: the
    // throttle would otherwise leave the bar short of full on a fast sweep, and
    // the next thing the user sees is `write-complete`.
    emit_source_sweep_progress(events, state, operation_id, None, sources_done, sources_total);

    Ok(leftovers)
}

/// Removes one top-level source's landed files, then the directories that held
/// them, deepest first. Nothing here recurses over what is on disk: the lists
/// were fixed when the copy phase ended.
fn remove_landed(swept: &SweptSource) -> Result<(), WriteOperationError> {
    for file in &swept.landed_files {
        remove_landed_file(file)?;
    }
    for dir in &swept.scanned_dirs {
        remove_swept_dir(dir)?;
    }
    Ok(())
}

/// Unlinks one landed source file. `remove_file` acts on a symlink itself, so a
/// link goes as a link and whatever it points at is left alone. A file already
/// gone is a success: something else removed it, and the destination has it.
fn remove_landed_file(path: &Path) -> Result<(), WriteOperationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_path(path),
    }
}

/// Removes one swept directory, and only if it is empty. A directory still
/// holding something is the sweep working as intended, not a failure: the
/// something was never copied to the destination, so it stays where it is.
fn remove_swept_dir(dir: &Path) -> Result<(), WriteOperationError> {
    let Ok(metadata) = fs::symlink_metadata(dir) else {
        // Already gone.
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        // A link the scan walked through is still a link on disk. Unlink it;
        // `remove_dir` would either refuse (ENOTDIR) or act through the link.
        return remove_landed_file(dir);
    }
    match fs::remove_dir(dir) {
        Ok(()) => Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(e).with_path(dir),
    }
}

/// Counts what the scan never saw under `dir`: what arrived while the move ran.
/// A whole unknown subtree counts once, since that's the item a person would
/// recognize in the pane. Anything the scan DID see isn't news — a Skip the user
/// asked for is not a surprise — so it isn't counted.
fn count_unscanned_entries(dir: &Path, scanned_paths: &HashSet<PathBuf>) -> u32 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !scanned_paths.contains(&path) {
            count += 1;
        } else if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
            count += count_unscanned_entries(&path, scanned_paths);
        }
    }
    count
}

/// One `Deleting`-phase tick for the source sweep, paired with its status-cache
/// update so no caller can emit one without the other.
fn emit_source_sweep_progress(
    events: &dyn OperationEventSink,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
    current_file: Option<String>,
    sources_done: usize,
    sources_total: usize,
) {
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            WriteOperationPhase::Deleting,
            current_file.clone(),
            sources_done,
            sources_total,
            0,
            0,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Deleting,
        current_file,
        sources_done,
        sources_total,
        0,
        0,
    );
}
