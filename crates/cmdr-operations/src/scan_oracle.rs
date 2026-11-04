//! The oracle-aware subtree scan: one subtree's totals, cache-fed where a
//! watcher can vouch for the listing, else from `Volume::list_directory`. Any
//! volume's, so it serves the volume scan preview in every build; the local
//! walk over top-level sources is `scan.rs`.

use cmdr_fs::entry::FileEntry;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{CopyScanResult, ScanRecorder, Volume, VolumeError};

/// Totals returned by `scan_subtree_with_oracle`.
///
/// `per_path` carries one entry per direct child of the scanned `path`, sized
/// to feed into a parent `BatchScanResult` upstream. The vec is empty when
/// `path` itself is a file (the caller knows it's a file in that case).
// DEFAULT-OK: zeroed totals are what a walk that hasn't counted anything has counted.
// This is an accumulator seed, not a finding: it only ever grows from here.
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
///
/// `pause` parks the walk at those same boundaries when the owning operation is
/// paused (`super::scan_bridge::ScanPause`), so pause is exactly as responsive
/// as cancel here.
///
/// `recorder` gets `path` and every entry below it, for a transfer's live tree
/// (`super::transfer_tree`), named the way the listing names them.
///
/// `listings` is the oracle: the engine host's view of what the open panes keep
/// fresh.
#[allow(
    clippy::too_many_arguments,
    reason = "one recursion's inputs: where to walk, what stops it, and who hears about it"
)]
pub(super) async fn scan_subtree_with_oracle(
    listings: &dyn ListingHost,
    volume: &dyn Volume,
    volume_id: &str,
    path: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    pause: Option<&super::scan_bridge::ScanPause>,
    on_progress: Option<&(dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    recorder: Option<&dyn ScanRecorder>,
    seen_inodes: &mut HashSet<u64>,
) -> Result<SubtreeTotals, VolumeError> {
    use cmdr_fs::volume::ListingProgress;

    if let Some(recorder) = recorder {
        recorder.dir(path);
    }

    if is_cancelled() {
        return Err(VolumeError::Cancelled("Operation cancelled by user".into()));
    }
    if let Some(pause) = pause {
        pause.park_while_paused_async().await;
    }

    // Load entries from oracle or the volume itself.
    let entries: Vec<FileEntry> = match listings.authoritative_listing(volume_id, path) {
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
            return Err(VolumeError::Cancelled("Operation cancelled by user".into()));
        }
        // After the cancel check, never before: cancel outranks pause.
        if let Some(pause) = pause {
            pause.park_while_paused_async().await;
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
                            ..p
                        })
                    };
                    Box::pin(scan_subtree_with_oracle(
                        listings,
                        volume,
                        volume_id,
                        &child_path,
                        is_cancelled,
                        pause,
                        Some(&shifted),
                        recorder,
                        seen_inodes,
                    ))
                    .await?
                }
                None => {
                    Box::pin(scan_subtree_with_oracle(
                        listings,
                        volume,
                        volume_id,
                        &child_path,
                        is_cancelled,
                        pause,
                        None,
                        recorder,
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
                    top_level_modified_at: entry.modified_at,
                    top_level_created_at: entry.created_at,
                },
            ));
            if let Some(cb) = on_progress {
                cb(tally);
            }
        } else {
            let size = entry.size.unwrap_or(0);
            if let Some(recorder) = recorder {
                recorder.file(&child_path, size);
            }
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
                    top_level_modified_at: entry.modified_at,
                    top_level_created_at: entry.created_at,
                },
            ));
            if let Some(cb) = on_progress {
                cb(tally);
            }
        }
    }

    Ok(totals)
}
