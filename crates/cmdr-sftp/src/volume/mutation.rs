//! Changing what's on the server: create, delete, rename, and the listing-cache
//! patch each one leaves behind.
//!
//! Two rules hold this module up, and both are about SFTP v3 having no way to
//! say "that name is taken".
//!
//! - **The SERVER refuses the clobber, never a check of ours.** `SSH_FXF_EXCL`
//!   and `SSH_FXP_MKDIR` both fail atomically on a name that's occupied. ❌ A
//!   stat-then-write would leave a window, and on a server with
//!   `posix-rename@openssh.com` what fits in that window is a silently
//!   overwritten file.
//! - **A stat probe classifies, ❌ never guards.** `SSH_FX_FAILURE` covers
//!   `EEXIST`, `ENOTEMPTY`, and most of the rest of errno, so asking what's at
//!   the path AFTER a failure is the only way to answer `AlreadyExists`. Asked
//!   BEFORE, the same question is the TOCTOU window the rule above closes.
//!
//! Every method has the same two halves: the round trips that decide success,
//! then — only once they succeeded — ONE `notify_mutation` per changed
//! directory. ❌ Never one per entry: the host walks every cached listing on the
//! volume, so a per-entry caller turns one directory into a quadratic sweep.
//!
//! `DETAILS.md` § "The error policy" carries the table, and § "Renaming without
//! clobbering" the reasoning behind the claim.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::{DirectoryCreation, MutationEvent, Volume, VolumeError};
use log::debug;
use openssh_sftp_client::Error as SftpError;

use super::SftpVolume;
use super::writes::{RemoteWrite, write_all_at};
use crate::errors::{Attempted, WhatIsThere, map_sftp_error, resolve_ambiguity};
use crate::transport::SshConnection;

impl SftpVolume {
    /// Writes `content` to a name nothing else holds.
    ///
    /// ❗ The refusal is `SSH_FXF_EXCL`'s, so there is no instant at which an
    /// existing file could be truncated. The New File command hands a user-typed
    /// name straight here and renders the refusal as "that name is taken".
    pub(super) async fn create_file_impl(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        debug!("SftpVolume::create_file: {remote}, size={}", content.len());

        let opened = session
            .sftp()
            .options()
            .write(true)
            .create_new(true)
            .open(&remote)
            .await;
        let file = match opened {
            Ok(file) => file,
            Err(e) => return Err(self.name_taken(&session, &remote, &e).await),
        };

        // Past this point the file is OURS: `SSH_FXF_EXCL` proved nothing was
        // there, so cleaning up after a failure can't take away anybody's data.
        let mut writer = RemoteWrite::new(file, Arc::from(remote.as_str()));
        let wrote = write_all_at(&mut writer, 0, content).await;
        // ❗ `File::close()` rather than a drop, and its answer is part of the
        // write: a drop sends the same `SSH_FXP_CLOSE` on a detached task and
        // throws away the one report a server gives of bytes it accepted but
        // could not commit.
        let closed = writer.close().await;
        if let Err(e) = wrote.and(closed) {
            let _ = session.sftp().fs().remove_file(&remote).await;
            return Err(e);
        }

        self.notify_created(path).await;
        Ok(())
    }

    /// Creates one directory.
    ///
    /// `SSH_FXP_MKDIR` refuses an occupied name on every server, extension or
    /// not, which is what lets [`Volume::create_directory_errors_on_existing_dir`]
    /// answer `true` here — and that answer is what gives a remote archive edit
    /// its atomic swap.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        debug!("SftpVolume::create_directory: {remote}");

