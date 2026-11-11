//! Changing names on the export: folders, deletes, renames, and patching the
//! panes' listings afterwards.
//!
//! - ❗ Every mutation resolves its folder through
//!   [`NfsVolume::dir_for_mutation`] (the parent chain proven first) and goes
//!   by `dir handle + name`, ❌ never by a leaf's cached handle.
//! - ❗ After a success, the reply's `wcc_data` goes to
//!   `HandleCache::note_dir_changed`, so the next mutation in the same folder
//!   needn't re-LOOKUP; a folder that moved or went is forgotten with its
//!   subtree, since its cached handle would still answer, for the new place.
//! - ❗ **Rename without clobbering.** v3 `RENAME` REPLACES its target, so
//!   `rename(force = false)` first claims the name with `CREATE GUARDED` (a
//!   file, a link) or `MKDIR` (a folder), which the server refuses atomically
//!   when it's taken, then `RENAME`s onto the empty claim. A failed rename takes
//!   the claim away again, but only once a LOOKUP shows the name still holds
//!   OUR claim: a `RENAME` whose reply never came may have landed, and the
//!   name then holds the user's file. `crates/cmdr-sftp/DETAILS.md` §
//!   "Renaming without clobbering" is the model.
//! - RFC 1813 § 3.3.14's edges: a RENAME onto the same file succeeds and does
//!   nothing, so the claim must not run first (it would answer `EXIST` for the
//!   source itself); a folder replaces only an EMPTY folder, which is what makes
//!   the `MKDIR` claim work.

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::default_folder_mode;
use cmdr_fs::os_error::OsErrorClass;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::mkdir_all::{self, LeadsTo, MakesDirectories};
use cmdr_fs::volume::patching::{PatchSource, patch_created, patch_deleted, patch_renamed};
use cmdr_fs::volume::scan_walk::Walking;
use cmdr_fs::volume::{DirectoryCreation, VolumeError};
use log::debug;

use super::NfsVolume;
use super::handles::{Follow, MutationDir};
use super::paths::child_of;
use super::reconnect::NfsClient;
use crate::errors::map_nfs_error;
use crate::proto::nfs3::{CreateHow, Fattr3, FileType, NfsError, NfsStat3, Sattr3};

/// The mode a rename's claim is made with: owner only, since it stands empty
/// for one round trip and is replaced by the source.
const CLAIM_FILE_MODE: u32 = 0o600;
const CLAIM_FOLDER_MODE: u32 = 0o700;

/// `remote`'s folder and its own name (`/a/b` → `/a`, `b`), or `None` for the
/// export's root, which has neither.
pub(super) fn split_parent(remote: &str) -> Option<(&str, &str)> {
    let trimmed = remote.trim_end_matches('/');
    let cut = trimmed.rfind('/')?;
    let name = &trimmed[cut + 1..];
    if name.is_empty() {
        return None;
    }
    Some((if cut == 0 { "/" } else { &trimmed[..cut] }, name))
}

/// The export's root can't be made, removed, or renamed: it's the mount.
fn the_root_itself(remote: &str) -> VolumeError {
    VolumeError::PermissionDenied {
        path: remote.to_string(),
        raw_os_error: None,
    }
}

