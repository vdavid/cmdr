//! Scanning functionality for write operations.
//!
//! Contains file scanning, dry-run operations, and the shared directory walker.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::cancellable::run_cancellable_scoped;
use super::conflict::{calculate_dest_path, create_conflict_info, sample_conflicts};
use super::state::{FileInfo, SCAN_PREVIEW_RESULTS, ScanResult, WriteOperationState, update_operation_status};
use super::types::{
    ConflictInfo, IoResultExt, OperationEventSink, ScanProgressEvent, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent,
};
use super::validation::is_symlink_loop;
use crate::file_system::listing::caching::try_get_watched_listing;
use crate::file_system::listing::{FileEntry, SortColumn, SortOrder};
use crate::file_system::volume::{CopyScanResult, Volume, VolumeError};

/// Per-regular-file hook fired by the walk, receiving the file path and size (the `WalkContext::on_file` field).
type OnFileHook<'a> = &'a dyn Fn(&Path, u64);

/// Callbacks for customizing `walk_dir_recursive` behavior per caller.
///
/// `on_progress` is called as `(files_done, dirs_done, bytes_done, current_file, current_dir)`:
/// - `current_file` is just the filename component of the entry being processed
/// - `current_dir` is the absolute parent directory path, surfaced to the UI so the user sees "in
///   directory: …" alongside the filename
pub(super) struct WalkContext<'a, E> {
    pub(super) progress_interval: Duration,
    pub(super) is_cancelled: &'a dyn Fn() -> bool,
    pub(super) on_io_error: &'a dyn Fn(&Path, std::io::Error) -> E,
    pub(super) on_cancelled: &'a dyn Fn() -> E,
    pub(super) on_symlink_loop: &'a dyn Fn(&Path) -> E,
    pub(super) on_progress: &'a dyn Fn(usize, usize, u64, Option<String>, Option<String>),
    /// Optional per-regular-file hook `(path, size)`, fired once for each file
    /// (not dirs or symlinks) as the walk discovers it. The compress-size
    /// estimator uses it to feed a sampling worker off the walk thread; all
    /// other callers pass `None`. Must stay cheap (a channel push) so it never
    /// lands on the walk's critical path.
    pub(super) on_file: Option<OnFileHook<'a>>,
}

