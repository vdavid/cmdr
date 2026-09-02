//! Changing what's on the server: PUT, MKCOL, DELETE, and MOVE.
//!
//! Every refusal is the SERVER's, ❌ never a check of ours: `If-None-Match: *`
//! and `Overwrite: F` are preconditions the server evaluates atomically, where
//! a stat-then-write would be a TOCTOU window.

use std::path::Path;

use cmdr_fs::pluralize::pluralize_with;
use cmdr_fs::volume::{DirectoryCreation, MutationEvent, VolumeError};
use log::debug;
use reqwest::Method;
use reqwest::header::IF_NONE_MATCH;

use super::WebdavVolume;
use crate::errors::Attempted;
use crate::transport::{Depth, MUTATION_BUDGET, WebdavClient, method};

/// `ENOTEMPTY`, which POSIX numbers differently per platform. The number is
/// what the app renders "this folder still has something in it" from.
#[cfg(target_os = "linux")]
pub(crate) const ENOTEMPTY: i32 = 39;
/// `ENOTEMPTY` on everything else Cmdr builds for.
#[cfg(not(target_os = "linux"))]
pub(crate) const ENOTEMPTY: i32 = 66;

impl WebdavVolume {
    /// A PUT that only lands on a name nothing holds.
    pub(super) async fn create_file_impl(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let request = client
            .request(Method::PUT, client.url_for(&remote, false))
            .header(IF_NONE_MATCH, "*")
            .body(content.to_vec())
            .timeout(MUTATION_BUDGET);
        self.send(request, &remote, Attempted::TakingAName).await?;
        self.notify_created(path).await;
        Ok(())
    }

    /// One MKCOL.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        self.mkcol(&client, &remote).await?;
        self.notify_created(path).await;
        Ok(())
    }

    async fn mkcol(&self, client: &WebdavClient, remote: &str) -> Result<(), VolumeError> {
        let request = client
            .request(method("MKCOL"), client.url_for(remote, true))
            .timeout(MUTATION_BUDGET);
        self.send(request, remote, Attempted::TakingAName).await.map(|_| ())
    }

    /// Leaf first, then the ancestors only if the leaf's parent was missing.
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let root = self.to_remote_path(Path::new("/"))?;
        if remote == root {
            return Ok(DirectoryCreation::AlreadyExisted);
        }
        match self.mkcol(&client, &remote).await {
            Ok(()) => {
                self.notify_created(path).await;
                return Ok(DirectoryCreation::Created);
            }
            Err(VolumeError::AlreadyExists(_)) => return Ok(DirectoryCreation::AlreadyExisted),
            Err(VolumeError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
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
            match self.mkcol(&client, dir).await {
                Ok(()) => {
                    first_created.get_or_insert(as_addressed);
                    if index == 0 {
                        leaf = DirectoryCreation::Created;
                    }
                }
                Err(VolumeError::AlreadyExists(_)) => {}
                Err(e) => return Err(e),
            }
        }
        // ❗ ONE patch, for the SHALLOWEST directory this created: its parent is
        // the only listing a pane could be holding.
        if let Some(created) = first_created {
            self.notify_created(created).await;
        }
        Ok(leaf)
    }

    /// One DELETE. ❗ A collection that still holds something is refused with
    /// `ENOTEMPTY` before the request goes out, because the protocol's DELETE is
    /// recursive and the trait's is not.
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let listing = self.propfind(&client, &remote, false, Depth::One).await?;
        // ❗ Judged by COUNT, ❌ never by which entry comes first: RFC 4918
        // promises no order, and a child read as "the resource" would send a
        // recursive DELETE at a folder with things in it. A `Depth: 1` on a
        // file or an empty collection answers exactly one response.
        if listing.len() > 1 {
            return Err(VolumeError::IoError {
                message: format!(
                    "{remote} still holds {}",
                    pluralize_with((listing.len() - 1) as u64, "entry", "entries")
                ),
                raw_os_error: Some(ENOTEMPTY),
            });
        }
        let is_collection = listing.first().is_some_and(|p| p.is_collection);
        let request = client
            .request(Method::DELETE, client.url_for(&remote, is_collection))
            .timeout(MUTATION_BUDGET);
        self.send(request, &remote, Attempted::Reaching).await?;
        self.notify_deleted(path).await;
        Ok(())
    }

    /// One MOVE. `Overwrite: F` unless forced, so the server refuses an
    /// occupied destination atomically (412 → `AlreadyExists`).
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        let client = self.clone_client().await?;
        let is_collection = self.stat(&client, &remote_from).await?.is_collection;
        debug!("WebdavVolume::rename: {remote_from} -> {remote_to} (force={force})");
        let request = client
            .request(method("MOVE"), client.url_for(&remote_from, is_collection))
            .header("Destination", client.url_for(&remote_to, is_collection).as_str())
            .header("Overwrite", if force { "T" } else { "F" })
            .timeout(MUTATION_BUDGET);
        let attempted = if force {
            Attempted::Reaching
        } else {
            Attempted::TakingAName
        };
        self.send(request, &remote_to, attempted).await?;
        self.notify_renamed(from, to).await;
        Ok(())
    }

    // ── Listing-cache patches ────────────────────────────────────────

    async fn notify_created(&self, path: &Path) {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let Some(parent) = self.display_path_for(parent) else {
            return;
        };
        self.notify_mutation_impl(&parent, MutationEvent::Created(name.to_string_lossy().into_owned()))
            .await;
    }

    async fn notify_deleted(&self, path: &Path) {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let Some(parent) = self.display_path_for(parent) else {
            return;
        };
        self.notify_mutation_impl(&parent, MutationEvent::Deleted(name.to_string_lossy().into_owned()))
            .await;
    }

    async fn notify_renamed(&self, from: &Path, to: &Path) {
        let (Some(from_parent), Some(from_name), Some(to_parent), Some(to_name)) =
            (from.parent(), from.file_name(), to.parent(), to.file_name())
        else {
            return;
        };
        let (Some(from_parent), Some(to_parent)) =
            (self.display_path_for(from_parent), self.display_path_for(to_parent))
        else {
            return;
        };
        let (from_name, to_name) = (
            from_name.to_string_lossy().into_owned(),
            to_name.to_string_lossy().into_owned(),
        );
        if from_parent == to_parent {
            self.notify_mutation_impl(
                &from_parent,
                MutationEvent::Renamed {
                    from: from_name,
                    to: to_name,
                },
            )
            .await;
        } else {
            self.notify_mutation_impl(&from_parent, MutationEvent::Deleted(from_name))
                .await;
            self.notify_mutation_impl(&to_parent, MutationEvent::Created(to_name))
                .await;
        }
    }

    /// Patches the listing cache for one change under `parent_path`. ❗ There is
    /// no watcher here, so this is the ONLY thing that keeps a pane honest.
    pub(super) async fn notify_mutation_impl(&self, parent_path: &Path, mutation: MutationEvent) {
        use cmdr_fs::volume::DirectoryChange;

        let listings = self.inner.host.listings();
        let volume_id = self.volume_id();
        match mutation {
            MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                let Ok(entry) = self.get_metadata_impl(&parent_path.join(name)).await else {
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
