//! The whole `impl Volume`: the capability answers, and one-line delegators to
//! the modules that do the work. Every answer here is deliberate: a default this
//! backend accepts silently is a promise it may not be able to keep.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataCaps, MetadataClasses, MetadataReport};
use cmdr_fs::volume::http_session::HttpBackend;
use cmdr_fs::volume::patching;
use cmdr_fs::volume::scan_walk;
use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, DirectoryCreation, LaneKey, ListingProgress, MutationEvent, Retirement,
    ScanBoundary, ScanConflict, SignInShape, SourceItemInfo, SpaceInfo, StreamLength, StreamWriteProgress,
    StreamWritten, Volume, VolumeError, VolumeReadStream, WatchCoverage, WriteMode,
};
use tokio_util::sync::CancellationToken;

use super::{BACKEND, WebdavVolume};

impl WebdavVolume {
    /// What a server that keeps dates (a Nextcloud, as its connect probe said)
    /// keeps of a copy beyond the mtime `X-OC-Mtime` already carries: a file's
    /// creation date (`X-OC-CTime` on the PUT), and a folder's creation and
    /// modification dates (one PROPPATCH). Its streams report nothing: a GET
    /// carries no creation date (`DETAILS.md` § "Dates"). Any other server keeps
    /// nothing beyond a file's mtime (`MetadataCaps::NONE`).
    pub const DATE_KEEPING_CAPS: MetadataCaps = MetadataCaps {
        stores: MetadataClasses::CREATED,
        reports: MetadataClasses::NONE,
        folder_stores: MetadataClasses::CREATED,
        folder_dates: true,
        staged_flags: cmdr_fs::metadata::UserFlags::NONE,
    };

    /// Whether the server keeps creation dates and a folder's dates, as its
    /// last connect probe said.
    pub(super) fn keeps_dates(&self) -> bool {
        self.inner.keeps_dates.load(Ordering::Relaxed)
    }
}

impl WebdavVolume {
    /// Runs `work` through the shared lifecycle's `noting`, which watches the
    /// answer for a server that is gone. ❗ Every delegator below that can reach
    /// the wire wraps itself in this: with no watcher, the operations ARE the
    /// detector (`cmdr_fs::volume::http_session`).
    pub(super) async fn noting<T>(
        &self,
        work: impl Future<Output = Result<T, VolumeError>> + Send,
    ) -> Result<T, VolumeError> {
        self.inner.noting(work).await
    }
}

