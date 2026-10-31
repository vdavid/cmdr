//! The `impl Volume for SmbVolume` block: identity, capabilities, and the
//! dispatch surface for everything the app asks this backend.
//!
//! A trait impl can't be split across files, so every `Volume` method lives
//! here. What each one DOES lives next door, in the concern module that owns it:
//! `paths` (path translation), `query` (listings, metadata, space), `mutation`
//! (create / delete / rename and their listing-cache patches), `scan`,
//! `scan_pool`, `read_stream`, `write_stream`, `reconnect`. A method that needs more than a few
//! lines of its own belongs there, not here; what stays here is the capability
//! answers, whose whole content is the reasoning in their doc comments.

use super::copy::ProgressOnly;
use super::dfs_links::link_open_reader;
use super::session::SessionLease;
use super::state::ConnectionState;
use super::{SmbVolume, foreground_yield};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::metadata::{CarriedMetadata, MetadataCaps, MetadataClasses, MetadataReport, Unblock, UserFlags};

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{
    BatchScanResult, ConnectionLiveness, CopyScanResult, EntryKind, LaneKey, MutationEvent, ScanBoundary, ScanConflict,
    ServerCopyProgress, SourceItemInfo, SpaceInfo, StreamLength, StreamWriteProgress, StreamWritten, Volume,
    VolumeError, VolumeRandomReader, VolumeReadStream, WatchCoverage, WriteMode, XattrAnswer, XattrReadMode,
};
use cmdr_fs::volume::{ListingProgress, ListingWatch, Retirement};
use log::debug;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::time::Duration;

impl SmbVolume {
    /// The creation date and Hidden and Locked both ways, on files and
    /// folders, riding the frames a date already costs: Hidden goes on the
    /// staged temp with its dates, and Read-only (Locked) only after the
    /// landing, one frame for a Locked file alone (`DETAILS.md` § "Dates on
    /// copies" and § "Hidden and Locked on copies"). The xattrs wait for named
    /// streams (vdavid/smb2#9), and a share has no POSIX mode.
    pub const METADATA_CAPS: MetadataCaps = MetadataCaps {
        stores: MetadataClasses::CREATED.union(MetadataClasses::FLAGS),
        reports: MetadataClasses::CREATED.union(MetadataClasses::FLAGS),
        folder_stores: MetadataClasses::CREATED.union(MetadataClasses::FLAGS),
        folder_dates: true,
        staged_flags: UserFlags::HIDDEN,
    };
}

