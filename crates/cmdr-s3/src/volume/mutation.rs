//! Folders, delete, and rename on S3, where a folder is a key prefix and maybe
//! a zero-byte `name/` marker.
//!
//! - **A folder exists** when it has a marker OR any key under its prefix, and
//!   a folder wins over an object of the same name, except for a file the
//!   listing showed as `<name> (file)`: its row names only the file, and the
//!   folder's row only the folder (`paths.rs`).
//! - **`create_directory` writes the marker** (conditional where allowlisted,
//!   else after a check) and refuses an occupied name or a missing parent the
//!   way `mkdir` does, so the shared `mkdir -p` walk works unchanged.
//! - **`delete` is one node**: an object, or a folder's marker. ❗ A folder
//!   still holding anything is refused with `ENOTEMPTY`, because `DeleteObject`
//!   on keys under it would be the recursion the trait forbids; the engine walks
//!   the children itself.
//! - **`rename` is one small file**: a server-side copy, verified, then the
//!   delete of the source. ❗ Everything else is
//!   [`RenameWork::CopyThenDelete`] (`rename_work`): a folder, and a file past
//!   the part floor, whose copy needs progress and cancel. Every caller asks
//!   first and sends those through the transfer engine as a move; `rename`
//!   itself still refuses them with `NotSupported`, so nothing copies a whole
//!   folder by accident.

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::mkdir_all::{self, LeadsTo, MakesDirectories};
use cmdr_fs::volume::patching::{PatchSource, patch_created, patch_deleted, patch_renamed};
use cmdr_fs::volume::scan_walk::Walking;
use cmdr_fs::volume::{DirectoryCreation, RenameWork, VolumeError, WriteMode};
use log::{debug, warn};

use super::S3Volume;
use super::errors::map_s3_error;
use super::listing::{FolderContents, can_hold_keys, folder_contents};
use super::paths::{Holder, Resolved, Target, target_of};
use super::query::body_error;
use super::writes::{Landed, Landing, judge_landing, refuse_unstorable};
use crate::error::S3Error;
use crate::ops::{self, BuildError, CopySource, ListObjectsParams, MetadataDirective, ObjectMetadata, Overwrite};
use crate::transport::S3Client;
use crate::xml::{parse_copy_result, parse_list_objects};

/// `ENOTEMPTY`, which POSIX numbers differently per platform. The number is
/// what the app renders "this folder still has something in it" from.
#[cfg(target_os = "linux")]
pub(crate) const ENOTEMPTY: i32 = 39;
/// `ENOTEMPTY` on everything else Cmdr builds for.
#[cfg(not(target_os = "linux"))]
pub(crate) const ENOTEMPTY: i32 = 66;

/// What holds a name, folder first (the listing's rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameHolds {
    Folder,
    File,
    Nothing,
}

impl S3Volume {
    /// What holds `key` in `bucket`: a folder (a marker or any key under the
    /// prefix), else an object, else nothing. One capped listing, plus a HEAD
    /// only when that finds no folder.
    pub(super) async fn name_holds(
        &self,
        client: &S3Client,
        bucket: &str,
        key: &str,
        remote: &str,
    ) -> Result<NameHolds, VolumeError> {
        if self.has_keys_under(client, bucket, key, remote).await? {
            return Ok(NameHolds::Folder);
        }
        Ok(match self.head_object(client, bucket, key, remote).await? {
            Some(_) => NameHolds::File,
            None => NameHolds::Nothing,
        })
    }

