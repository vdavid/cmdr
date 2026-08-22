//! Walking a subtree to say what a copy is about to cost, and reading a
//! destination for names that are already taken.
//!
//! Both are READS. ❗ Neither may report a listing change: a scan crosses every
//! entry in a tree, and one seam call per entry would sweep every cached listing
//! on the volume thousands of times (`host_seam_test.rs` is what holds this to
//! it).
//!
//! **The walk uses `list_directory_impl`, ❌ never a stat per child.** A stat is
//! a round trip and a directory listing already carries every child's size and
//! type, so a 1 000-file folder costs one round trip per DIRECTORY instead of one
//! per file. Over a 50 ms link that is the difference between a second and a
//! minute.
//!
//! ❌ **Nothing here calls `authoritative_listing`.** This backend has no
//! watcher, so its `listing_watch_coverage` is `None` and a cached listing is
//! only as fresh as the last time somebody looked. SMB's scan may consult the
//! cache because its watcher backs the claim; borrowing that shortcut here is how
//! a pre-flight conflict scan misses a file and a copy overwrites it.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::{BatchScanResult, CopyScanResult, ListingProgress, ScanConflict, ScanTicker, SourceItemInfo};

use super::SftpVolume;

impl SftpVolume {
    /// One subtree's file count, directory count, and bytes.
    pub(super) fn scan_for_copy_impl<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        // No callback on the single-path trait method: it gives one nowhere to
        // go. The batch method below is what the scan preview drives.
        Box::pin(async move {
            let ticker = ScanTicker::new(None);
            self.scan_recursive(path, &ticker).await
        })
    }

    /// Several subtrees, with the counts climbing across all of them.
    ///
    /// ❗ One ticker for the whole call, so the preview's counters keep rising
    /// through path two rather than restarting: cumulative-for-the-call is what
    /// the trait promises.
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
                // An aggregate over several paths has no one type. Callers that
                // need it read `per_path`.
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

    /// Which of `source_items` already have a name at `dest_path`.
    ///
    /// One listing, ❗ never one `exists()` per item: a 200-file paste would be
    /// 200 round trips against the one the listing costs.
    pub(super) fn scan_for_conflicts_impl<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let entries = match self.list_directory_impl(dest_path, None, None).await {
                Ok(entries) => entries,
                // A destination that isn't there yet holds nothing, so nothing
                // clashes. ❗ Reporting the error instead would turn "paste into
                // a folder I'm about to create" into a failure.
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
    ///
    /// Boxed because it recurses, which an `async fn` can't do on its own.
    fn scan_recursive<'a>(
        &'a self,
        path: &'a Path,
        ticker: &'a ScanTicker<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let top = self.get_metadata_impl(path).await?;
            if !top.is_directory {
                ticker.file(top.size.unwrap_or(0));
                return Ok(CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: top.size.unwrap_or(0),
                    // SFTP v3's stat carries no link count, so the source
                    // footprint is always taken as the write footprint. Kept in
                    // lockstep with `total_bytes` at every accumulation site.
                    dedup_bytes: top.size.unwrap_or(0),
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
    ///
    /// Split from [`Self::scan_recursive`] so the top-level stat is paid once
    /// rather than once per level: a child's type and size are already in its
    /// parent's listing.
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
                if entry.is_directory {
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
