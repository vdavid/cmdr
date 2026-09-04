//! The scan every backend whose only tools are a stat and a listing gets for
//! free: the recursive walk behind `scan_for_copy`, the batch loop behind
//! `scan_for_copy_batch`, and the name matcher behind `scan_for_conflicts`.
//!
//! **Why it lives here rather than in each backend.** The walk is arithmetic
//! over two operations every `Volume` already has, so a per-backend copy differs
//! from its neighbours only in the type name, and then drifts. A dedup rule
//! fixed in one copy and not the others is a wrong byte total in a transfer
//! estimate, which is the number the user decides on.
//!
//! ❗ **The walk lists, it never stats a child.** A stat is a round trip and a
//! listing already carries every child's size and type, so a 1,000-file folder
//! costs one round trip per DIRECTORY instead of one per file. Over a 50 ms link
//! that is the difference between a second and a minute.
//!
//! ❌ **Nothing here consults a listing cache.** A backend whose watcher backs
//! the claim may short-circuit its own batch scan against
//! `authoritative_listing` (SMB and MTP do, in their own `scan.rs`); a backend
//! with no watcher must not, or a pre-flight conflict scan misses a file and a
//! copy overwrites it. Reaching the cache is opt-in per backend, above this
//! module, and there is nothing to opt into here.
//!
//! ❌ **A scan reports no listing change.** It crosses every entry in a tree, and
//! one seam call per entry would sweep every cached listing on the volume
//! thousands of times.
//!
//! ❗ **Every entry passes the [`ScanBoundary`].** `boundary.dir()` runs BEFORE
//! the listing round trip and `boundary.file()` on every leaf, so a walk over a
//! sleeping share stops within one round trip of a Cancel and stands still on a
//! Pause. A backend that takes its own walk owes the same two calls; nothing else
//! makes Cancel work for it.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use crate::entry::FileEntry;
use crate::volume::VolumeError;
use crate::volume::{BatchScanResult, CopyScanResult, ScanBoundary, ScanConflict, SourceItemInfo};

/// A future the walk can recurse through. `async fn` can't call itself, so every
/// step of the walk hands back a boxed one, and so does every [`ScanSource`]
/// method an implementor writes.
pub type Walking<'a, T> = Pin<Box<dyn Future<Output = Result<T, VolumeError>> + Send + 'a>>;

/// The two operations a tree walk needs from a backend.
///
/// ❗ Both must be the backend's OWN read path, ❌ never a listing-cache lookup:
/// the module docs say why. Implement it on the volume and the three
/// `scan_for_*` bodies come with it.
pub trait ScanSource: Sync {
    /// One entry's type and size. The top of a walk is the only place a scan
    /// stats anything.
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry>;

    /// One directory's children, each carrying its own type and size.
    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>>;
}

/// One subtree's file count, directory count, and bytes.
///
/// `dedup_bytes` tracks `total_bytes` exactly: a backend reaching this walk has
/// no link count to dedupe by, so the source footprint IS the write footprint.
/// A backend that grows one takes its own walk rather than a flag here.
pub fn scan_tree<'a>(
    source: &'a dyn ScanSource,
    path: &'a Path,
    boundary: &'a ScanBoundary<'a>,
) -> Walking<'a, CopyScanResult> {
    Box::pin(async move {
        let top = source.scan_stat(path).await?;
        if !top.is_directory {
            let size = top.size.unwrap_or(0);
            boundary.file(size).await?;
            return Ok(CopyScanResult {
                file_count: 1,
                dir_count: 0,
                total_bytes: size,
                dedup_bytes: size,
                top_level_is_directory: false,
            });
        }
        let mut result = CopyScanResult {
            file_count: 0,
            dir_count: 0,
            total_bytes: 0,
            dedup_bytes: 0,
            top_level_is_directory: true,
        };
        walk_directory(source, path, boundary, &mut result).await?;
        Ok(result)
    })
}

/// One subtree, with nothing to report to and nobody to answer to.
///
/// ❗ The single-path trait method takes neither a callback nor a stop, so a
/// boundary here would have nowhere to send its counts and nothing to ask.
/// [`scan_trees`] is the one a scan preview drives, and the one a person can
/// cancel.
pub fn scan_one<'a>(source: &'a dyn ScanSource, path: &'a Path) -> Walking<'a, CopyScanResult> {
    Box::pin(async move {
        let boundary = ScanBoundary::silent();
        scan_tree(source, path, &boundary).await
    })
}