/// Recursively walks a directory tree, collecting files and directories.
///
/// Shared walker used by both scan preview and write operation scanning.
/// Behavior is customized via `WalkContext` callbacks for error handling and progress reporting.
///
/// **Oracle reuse**: when `volume_id` is provided and the listing cache holds a
/// watcher-backed listing for the directory currently being walked, the walker
/// hydrates that level's entries from the cache instead of touching the disk.
/// See `file_system::listing::caching::try_get_watched_listing` for the full
/// freshness contract. Pass `None` for `volume_id` to opt out (no listing
/// lookup is performed, behavior is identical to the pre-oracle walker).
///
/// **Two byte totals**: `total_bytes` is the **write footprint** — every file
/// at full size, including each hardlink, because hardlinks don't survive a
/// cross-volume copy. It's what a copy actually writes and what the
/// disk-space check must reserve. `dedup_bytes` is the **`du`-equivalent
/// source footprint** — each inode counted once. It's what a delete frees and
/// what the scan-phase progress bar compares against the indexer's inode-
/// dedup'd `dir_stats` estimate (so "X% of estimated" converges to ~100% on
/// hardlink-heavy trees like cargo's `target/`). Dedup is Unix-only (non-Unix
/// has no `nlink()`), where `dedup_bytes == total_bytes`. Copy consumes
/// `total_bytes`; delete consumes `dedup_bytes`; the Copy dialog shows both.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn walk_dir_recursive<E>(
    path: &Path,
    source_root: &Path,
    files: &mut Vec<FileInfo>,
    dirs: &mut Vec<PathBuf>,
    total_bytes: &mut u64,
    dedup_bytes: &mut u64,
    last_progress_time: &mut Instant,
    visited: &mut HashSet<PathBuf>,
    seen_inodes: &mut HashSet<u64>,
    volume_id: Option<&str>,
    ctx: &WalkContext<'_, E>,
) -> Result<(), E> {
    if (ctx.is_cancelled)() {
        return Err((ctx.on_cancelled)());
    }

    let metadata = fs::symlink_metadata(path).map_err(|e| (ctx.on_io_error)(path, e))?;

    if metadata.is_symlink() {
        // Symlinks contribute their own (tiny) target-string length, never
        // the target's bytes. No hardlink dedup: symlinks have distinct inodes.
        *total_bytes += metadata.len();
        *dedup_bytes += metadata.len();
        files.push(FileInfo::new(path.to_path_buf(), source_root.to_path_buf(), &metadata));
    } else if metadata.is_file() {
        // `total_bytes` is the write footprint (every file at full size, the
        // bytes a copy actually writes and the disk-space check must reserve).
        // `dedup_bytes` is the `du`-equivalent source footprint (each inode
        // once) — what delete frees and what the scan-phase progress bar
        // compares against the inode-dedup'd index estimate. `progress_bytes`
        // mirrors `dedup_bytes` per file so the delete active phase can sum a
        // running dedup'd total. See `CopyScanResult` for the consumer split.
        let counts = file_bytes_count_toward_total(&metadata, seen_inodes);
        let size = metadata.len();
        *total_bytes += size;
        if counts {
            *dedup_bytes += size;
        }
        let info = FileInfo::new(path.to_path_buf(), source_root.to_path_buf(), &metadata)
            .with_progress_bytes(if counts { size } else { 0 });
        files.push(info);
        if let Some(on_file) = ctx.on_file {
            on_file(path, size);
        }
    } else if metadata.is_dir() {
        if is_symlink_loop(path, visited) {
            return Err((ctx.on_symlink_loop)(path));
        }

        if let Ok(canonical) = path.canonicalize() {
            visited.insert(canonical);
        }

        dirs.push(path.to_path_buf());

        // Oracle short-circuit: if a watcher-backed listing exists for this dir,
        // walk it from cache instead of hitting the disk. The recurse-into-files
        // step below still goes through `walk_dir_recursive`, which re-applies
        // the oracle at each level.
        if let Some(vid) = volume_id
            && let Some(cached_entries) = try_get_watched_listing(vid, path)
        {
            walk_cached_entries(
                path,
                source_root,
                cached_entries,
                files,
                dirs,
                total_bytes,
                dedup_bytes,
                last_progress_time,
                visited,
                seen_inodes,
                volume_id,
                ctx,
            )?;
        } else {
            let entries = fs::read_dir(path).map_err(|e| (ctx.on_io_error)(path, e))?;
            for entry in entries.flatten() {
                walk_dir_recursive(
                    &entry.path(),
                    source_root,
                    files,
                    dirs,
                    total_bytes,
                    dedup_bytes,
                    last_progress_time,
                    visited,
                    seen_inodes,
                    volume_id,
                    ctx,
                )?;
            }
        }
    } else {
        log::debug!("scan: skipping special file: {}", path.display());
    }

    if last_progress_time.elapsed() >= ctx.progress_interval {
        let current_file = path.file_name().map(|n| n.to_string_lossy().to_string());
        let current_dir = path.parent().map(|p| p.display().to_string());
        // Report the dedup'd running total: the scan-phase bar compares this
        // against the index's inode-dedup'd estimate, so reporting the raw
        // write footprint would overshoot 100% on hardlink-heavy trees.
        (ctx.on_progress)(files.len(), dirs.len(), *dedup_bytes, current_file, current_dir);
        *last_progress_time = Instant::now();
    }

    Ok(())
}

