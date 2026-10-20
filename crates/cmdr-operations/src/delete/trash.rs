//! Trash implementation for write operations.
//!
//! Provides `move_to_trash_sync()` (reusable core) and `trash_files_with_progress()`
//! (batch operation with progress, cancellation, and partial failure support).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use cmdr_operation_log::types::OperationId;

use super::super::event_sinks::OperationEventSink;
use super::super::mutation_error::MutationError;
use super::super::state::WriteOperationState;
use super::super::types::{
    CancelRollback, SourceItemOutcome, TrashRefusalKind, TrashRefusedItems, WriteCancelledEvent, WriteCompleteEvent,
    WriteErrorEvent, WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
    WriteSourceItemDoneEvent,
};
use cmdr_fs::log_private::LogPrivate;

// ============================================================================
// Core trash function (reusable by commands/rename.rs and batch trash)
// ============================================================================

/// Synchronous trash implementation using macOS NSFileManager.trashItem.
///
/// Returns the item's **in-trash location** (`Some` on macOS, where the OS
/// reports it): the journal records it as the trash row's dest so a later restore
/// knows where the OS put the item (the trash rollback depends on it). `None`
/// means "trashed, but no restore location recorded" (Linux, or the rare case the
/// OS omitted the URL).
///
/// Uses `symlink_metadata()` for existence checks so dangling symlinks
/// are handled correctly (the link itself exists even if its target doesn't).
///
/// The macOS in-trash URL comes from `trashItemAtURL:resultingItemURL:error:`,
/// which populates the out-param with the final URL inside the user's Trash
/// (verified on macOS 15, live trash of a temp file returns a `~/.Trash/…` path,
/// 2026-07-10). NSFileManager may de-duplicate the name (`file 2.txt`) if the
/// Trash already holds one, so the returned location is the authoritative one.
#[cfg(target_os = "macos")]
pub fn move_to_trash_sync(path: &Path) -> Result<Option<PathBuf>, MutationError> {
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    if fs::symlink_metadata(path).is_err() {
        return Err(MutationError::NotFound {
            path: path.display().to_string(),
        });
    }

    // Drain autoreleased ObjC objects (NSURL, NSString, NSFileManager internals).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let path_str = path.to_string_lossy();
        let ns_path = NSString::from_str(&path_str);
        let url = NSURL::fileURLWithPath(&ns_path);
        let file_manager = NSFileManager::defaultManager();

        // Capture resultingItemURL (the final location inside Trash) so the
        // journal can record where to restore from (the trash rollback).
        let mut resulting: Option<Retained<NSURL>> = None;
        file_manager
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
            .map_err(|e| MutationError::TrashRefused {
                reason: refine_permission_refusal(path, classify_trash_refusal(&e)),
                detail: e.to_string(),
            })?;
        let in_trash = resulting.and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()));
        Ok(in_trash)
    })
}

/// Read an `NSError` from `trashItemAtURL` as a [`TrashRefusalKind`].
///
/// ❗ Domain plus code, ❌ never the message. `localizedDescription` is written for
/// the user, ships in the user's language, and Apple rewords it between releases;
/// classifying on it means the dialog silently stops offering the right next step
/// for everyone who isn't running English. `error-string-match` enforces this.
///
/// The codes are Foundation's `NSCocoaErrorDomain` constants (`NSFileReadNoPermissionError`
/// 257, `NSFileWriteNoPermissionError` 513, `NSFeatureUnsupportedError` 3328), which
/// are ABI-stable and documented; objc2 doesn't re-export them, so they're spelled
/// here with their names. (Verified against Foundation's `FoundationErrors.h` on
/// macOS 27.0, 2026-09-16.)
#[cfg(target_os = "macos")]
fn classify_trash_refusal(error: &objc2_foundation::NSError) -> TrashRefusalKind {
    const NS_FILE_READ_NO_PERMISSION: isize = 257;
    const NS_FILE_WRITE_NO_PERMISSION: isize = 513;
    const NS_FEATURE_UNSUPPORTED: isize = 3328;
    const EPERM: isize = 1;
    const EACCES: isize = 13;

    let domain = error.domain().to_string();
    let code = error.code();
    match domain.as_str() {
        "NSCocoaErrorDomain" => match code {
            NS_FILE_READ_NO_PERMISSION | NS_FILE_WRITE_NO_PERMISSION => TrashRefusalKind::NotPermitted,
            NS_FEATURE_UNSUPPORTED => TrashRefusalKind::NoTrashForVolume,
            _ => TrashRefusalKind::Other,
        },
        "NSPOSIXErrorDomain" => match code {
            EPERM | EACCES => TrashRefusalKind::NotPermitted,
            _ => TrashRefusalKind::Other,
        },
        _ => TrashRefusalKind::Other,
    }
}