impl NfsVolume {
    /// The proven folder of `remote` and `remote`'s own name in it.
    pub(super) async fn parent_for_mutation<'r>(
        &self,
        client: &NfsClient,
        remote: &'r str,
    ) -> Result<(MutationDir, &'r str), VolumeError> {
        let (parent, name) = split_parent(remote).ok_or_else(|| the_root_itself(remote))?;
        Ok((self.dir_for_mutation(client, parent).await?, name))
    }

    /// What `name` in `dir` is, fresh: the attributes a delete or a rename
    /// decides by. LOOKUP's own, so a link is the link.
    pub(super) async fn lookup_in(
        &self,
        client: &NfsClient,
        dir: &MutationDir,
        name: &str,
        remote: &str,
    ) -> Result<Fattr3, VolumeError> {
        let found = client
            .nfs
            .lookup(&dir.handle, name.as_bytes())
            .await
            .map_err(|err| map_nfs_error(&err, remote))?;
        match found.attributes {
            Some(attrs) => Ok(attrs),
            None => client
                .nfs
                .getattr(&found.handle)
                .await
                .map_err(|err| map_nfs_error(&err, remote)),
        }
    }

    /// Removes `name` from `dir` only while it's still the object `fileid`
    /// names: what a write's partial and a rename's claim are taken away by.
    /// Best effort and logged; `false` when something else holds the name now
    /// or the removal didn't happen.
    pub(super) async fn remove_if_ours(
        &self,
        client: &NfsClient,
        dir: &MutationDir,
        name: &str,
        remote: &str,
        fileid: u64,
    ) -> bool {
        let holds = match self.lookup_in(client, dir, name, remote).await {
            Ok(attrs) => attrs,
            Err(VolumeError::NotFound(_)) => return true,
            Err(err) => {
                debug!(
                    target: "nfs",
                    "couldn't check what {} holds before removing it: {}",
                    LogPrivate::file_path(remote),
                    LogError(&err)
                );
                return false;
            }
        };
        if holds.fileid != fileid {
            debug!(target: "nfs", "{} no longer holds what we made there; left alone", LogPrivate::file_path(remote));
            return false;
        }
        let removed = if holds.file_type == FileType::Directory {
            client.nfs.rmdir(&dir.handle, name.as_bytes()).await
        } else {
            client.nfs.remove(&dir.handle, name.as_bytes()).await
        };
        match removed {
            Ok(wcc) => {
                self.inner.handles.note_dir_changed(&dir.real, &wcc);
                true
            }
            Err(NfsError::Status(NfsStat3::NoEnt)) => true,
            Err(err) => {
                debug!(target: "nfs", "couldn't remove {}: {err:?}", LogPrivate::file_path(remote));
                false
            }
        }
    }

    /// One `MKDIR`, no parents: `AlreadyExists` on a taken name, `NotFound`
    /// when the folder it goes in is missing (what the `mkdir -p` walk needs).
    async fn mkdir(&self, remote: &str) -> Result<(), VolumeError> {
        let client = self.client().await?;
        let (dir, name) = self.parent_for_mutation(&client, remote).await?;
        let sattr = Sattr3 {
            mode: Some(default_folder_mode()),
            ..Sattr3::default()
        };
        let made = client
            .nfs
            .mkdir(&dir.handle, name.as_bytes(), &sattr)
            .await
            .map_err(|err| map_nfs_error(&err, remote))?;
        self.inner.handles.note_dir_changed(&dir.real, &made.dir_wcc);
        Ok(())
    }

    /// Makes exactly one folder (`MKDIR`), refusing a taken name.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        self.mkdir(&remote).await?;
        patch_created(self, path).await;
        Ok(())
    }

    /// `mkdir -p`, through the shared walk (`cmdr_fs::volume::mkdir_all`): leaf
    /// first, ancestors only when the leaf's folder was missing.
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let made = mkdir_all::create_directory_all(self, path).await?;
        if let Some(created) = made.shallowest_created {
            patch_created(self, &created).await;
        }
        Ok(made.leaf)
    }

    /// Removes one node: `REMOVE`, or `RMDIR` for a folder, chosen by the
    /// attributes a fresh LOOKUP gives (a link is removed as the link). ❌
    /// Never recursive: a folder that holds anything is the server's
    /// `NOTEMPTY`.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.client().await?;
        let (dir, name) = self.parent_for_mutation(&client, &remote).await?;
        let attrs = self.lookup_in(&client, &dir, name, &remote).await?;
        let removed = if attrs.file_type == FileType::Directory {
            client.nfs.rmdir(&dir.handle, name.as_bytes()).await
        } else {
            client.nfs.remove(&dir.handle, name.as_bytes()).await
        };
        let wcc = removed.map_err(|err| map_nfs_error(&err, &remote))?;
        self.inner.handles.note_dir_changed(&dir.real, &wcc);
        self.inner.handles.forget_subtree(&child_of(&dir.real, name));
        patch_deleted(self, path).await;
        Ok(())
    }

    /// Renames `from` to `to`; `force` false never replaces what's at `to`
    /// (the claim, in the module docs), `force` true is one `RENAME`, which
    /// replaces atomically.
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        if remote_from == remote_to {
            return Ok(());
        }
        let client = self.client().await?;
        let (from_dir, from_name) = self.parent_for_mutation(&client, &remote_from).await?;
        let (to_dir, to_name) = self.parent_for_mutation(&client, &remote_to).await?;
        let source = self.lookup_in(&client, &from_dir, from_name, &remote_from).await?;
        debug!(
            target: "nfs",
            "rename {} -> {} (force={force})",
            LogPrivate::path(&remote_from),
            LogPrivate::path(&remote_to)
        );

        let claim = if force {
            None
        } else {
            match self.lookup_in(&client, &to_dir, to_name, &remote_to).await {
                // The same file under another spelling: RENAME does nothing
                // to it or renames it in place, and a claim would refuse.
                Ok(target) if target.fileid == source.fileid && target.fsid == source.fsid => None,
                Ok(_) => return Err(VolumeError::AlreadyExists(remote_to)),
                Err(VolumeError::NotFound(_)) => {
                    Some(self.claim(&client, &to_dir, to_name, &remote_to, &source).await?)
                }
                Err(err) => return Err(err),
            }
        };

        let renamed = client
            .nfs
            .rename(
                &from_dir.handle,
                from_name.as_bytes(),
                &to_dir.handle,
                to_name.as_bytes(),
            )
            .await;
        match renamed {
            Ok(done) => {
                self.inner.handles.note_dir_changed(&from_dir.real, &done.from_dir_wcc);
                self.inner.handles.note_dir_changed(&to_dir.real, &done.to_dir_wcc);
                // ❗ A folder's cached handle still answers after a move, for
                // its NEW place: both spellings go.
                self.inner.handles.forget_subtree(&child_of(&from_dir.real, from_name));
                self.inner.handles.forget_subtree(&child_of(&to_dir.real, to_name));
                patch_renamed(self, from, to).await;
                Ok(())
            }
            Err(err) => {
                let failed = map_nfs_error(&err, &remote_to);
                let Some(fileid) = claim else {
                    return Err(failed);
                };
                if self.remove_if_ours(&client, &to_dir, to_name, &remote_to, fileid).await {
                    return Err(failed);
                }
                Err(claim_left_behind(&remote_to, &failed))
            }
        }
    }

    /// Takes `name` in `dir` with an empty placeholder of `source`'s kind,
    /// refused by the server when the name is taken. Answers the claim's
    /// `fileid`, which is how a failed rename recognizes it later.
    async fn claim(
        &self,
        client: &NfsClient,
        dir: &MutationDir,
        name: &str,
        remote: &str,
        source: &Fattr3,
    ) -> Result<u64, VolumeError> {
        let made = if source.file_type == FileType::Directory {
            let sattr = Sattr3 {
                mode: Some(CLAIM_FOLDER_MODE),
                ..Sattr3::default()
            };
            client.nfs.mkdir(&dir.handle, name.as_bytes(), &sattr).await
        } else {
            let sattr = Sattr3 {
                mode: Some(CLAIM_FILE_MODE),
                ..Sattr3::default()
            };
            client
                .nfs
                .create(&dir.handle, name.as_bytes(), &CreateHow::Guarded(sattr))
                .await
        }
        .map_err(|err| map_nfs_error(&err, remote))?;
        self.inner.handles.note_dir_changed(&dir.real, &made.dir_wcc);
        match made.attributes {
            Some(attrs) => Ok(attrs.fileid),
            None => Ok(self.lookup_in(client, dir, name, remote).await?.fileid),
        }
    }
}