/// Walks a directory level using cached `FileEntry` entries instead of `fs::read_dir`.
///
/// Used when the oracle reports a watcher-backed listing exists for the current
/// directory. For each cached entry: files are recorded with size from the
/// cache; directories recurse via `walk_dir_recursive`, which re-applies the
/// oracle (so subfolders open in another pane also short-circuit). Cached
/// symlinks (`is_symlink == true`) are recorded as files without recursing,
/// matching `walk_dir_recursive`'s symlink policy.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors `walk_dir_recursive`'s parameter list to keep state threading consistent."
)]
fn walk_cached_entries<E>(
    parent_path: &Path,
    source_root: &Path,
    cached: Vec<FileEntry>,
    files: &mut Vec<FileInfo>,
    dirs: &mut Vec<PathBuf>,
    total_bytes: &mut u64,
    dedup_bytes: &mut u64,
    last_progress_time: &mut Instant,
    visited: &mut HashSet<PathBuf>,
    seen_inodes: &mut HashSet<u64>,
    volume_id: Option<&str>,
    ctx: &WalkContext<'_, E>,
) -> Result<(), E> {
    for entry in cached {
        if (ctx.is_cancelled)() {
            return Err((ctx.on_cancelled)());
        }
        let child_path = PathBuf::from(&entry.path);
        if entry.is_directory && !entry.is_symlink {
            // Recurse: the oracle re-applies inside `walk_dir_recursive`, so a
            // grandchild dir open in another pane is also short-circuited.
            walk_dir_recursive(
                &child_path,
                source_root,
                files,
                dirs,
                total_bytes,
                dedup_bytes,
                last_progress_time,
                visited,
                seen_inodes,
                volume_id,
                ctx,
            )?;
        } else {
            // File or symlink: record from the cache, no I/O. We can't build a
            // full `FileInfo` (no `std::fs::Metadata`), but we have everything
            // the scan-preview caller actually consumes: path, size, the
            // symlink flag, and inode for hardlink dedup.
            let size = entry.size.unwrap_or(0);
            // `total_bytes` (write footprint) counts every entry at full size.
            // `dedup_bytes` (source footprint) counts each inode once: when the
            // backend supplied an inode (`LocalPosixVolume` populates it for
            // files with `nlink > 1`), only the first occurrence contributes.
            // Backends without inode info leave `inode = None` so every entry
            // is unique. `progress_bytes` mirrors `dedup_bytes` per file.
            let counts = match entry.inode {
                Some(ino) => seen_inodes.insert(ino),
                None => true,
            };
            let progress_bytes = if counts { size } else { 0 };
            *total_bytes += size;
            if counts {
                *dedup_bytes += size;
            }
            if let Some(on_file) = ctx.on_file
                && !entry.is_symlink
            {
                on_file(&child_path, size);
            }
            files.push(FileInfo {
                path: child_path,
                source_root: source_root.to_path_buf(),
                size,
                progress_bytes,
                modified: entry.modified_at.unwrap_or(0),
                created: entry.created_at.unwrap_or(0),
                is_symlink: entry.is_symlink,
            });
        }
    }

    if last_progress_time.elapsed() >= ctx.progress_interval {
        let current_dir = Some(parent_path.display().to_string());
        // Dedup'd running total — see the matching note in `walk_dir_recursive`.
        (ctx.on_progress)(files.len(), dirs.len(), *dedup_bytes, None, current_dir);
        *last_progress_time = Instant::now();
    }

    Ok(())
}

/// Totals returned by `scan_subtree_with_oracle`.
///
/// `per_path` carries one entry per direct child of the scanned `path`, sized
/// to feed into a parent `BatchScanResult` upstream. The vec is empty when
/// `path` itself is a file (the caller knows it's a file in that case).
#[derive(Debug, Clone, Default)]
pub(super) struct SubtreeTotals {
    pub file_count: usize,
    pub dir_count: usize,
    pub total_bytes: u64,
    /// Source on-disk footprint, hardlinks counted once. See
    /// `CopyScanResult::dedup_bytes`. Equal to `total_bytes` when the cached
    /// `FileEntry`s carry no inode (non-local backends).
    pub dedup_bytes: u64,
    /// Per-direct-child results so the scan-preview can populate the
    /// `BatchScanResult::per_path` slot the copy engine reads later.
    pub per_path: Vec<(PathBuf, CopyScanResult)>,
}