        self.create_one_directory(&session, &remote).await?;
        self.notify_created(path).await;
        Ok(())
    }

    /// `mkdir -p`, in one round trip when the parent is already there.
    ///
    /// ❗ Overridden rather than left to the trait default, which calls
    /// `exists()` once per ancestor: over a 50 ms link that is one round trip per
    /// level before a single directory gets made, and a deep destination pays it
    /// on every copy.
    ///
    /// ❗ And it answers honestly. `Created` promises the leaf was empty at that
    /// instant, and the transfer driver spends the promise by skipping its
    /// per-file destination conflict probe inside — so a `Created` for a
    /// directory we merely found turns "would have prompted" into "overwrote",
    /// for every file in the copy.
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        debug!("SftpVolume::create_directory_all: {remote}");

        // The volume root always exists, and so does every spelling of it.
        let root = self.to_remote_path(Path::new("/"))?;
        if remote == root {
            return Ok(DirectoryCreation::AlreadyExisted);
        }

        // The leaf first: the common case is a new folder under a directory
        // that's already there, and that costs exactly one request.
        match self.create_one_directory(&session, &remote).await {
            Ok(()) => {
                self.notify_created(path).await;
                return Ok(DirectoryCreation::Created);
            }
            Err(VolumeError::AlreadyExists(_)) => return Ok(DirectoryCreation::AlreadyExisted),
            // Only a missing ancestor earns the walk. Anything else (a read-only
            // export, a quota, a refused name) fails the same way at every level,
            // so walking would just spend round trips to arrive at the same
            // answer.
            Err(VolumeError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }

        // Leaf → root, stopping at the volume root, then created shallowest
        // first so no child is asked for before its parent. Each level keeps both
        // spellings: the remote one to create, the caller's to patch a pane with.
        let mut missing: Vec<(&Path, String)> = Vec::new();
        for ancestor in path.ancestors() {
            let Ok(remote_ancestor) = self.to_remote_path(ancestor) else {
                break;
            };
            if remote_ancestor == root {
                break;
            }
            missing.push((ancestor, remote_ancestor));
        }

        let mut leaf = DirectoryCreation::AlreadyExisted;
        let mut first_created: Option<&Path> = None;
        for (index, (as_addressed, dir)) in missing.iter().enumerate().rev() {
            match self.create_one_directory(&session, dir).await {
                Ok(()) => {
                    first_created.get_or_insert(as_addressed);
                    if index == 0 {
                        leaf = DirectoryCreation::Created;
                    }
                }
                // Somebody else got there first. Idempotent, so it's a success —
                // ❗ but not OURS: their directory may already hold something,
                // which is exactly what `AlreadyExisted` tells the caller.
                Err(VolumeError::AlreadyExists(_)) => {}
                Err(e) => return Err(e),
            }
        }
        // ❗ ONE patch, for the SHALLOWEST directory this created. Its parent is
        // the only level that was there before, so it is the only listing a pane
        // could be holding — the levels under it are brand new and nobody has
        // them cached. Patching the leaf instead leaves that pane a level short,
        // and patching every level would spend a stat round trip per level on
        // directories nothing is showing.
        if let Some(created) = first_created {
            self.notify_created(created).await;
        }
        Ok(leaf)
    }

    /// Deletes one file or one EMPTY directory.
    ///
    /// ❗ Strictly one node. Real data-safety logic leans on the refusal rather
    /// than on a check of its own: a same-volume move keeps a skipped child's
    /// only copy purely by letting its parent's delete fail.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        debug!("SftpVolume::delete: {remote}");

        // `SSH_FXP_REMOVE` first, so a bulk delete of files spends one round trip
        // each rather than a stat plus a remove. A directory refuses it, and the
        // directory form is the second guess.
        let first = match session.sftp().fs().remove_file(&remote).await {
            Ok(()) => {
                self.notify_deleted(path).await;
                return Ok(());
            }
            Err(e) => e,
        };
        let second = match session.sftp().fs().remove_dir(&remote).await {
            Ok(()) => {
                self.notify_deleted(path).await;
                return Ok(());
            }
            Err(e) => e,
        };

        // Both refused, so ask what is actually there. A directory means the
        // second refusal is the one that describes the path; anything else means
        // it was never a directory and the FILE delete's own refusal is the
        // honest answer — otherwise a permission-denied file would be reported
        // as the "not a directory" the rmdir complained about.
        match self.probe(&session, &remote).await {
            WhatIsThere::Directory => Err(resolve_ambiguity(
                &second,
                &remote,
                Attempted::RemovingANode,
                WhatIsThere::Directory,
            )),
            _ => Err(map_sftp_error(&first, &remote)),
        }
    }

    /// Moves an entry, clearing the destination only when the caller said it may.
    ///
    /// The two halves are genuinely different operations, and § "Renaming
    /// without clobbering" in `DETAILS.md` says why neither can be written as the
    /// other.
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        let session = self.clone_session().await?;
        debug!("SftpVolume::rename: {remote_from} → {remote_to}, force={force}");

        if force {
            self.rename_over(&session, &remote_from, &remote_to).await?;
        } else {
            self.rename_into_a_free_name(&session, &remote_from, &remote_to).await?;
        }

        self.notify_renamed(from, to).await;
        Ok(())
    }

    // ── The two renames ──────────────────────────────────────────────

    /// `force = true`: the destination may be replaced.
    ///
    /// `Fs::rename` reaches for `posix-rename@openssh.com` when the server offers
    /// it, and here that is exactly right: the extension is DEFINED to replace
    /// the destination atomically, so a remote archive edit swaps its new bytes
    /// in with no window at all.
    ///
    /// A server without the extension sends plain `SSH_FXP_RENAME`, which refuses
    /// an occupied destination — so that one needs the destination cleared
    /// first. ❗ Cleared only once something is proven to be there: clearing on
    /// any failure is how a transient blip becomes a deleted file.
    async fn rename_over(
        &self,
        session: &SshConnection,
        remote_from: &str,
        remote_to: &str,
    ) -> Result<(), VolumeError> {
        let first = match session.sftp().fs().rename(remote_from, remote_to).await {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        let found = self.probe(session, remote_to).await;
        if found == WhatIsThere::Nothing {
            return Err(map_sftp_error(&first, remote_from));
        }
        self.remove_whatever_is_at(session, remote_to, found).await?;
        session
            .sftp()
            .fs()
            .rename(remote_from, remote_to)
            .await
            .map_err(|e| map_sftp_error(&e, remote_to))
    }

    /// `force = false`: the destination must be free, and stay whatever it is if
    /// it isn't.
    ///
    /// ❌ `Fs::rename` is not the call here. On a server with
    /// `posix-rename@openssh.com` it silently replaces the destination, which is
    /// the opposite of what this promises — and it is what every caller that
    /// hasn't asked the user yet relies on.
    ///
    /// So the name is CLAIMED first, with a primitive the server refuses
    /// atomically, and the rename then lands on a placeholder of our own.
    async fn rename_into_a_free_name(
        &self,
        session: &SshConnection,
        remote_from: &str,
        remote_to: &str,
    ) -> Result<(), VolumeError> {
        if !session.extensions().posix_rename {
            // Plain `SSH_FXP_RENAME`, which is what `Fs::rename` sends on a
            // server without the extension, refuses an occupied destination by
            // itself (`link` + `EEXIST` for regular files, a stat guard on the
            // server for everything else). Nothing to add, and one round trip.
            return match session.sftp().fs().rename(remote_from, remote_to).await {
                Ok(()) => Ok(()),
                Err(e) => Err(self.name_taken(session, remote_to, &e).await),
            };
        }

        // A file-shaped placeholder covers the overwhelming case (a staged write
        // landing) in one extra round trip.
        match session
            .sftp()
            .options()
            .write(true)
            .create_new(true)
            .open(remote_to)
            .await
        {
            // Dropped rather than closed, and the ORDER still holds: `Drop`
            // writes `SSH_FXP_CLOSE` into the send buffer synchronously and only
            // spawns the wait for its answer (`handle.rs`), so the server sees
            // the close before the rename that follows on the same ordered
            // stream. ❗ That is what makes this safe on a server where renaming
            // over an open handle would fail. A zero-byte placeholder we are
            // about to replace has nothing to report, so the ANSWER is the only
            // thing worth skipping here.
            Ok(file) => drop(file),
            Err(e) => return Err(self.name_taken(session, remote_to, &e).await),
        }

        let first = match session.sftp().fs().rename(remote_from, remote_to).await {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        // ❗ The placeholder must never outlive the attempt that made it, or a
        // zero-byte file is left wearing the name the user chose.
        let _ = session.sftp().fs().remove_file(remote_to).await;

        if self.probe(session, remote_from).await != WhatIsThere::Directory {
            return Err(map_sftp_error(&first, remote_from));
        }
        // A directory can't be renamed onto a file (`ENOTDIR`), so the claim has
        // to be directory-shaped. `SSH_FXP_MKDIR` refuses an occupied name just
        // as atomically, and POSIX rename replaces an EMPTY directory.
        self.create_one_directory(session, remote_to).await?;
        match session.sftp().fs().rename(remote_from, remote_to).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = session.sftp().fs().remove_dir(remote_to).await;
                Err(map_sftp_error(&e, remote_to))
            }
        }
    }

    // ── The shared pieces ────────────────────────────────────────────

    /// One `SSH_FXP_MKDIR`, with its catch-all failure resolved.
    async fn create_one_directory(&self, session: &SshConnection, remote: &str) -> Result<(), VolumeError> {
        match session.sftp().fs().create_dir(remote).await {
            Ok(()) => Ok(()),
            Err(e) => Err(self.name_taken(session, remote, &e).await),
        }
    }

    /// Removes whatever the probe found at `remote`, so a forced rename can land
    /// on it.
    async fn remove_whatever_is_at(
        &self,
        session: &SshConnection,
        remote: &str,
        found: WhatIsThere,
    ) -> Result<(), VolumeError> {
        let removed = match found {
            WhatIsThere::Directory => session.sftp().fs().remove_dir(remote).await,
            WhatIsThere::NotADirectory => session.sftp().fs().remove_file(remote).await,
            WhatIsThere::Nothing => return Ok(()),
        };
        removed.map_err(|e| map_sftp_error(&e, remote))
    }

    /// The failure of an operation that was trying to take a name, read through
    /// what the server says is at that name now.
    ///
    /// ❗ One round trip, and only on a path that has already failed.
    async fn name_taken(&self, session: &SshConnection, remote: &str, err: &SftpError) -> VolumeError {
        let found = self.probe(session, remote).await;
        resolve_ambiguity(err, remote, Attempted::TakingAName, found)
    }

    /// What the server says is at `remote`.
    ///
    /// ❗ Only ever called after something already failed. As a pre-flight guard
    /// this same question is a TOCTOU window; afterwards it decides nothing that
    /// hasn't already happened.
    async fn probe(&self, session: &SshConnection, remote: &str) -> WhatIsThere {
        match session.sftp().fs().symlink_metadata(remote).await {
            Ok(meta) => match meta.file_type() {
                Some(kind) if kind.is_dir() => WhatIsThere::Directory,
                // A stat that answered at all means something is there, whatever
                // shape it is. ❗ A server that declines to say the type must not
                // read as an empty name: that would turn "taken" into
                // "unclassified" and lose the refusal.
                _ => WhatIsThere::NotADirectory,
            },
            Err(_) => WhatIsThere::Nothing,
        }
    }

    // ── The listing-cache patches ────────────────────────────────────

    /// The one patch a create leaves behind. A path that doesn't translate skips
    /// it: the write already succeeded, and a cache patch must never fail it.
    pub(super) async fn notify_created(&self, path: &Path) {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let Some(parent_display) = self.display_path_for(parent) else {
            return;
        };
        self.notify_mutation(
            self.volume_id(),
            &parent_display,
            MutationEvent::Created(name.to_string_lossy().to_string()),
        )
        .await;
    }

    /// The same for a delete.
    async fn notify_deleted(&self, path: &Path) {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let Some(parent_display) = self.display_path_for(parent) else {
            return;
        };
        self.notify_mutation(
            self.volume_id(),
            &parent_display,
            MutationEvent::Deleted(name.to_string_lossy().to_string()),
        )
        .await;
    }

    /// One `Renamed` when both ends share a parent, otherwise a `Deleted` at the
    /// source and a `Created` at the destination. Still one call per changed
    /// DIRECTORY.
    async fn notify_renamed(&self, from: &Path, to: &Path) {
        if from.parent() == to.parent() {
            let (Some(parent), Some(from_name), Some(to_name)) = (from.parent(), from.file_name(), to.file_name())
            else {
                return;
            };
            let Some(parent_display) = self.display_path_for(parent) else {
                return;
            };
            self.notify_mutation(
                self.volume_id(),
                &parent_display,
                MutationEvent::Renamed {
                    from: from_name.to_string_lossy().to_string(),
                    to: to_name.to_string_lossy().to_string(),
                },
            )
            .await;
            return;
        }
        self.notify_deleted(from).await;
        self.notify_created(to).await;
    }

    /// Patches the cached listing of ONE directory to match a mutation that has
    /// already landed on the server.
    pub(super) async fn notify_mutation_impl(&self, parent_path: &Path, mutation: MutationEvent) {
        use cmdr_fs::volume::DirectoryChange;

        let listings = self.inner.host.listings();
        let volume_id = self.volume_id();
        match mutation {
            MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                let entry_path = parent_path.join(name);
                // One stat, and a failure is simply no patch: the pane re-lists
                // eventually, and the mutation itself has already succeeded.
                let Ok(entry) = self.get_metadata_impl(&entry_path).await else {
                    return;
                };
                let change = if matches!(mutation, MutationEvent::Created(_)) {
                    DirectoryChange::Added(entry)
                } else {
                    DirectoryChange::Modified(entry)
                };
                listings.directory_changed(volume_id, parent_path, change);
            }
            MutationEvent::Deleted(name) => {
                listings.directory_changed(volume_id, parent_path, DirectoryChange::Removed(name));
            }
            MutationEvent::Renamed { from, to } => {
                let Ok(entry) = self.get_metadata_impl(&parent_path.join(&to)).await else {
                    return;
                };
                listings.directory_changed(
                    volume_id,
                    parent_path,
                    DirectoryChange::Renamed {
                        old_name: from,
                        new_entry: entry,
                    },
                );
            }
        }
    }
}
