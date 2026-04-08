//! Tauri commands for volume operations on Linux.

use serde::Serialize;
use tokio::time::Duration;

use super::util::{TimedOut, blocking_with_timeout_flag};
use crate::volumes_linux::{self, DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo, VolumeSpaceInfo};

const VOLUME_TIMEOUT: Duration = Duration::from_secs(2);

/// Result of resolving a path to its containing volume.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathVolumeResolution {
    pub volume: Option<VolumeInfo>,
    pub timed_out: bool,
}

/// Lists all mounted volumes, including connected MTP devices.
#[tauri::command]
pub async fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    let mut data = volumes_linux::list_mounted_volumes();
    append_mtp_volumes(&mut data).await;
    TimedOut { data, timed_out: false }
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Gets space information for a volume at the given path.
/// For MTP paths (`mtp://`), fetches from the MTP connection manager instead of statvfs.
#[tauri::command]
pub async fn get_volume_space(path: String) -> TimedOut<Option<VolumeSpaceInfo>> {
    if let Some(space) = get_mtp_space_info(&path).await {
        return TimedOut {
            data: Some(space),
            timed_out: false,
        };
    }
    TimedOut {
        data: volumes_linux::get_volume_space(&path),
        timed_out: false,
    }
}

/// Resolves a path to its containing volume without enumerating all volumes.
/// Parses `/proc/self/mountinfo` for filesystem paths, dispatches on protocol
/// for MTP/SMB. Uses `spawn_blocking` + timeout (2s).
#[tauri::command]
pub async fn resolve_path_volume(path: String) -> PathVolumeResolution {
    // MTP protocol dispatch
    if path.starts_with("mtp://") {
        let mtp_volume = find_mtp_volume_for_path(&path).await;
        return PathVolumeResolution {
            volume: mtp_volume,
            timed_out: false,
        };
    }

    // SMB/network protocol paths
    if path.starts_with("smb://") {
        return PathVolumeResolution {
            volume: Some(VolumeInfo {
                id: "network".to_string(),
                name: "Network".to_string(),
                path: "smb://".to_string(),
                category: LocationCategory::Network,
                icon: None,
                is_ejectable: false,
                fs_type: Some("cifs".to_string()),
                supports_trash: false,
                is_read_only: false,
                smb_connection_state: None,
            }),
            timed_out: false,
        };
    }

    // Filesystem paths: resolve via /proc/self/mountinfo with timeout
    let result = blocking_with_timeout_flag(VOLUME_TIMEOUT, None, move || {
        volumes_linux::resolve_path_volume_fast(&path)
    })
    .await;

    PathVolumeResolution {
        volume: result.data,
        timed_out: result.timed_out,
    }
}

/// Finds the MTP volume matching a `mtp://device_id/storage_id/...` path.
async fn find_mtp_volume_for_path(path: &str) -> Option<VolumeInfo> {
    let rest = path.strip_prefix("mtp://")?;
    let mut parts = rest.splitn(3, '/');
    let device_id = parts.next()?;
    let storage_id_str = parts.next()?;
    let _storage_id: u32 = storage_id_str.parse().ok()?;

    let mut volumes = Vec::new();
    append_mtp_volumes(&mut volumes).await;
    let prefix = format!("mtp://{}/{}", device_id, storage_id_str);
    volumes.into_iter().find(|v| v.path == prefix)
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
                smb_connection_state: None,
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