/// Scans a subtree using the fresh-listing oracle at every recursion level,
/// falling back to `volume.list_directory` on cache miss.
///
/// This is the oracle-aware analogue of the per-volume `scan_for_copy`. It's
/// designed for `run_volume_scan_preview` to call when the parent directory of
/// the selected sources is watcher-backed: top-level files come from the
/// cached listing directly, top-level directories recurse here.
///
/// Cancellation: the future polls `is_cancelled` between entries. Symlinks
/// (cached `is_symlink == true`) are counted as one entry and not recursed,
/// matching the local-FS walker's policy.
pub(super) async fn scan_subtree_with_oracle(
    volume: &dyn Volume,
    volume_id: &str,
    path: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_progress: Option<&(dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    seen_inodes: &mut HashSet<u64>,
) -> Result<SubtreeTotals, VolumeError> {
    use crate::file_system::volume::ListingProgress;

    if is_cancelled() {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
    }

    // Load entries from oracle or the volume itself.
    let entries: Vec<FileEntry> = match try_get_watched_listing(volume_id, path) {
        Some(e) => e,
        None => volume.list_directory(path, on_progress).await?,
    };

    let mut totals = SubtreeTotals::default();
    // Running tally for the on_progress callback so dirs/bytes climb alongside
    // file count as we walk this subtree. `dir_count` on the SubtreeTotals
    // counts descendant dirs only; this `tally.dirs` mirrors that semantic,
    // but the callback's running count tells the FE "we've seen N dirs so far
    // in this subtree" which is the intuitive display.
    let mut tally = ListingProgress::default();

    for entry in entries {
        if is_cancelled() {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
        }
        let child_path = PathBuf::from(&entry.path);
        if entry.is_directory && !entry.is_symlink {
            // Recurse — oracle re-applies inside this call. The recursive
            // emit reports counts local to the child subtree (starting fresh),
            // so wrap `on_progress` with a baseline of the current `tally` so
            // the FE display stays cumulative across sibling dirs.
            let baseline = tally;
            let child_totals = match on_progress {
                Some(cb) => {
                    let shifted = move |p: ListingProgress| {
                        cb(ListingProgress {
                            files: baseline.files + p.files,
                            dirs: baseline.dirs + p.dirs,
                            bytes: baseline.bytes + p.bytes,
                        })
                    };
                    Box::pin(scan_subtree_with_oracle(
                        volume,
                        volume_id,
                        &child_path,
                        is_cancelled,
                        Some(&shifted),
                        seen_inodes,
                    ))
                    .await?
                }
                None => {
                    Box::pin(scan_subtree_with_oracle(
                        volume,
                        volume_id,
                        &child_path,
                        is_cancelled,
                        None,
                        seen_inodes,
                    ))
                    .await?
                }
            };
            totals.file_count += child_totals.file_count;
            // The directory itself plus all its descendant dirs.
            totals.dir_count += 1 + child_totals.dir_count;
            totals.total_bytes += child_totals.total_bytes;
            totals.dedup_bytes += child_totals.dedup_bytes;
            tally.files += child_totals.file_count;
            tally.dirs += 1 + child_totals.dir_count;
            // The scan-phase climbing display is dedup'd so it converges with
            // the inode-dedup'd index estimate (the copy headline shows the
            // write footprint separately).
            tally.bytes += child_totals.dedup_bytes;
            totals.per_path.push((
                child_path,
                CopyScanResult {
                    file_count: child_totals.file_count,
                    dir_count: child_totals.dir_count,
                    total_bytes: child_totals.total_bytes,
                    dedup_bytes: child_totals.dedup_bytes,
                    top_level_is_directory: true,
                },
            ));
            if let Some(cb) = on_progress {
                cb(tally);
            }
        } else {
            let size = entry.size.unwrap_or(0);
            // Hardlink dedup for the source-footprint number. `FileEntry.inode`
            // is `Some` only for `LocalPosixVolume` files with `nlink > 1`;
            // non-local backends leave it `None` so every file counts as
            // unique. Mirrors `LocalPosixVolume::scan_for_copy`.
            let dedup_contribution = match entry.inode {
                Some(ino) if !seen_inodes.insert(ino) => 0,
                _ => size,
            };
            totals.file_count += 1;
            totals.total_bytes += size;
            totals.dedup_bytes += dedup_contribution;
            tally.files += 1;
            // Dedup'd climbing display — see the dir branch above.
            tally.bytes += dedup_contribution;
            totals.per_path.push((
                child_path,
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: size,
                    dedup_bytes: dedup_contribution,
                    top_level_is_directory: false,
                },
            ));
            if let Some(cb) = on_progress {
                cb(tally);
            }
        }
    }

    Ok(totals)
}

/// Returns `true` if this file's bytes should count toward the running scan
/// total. On Unix, dedupes hardlinks via inode: a file with `nlink > 1` only
/// contributes bytes the first time its inode is seen; subsequent occurrences
/// of the same inode skip the addition. Files with `nlink == 1` (the vast
/// majority) skip the `HashSet` check entirely. On non-Unix, always returns
/// `true` (`std::fs::Metadata` has no `nlink` accessor there).
fn file_bytes_count_toward_total(metadata: &fs::Metadata, seen_inodes: &mut HashSet<u64>) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() <= 1 {
            return true;
        }
        seen_inodes.insert(metadata.ino())
    }
    #[cfg(not(unix))]
    {
        let _ = (metadata, seen_inodes);
        true
    }
}

