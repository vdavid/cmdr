//! The dry run: what a copy or move WOULD do, without doing any of it.
//!
//! Same walk as a real scan (`scan_walker.rs`), plus per-file conflict
//! detection, so the caller can show "12 files, 3 conflicts" before anything is
//! written. ❗ It writes nothing and creates nothing, and `handle_dry_run` is
//! the whole entry point: it answers `Ok(true)` when the caller must stop
//! because the dry run has spoken for the operation.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::cancellable::run_cancellable_scoped;
use super::conflict::{calculate_dest_path, create_conflict_info, sample_conflicts};
use super::error_classification::IoResultExt;
use super::event_sinks::OperationEventSink;
use super::state::WriteOperationState;
use super::types::{ConflictInfo, ScanProgressEvent, WriteOperationError, WriteOperationType};
use super::validation::is_symlink_loop;
// ============================================================================
// Dry-run scanning (with conflict detection)
// ============================================================================

/// Result of a dry-run scan including conflicts.
pub(super) struct DryRunScanResult {
    pub file_count: usize,
    pub total_bytes: u64,
    pub conflicts: Vec<ConflictInfo>,
}

/// Performs a dry-run scan: scans sources, detects conflicts at destination.
/// Emits ScanProgressEvent during scanning with conflict counts.
///
/// Uses polling-based cancellation to remain responsive even when filesystem
/// operations block (for example, on stuck network drives).
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn dry_run_scan(
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
) -> Result<DryRunScanResult, WriteOperationError> {
    run_cancellable_scoped(
        || {
            dry_run_scan_internal(
                sources,
                destination,
                state,
                events,
                operation_id,
                operation_type,
                progress_interval,
            )
        },
        state,
        "dry_run_scan",
        operation_id,
    )
}

/// Internal dry-run scan implementation (runs in background thread).
fn dry_run_scan_internal(
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
) -> Result<DryRunScanResult, WriteOperationError> {
    let mut files_found = 0usize;
    let mut bytes_found = 0u64;
    let mut conflicts = Vec::new();
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();

    for source in sources {
        // A source that would land on ITSELF is a request to duplicate it, and
        // both engines answer that by writing the whole subtree under a name
        // nobody holds (`transfer/DETAILS.md` § "Self-collision (duplicating in
        // place)"). Nothing in it can be in the way, so reporting its own files
        // as conflicts would describe work the copy never does. Asked per
        // top-level source, the altitude the engines ask it at: every LEAF of a
        // same-folder folder copy is its own self-collision, and a per-leaf
        // question would be answering about `docs/a.txt` instead of `docs/`.
        let lands_on_itself = calculate_dest_path(source, source, destination)
            .is_ok_and(|dest_path| super::validation::is_same_file(source, &dest_path));
        dry_run_scan_recursive(
            source,
            source,
            destination,
            &mut files_found,
            &mut bytes_found,
            &mut conflicts,
            state,
            events,
            operation_id,
            operation_type,
            &progress_interval,
            &mut last_progress_time,
            &mut visited,
            lands_on_itself,
        )?;
    }

    // Emit final scan progress
    events.emit_scan_progress(ScanProgressEvent {
        operation_id: operation_id.to_string(),
        operation_type,
        files_found,
        bytes_found,
        conflicts_found: conflicts.len(),
        current_path: None,
    });

    Ok(DryRunScanResult {
        file_count: files_found,
        total_bytes: bytes_found,
        conflicts,
    })
}

