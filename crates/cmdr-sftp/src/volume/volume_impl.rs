//! The whole `impl Volume`: the capability answers, and one-line delegators to
//! the modules that do the work.
//!
//! Every answer here is deliberate. A default this backend accepts silently is a
//! promise it may not be able to keep, and two of them (`listing_watch_coverage`
//! and `max_concurrent_ops`) are the difference between a stale pane and a
//! serialized transfer.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{
    DirectoryCreation, LaneKey, ListingProgress, MutationEvent, SpaceInfo, Volume, VolumeError, VolumeReadStream,
    WatchCoverage,
};
use tokio_util::sync::CancellationToken;

use super::streams::{READ_WINDOW_DEPTH, SCAN_WINDOW_DEPTH};
use super::{BACKEND, SftpVolume};

impl Volume for SftpVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The SERVER, not the directory.
    ///
    /// ❗ The trait's default is the volume root, so two volumes opened at
    /// different directories on one server would each run full concurrency
    /// against the same host and the same single SSH connection.
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(format!(
            "sftp:{}:{}:{}",
            self.inner.params.host, self.inner.params.port, self.inner.params.username
        ))
    }

    /// Read per batch dispatch, ❗ never captured at construction: the trait
    /// default is 1, and a namespace with no row in the app's table gets a
    /// cautious 2.
    fn max_concurrent_ops(&self) -> usize {
        self.inner.host.settings().max_concurrent_operations(BACKEND)
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.list_directory_impl(path, on_progress, None))
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(self.list_directory_impl(path, on_progress, cancel))
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

    // ── The byte path ────────────────────────────────────────────────

    fn supports_streaming(&self) -> bool {
        true
    }

    /// The primitive the copy path reads through.
    ///
    /// Behind it is a window of positioned reads, ❗ never the engine's own file
    /// offset: `streams.rs` says what that would cost.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.open_read_stream_impl(path, READ_WINDOW_DEPTH).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    /// The background scan's reads, ❗ deliberately narrower than the foreground
    /// window: both share one SSH channel, and a prefetch that fills the channel
    /// window is a pane that waits for it.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_for_scan<'a>(
        &'a self,
        path: &'a Path,
        _size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.open_read_stream_impl(path, SCAN_WINDOW_DEPTH).await?;
            Ok(Box::new(stream) as Box<dyn VolumeReadStream>)
        })
    }

    /// The positioned read remote-archive browsing runs on.
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
        Box::pin(self.read_range_impl(path, offset, len))
    }

    // ── The write path ───────────────────────────────────────────────

    /// Every mutation below is implemented, so the panes' New folder, New file,
    /// Rename, and Paste are honestly enabled.
    fn is_writable(&self) -> bool {
        true
    }

    /// ❗ The refusal is `SSH_FXF_EXCL`'s, not a check of ours: `mutation.rs`
    /// says what a stat-then-write would cost.
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

    /// ❗ Overridden. The trait default spends one `exists()` round trip per
    /// ancestor before creating anything, which over a 50 ms link is the whole
    /// cost of a deep destination.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        Box::pin(self.create_directory_all_impl(path))
    }

    /// `SSH_FXP_MKDIR` refuses an occupied name on every server, so the
    /// folder-merge walker can read `AlreadyExists` as "merge into this one" —
    /// and a remote archive edit takes its ATOMIC swap instead of the
    /// delete-then-rename window, because this answer plus
    /// `posix-rename@openssh.com` on a forced rename is what that fast path is
    /// gated on.
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        true
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.delete_impl(path))
    }

    /// ❗ `force = false` never reaches for `posix-rename@openssh.com`, which is
    /// DEFINED to replace the destination. `mutation.rs` § the two renames.
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(self.rename_impl(from, to, force))
    }

    /// The window an upload runs through. ❗ `write_is_single_shot` keeps its
    /// `false` default, so every write here is staged on a `.cmdr-tmp-*` sibling
    /// and a partial never wears the user's filename.
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(self.write_from_stream_impl(dest, size, stream, on_progress))
    }

    /// ❗ There is no watcher here, so this patch is the ONLY thing that keeps a
    /// destination pane honest after a copy.
    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(self.notify_mutation_impl(parent_path, mutation))
    }

    // ── What this backend is, in capability terms ────────────────────

    /// No mount, so no local filesystem path and nothing the OS can open.
    fn supports_local_fs_access(&self) -> bool {
        false
    }

    /// ❗ Answering `true` would let a drag hand Finder a path that resolves to
    /// nothing (or, worse, to a local file of the same name).
    fn paths_are_os_visible(&self) -> bool {
        false
    }

    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    /// Every operation is a round trip, so the transfer engine must treat this
    /// as remote work and budget it accordingly.
    fn operations_are_local(&self) -> bool {
        false
    }

    /// ❗ No watcher, so the coverage stays `None` and ❌ nothing here may call
    /// `authoritative_listing`. Claiming a freshness we can't keep is how a
    /// pre-flight scan reuses a stale cache and overwrites a file it thought
    /// wasn't there.
    fn can_watch_listings(&self) -> bool {
        false
    }

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        WatchCoverage::None
    }

    /// `statvfs@openssh.com` is not reachable from this crate stack: the
    /// low-level crate has no request for it and the protocol crate carries only
    /// the extension NAME so the server hello parses. So free space is honestly
    /// unavailable rather than guessed at, and ❗ the poll interval below has to
    /// agree or a pane would poll something that always refuses.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        None
    }
}