/// Builds a map from top-level source path to the number of files it contains in the scan result.
///
/// Each `FileInfo` has a `source_root` (the parent of the top-level source) and a `path` (the full
/// file path). The top-level source is reconstructed as `source_root + first component of (path
/// relative to source_root)`.
pub(super) fn build_source_file_counts(files: &[FileInfo]) -> std::collections::HashMap<PathBuf, usize> {
    let mut counts = std::collections::HashMap::new();
    for file_info in files {
        let top_level_source = top_level_source_path(file_info);
        *counts.entry(top_level_source).or_insert(0) += 1;
    }
    counts
}

/// Reconstructs the top-level source path from a `FileInfo`.
///
/// For a file at `/home/user/docs/mydir/sub/file.txt` with `source_root = /home/user/docs`,
/// returns `/home/user/docs/mydir`.
/// For a single file `/home/user/docs/file.txt` with `source_root = /home/user/docs`,
/// returns `/home/user/docs/file.txt`.
pub(super) fn top_level_source_path(file_info: &FileInfo) -> PathBuf {
    if let Ok(relative) = file_info.path.strip_prefix(&file_info.source_root)
        && let Some(first_component) = relative.components().next()
    {
        return file_info.source_root.join(first_component);
    }
    // Fallback: use the path itself (shouldn't happen with well-formed FileInfo)
    file_info.path.clone()
}

/// Tracks per-source-item file counts and emits when all files for a source are done.
pub(super) struct SourceItemTracker {
    totals: std::collections::HashMap<PathBuf, usize>,
    processed: std::collections::HashMap<PathBuf, usize>,
}

impl SourceItemTracker {
    pub fn new(files: &[FileInfo]) -> Self {
        Self {
            totals: build_source_file_counts(files),
            processed: std::collections::HashMap::new(),
        }
    }

    /// Records a processed file. Returns `Some(source_path)` when all files for that source are
    /// done.
    pub fn record(&mut self, file_info: &FileInfo) -> Option<PathBuf> {
        let source_path = top_level_source_path(file_info);
        let count = self.processed.entry(source_path.clone()).or_insert(0);
        *count += 1;
        if self.totals.get(&source_path) == Some(count) {
            Some(source_path)
        } else {
            None
        }
    }
}

/// Tries to get cached scan results for a preview, removing them from cache.
pub(super) fn take_cached_scan_result(preview_id: &str) -> Option<ScanResult> {
    if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
        cache.remove(preview_id).map(|cached| ScanResult {
            files: cached.files,
            dirs: cached.dirs,
            file_count: cached.file_count,
            total_bytes: cached.total_bytes,
            dedup_bytes: cached.dedup_bytes,
            per_path: cached.per_path,
        })
    } else {
        None
    }
}

// ============================================================================
// Scanning helpers
// ============================================================================

/// Sorts files according to the specified column and order.
pub(super) fn sort_files(files: &mut [FileInfo], column: SortColumn, order: SortOrder) {
    files.sort_by(|a, b| {
        let cmp = match column {
            SortColumn::Name => a.name_lower().cmp(&b.name_lower()),
            SortColumn::Extension => a
                .extension()
                .cmp(&b.extension())
                .then_with(|| a.name_lower().cmp(&b.name_lower())),
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Modified => a.modified.cmp(&b.modified),
            SortColumn::Created => a.created.cmp(&b.created),
        };
        match order {
            SortOrder::Ascending => cmp,
            SortOrder::Descending => cmp.reverse(),
        }
    });
}

/// Scans source paths recursively, returns file list and totals.
/// Files are sorted according to the specified column and order.
///
/// Uses polling-based cancellation to remain responsive even when filesystem
/// operations block (for example, on stuck network drives).
pub(super) fn scan_sources(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
) -> Result<ScanResult, WriteOperationError> {
    let progress_interval = state.progress_interval;

    run_cancellable_scoped(
        || {
            scan_sources_internal(
                sources,
                state,
                events,
                operation_id,
                operation_type,
                sort_column,
                sort_order,
                progress_interval,
            )
        },
        state,
        "scan",
        operation_id,
    )
}

