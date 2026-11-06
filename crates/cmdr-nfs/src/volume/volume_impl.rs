//! The whole `impl Volume`: the capability answers, and one-line delegators to
//! the modules that do the work. Every answer here is deliberate: a default this
//! backend accepts silently is a promise it may not be able to keep.

use cmdr_fs::volume::Presence;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::metadata::{CarriedMetadata, MetadataCaps, MetadataReport};
use cmdr_fs::volume::http_session::HttpBackend;
use cmdr_fs::volume::{
    BackendKind, BatchScanResult, ConnectionState, CopyScanResult, DirectoryCreation, EntryKind, LaneKey,
    ListingProgress, MutationEvent, Retirement, ScanBoundary, ScanConflict, SignInShape, SourceItemInfo, SpaceInfo,
    StreamLength, StreamWriteProgress, StreamWritten, Volume, VolumeError, VolumeReadStream, WatchCoverage,
    WriteAccess, WriteMode, patching, scan_walk,
};
use tokio_util::sync::CancellationToken;

use super::{BACKEND, NfsVolume};

impl NfsVolume {
    /// Runs `work` through the shared lifecycle's `noting`, which watches the
    /// answer for a server that is gone. ❗ Every delegator below that can reach
    /// the wire wraps itself in this: with no watcher, the operations ARE the
    /// detector (`cmdr_fs::volume::http_session`).
    async fn noting<T>(&self, work: impl Future<Output = Result<T, VolumeError>> + Send) -> Result<T, VolumeError> {
        self.inner.noting(work).await
    }
}

impl Volume for NfsVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        self.root.app_root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The SERVER, ❌ not the export: two exports on one NAS share its disks
    /// and its link, so they share one concurrency budget.
    fn lane_key(&self) -> LaneKey {
        let server = self.inner.params().server;
        LaneKey::new(format!("nfs:{}:{}", server.host.to_lowercase(), server.identity_port()))
    }

    fn max_concurrent_ops(&self) -> usize {
        self.inner.host.settings().max_concurrent_operations(BACKEND)
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.list_for_pane(path, on_progress, None)))
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.list_for_pane(path, on_progress, cancel)))
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.get_metadata_impl(path)))
    }

    /// Kept rather than taken from the trait default, which routes through the
    /// `noting`-wrapped `get_metadata`: a bare existence probe on a dropped link
    /// would report a connection transition. An existence question stays a
    /// question.
    fn presence<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Presence> + Send + 'a>> {
        Box::pin(self.presence_impl(path))
    }

    /// LOOKUP's own attributes, which don't follow a link: a link to a folder
    /// is a `Symlink`, ❌ never the folder.
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.entry_kind_impl(path)))
    }

    // ── The byte path ────────────────────────────────────────────────

    fn supports_streaming(&self) -> bool {
        true
    }

    /// ❗ Implementing the read path does not declare it; this does.
    fn supports_export(&self) -> bool {
        true
    }

    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(async move {
            let stream = self.open_read_stream_impl(path, 0).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        }))
    }

    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_at_offset<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(async move {
            let stream = self.open_read_stream_impl(path, offset).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        }))
    }

    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.read_range_impl(path, offset, len)))
    }

    // ── The write path ───────────────────────────────────────────────

    fn is_writable(&self) -> bool {
        true
    }

    /// ❗ `CREATE GUARDED`'s refusal, ❌ never a check of ours.
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.create_file_impl(path, content)))
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.create_directory_impl(path)))
    }

    /// ❗ Overridden: the trait default spends one `exists()` per ancestor.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.create_directory_all_impl(path)))
    }

    /// `MKDIR` refuses a taken name with `NFS3ERR_EXIST`, so the folder-merge
    /// walker can read `AlreadyExists` as "merge into this one".
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        true
    }

    /// New names go out composed (NFC): a Linux server stores the bytes it's
    /// sent, so a decomposed name from macOS looks right and matches nothing
    /// (`crates/cmdr-webdav/DETAILS.md` § "New names go out composed").
    fn composes_new_names(&self) -> bool {
        true
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.delete_impl(path)))
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.rename_impl(from, to, force)))
    }

    /// A forced rename is one `RENAME`, which replaces its target atomically.
    fn supports_atomic_replace_rename(&self) -> bool {
        true
    }

    /// ❗ `write_is_single_shot` keeps its `false` default, so the transfer
    /// engine stages every write on a `.cmdr-tmp-*` sibling and a partial never
    /// wears the user's filename.
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        // ❗ Refused before anything opens: this backend hasn't opted into
        // unknown-length writes.
        if length.known().is_none() {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        }
        Box::pin(self.noting(self.write_from_stream_impl(dest, mode, length, stream, on_progress)))
    }

    fn metadata_caps(&self) -> MetadataCaps {
        self.metadata_caps_impl()
    }

    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Pin<Box<dyn Future<Output = MetadataReport> + Send + 'a>> {
        Box::pin(self.apply_metadata_impl(path, carried, modified))
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(patching::patch_mutation(self, parent_path, mutation))
    }

    // ── Scanning, before a copy runs ─────────────────────────────────

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(scan_walk::scan_one(self, path)))
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(scan_walk::scan_trees(self, paths, boundary)))
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(scan_walk::scan_conflicts(self, source_items, dest_path)))
    }

    // ── Lifecycle ────────────────────────────────────────────────────

    fn retirement(&self) -> Option<&Retirement> {
        Some(self.inner.session.connection().retirement())
    }

    /// Retires this instance ❗ without touching the live connection: whoever
    /// still holds an `Arc` keeps using it. What retires is what belongs to the
    /// id.
    fn on_superseded(&self) {
        self.inner.session.connection().retirement().retire();
    }

    /// The volume is leaving the registry: no more reconnects, the connection
    /// goes, and the server hears `UMNT`, best effort. ❌ No connection event:
    /// the frontend learns through `volumes-changed`.
    fn on_unmount(&self) {
        self.inner.leave();
    }

    fn connection_state(&self) -> Option<ConnectionState> {
        Some(self.inner.session.connection().state().published())
    }

    /// The installed connection's generation, bumped only when a reconnect
    /// installs a new one.
    fn connection_generation(&self) -> Option<u64> {
        Some(self.inner.session.connection().generation())
    }

    fn backend_kind(&self) -> BackendKind {
        BackendKind::Nfs
    }

    /// ❗ `AUTH_SYS` has no secret, so there is never a sheet to show.
    fn sign_in_prompt(&self) -> SignInShape {
        SignInShape::Nothing
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.attempt_reconnect())
    }

    // ── What this backend is, in capability terms ────────────────────

    fn supports_local_fs_access(&self) -> bool {
        false
    }

    fn paths_are_os_visible(&self) -> bool {
        false
    }

    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    fn operations_are_local(&self) -> bool {
        false
    }

    /// ❗ No watcher (the poller re-lists on a change it SAW, which can't vouch
    /// for a pre-flight), so ❌ nothing here may call `authoritative_listing`.
    fn can_watch_listings(&self) -> bool {
        false
    }

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        WatchCoverage::None
    }

    /// `FSSTAT` on the export's root: what THIS identity may still write.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.get_space_info_impl()))
    }

    fn space_poll_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))
    }

    /// The server's own `ACCESS` answer for this identity.
    fn write_access_at<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = WriteAccess> + Send + 'a>> {
        Box::pin(self.write_access_impl(path))
    }
}
