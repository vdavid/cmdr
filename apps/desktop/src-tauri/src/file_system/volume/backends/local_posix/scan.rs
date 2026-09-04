//! The copy-scan family's inherent bodies (`scan_subtree`,
//! `scan_for_copy_batch_with_boundary_impl`, `scan_for_conflicts_impl`), which
//! the trait methods in `local_posix.rs` delegate to.
//!
//! Split out for the same reason `mtp/scan.rs` and
//! `crates/cmdr-smb/src/volume/scan.rs` are: counting a tree while staying
//! cancelable is its own subject rather than one more `Volume` method. A trait
//! impl can't span files, so the trait side stays in the parent and the work
//! lives here.

use super::super::{CopyScanResult, ScanConflict, SourceItemInfo, VolumeError};
use super::LocalPosixVolume;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use tokio::task::spawn_blocking;
use walkdir::WalkDir;

impl LocalPosixVolume {
    /// One subtree's file count, directory count, and bytes, stopping when `stop`
    /// says to.
    ///
    /// ❗ The walk runs inside `spawn_blocking`, so it asks
    /// [`ScanStop::should_stop_blocking`](cmdr_fs::volume::ScanStop::should_stop_blocking)
    /// — the twin that parks this pool THREAD. The async one has nothing to await
    /// on in here. `ScanStop` is `Arc`-held precisely so it can cross into the
    /// closure.
    ///
    /// ❗ Per entry, ❌ not per directory. On a mounted network share (`smbfs`,
    /// NFS) a single `readdir` can take seconds and a subtree minutes, and `stop`
    /// costs two atomic loads when nobody has pressed anything.
    pub(super) fn scan_subtree<'a>(
        &'a self,
        path: &'a Path,
        stop: cmdr_fs::volume::ScanStop,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let abs_path = self.resolve(path);
        Box::pin(async move {
            spawn_blocking(move || {
                use std::os::unix::fs::MetadataExt;

                let mut file_count = 0;
                let mut dir_count = 0;
                let mut total_bytes = 0u64;
                // `dedup_bytes` is the source's on-disk (`du`) footprint:
                // each inode counted once. `total_bytes` keeps counting every
                // hardlink (the cross-volume write footprint). The set is
                // scoped to this one top-level source; cross-source hardlinks
                // aren't deduped (rare; see `CopyScanResult::dedup_bytes`).
                let mut dedup_bytes = 0u64;
                let mut seen_inodes: std::collections::HashSet<u64> = std::collections::HashSet::new();

                for entry in WalkDir::new(&abs_path).min_depth(0) {
                    if stop.should_stop_blocking() {
                        return Err(cmdr_fs::volume::scan_stopped());
                    }
                    let entry = entry.map_err(|e| VolumeError::IoError {
                        message: e.to_string(),
                        raw_os_error: None,
                    })?;
                    let ft = entry.file_type();
                    if ft.is_file() {
                        file_count += 1;
                        if let Ok(meta) = entry.metadata() {
                            let len = meta.len();
                            total_bytes += len;
                            // `nlink == 1` is the overwhelmingly common case
                            // (no hardlinks): skip the set entirely. Only
                            // multiply-linked inodes pay the lookup.
                            if meta.nlink() <= 1 || seen_inodes.insert(meta.ino()) {
                                dedup_bytes += len;
                            }
                        }
                    } else if ft.is_dir() {
                        // Don't count the root itself if it's the starting point
                        if entry.depth() > 0 {
                            dir_count += 1;
                        }
                    }
                }

                // Top-level stat (also fills in single-file / empty-dir edge cases).
                // This runs regardless so we can populate `top_level_is_directory`
                // without re-statting downstream.
                let top_meta = std::fs::metadata(&abs_path).ok();
                let top_level_is_directory = top_meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

                // If the path is a single file, count it
                if let Some(ref meta) = top_meta {
                    if meta.is_file() && file_count == 0 {
                        file_count = 1;
                        total_bytes = meta.len();
                        dedup_bytes = meta.len();
                    } else if meta.is_dir() && dir_count == 0 && file_count == 0 {
                        dir_count = 1;
                    }
                }

                Ok(CopyScanResult {
                    file_count,
                    dir_count,
                    total_bytes,
                    dedup_bytes,
                    top_level_is_directory,
                })
            })
            .await
            .expect("spawn_blocking scan_for_copy closure doesn't panic and the task is uncancelable")
        })
    }

    /// Walks each source in turn, asking `boundary` between paths and handing
    /// `scan_subtree` the stop so it also asks inside one.
    pub(super) fn scan_for_copy_batch_with_boundary_impl<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a cmdr_fs::volume::ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut per_path = Vec::with_capacity(paths.len());
            for path in paths {
                boundary.check().await?;
                let scan = self.scan_subtree(path, boundary.stop()).await?;
                boundary
                    .subtree(scan.file_count, scan.dir_count, scan.total_bytes)
                    .await?;
                per_path.push((path.clone(), scan));
            }
            Ok(cmdr_fs::volume::fold_batch(per_path))
        })
    }

    /// Stats each source's name under `dest_path` and reports the ones already
    /// taken, with both sides' size, mtime, and kind for the conflict dialog.
    pub(super) fn scan_for_conflicts_impl<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        let dest_abs = self.resolve(dest_path);
        let source_items: Vec<SourceItemInfo> = source_items.to_vec();
        Box::pin(async move {
            spawn_blocking(move || {
                let mut conflicts = Vec::new();

                for item in &source_items {
                    let dest_file_path = dest_abs.join(&item.name);
                    if dest_file_path.exists()
                        && let Ok(meta) = std::fs::metadata(&dest_file_path)
                    {
                        let dest_modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64));

                        conflicts.push(ScanConflict {
                            source_path: item.name.clone(),
                            dest_path: dest_file_path.to_string_lossy().to_string(),
                            source_size: item.size,
                            dest_size: meta.len(),
                            source_modified: item.modified,
                            dest_modified,
                            source_is_directory: item.is_directory,
                            dest_is_directory: meta.is_dir(),
                        });
                    }
                }

                Ok(conflicts)
            })
            .await
            .expect("spawn_blocking scan_for_conflicts closure doesn't panic and the task is uncancelable")
        })
    }
}
