//! Changing what's on the server: PUT, MKCOL, DELETE, and MOVE.
//!
//! Every refusal is the SERVER's, ❌ never a check of ours: `If-None-Match: *`
//! and `Overwrite: F` are preconditions the server evaluates atomically, where
//! a stat-then-write would be a TOCTOU window.

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::pluralize::pluralize_with;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::mkdir_all::{self, MakesDirectories};
use cmdr_fs::volume::patching::{PatchSource, patch_created, patch_deleted, patch_renamed};
use cmdr_fs::volume::scan_walk::Walking;
use cmdr_fs::volume::{DirectoryCreation, VolumeError};
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
        patch_created(self, path).await;
        Ok(())
    }

    /// One MKCOL.
    pub(super) async fn create_directory_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        self.mkcol(&client, &remote).await?;
        patch_created(self, path).await;
        Ok(())
    }

    async fn mkcol(&self, client: &WebdavClient, remote: &str) -> Result<(), VolumeError> {
        let request = client
            .request(method("MKCOL"), client.url_for(remote, true))
            .timeout(MUTATION_BUDGET);
        self.send(request, remote, Attempted::TakingAName).await.map(|_| ())
    }

    /// `mkdir -p`, through the shared walk: leaf first, ancestors only when the
    /// leaf's parent was missing. The honesty contract on the answer:
    /// `cmdr_fs::volume::mkdir_all`.
    pub(super) async fn create_directory_all_impl(&self, path: &Path) -> Result<DirectoryCreation, VolumeError> {
        let made = mkdir_all::create_directory_all(self, path).await?;
        if let Some(created) = made.shallowest_created {
            patch_created(self, &created).await;
        }
        Ok(made.leaf)
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
        patch_deleted(self, path).await;
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
        patch_renamed(self, from, to).await;
        Ok(())
    }
}

/// What the shared `mkdir -p` walk needs from this backend: MKCOL, and this
/// volume's own path spelling. The `Created` promise it answers with is spent by
/// the transfer driver, so the refusals matter: `cmdr_fs::volume::mkdir_all`.
impl MakesDirectories for WebdavVolume {
    fn remote_path_of(&self, path: &Path) -> Result<String, VolumeError> {
        self.to_remote_path(path)
    }

    fn make_one_directory<'a>(&'a self, remote: &'a str) -> Walking<'a, ()> {
        Box::pin(async move {
            let client = self.clone_client().await?;
            self.mkcol(&client, remote).await
        })
    }
}

/// What the shared listing-cache patcher needs from this backend. ❗ There is no
/// watcher here, so a patch is the ONLY thing that keeps a pane honest after a
/// write. The rules: `cmdr_fs::volume::patching`.
impl PatchSource for WebdavVolume {
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