impl Volume for SmbVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.mount_path
    }

    /// Move this share's registry ID to another of its mount roots, keeping the
    /// live smb2 session.
    ///
    /// A direct SMB share is exactly the case `rerooted` exists for: the mount is
    /// only an addressing prefix here (Cmdr's own I/O rides smb2), so a new
    /// instance over the same `Arc<SmbVolumeInner>` serves the new root with no
    /// re-auth and no transport rebuild.
    ///
    /// ❌ Never route a promotion through `on_superseded` / `on_unmount`: for the
    /// moment either instance is alive, they SHARE one session, and retiring the
    /// old one would stop the watcher and the scan pool the new one is about to
    /// serve from. The registry drops the old instance right after swapping it in,
    /// so the overlap is transient by construction. `DETAILS.md` § "Re-rooting
    /// a share".
    fn rerooted(&self, new_root: &Path) -> Option<std::sync::Arc<dyn Volume>> {
        Some(std::sync::Arc::new(self.instance_at_root(new_root)?) as std::sync::Arc<dyn Volume>)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        // Serialize transfers that hit the same share over one session.
        // `volume_id` already encodes `server+port+share` (via
        // `smb_volume_id`), exactly the server+share granularity we want.
        LaneKey::new(self.inner.volume_id.clone())
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.list_directory_with_cancel_impl(path, on_progress, None))
    }

    /// One server batch at a time: `on_progress` ticks per batch and `cancel`
    /// stops the listing between batches (`query.rs` `read_listing`).
    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.list_directory_with_cancel_impl(path, on_progress, cancel))
    }

    /// Draws from the per-scan connection pool when one is active (`scan_pool.rs`), and stops between
    /// server batches when `cancel` fires, like the pane's listing. Nothing here parks, so `clock` runs.
    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a tokio_util::sync::CancellationToken>,
        _clock: &'a cmdr_fs::volume::ScanListingClock,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.list_directory_for_scan_impl(path, cancel))
    }

    /// Walks a foreign path down against real listings, component by component
    /// (`spelling.rs`). The one place this backend folds a name, and only to
    /// FIND the server's bytes: every other method here sends exactly the bytes
    /// it's handed.
    fn find_stored_spelling<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, VolumeError>> + Send + 'a>> {
        Box::pin(self.find_stored_spelling_impl(path, cancel))
    }

    fn begin_scan_session<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Refcounted: concurrent background users (an index rescan overlapping a
            // media enrichment pass) share ONE pool; `open_scan_pool` is idempotent.
            self.inner.scan_session_refs.fetch_add(1, Ordering::AcqRel);
            self.open_scan_pool().await
        })
    }

    fn end_scan_session<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Saturating decrement: an unmatched end (a pass racing unmount
            // teardown) must not underflow into a never-closing pool.
            let prev = self
                .inner
                .scan_session_refs
                .try_update(Ordering::AcqRel, Ordering::Acquire, |n| Some(n.saturating_sub(1)))
                .unwrap_or(0);
            // Close only when the LAST session ends; an earlier end while a sibling
            // still scans would tear the pool out from under it mid-flight.
            if prev <= 1 {
                self.close_scan_pool().await;
            }
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(self.get_metadata_impl(path))
    }

    /// Kept rather than taken from the trait default: `exists_impl` is a raw
    /// `stat` rather than the full `FileEntry` build `get_metadata` does, and
    /// the share root answers with no round trip at all.
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(self.exists_impl(path))
    }

    /// Kept for the same reason as [`exists`](Self::exists): a raw `stat`, and a
    /// share root that needs no round trip.
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(self.is_directory_impl(path))
    }

    /// A bare `stat`, one round trip: the trait default goes through
    /// `get_metadata`, which follows a link the server calls a non-directory
    /// (two more round trips) and builds a whole `FileEntry`. The delete walk
    /// asks this of every folder it would descend (§ "Junctions and symlinks").
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        Box::pin(self.entry_kind_impl(path))
    }

    /// Whether the share has named streams, which is what an xattr is here. A plain
    /// Samba has none, and this hides the tag colors there rather than offering a
    /// click that can only fail.
    fn supports_xattrs(&self) -> bool {
        self.inner.stores_xattrs.load(Ordering::Relaxed)
    }

    fn read_xattr_many<'a>(
        &'a self,
        paths: &'a [PathBuf],
        name: &'a str,
        mode: XattrReadMode,
    ) -> Pin<Box<dyn Future<Output = Vec<XattrAnswer>> + Send + 'a>> {
        Box::pin(self.read_xattr_many_impl(paths, name, mode))
    }

    fn write_xattr<'a>(
        &'a self,
        path: &'a Path,
        name: &'a str,
        value: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.write_xattr_impl(path, name, value))
    }

    fn remove_xattr<'a>(
        &'a self,
        path: &'a Path,
        name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.remove_xattr_impl(path, name))
    }

    fn can_watch_listings(&self) -> bool {
        // A `notify` watch is the wrong instrument for this volume, not a missing
        // optimization. Watching happens over smb2 instead: `smb_watcher` long-polls
        // CHANGE_NOTIFY on the share root, which is what `listing_watch_coverage`
        // reports on.
        //
        // ❌ Don't "fix" this by pointing `notify` at the OS mount path. FSEvents on
        // an `smbfs` mount is a local-VFS notifier: it reports only what this machine
        // wrote through the mount, and delivers nothing for a change another client
        // makes to the share (verified on macOS 26.5.2, `notify` 8.2.0, one watcher
        // and both write paths against a live share, 2026-08-08; see
        // `docs/notes/silent-inertness-hunt-2026-08-08.md`). Remote changes are
        // exactly what a share watcher is for, so that swap would trade a real
        // channel for a blind one.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        // SmbVolume handles listing notifications via notify_mutation,
        // so the old std::fs-based synthetic diff path is not needed.
        false
    }

    fn paths_are_os_visible(&self) -> bool {
        // The share is normally OS-mounted at `mount_path` alongside the smb2
        // session (the "sneaky mount"), and every path this instance hands out is
        // an absolute path under it. So a `file://` URL built from one opens in
        // any other app, which is what a drag-out drop target needs.
        //
        // Until the mount goes away. Cmdr's own I/O never touched it, so browsing
        // carries on and nothing looks broken — which is exactly why answering a
        // hardcoded `true` here was dangerous: a drag would publish a URL under a
        // mount that isn't there, and the drop would silently do nothing. The
        // registry is what knows (nothing may probe a mount), so it tells us.
        //
        // ❌ Don't fold this into `supports_local_fs_access` (which is `false`
        // here on purpose): five write/caching call sites read that one as "is
        // this remote?", and the honest answer there stays yes.
        !self.mount_root_gone.load(Ordering::Relaxed)
    }

    /// The registry proved this instance's mount root is gone and had no live
    /// sibling to promote to. Latch it: the session is fine, the paths aren't.
    fn note_root_mount_gone(&self) {
        self.mount_root_gone.store(true, Ordering::Relaxed);
        debug!(
            "SmbVolume share={:?}: the mount at path={:?} is gone; still browsing over smb2, but its paths are no longer OS-openable",
            LogPrivate::volume(&self.inner.share_name),
            LogPrivate::dir_path(&self.mount_path)
        );
    }

    fn supports_foreground_yield(&self) -> bool {
        // A running copy rides its own session, but shares the link and the
        // server's disks with the pane's listings, so a transfer off this share
        // still competes with every navigation on it (measured: a slow link's
        // queue, `foreground_yield.rs`). Opting in tells `CheckpointStream` not to
        // start the next chunk while the user is browsing this share. The read
        // holds nothing between chunks, so this is a park in place, not a session
        // release.
        true
    }

    fn supports_foreground_yield_as_destination(&self) -> bool {
        // An UPLOAD to this share (local → SMB) writes in discrete SMB2 WRITE
        // chunks and holds only a file handle between them, with NO oplock or
        // lease requested (`create_file_writer` → `OplockLevel::None`, no durable
        // context; see `write_stream.rs`). So a running upload can stand aside for
        // the user browsing the same share between chunks. `CheckpointStream` caps
        // each such park so the open write handle never sits idle long enough for
        // the server to reap it. Contrast the read side (`supports_foreground_yield`):
        // both share this volume's per-share `foreground_pending` probe.
        true
    }

    fn connection_liveness(&self) -> Option<ConnectionLiveness> {
        // smb2's own reading of the connection a copy rides, off the handle
        // swapped in lockstep with its client (`liveness.rs`, which also owns
        // the mapping): the transfer session while one is up, else the main
        // session, which transfers fall back to (`transfer_session.rs`). The scan
        // pool's and the watcher's sessions carry no transfer bytes.
        self.transfer_connection().verdict()
    }

    /// The same connection as [`connection_liveness`](Self::connection_liveness).
    /// It drops to a new count when the transfer session comes or goes, which
    /// the watchdog tolerates (it only compares its own readings).
    fn connection_bytes_received(&self) -> Option<u64> {
        self.transfer_connection().bytes_received()
    }

    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { foreground_yield::foreground_pending(self.inner.host(), &self.inner.volume_id) })
    }

    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(
            async move { foreground_yield::wait_until_foreground_idle(self.inner.host(), &self.inner.volume_id).await },
        )
    }

    fn listing_watch_coverage(&self, path: &Path) -> WatchCoverage {
        if let Some(behind_a_link) = self.link_watch_coverage(path) {
            return behind_a_link;
        }
        // The SERVER raises CHANGE_NOTIFY, so it reports every client's writes:
        // `EveryWriter`, where an OS mount of the share earns `ThisMachineOnly`.
        // `try_lock` reads contention as "no coverage": the oracle then falls
        // through to a real read, the safe direction, without waiting on a lock.
        let has_watcher = match self.inner.watcher_cancel.try_lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => return WatchCoverage::None,
        };
        if has_watcher && self.session_state() == ConnectionState::Direct {
            WatchCoverage::EveryWriter
        } else {
            WatchCoverage::None
        }
    }

    fn watch_listing(&self, path: &Path) -> Option<ListingWatch> {
        self.hold_link_watch(path)
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self.notify_mutation_impl(parent_path, mutation))
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(self.get_space_info_impl())
    }

    fn space_poll_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(5))
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.create_file_impl(path, content))
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.create_directory_impl(path))
    }

    /// The creation and modification dates and Hidden and Locked, in the one
    /// frame a date alone costs (`attributes.rs`).
    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Pin<Box<dyn Future<Output = MetadataReport> + Send + 'a>> {
        Box::pin(self.apply_metadata_impl(path, carried, modified))
    }

    /// A folder's Hidden and Locked: one `stat` round trip (`attributes.rs`).
    fn folder_metadata<'a>(
        &'a self,
        path: &'a Path,
        wanted: MetadataClasses,
    ) -> Pin<Box<dyn Future<Output = CarriedMetadata> + Send + 'a>> {
        Box::pin(self.folder_metadata_impl(path, wanted))
    }

    /// Clears `READONLY`, for a reversal whose removal was refused
    /// (`attributes.rs`).
    fn clear_blockers<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Unblock> + Send + 'a>> {
        Box::pin(self.clear_blockers_impl(path))
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.delete_impl(path))
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.rename_impl(from, to, force))
    }

    /// A share is writable as a BACKEND: every mutation method above is really
    /// implemented, so `assert_writability_matches_the_mutations_offered` holds.
    /// A share the server hands us read-only is a per-VOLUME fact that travels as
    /// the location's `mountIsReadOnly`, and shows up here as a per-operation
    /// `PermissionDenied` rather than a blanket `false`.
    fn is_writable(&self) -> bool {
        true
    }

    /// Bytes stream out over smb2 (`open_read_stream`), so a share can be the
    /// source of a cross-volume copy.
    fn metadata_caps(&self) -> MetadataCaps {
        Self::METADATA_CAPS
    }

    /// Every listing row and `stat` carries the server's creation time
    /// (`mapping.rs`); SMB records no added date.
    fn supported_dates<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = cmdr_fs::volume::SupportedDates> + Send + 'a>> {
        Box::pin(async { cmdr_fs::volume::SupportedDates::CREATED })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_impl(path)
    }

    /// The scan preview's entry point. Reporting as the walk goes is what keeps
    /// the dialog's counters climbing on a folder-sized SMB scan, and what tells
    /// the scan watchdog this share is still answering; the same boundary is
    /// what lets a person cancel a walk over a share that has spun its disks
    /// down, which used to run to the end whatever they pressed.
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_impl(paths, boundary)
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        self.scan_for_conflicts_impl(source_items, dest_path)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    /// A forced rename over a file is one `ReplaceIfExists` SET_INFO, so the
    /// old file is never removed in a step of its own (`mutation.rs`
    /// `replacing_rename`). Only a directory at either end falls back to
    /// clearing first, and the promise is about files.
    fn supports_atomic_replace_rename(&self) -> bool {
        true
    }

    /// The server copies for itself (copychunk, `copy.rs`); ❗ `NotSupported` on a
    /// server without it, the caller's signal to stream the file instead.
    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.copy_on_server_impl(from, to, &ProgressOnly(on_progress)).await })
    }

    /// The trait default's gate, but handing the engine's own progress to the
    /// slice loop, so a pause parks between slices rather than at the next file.
    fn copy_on_server<'a>(
        &'a self,
        source: &'a dyn Volume,
        from: &'a Path,
        to: &'a Path,
        mode: WriteMode,
        progress: &'a dyn ServerCopyProgress,
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if mode.refuses_occupied() || !self.may_copy_on_server_from(source) {
                return Err(VolumeError::NotSupported);
            }
            self.copy_on_server_impl(from, to, progress)
                .await
                .map(StreamWritten::nothing_refused)
        })
    }

    /// "Ask": whether the server has copychunk shows only in its answer.
    fn supports_copy_within(&self) -> bool {
        true
    }

    fn supports_unknown_length_writes(&self) -> bool {
        true
    }

    /// New names go out composed. Every path still goes out byte-for-byte
    /// (`paths.rs::to_smb_path`); only a name Cmdr is creating gets respelled,
    /// by the caller that knows it's creating it. `DETAILS.md` § "SMB names are
    /// opaque bytes".
    fn composes_new_names(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // Read per batch dispatch, never captured at construction: the user moves
        // the slider and the next batch picks it up, with no remount. What the
        // host resolves the `"smb"` namespace to is its business (today the
        // `network.smbConcurrency` setting, default 10, clamped to 1..=32).
        let requested = self.inner.host().settings().max_concurrent_operations(super::BACKEND);
        // Then bounded by what the session's credit window can actually carry:
        // a slot the window can't serve parks on credits instead of copying,
        // holding a transfer slot that reports no progress. Only copy slots read
        // this method (the transfer driver is its sole caller), so listings,
        // stats, and the watcher keep their own share of the window either way.
        // The measurement is the connection's, refreshed by `clone_session`;
        // `0` means no session has been cloned yet, so there's nothing to clamp
        // with. `DETAILS.md` § "Copy concurrency and the credit window".
        match self.inner.credit_copy_capacity.load(Ordering::Relaxed) {
            0 => requested,
            // `credit_capacity_for` never answers 0, and neither may this: a
            // copy engine handed zero slots does nothing at all.
            capacity => requested.clamp(1, capacity),
        }
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path)?;

            debug!(
                "SmbVolume::open_read_stream: share={:?}, path={:?}",
                LogPrivate::volume(&self.inner.share_name),
                LogPrivate::file_path(&smb_path)
            );

            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    /// The compound fast path for a file whose size the caller knows, else
    /// the stream (`read_stream.rs`).
    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(self.open_read_stream_with_hint_impl(path, size_hint))
    }

    fn open_read_stream_for_scan<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        // Background bulk reads (media enrichment prefetch) draw small hinted files
        // from the scan-connection pool when one is active; see `scan_pool.rs`.
        Box::pin(async move { self.open_read_stream_for_scan_impl(path, size_hint).await })
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if len == 0 {
                return Ok(Vec::new());
            }
            let smb_path = self.to_smb_path(path)?;
            debug!(
                "SmbVolume::read_range: share={:?}, path={:?}, offset={}, len={}",
                LogPrivate::volume(&self.inner.share_name),
                LogPrivate::file_path(&smb_path),
                offset,
                len
            );

            // One open -> one positioned read -> close per call. `smb2::FileReader`
            // itself serves many `read_at`s per open, but `Volume::read_range` is
            // stateless (no handle persists across calls), so opening per call is
            // the simple, correct shape for now: a remote-zip browse issues only a
            // handful of ranged reads (the `TailCachedSource` collapses the
            // central-directory parse to ~1). Caching an open `FileReader` per path
            // is the future optimization; see the archive backend DETAILS.
            let SessionLease { tree, conn, generation } = self.clone_session().await?;
            let first = tree.open_file_reader(conn, &smb_path);
            let opened = self
                .or_through_link(generation, &smb_path, first, link_open_reader)
                .await;
            let reader = self.handle_smb_result("read_range(open)", &smb_path, generation, opened)?;

            let read_result = reader.read_at(offset, len as u64).await;
            // Close the handle regardless of the read outcome. Relying on `Drop`
            // would only log and leak the handle until session teardown, so we
            // close explicitly on both the success and error paths.
            let close_result = reader.close().await;

            let data = self.handle_smb_result("read_range", &smb_path, generation, read_result)?;
            self.handle_smb_result("read_range(close)", &smb_path, generation, close_result)?;
            Ok(data)
        })
    }

    fn open_random_reader<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeRandomReader>, VolumeError>> + Send + 'a>> {
        Box::pin(self.open_random_reader_impl(path))
    }

    /// A write that fits one compound CREATE+WRITE+FLUSH+CLOSE frame is
    /// all-or-nothing, so the transfer layer may write it straight to the file's
    /// final name instead of staging it on a `.cmdr-tmp-*`. Answers with the
    /// SAME condition `write_from_stream_impl`'s fast path branches on
    /// (`write_stream::fits_one_compound_write`); the two must never drift apart.
    fn write_is_single_shot<'a>(&'a self, length: StreamLength) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let Some(size) = length.known() else {
            return Box::pin(async { false });
        };
        Box::pin(self.write_is_single_shot_impl(size))
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        self.write_from_stream_impl(dest, mode, length, stream, on_progress)
    }

    fn connection_state(&self) -> Option<cmdr_fs::volume::ConnectionState> {
        // Always `Some`: the switcher dot and the reconnect manager both key off
        // this value. The internal state machine is binary; the outer `OsMount`
        // variant is only attached by `enrich_from_volume_registry` for SMB
        // shares that have an OS mount but no Cmdr smb2 session at all.
        Some(match self.session_state() {
            ConnectionState::Direct => cmdr_fs::volume::ConnectionState::Direct,
            ConnectionState::Disconnected => cmdr_fs::volume::ConnectionState::Disconnected,
        })
    }

    /// The installed main session's generation, the one counter the error
    /// funnels already guard on (`state.rs::SessionCell`): bumped only by
    /// `do_attempt_reconnect` installing a rebuilt session.
    fn connection_generation(&self) -> Option<u64> {
        Some(self.inner.state.generation().count())
    }

    fn backend_kind(&self) -> cmdr_fs::volume::BackendKind {
        cmdr_fs::volume::BackendKind::Smb
    }

    /// A share asks for a USERNAME beside the password: the share is the
    /// identity, so `reconnect_with_credentials` accepts a different account and
    /// rewrites this volume's params. The trait default would render the username
    /// read-only and take "sign in as someone else" away.
    ///
    /// ❗ `guest_allowed` reports the only guest fact a VOLUME holds: that this
    /// session itself got in as a guest. A share that also accepts guests but was
    /// opened with credentials answers `false`, and the sheet then shows two
    /// fields and no guest button — the honest rendering of "we don't know". The
    /// host-level answer lives in the app's share-listing cache
    /// (`network::smb_cache::get_cached_shares_auth_mode`), which this crate has
    /// no business reading.
    fn sign_in_prompt(&self) -> cmdr_fs::volume::SignInShape {
        // `try_read`, ❌ never a blocking read: this is a sync trait method the
        // async IPC thread calls, and the only writer is a reconnect that holds
        // the lock for a moment. Losing the race costs the guest button for one
        // render, which is the harmless way to be wrong.
        let guest_allowed = self
            .inner
            .params
            .try_read()
            .is_ok_and(|params| params.username.eq_ignore_ascii_case("Guest") && params.password.is_empty());
        cmdr_fs::volume::SignInShape::UsernamePassword { guest_allowed }
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.do_attempt_reconnect())
    }

    fn reconnect_with_credentials<'a>(
        &'a self,
        sign_in: cmdr_fs::volume::secret_store::SignIn,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::secret_store::SignedIn, VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.do_reconnect_with_credentials(sign_in))
    }

    fn remember_sign_in<'a>(
        &'a self,
        sign_in: cmdr_fs::volume::secret_store::SignIn,
    ) -> Pin<Box<dyn Future<Output = Result<cmdr_fs::volume::secret_store::SignedIn, VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.do_remember_sign_in(sign_in))
    }

    /// The share's retirement flag, so the registry can tell this volume when it
    /// stops serving it.
    ///
    /// Share-scoped, not instance-scoped: an instance re-rooted onto a surviving
    /// mount is the same share, the same session, and the same watcher, so the
    /// answer has to live where they do.
    fn retirement(&self) -> Option<&Retirement> {
        Some(&self.inner.retirement)
    }

    /// Retire this instance without touching the live smb2 session.
    ///
    /// A newer `SmbVolume` has taken this volume's id in the `VolumeManager`
    /// (an upgrade, a re-register after a remount). The share is still there,
    /// and everything that grabbed an `Arc` to this instance before the swap is
    /// still using it: a running transfer holds `src_vol` / `dst_vol` clones for
    /// its whole duration, a viewer holds a read stream, the indexer holds a
    /// scan session. Tearing the session down here kills all of them on a
    /// connection that is perfectly healthy, which is the bug this exists to
    /// prevent. The session is released when the last `Arc` drops (smb2 aborts
    /// its receiver task with the last `Arc<Inner>`), so retirement is just
    /// letting go.
    ///
    /// What DOES retire is everything scoped to the volume ID, which the
    /// successor now owns: the watcher (see below), the scan pool, the
    /// `volume-connection-changed` events, and the index-resume hook. Two watchers
    /// on one id double-feed the index, and the retired one's death path
    /// (`spawn_watcher_death_reconnect`) would otherwise keep driving that id.
    ///
    /// The registry retires a volume it REMOVES; a hand-over like this one has to
    /// say so itself, since the id lives on under the successor.
    fn on_superseded(&self) {
        self.inner.retirement.retire();
        self.inner.stop_watcher();
        self.inner.link_watches.stop_all();
        debug!(
            "SmbVolume share={:?}: superseded by a newer instance; session left up for in-flight work",
            LogPrivate::volume(&self.inner.share_name)
        );
    }

    fn on_unmount(&self) {
        // Mark the volume permanently dead so any in-flight reconnect bails
        // out before installing a session into an orphaned volume.
        self.inner.unmounted.store(true, Ordering::Relaxed);

        // Transition to Disconnected, whichever session is installed. We
        // deliberately skip `transition_to_disconnected()` (and its event),
        // because the volume is being unregistered: the FE will learn via
        // `volumes-changed` and an extra `volume-connection-changed` event would
        // race with that.
        self.inner.state.shut_down();

        // Cancel the background watcher task. The task will call watcher.close()
        // to release the SMB directory handle before exiting.
        if let Ok(mut guard) = self.inner.watcher_cancel.lock()
            && let Some(cancel_tx) = guard.take()
        {
            let _ = cancel_tx.send(());
            debug!(
                "SmbVolume cleanup share={:?}: watcher cancel sent",
                LogPrivate::volume(&self.inner.share_name)
            );
        }

        self.inner.link_watches.stop_all(); // and every DFS link's watch, on its own session
        // Tear down any live scan pool: a member session must not keep walking an
        // unmounted volume. Sync (no runtime here): flip its `closed` flag so
        // reconnect loops bail and drop this reference; the member sessions close
        // when the last `Arc` drops (within one backoff step). See `scan_pool.rs`.
        self.close_scan_pool_sync();
        // And the transfer session: nothing may open, reconnect, or idle on an
        // unmounted volume.
        self.inner.transfer.close();

        // Drop the smb2 session. Uses blocking_lock() since on_unmount is sync
        // (called from FSEvents thread, no Tokio runtime). Safe because we just
        // set state to Disconnected, so no async task will acquire the lock.
        // Drop Tree first, then SmbClient: Tree holds a tree_id referenced by
        // session-scoped server state, and we want it to go first so any
        // lingering `FileDownload` clones finish before the client (which owns
        // the Connection) vanishes. In practice both just drop their Arc
        // refcounts; the order is defensive.
        let session = self.inner.session.blocking_lock().take();
        if let Some(super::session::MainSession { client, tree, .. }) = session {
            drop(tree);
            drop(client);
        }
        self.inner.live_connection.replace(None);

        debug!(
            "SmbVolume cleanup share={:?}: smb2 session dropped",
            LogPrivate::volume(&self.inner.share_name)
        );
    }
}

#[cfg(test)]
#[path = "volume_impl_test.rs"]
mod volume_impl_test;
