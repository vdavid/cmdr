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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::mkdir_all::{self, MakesDirectories};
use cmdr_fs::volume::patching::{PatchSource, patch_created, patch_deleted, patch_renamed};
use cmdr_fs::volume::scan_walk::Walking;
use cmdr_fs::volume::{DirectoryCreation, VolumeError};
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

        patch_created(self, path).await;
        Ok(())
    }

    /// Creates one directory.
    ///
    /// `SSH_FXP_MKDIR` refuses an occupied name on every server, extension or
    /// not, which is what lets `Volume::create_directory_errors_on_existing_dir`
    /// answer `true` here — and that answer is what gives a remote archive edit
    /// its atomic swap.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        debug!("SftpVolume::create_directory: {remote}");

        self.create_one_directory(&session, &remote).await?;
        patch_created(self, path).await;
        Ok(())
    }

    /// `mkdir -p`, through the shared walk: leaf first, ancestors only when the
    /// leaf's parent was missing.
    ///
    /// ❗ Overridden rather than left to the trait default, which calls
    /// `exists()` once per ancestor: over a 50 ms link that is one round trip per
    /// level before a single directory gets made, and a deep destination pays it
    /// on every copy. The honesty contract on the answer, and why a `Created` we
    /// aren't sure of is an overwrite: `cmdr_fs::volume::mkdir_all`.
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        debug!("SftpVolume::create_directory_all: {}", path.display());
        let made = mkdir_all::create_directory_all(self, path).await?;
        if let Some(created) = made.shallowest_created {
            patch_created(self, &created).await;
        }
        Ok(made.leaf)
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
                patch_deleted(self, path).await;
                return Ok(());
            }
            Err(e) => e,
        };
        let second = match session.sftp().fs().remove_dir(&remote).await {
            Ok(()) => {
                patch_deleted(self, path).await;
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

        patch_renamed(self, from, to).await;
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
}

/// What the shared `mkdir -p` walk needs from this backend: one `SSH_FXP_MKDIR`,
/// and this volume's own path spelling. The `Created` promise it answers with is
/// spent by the transfer driver, so the refusals matter:
/// `cmdr_fs::volume::mkdir_all`.
impl MakesDirectories for SftpVolume {
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError> {
        self.to_remote_path(path)
    }

    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()> {
        Box::pin(async move {
            let session = self.clone_session().await?;
            self.create_one_directory(&session, remote).await
        })
    }
}

/// What the shared listing-cache patcher needs from this backend. ❗ There is no
/// watcher here, so a patch is the ONLY thing that keeps a pane honest after a
/// write. The rules: `cmdr_fs::volume::patching`.
impl PatchSource for SftpVolume {
    fn patch_volume_id(&self) -> &str {
        self.volume_id()
    }

    fn patch_listings(&self) -> &dyn ListingHost {
        self.inner.host.listings()
    }

    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn patch_display_path(&self, path: &Path) -> Option<PathBuf> {
        self.display_path_for(path)
    }
}
