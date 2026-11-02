//! The write engine's registry questions, answered by the app's volume
//! registry.
//!
//! Every answer reaches `get_volume_manager()` per call, ❌ never once at
//! construction: the registry is the one place volumes come and go, and a host
//! built before it would answer from nothing forever.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::{ResolvedVolume, Volume};
use cmdr_operation_log::rollback::VolumeLookup;

use crate::file_system::volume::manager::{get_volume_manager, path_routes_over_its_parent};
use crate::unregistered_volumes::{Unregistered, why_unregistered};
use cmdr_operations::host::{OperationVolumes, UnregisteredVolume, VolumeAnswer};

/// The app's `VolumeManager`, plus `unregistered_volumes` for an id it doesn't
/// hold.
pub(crate) struct AppOperationVolumes;

impl VolumeLookup for AppOperationVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        get_volume_manager().get(volume_id)
    }
}

impl OperationVolumes for AppOperationVolumes {
    fn resolve<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, ResolvedVolume> {
        Box::pin(get_volume_manager().resolve(volume_id, path))
    }

    fn routes_over_its_parent(&self, path: &Path) -> bool {
        path_routes_over_its_parent(path)
    }

    fn path_is_inside_archive<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(get_volume_manager().path_is_inside_archive(volume_id, path))
    }

    fn path_crosses_archive_boundary<'a>(&'a self, volume_id: &'a str, path: &'a Path) -> VolumeAnswer<'a, bool> {
        Box::pin(get_volume_manager().path_crosses_archive_boundary(volume_id, path))
    }

    fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        get_volume_manager().find_by_root(root)
    }

    fn forget_archive_index(&self, archive_path: &Path) {
        get_volume_manager().forget_archive_index(archive_path);
    }

    fn on_volume_arrival(&self, listener: Box<dyn Fn(&str) + Send + Sync>) {
        get_volume_manager().on_volume_arrival(listener);
    }

    fn unregistered<'a>(&'a self, volume_id: &'a str) -> VolumeAnswer<'a, UnregisteredVolume> {
        Box::pin(async move {
            match why_unregistered(volume_id).await {
                Unregistered::NotConnected => UnregisteredVolume::NotConnected,
                Unregistered::Gone => UnregisteredVolume::Gone,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::file_system::volume::InMemoryVolume;
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;

    /// The adapter is the registry, asked live: a volume registered AFTER the
    /// adapter exists is served by it, by id and through `resolve`, and an id
    /// nothing lists reads as gone.
    #[tokio::test]
    async fn the_adapter_answers_from_the_live_registry() {
        let volumes = AppOperationVolumes;
        let volume_id = "test://operations-host/volumes/live";
        assert!(VolumeLookup::get(&volumes, volume_id).is_none());

        let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Live"));
        let _registration = TestVolumeRegistration::install(volume_id, Arc::clone(&volume));

        let by_id = VolumeLookup::get(&volumes, volume_id).expect("registered after the adapter was built");
        assert!(Arc::ptr_eq(&by_id, &volume));
        let path = PathBuf::from("/a/plain/file.txt");
        let resolved = volumes.resolve(volume_id, &path).await;
        assert!(resolved.volume.as_ref().is_some_and(|v| Arc::ptr_eq(v, &volume)));
        assert_eq!(resolved.path, path);
        assert!(!resolved.is_routed());
        assert!(!volumes.routes_over_its_parent(&path));

        assert_eq!(
            volumes.unregistered("test://operations-host/volumes/nobody").await,
            UnregisteredVolume::Gone
        );
    }
}
