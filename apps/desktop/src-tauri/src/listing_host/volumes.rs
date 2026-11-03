//! The volume registry, answering the listing engine's "which volume serves
//! this path?".

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::{ResolvedVolume, Volume};

use crate::file_system::volume::manager::get_volume_manager;
use cmdr_listing::host::{ListingVolumes, Unregistered};

/// How long the last-chance mount adoption may take before the listing gives up
/// and reports the volume missing. It reads the mount table and one NSURL
/// resource, which is sub-millisecond locally and can hang on a wedged mount, so
/// it's bounded like every other mount-touching read (`commands/CLAUDE.md`).
#[cfg(any(target_os = "macos", target_os = "linux"))]
const ADOPT_MOUNT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Answers from the app's `VolumeManager`, resolved per call.
pub(crate) struct AppVolumes;

impl ListingVolumes for AppVolumes {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        get_volume_manager().get(volume_id)
    }

    fn resolve<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ResolvedVolume> + Send + 'a>> {
        // Every resolve the engine makes is for a listing, which enters an archive's
        // root where a caller acting on the path would get the file.
        Box::pin(get_volume_manager().resolve_to_list(volume_id, path))
    }

    fn resolve_local_only(&self, volume_id: &str, path: &Path) -> ResolvedVolume {
        get_volume_manager().resolve_local_only(volume_id, path)
    }

    /// Before calling an id gone, asks whether the mount under `path` IS this
    /// volume and nobody had registered it: a cloud drive that mounted into the
    /// home folder posts no `NSWorkspace` notification, and the startup sweep
    /// runs off the main thread, so a restored tab can resolve ahead of it.
    /// Adoption only ever confirms the caller's own id
    /// (`mount_registration::adopt_mount_serving`). Its cost is paid on a path
    /// that was about to fail anyway. The why of the other two answers:
    /// `crate::unregistered_volumes`.
    fn unregistered<'a>(
        &'a self,
        volume_id: &'a str,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Unregistered> + Send + 'a>> {
        Box::pin(async move {
            if let Some(adopted) = adopt_mount(volume_id, path).await {
                return Unregistered::Adopted(adopted);
            }
            match crate::unregistered_volumes::why_unregistered(volume_id).await {
                crate::unregistered_volumes::Unregistered::NotConnected => Unregistered::NotConnected,
                crate::unregistered_volumes::Unregistered::Gone => Unregistered::Gone,
            }
        })
    }
}

/// Registers the live mount serving `path` when it derives exactly `volume_id`,
/// off the async thread and under [`ADOPT_MOUNT_TIMEOUT`].
async fn adopt_mount(volume_id: &str, path: &Path) -> Option<Arc<dyn Volume>> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let volume_id = volume_id.to_string();
        let path = path.to_path_buf();
        cmdr_fs::deadline::blocking_with_timeout(ADOPT_MOUNT_TIMEOUT, None, move || {
            crate::file_system::volume::mount_registration::adopt_mount_serving(&volume_id, &path)
        })
        .await
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (volume_id, path);
        None
    }
}