/// Internal scan implementation (runs in background thread).
#[allow(
    clippy::too_many_arguments,
    reason = "Internal helper passes through all required context"
)]
fn scan_sources_internal(
    sources: &[PathBuf],
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: WriteOperationType,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval: Duration,
) -> Result<ScanResult, WriteOperationError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    // Write footprint (every file at full size) and `du`-equivalent source
    // footprint (each inode once). See `walk_dir_recursive`.
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();
    // Shared across all sources in this scan so a file hardlinked between
    // separate source roots still only contributes its bytes once, matching
    // what the indexer does for dir_stats aggregation.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Index-derived expected totals: the denominator the FE renders the
    // scan-phase progress bar against while the foolproof scan runs. `None`
    // when any source isn't in the index; the FE falls back to tallies only.
    let expected = crate::indexing::read::expected_totals::expected_totals_for_sources(sources);
    log::debug!(
        "scan: op={} index expected={}",
        operation_id,
        expected
            .map(|e| format!("{} files / {} bytes", e.files, e.bytes))
            .unwrap_or_else(|| "(not available)".to_string())
    );

    let ctx = WalkContext {
        progress_interval,
        is_cancelled: &|| super::state::is_cancelled(&state.intent),
        on_io_error: &|path, e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: e.to_string(),
        },
        on_cancelled: &|| WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        },
        on_symlink_loop: &|path| WriteOperationError::SymlinkLoop {
            path: path.display().to_string(),
        },
        on_progress: &|files_done, dirs_done, bytes_done, current_file, current_dir| {
            log::debug!(
                "scan: emitting write-progress op={} phase=scanning files_found={} dirs_found={} bytes_found={}",
                operation_id,
                files_done,
                dirs_done,
                bytes_done
            );
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    operation_type,
                    WriteOperationPhase::Scanning,
                    current_file.clone(),
                    files_done,
                    0,
                    bytes_done,
                    0,
                )
                .with_scan_meta(current_dir, dirs_done, expected),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Scanning,
                current_file,
                files_done,
                0,
                bytes_done,
                0,
            );
        },
        // The real copy/move/delete scan never samples for a compress estimate.
        on_file: None,
    };

    // Local FS scan goes through `LocalPosixVolume`, which is always registered as
    // the `"root"` volume. Passing it threads the oracle through: when the source
    // (or any subdirectory we recurse into) is open in a pane with a live FSEvents
    // watcher, the walker skips the disk read for that level.
    let volume_id = Some(crate::file_system::volume::DEFAULT_VOLUME_ID);

    for source in sources {
        let source_root = source.parent().unwrap_or(source);
        walk_dir_recursive(
            source,
            source_root,
            &mut files,
            &mut dirs,
            &mut total_bytes,
            &mut dedup_bytes,
            &mut last_progress_time,
            &mut visited,
            &mut seen_inodes,
            volume_id,
            &ctx,
        )?;
    }

    // Sort files according to configuration
    sort_files(&mut files, sort_column, sort_order);

    // Emit final scanning progress. The scan-phase bar reports the dedup'd
    // running total (matches the inode-dedup'd index estimate); the final
    // snapshot does the same so it lands exactly on the estimate.
    log::debug!(
        "scan: emitting final write-progress op={} phase=scanning files={} write_bytes={} dedup_bytes={}",
        operation_id,
        files.len(),
        total_bytes,
        dedup_bytes
    );
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Scanning,
            None,
            files.len(),
            files.len(),
            dedup_bytes,
            dedup_bytes,
        )
        .with_scan_meta(None, dirs.len(), expected),
    );

    Ok(ScanResult {
        file_count: files.len(),
        files,
        dirs,
        total_bytes,
        dedup_bytes,
        per_path: Vec::new(),
    })
}

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
        if (dest_path.exists() || fs::symlink_metadata(&dest_path).is_ok())
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
        if dest_path.exists()
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

#[cfg(test)]
mod tests {
    use super::super::state::FileInfo;
    use super::*;

    fn make_file_info(path: &str, source_root: &str) -> FileInfo {
        FileInfo {
            path: PathBuf::from(path),
            source_root: PathBuf::from(source_root),
            size: 100,
            progress_bytes: 100,
            modified: 0,
            created: 0,
            is_symlink: false,
        }
    }

    #[test]
    fn test_top_level_source_path_file() {
        let fi = make_file_info("/home/user/docs/file.txt", "/home/user/docs");
        assert_eq!(top_level_source_path(&fi), PathBuf::from("/home/user/docs/file.txt"));
    }