/// Narrow a [`TrashRefusalKind::NotPermitted`] down to what the item and its folder
/// say about themselves, since the `NSError` can't: a Locked item and an item in a
/// folder that takes no writes both refuse as 513 (verified on macOS 27.0 with
/// `trashItemAtURL` on a `chflags uchg` file and on a file in a `0555` folder,
/// 2026-10-14). Neither is a grant away, and a permanent delete refuses them too, so
/// each gets its own advice. Asked right after the refusal: `trashItemAtURL` is
/// atomic, so the item is still where it was.
#[cfg(target_os = "macos")]
fn refine_permission_refusal(path: &Path, reason: TrashRefusalKind) -> TrashRefusalKind {
    use super::super::error_classification::is_locked_item;
    use super::super::validation::folder_write_refusal;

    if reason != TrashRefusalKind::NotPermitted {
        return reason;
    }
    if is_locked_item(path) {
        return TrashRefusalKind::Locked;
    }
    if path
        .parent()
        .is_some_and(|folder| folder_write_refusal(folder).is_some())
    {
        return TrashRefusalKind::FolderNotWritable;
    }
    reason
}

/// Moves `path` to the trash via the `trash` crate. Returns `Ok(None)`: the crate
/// doesn't surface the in-trash location, so no restore location is recorded.
#[cfg(target_os = "linux")]
pub fn move_to_trash_sync(path: &Path) -> Result<Option<PathBuf>, MutationError> {
    if fs::symlink_metadata(path).is_err() {
        return Err(MutationError::NotFound {
            path: path.display().to_string(),
        });
    }

    // Linux has no NSError to classify, and the `trash` crate's own error kinds don't
    // map onto the macOS permission question the UI asks. `Other` keeps the offer off.
    trash::delete(path).map_err(|e| MutationError::TrashRefused {
        reason: TrashRefusalKind::Other,
        detail: e.to_string(),
    })?;
    // The `trash` crate doesn't surface the in-trash location, so no restore
    // location is recorded (trash rollback is then unavailable on Linux).
    Ok(None)
}

/// Refuses with `TrashNotSupported`: this platform has no trash integration.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn move_to_trash_sync(_path: &Path) -> Result<Option<PathBuf>, MutationError> {
    Err(MutationError::TrashNotSupported)
}

