//! Conflict resolution for volume-to-volume copy operations.
//!
//! Handles what to do when a destination file already exists:
//! - Stop: Emit conflict event, wait for user input via oneshot channel
//! - Skip: Return None to skip this file
//! - Overwrite: Delete existing, return same path
//! - Rename: Find unique name like "file (1).txt"

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::helpers::{ApplyToAll, apply_to_all_effective, apply_to_all_record};
use super::super::state::WriteOperationState;
use super::super::types::{
    ConflictResolution, OperationEventSink, VolumeCopyConfig, WriteConflictEvent, WriteOperationError,
};
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
    apply_to_all_resolution: &mut ApplyToAll,
    // Size hints for the conflict dialog. `Some` skips a `scan_for_copy` call
    // on that side. The copy path already has both: source size in
    // `source_hints` (from the cached preview scan), dest size in `dest_meta`
    // from the stat just done by the caller. Without these hints, an MTP
    // source means listing the parent directory of `source_path` to find one
    // entry's size — 18 s for /DCIM/Camera with 1046 photos when the listing
    // cache has lapsed. The move path doesn't have a scan phase, so it still
    // falls through to `scan_for_copy` for unknown hints.
    source_size_hint: Option<u64>,
    dest_size_hint: Option<u64>,
) -> Result<Option<PathBuf>, WriteOperationError> {
    // Classify the clash up front so the two-bucket lookup and store stay
    // consistent. `is_directory` errors fall back to `false`, same as the
    // dialog-side default — we'd rather over-prompt than route an unknown
    // clash into the destructive file→folder latch.
    let source_is_directory = source_volume.is_directory(source_path).await.unwrap_or(false);
    let destination_is_directory = dest_volume.is_directory(dest_path).await.unwrap_or(false);
    let is_file_to_folder = !source_is_directory && destination_is_directory;

    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_effective(apply_to_all_resolution, is_file_to_folder)
    {
        // Use saved "apply to all" resolution
        saved_resolution
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Need to prompt user - gather metadata for the conflict event.
            // Source size: the pre-flight scan hint is authoritative for both
            // file and folder sources. Fall back to 0 when the source is a
            // file with no hint (rare MCP / skip-preflight path); folders
            // without a hint shouldn't happen post-preflight.
            let source_size = source_size_hint.unwrap_or(0);

            // Destination size: prefer the hint (already populated from the
            // caller's stat for file destinations). For folder destinations
            // the hint is `None` because the volume layer never walks the
            // remote tree — leave it `None` so the FE renders "(unknown)".
            let dest_size: Option<u64> = if destination_is_directory {
                dest_size_hint
            } else {
                Some(dest_size_hint.unwrap_or(0))
            };

            // Pull mtimes via `get_metadata` so the per-file conflict dialog
            // can render its "(newer)" / "(older)" annotations on volume copies
            // (MTP, SMB) the same way it does on local-FS. Both sides may
            // legitimately return `None` (SMB servers vary on `modified_at`);
            // we surface that as `None` and the FE simply omits the annotation.
            //
            // Fired only on the Stop path (user-prompted), so the extra two
            // round-trips never run for Skip / Overwrite / Rename / conditional
            // policies. Each is bounded by the time the user takes to click,
            // so the cost is invisible.
            let source_modified: Option<i64> = source_volume
                .get_metadata(source_path)
                .await
                .ok()
                .and_then(|m| m.modified_at)
                .map(|s| s as i64);
            let destination_modified: Option<i64> = dest_volume
                .get_metadata(dest_path)
                .await
                .ok()
                .and_then(|m| m.modified_at)
                .map(|s| s as i64);
            let destination_is_newer = matches!((source_modified, destination_modified), (Some(s), Some(d)) if d > s);
            let size_difference = dest_size.map(|d| d as i64 - source_size as i64);

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
                source_is_directory,
                destination_is_directory,
            });

            // Create a oneshot channel for this conflict resolution
            let (tx, rx) = tokio::sync::oneshot::channel();
            *state.conflict_resolution_tx.lock().unwrap() = Some(tx);

            // Wait for user to call resolve_write_conflict.
            match rx.await {
                Ok(response) => {
                    // Save the original (unreduced) variant under the right bucket so
                    // subsequent clashes re-evaluate the conditional variants against
                    // their own metadata. `apply_to_all_record` also flips the
                    // "first-clash" flag whether or not the user picked an apply-to-all
                    // option, so a later file→folder "* all" choice won't be considered
                    // "first" if a regular clash happened earlier in this op.
                    apply_to_all_record(
                        apply_to_all_resolution,
                        is_file_to_folder,
                        response.resolution,
                        response.apply_to_all,
                    );
                    let effective = reduce_volume_conditional_resolution(
                        response.resolution,
                        source_volume,
                        source_path,
                        dest_volume,
                        dest_path,
                        Some(source_size),
                        dest_size,
                    )
                    .await;
                    apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory).await
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
            apply_volume_conflict_resolution(
                ConflictResolution::Overwrite,
                dest_volume,
                dest_path,
                source_is_directory,
            )
            .await
        }
        ConflictResolution::Rename => {
            apply_volume_conflict_resolution(ConflictResolution::Rename, dest_volume, dest_path, source_is_directory)
                .await
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            let effective = reduce_volume_conditional_resolution(
                resolution,
                source_volume,
                source_path,
                dest_volume,
                dest_path,
                source_size_hint,
                dest_size_hint,
            )
            .await;
            apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory).await
        }
    }
}

