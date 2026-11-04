//! The demo's answer to the write engine's volume registry: the page's volumes
//! by id (the page registers one; the golden replay registers two more), and NO
//! local disk (`local_disk_id` is `None`), so the demo's `root` takes the volume
//! engine for every operation. Mirrors the desktop's `operations_host/volumes.rs`,
//! minus the routes: the demo has no archive edits and no git portal.

use std::path::Path;
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{ResolvedVolume, Volume};
use cmdr_operation_log::rollback::VolumeLookup;
use cmdr_operations::host::{OperationVolumes, UnregisteredVolume, VolumeAnswer};

type ArrivalListener = Box<dyn Fn(&str) + Send + Sync>;

/// The page's volumes, in the order they arrived.
pub(crate) struct DemoVolumes {
    volumes: Mutex<Vec<(String, Arc<dyn Volume>)>>,
    arrival_listeners: Mutex<Vec<ArrivalListener>>,
}

impl DemoVolumes {
    /// A registry holding `volume` under `volume_id`.
    pub(crate) fn new(volume_id: String, volume: Arc<dyn Volume>) -> Self {
        let volumes = Self {
            volumes: Mutex::new(Vec::new()),
            arrival_listeners: Mutex::new(Vec::new()),
        };
        volumes.register(&volume_id, volume);
        volumes
    }

    /// Files `volume` under `volume_id` (replacing one there) and tells every
    /// arrival listener, as the app's registry does.
    pub(crate) fn register(&self, volume_id: &str, volume: Arc<dyn Volume>) {
        {
            let mut volumes = self.volumes.lock_ignore_poison();
            volumes.retain(|(id, _)| id != volume_id);
            volumes.push((volume_id.to_string(), volume));
        }
        for listener in self.arrival_listeners.lock_ignore_poison().iter() {
            listener(volume_id);
        }
    }
}

impl VolumeLookup for DemoVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        self.volumes
            .lock_ignore_poison()
            .iter()
            .find(|(id, _)| id == volume_id)
            .map(|(_, volume)| Arc::clone(volume))
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
        self.volumes
            .lock_ignore_poison()
            .iter()
            .find(|(_, volume)| volume.root() == root)
            .map(|(id, volume)| (id.clone(), Arc::clone(volume)))
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
