//! Changing what's on the share: create, delete, rename, and the listing-cache
//! patch each one leaves behind.
//!
//! Every method here has the same two halves. First the SMB round trips, which
//! decide success or failure. Then, only once they succeeded, ONE
//! `notify_mutation` per changed directory, so the panes showing it update
//! without a re-list. ❌ Never call `notify_mutation` per entry: the host walks
//! every cached listing on the volume, so a per-entry caller turns one directory
//! into a quadratic sweep (`crates/cmdr-fs/src/volume/host/DETAILS.md`).

use super::SmbVolume;
use cmdr_fs::volume::{MutationEvent, Volume, VolumeError};
use log::{debug, warn};
use std::path::Path;

impl SmbVolume {
    /// Writes `content` to a file that must not already exist, then tells the
    /// panes about it.
    pub(super) async fn create_file_impl(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let data = content.to_vec();

        debug!(
            "SmbVolume::create_file: share={}, path={:?}",
            self.inner.share_name, smb_path
        );

        {
            let (tree, conn) = self.clone_session().await?;
            // No-clobber contract via the exclusive-create writer
            // (`FileCreate` disposition): if the file already exists the
            // server returns `STATUS_OBJECT_NAME_COLLISION`, which the
            // smb2 crate maps to `ErrorKind::AlreadyExists`. The earlier
            // stat-then-write workaround left a microsecond TOCTOU
            // window; this closes it atomically at the protocol layer.
            let writer_result = tree.create_file_writer_exclusive(conn, &smb_path).await;
            let mut writer = self.handle_smb_result("create_file(open)", &smb_path, writer_result)?;
            if !data.is_empty() {
                let write_result = writer.write_chunk(&data).await;
                self.handle_smb_result("create_file(write_chunk)", &smb_path, write_result)?;
            }
            let finish_result = writer.finish().await;
            self.handle_smb_result("create_file(finish)", &smb_path, finish_result)?;
        }

        self.notify_created(path).await;
        Ok(())
    }

