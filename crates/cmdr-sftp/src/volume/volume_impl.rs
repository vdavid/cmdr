//! The whole `impl Volume`: the capability answers, and one-line delegators to
//! the modules that do the work.
//!
//! Every answer here is deliberate. A default this backend accepts silently is a
//! promise it may not be able to keep, and two of them (`listing_watch_coverage`
//! and `max_concurrent_ops`) are the difference between a stale pane and a
//! serialized transfer.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{LaneKey, ListingProgress, SpaceInfo, Volume, VolumeError, WatchCoverage};
use tokio_util::sync::CancellationToken;

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