/// Volume-side counterpart of `reduce_conditional_resolution`. Maps the
/// conditional variants to `Overwrite` / `Skip` by comparing source vs dest
/// sizes (cheap: hints from the caller or one `get_metadata` round-trip each)
/// or `modified_at` timestamps (`get_metadata` on both sides).
///
/// Strict comparison: equal sizes / equal mtimes / unknown values all reduce
/// to `Skip`. Volume backends may not always populate `modified_at` (SMB
/// servers vary, MTP usually does); in that case `OverwriteOlder` skips,
/// which is the safe default.
async fn reduce_volume_conditional_resolution(
    resolution: ConflictResolution,
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    source_size_hint: Option<u64>,
    dest_size_hint: Option<u64>,
) -> ConflictResolution {
    match resolution {
        ConflictResolution::OverwriteSmaller => {
            let src_size = match source_size_hint {
                Some(s) => Some(s),
                None => source_volume.get_metadata(source_path).await.ok().and_then(|m| m.size),
            };
            let dst_size = match dest_size_hint {
                Some(s) => Some(s),
                None => dest_volume.get_metadata(dest_path).await.ok().and_then(|m| m.size),
            };
            match (src_size, dst_size) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(src), Some(dst)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller (volume): skipping {} — destination not strictly smaller (src={src}, dst={dst})",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller (volume): skipping {} — size unknown for source or destination (the volume backend may not surface it)",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
            }
        }
        ConflictResolution::OverwriteOlder => {
            let src_t = source_volume
                .get_metadata(source_path)
                .await
                .ok()
                .and_then(|m| m.modified_at);
            let dst_t = dest_volume
                .get_metadata(dest_path)
                .await
                .ok()
                .and_then(|m| m.modified_at);
            match (src_t, dst_t) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(_), Some(_)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder (volume): skipping {} — destination not strictly older than source",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder (volume): skipping {} — modified time unknown for source or destination (some SMB servers don't surface it)",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
            }
        }
        other => other,
    }
}

