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
//! Each ledger entry also carries what its original looked like right before
//! the copy read it (`SourceStamp`). An original that no longer matches was
//! saved over after the copy started, so the destination holds the old bytes
//! and the source holds the only copy of the new ones: it stays, and the
//! operation reports it beside what appeared.
//!
//! ❌ Never reach for `remove_dir_all` here. The source is the only other copy of
//! the data until Phase 3 lands and Phase 4 runs, and a recursive delete acts on
//! what is on disk NOW rather than on what this move carried.

use std::collections::HashSet;
use std::fs;

use cmdr_fs::apple_double::SidecarRule;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::error_classification::IoResultExt;
use crate::event_sinks::OperationEventSink;
use crate::ledger::{NodeId, WrittenIdentity};
use crate::locked_sources::LockedPlan;
use crate::state::ScanResult;
use crate::state::WriteOperationState;
use crate::transfer_sides::{MoveSourceCounts, TransferSide};
use crate::types::{
    CancelRollback, SourceItemOutcome, WriteCancelledEvent, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use crate::validation::is_real_directory;

use super::super::left_in_source::LeftInSource;

/// What a source file looked like right before its copy began: size,
/// modification time, and node. The sweep stats the original again before it
/// deletes, and any difference means someone wrote to it after the copy started.
///
/// An mtime is sound HERE though the in-flight ledgers refuse one
/// (`../DETAILS.md` § "What the in-flight ledgers record"): both stats read the
/// SAME file on the SAME filesystem, so a coarse clock (FAT's two seconds)
/// truncates both alike and can't invent a change. What it can do is hide a
/// same-size save inside one tick, which is why the node rides along: an
/// editor's write-temp-then-rename gives the original a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceStamp {
    size: u64,
    modified: Option<SystemTime>,
    node: Option<NodeId>,
}

impl SourceStamp {
    /// The stamp of the entry at `path`, read with `symlink_metadata` so a
    /// symlink describes itself. `None` when the stat fails, which the sweep
    /// reads as unprovable and keeps.
    pub(super) fn read(path: &Path) -> Option<Self> {
        fs::symlink_metadata(path).ok().map(|metadata| Self::of(&metadata))
    }

    fn of(metadata: &fs::Metadata) -> Self {
        Self {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            node: WrittenIdentity::node_of_stat(metadata),
        }
    }
}

/// One source file this move staged, and what it looked like when the copy
/// read it.
pub(super) struct LandedOriginal {
    pub(super) path: PathBuf,
    /// `None` when the stat before the copy failed: nothing proves the original
    /// is still what landed, so the sweep keeps it.
    pub(super) stamp: Option<SourceStamp>,
}