/// A rename failed with `failed`, and its empty claim at `remote` couldn't be
/// taken away: ❌ never silently left, so the error names the claim's path.
fn claim_left_behind(remote: &str, failed: &VolumeError) -> VolumeError {
    VolumeError::io_of_class(
        ErrorDetail::path_then(
            remote,
            format!(" holds an empty placeholder a failed rename left behind ({failed:?})"),
        ),
        OsErrorClass::Io,
    )
}

impl MakesDirectories for NfsVolume {
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError> {
        self.to_remote_path(path)
    }

    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()> {
        Box::pin(self.mkdir(remote))
    }

    /// One walk that follows links, the `stat` the shared walk asks for.
    fn leads_to<'a>(&'a self, remote: &'a str) -> Walking<'a, LeadsTo> {
        Box::pin(async move {
            let client = self.client().await?;
            match self.resolve(&client, remote, Follow::Always, true).await {
                Ok(found)
                    if found
                        .attributes
                        .is_none_or(|attrs| attrs.file_type == FileType::Directory) =>
                {
                    Ok(LeadsTo::Directory)
                }
                Ok(_) => Ok(LeadsTo::NotADirectory),
                Err(VolumeError::NotFound(_)) => Ok(LeadsTo::Nothing),
                Err(VolumeError::NotADirectory(_)) => Ok(LeadsTo::NotADirectory),
                Err(err) => Err(err),
            }
        })
    }
}

impl PatchSource for NfsVolume {
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

#[cfg(test)]
#[path = "mutation_test.rs"]
mod mutation_test;
