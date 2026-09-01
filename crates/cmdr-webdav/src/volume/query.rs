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

    /// One PROPFIND, reduced to the collection bit.
    pub(super) async fn is_directory_impl(&self, path: &Path) -> Result<bool, VolumeError> {
        Ok(self.get_metadata_impl(path).await?.is_directory)
    }

    /// RFC 4331 quota on the root. `NotSupported` unless the server reports
    /// both numbers and neither is the "unlimited" sentinel Nextcloud spells
    /// as a negative value.
    pub(super) async fn get_space_info_impl(&self) -> Result<SpaceInfo, VolumeError> {
        let root = self.to_remote_path(Path::new("/"))?;
        let client = self.clone_client().await?;
        let prop = self.stat(&client, &root).await?;
        match (prop.quota_available, prop.quota_used) {
            (Some(available), Some(used)) if available >= 0 && used >= 0 => {
                let available = available.unsigned_abs();
                let used = used.unsigned_abs();
                Ok(SpaceInfo {
                    total_bytes: available.saturating_add(used),
                    available_bytes: available,
                    used_bytes: used,
                })
            }
            _ => Err(VolumeError::NotSupported),
        }
    }
}