/// What the sweep may remove under one top-level source.
pub(super) struct SweptSource {
    /// The source files this move staged and landed. Removed with
    /// `remove_file`, which unlinks a symlink rather than following it, unless
    /// the original changed after its copy started.
    landed_files: Vec<LandedOriginal>,
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
        landed_files: Vec<LandedOriginal>,
        scan_result: &ScanResult,
        skipped: HashSet<PathBuf>,
    ) -> Self {
        // Deepest first, so a parent is only tried once its children are gone.
        // `ScanResult.dirs` comes out of the walker in discovery order, and a
        // seeded preview can carry any order at all, so the sweep sorts rather
        // than trusting one.
        //
        // A folder at or under a Skip stays out: it holds what the Skip kept,
        // and an EMPTY one would otherwise go though it never landed.
        let mut scanned_dirs: Vec<PathBuf> = scan_result
            .dirs
            .iter()
            .filter(|dir| skipped.is_empty() || !dir.ancestors().any(|ancestor| skipped.contains(ancestor)))
            .cloned()
            .collect();
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
        for original in landed_files {
            if let Some(index) = sources.iter().position(|source| original.path.starts_with(source)) {
                per_source[index].landed_files.push(original);
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

/// Why the sweep stopped, and how many originals it had removed by then.
///
/// The counts ride out with the error because nobody upstream can work them out
/// afterwards: what the user needs to hear is which of their files are still in
/// the source folder.
#[derive(Debug)]
pub(super) struct SweepStopped {
    pub(super) error: WriteOperationError,
    /// `None` for a stop that speaks for itself (a cancel, which emits its own
    /// event with its own tally).
    pub(super) counts: Option<MoveSourceCounts>,
}

/// The source volume, asked before the sweep may read a missing file as one it
/// already carried.
///
/// ❗ This is the whole reason the sweep knows about drives: every "already
/// gone" answer in this file is a `NotFound` or a failed stat, and a drive
/// that LEFT answers exactly the same way. Without the question, a pulled cable
/// mid-sweep reads as "the move took all of these" and the remaining originals
/// are reported deleted while they sit safely on a drive in someone's hand.
struct SourceDrive<'a> {
    side: Option<&'a TransferSide>,
}

impl SourceDrive<'_> {
    /// The error to stop on when the mount table says the source drive has left,
    /// `None` while it's still there — and `None` when the table can't be read,
    /// because a guess is not evidence.
    fn left(&self, path: &Path) -> Option<WriteOperationError> {
        self.side
            .filter(|side| side.has_left())
            .map(|_| WriteOperationError::DeviceDisconnected {
                path: path.display().to_string(),
                side: None,
            })
    }
}

/// The stop, with the source tally as it stands: removed, and everything else.
fn stop(error: WriteOperationError, originals_removed: usize, sources_total: usize) -> SweepStopped {
    SweepStopped {
        error,
        counts: Some(MoveSourceCounts {
            removed: originals_removed as u32,
            left: sources_total.saturating_sub(originals_removed) as u32,
        }),
    }
}

/// Deletes the originals after a successful cross-FS copy+rename, removing only
/// what the move actually landed at the destination.
///
/// A whole top-level source in the skip set (single-file / type-mismatch Skip)
/// is left untouched. Everything else is swept from the ledger: the landed files
/// go, then the scanned directories under the source, deepest first, each via
/// `remove_dir`, except an original that changed after its copy started. A
/// source that survives the sweep — because a Skip kept a child, something
/// arrived after the scan, or an original changed — ends on `Skipped` with
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
    locked: &LockedPlan,
) -> Result<LeftInSource, SweepStopped> {
    let drive = SourceDrive {
        side: state.sides.as_ref().map(|sides| &sides.source),
    };
    let sources_total = sources.len();
    let mut sources_done = 0usize;
    // Top-level originals this sweep has actually unlinked, counted from what
    // the disk says afterwards rather than from what the move intended.
    let mut originals_removed = 0usize;
    // Originals the sweep has walked past and deliberately left where they are.
    // A stop has to count these alongside the ones it never reached: both are
    // still sitting in the user's source folder.
    let mut originals_spared = 0usize;
    let mut leftovers = LeftInSource::default();
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
            return Err(SweepStopped {
                error: WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".into(),
                },
                counts: None,
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

        // A source that isn't there is ordinarily one this move already carried
        // (a retry, a sync client). On a drive that LEFT it means nothing of the
        // kind, so the drive is asked before that reading is acted on.
        let source_present = fs::symlink_metadata(source).is_ok();
        if !source_present && let Some(error) = drive.left(source) {
            return Err(stop(error, originals_removed, sources_total));
        }
        if source_present {
            let changed = match remove_landed(&sweep.per_source[index], &drive, locked) {
                Ok(changed) => changed,
                Err(error) => return Err(stop(error, originals_removed, sources_total)),
            };

            // Whatever is still standing here was never carried to the
            // destination. `source_removed` is the vanished-path contract, so it
            // reports what the disk says rather than what the move intended.
            let source_removed = fs::symlink_metadata(source).is_err();
            if source_removed && let Some(error) = drive.left(source) {
                return Err(stop(error, originals_removed, sources_total));
            }
            if source_removed {
                originals_removed += 1;
            }
            if !source_removed {
                let appeared = count_unscanned_entries(source, &sweep.scanned_paths, &SidecarRule::new());
                leftovers.note(source, is_real_directory(source), appeared, changed);
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
/// were fixed when the copy phase ended. Answers how many originals it kept
/// because they changed after their copy started; each one keeps its
/// directories alive through their `ENOTEMPTY`.
///
/// What the person agreed to unlock (`locked`) is unlocked right before it goes:
/// a Locked folder first, so its children can leave it (and locked again if it
/// stays), a Locked file right before its unlink. Its copy already landed
/// Locked, so nothing goes back on at the destination.
fn remove_landed(
    swept: &SweptSource,
    drive: &SourceDrive<'_>,
    locked: &LockedPlan,
) -> Result<u32, WriteOperationError> {
    let opened: Vec<_> = swept
        .scanned_dirs
        .iter()
        .filter(|dir| locked.unlocks(dir))
        .map(|dir| (dir, locked.unlock(dir)))
        .collect();
    let mut changed = 0;
    for original in &swept.landed_files {
        if changed_since_copy(original)? {
            changed += 1;
            continue;
        }
        let unlocked = locked.unlock(&original.path);
        remove_landed_file(&original.path, drive)?;
        unlocked.gone();
    }
    for dir in &swept.scanned_dirs {
        remove_swept_dir(dir, drive)?;
    }
    for (dir, unlocked) in opened {
        if fs::symlink_metadata(dir).is_err() {
            unlocked.gone();
        }
    }
    Ok(changed)
}

/// Whether the original is no longer what its copy read. A missing one isn't
/// "changed": `remove_landed_file` owns what a `NotFound` means, drive and all.
/// Any other failed stat stops the sweep rather than deleting a file it
/// couldn't look at.
///
/// A save that lands between this stat and the unlink after it still goes
/// unseen; POSIX has no compare-and-unlink, and the window is two syscalls wide
/// rather than the whole copy.
fn changed_since_copy(original: &LandedOriginal) -> Result<bool, WriteOperationError> {
    match fs::symlink_metadata(&original.path) {
        Ok(metadata) => Ok(original.stamp != Some(SourceStamp::of(&metadata))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_path(&original.path),
    }
}

/// Unlinks one landed source file. `remove_file` acts on a symlink itself, so a
/// link goes as a link and whatever it points at is left alone. A file already
/// gone is a success: something else removed it, and the destination has it —
/// unless the drive holding it has left, where the same `NotFound` means the
/// mount is gone and the file is fine.
fn remove_landed_file(path: &Path, drive: &SourceDrive<'_>) -> Result<(), WriteOperationError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match drive.left(path) {
            Some(vanished) => Err(vanished),
            None => Ok(()),
        },
        Err(e) => Err(e).with_path(path),
    }
}

/// Removes one swept directory, and only if it is empty. A directory still
/// holding something is the sweep working as intended, not a failure: the
/// something was never copied to the destination, so it stays where it is.
fn remove_swept_dir(dir: &Path, drive: &SourceDrive<'_>) -> Result<(), WriteOperationError> {
    let Ok(metadata) = fs::symlink_metadata(dir) else {
        // Already gone — or the whole mount is, which is not the same thing.
        return match drive.left(dir) {
            Some(vanished) => Err(vanished),
            None => Ok(()),
        };
    };
    if metadata.file_type().is_symlink() {
        // A link the scan walked through is still a link on disk. Unlink it;
        // `remove_dir` would either refuse (ENOTDIR) or act through the link.
        return remove_landed_file(dir, drive);
    }
    match fs::remove_dir(dir) {
        Ok(()) => Ok(()),
        // Still holding something the move never carried: the sweep working as
        // intended, and nothing to do with a drive.
        Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match drive.left(dir) {
            Some(vanished) => Err(vanished),
            None => Ok(()),
        },
        Err(e) => Err(e).with_path(dir),
    }
}

/// Counts what the scan never saw under `dir`: what arrived while the move ran.
/// A whole unknown subtree counts once, since that's the item a person would
/// recognize in the pane. Anything the scan DID see isn't news — a Skip the user
/// asked for is not a surprise — so it isn't counted. Nor is the AppleDouble
/// sidecar of an item that stayed: the scan left it out on purpose, and it's
/// that item's xattrs (`cmdr_fs::apple_double`).
fn count_unscanned_entries(dir: &Path, scanned_paths: &HashSet<PathBuf>, sidecars: &SidecarRule) -> u32 {
    let (Ok(dir_meta), Ok(entries)) = (fs::symlink_metadata(dir), fs::read_dir(dir)) else {
        return 0;
    };
    let device = {
        use std::os::unix::fs::MetadataExt;
        dir_meta.dev()
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !scanned_paths.contains(&path) {
            if !sidecars.is_sidecar(dir, device, &entry.file_name()) {
                count += 1;
            }
        } else if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
            count += count_unscanned_entries(&path, scanned_paths, sidecars);
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
    state.scope().status().update(
        operation_id,
        WriteOperationPhase::Deleting,
        current_file,
        sources_done,
        sources_total,
        0,
        0,
    );
}
