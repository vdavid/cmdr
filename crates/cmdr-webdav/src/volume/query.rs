//! Reading what's on the server without changing it: PROPFIND, at depth 1 for
//! a listing and depth 0 for one resource.
use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{ListingProgress, SpaceInfo, VolumeError};
use tokio_util::sync::CancellationToken;

use super::WebdavVolume;
use super::mapping::{href_name, propfind_to_file_entry};
use super::paths::child_of;
use crate::errors::{Attempted, map_status, map_transport_error};
use crate::propfind::PropfindEntry;
use crate::transport::{Depth, PropfindOutcome, WebdavClient};

impl WebdavVolume {
    /// One PROPFIND at `remote`, in the `Volume` vocabulary.
    pub(super) async fn propfind(
        &self,
        client: &WebdavClient,
        remote: &str,
        collection: bool,
        depth: Depth,
    ) -> Result<Vec<PropfindEntry>, VolumeError> {
        let url = client.url_for(remote, collection);
        match client
            .propfind(url, depth)
            .await
            .map_err(|e| map_transport_error(&e, self.volume_id(), remote))?
        {
            PropfindOutcome::Entries(entries) => Ok(entries),
            PropfindOutcome::NotMultistatus => Err(VolumeError::IoError {
                message: "the server answered 207 without a multistatus body".to_string(),
                raw_os_error: None,
            }),
            PropfindOutcome::Status(status) => Err(map_status(status, remote, Attempted::Reaching)),
        }
    }

    /// One `Depth: 1` PROPFIND, with the collection's own entry left out.
    ///
    /// ❗ The self entry is found by comparing decoded paths with the trailing
    /// slash normalized away, ❌ never by position: RFC 4918 promises no order.
    pub(super) async fn list_directory_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let props = self.propfind(&client, &remote, true, Depth::One).await?;
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(VolumeError::Cancelled(remote));
        }
        let own = format!(
            "{}{}",
            client.base_path().trim_end_matches('/'),
            remote.trim_end_matches('/')
        );
        let mut entries = Vec::with_capacity(props.len());
        let mut tally = ListingProgress::default();
        for prop in &props {
            let href = prop.href.trim_end_matches('/');
            if href == own || href.is_empty() {
                continue;
            }
            let name = href_name(&prop.href);
            if name.is_empty() {
                continue;
            }
            let built = propfind_to_file_entry(name, &child_of(&remote, name), prop);
            if built.is_directory {
                tally.dirs += 1;
            } else {
                tally.files += 1;
                tally.bytes += built.size.unwrap_or(0);
            }
            entries.push(built);
        }
        // One report for the whole listing, ❌ never one per entry.
        if let Some(on_progress) = on_progress {
            on_progress(tally);
        }
        Ok(entries)
    }

    /// One `Depth: 0` PROPFIND, as a `FileEntry`.
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let prop = self.stat(&client, &remote).await?;
        let name = Path::new(&remote)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone());
        Ok(propfind_to_file_entry(&name, &remote, &prop))
    }

    /// The one entry a `Depth: 0` PROPFIND answers with.
    pub(super) async fn stat(&self, client: &WebdavClient, remote: &str) -> Result<PropfindEntry, VolumeError> {
        let is_root = remote == self.to_remote_path(Path::new("/"))?;
        self.propfind(client, remote, is_root, Depth::Zero)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| VolumeError::NotFound(remote.to_string()))
    }

    /// Whether `path` is there, as a plain yes/no.
    pub(super) async fn exists_impl(&self, path: &Path) -> bool {
        self.get_metadata_impl(path).await.is_ok()
    }


    /// Space on the root, as far as the server is willing to say.
    ///
    /// Three answers, because servers give three:
    ///
    /// - **Both RFC 4331 numbers, both non-negative** ⇒ a quota'd account.
    ///   [`SpaceInfo::Bounded`], with the total built from the pair (RFC 4331
    ///   has no capacity property, so `available + used` IS the total). ❗ The
    ///   used figure here is `quota-used-bytes` and ❌ never `oc:size`, so the
    ///   three numbers stay one self-consistent set: a total assembled from one
    ///   source and a used figure from another can't be trusted to add up.
    /// - **No usable `quota-available-bytes`** ⇒ an account with no quota,
    ///   which is what a stock Nextcloud user is and so the common case.
    ///   sabre/dav answers `-3` there (`SPACE_UNLIMITED`); `-1` and `-2` are its
    ///   other sentinels, and a server that simply omits the property lands here
    ///   too. All mean the same thing: no ceiling. ❌ Reading the sentinel as a
    ///   size would put a nonsense figure under the user's pane. The used figure
    ///   comes from `oc:size`, falling back to `quota-used-bytes`, and
    ///   [`SpaceInfo::Unbounded`] shows it.
    /// - **Neither figure usable** ⇒ nothing to show. `NotSupported`. Apache
    ///   `mod_dav` is here: it sends none of these properties.
    ///
    /// ❗ `oc:size` leads in the unbounded branch because Nextcloud answers
    /// `quota-used-bytes: 0` for an unlimited account while `oc:size` reports
    /// what it actually holds (verified against the fixture's own Nextcloud,
    /// 2026-09-03: both accounts hold the same ~65 MB skeleton, and the
    /// unlimited one reports `quota-used-bytes: 0` next to `oc:size: 64934262`).
    /// Trusting the standard alone there renders "0 bytes used" for an account
    /// full of files.
    pub(super) async fn get_space_info_impl(&self) -> Result<SpaceInfo, VolumeError> {
        let root = self.to_remote_path(Path::new("/"))?;
        let client = self.clone_client().await?;
        let prop = self.stat(&client, &root).await?;
        let quota_used = prop.quota_used.filter(|used| *used >= 0).map(i64::unsigned_abs);
        match prop.quota_available.filter(|available| *available >= 0) {
            Some(available) => {
                let Some(used) = quota_used else {
                    return Err(VolumeError::NotSupported);
                };
                let available = available.unsigned_abs();
                Ok(SpaceInfo::Bounded {
                    total_bytes: available.saturating_add(used),
                    available_bytes: available,
                    used_bytes: used,
                })
            }
            None => {
                let Some(used) = prop.oc_size.or(quota_used) else {
                    return Err(VolumeError::NotSupported);
                };
                Ok(SpaceInfo::Unbounded { used_bytes: used })
            }
        }
    }
}