    /// Creates a directory, then tells the panes about it.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::create_directory: share={}, path={:?}",
            self.inner.share_name, smb_path
        );

        {
            let (tree, mut conn) = self.clone_session().await?;
            let result = tree.create_directory(&mut conn, &smb_path).await;
            self.handle_smb_result("create_directory", &smb_path, result)?;
        }

        self.notify_created(path).await;
        Ok(())
    }

    /// Deletes a file or an empty directory, then tells the panes about it.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::delete: share={}, path={:?}",
            self.inner.share_name, smb_path
        );

        // Try delete_file first (one round-trip). If the path is a directory,
        // the server returns STATUS_FILE_IS_A_DIRECTORY; then try delete_directory.
        // This avoids a stat round-trip for every file in bulk deletes.
        let file_result = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.delete_file(&mut conn, &smb_path).await;
            self.handle_smb_result("delete_file", &smb_path, r)
        };

        match file_result {
            Ok(()) => {} // File deleted successfully
            Err(VolumeError::IsADirectory(_)) => {
                // Expected fall-through: path is a directory, retry with delete_directory.
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.delete_directory(&mut conn, &smb_path).await;
                self.handle_smb_result("delete_directory", &smb_path, r)?;
            }
            Err(e) => return Err(e),
        }

        if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && let Some(parent_display) = self.display_path_for(parent)
        {
            self.notify_mutation(
                &self.inner.volume_id,
                &parent_display,
                MutationEvent::Deleted(name.to_string_lossy().to_string()),
            )
            .await;
        }
        Ok(())
    }

    /// Renames or moves an entry, optionally clearing the destination first,
    /// then tells the panes about both ends.
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let smb_from = self.to_smb_path(from)?;
        let smb_to = self.to_smb_path(to)?;

        debug!(
            "SmbVolume::rename: share={}, from={:?}, to={:?}, force={}",
            self.inner.share_name, smb_from, smb_to, force
        );

        if force {
            self.clear_rename_destination(&smb_to).await?;
        } else {
            // Check if dest exists and return AlreadyExists if so
            let dest_exists = {
                let (tree, mut conn) = self.clone_session().await?;
                tree.stat(&mut conn, &smb_to).await.is_ok()
            };
            if dest_exists {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }
        }

        {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.rename(&mut conn, &smb_from, &smb_to).await;
            // `AlreadyExists` is about the DESTINATION, everything else about the
            // source, the same split `local_posix.rs::rename_error` makes.
            let path_at_fault = if matches!(r, Err(ref e) if e.kind() == smb2::ErrorKind::AlreadyExists) {
                &smb_to
            } else {
                &smb_from
            };
            self.handle_smb_result("rename", path_at_fault, r)?;
        }

        self.notify_renamed(from, to).await;
        Ok(())
    }

    /// Removes whatever sits at `smb_to`, so a forced rename can land on it.
    ///
    /// Tries the file delete first; only a typed `IsADirectory` earns the second
    /// round trip. Any other refusal (`PermissionDenied`, `SharingViolation`, …)
    /// propagates immediately instead of being masked by a second futile delete.
    async fn clear_rename_destination(&self, smb_to: &str) -> Result<(), VolumeError> {
        let dest_exists = {
            let (tree, mut conn) = self.clone_session().await?;
            tree.stat(&mut conn, smb_to).await.is_ok()
        };
        if !dest_exists {
            return Ok(());
        }

        let file_result = {
            let (tree, mut conn) = self.clone_session().await?;
            let r = tree.delete_file(&mut conn, smb_to).await;
            self.handle_smb_result("rename(delete_dest_file)", smb_to, r)
        };
        match file_result {
            Ok(()) => Ok(()),
            Err(VolumeError::IsADirectory(_)) => {
                let (tree, mut conn) = self.clone_session().await?;
                let r = tree.delete_directory(&mut conn, smb_to).await;
                self.handle_smb_result("rename(delete_dest_dir)", smb_to, r)?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The one listing-cache patch a create leaves behind. A path that doesn't
    /// convert skips it: the write already succeeded.
    async fn notify_created(&self, path: &Path) {
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && let Some(parent_display) = self.display_path_for(parent)
        {
            self.notify_mutation(
                &self.inner.volume_id,
                &parent_display,
                MutationEvent::Created(name.to_string_lossy().to_string()),
            )
            .await;
        }
    }

    /// The listing-cache patches a rename leaves behind: one `Renamed` when both
    /// ends share a parent, otherwise a `Deleted` at the source and a `Created`
    /// at the destination. Still one call per changed DIRECTORY, never per entry.
    async fn notify_renamed(&self, from: &Path, to: &Path) {
        let (Some(from_parent), Some(from_name)) = (from.parent(), from.file_name()) else {
            return;
        };
        let Some(from_parent_display) = self.display_path_for(from_parent) else {
            return;
        };
        let to_name = to
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if from.parent() == to.parent() {
            // Same-directory rename
            self.notify_mutation(
                &self.inner.volume_id,
                &from_parent_display,
                MutationEvent::Renamed {
                    from: from_name.to_string_lossy().to_string(),
                    to: to_name,
                },
            )
            .await;
        } else {
            // Cross-directory move: remove from source, add in dest
            self.notify_mutation(
                &self.inner.volume_id,
                &from_parent_display,
                MutationEvent::Deleted(from_name.to_string_lossy().to_string()),
            )
            .await;
            if let Some(to_parent_display) = to.parent().and_then(|p| self.display_path_for(p)) {
                self.notify_mutation(
                    &self.inner.volume_id,
                    &to_parent_display,
                    MutationEvent::Created(to_name),
                )
                .await;
            }
        }
    }

    /// Patches the cached listing of ONE directory to match a mutation that has
    /// already landed on the share.
    pub(super) async fn notify_mutation_impl(&self, parent_path: &Path, mutation: MutationEvent) {
        use cmdr_fs::volume::DirectoryChange;

        // One call per MUTATION, never per entry: the host walks every cached
        // listing on the volume, so a per-entry caller turns one directory into
        // a quadratic sweep.
        let listings = self.inner.host().listings();

        match mutation {
            MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                let entry_path = parent_path.join(name);
                match self.get_metadata_impl(&entry_path).await {
                    Ok(entry) => {
                        let change = if matches!(mutation, MutationEvent::Created(_)) {
                            DirectoryChange::Added(entry)
                        } else {
                            DirectoryChange::Modified(entry)
                        };
                        listings.directory_changed(&self.inner.volume_id, parent_path, change);
                    }
                    Err(e) => {
                        warn!(
                            "SmbVolume::notify_mutation: couldn't stat {}: {}",
                            entry_path.display(),
                            e
                        );
                    }
                }
            }
            MutationEvent::Deleted(name) => {
                listings.directory_changed(&self.inner.volume_id, parent_path, DirectoryChange::Removed(name));
            }
            MutationEvent::Renamed { from, to } => {
                let new_path = parent_path.join(&to);
                match self.get_metadata_impl(&new_path).await {
                    Ok(entry) => {
                        listings.directory_changed(
                            &self.inner.volume_id,
                            parent_path,
                            DirectoryChange::Renamed {
                                old_name: from,
                                new_entry: entry,
                            },
                        );
                    }
                    Err(e) => {
                        warn!(
                            "SmbVolume::notify_mutation: couldn't stat renamed entry {}: {}",
                            new_path.display(),
                            e
                        );
                    }
                }
            }
        }
    }
}
