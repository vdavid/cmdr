//! Which volume serves a path.
//!
//! The engine never reaches the app's volume registry: it asks here. Not the
//! index's `VolumeProvider`, which answers mount identity for scanning; this one
//! answers routing for reading, and sharing a trait would couple two crates
//! through methods neither needs.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::{ResolvedVolume, Volume};

/// What to do about a volume id nothing serves.
pub enum Unregistered {
    /// The mount under the path IS that volume and nobody had registered it; it
    /// is now. Read through it.
    Adopted(Arc<dyn Volume>),
    /// A listed device or saved server nobody connected yet.
    NotConnected,
    /// Nothing by that id, as after an unmount race.
    Gone,
}

/// Answers "which volume serves this?" for the engine.
pub trait ListingVolumes: Send + Sync {
    /// The volume registered under `volume_id`, with no path routing.
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>>;

    /// The volume that LISTS `path` under `volume_id`, routing a path that
    /// crosses into an archive, a repo's virtual `.git` trees, or a restic
    /// snapshot to the volume for that. A listing enters an archive's root, so
    /// a `.zip` inside a snapshot lists as the archive, where a caller acting on
    /// the path itself would get the backed-up file.
    fn resolve<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ResolvedVolume> + Send + 'a>>;

    /// [`resolve`](Self::resolve) without any route that would need I/O, for the
    /// synchronous fresh-listing oracle.
    fn resolve_local_only(&self, volume_id: &str, path: &Path) -> ResolvedVolume;

    /// Nothing serves `volume_id`; asks once whether the mount under `path` can
    /// be adopted and, if not, why it's missing. One question, so the engine
    /// can't adopt without asking why or the other way around.
    fn unregistered<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Unregistered> + Send + 'a>>;
}

/// Nothing registered: every id is [`Unregistered::Gone`].
pub struct NoVolumes;

impl ListingVolumes for NoVolumes {
    fn get(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
        None
    }

    fn resolve<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ResolvedVolume> + Send + 'a>> {
        Box::pin(async move { self.resolve_local_only(volume_id, path) })
    }

    fn resolve_local_only(&self, _volume_id: &str, path: &Path) -> ResolvedVolume {
        ResolvedVolume::passthrough(None, path)
    }

    fn unregistered<'a>(
        &'a self,
        _volume_id: &'a str,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Unregistered> + Send + 'a>> {
        Box::pin(async { Unregistered::Gone })
    }
}
