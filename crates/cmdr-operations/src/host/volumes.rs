//! Which volume serves an id or a path: the host's volume registry, as the
//! engine asks it.
//!
//! The engine never holds a registry of its own. It asks per call, by id, and
//! the host answers from whatever it keeps (Cmdr's is the `VolumeManager`,
//! through `operations_host::volumes`). The same answer serves the rollback
//! planner, whose [`VolumeLookup`] is this trait's supertrait.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::{ResolvedVolume, Volume};
use cmdr_operation_log::rollback::VolumeLookup;

/// An answer that needs I/O to give, as the `Volume` trait spells one.
pub type VolumeAnswer<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The registry questions the engine asks. [`VolumeLookup::get`] is the plain
/// by-id lookup.
///
/// ❗ **A site passing a path calls [`resolve`](Self::resolve), never `get`.**
/// `resolve` routes a `.zip`-crossing or `.git/<category>/` path to the
/// read-only volume that serves it; `get` hands back the drive underneath. A
/// site that stays on `get` with a path in hand says why in one line.
pub trait OperationVolumes: VolumeLookup {
    /// The volume that serves `path` on `volume_id`, routed through an archive
    /// or the git portal where the path crosses one. The path comes back
    /// unchanged.
    fn resolve<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, ResolvedVolume>;

    /// Whether `path` could be served by a route rather than by the volume that
    /// holds it. Pure string work, the cheap gate in front of an awaited
    /// [`resolve`](Self::resolve): ❌ never the answer itself.
    fn routes_over_its_parent(&self, path: &Path) -> bool;

    /// Whether `path` lies INSIDE a confirmed archive on `volume_id` (the `.zip`
    /// file itself doesn't), confirmed through the parent's own I/O.
    fn path_is_inside_archive<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, bool>;

    /// Whether `path` crosses into a confirmed archive on `volume_id`: the `.zip`
    /// file itself, or anything inside it.
    fn path_crosses_archive_boundary<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, bool>;

    /// The registered volume one of whose mount roots is `root`, with its id.
    fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)>;

    /// Drops the parsed index of the archive at `archive_path`, so the next read
    /// sees an edit the engine just committed.
    fn forget_archive_index(&self, archive_path: &Path);

    /// Tells `listener`, by id, whenever a volume becomes available: it
    /// registers, or a registered volume reconnects in place (its session came
    /// back, which no registration announces).
    ///
    /// ❗ The listener runs INSIDE the registration or the backend's report of
    /// the reconnect: it returns at once and hands real work to a task.
    fn on_volume_arrival(&self, listener: Box<dyn Fn(&str) + Send + Sync>);

    /// Why nothing is registered under `volume_id`. Ask it only after a lookup
    /// came back empty: it doesn't consult the registry. ❗ Asking never dials.
    fn unregistered<'a>(&'a self, volume_id: &'a str) -> VolumeAnswer<'a, UnregisteredVolume>;

    /// Which volume id is this host's LOCAL DISK, the one the engine's `std::fs`
    /// bodies (the local copy, move, delete, and trash, the local rename and its
    /// `lstat` pre-flight) may run on: the desktop's is
    /// [`DEFAULT_VOLUME_ID`](cmdr_fs::volume::DEFAULT_VOLUME_ID), and a host with
    /// no local disk (the browser demo, whose one volume is also called `"root"`)
    /// answers `None`, so every operation takes the volume engine.
    fn local_disk_id(&self) -> Option<&str> {
        Some(cmdr_fs::volume::DEFAULT_VOLUME_ID)
    }

    /// The local disk, when `volume_id` names it: the capability every local-disk
    /// branch of the engine needs to run, so the `std::fs` engine can't start on
    /// a volume that isn't one. ❌ Don't override it; answer [`Self::local_disk_id`].
    fn local_disk<'a>(&self, volume_id: &'a str) -> Option<LocalDisk<'a>> {
        (self.local_disk_id() == Some(volume_id)).then_some(LocalDisk { volume_id })
    }
}

/// Proof that an id is this host's local disk, handed out only by
/// [`OperationVolumes::local_disk`] (its field is private, so nothing else can make
/// one). The engine's local-disk branches take it, so "run the `std::fs` engine on
/// a volume that isn't the local disk" can't be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDisk<'a> {
    volume_id: &'a str,
}

impl<'a> LocalDisk<'a> {
    /// The local disk's volume id.
    pub fn volume_id(&self) -> &'a str {
        self.volume_id
    }
}

/// Why no volume is registered under an id. Each caller maps it into its own
/// vocabulary: ❌ never word a not-connected id as a dropped session or a
/// deleted folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnregisteredVolume {
    /// A listed device or a saved server that nothing has connected yet.
    NotConnected,
    /// An id nothing lists or saves: a volume that left the registry.
    Gone,
}

/// The answer for a host with no volumes: a bench, a tool, an engine before the
/// app wired its own. Nothing is registered, every id is gone, and a listener is
/// never called.
#[cfg(any(test, feature = "testing"))]
pub(super) struct NoOperationVolumes;

#[cfg(any(test, feature = "testing"))]
impl VolumeLookup for NoOperationVolumes {
    fn get(&self, _volume_id: &str) -> Option<Arc<dyn Volume>> {
        None
    }
}

#[cfg(any(test, feature = "testing"))]
impl OperationVolumes for NoOperationVolumes {
    fn resolve<'a>(&'a self, _volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, ResolvedVolume> {
        Box::pin(async move { ResolvedVolume::passthrough(None, path) })
    }

    fn routes_over_its_parent(&self, _path: &Path) -> bool {
        false
    }

    fn path_is_inside_archive<'a>(&'a self, _volume_id: &'a str, _path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(async { false })
    }

    fn path_crosses_archive_boundary<'a>(&'a self, _volume_id: &'a str, _path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(async { false })
    }

    fn find_by_root(&self, _root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        None
    }

    fn forget_archive_index(&self, _archive_path: &Path) {}

    fn on_volume_arrival(&self, _listener: Box<dyn Fn(&str) + Send + Sync>) {}

    fn unregistered<'a>(&'a self, _volume_id: &'a str) -> VolumeAnswer<'a, UnregisteredVolume> {
        Box::pin(async { UnregisteredVolume::Gone })
    }
}
