//! The `impl Volume for SmbVolume` block: identity, capabilities, and the
//! dispatch surface for everything the app asks this backend.
//!
//! A trait impl can't be split across files, so every `Volume` method lives
//! here. What each one DOES lives next door, in the concern module that owns it:
//! `paths` (path translation), `query` (listings, metadata, space), `mutation`
//! (create / delete / rename and their listing-cache patches), `scan`,
//! `scan_pool`, `streams`, `reconnect`. A method that needs more than a few
//! lines of its own belongs there, not here; what stays here is the capability
//! answers, whose whole content is the reasoning in their doc comments.

use super::state::ConnectionState;
use super::streams::InlineReadStream;
use super::{SmbVolume, foreground_yield};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::SmbConnectionState;
use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume,
    VolumeError, VolumeReadStream, WatchCoverage,
};
use cmdr_fs::volume::{ListingProgress, Retirement};
use log::debug;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::time::Duration;

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
        Some(std::sync::Arc::new(self.instance_at_root(new_root)))
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
        Box::pin(self.list_directory_with_progress_impl(path, on_progress))
    }

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        _cancel: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        // SMB ignores the token (there's no mid-listing SMB cancel today; the
        // scanner's `LIST_TIMEOUT` on a detached task handles a wedged listing) —
        // same as the default `list_directory_with_cancel` this used to fall through
        // to. The override exists to draw from the per-scan connection pool when one
        // is active; see `scan_pool.rs`.
        Box::pin(async move { self.list_directory_for_scan_impl(path).await })
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
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| Some(n.saturating_sub(1)))
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

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(self.exists_impl(path))
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(self.is_directory_impl(path))
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
            "SmbVolume for {}: the mount at {} is gone; still browsing over smb2, but its paths are no longer OS-openable",
            self.inner.share_name,
            self.mount_path.display()
        );
    }

    fn supports_foreground_yield(&self) -> bool {
        // A running copy and the pane's listings share ONE SMB session, so a
        // transfer off this share competes with every navigation on it. Opting in
        // tells `CheckpointStream` not to start the next chunk while the user is
        // browsing this share. The read holds nothing between chunks, so this is a
        // park in place, not a session release. See `foreground_yield.rs`.
        true
    }

    fn supports_foreground_yield_as_destination(&self) -> bool {
        // An UPLOAD to this share (local → SMB) writes in discrete SMB2 WRITE
        // chunks and holds only a file handle between them, with NO oplock or
        // lease requested (`create_file_writer` → `OplockLevel::None`, no durable
        // context; see `streams.rs`). So a running upload can stand aside for
        // the user browsing the same share between chunks. `CheckpointStream` caps
        // each such park so the open write handle never sits idle long enough for
        // the server to reap it. Contrast the read side (`supports_foreground_yield`):
        // both share this volume's per-share `foreground_pending` probe.
        true
    }

    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { foreground_yield::foreground_pending(self.inner.host(), &self.inner.volume_id) })
    }

    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(
            async move { foreground_yield::wait_until_foreground_idle(self.inner.host(), &self.inner.volume_id).await },
        )
    }

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        // SMB watching is volume-level: the smb_watcher monitors the whole share
        // via CHANGE_NOTIFY, which the SERVER raises, so it reports every client's
        // writes and not only ours. That's what earns `EveryWriter` here while the
        // same share seen through an OS mount only earns `ThisMachineOnly`.
        //
        // `watcher_cancel` is a std `Mutex` (not async): use `try_lock` and treat
        // contention as "no coverage" to keep the oracle out of the lock-wait path.
        // The oracle will simply fall through to a real read; that's the safe
        // direction. Don't hold the lock across awaits (we never `.await` here
        // anyway: this is a sync method).
        let has_watcher = match self.inner.watcher_cancel.try_lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => return WatchCoverage::None,
        };
        if has_watcher && self.connection_state() == ConnectionState::Direct {
            WatchCoverage::EveryWriter
        } else {
            WatchCoverage::None
        }
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
    /// the scan watchdog this share is still answering.
    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_impl(paths, on_progress)
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

    fn max_concurrent_ops(&self) -> usize {
        // Read per batch dispatch, never captured at construction: the user moves
        // the slider and the next batch picks it up, with no remount. What the
        // host resolves the `"smb"` namespace to is its business (today the
        // `network.smbConcurrency` setting, default 10, clamped to 1..=32).
        self.inner.host().settings().max_concurrent_operations(super::BACKEND)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path)?;

            debug!(
                "SmbVolume::open_read_stream: share={}, path={:?}",
                self.inner.share_name, smb_path
            );

            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let smb_path = self.to_smb_path(path)?;

            // Compound fast-path: if the caller-provided hint fits in one READ,
            // send CREATE+READ+CLOSE as a single compound frame (1 RTT) instead
            // of the 3-RTT streaming open. Drives the compound on a cloned
            // `Connection` with no lock held, so N concurrent small reads
            // pipeline over one SMB session. Falls through to the streaming
            // path when the hint is missing or too large, or when the file
            // changed size since the scan: shrunk files come back short
            // (`data.len() != size`), grown-past-`max_read` files come back as
            // a typed `ErrorKind::TooLarge` (smb2 refuses to truncate a file
            // that no longer fits in one READ).
            if let Some(size) = size_hint {
                let (tree, mut conn) = self.clone_session().await?;
                let max_read = conn.params().map(|p| p.max_read_size).unwrap_or(65536) as u64;
                if size > 0 && size <= max_read {
                    debug!(
                        "SmbVolume::open_read_stream_with_hint: share={}, path={:?}, size={}; using compound fast-path",
                        self.inner.share_name, smb_path, size
                    );
                    match tree.read_file_compound(&mut conn, &smb_path).await {
                        Err(e) if matches!(e.kind(), smb2::ErrorKind::TooLarge) => {
                            debug!(
                                "SmbVolume::open_read_stream_with_hint: file grew past max_read since the scan ({}); falling back to streaming",
                                e
                            );
                        }
                        read_result => {
                            let data = self.handle_smb_result("open_read_stream_with_hint(compound)", read_result)?;
                            if data.len() as u64 == size {
                                return Ok(Box::new(InlineReadStream::new(data)) as Box<dyn VolumeReadStream>);
                            }
                            debug!(
                                "SmbVolume::open_read_stream_with_hint: compound read returned {} bytes, expected {}; falling back to streaming",
                                data.len(),
                                size
                            );
                        }
                    }
                }
            }

            debug!(
                "SmbVolume::open_read_stream_with_hint: share={}, path={:?}; using streaming path",
                self.inner.share_name, smb_path
            );
            let stream = self.open_smb_download_stream(&smb_path).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
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
                "SmbVolume::read_range: share={}, path={:?}, offset={}, len={}",
                self.inner.share_name, smb_path, offset, len
            );

            // One open -> one positioned read -> close per call. `smb2::FileReader`
            // itself serves many `read_at`s per open, but `Volume::read_range` is
            // stateless (no handle persists across calls), so opening per call is
            // the simple, correct shape for now: a remote-zip browse issues only a
            // handful of ranged reads (the `TailCachedSource` collapses the
            // central-directory parse to ~1). Caching an open `FileReader` per path
            // is the future optimization; see the archive backend DETAILS.
            let (tree, conn) = self.clone_session().await?;
            let reader = self.handle_smb_result("read_range(open)", tree.open_file_reader(conn, &smb_path).await)?;

            let read_result = reader.read_at(offset, len as u64).await;
            // Close the handle regardless of the read outcome. Relying on `Drop`
            // would only log and leak the handle until session teardown, so we
            // close explicitly on both the success and error paths.
            let close_result = reader.close().await;

            let data = self.handle_smb_result("read_range", read_result)?;
            self.handle_smb_result("read_range(close)", close_result)?;
            Ok(data)
        })
    }

    /// A write that fits one compound CREATE+WRITE+FLUSH+CLOSE frame is
    /// all-or-nothing, so the transfer layer may write it straight to the file's
    /// final name instead of staging it on a `.cmdr-tmp-*`. Answers with the
    /// SAME condition `write_from_stream_impl`'s fast path branches on
    /// (`streams::fits_one_compound_write`); the two must never drift apart.
    fn write_is_single_shot<'a>(&'a self, size: u64) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(self.write_is_single_shot_impl(size))
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.write_from_stream_impl(dest, size, stream, on_progress)
    }

    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        // SmbVolume always returns `Some` so the frontend can distinguish
        // "not an SMB volume" (None) from "SMB volume in trouble"
        // (Some(Disconnected)). The reconnect manager keys off the latter.
        // The internal state machine is binary; the outer `OsMount` variant
        // is only attached by `enrich_from_volume_registry` for SMB shares
        // that have an OS mount but no Cmdr smb2 session at all.
        Some(match self.connection_state() {
            ConnectionState::Direct => SmbConnectionState::Direct,
            ConnectionState::Disconnected => SmbConnectionState::Disconnected,
        })
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.do_attempt_reconnect())
    }

    fn reconnect_with_credentials<'a>(
        &'a self,
        username: String,
        password: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.do_reconnect_with_credentials(username, password))
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
        debug!(
            "SmbVolume for {}: superseded by a newer instance; session left up for in-flight work",
            self.inner.share_name
        );
    }

    fn on_unmount(&self) {
        // Mark the volume permanently dead so any in-flight reconnect bails
        // out before installing a session into an orphaned volume.
        self.inner.unmounted.store(true, Ordering::Relaxed);

        // Transition to Disconnected. We deliberately set the atomic directly
        // instead of going through `transition_to_disconnected()`, because the
        // volume is being unregistered: the FE will learn via `volumes-changed`
        // and an extra `volume-connection-changed` event would race with that.
        self.inner
            .state
            .store(ConnectionState::Disconnected as u8, Ordering::Relaxed);

        // Cancel the background watcher task. The task will call watcher.close()
        // to release the SMB directory handle before exiting.
        if let Ok(mut guard) = self.inner.watcher_cancel.lock()
            && let Some(cancel_tx) = guard.take()
        {
            let _ = cancel_tx.send(());
            debug!("SmbVolume cleanup for {}: watcher cancel sent", self.inner.share_name);
        }

        // Tear down any live scan pool: a member session must not keep walking an
        // unmounted volume. Sync (no runtime here): flip its `closed` flag so
        // reconnect loops bail and drop this reference; the member sessions close
        // when the last `Arc` drops (within one backoff step). See `scan_pool.rs`.
        self.close_scan_pool_sync();

        // Drop the smb2 session. Uses blocking_lock() / blocking_write() since
        // on_unmount is sync (called from FSEvents thread, no Tokio runtime).
        // Safe because we just set state to Disconnected, so no async task
        // will acquire either lock. Drop Tree first, then SmbClient: Tree
        // holds a tree_id referenced by session-scoped server state, and we
        // want it to go first so any lingering `FileDownload` clones finish
        // before the client (which owns the Connection) vanishes. In
        // practice all three just drop their Arc refcounts; the order is
        // defensive.
        {
            let mut tree_guard = self.inner.tree.blocking_write();
            *tree_guard = None;
        }
        {
            let mut client_guard = self.inner.client.blocking_lock();
            *client_guard = None;
        }

        debug!("SmbVolume cleanup for {}: smb2 session dropped", self.inner.share_name);
    }
}

#[cfg(test)]
#[path = "volume_impl_test.rs"]
mod volume_impl_test;
