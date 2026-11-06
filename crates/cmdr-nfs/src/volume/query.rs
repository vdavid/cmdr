//! Reading what's on the export without changing it: listings (READDIRPLUS,
//! with READDIR + LOOKUP where the server has none), one entry's attributes,
//! the room left, and whether a folder takes writes.

use cmdr_fs::volume::Presence;
use std::path::Path;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::volume::{EntryKind, ListingProgress, SpaceInfo, UnwritableReason, VolumeError, WriteAccess};
use log::debug;
use tokio_util::sync::CancellationToken;

use super::NfsVolume;
use super::handles::{Follow, Stamp};
use super::mapping::{attrs_to_file_entry, entry_kind, fsstat_to_space_info};
use super::paths::child_of;
use super::reconnect::NfsClient;
use crate::errors::map_nfs_error;
use crate::proto::nfs3::{FileHandle, FileType, ListedEntry, ListingEvent, NfsError, NfsStat3, access};

/// The smallest and largest READDIRPLUS page asked for. Above the floor a
/// server's own `dtpref` decides, up to the ceiling: 128 KiB is about 600
/// entries with their attributes, so a big folder reports progress every page
/// rather than once at the end.
const PAGE_FLOOR: u32 = 8 * 1024;
const PAGE_CEILING: u32 = 128 * 1024;

/// What one listing has gathered so far.
#[derive(Default)]
struct Gathered {
    entries: Vec<FileEntry>,
    tally: ListingProgress,
    /// Names that aren't UTF-8: listed nowhere, counted, logged once. A lossy
    /// name would point every later call at bytes that don't exist.
    skipped: usize,
    /// The folder's own `(mtime, ctime)` as the listing's first page saw it,
    /// when the server sent it: what the poller compares against.
    stamp: Option<Stamp>,
}

impl NfsVolume {
    /// The whole listing of the folder `path` names, batch by batch into
    /// `on_progress`.
    pub(super) async fn list_directory_impl(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        Ok(self.list_stamped(path, on_progress, cancel).await?.0)
    }