    /// New folder: the zero-byte `name/` marker, refused like `mkdir` refuses.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        if let Target::Key { key, .. } = target_of(&remote) {
            refuse_unstorable(&client, key, &remote)?;
        }
        self.make_folder(&client, &remote).await?;
        patch_created(self, path).await;
        Ok(())
    }

    /// `mkdir -p`, through the shared walk, which asks [`MakesDirectories`].
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let made = mkdir_all::create_directory_all(self, path).await?;
        if let Some(created) = made.shallowest_created {
            patch_created(self, &created).await;
        }
        Ok(made.leaf)
    }

    /// One folder at `remote`: `AlreadyExists` when a folder or a file holds
    /// the name, `NotFound` when its parent is no folder, `NotADirectory` when a
    /// file holds the parent's name. A bucket can't be made here.
    async fn make_folder(&self, client: &S3Client, remote: &str) -> Result<(), VolumeError> {
        let (bucket, key) = match target_of(remote) {
            Target::Account => return Err(VolumeError::AlreadyExists(remote.to_string())),
            Target::Bucket(bucket) => {
                let request = ops::head_bucket(client.profile(), bucket).map_err(|_| VolumeError::NotSupported)?;
                return match self.ask(client, request, remote).await {
                    Ok(_) => Err(VolumeError::AlreadyExists(remote.to_string())),
                    // Creating buckets isn't something a pane does.
                    Err(VolumeError::NotFound(_)) => Err(VolumeError::NotSupported),
                    Err(other) => Err(other),
                };
            }
            Target::Key { bucket, key } => (bucket, key),
        };
        if self.name_holds(client, bucket, key, remote).await? != NameHolds::Nothing {
            return Err(VolumeError::AlreadyExists(remote.to_string()));
        }
        // The parent has to be a folder already, as `mkdir` wants it: ❗ a
        // marker under a FILE's name would turn that file into a folder in
        // every listing, hiding it.
        if let Some((parent_key, _)) = key.rsplit_once('/') {
            let parent_remote = format!("/{bucket}/{parent_key}");
            match self.name_holds(client, bucket, parent_key, &parent_remote).await? {
                NameHolds::Folder => {}
                NameHolds::File => return Err(VolumeError::NotADirectory(parent_remote)),
                NameHolds::Nothing => return Err(VolumeError::NotFound(remote.to_string())),
            }
        }
        let marker = format!("{key}/");
        let built = ops::put_object(
            client.profile(),
            bucket,
            &marker,
            0,
            Overwrite::Refuse,
            &ObjectMetadata::default(),
        )
        .map_err(|_| VolumeError::NotFound(remote.to_string()))?;
        let mut request = built.request;
        request.body = crate::request::Body::Bytes(Vec::new());
        // `check_first` is already answered: nothing held the name above.
        let answer = client
            .exchange(request, crate::transport::QUERY_BUDGET)
            .await
            .map_err(|e| crate::transport::map_exchange_error(&e, self.volume_id(), remote))?;
        if answer.status.is_success() {
            debug!(target: "volume", "s3 folder marker written for {remote}");
            return Ok(());
        }
        let error = S3Error::from_response(answer.status, &answer.text());
        self.note_refused_condition(client, &error, crate::profile::ConditionalOp::Put, !built.check_first);
        Err(map_s3_error(&error, remote))
    }

    /// One node: an object, or a folder's own marker. ❗ A folder still holding
    /// anything is refused, never recursed into.
    ///
    /// A `<name> (file)` row deletes only its file, and the folder row beside
    /// it only the folder's marker: ❗ a folder whose last key went is never
    /// mistaken for the file of its name (`paths.rs`).
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let Resolved { remote, holder } = self.resolve(path)?;
        let Target::Key { bucket, key } = target_of(&remote) else {
            // Deleting a bucket, or the account, isn't a file operation.
            return Err(VolumeError::NotSupported);
        };
        let client = self.clone_client().await?;
        if holder == Holder::File {
            if self.head_object(&client, bucket, key, &remote).await?.is_none() {
                return Err(VolumeError::NotFound(remote));
            }
            self.delete_key(&client, bucket, key, &remote).await?;
            self.forget_beside_folder(&remote);
            patch_deleted(self, path).await;
            return Ok(());
        }
        let prefix = format!("{key}/");
        let wire_prefix = client.profile().normalize_key(&prefix).into_owned();
        let contents = if can_hold_keys(&wire_prefix) {
            let params = ListObjectsParams {
                prefix: &prefix,
                delimiter: Some("/"),
                continuation_token: None,
                max_keys: Some(2),
            };
            let request = ops::list_objects(client.profile(), bucket, &params)
                .map_err(|_| VolumeError::NotFound(remote.clone()))?;
            let answer = self.ask(&client, request, &remote).await?;
            let page = parse_list_objects(&answer.text()).map_err(|e| body_error(&e, &remote))?;
            folder_contents(&page, &wire_prefix)
        } else {
            FolderContents::Nothing
        };
        let target_key = match contents {
            FolderContents::Holds => {
                return Err(VolumeError::IoError {
                    message: format!("{remote} still holds something"),
                    raw_os_error: Some(ENOTEMPTY),
                });
            }
            FolderContents::MarkerOnly => prefix,
            FolderContents::Nothing if holder == Holder::Folder => return Err(VolumeError::NotFound(remote)),
            FolderContents::Nothing => {
                if self.head_object(&client, bucket, key, &remote).await?.is_none() {
                    return Err(VolumeError::NotFound(remote));
                }
                key.to_string()
            }
        };
        self.delete_key(&client, bucket, &target_key, &remote).await?;
        patch_deleted(self, path).await;
        Ok(())
    }

    /// One `DeleteObject`.
    pub(super) async fn delete_key(
        &self,
        client: &S3Client,
        bucket: &str,
        key: &str,
        remote: &str,
    ) -> Result<(), VolumeError> {
        let request =
            ops::delete_object(client.profile(), bucket, key).map_err(|_| VolumeError::NotFound(remote.to_string()))?;
        self.ask(client, request, remote).await.map(|_| ())
    }

    /// A small file's rename: `CopyObject` (refusing an occupied name unless
    /// `force`), a HEAD proving the copy is ours, then the source's delete. ❗
    /// The source goes only after the copy is verified, so the worst a failure
    /// leaves is two copies, never none.
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let Resolved {
            remote: remote_from,
            holder,
        } = self.resolve(from)?;
        if holder == Holder::Folder {
            // ❗ Never the file of its name: a folder is the engine's job.
            return Err(VolumeError::NotSupported);
        }
        let remote_to = self.to_remote_path(to)?;
        let (
            Target::Key {
                bucket: from_bucket,
                key: from_key,
            },
            Target::Key {
                bucket: to_bucket,
                key: to_key,
            },
        ) = (target_of(&remote_from), target_of(&remote_to))
        else {
            return Err(VolumeError::NotSupported);
        };
        if remote_from == remote_to {
            return Ok(());
        }
        let client = self.clone_client().await?;
        refuse_unstorable(&client, to_key, &remote_to)?;
        let Some(head) = self.head_object(&client, from_bucket, from_key, &remote_from).await? else {
            return Err(
                if holder == Holder::Either
                    && self
                        .has_keys_under(&client, from_bucket, from_key, &remote_from)
                        .await?
                {
                    // A folder is one copy and one delete per object: the
                    // transfer engine's job (`rename_work`).
                    VolumeError::NotSupported
                } else {
                    VolumeError::NotFound(remote_from)
                },
            );
        };
        let size: u64 = head.object_length().unwrap_or(0);
        if !self.copies_whole(size) {
            return Err(VolumeError::NotSupported);
        }
        if !force && self.name_holds(&client, to_bucket, to_key, &remote_to).await? != NameHolds::Nothing {
            return Err(VolumeError::AlreadyExists(remote_to));
        }
        let overwrite = if force { Overwrite::Replace } else { Overwrite::Refuse };
        let source = CopySource {
            bucket: from_bucket,
            key: from_key,
        };
        let built = match ops::copy_object(
            client.profile(),
            source,
            None,
            to_bucket,
            to_key,
            overwrite,
            &MetadataDirective::Copy,
        ) {
            Ok(built) => built,
            // A provider that copies within one bucket only (Spaces).
            Err(BuildError::CrossBucketCopy) => return Err(VolumeError::NotSupported),
            Err(_) => return Err(VolumeError::NotFound(remote_to)),
        };
        let answer = self.ask(&client, built.request, &remote_to).await?;
        // ❗ A copy can fail inside a 200.
        let copied = parse_copy_result(&answer.text()).map_err(|e| body_error(&e, &remote_to))?;
        let mode = if force {
            WriteMode::CreateOrReplace
        } else {
            WriteMode::CreateNew
        };
        let landed = self
            .head_object(&client, to_bucket, to_key, &remote_to)
            .await?
            .map(|answer| Landed {
                size: answer.object_length(),
                etag: answer.header("etag").map(str::to_string),
            });
        match judge_landing(size, copied.etag.as_deref(), landed.as_ref(), mode) {
            Landing::Verified => {}
            Landing::TakenAfterUs => {
                warn!(target: "volume", "s3: another writer took {remote_to} during a rename; the source stays");
                return Err(VolumeError::AlreadyExists(remote_to));
            }
            other => {
                return Err(VolumeError::IoError {
                    message: format!("{remote_to}: the renamed copy isn't what was sent ({other:?}); the source stays"),
                    raw_os_error: None,
                });
            }
        }
        if let Err(e) = self.delete_key(&client, from_bucket, from_key, &remote_from).await {
            warn!(target: "volume", "s3: {remote_from} was copied to {remote_to} but its delete failed: {e}");
            return Err(e);
        }
        self.forget_beside_folder(&remote_from);
        patch_renamed(self, from, to).await;
        Ok(())
    }

    /// Whether renaming `path` is one call here: an object up to the part
    /// floor is (`CopyObject`, then a delete); a folder, or a bigger object,
    /// is a copy the transfer engine runs. The account and a bucket's top
    /// can't be renamed at all, which `rename` itself says.
    pub(super) async fn rename_work_impl(&self, path: &Path) -> Result<RenameWork, VolumeError> {
        let Resolved { remote, holder } = self.resolve(path)?;
        let Target::Key { bucket, key } = target_of(&remote) else {
            return Ok(RenameWork::OneCall);
        };
        let client = self.clone_client().await?;
        if holder != Holder::File && self.has_keys_under(&client, bucket, key, &remote).await? {
            return Ok(RenameWork::CopyThenDelete);
        }
        if holder == Holder::Folder {
            return Err(VolumeError::NotFound(remote));
        }
        let Some(head) = self.head_object(&client, bucket, key, &remote).await? else {
            return Err(VolumeError::NotFound(remote));
        };
        let size: u64 = head.object_length().unwrap_or(u64::MAX);
        Ok(if self.copies_whole(size) {
            RenameWork::OneCall
        } else {
            RenameWork::CopyThenDelete
        })
    }

    /// The path the app addresses `path` by.
    fn display_path_for(&self, path: &Path) -> Option<PathBuf> {
        self.root
            .to_remote_path(path)
            .and_then(|remote| self.root.to_app_path(&remote))
    }
}

