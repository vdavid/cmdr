//! Walking a subtree to say what a copy is about to cost, and reading a
//! destination for names that are already taken.
//!
//! Both are READS: ❗ neither reports a listing change, and ❌ nothing here
//! calls `authoritative_listing` (no watcher, so no cached listing is fresh).
//! The walk is one `LIST` per DIRECTORY, ❌ never a stat per child: a listing
//! already carries every child's size and type.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, ListingProgress, ScanConflict, ScanTicker, SourceItemInfo, VolumeError,
};

use super::AdbVolume;

impl AdbVolume {
    /// One subtree's file count, directory count, and bytes.
    pub(super) fn scan_for_copy_impl<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let ticker = ScanTicker::new(None);
            self.scan_recursive(path, &ticker).await
        })
    }

    /// Several subtrees, with the counts climbing across all of them: ❗ one
    /// ticker for the whole call, cumulative-for-the-call being what the trait
    /// promises.
    pub(super) fn scan_for_copy_batch_impl<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let ticker = ScanTicker::new(on_progress);
            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                dedup_bytes: 0,
                top_level_is_directory: false,
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for path in paths {
                let scan = self.scan_recursive(path, &ticker).await?;
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
                aggregate.dedup_bytes += scan.dedup_bytes;
                per_path.push((path.clone(), scan));
            }
            if paths.len() == 1 {
                aggregate.top_level_is_directory = per_path[0].1.top_level_is_directory;
            }
            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    /// Which of `source_items` already have a name at `dest_path`. One listing,
    /// ❗ never one `exists()` per item.
    pub(super) fn scan_for_conflicts_impl<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = match self.list_directory_impl(dest_path, None, None).await {
                Ok(entries) => entries,
                // A destination that isn't there yet holds nothing.
                Err(VolumeError::NotFound(_)) => return Ok(Vec::new()),
                Err(e) => return Err(e),
            };
            let mut conflicts = Vec::new();
            for item in source_items {
                let Some(existing) = entries.iter().find(|entry| entry.name == item.name) else {
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
            Ok(conflicts)
        })
    }

    /// The walk itself: one stat for the top, then one listing per directory.
    fn scan_recursive<'a>(
        &'a self,
        path: &'a Path,
        ticker: &'a ScanTicker<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let top = self.get_metadata_impl(path).await?;
            if !top.is_directory {
                let size = top.size.unwrap_or(0);
                ticker.file(size);
                return Ok(CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: size,
                    // No link count on the wire, so the source footprint is
                    // the write footprint.
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
            self.scan_directory(path, ticker, &mut result).await?;
            Ok(result)
        })
    }

    /// One directory's contents, folded into `into`, recursing into children.
    fn scan_directory<'a>(
        &'a self,
        dir: &'a Path,
        ticker: &'a ScanTicker<'a>,
        into: &'a mut CopyScanResult,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            into.dir_count += 1;
            ticker.dir();
            let entries = self.list_directory_impl(dir, None, None).await?;
            for entry in entries {
                let child = dir.join(&entry.name);
                // A symlinked folder is counted as the one entry it is, not
                // walked: following links in a scan is how a loop becomes a
                // hang.
                if entry.is_directory && !entry.is_symlink {
                    self.scan_directory(&child, ticker, into).await?;
                } else {
                    let size = entry.size.unwrap_or(0);
                    into.file_count += 1;
                    into.total_bytes += size;
                    into.dedup_bytes += size;
                    ticker.file(size);
                }
            }
            Ok(())
        })
    }
}