/// Several subtrees, with the counts climbing across all of them.
///
/// ❗ One boundary for the whole call, so a preview's counters keep rising
/// through path two rather than restarting: cumulative-for-the-call is what the
/// trait promises. It is also what carries the stop, so every path in the batch
/// answers to the same Cancel.
pub fn scan_trees<'a>(
    source: &'a dyn ScanSource,
    paths: &'a [PathBuf],
    boundary: &'a ScanBoundary<'a>,
) -> Walking<'a, BatchScanResult> {
    Box::pin(async move {
        let mut per_path = Vec::with_capacity(paths.len());
        for path in paths {
            per_path.push((path.clone(), scan_tree(source, path, boundary).await?));
        }
        Ok(fold_batch(per_path))
    })
}

/// Folds per-path scans into the aggregate the batch method answers with.
///
/// ❗ `top_level_is_directory` is only meaningful for a single path: an
/// aggregate over several has no one type, and callers that need it read
/// `per_path`. Public because a backend with its own batch strategy (SMB's
/// oracle short-circuit, MTP's parent grouping) still owes the same fold.
pub fn fold_batch(per_path: Vec<(PathBuf, CopyScanResult)>) -> BatchScanResult {
    let mut aggregate = CopyScanResult {
        file_count: 0,
        dir_count: 0,
        total_bytes: 0,
        dedup_bytes: 0,
        top_level_is_directory: false,
    };
    for (_, scan) in &per_path {
        aggregate.file_count += scan.file_count;
        aggregate.dir_count += scan.dir_count;
        aggregate.total_bytes += scan.total_bytes;
        aggregate.dedup_bytes += scan.dedup_bytes;
    }
    if let [(_, only)] = per_path.as_slice() {
        aggregate.top_level_is_directory = only.top_level_is_directory;
    }
    BatchScanResult { aggregate, per_path }
}

/// Which of `source_items` already have a name among `dest_entries`.
///
/// ❗ Pure, and takes the listing rather than fetching it: a backend that lists
/// its destination differently (SMB and MTP go through their own cache-aware
/// paths) still owes the caller the same `ScanConflict` shape, and the shape is
/// what a conflict dialog renders. [`scan_conflicts`] is the wrapper for a
/// backend with nothing special to say about the listing.
pub fn conflicts_against(source_items: &[SourceItemInfo], dest_entries: &[FileEntry]) -> Vec<ScanConflict> {
    let mut conflicts = Vec::new();
    for item in source_items {
        let Some(existing) = dest_entries.iter().find(|entry| entry.name == item.name) else {
            continue;
        };
        conflicts.push(ScanConflict {
            source_path: item.name.clone(),
            dest_path: existing.path.clone(),
            source_size: item.size,
            dest_size: existing.size.unwrap_or(0),
            source_modified: item.modified,
            dest_modified: existing.modified_at.map(|seconds| seconds as i64),
            source_is_directory: item.is_directory,
            dest_is_directory: existing.is_directory,
        });
    }
    conflicts
}

/// One listing of `dest_path`, matched against `source_items`.
///
/// ❗ This is where a walk-driven backend keeps
/// [`Volume::scan_for_conflicts`](crate::volume::Volume::scan_for_conflicts)'
/// promise that a destination which isn't there yet answers an empty list; that
/// method's doc says why the promise exists.
pub fn scan_conflicts<'a>(
    source: &'a dyn ScanSource,
    source_items: &'a [SourceItemInfo],
    dest_path: &'a Path,
) -> Walking<'a, Vec<ScanConflict>> {
    Box::pin(async move {
        let entries = match source.scan_list(dest_path).await {
            Ok(entries) => entries,
            Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(conflicts_against(source_items, &entries))
    })
}

/// One directory's contents, folded into `into`, recursing into children.
///
/// Split from [`scan_tree`] so the top-level stat is paid once rather than once
/// per level: a child's type and size are already in its parent's listing.
fn walk_directory<'a>(
    source: &'a dyn ScanSource,
    dir: &'a Path,
    boundary: &'a ScanBoundary<'a>,
    into: &'a mut CopyScanResult,
) -> Walking<'a, ()> {
    Box::pin(async move {
        into.dir_count += 1;
        boundary.dir().await?;
        for entry in source.scan_list(dir).await? {
            let child = dir.join(&entry.name);
            // ❗ A symlinked directory is ONE entry, never a subtree. Following
            // one double-counts its target (Android's `/sdcard` and
            // `/storage/emulated/0` are the same bytes) and a link pointing at
            // an ancestor turns the scan into a hang. `scan_preview.rs` makes
            // the same promise app-side, so a copy estimate reads the same
            // whichever walker produced it.
            if entry.is_directory && !entry.is_symlink {
                walk_directory(source, &child, boundary, into).await?;
            } else {
                let size = entry.size.unwrap_or(0);
                into.file_count += 1;
                into.total_bytes += size;
                into.dedup_bytes += size;
                boundary.file(size).await?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "scan_walk_test.rs"]
mod scan_walk_test;
