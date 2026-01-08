//! Volume and location discovery for macOS.
//!
//! Provides a Finder-like location picker with:
//! - Favorites (from Finder sidebar)
//! - Main volume (Macintosh HD)
//! - Attached volumes (external drives)
//! - Cloud drives (Dropbox, iCloud, Google Drive, etc.)
//! - Network locations

pub mod watcher;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Category of a location item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationCategory {
    /// User's favorite locations from Finder sidebar.
    Favorite,
    /// Main boot volume (Macintosh HD).
    MainVolume,
    /// External/attached volumes (USB drives, etc.).
    AttachedVolume,
    /// Cloud storage providers (Dropbox, iCloud, Google Drive).
    CloudDrive,
    /// Network locations.
    Network,
}

/// Information about a location (volume, folder, or cloud drive).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    /// Unique identifier for the location.
    pub id: String,
    /// Display name (e.g., "Macintosh HD", "Dropbox").
    pub name: String,
    /// Path to the location.
    pub path: String,
    /// Category of this location.
    pub category: LocationCategory,
    /// Base64-encoded icon (WebP format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Whether this can be ejected.
    pub is_ejectable: bool,
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Get all locations organized by category, deduplicated.
pub fn list_locations() -> Vec<LocationInfo> {
    let mut locations = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    // 1. Favorites
    for loc in get_favorites() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 2. Main volume
    if let Some(loc) = get_main_volume()
        && seen_paths.insert(loc.path.clone())
    {
        locations.push(loc);
    }

    // 3. Attached volumes
    for loc in get_attached_volumes() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 4. Cloud drives (skip if already in favorites)
    for loc in get_cloud_drives() {
        if seen_paths.insert(loc.path.clone()) {
            locations.push(loc);
        }
    }

    // 5. Network - commented out for now as /Network requires special handling
    // for loc in get_network_locations() {
    //     if seen_paths.insert(loc.path.clone()) {
    //         locations.push(loc);
    //     }
    // }

    locations
}

/// Get Finder favorites (common user folders).
fn get_favorites() -> Vec<LocationInfo> {
    let home = dirs::home_dir().unwrap_or_default();
    let desktop = home.join("Desktop");
    let documents = home.join("Documents");
    let downloads = home.join("Downloads");
    let desktop_str = desktop.to_string_lossy();
    let documents_str = documents.to_string_lossy();
    let downloads_str = downloads.to_string_lossy();
    let favorites_paths = [
        ("/Applications", "Applications"),
        (desktop_str.as_ref(), "Desktop"),
        (documents_str.as_ref(), "Documents"),
        (downloads_str.as_ref(), "Downloads"),
    ];

    favorites_paths
        .into_iter()
        .filter(|(path, _)| Path::new(*path).exists())
        .map(|(path, name)| LocationInfo {
            id: format!("fav-{}", name.to_lowercase()),
            name: name.to_string(),
            path: path.to_string(),
            category: LocationCategory::Favorite,
            icon: get_icon_for_path(path),
            is_ejectable: false,
        })
        .collect()
}

/// Get the main boot volume.
fn get_main_volume() -> Option<LocationInfo> {
    use objc2_foundation::{NSArray, NSFileManager, NSURL, NSVolumeEnumerationOptions};

    let file_manager = NSFileManager::defaultManager();
    let options = NSVolumeEnumerationOptions::SkipHiddenVolumes;

    let volume_urls: Option<objc2::rc::Retained<NSArray<NSURL>>> =
        file_manager.mountedVolumeURLsIncludingResourceValuesForKeys_options(None, options);

    let urls = volume_urls?;

    for url in urls.iter() {
        let path_str = url.path()?;
        let path = path_str.to_string();

        // Root volume
        if path == "/" {
            let name = get_volume_name(&url, &path);
            return Some(LocationInfo {
                id: DEFAULT_VOLUME_ID.to_string(),
                name,
                path,
                category: LocationCategory::MainVolume,
                icon: get_icon_for_path("/"),
                is_ejectable: false,
            });
        }
    }
    None
}

/// Get attached volumes (external drives, USB, etc.).
fn get_attached_volumes() -> Vec<LocationInfo> {
    use objc2_foundation::{NSArray, NSFileManager, NSURL, NSVolumeEnumerationOptions};

    let file_manager = NSFileManager::defaultManager();
    let options = NSVolumeEnumerationOptions::SkipHiddenVolumes;

    let volume_urls: Option<objc2::rc::Retained<NSArray<NSURL>>> =
        file_manager.mountedVolumeURLsIncludingResourceValuesForKeys_options(None, options);

    let Some(urls) = volume_urls else {
        return vec![];
    };

    let mut volumes = Vec::new();

    for url in urls.iter() {
        let Some(path_str) = url.path() else { continue };
        let path = path_str.to_string();

        // Skip root (already handled as main volume)
        if path == "/" {
            continue;
        }

        // Skip system volumes
        if path.starts_with("/System") || path.contains("/Preboot") || path.contains("/Recovery") {
            continue;
        }

        // Skip cloud storage (handled separately)
        if path.contains("/Library/CloudStorage") {
            continue;
        }

        // Only include /Volumes/* paths (actual mounted volumes)
        if !path.starts_with("/Volumes/") {
            continue;
        }

        let name = get_volume_name(&url, &path);
        let is_ejectable = get_bool_resource(&url, "NSURLVolumeIsEjectableKey").unwrap_or(false);

        volumes.push(LocationInfo {
            id: path_to_id(&path),
            name,
            path: path.clone(),
            category: LocationCategory::AttachedVolume,
            icon: get_icon_for_path(&path),
            is_ejectable,
        });
    }

    // Sort alphabetically
    volumes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    volumes
}

