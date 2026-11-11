//! Windows: no volume discovery yet. Lists the root and three home folders, the answer the
//! pre-seam stub gave; everything that reads a mount table answers "nothing here", and the
//! watcher has nothing to watch.

use std::path::Path;

use crate::volumes::{DEFAULT_VOLUME_ID, LocationCategory, LocationInfo, NetworkMount, SmbMountInfo, VolumeInfo};
use cmdr_fs::volume::SpaceInfo;

/// What the synthetic `network` volume reports as its `fs_type`: the SMB name the other arms
/// report, since this one mounts no shares.
pub(crate) const NETWORK_FS_TYPE: &str = "smbfs";

/// The home folders that exist, then the root, then home itself.
pub(crate) fn list_locations() -> Vec<LocationInfo> {
    let home = dirs::home_dir().unwrap_or_default();
    let favorites = [
        (home.join("Desktop"), "Desktop", "fav-desktop"),
        (home.join("Documents"), "Documents", "fav-documents"),
        (home.join("Downloads"), "Downloads", "fav-downloads"),
    ];
    let mut locations: Vec<LocationInfo> = favorites
        .into_iter()
        .filter(|(path, _, _)| path.exists())
        .map(|(path, name, id)| row(id, name, &path, LocationCategory::Favorite))
        .collect();
    locations.push(row(
        DEFAULT_VOLUME_ID,
        "Root",
        Path::new("/"),
        LocationCategory::MainVolume,
    ));
    locations.push(row("home", "Home", &home, LocationCategory::Favorite));
    locations
}

fn row(id: &str, name: &str, path: &Path, category: LocationCategory) -> LocationInfo {
    LocationInfo {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        category,
        icon: None,
        is_ejectable: false,
        fs_type: None,
        supports_trash: true,
        mount_is_read_only: false,
        is_disk_image: false,
        shares_disk_with_others: false,
        is_cloud_mount: false,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    }
}

pub(crate) fn resolve_path_volume_fast(_path: &str) -> Option<VolumeInfo> {
    None
}

pub(crate) fn network_mount_under(_path: &Path) -> Option<NetworkMount> {
    None
}

/// Nothing to copy: the listing publishes no row a backend serves.
pub(crate) fn enrich_from_volume_registry(_volumes: &mut [LocationInfo]) {}

pub(crate) fn get_smb_mount_info(_mount_path: &str) -> Option<SmbMountInfo> {
    None
}

pub(crate) fn smb_mount_info_at(_mount_point: &str) -> Option<SmbMountInfo> {
    None
}

pub(crate) fn smb_mounts() -> Option<Vec<(String, SmbMountInfo)>> {
    Some(Vec::new())
}

pub(crate) fn mount_rows() -> Option<Vec<(String, String, String)>> {
    Some(Vec::new())
}

pub(crate) fn registrable_mount_roots() -> Option<Vec<String>> {
    Some(Vec::new())
}

pub(crate) fn get_mount_point(_path: &str) -> Option<(String, String, String)> {
    None
}

pub(crate) fn volume_id_for_mount(_mount_path: &str) -> String {
    DEFAULT_VOLUME_ID.to_string()
}

pub(crate) fn mount_identity_at(_path: &str) -> Option<u64> {
    None
}

pub(crate) fn has_mount_identity(_identity: u64) -> Option<bool> {
    None
}

pub(crate) fn mount_type_and_source_for(_path: &Path) -> Option<(String, String)> {
    None
}

pub(crate) fn live_volume_space(_path: &str) -> Option<SpaceInfo> {
    None
}

/// No mount watching yet.
pub(crate) mod watcher {
    use tauri::AppHandle;

    pub(crate) fn start_volume_watcher(_app: &AppHandle) {}

    pub(crate) fn stop_volume_watcher() {}
}
