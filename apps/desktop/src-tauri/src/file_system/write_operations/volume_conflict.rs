//! Conflict resolution for volume-to-volume copy operations.
//!
//! Handles what to do when a destination file already exists:
//! - Stop: Emit conflict event, wait for user input via oneshot channel
//! - Skip: Return None to skip this file
//! - Overwrite: Delete existing, return same path
//! - Rename: Find unique name like "file (1).txt"

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::state::WriteOperationState;
use super::types::{ConflictResolution, OperationEventSink, VolumeCopyConfig, WriteConflictEvent, WriteOperationError};
use crate::file_system::volume::Volume;

/// Resolves a file conflict for volume-to-volume copy.
/// Returns None if file should be skipped, or Some(path) with the resolved destination path.
#[allow(
    clippy::too_many_arguments,
    reason = "Conflict resolution requires many context parameters"
)]
pub(super) async fn resolve_volume_conflict(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
) -> Result<Option<PathBuf>, WriteOperationError> {
    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_resolution {
        // Use saved "apply to all" resolution
        *saved_resolution
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Need to prompt user - gather metadata for the conflict event
            let source_scan = source_volume.scan_for_copy(source_path).await.ok();
            let source_size = source_scan.as_ref().map(|s| s.total_bytes).unwrap_or(0);

            // Try to get destination size by scanning (best effort)
            let dest_size = dest_volume
                .scan_for_copy(dest_path)
                .await
                .ok()
                .map(|s| s.total_bytes)
                .unwrap_or(0);

            // We can't easily get modification times from Volume trait, so use None
            let source_modified: Option<i64> = None;
            let destination_modified: Option<i64> = None;
            let destination_is_newer = false;
            let size_difference = dest_size as i64 - source_size as i64;

            events.emit_conflict(WriteConflictEvent {
                operation_id: operation_id.to_string(),
                source_path: source_path.display().to_string(),
                destination_path: dest_path.display().to_string(),
                source_size,
                destination_size: dest_size,
                source_modified,
                destination_modified,
                destination_is_newer,
                size_difference,
            });

            // Create a oneshot channel for this conflict resolution
            let (tx, rx) = tokio::sync::oneshot::channel();
            *state.conflict_resolution_tx.lock().unwrap() = Some(tx);

            // Wait for user to call resolve_write_conflict.
            match rx.await {
                Ok(response) => {
                    // Save for future conflicts if apply_to_all
                    if response.apply_to_all {
                        *apply_to_all_resolution = Some(response.resolution);
                    }
                    // Apply the chosen resolution
                    apply_volume_conflict_resolution(response.resolution, dest_volume, dest_path).await
                }
                Err(_) => {
                    // Sender dropped = operation cancelled
                    Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    })
                }
            }
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            apply_volume_conflict_resolution(ConflictResolution::Overwrite, dest_volume, dest_path).await
        }
        ConflictResolution::Rename => {
            apply_volume_conflict_resolution(ConflictResolution::Rename, dest_volume, dest_path).await
        }
    }
}

/// Applies a specific conflict resolution for volume copy.
/// Returns None for Skip, or Some(path) with the path to write to.
async fn apply_volume_conflict_resolution(
    resolution: ConflictResolution,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
) -> Result<Option<PathBuf>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Cmdr's UX promise is "Overwrite means merge for dirs, replace for files":
            //
            // - For files: delete the dest first so the streaming writer lands a fresh
            //   copy. Same-named files in dest get genuinely replaced, byte-for-byte.
            // - For directories: SKIP the delete entirely. The recursive copy will
            //   merge into the existing tree — same-named files inside get overwritten
            //   by the streaming writers, files in dest that aren't in source are
            //   preserved.
            //
            // The dir branch is enforced HERE rather than relying on `Volume::delete`'s
            // "file or empty directory" trait contract. Today every backend honors
            // that contract (delete of a non-empty dir fails benignly), but a future
            // backend with recursive delete semantics — or a refactor that consolidates
            // delete + delete_recursive — would silently flip the UX from merge to
            // wholesale replace, deleting files unique to dest. That's a data-loss
            // footgun. Stat-and-skip makes the merge guarantee architectural, not
            // emergent. See `dir_overwrite_must_merge_not_replace_even_with_recursive_delete`
            // in the test module — it pins this with a wrapper Volume that violates
            // the contract.
            let is_dir = dest_volume.is_directory(dest_path).await.unwrap_or(false);
            if !is_dir
                && let Err(e) = dest_volume.delete(dest_path).await {
                    log::warn!(
                        "apply_volume_conflict_resolution(Overwrite): delete of file {} failed: {}",
                        dest_path.display(),
                        e
                    );
                    // Continue — the streaming writer might still succeed if the failure
                    // was transient.
                }
            Ok(Some(dest_path.to_path_buf()))
        }
        ConflictResolution::Rename => {
            // Find a unique name - we need to check what exists on the volume
            let unique_path = find_unique_volume_name(dest_volume, dest_path).await;
            Ok(Some(unique_path))
        }
    }
}

