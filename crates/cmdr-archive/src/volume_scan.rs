//! The copy-scan family's inherent bodies, which the `scan_for_copy*` trait
//! methods in `volume.rs` delegate to.
//!
//! Split out the way the SMB and local backends split their `scan.rs`: sizing a
//! subtree from the parsed index, and handing a transfer's tree every entry of
//! it, is one subject rather than one more `Volume` method.

use std::path::{Path, PathBuf};

use cmdr_fs::mac_only_files::MacOnlyFiles;
use cmdr_fs::volume::{BatchScanResult, CopyScanResult, ScanBoundary, ScanRecorder, VolumeError, fold_batch};

use super::ArchiveVolume;

impl ArchiveVolume {
    /// One subtree's counts and bytes, from the central directory: no
    /// decompression during a scan.
    pub(super) async fn scan_subtree(
        &self,
        path: &Path,
        mac_only: MacOnlyFiles,
    ) -> Result<CopyScanResult, VolumeError> {
        let inner = self.inner_path(path);
        let index = self.index().await?;
        let node = index
            .get(&inner)
            .ok_or_else(|| VolumeError::NotFound(self.archive_path.join(&inner).display().to_string()))?;
        let top_level_modified_at = node.modified.and_then(|secs| u64::try_from(secs).ok());

        // A single file: one entry, its uncompressed size.
        if !node.is_dir {
            let size = node.size.unwrap_or(0);
            return Ok(CopyScanResult {
                file_count: 1,
                dir_count: 0,
                total_bytes: size,
                // No hardlinks in a zip: the two footprints are equal.
                dedup_bytes: size,
                top_level_is_directory: false,
                top_level_modified_at,
                top_level_created_at: None,
            });
        }

        // A directory: walk the subtree via the index's per-dir child lists. The
        // top-level dir isn't counted (matches `LocalPosixVolume`).
        let mut file_count = 0;
        let mut dir_count = 0;
        let mut total_bytes = 0u64;
        let mut pending = vec![inner];
        while let Some(dir) = pending.pop() {
            let Some(children) = index.list(&dir) else { continue };
            for child in children {
                if mac_only.leaves_out_str(&child.name, child.is_dir) {
                    continue;
                }
                if child.is_dir {
                    dir_count += 1;
                    pending.push(child.path);
                } else {
                    file_count += 1;
                    total_bytes += child.size.unwrap_or(0);
                }
            }
        }
        Ok(CopyScanResult {
            file_count,
            dir_count,
            total_bytes,
            dedup_bytes: total_bytes,
            top_level_is_directory: true,
            top_level_modified_at,
            top_level_created_at: None,
        })
    }

    /// The trait's per-path default, plus every entry handed to the boundary's
    /// recorder from the same in-memory index, so a transfer out of an archive
    /// draws the selection as a full tree. The counts are the default's.
    pub(super) async fn scan_batch(
        &self,
        paths: &[PathBuf],
        boundary: &ScanBoundary<'_>,
    ) -> Result<BatchScanResult, VolumeError> {
        let mut per_path = Vec::with_capacity(paths.len());
        for path in paths {
            boundary.check().await?;
            let scan = self.scan_subtree(path, boundary.mac_only()).await?;
            if let Some(recorder) = boundary.recorder() {
                self.record_subtree(path, recorder.as_ref(), boundary.mac_only())
                    .await?;
            }
            boundary.counted(&scan).await?;
            per_path.push((path.clone(), scan));
        }
        let mut batch = fold_batch(per_path);
        // The default's aggregate, which never claims a type for the batch.
        batch.aggregate.top_level_is_directory = false;
        batch.aggregate.top_level_modified_at = None;
        batch.aggregate.top_level_created_at = None;
        Ok(batch)
    }

    /// Hands `recorder` the entry at `path` and every one below it, named under
    /// `path` as the caller spelled it.
    async fn record_subtree(
        &self,
        path: &Path,
        recorder: &dyn ScanRecorder,
        mac_only: MacOnlyFiles,
    ) -> Result<(), VolumeError> {
        let index = self.index().await?;
        let inner = self.inner_path(path);
        let Some(node) = index.get(&inner) else {
            return Ok(());
        };
        if !node.is_dir {
            recorder.file(path, node.size.unwrap_or(0));
            return Ok(());
        }
        recorder.dir(path);
        let mut pending = vec![(inner, path.to_path_buf())];
        while let Some((dir, as_called)) = pending.pop() {
            let Some(children) = index.list(&dir) else { continue };
            for child in children {
                if mac_only.leaves_out_str(&child.name, child.is_dir) {
                    continue;
                }
                let child_as_called = as_called.join(&child.name);
                if child.is_dir {
                    recorder.dir(&child_as_called);
                    pending.push((child.path, child_as_called));
                } else {
                    recorder.file(&child_as_called, child.size.unwrap_or(0));
                }
            }
        }
        Ok(())
    }
}