/// Applies a specific conflict resolution for volume copy.
/// Returns None for Skip, or Some(path) with the path to write to.
async fn apply_volume_conflict_resolution(
    resolution: ConflictResolution,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    source_is_directory: bool,
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
            // - For files: delete the dest first so the streaming writer lands a fresh copy. Same-named files
            //   in dest get genuinely replaced, byte-for-byte.
            // - For directories (same type): SKIP the delete entirely. The recursive copy merges into the existing
            //   tree; same-named files inside get overwritten by the streaming writers, files in dest that
            //   aren't in source are preserved.
            // - For cross-type clashes (file→folder or folder→file): the dest type is wrong, so we
            //   must delete it before the source materializes. There's no volume-level temp+rename
            //   atomicity (cross-backend), so a recursive delete is the best we can do; backends
            //   that support it (LocalPosix, MTP, SMB) handle the delete safely under their own
            //   semantics.
            //
            // The same-type dir branch is enforced HERE rather than relying on `Volume::delete`'s
            // "file or empty directory" trait contract. Today every backend honors
            // that contract (delete of a non-empty dir fails benignly), but a future
            // backend with recursive delete semantics, or a refactor that consolidates
            // delete + delete_recursive, would silently flip the UX from merge to
            // wholesale replace, deleting files unique to dest. That's a data-loss
            // footgun. Stat-and-skip makes the merge guarantee architectural, not
            // emergent. See `dir_overwrite_must_merge_not_replace_even_with_recursive_delete`
            // in the test module; it pins this with a wrapper Volume that violates
            // the contract.
            let dest_is_dir = dest_volume.is_directory(dest_path).await.unwrap_or(false);
            let same_type_dir = dest_is_dir && source_is_directory;
            if !same_type_dir {
                // Cross-type or both-files: clear the dest first.
                if dest_is_dir {
                    // File→folder overwrite: recursively delete the dest folder.
                    if let Err(e) = super::volume_copy::delete_volume_path_recursive(dest_volume, dest_path).await {
                        log::warn!(
                            "apply_volume_conflict_resolution(Overwrite): recursive delete of folder {} failed: {}",
                            dest_path.display(),
                            e
                        );
                    }
                } else if let Err(e) = dest_volume.delete(dest_path).await {
                    log::warn!(
                        "apply_volume_conflict_resolution(Overwrite): delete of file {} failed: {}",
                        dest_path.display(),
                        e
                    );
                    // Continue: the streaming writer might still succeed if the failure
                    // was transient.
                }
            }
            Ok(Some(dest_path.to_path_buf()))
        }
        ConflictResolution::Rename => {
            // Find a unique name - we need to check what exists on the volume
            let unique_path = find_unique_volume_name(dest_volume, dest_path).await;
            Ok(Some(unique_path))
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            // Reduced to Overwrite / Skip by `reduce_volume_conditional_resolution`
            // before reaching this function.
            unreachable!("conditional conflict resolutions must be reduced before apply_volume_conflict_resolution")
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

    /// Wraps an `InMemoryVolume` but makes `delete` recursive, simulating a future
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
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
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
        /// Recursive delete: contractually wrong, but plausible for some backends.
        fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
            Box::pin(async move {
                if self.inner.is_directory(path).await.unwrap_or(false) {
                    let entries = self.inner.list_directory(path, None).await?;
                    for entry in entries {
                        let child = PathBuf::from(&entry.path);
                        // Recurse: child might also be a non-empty directory.
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

        // Wrap so `delete` is recursive: the dangerous future-backend scenario.
        let dest_recursive: Arc<dyn Volume> = Arc::new(RecursiveDeleteVolume {
            inner: Arc::clone(&inner),
        });

        // Resolve an Overwrite conflict for `/photos` (source is also a directory).
        let result = apply_volume_conflict_resolution(
            ConflictResolution::Overwrite,
            &dest_recursive,
            Path::new("/photos"),
            true,
        )
        .await
        .unwrap();

        // The resolver should hand back the same path (caller will write to it).
        assert_eq!(result, Some(PathBuf::from("/photos")));

        // CRITICAL: files unique to dest must still be there. If this fails, the
        // resolver wholesale-deleted the dest tree. Cmdr's "Overwrite means merge
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
            "Dest directory itself must remain; the recursive copy needs it as a merge target."
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
            apply_volume_conflict_resolution(ConflictResolution::Overwrite, &dest_dyn, Path::new("/notes.txt"), false)
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

    // ======================================================================
    // Conditional resolution (OverwriteSmaller / OverwriteOlder)
    // ======================================================================
    //
    // Same data-safety contract as the local-FS path: a destination is
    // overwritten ONLY when strictly smaller / strictly older than the source.
    // The volume side has two extra wrinkles the local side doesn't:
    //   1. Size hints from the caller (preview scan) can short-circuit the `get_metadata` round-trip;
    //      tests cover both hint-provided and hint-absent paths.
    //   2. Volume backends may not surface `modified_at` (SMB servers vary). OverwriteOlder must Skip
    //      rather than overwrite when mtime is unknown on either side.

    /// Build an InMemoryVolume holding a single file at `path` with the given
    /// `size` and `modified_at`. The volume's `get_metadata` will return
    /// exactly these values, letting tests pin the comparison behavior
    /// independent of clock drift.
    fn volume_with_file(name: &str, path: &str, size: u64, modified_at: Option<u64>) -> Arc<InMemoryVolume> {
        let entry = FileEntry {
            size: Some(size),
            modified_at,
            created_at: modified_at,
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            extended_metadata_loaded: true,
            ..FileEntry::new(
                path.rsplit('/').next().unwrap_or(path).to_string(),
                path.to_string(),
                false,
                false,
            )
        };
        Arc::new(InMemoryVolume::with_entries(name, vec![entry]))
    }

    // ----- OverwriteSmaller -----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_smaller_overwrites_when_dest_strictly_smaller_via_hints() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteSmaller,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            Some(1000),
            Some(500),
        )
        .await;

        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_smaller_skips_when_dest_equal_size() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 500, Some(100));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteSmaller,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            Some(500),
            Some(500),
        )
        .await;

        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Equal-size dst must NOT be overwritten on a volume any more than on local FS"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_smaller_skips_when_dest_larger() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(100));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 9999, Some(100));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteSmaller,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            Some(100),
            Some(9999),
        )
        .await;

        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Larger dst must NOT be overwritten — would clobber the user's keeper file"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_smaller_falls_back_to_get_metadata_when_hints_missing() {
        // Critical: when the caller (move path, no scan phase) passes no hints,
        // the reducer must `get_metadata` from each volume rather than
        // defaulting to Skip on absent hints. Otherwise OverwriteSmaller would
        // never actually overwrite on moves.
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteSmaller,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(
            resolved,
            ConflictResolution::Overwrite,
            "With no hints, the reducer should still get_metadata and compare correctly"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_smaller_skips_when_dest_metadata_unavailable() {
        // Source is fine but dest get_metadata fails (path missing). Reducer
        // must Skip — we can't prove the destination is smaller, so we never
        // touch it.
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
        let dst: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dst")); // empty

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteSmaller,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(resolved, ConflictResolution::Skip);
    }

    // ----- OverwriteOlder -----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_older_overwrites_when_dest_strictly_older() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteOlder,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_older_skips_when_dest_equal_mtime() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteOlder,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(resolved, ConflictResolution::Skip);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_older_skips_when_dest_strictly_newer() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_700_000_000));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteOlder,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Newer dst must NOT be overwritten — would clobber the user's fresher file"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_older_skips_when_source_mtime_unknown() {
        // Many SMB servers don't surface modified_at reliably. The reducer
        // must fail closed to Skip rather than defaulting to overwrite.
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, None);
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteOlder,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Unknown source mtime must fail closed; we cannot prove dst is older"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_older_skips_when_dest_mtime_unknown() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, None);

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteOlder,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Unknown dest mtime must fail closed; we cannot prove it's older"
        );
    }

    // ----- Pass-through -----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_non_conditional_variants_pass_through_unchanged() {
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

        for v in [
            ConflictResolution::Stop,
            ConflictResolution::Skip,
            ConflictResolution::Overwrite,
            ConflictResolution::Rename,
        ] {
            let resolved = reduce_volume_conditional_resolution(
                v,
                &src,
                Path::new("/f.bin"),
                &dst,
                Path::new("/f.bin"),
                Some(100),
                Some(100),
            )
            .await;
            assert_eq!(resolved, v, "Variant {v:?} must pass through unchanged");
        }
    }

    // ----- Axis independence -----

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_smaller_ignores_mtime() {
        // Smaller AND newer dst: still overwrite under OverwriteSmaller.
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(1_600_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_700_000_000));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteSmaller,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            Some(1000),
            Some(100),
        )
        .await;

        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn volume_older_ignores_size() {
        // Older AND larger dst: still overwrite under OverwriteOlder.
        let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
        let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 9999, Some(1_600_000_000));

        let resolved = reduce_volume_conditional_resolution(
            ConflictResolution::OverwriteOlder,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            None,
            None,
        )
        .await;

        assert_eq!(resolved, ConflictResolution::Overwrite);
    }
}
