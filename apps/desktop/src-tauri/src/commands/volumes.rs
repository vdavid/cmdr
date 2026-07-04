//! Tauri commands for volume operations.

use serde::Serialize;
use tokio::time::Duration;

use super::util::{TimedOut, blocking_with_timeout_flag};
use crate::location::{Location, ResolveLocationResult};
use crate::volumes::{self, DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo, VolumeSpaceInfo};

/// Result of resolving a path to its containing volume.
/// Unlike `TimedOut<Option<VolumeInfo>>`, `timed_out: true` means "the filesystem
/// didn't respond, we genuinely don't know" (not "here's a fallback").
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PathVolumeResolution {
    pub volume: Option<VolumeInfo>,
    pub timed_out: bool,
}

const VOLUME_TIMEOUT: Duration = Duration::from_secs(2);

/// Lists all mounted volumes, including connected MTP devices.
/// Enriches SMB volumes with their connection state from the VolumeManager.
#[tauri::command]
#[specta::specta]
pub async fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    let mut result = blocking_with_timeout_flag(VOLUME_TIMEOUT, vec![], volumes::list_mounted_volumes).await;
    append_mtp_volumes(&mut result.data).await;
    volumes::enrich_smb_connection_state(&mut result.data);
    result
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
#[specta::specta]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Gets space information for a volume at the given path.
/// Returns total and available bytes for the volume.
/// For MTP paths (`mtp://`), fetches from the MTP connection manager instead of macOS NSURL.
#[tauri::command]
#[specta::specta]
pub async fn get_volume_space(path: String) -> TimedOut<Option<VolumeSpaceInfo>> {
    if let Some(space) = get_mtp_space_info(&path).await {
        return TimedOut {
            data: Some(space),
            timed_out: false,
        };
    }
    blocking_with_timeout_flag(VOLUME_TIMEOUT, None, move || volumes::get_volume_space(&path)).await
}

/// Resolves a path to its containing volume without enumerating all volumes.
/// Uses `statfs()` for filesystem paths (<1ms for local disks), protocol
/// dispatch for MTP/SMB paths. Returns `timed_out: true` if the filesystem
/// didn't respond within 2s.
#[tauri::command]
#[specta::specta]
pub async fn resolve_path_volume(path: String) -> PathVolumeResolution {
    let (volume, timed_out) = resolve_path_to_volume(path).await;
    PathVolumeResolution { volume, timed_out }
}

/// Resolves a path to a `Location` (`volume_id` + the path itself), the
/// canonical path→volume resolver for navigation edges. Shares the full
/// protocol dispatch with `resolve_path_volume`, so `mtp://` / `smb://` virtual
/// paths resolve correctly (calling `resolve_path_volume_fast` alone would
/// return `None` for them). `location: None` means no volume contains the path;
/// `timed_out: true` means the filesystem didn't respond.
#[tauri::command]
#[specta::specta]
pub async fn resolve_location(path: String) -> ResolveLocationResult {
    let (volume, timed_out) = resolve_path_to_volume(path.clone()).await;
    ResolveLocationResult {
        location: volume.map(|v| Location { volume_id: v.id, path }),
        timed_out,
    }
}

/// Shared body for `resolve_path_volume` and `resolve_location`: resolves a path
/// to its containing volume via protocol dispatch (`mtp://` → matching connected
/// storage, `smb://` → the virtual `network` volume) or, for filesystem paths,
/// `statfs` under a timeout. Returns the volume (if any) and whether it timed out.
async fn resolve_path_to_volume(path: String) -> (Option<VolumeInfo>, bool) {
    // MTP protocol dispatch
    if path.starts_with("mtp://") {
        return (find_mtp_volume_for_path(&path).await, false);
    }

    // SMB/network protocol paths → return the virtual network volume
    if path.starts_with("smb://") {
        return (
            Some(VolumeInfo {
                id: "network".to_string(),
                name: "Network".to_string(),
                path: "smb://".to_string(),
                category: LocationCategory::Network,
                icon: None,
                is_ejectable: false,
                fs_type: Some("smbfs".to_string()),
                supports_trash: false,
                is_read_only: false,
                is_disk_image: false,
                smb_connection_state: None,
                usb_speed: None,
            }),
            false,
        );
    }

    // Filesystem paths: resolve via statfs with timeout. A path INSIDE an archive
    // resolves to the PARENT drive (display semantics — the FE holds the parent
    // drive id, never a per-archive id), so statfs the `.zip`'s real location, not
    // the inner path (which isn't a real FS path). The boundary check runs inside
    // the timeout-wrapped closure so its stat can't block IPC on a hung mount.
    let result = blocking_with_timeout_flag(VOLUME_TIMEOUT, None, move || {
        let fs_path = match crate::file_system::volume::backends::archive::confirm_archive_boundary(
            std::path::Path::new(&path),
        ) {
            Some((zip_path, _inner)) => zip_path,
            None => std::path::PathBuf::from(&path),
        };
        volumes::resolve_path_volume_fast(&fs_path.to_string_lossy())
    })
    .await;
    (result.data, result.timed_out)
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
    // Match on the path prefix (mtp://device_id/storage_id)
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
                is_disk_image: false,
                fs_type: Some("mtp".to_string()),
                supports_trash: false,
                smb_connection_state: None,
                usb_speed: device.device.usb_speed,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_location_local_dir_returns_root_volume() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().to_string_lossy().to_string();

        let result = resolve_location(path.clone()).await;

        assert!(!result.timed_out);
        let location = result.location.expect("local dir should resolve to a volume");
        // The temp dir lives on the boot volume.
        assert_eq!(location.volume_id, DEFAULT_VOLUME_ID);
        // The resolved path is the input path (the dir the caller wants to land on).
        assert_eq!(location.path, path);
    }

    #[tokio::test]
    async fn resolve_location_local_file_returns_root_volume() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, b"hi").expect("write temp file");
        let path = file_path.to_string_lossy().to_string();

        let result = resolve_location(path.clone()).await;

        assert!(!result.timed_out);
        let location = result.location.expect("local file should resolve to a volume");
        assert_eq!(location.volume_id, DEFAULT_VOLUME_ID);
        assert_eq!(location.path, path);
    }

    #[tokio::test]
    async fn resolve_location_inside_an_archive_returns_the_parent_drive() {
        // A path INSIDE a `.zip` resolves to the parent drive (display semantics),
        // not `None` — so restoring a pane deep-linked inside an archive works. The
        // inner path isn't a real FS path, so this only works by resolving the
        // `.zip`'s real location.
        let dir = tempfile::tempdir().expect("create temp dir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");
        let inner = zip.join("docs/readme.txt");
        let path = inner.to_string_lossy().to_string();

        let result = resolve_location(path.clone()).await;

        assert!(!result.timed_out);
        let location = result
            .location
            .expect("archive-inner path should resolve to the parent drive");
        assert_eq!(location.volume_id, DEFAULT_VOLUME_ID);
        // The returned path is the full inner path the caller wants to land on.
        assert_eq!(location.path, path);
    }

    #[tokio::test]
    async fn resolve_location_unresolvable_mtp_path_returns_none() {
        // No MTP device is connected in tests, so the protocol-dispatch branch
        // finds no matching storage and yields `location: None` (proving
        // `resolve_location` runs the full dispatch, not just the local helper).
        let result = resolve_location("mtp://no-such-device/1/folder".to_string()).await;

        assert!(!result.timed_out);
        assert!(result.location.is_none());
    }
}