/// What the shared `mkdir -p` walk needs: the marker write, and what a name
/// leads to. S3 has no links, so a folder is a folder.
impl MakesDirectories for S3Volume {
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError> {
        self.to_remote_path(path)
    }

    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()> {
        Box::pin(async move {
            let client = self.clone_client().await?;
            self.make_folder(&client, remote).await
        })
    }

    fn leads_to<'a>(&'a self, remote: &'a str) -> Walking<'a, LeadsTo> {
        Box::pin(async move {
            let Target::Key { bucket, key } = target_of(remote) else {
                return Ok(LeadsTo::Directory);
            };
            let client = self.clone_client().await?;
            Ok(match self.name_holds(&client, bucket, key, remote).await? {
                NameHolds::Folder => LeadsTo::Directory,
                NameHolds::File => LeadsTo::NotADirectory,
                NameHolds::Nothing => LeadsTo::Nothing,
            })
        })
    }
}

/// What the shared listing-cache patcher needs. ❗ There's no watcher here, so
/// a patch is the ONLY thing that keeps a pane honest after a write.
impl PatchSource for S3Volume {
    fn patch_volume_id(&self) -> &str {
        self.volume_id()
    }

    fn patch_listings(&self) -> &dyn ListingHost {
        self.inner.host.listings()
    }

    /// The entry a write just verified, when it's the one asked for, so the
    /// patch after an upload costs no second HEAD; else a fresh stat.
    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(async move {
            if let Ok(remote) = self.to_remote_path(path)
                && let Some(entry) = self.take_written(&remote)
            {
                return Ok(entry);
            }
            self.get_metadata_impl(path).await
        })
    }

    fn patch_display_path(&self, path: &Path) -> Option<PathBuf> {
        self.display_path_for(path)
    }
}
