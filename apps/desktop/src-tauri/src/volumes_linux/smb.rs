//! SMB discovery on Linux, which arrives through two unrelated doors: CIFS
//! mounts that show up in `/proc/mounts` with a `//server/share` source, and
//! GVFS shares that never appear there at all (the whole `gvfs/` dir is one FUSE
//! mount, each share a subdirectory). This module parses both and turns the GVFS
//! side into `Network` locations.

use super::{LocationCategory, LocationInfo, ids::volume_id_for_mount, linux_mounts};
use std::path::Path;

/// Enriches discovered locations with what only the registered `Volume` knows.
///
/// The twin of macOS `volumes::enrich_from_volume_registry`, and it must stay
/// one: the frontend doesn't branch on platform, so a capability published on
/// one and not the other is a pane whose buttons differ by OS. Only the
/// capability half exists here — Linux has no smb2 session tracking, so
/// `smb_connection_state` stays `None`.
pub fn enrich_from_volume_registry(volumes: &mut [LocationInfo]) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    for vol in volumes.iter_mut() {
        if let Some(registered) = manager.get(&vol.id) {
            vol.capabilities = Some(registered.capabilities());
        }
    }
}

/// Information about an SMB mount extracted from `/proc/mounts`.
#[derive(Debug, Clone)]
pub struct SmbMountInfo {
    /// Server hostname or IP (for example, "192.168.1.111").
    pub server: String,
    /// Share name (for example, "naspi").
    pub share: String,
    /// Username if present in the mount source (for example, "david").
    pub username: Option<String>,
    /// Port from the mount source (for example, 10480). Defaults to 445.
    pub port: u16,
}

/// Extracts SMB server, share, and username from a mount path via `/proc/mounts`.
///
/// On Linux, CIFS mounts have a device field like:
/// - `//192.168.1.111/share` (no credentials in device)
/// - `//user@192.168.1.111/share` (some configurations)
///
/// Returns `None` if the path is not a CIFS mount or parsing fails.
pub fn get_smb_mount_info(mount_path: &str) -> Option<SmbMountInfo> {
    let mounts = linux_mounts::parse_proc_mounts();
    let entry = mounts
        .iter()
        .filter(|e| e.fstype == "cifs")
        .find(|e| e.mountpoint == mount_path)?;
    parse_smb_mount_source(&entry.device)
}

/// Parses an SMB mount source string like `//user@host/share` or `//host/share`.
pub(super) fn parse_smb_mount_source(source: &str) -> Option<SmbMountInfo> {
    let rest = source.strip_prefix("//")?;
    let (server_part, share) = rest.split_once('/')?;
    if share.is_empty() {
        return None;
    }

    let (username, server) = if let Some((user, host)) = server_part.split_once('@') {
        (Some(user.to_string()), host.to_string())
    } else {
        (None, server_part.to_string())
    };

    // Extract port if present (for example, "192.168.1.111:10480")
    let (server, port) = if let Some((host, port_str)) = server.rsplit_once(':') {
        (host.to_string(), port_str.parse().unwrap_or(445))
    } else {
        (server, 445)
    };

    Some(SmbMountInfo {
        server,
        share: share.to_string(),
        username,
        port,
    })
}

/// Parse a GVFS SMB directory name into (server, share).
///
/// GVFS mounts SMB shares as subdirectories under `/run/user/<uid>/gvfs/`
/// with names like `smb-share:server=192.168.1.150,share=pihdd` (optionally
/// with `,user=X,domain=Y` suffixes). Returns None for non-SMB entries.
pub(crate) fn parse_gvfs_smb_dirname(dirname: &str) -> Option<(String, String)> {
    let rest = dirname.strip_prefix("smb-share:")?;
    let mut server = None;
    let mut share = None;
    for part in rest.split(',') {
        if let Some(val) = part.strip_prefix("server=") {
            server = Some(val.to_string());
        } else if let Some(val) = part.strip_prefix("share=") {
            share = Some(val.to_string());
        }
    }
    Some((server?, share?))
}

/// Discover GVFS-mounted SMB shares as network locations.
///
/// Scans `/run/user/<uid>/gvfs/` for `smb-share:*` directories. Each one
/// becomes a `Network` location. Skips silently if the GVFS directory
/// doesn't exist (non-GNOME systems).
pub(super) fn get_network_mounts() -> Vec<LocationInfo> {
    // SAFETY: `getuid` reads the process's real UID; always safe, no args or pointers.
    let uid = unsafe { libc::getuid() };
    let gvfs_dir = format!("/run/user/{}/gvfs", uid);
    let gvfs_path = Path::new(&gvfs_dir);

    if !gvfs_path.is_dir() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(gvfs_path) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut mounts = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let dirname = name.to_string_lossy();
        if let Some((_server, share)) = parse_gvfs_smb_dirname(&dirname) {
            let path = entry.path().to_string_lossy().to_string();
            // Skip inaccessible entries (hung FUSE mount)
            if !entry.path().is_dir() {
                continue;
            }
            mounts.push(LocationInfo {
                id: volume_id_for_mount(&path),
                name: share,
                path,
                category: LocationCategory::Network,
                icon: None,
                is_ejectable: true,
                fs_type: None,
                supports_trash: false,
                is_read_only: false,
                is_disk_image: false,
                smb_connection_state: None,
                usb_speed: None,
                capabilities: None,
            });
        }
    }

    mounts.sort_by_key(|m| m.name.to_lowercase());
    mounts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gvfs_smb_dirname_basic() {
        let result = parse_gvfs_smb_dirname("smb-share:server=192.168.1.150,share=pihdd");
        assert_eq!(result, Some(("192.168.1.150".to_string(), "pihdd".to_string())));
    }

    #[test]
    fn test_parse_gvfs_smb_dirname_with_extra_params() {
        let result = parse_gvfs_smb_dirname("smb-share:server=mynas.local,share=photos,user=alice,domain=WORKGROUP");
        assert_eq!(result, Some(("mynas.local".to_string(), "photos".to_string())));
    }

    #[test]
    fn test_parse_gvfs_smb_dirname_non_smb() {
        assert_eq!(parse_gvfs_smb_dirname("dav+sd:host=example.com"), None);
        assert_eq!(parse_gvfs_smb_dirname("ftp:host=ftp.example.com"), None);
        assert_eq!(parse_gvfs_smb_dirname("some-random-dir"), None);
    }

    #[test]
    fn test_parse_gvfs_smb_dirname_missing_fields() {
        assert_eq!(parse_gvfs_smb_dirname("smb-share:server=192.168.1.1"), None);
        assert_eq!(parse_gvfs_smb_dirname("smb-share:share=data"), None);
        assert_eq!(parse_gvfs_smb_dirname("smb-share:"), None);
    }
}
