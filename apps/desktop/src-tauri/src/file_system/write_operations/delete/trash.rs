//! Trash implementation for write operations.
//!
//! Provides `move_to_trash_sync()` (reusable core) and `trash_files_with_progress()`
//! (batch operation with progress, cancellation, and partial failure support).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::super::event_sinks::OperationEventSink;
use super::super::mutation_error::MutationError;
use super::super::state::{WriteOperationState, update_operation_status};
use super::super::types::{
    SourceItemOutcome, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};

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
            .map_err(|e| MutationError::TrashRefused { detail: e.to_string() })?;
        let in_trash = resulting.and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()));
        Ok(in_trash)
    })
}

#[cfg(target_os = "linux")]
pub fn move_to_trash_sync(path: &Path) -> Result<Option<PathBuf>, MutationError> {
    if fs::symlink_metadata(path).is_err() {
        return Err(MutationError::NotFound {
            path: path.display().to_string(),
        });
    }

    trash::delete(path).map_err(|e| MutationError::TrashRefused { detail: e.to_string() })?;
    // The `trash` crate doesn't surface the in-trash location, so no restore
    // location is recorded (trash rollback is then unavailable on Linux).
    Ok(None)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn move_to_trash_sync(_path: &Path) -> Result<Option<PathBuf>, MutationError> {
    Err(MutationError::TrashNotSupported)
}

/// Trash ONE item and journal it as a one-item trash operation — the single-trash
/// path (`commands/rename.rs::move_to_trash`), which bypasses the batch trash and
/// its manager op. Mirrors the batch path's capture: enumerate the subtree's
/// `search_only` leaves from the drive index BEFORE the OS move (the reconciler
/// prunes on the FSEvent), record the top-level `rollback_unit` row with the
/// in-trash dest, persist the leaves only on success, and finalize. Returns
/// the in-trash location like [`move_to_trash_sync`]. Runs on a `spawn_blocking`
/// thread (sync index reads + journal sends are fine there).
pub fn trash_single_journaled(
    source: &Path,
    initiator: crate::operation_log::types::Initiator,
) -> Result<Option<PathBuf>, MutationError> {
    use crate::operation_log::types::{EntryType, ExecutionStatus, ItemOutcome, OpKind};

    let source_meta = fs::symlink_metadata(source).map_err(|_| MutationError::NotFound {
        path: source.display().to_string(),
    })?;

    // Enumerate BEFORE the move (buffered, persisted only on success).
    let buffered = if source_meta.is_dir() {
        Some(super::super::journal_search::enumerate_subtree_for_search(
            "root",
            source,
            super::super::journal_search::SEARCH_LEAF_CAP,
        ))
    } else {
        None
    };

    let op_id = crate::operation_log::new_operation_id();
    // A single trash has no destination volume (the in-trash location rides on the
    // item row, not the header); one top-level item.
    super::super::journal::open_local_op(&op_id, OpKind::Trash, initiator, 1, None);

    match move_to_trash_sync(source) {
        Ok(in_trash) => {
            let entry_type = if source_meta.is_dir() {
                EntryType::Dir
            } else {
                EntryType::File
            };
            super::super::journal::record_local_leaf(
                &op_id,
                entry_type,
                source,
                in_trash.as_deref(),
                Some(source_meta.len() as i64),
                super::super::journal::mtime_secs(&source_meta),
                false,
                ItemOutcome::Done,
            );
            if let Some(buffered) = &buffered {
                super::super::journal_search::persist_and_note(
                    &op_id,
                    crate::file_system::volume::DEFAULT_VOLUME_ID,
                    source,
                    crate::file_system::volume::DEFAULT_VOLUME_ID,
                    in_trash.as_deref(),
                    buffered,
                );
            }
            super::super::journal::finalize_op(&op_id, OpKind::Trash, ExecutionStatus::Done);
            Ok(in_trash)
        }
        Err(e) => {
            super::super::journal::finalize_op(&op_id, OpKind::Trash, ExecutionStatus::Failed);
            Err(e)
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

#[cfg(not(target_os = "macos"))]
pub fn trash_dir_for_path(_path: &Path) -> Option<PathBuf> {
    // The `trash` crate doesn't surface a trash location, and the XDG layout has no
    // single answer for a non-home volume either. Callers degrade to "nowhere to go".
    None
}

// ============================================================================
// Batch trash with progress
// ============================================================================

/// Per-item error from a trash operation.
#[derive(Debug, Clone)]
pub struct TrashItemError {
    pub path: PathBuf,
    pub message: String,
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
pub(in crate::file_system::write_operations) fn trash_files_with_progress(
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
        // Check cancellation between items
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Trash,
                files_processed: items_done,
                rolled_back: false, // Trash is recoverable, no rollback needed
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
        crate::downloads::note_pending_write_for_cmdr(source);

        // Enumerate the subtree's `search_only` leaves from the drive index BEFORE
        // the OS move — the index reconciler prunes the subtree the instant it sees
        // the trash FSEvent, so a later read would miss them (search-leaf enumeration). Only a directory
        // has descendants; a file is fully covered by its top-level row. Buffered
        // now, persisted only after this item succeeds (below).
        let buffered_leaves = if source_meta.is_dir() {
            Some(super::super::journal_search::enumerate_subtree_for_search(
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
                    crate::operation_log::types::EntryType::Dir
                } else {
                    crate::operation_log::types::EntryType::File
                };
                super::super::journal::record_local_leaf(
                    operation_id,
                    entry_type,
                    source,
                    in_trash.as_deref(),
                    item_size.map(|s| s as i64).or(Some(source_meta.len() as i64)),
                    super::super::journal::mtime_secs(&source_meta),
                    false,
                    crate::operation_log::types::ItemOutcome::Done,
                );

                // Persist the buffered `search_only` leaves NOW that this item
                // succeeded (never before — a failed item must record no leaves,
                // else search would return a trash that never happened). Their dest
                // is rebased onto the in-trash location; coverage downgrades are
                // noted worst-wins.
                if let Some(buffered) = &buffered_leaves {
                    super::super::journal_search::persist_and_note(
                        operation_id,
                        crate::file_system::volume::DEFAULT_VOLUME_ID,
                        source,
                        crate::file_system::volume::DEFAULT_VOLUME_ID,
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
                    // The batch path reports through `WriteOperationError`, whose
                    // own typed variant carries the words; this string is the
                    // technical detail beside it, so `Display` is right here.
                    message: e.to_string(),
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
            update_operation_status(
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
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Trash,
            WriteOperationError::IoError {
                path: String::new(),
                message: error_summary,
            },
        ));
        return Err(WriteOperationError::IoError {
            path: String::new(),
            message: format!(
                "Couldn't move {} to trash",
                if errors.len() == 1 {
                    format!("'{}'", errors[0].path.display())
                } else {
                    format!("{} items", errors.len())
                }
            ),
        });
    }

    // Emit completion (may include partial errors)
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Trash,
        files_processed: items_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
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
            log::warn!("  Failed: {}: {}", error.path.display(), error.message);
        }
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

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
mod tests {
    use super::*;
    // Only the macOS-gated cases below build a real directory.
    #[cfg(target_os = "macos")]
    use crate::test_support::TestDir;
    use std::sync::Arc;
    use std::time::Duration;

    #[cfg(target_os = "macos")]
    fn create_test_dir(name: &str) -> TestDir {
        TestDir::new(&format!("trash_test_{}", name))
    }

    // ========================================================================
    // move_to_trash_sync tests
    // ========================================================================

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_to_trash_sync_file() {
        let tmp = create_test_dir("trash_sync_file");
        let file = tmp.join("test.txt");
        fs::write(&file, "content").unwrap();
        assert!(fs::symlink_metadata(&file).is_ok());

        let result = move_to_trash_sync(&file);
        assert!(result.is_ok());
        assert!(fs::symlink_metadata(&file).is_err());

        // The in-trash location (resultingItemURL) is captured and points into
        // the user's Trash — the rollback restore depends on this dest.
        let in_trash = result.unwrap().expect("macOS reports an in-trash location");
        assert!(
            in_trash.components().any(|c| c.as_os_str() == ".Trash"),
            "expected a ~/.Trash path, got {}",
            in_trash.display()
        );
        assert!(fs::symlink_metadata(&in_trash).is_ok(), "the item exists in Trash");
        let _ = fs::remove_file(&in_trash);
    }

    // ========================================================================
    // trash_dir_for_path tests
    // ========================================================================

    /// The resolver has to agree with where the trash move actually puts things,
    /// or "Go to trash" navigates somewhere the item isn't. Trashing a real file
    /// and comparing its recorded location against the resolver is the only check
    /// that pins the two together.
    #[cfg(target_os = "macos")]
    #[test]
    fn trash_dir_for_path_matches_where_the_item_actually_landed() {
        let tmp = create_test_dir("trash_dir_resolve");
        let file = tmp.join("test.txt");
        fs::write(&file, "content").unwrap();

        let resolved = trash_dir_for_path(&file).expect("the boot volume has a trash");
        let in_trash = move_to_trash_sync(&file)
            .expect("trash succeeds")
            .expect("macOS reports an in-trash location");

        assert_eq!(
            in_trash.parent(),
            Some(resolved.as_path()),
            "resolver said {}, the item landed in {}",
            resolved.display(),
            in_trash.display()
        );
        let _ = fs::remove_file(&in_trash);
    }

    /// The case the feature exists for: by the time anyone asks where a trashed item
    /// went, its original path is gone. Cocoa refuses to resolve a volume for a path
    /// that doesn't exist, so the ancestor walk is what keeps the answer coming.
    #[cfg(target_os = "macos")]
    #[test]
    fn trash_dir_for_path_answers_for_a_path_that_is_already_gone() {
        let tmp = create_test_dir("trash_dir_missing");
        let never_existed = tmp.join("no-such-file.txt");

        let resolved = trash_dir_for_path(&never_existed).expect("the volume still has a trash");
        assert!(
            resolved.components().any(|c| c.as_os_str() == ".Trash"),
            "expected a ~/.Trash path, got {}",
            resolved.display()
        );
    }

    /// A whole subtree can be gone, not only the leaf (trashing a folder takes its
    /// children with it), so the walk must climb as far as it needs to.
    #[cfg(target_os = "macos")]
    #[test]
    fn trash_dir_for_path_climbs_past_several_missing_levels() {
        let tmp = create_test_dir("trash_dir_deep_missing");
        let deep = tmp.join("gone").join("also-gone").join("file.txt");

        assert!(
            trash_dir_for_path(&deep).is_some(),
            "a path several levels below a live ancestor still resolves"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_to_trash_sync_directory() {
        let tmp = create_test_dir("trash_sync_dir");
        let dir = tmp.join("subdir");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("inner.txt"), "data").unwrap();

        let result = move_to_trash_sync(&dir);
        assert!(result.is_ok());
        assert!(fs::symlink_metadata(&dir).is_err());
    }

    #[test]
    fn test_move_to_trash_sync_nonexistent() {
        let result = move_to_trash_sync(Path::new("/nonexistent_12345/file.txt"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MutationError::NotFound { .. }));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_to_trash_sync_dangling_symlink() {
        let tmp = create_test_dir("trash_sync_dangling");
        let target = tmp.join("target.txt");
        let link = tmp.join("link.txt");
        fs::write(&target, "data").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // Remove the target, leaving a dangling symlink
        fs::remove_file(&target).unwrap();

        // The link itself still exists (symlink_metadata succeeds)
        assert!(fs::symlink_metadata(&link).is_ok());
        // But path.exists() would return false (follows symlink)
        assert!(!link.exists());

        // move_to_trash_sync should handle this correctly
        let result = move_to_trash_sync(&link);
        assert!(result.is_ok());
        assert!(fs::symlink_metadata(&link).is_err());
    }

    // ========================================================================
    // trash_files_with_progress tests (via CollectorEventSink)
    // ========================================================================

    use crate::file_system::write_operations::event_sinks::CollectorEventSink;

    /// Empty source list short-circuits: no destructive work, but a
    /// `write-complete` event still fires so the FE dialog closes cleanly.
    #[test]
    fn trash_empty_sources_emits_complete_via_sink() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

        let result = trash_files_with_progress(&*events, "op-trash-empty", &state, &[], None);
        assert!(result.is_ok(), "expected Ok, got {:?}", result);

        let complete = events.complete.lock().unwrap();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].files_processed, 0);
        assert_eq!(complete[0].bytes_processed, 0);
        assert!(events.cancelled.lock().unwrap().is_empty());
        assert!(events.errors.lock().unwrap().is_empty());
    }

    /// Pre-cancel: `Stopped` set before the loop's first iteration. Trash
    /// emits `write-cancelled` via the sink and returns
    /// `WriteOperationError::Cancelled` without invoking `move_to_trash_sync`.
    /// Source path is intentionally bogus — the cancel check fires first, so
    /// the path is never stat'd.
    #[test]
    fn trash_pre_cancel_emits_cancelled_via_sink() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        state.intent.store(2u8, std::sync::atomic::Ordering::Relaxed); // Stopped

        let sources = [PathBuf::from("/nonexistent_trash_test_12345/file.txt")];
        let result = trash_files_with_progress(&*events, "op-trash-cancel", &state, &sources, None);
        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));

        let cancelled = events.cancelled.lock().unwrap();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].files_processed, 0);
        assert!(!cancelled[0].rolled_back);
        assert!(events.complete.lock().unwrap().is_empty());
    }

    /// All sources missing: trash emits `write-error` via the sink and
    /// returns `IoError`. Tests the all-failed branch without invoking
    /// `move_to_trash_sync` (the missing-source branch short-circuits
    /// before the trash call).
    #[test]
    fn trash_all_sources_missing_emits_error_via_sink() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

        let sources = [
            PathBuf::from("/nonexistent_trash_test_aaa/x.txt"),
            PathBuf::from("/nonexistent_trash_test_bbb/y.txt"),
        ];
        let result = trash_files_with_progress(&*events, "op-trash-all-missing", &state, &sources, None);
        assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

        let errors = events.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(events.complete.lock().unwrap().is_empty());
    }

    /// One item failing leaves the rest of the batch running, so the operation's
    /// own terminal event says nothing about that item. Before this, it said
    /// nothing at all: a caller tracking per-source outcomes waited forever for a
    /// verdict on a file that was never going to move.
    #[test]
    fn a_source_trash_could_not_take_reports_itself_as_failed() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        let missing = PathBuf::from("/nonexistent_trash_test_ccc/gone.txt");

        let result = trash_files_with_progress(
            &*events,
            "op-trash-one-missing",
            &state,
            std::slice::from_ref(&missing),
            None,
        );
        assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

        let items = events.source_items_done.lock().unwrap();
        assert_eq!(items.len(), 1, "the item that couldn't be taken speaks for itself");
        assert_eq!(items[0].source_path, missing.display().to_string());
        assert_eq!(items[0].outcome, SourceItemOutcome::Failed);
        assert!(
            items[0].source_removed,
            "a NotFound source really is gone, so a stale search snapshot may drop it"
        );
    }

    #[test]
    fn test_trash_item_error_captures_path_and_message() {
        let error = TrashItemError {
            path: PathBuf::from("/some/file.txt"),
            message: "Permission denied".to_string(),
        };
        assert_eq!(error.path.display().to_string(), "/some/file.txt");
        assert_eq!(error.message, "Permission denied");
    }

    #[test]
    fn test_cancellation_flag_checked_by_state() {
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

        assert!(!crate::file_system::write_operations::is_cancelled(&state.intent));
        state.intent.store(2u8, std::sync::atomic::Ordering::Relaxed);
        assert!(crate::file_system::write_operations::is_cancelled(&state.intent));
    }
}
