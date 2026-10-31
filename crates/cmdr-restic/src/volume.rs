//! [`ResticVolume`]: a read-only [`Volume`] over one repo's snapshots.
//!
//! The same shape `GitPortalVolume` has. A path crossing `<repo>/Browse
//! snapshots/` isn't a hook inside the parent's backend: the host routes it
//! here, and this volume maps the whole path to (snapshot, path inside it)
//! through [`classify`], so a resolved path stays the caller's path verbatim.
//!
//! Its namespace is the view and everything under it: the root lists one folder
//! per snapshot plus `latest`, and each holds that snapshot's tree. The repo's
//! real files (`config`, `data/`, …) are the parent's, and reach a pane through
//! the parent; the row leading here is the host's listing overlay, ❌ never a
//! listing of this volume or the parent's.
//!
//! The volume is thin (the hub, the repo's slot, the parent): everything costly
//! lives in the shared [`ResticRepos`].

use crate::hub::{IndexSource, RepoSlot, ResticRepos};
use crate::path::{SnapshotRef, VIRTUAL_ENTRY_NAME, VirtualPath, classify};
use crate::read_stream::ResticReadStream;
use crate::repo::{IndexedRepo, ResticError, worker_crashed};
use crate::snapshots::{LATEST_FOLDER_NAME, Snapshot, SnapshotList};
use crate::tree::{entry_of, folder_tree, modified_secs, node_at, permissions_of};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::metadata::MetadataCaps;
use cmdr_fs::volume::scan_walk::{self, ScanSource, Walking};
use cmdr_fs::volume::{
    BackendKind, BatchScanResult, CopyScanResult, DirectoryCreation, LaneKey, ListingProgress, PasswordForgotten,
    PasswordLocked, ScanBoundary, SpaceInfo, TransferHold, UnlockOutcome, Volume, VolumeError, VolumeReadStream,
};
use rustic_core::repofile::Node;
use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// The most folder listings a background walk keeps in flight here. Each one
/// below a snapshot is a tree blob read from the parent, so a copy's pre-scan of
/// a whole snapshot would otherwise fan out without bound on a NAS.
const MAX_CONCURRENT_SCAN_LISTINGS: usize = 4;

/// A read-only [`Volume`] over one restic repo's snapshots, rooted at
/// `<repo>/Browse snapshots`.
pub struct ResticVolume {
    hub: Arc<ResticRepos>,
    slot: Arc<RepoSlot>,
    /// The volume physically holding the repo. Every read goes through it, and
    /// it lends this volume its lane key and its free space.
    parent: Arc<dyn Volume>,
    /// `<repo>/Browse snapshots`: this volume's root, and the prefix of every
    /// path it serves.
    root: PathBuf,
}

/// A snapshot a path named, and how: through `latest`, or by its own folder.
struct Resolved {
    snapshot: Snapshot,
    via_latest: bool,
}

impl ResticVolume {
    /// The view over the repo in `repo_root` on `parent`, which the host
    /// registered as `volume_id`.
    ///
    /// Cheap and infallible: nothing is opened until a listing asks, and the
    /// password, opened repo, and index live in `hub`, keyed by `volume_id` and
    /// `repo_root` exactly as spelled.
    pub fn new(hub: Arc<ResticRepos>, volume_id: &str, parent: Arc<dyn Volume>, repo_root: PathBuf) -> Self {
        let slot = hub.slot(volume_id, &repo_root);
        Self {
            hub,
            slot,
            parent,
            root: repo_root.join(VIRTUAL_ENTRY_NAME),
        }
    }

