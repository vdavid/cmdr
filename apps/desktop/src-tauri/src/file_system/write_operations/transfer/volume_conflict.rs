//! Conflict resolution for volume-to-volume copy operations.
//!
//! Handles what to do when a destination file already exists:
//! - Stop: Emit conflict event, wait for user input via oneshot channel
//! - Skip: Return None to skip this file
//! - Overwrite (file→file): safe-replace — write into a temp sibling, then
//!   delete the original and rename the temp in (`finalize_safe_replace`), so a
//!   mid-stream failure can't lose both the old and the new copy
//! - Overwrite (dir→dir): merge into the existing tree (no delete)
//! - Overwrite (cross-type): delete the dest first, then write
//! - Rename: Find unique name like "file (1).txt"

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::helpers::{ApplyToAll, apply_to_all_effective, apply_to_all_record};
use super::super::state::WriteOperationState;
use super::super::types::{
    ConflictResolution, OperationEventSink, VolumeCopyConfig, WriteConflictEvent, WriteOperationError,
};
use crate::file_system::volume::{Volume, VolumeError};

/// Outcome of resolving a volume conflict.
///
/// The caller writes streaming bytes to `write_path`. When `replace_after_write`
/// is `Some(orig)`, `write_path` is a temp sibling on the destination volume:
/// after the streaming write fully succeeds, the caller must call
/// [`finalize_safe_replace`] to delete `orig` (which survived the whole write)
/// and rename `write_path` → `orig`. When `replace_after_write` is `None`,
/// `write_path` is the final destination and the caller writes directly.
pub(super) struct ResolvedConflict {
    /// Where the streaming writer should land bytes.
    pub write_path: PathBuf,
    /// `Some(orig)` ⇒ `write_path` is a temp sibling; after a successful write the
    /// caller must delete `orig` (it survived the full write) then rename
    /// `write_path` → `orig`. `None` ⇒ `write_path` is final, write directly.
    pub replace_after_write: Option<PathBuf>,
}

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
) -> Result<Option<ResolvedConflict>, WriteOperationError> {
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
/// Returns `None` for Skip, or `Some(ResolvedConflict)` describing where to
/// write and whether a post-write safe-replace finalize is needed.
async fn apply_volume_conflict_resolution(
    resolution: ConflictResolution,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    source_is_directory: bool,
) -> Result<Option<ResolvedConflict>, WriteOperationError> {
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
            // - For files (file→file): SAFE-REPLACE. Stream into a temp sibling on the dest volume
            //   and return `replace_after_write: Some(dest_path)`. The original survives the entire
            //   write; only after the temp is fully written does the caller delete the original and
            //   rename the temp into place (see `finalize_safe_replace`). A mid-stream failure
            //   (network drop, USB yank, cancel) leaves the original intact — we never lose both the
            //   old and the new copy. DO NOT delete the dest here.
            // - For directories (same type): SKIP the delete entirely. The recursive copy merges into
            //   the existing tree; same-named files inside get overwritten by the streaming writers,
            //   files in dest that aren't in source are preserved.
            // - For cross-type clashes (file→folder or folder→file): the dest type is wrong, so we
            //   must delete it before the source materializes. There's no volume-level temp+rename
            //   atomicity (cross-backend) for a type swap, so a recursive delete is the best we can
            //   do; backends that support it (LocalPosix, MTP, SMB) handle the delete safely under
            //   their own semantics. These are rare and lower-stakes (a type mismatch already means
            //   the dest content is being intentionally replaced wholesale).
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

            if !dest_is_dir && !source_is_directory {
                // file→file: safe-replace via a temp sibling. No delete here.
                let temp = temp_sibling_path(dest_path);
                return Ok(Some(ResolvedConflict {
                    write_path: temp,
                    replace_after_write: Some(dest_path.to_path_buf()),
                }));
            }

            let same_type_dir = dest_is_dir && source_is_directory;
            if !same_type_dir {
                // Cross-type (file→folder or folder→file): clear the dest first.
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
            Ok(Some(ResolvedConflict {
                write_path: dest_path.to_path_buf(),
                replace_after_write: None,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name - we need to check what exists on the volume
            let unique_path = find_unique_volume_name(dest_volume, dest_path).await;
            Ok(Some(ResolvedConflict {
                write_path: unique_path,
                replace_after_write: None,
            }))
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            // Reduced to Overwrite / Skip by `reduce_volume_conditional_resolution`
            // before reaching this function.
            unreachable!("conditional conflict resolutions must be reduced before apply_volume_conflict_resolution")
        }
    }
}

/// Builds a temp sibling path next to `dest_path` for the safe-replace write.
///
/// Uses the recognizable `.cmdr-tmp-<uuid>` marker (matches the project's temp
/// convention, so a leftover after a crash is identifiable and cleanup helpers
/// recognize it). The temp lives in the same parent directory as the original
/// so the finalize step's `rename` stays within one directory (no cross-dir
/// rename, which some backends refuse).
fn temp_sibling_path(dest_path: &Path) -> PathBuf {
    let parent = dest_path.parent().unwrap_or(Path::new(""));
    let filename = dest_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    parent.join(format!("{filename}.cmdr-tmp-{}", uuid::Uuid::new_v4()))
}

/// Finalizes a file→file safe-replace: deletes the original `orig` (which
/// survived the entire streaming write) and renames the fully-written temp into
/// its place.
///
/// Order matters and is the whole point of safe-replace: the temp holds the
/// COMPLETE new data the moment this is called, and `orig` still holds the
/// complete old data. We delete `orig` first, then `rename(temp, orig, false)`
/// into the now-absent slot. We do NOT use `rename(force=true)` to replace:
/// MTP's `rename(force=true)` does NOT delete an existing destination (it can
/// create a duplicate), so an explicit delete-then-rename is the only shape
/// that's correct and uniform across Local / SMB / MTP / InMemory.
///
/// There is a tiny window between the delete and the rename where neither name
/// resolves to a file on disk — but the complete new data lives in `temp`
/// throughout, so a crash in that window leaves a recoverable `.cmdr-tmp-*`
/// sibling rather than data loss. We tolerate `NotFound` on the delete (the
/// original may have vanished out from under us). If the delete fails for any
/// other reason we return the error WITHOUT deleting the temp — the new data
/// must survive so the user (or a retry) can recover it.
///
/// CALLER CONTRACT: when this returns `Err` (either the delete failed, or — the
/// nastier case — the delete SUCCEEDED and the rename failed), `temp` holds the
/// only complete copy of the new data and the original may already be gone. The
/// caller MUST NOT delete `temp` on this error path: leaving it as a recoverable
/// `.cmdr-tmp-*` artifact is the safe outcome; cleaning it would be total data
/// loss. The three write sites enforce this by stopping their partial-cleanup
/// tracking from designating the temp the moment the streaming write succeeded,
/// before this function runs. See `transfer/CLAUDE.md` § "The post-write temp is
/// committed data" and the `*_preserves_new_data_on_finalize_failure` tests.
pub(super) async fn finalize_safe_replace(
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    orig: &Path,
) -> Result<(), VolumeError> {
    match dest_volume.delete(orig).await {
        Ok(()) => {}
        Err(VolumeError::NotFound(_)) => {
            // Already gone; the rename below will land the new data anyway.
        }
        Err(e) => {
            log::warn!(
                "finalize_safe_replace: failed to delete original {} before rename (temp {} holds the complete new data and is preserved): {}",
                orig.display(),
                temp.display(),
                e
            );
            return Err(e);
        }
    }
    dest_volume.rename(temp, orig, false).await
}

/// Finds a unique filename on a volume by appending " (1)", " (2)", etc.
///
/// On a **local-FS-backed** destination volume (`local_path().is_some()`) the
/// chosen name is atomically RESERVED with an `O_CREAT|O_EXCL` placeholder, the
/// same TOCTOU guard `helpers::find_unique_name` uses for the local-FS copy
/// path. Without it, a concurrent writer (a second Cmdr op, a cloud-sync agent,
/// a backup tool) could land a real file at `name (N)` between our non-atomic
/// `exists()` probe and the streaming writer's create+truncate, and the copy
/// would silently clobber it. The streaming write then lands ON the placeholder
/// (the write site opens the dest with create+truncate), exactly like the
/// local-FS path's `needs_safe_overwrite` flow. The returned path is the volume
/// path; the placeholder is created at the resolved local path.
///
/// On backends without exclusive-create semantics (MTP / SMB / InMemory,
/// `local_path()` is `None`) we can't reserve, so we fall back to the
/// `exists()` probe and re-check existence immediately before returning to keep
/// the residual window as narrow as the backend allows.
async fn find_unique_volume_name(dest_volume: &Arc<dyn Volume>, path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let local_root = dest_volume.local_path();
    let build_name = |counter: u32| -> PathBuf {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        parent.join(new_name)
    };

    let mut counter: u32 = 1;
    loop {
        let new_path = build_name(counter);

        if let Some(root) = &local_root {
            // Local-FS dest: reserve the name with an O_CREAT|O_EXCL placeholder
            // so no concurrent writer can sneak a file in before our write lands.
            let local_path = resolve_local_path(root, &new_path);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&local_path)
            {
                Ok(_) => return new_path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    counter = counter.saturating_add(1);
                }
                Err(_) => {
                    // Anything else (parent unwritable, ENOSPC, …) leaks back to
                    // the caller's write attempt, which has its own error path.
                    return new_path;
                }
            }
        } else {
            // Non-local backend: best-effort `exists()` probe. Re-check right
            // before returning to keep the residual window as narrow as we can.
            if !dest_volume.exists(&new_path).await {
                return new_path;
            }
            counter = counter.saturating_add(1);
        }

        // Safety limit to prevent infinite loop.
        if counter > 1000 {
            // Extremely unlikely to happen.
            return build_name(counter);
        }
    }
}

/// Resolves a destination-volume path against a local-FS volume root, mirroring
/// `LocalPosixVolume::resolve`: an absolute path already under the root is used
/// as-is; under a `/` root it passes through; otherwise the leading `/` is
/// stripped and the remainder is joined onto the root. Relative paths join
/// directly. This lets the O_EXCL reservation land at the same local path the
/// volume's streaming writer will later resolve `new_path` to.
fn resolve_local_path(root: &Path, path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() || path == Path::new(".") {
        root.to_path_buf()
    } else if path.is_absolute() {
        if path.starts_with(root) || root == Path::new("/") {
            path.to_path_buf()
        } else {
            let relative = path.strip_prefix("/").unwrap_or(path);
            root.join(relative)
        }
    } else {
        root.join(path)
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
        .unwrap()
        .expect("dir→dir Overwrite must resolve to a merge target, not Skip");

        // The resolver should hand back the same path (caller will merge into it)
        // and must NOT request a safe-replace finalize (dirs merge, not replace).
        assert_eq!(result.write_path, PathBuf::from("/photos"));
        assert_eq!(result.replace_after_write, None);

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
    async fn file_overwrite_keeps_original_until_temp_is_written() {
        // For a file→file Overwrite, the resolver must NOT delete the existing
        // destination. Instead it hands back a temp sibling to write into plus
        // `replace_after_write: Some(orig)`, so the original survives the full
        // streaming write and is only swapped out at finalize time. This is the
        // safe-replace contract that protects data on a mid-stream failure.
        let dest = Arc::new(InMemoryVolume::new("dest"));
        dest.create_file(Path::new("/notes.txt"), b"old content").await.unwrap();
        let dest_dyn: Arc<dyn Volume> = dest.clone();

        let resolved =
            apply_volume_conflict_resolution(ConflictResolution::Overwrite, &dest_dyn, Path::new("/notes.txt"), false)
                .await
                .unwrap()
                .expect("file→file Overwrite must resolve to a write path, not Skip");

        // (a) The original MUST still exist after resolution — current code
        // deletes it here, so this assertion is RED against the buggy version.
        assert!(
            dest.exists(Path::new("/notes.txt")).await,
            "Overwrite resolution must NOT delete the existing FILE before the \
             streaming write. The original must survive so a mid-stream failure \
             can't lose both the old and the new copy."
        );

        // (b) The caller is told to replace `/notes.txt` after the write lands.
        assert_eq!(
            resolved.replace_after_write,
            Some(PathBuf::from("/notes.txt")),
            "file→file Overwrite must request a post-write replace of the original"
        );

        // (c) The write lands in a temp sibling, not directly on the original.
        assert_ne!(resolved.write_path, PathBuf::from("/notes.txt"));
        assert_eq!(resolved.write_path.parent(), Path::new("/notes.txt").parent());
        assert!(
            resolved
                .write_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains(".cmdr-tmp-"))
                .unwrap_or(false),
            "temp sibling should carry the recognizable .cmdr-tmp- marker, got {:?}",
            resolved.write_path
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn finalize_safe_replace_swaps_temp_over_original() {
        // After the streaming write lands the new bytes in the temp sibling,
        // `finalize_safe_replace` must delete the original and rename the temp
        // into its place — leaving exactly the new content and no temp behind.
        let dest = Arc::new(InMemoryVolume::new("dest"));
        dest.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
        dest.create_file(Path::new("/notes.txt.cmdr-tmp-abc"), b"NEW")
            .await
            .unwrap();
        let dest_dyn: Arc<dyn Volume> = dest.clone();

        finalize_safe_replace(&dest_dyn, Path::new("/notes.txt.cmdr-tmp-abc"), Path::new("/notes.txt"))
            .await
            .unwrap();

        assert!(!dest.exists(Path::new("/notes.txt.cmdr-tmp-abc")).await);
        let mut stream = dest.open_read_stream(Path::new("/notes.txt")).await.unwrap();
        assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"NEW");
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

    // ======================================================================
    // find_unique_volume_name — TOCTOU reservation on local-FS dest volumes
    // ======================================================================
    //
    // Volume-side sibling of `helpers::find_unique_name`. For a Rename
    // resolution the chosen `name (N)` must be atomically RESERVED with an
    // `O_CREAT|O_EXCL` placeholder when the destination volume is backed by a
    // local filesystem (`local_path().is_some()`), so a concurrent writer
    // (second Cmdr op, cloud-sync agent, backup tool) can't land a file at the
    // same name between our pick and the streaming write. Pre-fix the function
    // only probed `dest_volume.exists()` (non-atomic) and returned the path,
    // leaving a TOCTOU window. Mirrors `helpers.rs::find_unique_name_tests`.

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_fs_rename_reserves_the_chosen_name_on_disk() {
        use crate::file_system::volume::backends::LocalPosixVolume;
        let temp = tempfile::TempDir::new().unwrap();
        let target = temp.path().join("notes.txt");
        std::fs::write(&target, b"original").unwrap();

        let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));

        let unique = find_unique_volume_name(&vol, &target).await;

        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
        // The O_EXCL placeholder must already exist on disk after the call.
        assert!(
            unique.exists(),
            "reservation must create the placeholder on a local-FS dest"
        );
        // A second call escalates to (2), proving the first reservation persisted.
        let next = find_unique_volume_name(&vol, &target).await;
        assert_eq!(next.file_name().unwrap().to_string_lossy(), "notes (2).txt");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_fs_rename_keeps_extension_in_the_right_place() {
        use crate::file_system::volume::backends::LocalPosixVolume;
        let temp = tempfile::TempDir::new().unwrap();
        let target = temp.path().join("report.pdf");
        std::fs::write(&target, b"x").unwrap();

        let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));
        let unique = find_unique_volume_name(&vol, &target).await;
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "report (1).pdf");
        assert!(unique.exists(), "reservation must create the placeholder");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn non_local_dest_does_not_reserve_a_placeholder() {
        // MTP / SMB / InMemory have no exclusive-create semantics here
        // (`local_path()` is `None`), so the function must NOT try to touch the
        // real local FS. It returns the next free name based on `exists()`,
        // accepting the documented narrow residual window.
        let dst = Arc::new(InMemoryVolume::new("dst"));
        dst.create_file(Path::new("/notes.txt"), b"old").await.unwrap();
        let dst_dyn: Arc<dyn Volume> = dst.clone();

        let unique = find_unique_volume_name(&dst_dyn, Path::new("/notes.txt")).await;
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
        // No placeholder was created on the in-memory volume.
        assert!(
            !dst.exists(&unique).await,
            "non-local dest must not pre-create the renamed name"
        );
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
