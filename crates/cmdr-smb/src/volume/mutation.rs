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
use super::session::SessionLease;
use super::state::SessionGeneration;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
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
            "SmbVolume::create_file: share={:?}, path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::file_path(&smb_path)
        );

        {
            let SessionLease { tree, conn, generation } = self.clone_session().await?;
            // No-clobber contract via the exclusive-create writer
            // (`FileCreate` disposition): if the file already exists the
            // server returns `STATUS_OBJECT_NAME_COLLISION`, which the
            // smb2 crate maps to `ErrorKind::AlreadyExists`. The earlier
            // stat-then-write workaround left a microsecond TOCTOU
            // window; this closes it atomically at the protocol layer.
            let writer_result = tree.create_file_writer_exclusive(conn, &smb_path).await;
            let mut writer = self.handle_smb_result("create_file(open)", &smb_path, generation, writer_result)?;
            if !data.is_empty() {
                let write_result = writer.write_chunk(&data).await;
                self.handle_smb_result("create_file(write_chunk)", &smb_path, generation, write_result)?;
            }
            let finish_result = writer.finish().await;
            self.handle_smb_result("create_file(finish)", &smb_path, generation, finish_result)?;
        }

        self.notify_created(path).await;
        Ok(())
    }

    /// Creates a directory, then tells the panes about it.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::create_directory: share={:?}, path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::dir_path(&smb_path)
        );

        {
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let result = tree.create_directory(&mut conn, &smb_path).await;
            self.handle_smb_result("create_directory", &smb_path, generation, result)?;
        }

        self.notify_created(path).await;
        Ok(())
    }

    /// Sets what `times` names (the dates, and the attributes when it carries
    /// them) on a file or folder, by path, in one CREATE + SET_INFO + CLOSE
    /// frame, then tells the panes about it. What `times` leaves unset stays
    /// the server's own.
    pub(super) async fn set_basic_info_impl(&self, path: &Path, times: smb2::FileTimes) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::set_times: share={:?}, path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::path(&smb_path)
        );

        {
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let result = tree.set_times(&mut conn, &smb_path, times).await;
            self.handle_smb_result("set_times", &smb_path, generation, result)?;
        }

        if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && let Some(parent_display) = self.display_path_for(parent)
        {
            self.notify_mutation(
                &self.inner.volume_id,
                &parent_display,
                MutationEvent::Modified(name.to_string_lossy().to_string()),
            )
            .await;
        }
        Ok(())
    }

    /// Deletes a file or an empty directory, then tells the panes about it.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;

        debug!(
            "SmbVolume::delete: share={:?}, path={:?}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::path(&smb_path)
        );

        // `delete_link` opens the entry's reparse point itself with no
        // file-or-directory condition, so one round trip takes a file, an empty
        // folder, or a junction or symlink AS the link, ❌ never its target (a
        // junction is a folder on Windows, a Samba symlink a file, #516). A
        // non-empty folder is refused, as the trait promises.
        {
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let r = tree.delete_link(&mut conn, &smb_path).await;
            self.handle_smb_result("delete", &smb_path, generation, r)?;
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

    /// Renames or moves an entry, replacing what's at the destination when
    /// `force`, then tells the panes about both ends.
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let smb_from = self.to_smb_path(from)?;
        let smb_to = self.to_smb_path(to)?;

        debug!(
            "SmbVolume::rename: share={:?}, from={:?}, to={:?}, force={}",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::path(&smb_from),
            LogPrivate::path(&smb_to),
            force
        );

        if force {
            self.replacing_rename(&smb_from, &smb_to).await?;
        } else {
            self.plain_rename(&smb_from, &smb_to).await?;
        }

        self.notify_renamed(from, to).await;
        Ok(())
    }

    /// One rename that refuses a taken name. The server is what refuses it
    /// (smb2 sends `ReplaceIfExists = false`), with `AlreadyExists` naming the
    /// destination. No stat goes out first: on a case-insensitive share it finds
    /// the SOURCE under a case-only new name and refused that rename as taken.
    async fn plain_rename(&self, smb_from: &str, smb_to: &str) -> Result<(), VolumeError> {
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let r = tree.rename(&mut conn, smb_from, smb_to).await;
        self.handle_rename_result(smb_from, smb_to, generation, r)
    }

    /// A forced rename: the server swaps the source in under `smb_to` in the
    /// same step that renames it (`ReplaceIfExists`), so a file at the
    /// destination is never missing and never removed on its own. That's what
    /// makes `supports_atomic_replace_rename` true.
    ///
    /// smb2 refuses a directory at either end (Samba with `AlreadyExists`,
    /// Windows with `AccessDenied`), and only then does this fall back to
    /// clearing the destination first, which is not atomic and doesn't have to
    /// be: the trait promises atomicity for a FILE. A destination that names the
    /// source itself under another case is never cleared: on a case-insensitive
    /// share that would delete the source.
    async fn replacing_rename(&self, smb_from: &str, smb_to: &str) -> Result<(), VolumeError> {
        let replace = smb2::RenameOptions::new().replace_if_exists(true);
        let (r, generation) = {
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            (tree.rename_with(&mut conn, smb_from, smb_to, replace).await, generation)
        };
        let refused_for_a_directory = match &r {
            Err(e) if matches!(e.kind(), smb2::ErrorKind::AlreadyExists | smb2::ErrorKind::AccessDenied) => {
                !same_name_ignoring_case(smb_from, smb_to) && self.either_end_is_a_directory(smb_from, smb_to).await
            }
            _ => false,
        };
        if !refused_for_a_directory {
            return self.handle_rename_result(smb_from, smb_to, generation, r);
        }
        self.clear_rename_destination(smb_to).await?;
        self.plain_rename(smb_from, smb_to).await
    }

    /// Whether a stat says either end of a rename is a directory. A stat that
    /// fails says no: the refusal then stands as it came.
    async fn either_end_is_a_directory(&self, smb_from: &str, smb_to: &str) -> bool {
        for path in [smb_from, smb_to] {
            // A probe: its failure says "no" and goes nowhere, so it reports
            // nothing to the error funnel.
            let SessionLease {
                tree,
                mut conn,
                generation: _,
            } = match self.clone_session().await {
                Ok(lease) => lease,
                Err(_) => return false,
            };
            if tree.stat(&mut conn, path).await.is_ok_and(|info| info.is_directory) {
                return true;
            }
        }
        false
    }

    /// `AlreadyExists` is about the DESTINATION, everything else about the
    /// source, the same split `local_posix.rs::rename_error` makes.
    fn handle_rename_result(
        &self,
        smb_from: &str,
        smb_to: &str,
        ran_on: SessionGeneration,
        r: Result<(), smb2::Error>,
    ) -> Result<(), VolumeError> {
        let path_at_fault = if matches!(r, Err(ref e) if e.kind() == smb2::ErrorKind::AlreadyExists) {
            smb_to
        } else {
            smb_from
        };
        self.handle_smb_result("rename", path_at_fault, ran_on, r)
    }

    /// Removes whatever sits at `smb_to`, so a forced rename can land on it: a
    /// file, an empty folder, or a link (the link alone). A refusal
    /// (`PermissionDenied`, a non-empty folder, …) propagates as it came.
    async fn clear_rename_destination(&self, smb_to: &str) -> Result<(), VolumeError> {
        let dest_exists = {
            let SessionLease {
                tree,
                mut conn,
                generation: _,
            } = self.clone_session().await?;
            tree.stat(&mut conn, smb_to).await.is_ok()
        };
        if !dest_exists {
            return Ok(());
        }

        // The same one-frame delete `delete_impl` sends: a file, an empty
        // folder, or a link as the link.
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let r = tree.delete_link(&mut conn, smb_to).await;
        self.handle_smb_result("rename(delete_dest)", smb_to, generation, r)
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
                            "SmbVolume::notify_mutation: couldn't stat path={:?}: {}",
                            LogPrivate::path(&entry_path),
                            LogError(&e)
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
                            "SmbVolume::notify_mutation: couldn't stat renamed entry path={:?}: {}",
                            LogPrivate::path(&new_path),
                            LogError(&e)
                        );
                    }
                }
            }
        }
    }
}

/// Whether two share paths name the same entry on a case-insensitive share.
/// Folds case only, never Unicode form: SMB names are opaque bytes
/// (`DETAILS.md` § "SMB names are opaque bytes").
fn same_name_ignoring_case(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}
