//! Changing what's on the server: PUT, MKCOL, DELETE, MOVE, and a folder's dates (PROPPATCH).
//!
//! Every refusal is the SERVER's, ❌ never a check of ours: `If-None-Match: *`
//! and `Overwrite: F` are preconditions the server evaluates atomically, where
//! a stat-then-write would be a TOCTOU window.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::pluralize::pluralize_with;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::mkdir_all::{self, LeadsTo, MakesDirectories};
use cmdr_fs::volume::patching::{PatchSource, patch_created, patch_deleted, patch_modified, patch_renamed};
use cmdr_fs::volume::scan_walk::Walking;
use cmdr_fs::volume::{DirectoryCreation, VolumeError};
use log::debug;
use reqwest::Method;
use reqwest::header::IF_NONE_MATCH;

use super::WebdavVolume;
use crate::errors::{Attempted, map_status, map_transport_error};
use crate::propfind::PropfindEntry;
use crate::transport::{Depth, MUTATION_BUDGET, ProppatchOutcome, WebdavClient, method};

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
        self.send(&client, request, &remote, Attempted::TakingAName).await?;
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

    /// A folder's modification and creation dates in ONE PROPPATCH
    /// (`d:lastmodified`, Nextcloud's `nc:creation_time`), whole seconds. `true`
    /// when the server applied every property it was sent. ❗ Only for a server
    /// that keeps dates: `mod_dav` answers 207 and stores them as dead properties.
    pub(super) async fn set_dates_impl(
        &self,
        path: &Path,
        modified: Option<SystemTime>,
        created: Option<SystemTime>,
    ) -> Result<bool, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let secs = |at: SystemTime| at.duration_since(UNIX_EPOCH).ok().map(|since| since.as_secs());
        let mut props = String::new();
        if let Some(secs) = modified.and_then(secs) {
            props.push_str(&format!("<d:lastmodified>{secs}</d:lastmodified>"));
        }
        if let Some(secs) = created.and_then(secs) {
            props.push_str(&format!("<nc:creation_time>{secs}</nc:creation_time>"));
        }
        if props.is_empty() {
            // Before 1970: nothing the properties can say.
            return Ok(false);
        }
        let client = self.clone_client().await?;
        debug!("WebdavVolume::set_dates: {remote}", remote = LogPrivate::path(&remote));
        let applied = match client
            .proppatch(client.url_for(&remote, true), &props)
            .await
            .map_err(|e| map_transport_error(&e, self.volume_id(), &remote))?
        {
            ProppatchOutcome::Multistatus(applied) => applied,
            ProppatchOutcome::Unreadable => false,
            ProppatchOutcome::Status(status) if status.is_success() => true,
            ProppatchOutcome::Status(status) => return Err(map_status(status, &remote, Attempted::Reaching)),
        };
        if applied {
            patch_modified(self, path).await;
        }
        Ok(applied)
    }

    async fn mkcol(&self, client: &WebdavClient, remote: &str) -> Result<(), VolumeError> {
        let request = client
            .request(method("MKCOL"), client.url_for(remote, true))
            .timeout(MUTATION_BUDGET);
        self.send(client, request, remote, Attempted::TakingAName)
            .await
            .map(|_| ())
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

    /// One DELETE for a file. ❗ A collection that still holds something is
    /// refused with `ENOTEMPTY` before anything changes, because the protocol's
    /// DELETE is recursive and the trait's is not, and an empty one is moved
    /// aside before its DELETE so nothing can land in it on the way
    /// (`delete_aside.rs`).
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let listing = self.propfind(&client, &remote, false, Depth::One).await?;
        refuse_if_held(&listing, &remote)?;
        if listing.first().is_some_and(|p| p.is_collection) {
            self.delete_collection(Arc::clone(&client), &remote).await?;
        } else {
            let request = client
                .request(Method::DELETE, client.url_for(&remote, false))
                .timeout(MUTATION_BUDGET);
            self.send(&client, request, &remote, Attempted::Reaching).await?;
        }
        patch_deleted(self, path).await;
        Ok(())
    }

    /// One MOVE. `Overwrite: F` unless forced, so the server refuses an
    /// occupied destination atomically (412 → `AlreadyExists`).
    ///
    /// ❗ A forced MOVE deletes a destination collection, tree and all, before
    /// it moves anything (RFC 4918 § 9.9.3), so one that holds something is
    /// refused with `ENOTEMPTY` first (`Volume::rename`), judged by the same
    /// count as `delete`'s. An empty destination folder is then moved aside
    /// the same way, so a child landing after the count is never taken
    /// (`delete_aside.rs`).
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        let client = self.clone_client().await?;
        let is_collection = self.stat(&client, &remote_from).await?.is_collection;
        let mut onto_a_folder = false;
        if force && remote_from != remote_to {
            match self.propfind(&client, &remote_to, false, Depth::One).await {
                Ok(listing) => {
                    refuse_if_held(&listing, &remote_to)?;
                    onto_a_folder = listing.first().is_some_and(|p| p.is_collection);
                }
                Err(VolumeError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        debug!(
            "WebdavVolume::rename: {remote_from} -> {remote_to} (force={force})",
            remote_from = LogPrivate::path(&remote_from),
            remote_to = LogPrivate::path(&remote_to)
        );
        if onto_a_folder {
            self.replace_collection(Arc::clone(&client), &remote_from, &remote_to, is_collection)
                .await?;
            patch_renamed(self, from, to).await;
            return Ok(());
        }
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
        self.send(&client, request, &remote_to, attempted).await?;
        patch_renamed(self, from, to).await;
        Ok(())
    }
}

/// `ENOTEMPTY` when a `Depth: 1` listing of `remote` shows anything under it,
/// for the two verbs the protocol makes recursive (DELETE, a forced MOVE).
///
/// ❗ Judged by COUNT, ❌ never by which entry comes first: RFC 4918 promises no
/// order, and a child read as "the resource" would send a recursive verb at a
/// folder with things in it. A `Depth: 1` on a file or an empty collection
/// answers exactly one response.
fn refuse_if_held(listing: &[PropfindEntry], remote: &str) -> Result<(), VolumeError> {
    if listing.len() <= 1 {
        return Ok(());
    }
    Err(still_holds(remote, listing.len() - 1))
}

/// The `ENOTEMPTY` a recursive verb is refused with, `held` entries counted.
pub(super) fn still_holds(remote: &str, held: usize) -> VolumeError {
    VolumeError::IoError {
        message: format!(
            "{remote} still holds {}",
            pluralize_with(held as u64, "entry", "entries")
        ),
        raw_os_error: Some(ENOTEMPTY),
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

    /// One `Depth: 0` PROPFIND. ❗ MKCOL's own answer can't say this: a taken
    /// name is 405 whether a file or a collection holds it. The protocol has no
    /// links of its own, and a server that follows one on its disk reports what
    /// it leads to.
    fn leads_to<'a>(&'a self, remote: &'a str) -> Walking<'a, LeadsTo> {
        Box::pin(async move {
            let client = self.clone_client().await?;
            match self.stat(&client, remote).await {
                Ok(entry) if entry.is_collection => Ok(LeadsTo::Directory),
                Ok(_) => Ok(LeadsTo::NotADirectory),
                Err(VolumeError::NotFound(_)) => Ok(LeadsTo::Nothing),
                Err(e) => Err(e),
            }
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
