//! Tauri commands for volume operations.

use tokio::time::Duration;

use super::util::{TimedOut, blocking_with_timeout_flag};
use crate::volumes::{self, DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo, VolumeSpaceInfo};

const VOLUME_TIMEOUT: Duration = Duration::from_secs(2);

/// Lists all mounted volumes, including connected MTP devices.
#[tauri::command]
pub async fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    let mut result = blocking_with_timeout_flag(VOLUME_TIMEOUT, vec![], volumes::list_mounted_volumes).await;
    append_mtp_volumes(&mut result.data).await;
    result
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Finds the actual volume (not a favorite) that contains a given path.
/// Returns the volume info for the best matching volume, excluding favorites.
/// This is used to determine which volume to set as active when a favorite is chosen.
#[tauri::command]
pub async fn find_containing_volume(path: String) -> TimedOut<Option<VolumeInfo>> {
    let mut result = blocking_with_timeout_flag(VOLUME_TIMEOUT, vec![], volumes::list_locations).await;
    append_mtp_volumes(&mut result.data).await;

    // Only consider actual volumes, not favorites
    let volumes: Vec<_> = result
        .data
        .into_iter()
        .filter(|loc| loc.category != LocationCategory::Favorite)
        .collect();

    // Find the volume with the longest matching path prefix
    let mut best_match: Option<VolumeInfo> = None;
    let mut best_len = 0;

    for vol in volumes {
        if path.starts_with(&vol.path) && vol.path.len() > best_len {
            best_len = vol.path.len();
            best_match = Some(vol);
        }
    }

    TimedOut {
        data: best_match,
        timed_out: result.timed_out,
    }
}

/// Gets space information for a volume at the given path.
/// Returns total and available bytes for the volume.
/// For MTP paths (`mtp://`), fetches from the MTP connection manager instead of macOS NSURL.
#[tauri::command]
pub async fn get_volume_space(path: String) -> TimedOut<Option<VolumeSpaceInfo>> {
    if let Some(space) = get_mtp_space_info(&path).await {
        return TimedOut {
            data: Some(space),
            timed_out: false,
        };
    }
    blocking_with_timeout_flag(VOLUME_TIMEOUT, None, move || volumes::get_volume_space(&path)).await
}

/// Appends connected MTP device storages to the volume list.
/// Each storage becomes a separate volume entry with category `MobileDevice`.
async fn append_mtp_volumes(volumes: &mut Vec<VolumeInfo>) {
    let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
    for device in devices {
        let multi = device.storages.len() > 1;
        let device_name = device
            .device
            .product
            .as_deref()
            .or(device.device.manufacturer.as_deref())
            .unwrap_or("Mobile device");
        for storage in &device.storages {
            let name = if multi {
                format!("{} - {}", device_name, storage.name)
            } else {
                device_name.to_string()
            };
            volumes.push(VolumeInfo {
                id: format!("{}:{}", device.device.id, storage.id),
                name,
                path: format!("mtp://{}/{}", device.device.id, storage.id),
                category: LocationCategory::MobileDevice,
                icon: None,
                is_ejectable: true,
                is_read_only: storage.is_read_only,
                fs_type: Some("mtp".to_string()),
                supports_trash: false,
            });
        }
    }
}

/// Queries live MTP space info from a `mtp://{device_id}/{storage_id}/...` path.
async fn get_mtp_space_info(path: &str) -> Option<VolumeSpaceInfo> {
    let rest = path.strip_prefix("mtp://")?;
    let mut parts = rest.splitn(3, '/');
    let device_id = parts.next()?;
    let storage_id: u32 = parts.next()?.parse().ok()?;

    let (total_bytes, available_bytes) = crate::mtp::connection_manager()
        .get_live_storage_space(device_id, storage_id)
        .await?;
    Some(VolumeSpaceInfo {
        total_bytes,
        available_bytes,
    })
}