impl Volume for WebdavVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        self.root.app_root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The SERVER and account, not the directory: two volumes opened at
    /// different collections on one server share its concurrency budget.
    fn lane_key(&self) -> LaneKey {
        let params = self.inner.params();
        LaneKey::new(format!(
            "webdav:{}:{}:{}",
            params.host(),
            params.port(),
            params.username
        ))
    }

    fn max_concurrent_ops(&self) -> usize {
        self.inner.host.settings().max_concurrent_operations(BACKEND)
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.list_directory_impl(path, on_progress, None)))
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.list_directory_impl(path, on_progress, cancel)))
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.get_metadata_impl(path)))
    }

    /// Kept rather than taken from the trait default: the default routes through
    /// `Volume::get_metadata`, which is `noting`-wrapped, so a bare existence
    /// probe on a dropped link would report a connection transition and drive a
    /// reconnect. An `exists` question stays a question.
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(self.exists_impl(path))
    }

    // ── The byte path ────────────────────────────────────────────────

    fn supports_streaming(&self) -> bool {
        true
    }

    /// ❗ Implementing the read path does not declare it; this does.
    fn supports_export(&self) -> bool {
        true
    }

    /// [`Self::DATE_KEEPING_CAPS`] on a server that keeps dates, else nothing.
    fn metadata_caps(&self) -> MetadataCaps {
        if self.keeps_dates() {
            Self::DATE_KEEPING_CAPS
        } else {
            MetadataCaps::NONE
        }
    }

    /// A folder's creation and modification dates in one PROPPATCH, on a server
    /// that keeps dates; every class answers `NotApplicable` anywhere else
    /// (Apache `mod_dav` would store the PROPPATCH as dead properties and
    /// change nothing).
    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Pin<Box<dyn Future<Output = MetadataReport> + Send + 'a>> {
        Box::pin(async move {
            let mut report = MetadataReport::none_applicable(carried, modified);
            if !self.keeps_dates() || (modified.is_none() && carried.created.is_none()) {
                return report;
            }
            let outcome = match self.noting(self.set_dates_impl(path, modified, carried.created)).await {
                Ok(true) => ClassOutcome::Kept,
                Ok(false) => {
                    log::warn!(
                        "WebdavVolume::apply_metadata: the server refused the dates of {}",
                        LogPrivate::path(path)
                    );
                    ClassOutcome::Refused
                }
                Err(e) => {
                    log::warn!(
                        "WebdavVolume::apply_metadata: {} keeps the server's dates, not the source's: {e}",
                        LogPrivate::path(path),
                        e = LogError(&e)
                    );
                    ClassOutcome::Refused
                }
            };
            if modified.is_some() {
                report.modified = outcome;
            }
            if carried.created.is_some() {
                report.created = outcome;
            }
            report
        })
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

    /// The hint is what the stream declares when the GET's answer names no
    /// length (a proxy that stripped it, a chunked body), so a copy holds the
    /// body to the listing's size instead of being told nothing.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(async move {
            let stream = self.open_read_stream_impl(path, 0).await?.hinted(size_hint);
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

    /// ❗ The refusal is `If-None-Match: *`'s, not a check of ours.
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

    /// MKCOL refuses an occupied name with 405 (RFC 4918 § 9.3.1), so the
    /// folder-merge walker can read `AlreadyExists` as "merge into this one".
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        true
    }

    /// New names go out composed (NFC); every path still goes out byte for byte,
    /// and only a name Cmdr is creating gets respelled, by the caller that knows
    /// it's creating it. `DETAILS.md` § "New names go out composed".
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

    /// ❗ `write_is_single_shot` keeps its `false` default, so the transfer
    /// engine stages every write on a `.cmdr-tmp-*` sibling and a partial never
    /// wears the user's filename. What this writes to is that sibling, with one
    /// plain `PUT` (`writes.rs`).
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        let Some(size) = length.known() else {
            return Box::pin(async { Err(VolumeError::NotSupported) });
        };
        Box::pin(async move {
            self.noting(
                self.write_from_stream_impl(dest, mode, size, stream, &|bytes_written, _| {
                    on_progress(StreamWriteProgress {
                        bytes_written,
                        expected_length: StreamLength::Known(size),
                    })
                }),
            )
            .await
        })
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

    /// The server copies for itself (COPY), so duplicating a file inside one
    /// server sends no bytes over the link.
    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.copy_within_impl(from, to, on_progress)))
    }

    fn supports_copy_within(&self) -> bool {
        true
    }

    // ── Lifecycle ────────────────────────────────────────────────────

    fn retirement(&self) -> Option<&Retirement> {
        Some(self.inner.session.connection().retirement())
    }

    /// Retires this instance ❗ without touching the live client: whoever still
    /// holds an `Arc` keeps using it. What retires is what belongs to the ID.
    fn on_superseded(&self) {
        self.inner.session.connection().retirement().retire();
    }

    /// The volume is leaving the registry. ❌ No connection event: the frontend
    /// learns through `volumes-changed`.
    fn on_unmount(&self) {
        self.inner.unmount();
    }

    /// Where this connection stands, for the switcher dot and the pane's connect
    /// views. HTTP has no session, so `Direct` means the last request that
    /// reached the wire came back.
    fn connection_state(&self) -> Option<cmdr_fs::volume::ConnectionState> {
        Some(self.inner.session.connection().state().published())
    }

    /// The installed client's generation, bumped only when a reconnect installs
    /// a rebuilt client (`reconnect.rs`).
    fn connection_generation(&self) -> Option<u64> {
        Some(self.inner.session.connection().generation())
    }

    fn backend_kind(&self) -> cmdr_fs::volume::BackendKind {
        cmdr_fs::volume::BackendKind::Webdav
    }

    /// One rung, one prompt: a password always mends this backend.
    fn sign_in_prompt(&self) -> SignInShape {
        SignInShape::Password
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.attempt_reconnect())
    }

    fn reconnect_with_credentials<'a>(
        &'a self,
        sign_in: cmdr_fs::volume::secret_store::SignIn,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::secret_store::SignedIn, VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.reconnect_as(sign_in))
    }

    fn remember_sign_in<'a>(
        &'a self,
        sign_in: cmdr_fs::volume::secret_store::SignIn,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::secret_store::SignedIn, VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.remember_as(sign_in))
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

    /// ❗ No watcher, so ❌ nothing here may call `authoritative_listing`.
    fn can_watch_listings(&self) -> bool {
        false
    }

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        WatchCoverage::None
    }

    /// RFC 4331 quota, where the server reports it; `NotSupported` otherwise.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.get_space_info_impl()))
    }

    fn space_poll_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(60))
    }
}