    /// Tries `password` on the repo (see [`ResticRepos::unlock`]), saving nothing.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) async fn unlock(&self, password: &str) -> Result<bool, VolumeError> {
        self.hub.unlock(&self.slot, &self.parent, password).await
    }

    /// The hub's slot for this repo, for a test that looks inside it.
    #[cfg(test)]
    pub(crate) fn slot_for_tests(&self) -> Arc<RepoSlot> {
        Arc::clone(&self.slot)
    }

    /// `path` as an absolute path under the root, the way rows report it.
    fn absolute(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    fn classify(&self, path: &Path) -> Result<VirtualPath, VolumeError> {
        classify(&self.root, path).ok_or_else(|| not_found(path))
    }

    /// The snapshot `which` names. `refresh` reads the snapshot list afresh.
    async fn resolve(&self, which: &SnapshotRef, path: &Path, refresh: bool) -> Result<Resolved, VolumeError> {
        // Anything inside a snapshot counts as using the index; the root
        // listing, which never needs it, doesn't (it goes to `snapshots` directly).
        self.slot.touch_index();
        let list = self.hub.snapshots(&self.slot, &self.parent, refresh).await?;
        let found = match which {
            SnapshotRef::Latest => self.slot.latest_in(&list),
            SnapshotRef::ShortId(short_id) => list.by_short_id(short_id),
        };
        let snapshot = found.cloned().ok_or_else(|| not_found(path))?;
        Ok(Resolved {
            snapshot,
            via_latest: *which == SnapshotRef::Latest,
        })
    }

    /// Where reads under `snapshot` get its index.
    fn index_source(&self, snapshot: &Snapshot) -> IndexSource {
        IndexSource {
            hub: Arc::clone(&self.hub),
            slot: Arc::clone(&self.slot),
            parent: Arc::clone(&self.parent),
            snapshot: snapshot.clone(),
        }
    }

    /// Runs `work` on the blocking pool against the index holding `snapshot`
    /// (waiting on, or starting, its load, with the progress handed to
    /// `on_progress`), and hands back the index it ran on. A pack gone under the
    /// index ([`ResticError::PackGone`], a prune elsewhere) reloads it once and
    /// runs `work` again; a second miss, through the fresh index, stands.
    async fn on_index<T, F>(
        &self,
        snapshot: &Snapshot,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
        path: &Path,
        work: F,
    ) -> Result<(T, Arc<IndexedRepo>), VolumeError>
    where
        T: Send + 'static,
        F: Fn(&IndexedRepo) -> Result<T, ResticError> + Send + Sync + 'static,
    {
        let source = self.index_source(snapshot);
        let work = Arc::new(work);
        let mut repo = source.get(on_progress).await?;
        let mut reloaded = false;
        loop {
            let (task_repo, task_work) = (Arc::clone(&repo), Arc::clone(&work));
            match self.blocking(path, move || Ok(task_work(&task_repo))).await? {
                Err(ResticError::PackGone(_)) if !reloaded => {
                    repo = source.reload(&repo, on_progress).await?;
                    reloaded = true;
                }
                outcome => {
                    return outcome
                        .map(|value| (value, repo))
                        .map_err(|err| err.into_volume_error(path));
                }
            }
        }
    }

    /// Runs `work` on the blocking pool: every rustic call below reads through
    /// the parent with `block_on`, which would panic on an async worker.
    async fn blocking<T, F>(&self, path: &Path, work: F) -> Result<T, VolumeError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, VolumeError> + Send + 'static,
    {
        self.hub
            .host()
            .runtime()
            .spawn_blocking(work)
            .await
            .map_err(|err| worker_crashed(err).into_volume_error(path))?
    }

    async fn list(
        &self,
        path: &Path,
        on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
    ) -> Result<Vec<FileEntry>, VolumeError> {
        let dir = self.absolute(path);
        match self.classify(path)? {
            VirtualPath::Root => {
                let list = self.hub.snapshots(&self.slot, &self.parent, true).await?;
                Ok(self.snapshot_rows(&list))
            }
            VirtualPath::Snapshot(which, inner) => {
                let resolved = self.resolve(&which, path, false).await?;
                let (tree_root, at) = (resolved.snapshot.tree, dir.clone());
                let (tree, _) = self
                    .on_index(&resolved.snapshot, on_progress, path, move |repo| {
                        folder_tree(repo, tree_root, &inner, &at)
                    })
                    .await?;
                let tree = tree.ok_or_else(|| not_found(path))?;
                let mut entries: Vec<FileEntry> = tree.nodes.iter().map(|node| entry_of(node, &dir)).collect();
                entries.sort_by(|a, b| b.is_directory.cmp(&a.is_directory).then_with(|| a.name.cmp(&b.name)));
                Ok(entries)
            }
        }
    }

    /// The root's rows: one folder per snapshot, plus `latest` when there's one.
    fn snapshot_rows(&self, list: &SnapshotList) -> Vec<FileEntry> {
        let mut rows: Vec<FileEntry> = list
            .snapshots()
            .iter()
            .map(|s| snapshot_row(&s.folder_name, &self.root.join(&s.folder_name), s))
            .collect();
        if let Some(latest) = self.slot.latest_shown(list) {
            rows.push(snapshot_row(
                LATEST_FOLDER_NAME,
                &self.root.join(LATEST_FOLDER_NAME),
                latest,
            ));
        }
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows
    }

    /// The node at a path inside a snapshot, with the repo that reads it.
    async fn node(
        &self,
        which: &SnapshotRef,
        inner: Vec<OsString>,
        path: &Path,
    ) -> Result<(Node, Arc<IndexedRepo>, Resolved), VolumeError> {
        let resolved = self.resolve(which, path, false).await?;
        let (tree_root, at) = (resolved.snapshot.tree, self.absolute(path));
        let (node, repo) = self
            .on_index(&resolved.snapshot, None, path, move |repo| {
                node_at(repo, tree_root, &inner, &at)
            })
            .await?;
        let node = node.ok_or_else(|| not_found(path))?;
        Ok((node, repo, resolved))
    }

    async fn metadata(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let absolute = self.absolute(path);
        let name = absolute
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match self.classify(path)? {
            VirtualPath::Root => Ok(FileEntry {
                ..FileEntry::new(
                    VIRTUAL_ENTRY_NAME.to_string(),
                    absolute.to_string_lossy().into_owned(),
                    true,
                    false,
                )
            }),
            VirtualPath::Snapshot(which, inner) if inner.is_empty() => {
                let resolved = self.resolve(&which, path, false).await?;
                Ok(snapshot_row(&name, &absolute, &resolved.snapshot))
            }
            VirtualPath::Snapshot(which, inner) => {
                let (node, _, _) = self.node(&which, inner, path).await?;
                let dir = absolute.parent().unwrap_or(&self.root);
                Ok(entry_of(&node, dir))
            }
        }
    }

    /// The file at `path`, opened for reads, with what its stream reports.
    async fn open_file(&self, path: &Path) -> Result<ResticReadStream, VolumeError> {
        let VirtualPath::Snapshot(which, inner) = self.classify(path)? else {
            return Err(VolumeError::IsADirectory(path.display().to_string()));
        };
        if inner.is_empty() {
            return Err(VolumeError::IsADirectory(path.display().to_string()));
        }
        let (node, repo, resolved) = self.node(&which, inner, path).await?;
        if node.is_dir() {
            return Err(VolumeError::IsADirectory(path.display().to_string()));
        }
        if !node.is_file() {
            // A symlink is never followed, and a device, fifo, or socket has no
            // bytes in a backup. Typed, so a folder copy skips it and carries on.
            return Err(VolumeError::NoContent(path.display().to_string()));
        }
        let (task_repo, task_node, at) = (Arc::clone(&repo), node.clone(), path.to_path_buf());
        let file = self
            .blocking(path, move || {
                task_repo
                    .repo
                    .open_file(&task_node)
                    .map_err(|err| task_repo.classify(&err).into_volume_error(&at))
            })
            .await?;
        Ok(ResticReadStream {
            repo,
            source: self.index_source(&resolved.snapshot),
            reloaded: false,
            file: Arc::new(file),
            runtime: self.hub.host().runtime(),
            path: path.to_path_buf(),
            size: node.meta.size,
            modified: modified_secs(&node).map(|secs| UNIX_EPOCH + Duration::from_secs(secs)),
            mode: permissions_of(node.meta.mode),
            latest_of: resolved.via_latest.then(|| Arc::clone(&self.slot)),
            read: 0,
            ahead: std::collections::VecDeque::new(),
            queued_to: 0,
        })
    }
}