/// Get cloud drives (Dropbox, iCloud, Google Drive, etc.).
fn get_cloud_drives() -> Vec<LocationInfo> {
    let mut drives = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // iCloud Drive
    let icloud_path = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if icloud_path.exists() {
        drives.push(LocationInfo {
            id: "cloud-icloud".to_string(),
            name: "iCloud Drive".to_string(),
            path: icloud_path.to_string_lossy().to_string(),
            category: LocationCategory::CloudDrive,
            icon: get_icon_for_path(&icloud_path.to_string_lossy()),
            is_ejectable: false,
        });
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
                    drives.push(LocationInfo {
                        id,
                        name: provider_name,
                        path: path.to_string_lossy().to_string(),
                        category: LocationCategory::CloudDrive,
                        icon: get_icon_for_path(&path.to_string_lossy()),
                        is_ejectable: false,
                    });
                }
            }
        }
    }

    // Sort alphabetically
    drives.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    drives
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

/// Get network locations.
#[allow(dead_code)]
fn get_network_locations() -> Vec<LocationInfo> {
    let mut locations = Vec::new();

    // Always include Network like Finder does
    // Even if /Network doesn't exist as a directory, it's a browseable location in Finder
    let network_path = "/Network";
    locations.push(LocationInfo {
        id: "network".to_string(),
        name: "Network".to_string(),
        path: network_path.to_string(),
        category: LocationCategory::Network,
        icon: None, // Will use placeholder in frontend
        is_ejectable: false,
    });

    locations
}

/// Get the display name for a volume.
fn get_volume_name(url: &objc2_foundation::NSURL, path: &str) -> String {
    // Try localized name first
    if let Some(name) = get_string_resource(url, "NSURLVolumeLocalizedNameKey") {
        return name;
    }
    if let Some(name) = get_string_resource(url, "NSURLVolumeNameKey") {
        return name;
    }
    // Fallback to path-based name
    if path == "/" {
        "Macintosh HD".to_string()
    } else {
        Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }
}

/// Convert path to a safe ID.
fn path_to_id(path: &str) -> String {
    if path == "/" {
        return DEFAULT_VOLUME_ID.to_string();
    }
    path.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase()
}

/// Get icon for a path as base64-encoded WebP.
fn get_icon_for_path(path: &str) -> Option<String> {
    crate::icons::get_icon_for_path(path)
}

/// Get a boolean resource value from an NSURL.
fn get_bool_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<bool> {
    use objc2::rc::Retained;
    use objc2_foundation::{NSNumber, NSString};

    let key = NSString::from_str(key);
    let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
    let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

    if success.is_ok() {
        value.and_then(|obj| obj.downcast::<NSNumber>().ok().map(|n| n.boolValue()))
    } else {
        None
    }
}

/// Get a string resource value from an NSURL.
fn get_string_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<String> {
    use objc2::rc::Retained;
    use objc2_foundation::NSString;

    let key = NSString::from_str(key);
    let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
    let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

    if success.is_ok() {
        value.and_then(|obj| obj.downcast::<NSString>().ok().map(|s| s.to_string()))
    } else {
        None
    }
}

// Legacy compatibility - maintain VolumeInfo type for backwards compatibility
pub use LocationInfo as VolumeInfo;

/// Legacy function - now calls list_locations
pub fn list_mounted_volumes() -> Vec<LocationInfo> {
    list_locations()
}

#[allow(dead_code)]
pub fn find_volume_for_path(path: &str) -> Option<String> {
    let locations = list_locations();
    let mut best_match: Option<&LocationInfo> = None;
    let mut best_len = 0;

    for loc in &locations {
        if path.starts_with(&loc.path) && loc.path.len() > best_len {
            best_match = Some(loc);
            best_len = loc.path.len();
        }
    }

    best_match.map(|v| v.id.clone())
}

#[allow(dead_code)]
pub fn is_volume_mounted(volume_id: &str) -> bool {
    list_locations().iter().any(|v| v.id == volume_id)
}

#[allow(dead_code)]
pub fn get_volume_by_id(volume_id: &str) -> Option<LocationInfo> {
    list_locations().into_iter().find(|v| v.id == volume_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_locations_includes_root() {
        let locations = list_locations();
        assert!(!locations.is_empty(), "Should have at least one location");
        // Should have main volume
        assert!(
            locations.iter().any(|l| l.category == LocationCategory::MainVolume),
            "Should include main volume"
        );
    }

    #[test]
    fn test_locations_are_deduplicated() {
        let locations = list_locations();
        let mut seen_paths = HashSet::new();
        for loc in &locations {
            assert!(seen_paths.insert(&loc.path), "Duplicate path found: {}", loc.path);
        }
    }

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
    fn test_path_to_id() {
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/Volumes/External"), "volumesexternal");
    }
}