    /// [`list_directory_impl`](Self::list_directory_impl), plus the folder's
    /// stamp as the listing's first page saw it.
    async fn list_stamped(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<(Vec<FileEntry>, Option<Stamp>), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.client().await?;
        let found = self.resolve(&client, &remote, Follow::Always, false).await?;
        let gathered = match self
            .list_into(&client, &found.handle, &found.real, &remote, on_progress, cancel)
            .await
        {
            Err(NfsError::Status(NfsStat3::Stale)) => {
                // The folder was replaced under its name: once more, fresh.
                self.inner.handles.forget_subtree(&found.real);
                let found = self.resolve(&client, &remote, Follow::Always, false).await?;
                self.list_into(&client, &found.handle, &found.real, &remote, on_progress, cancel)
                    .await
            }
            other => other,
        };
        let gathered = match gathered {
            Ok(gathered) => gathered,
            Err(err) => return Err(map_nfs_error(&err, &remote)),
        };
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return Err(VolumeError::Cancelled(ErrorDetail::path(remote)));
        }
        if gathered.skipped > 0 {
            debug!(target: "nfs", "left {} name(s) that aren't UTF-8 out of a listing", gathered.skipped);
        }
        Ok((gathered.entries, gathered.stamp))
    }

    /// A listing a PANE asked for: the poller's cue that something on this
    /// volume is being shown. ❗ A scan lists through `list_directory_impl`
    /// directly, so a copy's walk never starts polling.
    pub(super) async fn list_for_pane(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let (entries, stamp) = self.list_stamped(path, on_progress, cancel).await?;
        self.listing_opened(path, stamp);
        Ok(entries)
    }

    /// Lists `dir` (handle `handle`, real path `real`, spelled `remote`) into
    /// a fresh [`Gathered`]. Child folders' handles go into the cache under
    /// their REAL path, so the next step down costs no LOOKUP.
    async fn list_into(
        &self,
        client: &NfsClient,
        handle: &FileHandle,
        real: &str,
        remote: &str,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&CancellationToken>,
    ) -> Result<Gathered, NfsError> {
        let mut gathered = Gathered::default();
        let page = client.fsinfo.dtpref.clamp(PAGE_FLOOR, PAGE_CEILING);
        let listing = client.nfs.list_dir(handle, page, |event| match event {
            ListingEvent::Entries(batch) => {
                for entry in batch {
                    self.gather(&mut gathered, real, remote, entry);
                }
                if let Some(on_progress) = on_progress {
                    on_progress(gathered.tally);
                }
            }
            ListingEvent::Restarted => gathered = Gathered::default(),
        });
        let outcome = match cancel {
            Some(cancel) => tokio::select! {
                biased;
                () = cancel.cancelled() => return Ok(Gathered::default()),
                outcome = listing => outcome,
            },
            None => listing.await,
        };
        outcome.map(|folder| Gathered {
            stamp: folder.map(|attrs| (attrs.mtime, attrs.ctime)),
            ..gathered
        })
    }

    fn gather(&self, gathered: &mut Gathered, real: &str, remote: &str, entry: ListedEntry) {
        let Ok(name) = std::str::from_utf8(&entry.name) else {
            gathered.skipped += 1;
            return;
        };
        // An entry a misbehaving server names off the export (`..`, a `/` in
        // the name) is dropped, ❌ never listed under a path we'd refuse.
        if name.is_empty() || name == "." || name == ".." || name.contains('/') {
            return;
        }
        let Some(app_path) = self.app_path_of(&child_of(remote, name)) else {
            return;
        };
        if entry.attributes.file_type == FileType::Directory {
            self.inner.handles.offer(child_of(real, name), entry.handle.clone());
        }
        let built = attrs_to_file_entry(name, &app_path, &entry.attributes);
        if built.is_directory {
            gathered.tally.dirs += 1;
        } else {
            gathered.tally.files += 1;
            gathered.tally.bytes += built.size.unwrap_or(0);
        }
        gathered.entries.push(built);
    }

    /// What `path` is, a link followed where it can be (the `stat` shape; a
    /// link that leaves the export answers as the link).
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.client().await?;
        let found = self.resolve(&client, &remote, Follow::IfInside, true).await?;
        let attrs = found.attributes.ok_or_else(|| VolumeError::NotFound(remote.clone()))?;
        let name = Path::new(&remote)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.name.clone());
        let app_path = self
            .app_path_of(&remote)
            .ok_or_else(|| VolumeError::NotFound(path.to_string_lossy().into_owned()))?;
        Ok(attrs_to_file_entry(&name, &app_path, &attrs))
    }

    /// What is AT `path`, a link reported as the link: one fresh LOOKUP, whose
    /// attributes don't follow it.
    pub(super) async fn entry_kind_impl(&self, path: &Path) -> Result<EntryKind, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.client().await?;
        let found = self.resolve(&client, &remote, Follow::Never, true).await?;
        Ok(found.attributes.as_ref().map_or(EntryKind::Directory, entry_kind))
    }

    /// Whether anything is at `path`, a dangling link included. A path off
    /// this export is `Absent`; no client, or a refusal, is `Unknown`.
    pub(super) async fn presence_impl(&self, path: &Path) -> Presence {
        let Ok(remote) = self.to_remote_path(path) else {
            return Presence::Absent;
        };
        let Ok(client) = self.client().await else {
            return Presence::Unknown;
        };
        Presence::of(&self.resolve(&client, &remote, Follow::Never, true).await)
    }

    /// The room on the export's filesystem, from one `FSSTAT` on the root.
    pub(super) async fn get_space_info_impl(&self) -> Result<SpaceInfo, VolumeError> {
        let client = self.client().await?;
        let stat = client
            .nfs
            .fsstat(&client.root)
            .await
            .map_err(|err| map_nfs_error(&err, "/"))?;
        Ok(fsstat_to_space_info(&stat))
    }

    /// Whether this identity may add names to the folder `path`, from one
    /// `ACCESS` call: the server's own answer, `root_squash` and all.
    ///
    /// ❗ A read-only export and a refused permission both come back as "not
    /// granted", so a refusal is `Unexplained`, ❌ never a guess at which.
    /// Anything that kept the question from being asked is `Unknown`.
    pub(super) async fn write_access_impl(&self, path: &Path) -> WriteAccess {
        let Ok(remote) = self.to_remote_path(path) else {
            return WriteAccess::Unknown;
        };
        let Ok(client) = self.client().await else {
            return WriteAccess::Unknown;
        };
        let asked = access::MODIFY | access::EXTEND;
        let answer = self
            .on_resolved(&client, &remote, Follow::Always, false, |found| {
                let nfs = client.nfs.clone();
                async move { nfs.access(&found.handle, asked).await }
            })
            .await;
        match answer {
            Ok(granted) if granted.granted & access::EXTEND != 0 => WriteAccess::Writable,
            Ok(_) => WriteAccess::Unwritable {
                reason: UnwritableReason::Unexplained,
            },
            Err(_) => WriteAccess::Unknown,
        }
    }
}