/// A snapshot's folder row: dated by the backup, sized by what it processed,
/// and stating the host, tags, and paths it recorded.
fn snapshot_row(name: &str, path: &Path, snapshot: &Snapshot) -> FileEntry {
    let modified_at = snapshot
        .time
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|since| since.as_secs());
    FileEntry {
        size: snapshot.size,
        // A folder's Size cell reads its recursive size.
        recursive_size: snapshot.size,
        restic_meta: Some(snapshot.meta.clone()),
        modified_at,
        ..FileEntry::new(name.to_string(), path.to_string_lossy().into_owned(), true, false)
    }
}

fn not_found(path: &Path) -> VolumeError {
    VolumeError::NotFound(path.display().to_string())
}

impl Volume for ResticVolume {
    /// The name of the thing at [`root`](Volume::root), as every backend
    /// answers it. The registry id carries which repo this is.
    fn name(&self) -> &str {
        VIRTUAL_ENTRY_NAME
    }

    /// Its own kind, ❌ never the trait's `Local` default: favorites, the error
    /// report, and the index would otherwise treat a snapshot as a local disk.
    fn backend_kind(&self) -> BackendKind {
        BackendKind::Restic
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The **parent's** lane key: every snapshot read is a read of the repo's
    /// pack files off the parent, so the two must not be scheduled as
    /// independent resources.
    fn lane_key(&self) -> LaneKey {
        self.parent.lane_key()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_with_cancel(path, on_progress, None)
    }

    /// Overridden so the pane's token reaches the volume: a listing that has to
    /// open the repo or load its index can take seconds, and the trait default
    /// drops the token. A cancelled listing answers at once; the blocking work
    /// it started finishes on its own and is dropped.
    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let listing = self.list(path, on_progress);
            let entries = match cancel {
                Some(token) => tokio::select! {
                    biased;
                    () = token.cancelled() => return Err(VolumeError::Cancelled(path.display().to_string().into())),
                    entries = listing => entries?,
                },
                None => listing.await?,
            };
            if let Some(report) = on_progress {
                report(ListingProgress::of(&entries));
            }
            Ok(entries)
        })
    }

    fn max_concurrent_scan_listings(&self) -> usize {
        MAX_CONCURRENT_SCAN_LISTINGS
    }

    fn as_password_locked(&self) -> Option<&dyn PasswordLocked> {
        Some(self)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(self.metadata(path))
    }

    /// The target the backup recorded, raw bytes included. A folder of the view
    /// itself (the root, a snapshot row) is no link.
    fn read_link_target<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let VirtualPath::Snapshot(which, inner) = self.classify(path)? else {
                return Ok(None);
            };
            if inner.is_empty() {
                return Ok(None);
            }
            let (node, _, _) = self.node(&which, inner, path).await?;
            Ok(node.is_symlink().then(|| node.node_type.to_link().to_path_buf()))
        })
    }

    // ---- Read-only: a backup is changed with `restic`, not with a file manager

    /// ❌ Never flip this to track "the user can write to the repo". They can,
    /// through the parent, which serves every real file in it. A snapshot is
    /// content-addressed history; there is nothing here to write to.
    fn is_writable(&self) -> bool {
        false
    }

    /// `create_file`, `create_directory`, `delete`, `rename`, and
    /// `write_from_stream` all inherit the trait's `NotSupported` default. Only
    /// this one is overridden: the default walks `exists()` and answers
    /// `Ok(AlreadyExisted)` for a folder that's already there, which would have a
    /// read-only volume claim it created something.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ---- Copy out: streaming reads and the scan that plans them ------------

    fn supports_export(&self) -> bool {
        true
    }

    /// Reports the mode: a node's permission bits, so a script restored out of
    /// a snapshot still runs. Read-only, so it keeps nothing.
    fn metadata_caps(&self) -> MetadataCaps {
        MetadataCaps::MODE_REPORTER
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.open_file(path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut stream = self.open_file(path).await?;
            if offset >= stream.size {
                return Ok(Vec::new());
            }
            let offset = usize::try_from(offset).map_err(|_| VolumeError::NotSupported)?;
            stream.read_at(offset, len).await
        })
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        scan_walk::scan_one(self, path)
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        scan_walk::scan_trees(self, paths, boundary)
    }

    // ---- Capability flags: answered explicitly, never inherited ------------

    /// `true`: this volume's root is `<repo>/Browse snapshots`, a path inside the
    /// volume that holds the repo. See [`Volume::routes_over_a_parent`].
    fn routes_over_a_parent(&self) -> bool {
        true
    }

    /// Keeps the repo's index loaded for a transfer reading from it, paused or
    /// not (`RepoSlot::hold_for_transfer`).
    fn hold_for_transfer(&self) -> Option<TransferHold> {
        Some(self.slot.hold_for_transfer())
    }

    /// `None`: a snapshot path has no counterpart on disk.
    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    /// `false`: a file in a snapshot is blobs in pack files, not something
    /// `std::fs` can open.
    fn supports_local_fs_access(&self) -> bool {
        false
    }

    /// `false`: no other app can open these paths, so a drag out must hand over
    /// bytes, not paths.
    fn paths_are_os_visible(&self) -> bool {
        false
    }

    /// `None`: a snapshot's size doesn't move, so polling it would be churn.
    fn space_poll_interval(&self) -> Option<Duration> {
        None
    }

    /// The parent's. A copy OUT of a snapshot lands wherever the destination is,
    /// so the parent's free space is the honest answer; zero would read as "disk
    /// full" to the pre-copy space check.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.parent.get_space_info().await })
    }

    /// The parent's answer for the filesystem holding the repo, whatever `path`
    /// is asked about.
    fn get_space_info_at<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        let repo_root = self.root.parent().unwrap_or(&self.root);
        Box::pin(async move { self.parent.get_space_info_at(repo_root).await })
    }

    /// `false`, and `listing_watch_coverage` stays at the trait's `None`: these
    /// paths don't exist on disk, so there's nothing to arm a watcher on. A new
    /// backup shows on the next listing of the root, which reads `snapshots/`
    /// again.
    fn can_watch_listings(&self) -> bool {
        false
    }
}