impl super::super::Operations {
    /// Trash ONE item and journal it as a one-item trash operation — the
    /// single-trash path (`commands/rename.rs::move_to_trash`), which bypasses the
    /// batch trash and its manager op. Mirrors the batch path's capture: enumerate
    /// the subtree's `search_only` leaves from the drive index BEFORE the OS move
    /// (the reconciler prunes on the FSEvent), record the top-level
    /// `rollback_unit` row with the in-trash dest, persist the leaves only on
    /// success, and finalize. Returns the in-trash location like
    /// [`move_to_trash_sync`]. Runs on a `spawn_blocking` thread (sync index reads
    /// + journal sends are fine there).
    pub fn trash_single_journaled(
        &self,
        source: &Path,
        initiator: cmdr_operation_log::types::Initiator,
    ) -> Result<Option<PathBuf>, MutationError> {
        use cmdr_operation_log::types::{EntryType, ExecutionStatus, OpKind};
        let scope = self.op_scope();

        let source_meta = fs::symlink_metadata(source).map_err(|_| MutationError::NotFound {
            path: source.display().to_string(),
        })?;

        // Enumerate BEFORE the move (buffered, persisted only on success).
        let buffered = if source_meta.is_dir() {
            Some(super::super::journal_search::enumerate_subtree_for_search(
                scope.host().index(),
                "root",
                source,
                super::super::journal_search::SEARCH_LEAF_CAP,
            ))
        } else {
            None
        };

        let op_id = OperationId::from(cmdr_operation_log::new_operation_id());
        // A single trash has no destination volume (the in-trash location rides on the
        // item row, not the header); one top-level item.
        super::super::journal::open_local_op(&scope, op_id.as_str(), OpKind::Trash, initiator, 1, None);

        match move_to_trash_sync(source) {
            Ok(in_trash) => {
                let entry_type = if source_meta.is_dir() {
                    EntryType::Dir
                } else {
                    EntryType::File
                };
                super::super::journal::record_trashed(
                    &scope,
                    entry_type,
                    source,
                    in_trash.as_deref(),
                    Some(source_meta.len() as i64),
                    super::super::journal::mtime_secs(&source_meta),
                );
                if let Some(buffered) = &buffered {
                    super::super::journal_search::persist_and_note(
                        &scope,
                        cmdr_fs::volume::DEFAULT_VOLUME_ID,
                        source,
                        cmdr_fs::volume::DEFAULT_VOLUME_ID,
                        in_trash.as_deref(),
                        buffered,
                    );
                }
                super::super::journal::finalize_op(&scope, OpKind::Trash, ExecutionStatus::Done);
                Ok(in_trash)
            }
            Err(e) => {
                super::super::journal::finalize_op(&scope, OpKind::Trash, ExecutionStatus::Failed);
                Err(e)
            }
        }
    }
}

// ============================================================================
// Where trashed items land
// ============================================================================

/// The trash directory that holds items trashed from `path`'s volume.
///
/// macOS keeps ONE trash per volume, so there is no single answer: the boot
/// volume's is `~/.Trash`, and every other mounted volume gets its own
/// `<mount point>/.Trashes/<uid>`. Rather than reconstruct that from `statfs` +
/// `getuid`, this asks Cocoa the same question the trash move itself asks, via
/// `URLForDirectory:inDomain:appropriateForURL:create:`, so an unusual mount
/// (a disk image, a synthetic firmlink, a volume with trash turned off) answers
/// for itself instead of against our guess. `create: false` keeps this a pure
/// lookup: a volume nobody has trashed anything to yet has no trash directory,
/// and that answers `None` rather than quietly creating one.
///
/// `None` also covers a volume with no trash at all (FAT32, SMB), which is why
/// callers treat it as "nowhere to go", never as a failure.
///
/// **Gotcha**: `appropriateForURL:` answers only for a path that EXISTS — it
/// resolves the URL's volume, and a missing path has none. The question is asked
/// about paths that are routinely gone by then (the item the user just trashed is
/// no longer where it was), so this walks up to the nearest existing ancestor,
/// which sits on the same volume and gives the same trash. Without the walk, "Go
/// to trash" would answer `None` in exactly the case it exists for.
///
/// (verified on macOS 15: the boot volume answers `~/.Trash` and a mounted
/// USB volume answers `/Volumes/<name>/.Trashes/501`; a nonexistent path answers
/// an error until it's resolved against a live ancestor, 2026-08-27.)
#[cfg(target_os = "macos")]
pub fn trash_dir_for_path(path: &Path) -> Option<PathBuf> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask, NSString, NSURL};

    let anchor = path.ancestors().find(|p| fs::symlink_metadata(p).is_ok())?;

    autoreleasepool(|_| {
        let path_str = anchor.to_string_lossy();
        let ns_path = NSString::from_str(&path_str);
        let url = NSURL::fileURLWithPath(&ns_path);
        let file_manager = NSFileManager::defaultManager();

        let trash_url = file_manager
            .URLForDirectory_inDomain_appropriateForURL_create_error(
                NSSearchPathDirectory::TrashDirectory,
                NSSearchPathDomainMask::UserDomainMask,
                Some(&url),
                false,
            )
            .ok()?;
        trash_url.path().map(|p| PathBuf::from(p.to_string()))
    })
}