    #[test]
    fn test_top_level_source_path_nested() {
        let fi = make_file_info("/home/user/docs/mydir/sub/file.txt", "/home/user/docs");
        assert_eq!(top_level_source_path(&fi), PathBuf::from("/home/user/docs/mydir"));
    }

    #[test]
    fn test_build_source_file_counts_mixed() {
        let files = vec![
            make_file_info("/home/docs/file1.txt", "/home/docs"),
            make_file_info("/home/docs/mydir/a.txt", "/home/docs"),
            make_file_info("/home/docs/mydir/b.txt", "/home/docs"),
            make_file_info("/home/docs/mydir/sub/c.txt", "/home/docs"),
            make_file_info("/home/docs/other/x.txt", "/home/docs"),
        ];
        let counts = build_source_file_counts(&files);
        assert_eq!(counts.len(), 3);
        assert_eq!(counts[&PathBuf::from("/home/docs/file1.txt")], 1);
        assert_eq!(counts[&PathBuf::from("/home/docs/mydir")], 3);
        assert_eq!(counts[&PathBuf::from("/home/docs/other")], 1);
    }

    #[test]
    fn test_build_source_file_counts_empty() {
        let counts = build_source_file_counts(&[]);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_build_source_file_counts_single_file() {
        let files = vec![make_file_info("/tmp/a.txt", "/tmp")];
        let counts = build_source_file_counts(&files);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[&PathBuf::from("/tmp/a.txt")], 1);
    }

    // ── Walker integration tests ─────────────────────────────────────────

    /// Result bundle from `run_walker` / `run_walker_with_sources`. Named
    /// fields avoid `clippy::type_complexity` on the helper's return type.
    struct WalkOutcome {
        files: Vec<FileInfo>,
        /// Write footprint (every file at full size).
        bytes: u64,
        /// `du`-equivalent source footprint (each inode once).
        dedup_bytes: u64,
        /// Captured `(current_file, current_dir)` pairs from each `on_progress` call.
        progress: Vec<(Option<String>, Option<String>)>,
    }

    /// Run the walker over `root`, with `progress_interval = 0` so the
    /// callback fires on every entry. Captures progress payloads for assertions.
    fn run_walker(root: &Path) -> WalkOutcome {
        run_walker_with_sources(&[root.to_path_buf()])
    }