/// A repo's password is checked before the dialog closes: `unlock` opens the
/// repo with it (the ~0.5 s scrypt, on the blocking pool) and answers
/// `Unlocked` or `WrongPassword`, never `Stored`.
impl PasswordLocked for ResticVolume {
    /// Opens the repo with `password` and keeps it in the hub. A wrong one keeps
    /// nothing. Dropped mid-open, nothing is kept: the hub commits after the open
    /// returns, with no await before the answer.
    fn unlock<'a>(
        &'a self,
        password: String,
    ) -> Pin<Box<dyn Future<Output = Result<UnlockOutcome, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let password = Zeroizing::new(password);
            if !self.hub.unlock(&self.slot, &self.parent, &password).await? {
                return Ok(UnlockOutcome::WrongPassword);
            }
            Ok(UnlockOutcome::Unlocked)
        })
    }

    /// Files the password under service `restic://`, scope `<volume id>:<repo
    /// path>`, per `remember` (`secret_store::keep_signed_in_secret`).
    fn remember_password<'a>(
        &'a self,
        password: String,
        remember: bool,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let password = Zeroizing::new(password);
            // allowed-discarded-outcome: the unlock stands whether or not the
            // store kept the password (a Deny on the Keychain prompt), and the
            // seam has no way to say so yet; the next launch asks.
            let _kept = self.hub.keep_password(&self.slot, &password, remember).await;
        })
    }

    fn forget_password(&self) {
        self.slot.forget_password();
    }

    /// Forgets the password in the hub and the credential store, and drops what
    /// was opened with it (`ResticRepos::forget_saved_password`).
    fn forget_saved_password<'a>(&'a self) -> Pin<Box<dyn Future<Output = PasswordForgotten> + Send + 'a>> {
        Box::pin(async move {
            if self.hub.forget_saved_password(&self.slot).await {
                PasswordForgotten::Everywhere
            } else {
                PasswordForgotten::StoreKeptIt
            }
        })
    }
}

/// The two reads `scan_walk` needs, so `scan_for_copy` and the batch scan behind
/// every transfer come for free: that's what makes restoring a whole folder out
/// of a snapshot work at all.
impl ScanSource for ResticVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        self.get_metadata(path)
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        self.list_directory(path, None)
    }

    /// `true`: a read here is safe to drop mid-flight (its waiter leaves the
    /// index load, and blocking reads finish detached), so a copy's Cancel
    /// lands inside a listing that waits on the index load, not after it.
    fn abandons_reads_on_stop(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stale_index_tests.rs"]
mod stale_index_tests;
