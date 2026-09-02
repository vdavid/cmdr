//! The whole `impl Volume`: the capability answers, and one-line delegators to
//! the modules that do the work.
//!
//! Every answer here is deliberate. A default this backend accepts silently is
//! a promise it may not be able to keep: `supports_local_fs_access` defaults
//! to `true` in the trait, and a device path handed to the OS opens nothing.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, DirectoryCreation, LaneKey, ListingProgress, MutationEvent, Retirement,
    ScanConflict, SignInPrompt, SourceItemInfo, SpaceInfo, Volume, VolumeError, VolumeReadStream, WatchCoverage,
};
use cmdr_fs::volume::{patching, scan_walk};
use tokio_util::sync::CancellationToken;

use super::{AdbVolume, BACKEND};
use crate::shell;

impl AdbVolume {
    /// Runs `work`, noticing on the way out if the answer says the device is
    /// gone.
    ///
    /// ❗ Every delegator below that can reach the wire wraps itself in this:
    /// with no watcher on this backend, a gone device is invisible until an
    /// operation asks it for something, so the operations ARE the detector.
    async fn noting<T>(&self, work: impl Future<Output = Result<T, VolumeError>> + Send) -> Result<T, VolumeError> {
        let outcome = work.await;
        if let Err(error) = &outcome {
            self.note_lost_session(error);
        }
        outcome
    }
}

impl Volume for AdbVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The DEVICE: one USB pipe, whatever path a transfer touches on it.
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(format!("adb:{}", self.inner.serial))
    }

    /// Read per batch dispatch, ❗ never captured at construction. The app's
    /// table answers 1 for this namespace: one sync socket at a time is what a
    /// phone's `adbd` serves without thrashing.
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

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.list_directory_impl(path, None, cancel)))
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.get_metadata_impl(path)))
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(self.exists_impl(path))
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.is_directory_impl(path)))
    }

    // ── The byte path ────────────────────────────────────────────────

    fn supports_streaming(&self) -> bool {
        true
    }

    /// ❗ Implementing the read path does not declare it: the copy engine
    /// refuses a source answering `false` before it opens anything.
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
        self.open_read_stream_at_offset(path, 0)
    }

    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        _size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }

    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_for_scan<'a>(
        &'a self,
        path: &'a Path,
        _size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_at_offset(path, 0)
    }

    /// `RECV` has no offset, so `[offset, size)` is honored by discarding the
    /// head as it arrives (`streams.rs`).
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

    // ── The write path ───────────────────────────────────────────────

    /// Every mutation below is implemented, so New folder, New file, Rename,
    /// and Paste are honestly enabled.
    fn is_writable(&self) -> bool {
        true
    }

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

    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.create_directory_all_impl(path)))
    }

    /// `mkdir -p` succeeds on a directory that is already there, so the
    /// folder-merge walker pre-checks existence here the way it does on MTP.
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        false
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.delete_impl(path)))
    }

    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(async move {
            if cancel.is_some_and(CancellationToken::is_cancelled) {
                return Err(VolumeError::Cancelled(path.to_string_lossy().into_owned()));
            }
            self.delete_impl(path).await
        }))
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.rename_impl(from, to, force)))
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.write_from_stream_impl(dest, size, stream, on_progress)))
    }

    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(self.copy_within_impl(from, to, on_progress)))
    }

    /// ❗ No watcher here, so this patch is the ONLY thing that keeps a
    /// destination pane honest after a copy.
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

    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(scan_walk::scan_trees(self, paths, on_progress)))
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
        Some(&self.inner.retirement)
    }

    fn on_superseded(&self) {
        self.inner.retirement.retire();
    }

    /// ❌ No connection event: the frontend learns through `volumes-changed`.
    fn on_unmount(&self) {
        self.inner.unmounted.store(true, Ordering::Relaxed);
        self.inner.mark_gone_silently();
    }

    /// The device authorizes the HOST on its own screen; there is no secret a
    /// person could type here.
    fn sign_in_prompt(&self) -> SignInPrompt {
        SignInPrompt::Nothing
    }

    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.inner.do_attempt_reconnect())
    }

    // ── What this backend is, in capability terms ────────────────────

    /// No mount, so no local path and nothing the OS can open. ❗ The trait
    /// default is `true`.
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

    /// `df -k` on the volume root, parsed by the shell module.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(self.noting(async move {
            let root = self.to_device_path(&self.root)?;
            let outcome = shell::run(&self.inner.endpoint, &self.inner.serial, &["df", "-k", &root])
                .await
                .map_err(|e| self.inner.map_adb_error(e, &root))?;
            if !outcome.succeeded() {
                return Err(VolumeError::NotSupported);
            }
            let parts =
                shell::parse_df_k(&String::from_utf8_lossy(&outcome.stdout)).ok_or(VolumeError::NotSupported)?;
            Ok(SpaceInfo {
                total_bytes: parts.total_bytes,
                available_bytes: parts.available_bytes,
                used_bytes: parts.total_bytes.saturating_sub(parts.available_bytes),
            })
        }))
    }

    /// A shell round trip per poll, so well above the local 2 s.
    fn space_poll_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(30))
    }
}