    fn run_walker_with_sources(sources: &[PathBuf]) -> WalkOutcome {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut total_bytes = 0u64;
        let mut dedup_bytes = 0u64;
        let mut last_progress = Instant::now() - Duration::from_secs(60);
        let mut visited = HashSet::new();
        let mut seen_inodes = HashSet::new();
        let captured = std::cell::RefCell::new(Vec::new());
        let ctx = WalkContext::<'_, String> {
            progress_interval: Duration::from_millis(0),
            is_cancelled: &|| false,
            on_io_error: &|p, e| format!("io: {} {}", p.display(), e),
            on_cancelled: &|| "cancelled".to_string(),
            on_symlink_loop: &|p| format!("loop: {}", p.display()),
            on_progress: &|_, _, _, cur_file, cur_dir| {
                captured.borrow_mut().push((cur_file, cur_dir));
            },
            on_file: None,
        };
        for source in sources {
            let source_root = source.parent().unwrap_or(source);
            walk_dir_recursive(
                source,
                source_root,
                &mut files,
                &mut dirs,
                &mut total_bytes,
                &mut dedup_bytes,
                &mut last_progress,
                &mut visited,
                &mut seen_inodes,
                None,
                &ctx,
            )
            .expect("walk should succeed");
        }
        WalkOutcome {
            files,
            bytes: total_bytes,
            dedup_bytes,
            progress: captured.into_inner(),
        }
    }

    #[test]
    fn walker_emits_current_dir_for_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let subdir = root.join("inner");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("a.txt"), b"hello").unwrap();

        let outcome = run_walker(root);

        // Find the progress event for a.txt. Its parent dir should be `inner`.
        let a_event = outcome
            .progress
            .iter()
            .find(|(f, _)| f.as_deref() == Some("a.txt"))
            .expect("walker should have emitted progress for a.txt");
        let dir = a_event.1.as_deref().expect("current_dir should be set for a file");
        assert!(dir.ends_with("inner"), "expected dir to end with 'inner', got: {dir}");
    }

    #[test]
    fn walker_sums_bytes_for_unique_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("a.bin"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("b.bin"), vec![0u8; 2000]).unwrap();

        let outcome = run_walker(root);
        assert_eq!(outcome.files.len(), 2);
        assert_eq!(outcome.bytes, 3000);
    }

    #[cfg(unix)]
    #[test]
    fn walker_dedupes_hardlinks_by_inode() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let original = root.join("original.bin");
        let link = root.join("link.bin");
        fs::write(&original, vec![0u8; 1000]).unwrap();
        fs::hard_link(&original, &link).unwrap();

        let outcome = run_walker(root);
        // Both directory entries are visited (the delete/copy op must unlink both)…
        assert_eq!(outcome.files.len(), 2, "both hardlinked entries should be enumerated");
        // …the write footprint counts both (a cross-volume copy writes both)…
        assert_eq!(
            outcome.bytes, 2000,
            "write footprint should count both hardlinked entries"
        );
        // …but the source footprint counts the inode once (what delete frees).
        assert_eq!(
            outcome.dedup_bytes, 1000,
            "source footprint should count the shared inode once"
        );
    }

    #[cfg(unix)]
    #[test]
    fn walker_dedupes_hardlinks_across_separate_sources() {
        // A file hardlinked into two different source roots in one scan
        // should still contribute its bytes exactly once.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let dir_a = root.join("a");
        let dir_b = root.join("b");
        fs::create_dir(&dir_a).unwrap();
        fs::create_dir(&dir_b).unwrap();
        let original = dir_a.join("file.bin");
        fs::write(&original, vec![0u8; 5000]).unwrap();
        fs::hard_link(&original, dir_b.join("file.bin")).unwrap();

        let outcome = run_walker_with_sources(&[dir_a.clone(), dir_b.clone()]);
        assert_eq!(outcome.files.len(), 2);
        assert_eq!(
            outcome.bytes, 10000,
            "write footprint counts both copies (cross-volume copy writes both)"
        );
        assert_eq!(
            outcome.dedup_bytes, 5000,
            "source footprint counts the shared inode once across source roots"
        );
    }

    #[cfg(unix)]
    #[test]
    fn walker_does_not_dedupe_distinct_inodes_with_same_size() {
        // Sanity: two unrelated 1000-byte files (distinct inodes) should
        // sum to 2000, not 1000.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("a.bin"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("b.bin"), vec![1u8; 1000]).unwrap();

        let outcome = run_walker(root);
        assert_eq!(outcome.bytes, 2000);
    }

    // ── WriteProgressEvent constructor / builder tests ───────────────────

    #[test]
    fn write_progress_new_defaults_scan_meta_to_none() {
        use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
        let event = WriteProgressEvent::new(
            "op-1".to_string(),
            WriteOperationType::Delete,
            WriteOperationPhase::Scanning,
            Some("foo.txt".to_string()),
            10,
            0,
            1234,
            0,
        );
        assert_eq!(event.current_dir, None);
        assert_eq!(event.dirs_done, 0);
        assert_eq!(event.expected_files_total, None);
        assert_eq!(event.expected_bytes_total, None);
    }

    #[test]
    fn with_scan_meta_populates_all_fields() {
        use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
        use crate::indexing::read::expected_totals::ExpectedTotals;
        let event = WriteProgressEvent::new(
            "op-1".to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::Scanning,
            Some("foo.txt".to_string()),
            10,
            0,
            500,
            0,
        )
        .with_scan_meta(
            Some("/some/dir".to_string()),
            3,
            Some(ExpectedTotals {
                files: 100,
                bytes: 5000,
            }),
        );
        assert_eq!(event.current_dir.as_deref(), Some("/some/dir"));
        assert_eq!(event.dirs_done, 3);
        assert_eq!(event.expected_files_total, Some(100));
        assert_eq!(event.expected_bytes_total, Some(5000));
    }

    #[test]
    fn with_scan_meta_handles_missing_expected_totals() {
        use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
        let event = WriteProgressEvent::new(
            "op-1".to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::Scanning,
            None,
            0,
            0,
            0,
            0,
        )
        .with_scan_meta(Some("/x".to_string()), 2, None);
        assert_eq!(event.current_dir.as_deref(), Some("/x"));
        assert_eq!(event.dirs_done, 2);
        // No expected totals → fields stay None so the FE falls back to tallies-only.
        assert_eq!(event.expected_files_total, None);
        assert_eq!(event.expected_bytes_total, None);
    }
}
