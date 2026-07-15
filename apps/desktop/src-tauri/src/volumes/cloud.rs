//! Cloud-drive discovery: enumerating iCloud Drive and `~/Library/CloudStorage`
//! providers into switcher entries, and resolving an arbitrary path to the
//! cloud drive that contains it.

use super::*;
use std::path::{Path, PathBuf};

/// Get cloud drives (Dropbox, iCloud, Google Drive, etc.).
pub fn get_cloud_drives() -> Vec<LocationInfo> {
    // Skip during FDA-pending onboarding: enumerating `~/Library/CloudStorage`
    // touches an FDA-gated path. The list re-emits via `volumes-changed`
    // once the gate clears (see `start_indexing_after_fda_decision`).
    if crate::fda_gate::is_fda_pending_runtime() {
        return Vec::new();
    }

    let mut drives = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // iCloud Drive
    let icloud_path = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if icloud_path.exists() {
        drives.push(cloud_volume_info(
            ICLOUD_VOLUME_ID.to_string(),
            "iCloud Drive".to_string(),
            &icloud_path,
        ));
    }

    // Scan ~/Library/CloudStorage for other cloud providers
    let cloud_storage_path = home.join("Library/CloudStorage");
    if let Ok(entries) = std::fs::read_dir(&cloud_storage_path) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Parse cloud provider name from directory
                let (provider_name, id) = parse_cloud_provider_name(dir_name);
                if !provider_name.is_empty() {
                    drives.push(cloud_volume_info(id, provider_name, &path));
                }
            }
        }
    }

    // Sort alphabetically
    drives.sort_by_key(|a| a.name.to_lowercase());
    drives
}

/// Build a `CloudDrive` [`LocationInfo`] for a cloud-drive root folder. Shared
/// by [`get_cloud_drives`] (the switcher list) and [`resolve_cloud_drive_for_path`]
/// (the per-path resolver) so the two can't drift on ID, category, or fields.
fn cloud_volume_info(id: String, name: String, root: &Path) -> LocationInfo {
    let path = root.to_string_lossy().to_string();
    let fs_type = get_fs_type(&path);
    let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());
    LocationInfo {
        id,
        name,
        icon: get_icon_for_path(&path),
        path,
        category: LocationCategory::CloudDrive,
        is_ejectable: false,
        fs_type,
        supports_trash,
        is_read_only: false,
        is_disk_image: false,
        smb_connection_state: None,
        usb_speed: None,
    }
}

/// Resolve a path to its containing cloud drive, if any. Returns the same
/// `VolumeInfo` shape [`get_cloud_drives`] would for that drive, so the volume
/// switcher's checkmark (matched by `id`) lands on it.
pub(crate) fn resolve_cloud_drive_for_path(path: &str) -> Option<VolumeInfo> {
    let home = dirs::home_dir()?;
    let (id, name, root) = match_cloud_drive_root(&home, path)?;
    Some(cloud_volume_info(id, name, &root))
}

/// If `path` is the root of, or anywhere inside, a known cloud-drive folder,
/// return `(volume_id, display_name, cloud_root)`.
///
/// Cloud drives (iCloud Drive, Dropbox, Google Drive, …) are plain folders on
/// the data volume, so `statfs` resolves any path inside them to `/`. Without
/// this match, the volume switcher would highlight "Macintosh HD" instead of
/// the cloud drive whenever the user is anywhere inside one.
///
/// Pure (no I/O, matches by path prefix only) so it's unit-testable and cheap
/// to call on every navigation. The I/O wrapper is [`resolve_cloud_drive_for_path`].
fn match_cloud_drive_root(home: &Path, path: &str) -> Option<(String, String, PathBuf)> {
    let candidate = Path::new(path);

    // iCloud Drive: a fixed folder under the home directory.
    let icloud_root = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if candidate.starts_with(&icloud_root) {
        return Some((ICLOUD_VOLUME_ID.to_string(), "iCloud Drive".to_string(), icloud_root));
    }

    // Other providers: ~/Library/CloudStorage/<provider-dir>/… The first path
    // component under CloudStorage names the provider; deeper components are
    // subfolders we want to attribute to that same drive.
    let cloud_storage_root = home.join("Library/CloudStorage");
    let rel = candidate.strip_prefix(&cloud_storage_root).ok()?;
    let provider_dir = rel.components().next()?.as_os_str().to_str()?;
    let (name, id) = parse_cloud_provider_name(provider_dir);
    if name.is_empty() {
        return None;
    }
    Some((id, name, cloud_storage_root.join(provider_dir)))
}

