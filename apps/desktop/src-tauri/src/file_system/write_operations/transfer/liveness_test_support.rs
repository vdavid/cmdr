//! A volume that reports its connection PROVEN dead, standing in for a verdict
//! no real backend can give.
//!
//! The stall watchdog's one aggressive action is gated on
//! `Volume::connection_liveness` answering `Dead`, and NO backend in this
//! workspace answers that — `smb2` 0.16.0's keepalive deliberately never reads a
//! missed probe as death, and its sound verdict arrives only as an error that has
//! already torn the connection down.
//! So without this double every "the watchdog acts" test would be exercising a
//! path production cannot reach, and the gate would be trusted rather than
//! pinned. Its twin is the trait default (`None`), which
//! `transfer_probe_tests::a_connection_with_no_liveness_verdict_is_never_aborted`
//! uses to prove a volume with no verdict is only ever reported on.
//!
//! Lives at the `transfer/` level because two sibling suites need it:
//! `transfer_probe_tests.rs` (the watchdog in isolation) and
//! `volume_strategy_retry_tests.rs` (the wedge-to-retry handoff end to end).

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{ConnectionLiveness, InMemoryVolume, ListingProgress, Volume, VolumeError};

pub(crate) struct DeadConnectionVolume {
    inner: Arc<InMemoryVolume>,
}

/// A volume whose connection is proven dead. Only `connection_liveness` matters;
/// everything else delegates so it stays usable wherever a `Volume` is wanted.
pub(crate) fn dead_connection_volume() -> Arc<dyn Volume> {
    Arc::new(DeadConnectionVolume {
        inner: Arc::new(InMemoryVolume::new("dead-connection")),
    }) as Arc<dyn Volume>
}

impl Volume for DeadConnectionVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn connection_liveness(&self) -> Option<ConnectionLiveness> {
        Some(ConnectionLiveness::Dead)
    }
}