/// Recursively scans a path for dry-run, detecting conflicts.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
fn dry_run_scan_recursive(
    path: &Path,
    source_root: &Path,
    dest_root: &Path,
    files_found: &mut usize,
    bytes_found: &mut u64,
    conflicts: &mut Vec<ConflictInfo>,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: &Duration,
    last_progress_time: &mut Instant,
    visited: &mut HashSet<PathBuf>,
    // `true` when this subtree's top-level source lands on itself, so the whole
    // of it is a duplicate under a fresh name and none of it can clash. Decided
    // once by the caller and carried down, never re-asked per leaf.
    lands_on_itself: bool,
) -> Result<(), WriteOperationError> {
    // Check cancellation
    if super::state::is_cancelled(&state.intent) {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // Use symlink_metadata to not follow symlinks
    let metadata = fs::symlink_metadata(path).with_path(path)?;

    // Calculate destination path
    let dest_path = calculate_dest_path(path, source_root, dest_root)?;

    if metadata.is_symlink() || metadata.is_file() {
        *bytes_found += metadata.len();
        *files_found += 1;

        // Check for conflict
        if !lands_on_itself
            && (dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok())
            && let Some(conflict) = create_conflict_info(path, &dest_path, &metadata)?
        {
            // Emit conflict event for streaming
            events.emit_scan_conflict(conflict.clone());
            conflicts.push(conflict);
        }
    } else if metadata.is_dir() {
        // Check for symlink loop before recursing
        if is_symlink_loop(path, visited) {
            return Err(WriteOperationError::SymlinkLoop {
                path: path.display().to_string(),
            });
        }

        // Track this directory
        if let Ok(canonical) = path.canonicalize() {
            visited.insert(canonical);
        }

        // Check if destination exists and is not a directory (type conflict)
        if !lands_on_itself
            && dest_path.exists()
            && !dest_path.is_dir()
            && let Some(conflict) = create_conflict_info(path, &dest_path, &metadata)?
        {
            events.emit_scan_conflict(conflict.clone());
            conflicts.push(conflict);
        }

        // Scan contents
        let entries = fs::read_dir(path).with_path(path)?;

        for entry in entries.flatten() {
            dry_run_scan_recursive(
                &entry.path(),
                source_root,
                dest_root,
                files_found,
                bytes_found,
                conflicts,
                state,
                events,
                operation_id,
                operation_type,
                progress_interval,
                last_progress_time,
                visited,
                lands_on_itself,
            )?;
        }
    } else {
        // Skip special files (sockets, FIFOs, char/block devices)
        log::debug!("dry_run_scan: skipping special file: {}", path.display());
    }

    // Emit progress periodically
    if last_progress_time.elapsed() >= *progress_interval {
        events.emit_scan_progress(ScanProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type,
            files_found: *files_found,
            bytes_found: *bytes_found,
            conflicts_found: conflicts.len(),
            current_path: path.file_name().map(|n| n.to_string_lossy().to_string()),
        });
        *last_progress_time = Instant::now();
    }

    Ok(())
}

/// Handles dry-run mode for copy/move operations.
/// Returns Ok(true) if dry-run was performed, Ok(false) if not dry-run mode.
#[allow(
    clippy::too_many_arguments,
    reason = "Dry-run requires all operation context parameters"
)]
pub(super) fn handle_dry_run(
    config_dry_run: bool,
    sources: &[PathBuf],
    destination: &Path,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    progress_interval: Duration,
    max_conflicts_to_show: usize,
) -> Result<bool, WriteOperationError> {
    use super::types::DryRunResult;

    if !config_dry_run {
        return Ok(false);
    }

    let scan_result = dry_run_scan(
        sources,
        destination,
        state,
        events,
        operation_id,
        operation_type,
        progress_interval,
    )?;

    let conflicts_count = scan_result.conflicts.len();
    let (sampled_conflicts, conflicts_sampled) = sample_conflicts(scan_result.conflicts, max_conflicts_to_show);

    let result = DryRunResult {
        operation_id: operation_id.to_string(),
        operation_type,
        files_total: scan_result.file_count,
        bytes_total: scan_result.total_bytes,
        conflicts_total: conflicts_count,
        conflicts: sampled_conflicts,
        conflicts_sampled,
    };

    events.emit_dry_run_complete(result);
    Ok(true)
}