/// Finds a unique filename on a volume by appending " (1)", " (2)", etc.
async fn find_unique_volume_name(dest_volume: &Arc<dyn Volume>, path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);
        if !dest_volume.exists(&new_path).await {
            return new_path;
        }
        counter += 1;

        // Safety limit to prevent infinite loop
        if counter > 1000 {
            // Just return with counter - extremely unlikely to happen
            let new_name = match &extension {
                Some(ext) => format!("{} ({}).{}", stem, counter, ext),
                None => format!("{} ({})", stem, counter),
            };
            return parent.join(new_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::FileEntry;
    use crate::file_system::volume::{InMemoryVolume, VolumeError};
    use std::pin::Pin;
    use std::sync::Arc;

    /// Wraps an `InMemoryVolume` but makes `delete` recursive — simulates a future
    /// backend (or refactor) that doesn't honor the trait's "file or empty directory"
    /// contract.
    ///
    /// Used to assert that `apply_volume_conflict_resolution(Overwrite)` produces a
    /// merge UX even when the underlying delete is recursive. If this volume's
    /// `delete` ever runs against a non-empty `dest_path`, the test below catches
    /// it because files unique to the dest tree disappear.
    struct RecursiveDeleteVolume {
        inner: Arc<InMemoryVolume>,
    }

    impl Volume for RecursiveDeleteVolume {
        fn name(&self) -> &str {
            self.inner.name()
        }
        fn root(&self) -> &Path {
            self.inner.root()
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            self.inner.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            self.inner.get_metadata(path)
        }
        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
        fn is_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            self.inner.is_directory(path)
        }
        /// Recursive delete — contractually wrong, but plausible for some backends.
        fn delete<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
            Box::pin(async move {
                if self.inner.is_directory(path).await.unwrap_or(false) {
                    let entries = self.inner.list_directory(path, None).await?;
                    for entry in entries {
                        let child = PathBuf::from(&entry.path);
                        // Recurse — child might also be a non-empty directory.
                        Box::pin(self.delete(&child)).await.ok();
                    }
                }
                self.inner.delete(path).await
            })
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dir_overwrite_must_merge_not_replace_even_with_recursive_delete() {
        // Build a dest dir with two files: one will conflict with the source,
        // one is unique to dest (`keep-me.jpg`) and MUST survive merge.
        let inner = Arc::new(InMemoryVolume::new("dest"));
        inner.create_directory(Path::new("/photos")).await.unwrap();
        inner
            .create_file(Path::new("/photos/keep-me.jpg"), b"existing")
            .await
            .unwrap();
        inner
            .create_file(Path::new("/photos/will-conflict.jpg"), b"old")
            .await
            .unwrap();

        // Wrap so `delete` is recursive — the dangerous future-backend scenario.
        let dest_recursive: Arc<dyn Volume> = Arc::new(RecursiveDeleteVolume {
            inner: Arc::clone(&inner),
        });

        // Resolve an Overwrite conflict for `/photos`.
        let result =
            apply_volume_conflict_resolution(ConflictResolution::Overwrite, &dest_recursive, Path::new("/photos"))
                .await
                .unwrap();

        // The resolver should hand back the same path (caller will write to it).
        assert_eq!(result, Some(PathBuf::from("/photos")));

        // CRITICAL: files unique to dest must still be there. If this fails, the
        // resolver wholesale-deleted the dest tree — Cmdr's "Overwrite means merge
        // for dirs" UX has silently flipped to "Overwrite means replace", and any
        // file in dest that isn't in source is gone.
        assert!(
            inner.exists(Path::new("/photos/keep-me.jpg")).await,
            "Overwrite resolution must NOT recursively delete the dest directory. \
             Cmdr's UX promise is merge-not-replace for dirs; if this fails, users \
             will lose files that exist in dest but not in source."
        );

        // Also check the dir itself is intact (not a `delete` retry surprise).
        assert!(
            inner.exists(Path::new("/photos")).await,
            "Dest directory itself must remain — the recursive copy needs it as a merge target."
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn file_overwrite_still_deletes_the_existing_file() {
        // Sanity: for files (not dirs), Overwrite SHOULD delete first so the
        // recursive writer lands a fresh copy. The dir-merge guarantee must
        // not regress this case.
        let dest = Arc::new(InMemoryVolume::new("dest"));
        dest.create_file(Path::new("/notes.txt"), b"old content").await.unwrap();
        let dest_dyn: Arc<dyn Volume> = dest.clone();

        let result =
            apply_volume_conflict_resolution(ConflictResolution::Overwrite, &dest_dyn, Path::new("/notes.txt"))
                .await
                .unwrap();

        assert_eq!(result, Some(PathBuf::from("/notes.txt")));
        // After resolution, before the recursive copy runs, the file should be gone
        // (so the writer creates a fresh one rather than appending or failing).
        assert!(
            !dest.exists(Path::new("/notes.txt")).await,
            "Overwrite resolution must delete an existing FILE so the writer can \
             create a fresh copy. Skipping delete only applies to directories."
        );
    }
}
