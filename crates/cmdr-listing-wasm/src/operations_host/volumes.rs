//! The demo's answer to the write engine's volume registry: one volume, under
//! its one id, and NO local disk (`local_disk_id` is `None`), so the demo's
//! `root` takes the volume engine for every operation. Mirrors the desktop's
//! `operations_host/volumes.rs`, minus the routes: the demo has no archive edits
//! and no git portal.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{ResolvedVolume, Volume};
use cmdr_operation_log::rollback::VolumeLookup;
use cmdr_operations::host::{OperationVolumes, UnregisteredVolume, VolumeAnswer};

type ArrivalListener = Box<dyn Fn(&str) + Send + Sync>;

/// The page's one volume.
pub(crate) struct DemoVolumes {
    volume_id: String,
    volume: Arc<dyn Volume>,
    /// Kept so a listener's lifetime matches the app's; the demo's one volume
    /// never arrives again, so none is ever called.
    arrival_listeners: Mutex<Vec<ArrivalListener>>,
}

impl DemoVolumes {
    pub(crate) fn new(volume_id: String, volume: Arc<dyn Volume>) -> Self {
        Self {
            volume_id,
            volume,
            arrival_listeners: Mutex::new(Vec::new()),
        }
    }
}

impl VolumeLookup for DemoVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        (volume_id == self.volume_id).then(|| Arc::clone(&self.volume))
    }
}

impl OperationVolumes for DemoVolumes {
    fn resolve<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, ResolvedVolume> {
        Box::pin(async move { ResolvedVolume::passthrough(self.get(volume_id), path) })
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

    fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        (self.volume.root() == root).then(|| (self.volume_id.clone(), Arc::clone(&self.volume)))
    }

    fn forget_archive_index(&self, _archive_path: &Path) {}

    fn on_volume_arrival(&self, listener: ArrivalListener) {
        self.arrival_listeners.lock_ignore_poison().push(listener);
    }

    fn unregistered<'a>(&'a self, _volume_id: &'a str) -> VolumeAnswer<'a, UnregisteredVolume> {
        Box::pin(async { UnregisteredVolume::Gone })
    }

    /// The page has no disk of its own: its `root` is the in-memory volume.
    fn local_disk_id(&self) -> Option<&str> {
        None
    }
}