/// The trash directory `path` would go to: always `None` here. The `trash` crate
/// doesn't surface a trash location, and the XDG layout has no single answer for
/// a non-home volume either, so callers degrade to "nowhere to go".
#[cfg(not(target_os = "macos"))]
pub fn trash_dir_for_path(_path: &Path) -> Option<PathBuf> {
    None
}

// ============================================================================
// Batch trash with progress
// ============================================================================

/// Per-item error from a trash operation.
#[derive(Debug, Clone)]
pub(crate) struct TrashItemError {
    pub path: PathBuf,
    pub message: String,
    /// Why the OS refused this one, kept as a value so the batch's terminal error can
    /// carry it. Without it the whole batch collapses to one sentence and the dialog
    /// has nothing to offer but "try again".
    pub reason: TrashRefusalKind,
    /// This item's contents were only on the provider's servers, which is what
    /// the refusal was really about. Stat'ed at the refusal, while the item is
    /// still there to ask. See `cloud_trash.rs`.
    pub online_only: bool,
}

/// Moves files to trash with progress reporting, cancellation, and partial failure.
///
/// Iterates top-level items, calling `move_to_trash_sync()` for each.
/// Unlike permanent delete, trash doesn't need a recursive scan phase because
/// `trashItemAtURL` is atomic per top-level item (the OS moves the entire tree).
///
/// # Arguments
/// * `events` - Event sink for `write-progress`, `write-complete`, `write-cancelled`,
///   `write-error`, and `write-source-item-done` emits. Production wraps a Tauri AppHandle
///   in `TauriEventSink`; tests use `CollectorEventSink`.
/// * `operation_id` - Unique operation ID for event correlation
/// * `state` - Shared state with cancellation flag and progress interval
/// * `sources` - Top-level items to trash
/// * `item_sizes` - Optional per-item sizes for byte-level progress (from scan preview or drive
///   index). When `None`, bytes progress is not reported.
pub(crate) fn trash_files_with_progress(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    item_sizes: Option<&[u64]>,
) -> Result<(), WriteOperationError> {
    let items_total = sources.len();
    let bytes_total: u64 = item_sizes.map(|s| s.iter().sum()).unwrap_or(0);

    let mut items_done = 0usize;
    let mut bytes_done = 0u64;
    let mut errors: Vec<TrashItemError> = Vec::new();
    let mut last_progress_time = Instant::now();

    for (i, source) in sources.iter().enumerate() {
        // The cooperative boundary, between items. Trash hands a whole tree to
        // the OS in one call, so this is the only place it can stop, and it's
        // the same boundary the two delete walkers park at.
        if state.stop_or_park_sync() {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Trash,
                files_processed: items_done,
                rollback: CancelRollback::none(), // Trash is recoverable, no rollback needed
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Check existence using symlink_metadata (handles dangling symlinks).
        // Keep the metadata: it's the free snapshot (kind + mtime) the journal
        // records for the top-level item, stat'd BEFORE the OS moves it to trash.
        let source_meta = match fs::symlink_metadata(source) {
            Ok(m) => m,
            Err(error) => {
                errors.push(TrashItemError {
                    path: source.clone(),
                    message: format!("'{}' no longer exists", source.display()),
                    // A vanished source is not a permission problem; offering a grant
                    // as the fix would send the user somewhere useless.
                    reason: TrashRefusalKind::Other,
                    // We couldn't stat it at all, so there are no flags to read.
                    online_only: false,
                });
                emit_item_failed(
                    events,
                    operation_id,
                    source,
                    // Only a NotFound proves the item is gone. "We couldn't look"
                    // (permissions, a dead mount) is not evidence, and this flag
                    // decides whether every search snapshot drops the row.
                    error.kind() == std::io::ErrorKind::NotFound,
                );
                continue;
            }
        };

        // Defensive: register with the downloads watcher's ignore set so a
        // future "deleted from Downloads" event source wouldn't surprise us.
        // No-ops outside ~/Downloads.
        state.scope().host().announce(source);

        // Enumerate the subtree's `search_only` leaves from the drive index BEFORE
        // the OS move — the index reconciler prunes the subtree the instant it sees
        // the trash FSEvent, so a later read would miss them (search-leaf enumeration). Only a directory
        // has descendants; a file is fully covered by its top-level row. Buffered
        // now, persisted only after this item succeeds (below).
        let buffered_leaves = if source_meta.is_dir() {
            Some(super::super::journal_search::enumerate_subtree_for_search(
                state.scope().host().index(),
                "root",
                source,
                super::super::journal_search::SEARCH_LEAF_CAP,
            ))
        } else {
            None
        };

        // Attempt to trash the item
        match move_to_trash_sync(source) {
            Ok(in_trash) => {
                items_done += 1;
                let item_size = item_sizes.and_then(|s| s.get(i).copied());
                if let Some(size) = item_size {
                    bytes_done += size;
                }

                // Journal the trashed top-level item as the rollback unit (one
                // restore-from-trash reverses the whole subtree). The in-trash
                // location (`resultingItemURL`) is the row's dest so the rollback restore
                // knows where to move it back FROM. The subtree's `search_only`
                // leaves are enumerated from the drive index (search-leaf enumeration).
                let entry_type = if source_meta.is_dir() {
                    cmdr_operation_log::types::EntryType::Dir
                } else {
                    cmdr_operation_log::types::EntryType::File
                };
                super::super::journal::record_trashed(
                    state.scope(),
                    entry_type,
                    source,
                    in_trash.as_deref(),
                    item_size.map(|s| s as i64).or(Some(source_meta.len() as i64)),
                    super::super::journal::mtime_secs(&source_meta),
                );

                // Persist the buffered `search_only` leaves NOW that this item
                // succeeded (never before — a failed item must record no leaves,
                // else search would return a trash that never happened). Their dest
                // is rebased onto the in-trash location; coverage downgrades are
                // noted worst-wins.
                if let Some(buffered) = &buffered_leaves {
                    super::super::journal_search::persist_and_note(
                        state.scope(),
                        cmdr_fs::volume::DEFAULT_VOLUME_ID,
                        source,
                        cmdr_fs::volume::DEFAULT_VOLUME_ID,
                        in_trash.as_deref(),
                        buffered,
                    );
                }

                events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source.display().to_string(),
                    // `trashItemAtURL` moved the whole tree, so the original path
                    // is gone.
                    source_removed: true,
                    outcome: SourceItemOutcome::Done,
                });
            }
            Err(e) => {
                errors.push(TrashItemError {
                    path: source.clone(),
                    reason: match &e {
                        MutationError::TrashRefused { reason, .. } => *reason,
                        _ => TrashRefusalKind::Other,
                    },
                    // The batch path reports through `WriteOperationError`, whose
                    // own typed variant carries the words; this string is the
                    // technical detail beside it, so `Display` is right here.
                    message: e.to_string(),
                    // `trashItemAtURL` is atomic, so a refusal left the item
                    // exactly where it was and its flags are still readable.
                    online_only: super::cloud_trash::is_online_only(source),
                });
                // `trashItemAtURL` is atomic per item, so a failure left this one
                // exactly where it was.
                emit_item_failed(events, operation_id, source, false);
                continue;
            }
        }

        // Emit throttled progress
        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = source.file_name().map(|n| n.to_string_lossy().to_string());
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Trash,
                    WriteOperationPhase::Trashing,
                    current_file.clone(),
                    items_done,
                    items_total,
                    bytes_done,
                    bytes_total,
                ),
            );
            state.scope().status().update(
                operation_id,
                WriteOperationPhase::Trashing,
                current_file,
                items_done,
                items_total,
                bytes_done,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    // No fsync after trashing: like delete, a non-durable trash fails
    // annoyance-class (a trashed file could reappear after a crash, the user
    // re-trashes; never data loss), so targeted fsync isn't worth its cost, and
    // dropping the old whole-machine global sync (`sync(2)`) removes the stall
    // it caused on unrelated apps. See `CLAUDE.md` § "Durability".

    // If all items failed, emit error
    if items_done == 0 && !errors.is_empty() {
        let error_summary = errors
            .iter()
            .map(|e| format!("{}: {}", e.path.display(), e.message))
            .collect::<Vec<_>>()
            .join("; ");
        // One reason for the batch: the strongest one any item reported. A batch where
        // even one item was refused for permission is a batch the FDA hint can help with,
        // and `Other` would silently withhold it.
        let reason = strongest_refusal(&errors);
        let failure = WriteOperationError::TrashRefused {
            item_count: errors.len(),
            reason,
            message: error_summary,
            // One evicted item is enough to make the FDA advice wrong for this
            // batch, the same all-or-nothing stance the routing takes.
            online_only: errors.iter().any(|e| e.online_only),
        };
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Trash,
            failure.clone(),
        ));
        return Err(failure);
    }

    // A batch that took some items and was refused others is neither a success
    // nor a failure. Reporting it as a plain success left the refused items
    // sitting in the pane with nothing said about them, so the completion carries
    // them: the count the FE names, and the same `strongest_refusal` the
    // all-refused error reports, so the wording can offer the same way through.
    // ❌ Not `files_skipped`, which means a deliberate skip.
    let refused = (!errors.is_empty()).then(|| TrashRefusedItems {
        item_count: errors.len() as u32,
        reason: strongest_refusal(&errors),
    });

    // Emit completion (may include partial errors)
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Trash,
        files_processed: items_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
        appeared_during_move: None,
        top_level_skipped: None,
        refused,
        metadata_left_behind: None,
        durability_unconfirmed: None,
        locked_kept: None,
    });

    // Log partial failures
    if !errors.is_empty() {
        log::warn!(
            "Trash operation {} completed with {} errors out of {} items",
            operation_id,
            errors.len(),
            items_total
        );
        for error in &errors {
            log::warn!("  Failed: {}: {}", LogPrivate::path(&error.path), error.message);
        }
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

/// The reason a whole failed batch reports, from its items' reasons.
///
/// Takes the one that opens the most doors for the user rather than the most common:
/// a batch of nine items where one was refused for permission is still a batch where
/// granting a permission might help, and the majority answer would hide that. The
/// two reasons read from the item itself (Locked, a folder that takes no writes) come
/// next: each names a concrete fix the user can make.
fn strongest_refusal(errors: &[TrashItemError]) -> TrashRefusalKind {
    const RANKED: [TrashRefusalKind; 4] = [
        TrashRefusalKind::NotPermitted,
        TrashRefusalKind::Locked,
        TrashRefusalKind::FolderNotWritable,
        TrashRefusalKind::NoTrashForVolume,
    ];
    RANKED
        .into_iter()
        .find(|kind| errors.iter().any(|e| e.reason == *kind))
        .unwrap_or(TrashRefusalKind::Other)
}

/// One top-level item this trash could not take.
///
/// Trash is per-item: one failure leaves the rest of the batch running, so the
/// operation's own terminal event says nothing about THIS item. Without a verdict
/// here, a caller tracking per-source outcomes waits forever for one that never
/// comes.
fn emit_item_failed(events: &dyn OperationEventSink, operation_id: &str, source: &Path, source_removed: bool) {
    events.emit_source_item_done(WriteSourceItemDoneEvent {
        operation_id: operation_id.to_string(),
        source_path: source.display().to_string(),
        source_removed,
        outcome: SourceItemOutcome::Failed,
    });
}

#[cfg(test)]
#[path = "trash_tests.rs"]
mod tests;