/// Parse cloud provider name from CloudStorage directory name.
/// E.g., "Dropbox" -> "Dropbox", "GoogleDrive-email@gmail.com" -> "Google Drive"
fn parse_cloud_provider_name(dir_name: &str) -> (String, String) {
    if dir_name.starts_with("Dropbox") {
        return ("Dropbox".to_string(), "cloud-dropbox".to_string());
    }
    if dir_name.starts_with("GoogleDrive") {
        return ("Google Drive".to_string(), "cloud-google-drive".to_string());
    }
    if dir_name.starts_with("OneDrive") {
        // Handle OneDrive-Personal, OneDrive-Business, etc.
        if dir_name.contains("Business") {
            return (
                "OneDrive for Business".to_string(),
                "cloud-onedrive-business".to_string(),
            );
        }
        return ("OneDrive".to_string(), "cloud-onedrive".to_string());
    }
    if dir_name.starts_with("Box") {
        return ("Box".to_string(), "cloud-box".to_string());
    }
    if dir_name.starts_with("pCloud") {
        return ("pCloud".to_string(), "cloud-pcloud".to_string());
    }
    // Generic cloud provider
    if !dir_name.is_empty() {
        let clean_name = dir_name.split('-').next().unwrap_or(dir_name);
        return (clean_name.to_string(), format!("cloud-{}", clean_name.to_lowercase()));
    }
    (String::new(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cloud_provider_name() {
        assert_eq!(
            parse_cloud_provider_name("Dropbox"),
            ("Dropbox".to_string(), "cloud-dropbox".to_string())
        );
        assert_eq!(
            parse_cloud_provider_name("GoogleDrive-user@gmail.com"),
            ("Google Drive".to_string(), "cloud-google-drive".to_string())
        );
        assert_eq!(
            parse_cloud_provider_name("OneDrive-Personal"),
            ("OneDrive".to_string(), "cloud-onedrive".to_string())
        );
    }

    #[test]
    fn test_match_cloud_drive_root() {
        let home = Path::new("/Users/test");
        let id = |p: &str| match_cloud_drive_root(home, p).map(|(id, ..)| id);

        // iCloud Drive: root and any descendant resolve to the iCloud volume.
        let icloud = "/Users/test/Library/Mobile Documents/com~apple~CloudDocs";
        assert_eq!(id(icloud).as_deref(), Some("cloud-icloud"));
        assert_eq!(
            id(&format!("{icloud}/Projects/notes.md")).as_deref(),
            Some("cloud-icloud")
        );

        // Dropbox: the bug repro. A deep subfolder must still highlight Dropbox.
        let dropbox = "/Users/test/Library/CloudStorage/Dropbox";
        assert_eq!(id(dropbox).as_deref(), Some("cloud-dropbox"));
        assert_eq!(
            id(&format!("{dropbox}/Work/2026/Q2/report.pdf")).as_deref(),
            Some("cloud-dropbox")
        );

        // Google Drive: the CloudStorage dir carries the account suffix.
        assert_eq!(
            id("/Users/test/Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive/x").as_deref(),
            Some("cloud-google-drive")
        );

        // Non-cloud paths resolve to no cloud drive (statfs handles them).
        assert_eq!(id("/"), None);
        assert_eq!(id("/Users/test/Documents"), None);
        assert_eq!(id("/Volumes/External/photos"), None);
        // The CloudStorage container itself isn't a cloud drive.
        assert_eq!(id("/Users/test/Library/CloudStorage"), None);
        // A sibling that merely shares a name prefix must not match (component-wise).
        assert_eq!(
            id("/Users/test/Library/Mobile Documents/com~apple~CloudDocsBackup"),
            None
        );

        // The full tuple carries name and the cloud root (not the navigated subpath).
        let (id, name, root) = match_cloud_drive_root(home, &format!("{dropbox}/Work")).expect("Dropbox match");
        assert_eq!(id, "cloud-dropbox");
        assert_eq!(name, "Dropbox");
        assert_eq!(root, PathBuf::from(dropbox));
    }
}
