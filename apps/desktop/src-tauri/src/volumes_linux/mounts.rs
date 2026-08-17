//! Attached-volume enumeration: turning `/proc/mounts` rows into switcher
//! entries, and the filters that decide which rows are user-facing at all
//! (virtual filesystems, system internals, bind mounts nested under another
//! volume, double mounts of one filesystem).

use super::fs_type::{is_virtual_fs, supports_trash_for_fs_type};
use super::ids::volume_id_for_mount;
use super::{LocationCategory, LocationInfo, MountEntry};
use cmdr_fs::volume::canonical_root::collapse_by_volume_id;
use std::path::Path;

/// Mount paths that are system internals and should never appear as volumes.
/// These are path prefixes; any mount whose mountpoint starts with one of these is filtered out.
const HIDDEN_MOUNT_PREFIXES: &[&str] = &[
    "/snap/",            // Ubuntu snap loopback packages (squashfs)
    "/run/snapd/",       // Snap daemon internals
    "/boot/",            // EFI system partition, boot loaders
    "/run/user/",        // Per-user runtime mounts (XDG portals, GVFS)
    "/run/credentials/", // systemd credential mounts
];

/// Get mounted real filesystems, filtering out virtual ones and root.
pub fn get_mounted_volumes(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    get_mounted_volumes_with(mounts, volume_id_for_mount)
}

/// The body of [`get_mounted_volumes`], with ID derivation injected.
///
/// `volume_id` is a parameter purely for testability: [`volume_id_for_mount`]
/// reads the LIVE `/proc/mounts` and `/dev/disk/by-uuid`, which no `mounts`
/// fixture can stand in for, so a test that needs two mounts to share an ID has
/// to say so directly.
fn get_mounted_volumes_with(mounts: &[MountEntry], volume_id: impl Fn(&str) -> String) -> Vec<LocationInfo> {
    let username = get_username();

    // Collect candidate mount points (real, non-hidden, non-root).
    let candidate_paths: Vec<&str> = mounts
        .iter()
        .filter(|e| !is_virtual_fs(&e.fstype) && e.mountpoint != "/" && !is_hidden_mount(&e.mountpoint))
        .map(|e| e.mountpoint.as_str())
        .collect();

    let mut volumes = Vec::new();

    for entry in mounts {
        if is_virtual_fs(&entry.fstype) {
            continue;
        }
        if entry.mountpoint == "/" {
            continue;
        }
        if is_hidden_mount(&entry.mountpoint) {
            continue;
        }
        // Skip sub-mounts (bind mounts nested under another real mount).
        if is_submount(&entry.mountpoint, &candidate_paths) {
            continue;
        }

        let is_removable = is_removable_mount(&entry.mountpoint, &username);
        let name = mount_display_name(&entry.mountpoint);
        let fs_type = Some(entry.fstype.clone());
        let supports_trash = supports_trash_for_fs_type(fs_type.as_deref());

        volumes.push(LocationInfo {
            id: volume_id(&entry.mountpoint),
            name,
            path: entry.mountpoint.clone(),
            category: LocationCategory::AttachedVolume,
            icon: None,
            is_ejectable: is_removable,
            fs_type,
            supports_trash,
            mount_is_read_only: false,
            is_disk_image: false,
            smb_connection_state: None,
            usb_speed: None,
            capabilities: None,
        });
    }

    // One volume ID publishes ONE mount root. A CIFS share mounted twice, or a
    // bind mount that `is_submount` can't see (it only filters mounts nested
    // under another volume), is several `/proc/mounts` rows for one filesystem,
    // all deriving one ID. Display-only: the registry still keeps every root it
    // learns about, and no pane sitting on a dropped root is moved.
    let mut volumes = collapse_by_volume_id(volumes);

    volumes.sort_by_key(|v| v.name.to_lowercase());
    volumes
}

/// Extract a display name from a mount path.
pub(super) fn mount_display_name(mountpoint: &str) -> String {
    Path::new(mountpoint)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(mountpoint)
        .to_string()
}

/// Check if a mount is nested under another real mount (bind mount or sub-partition).
/// For example, `/mnt/share/project/node_modules` is a sub-mount of `/mnt/share`.
fn is_submount(mountpoint: &str, candidate_paths: &[&str]) -> bool {
    candidate_paths.iter().any(|parent| {
        *parent != mountpoint
            && mountpoint.starts_with(parent)
            && mountpoint.as_bytes().get(parent.len()) == Some(&b'/')
    })
}

/// Check if a mount path should be hidden from the volume list.
fn is_hidden_mount(mountpoint: &str) -> bool {
    HIDDEN_MOUNT_PREFIXES
        .iter()
        .any(|prefix| mountpoint.starts_with(prefix))
}

/// Check if a mount point is under a removable media path.
fn is_removable_mount(mountpoint: &str, username: &str) -> bool {
    if username.is_empty() {
        return false;
    }
    let run_media = format!("/run/media/{}/", username);
    let media_user = format!("/media/{}/", username);
    mountpoint.starts_with(&run_media) || mountpoint.starts_with(&media_user)
}

/// Get the current username for removable media path detection.
fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::linux_mounts;
    use crate::file_system::volume::path_volume_id;
    use crate::volumes_linux::ids::smb_volume_id;
    use crate::volumes_linux::smb::parse_smb_mount_source;
    use crate::volumes_linux::test_support::parse_test_mounts;

    #[test]
    fn test_is_removable_mount() {
        assert!(is_removable_mount("/run/media/user/USB", "user"));
        assert!(is_removable_mount("/media/user/SD", "user"));
        assert!(!is_removable_mount("/mnt/data", "user"));
        assert!(!is_removable_mount("/home", "user"));
        assert!(!is_removable_mount("/run/media/other/USB", "user"));
        assert!(!is_removable_mount("/run/media/user/USB", ""));
    }

    #[test]
    fn test_mount_display_name() {
        assert_eq!(mount_display_name("/mnt/data"), "data");
        assert_eq!(mount_display_name("/run/media/user/USB"), "USB");
        assert_eq!(mount_display_name("/home"), "home");
    }

    #[test]
    fn test_is_hidden_mount() {
        assert!(is_hidden_mount("/snap/firefox/7764"));
        assert!(is_hidden_mount("/snap/core22/2134"));
        assert!(is_hidden_mount("/run/snapd/ns/something.mnt"));
        assert!(is_hidden_mount("/boot/efi"));
        assert!(is_hidden_mount("/run/user/1000/doc"));
        assert!(is_hidden_mount("/run/user/1000/gvfs"));
        assert!(is_hidden_mount("/run/credentials/systemd-journald.service"));
        assert!(!is_hidden_mount("/mnt/data"));
        assert!(!is_hidden_mount("/home"));
        assert!(!is_hidden_mount("/media/user/USB"));
        assert!(!is_hidden_mount("/run/media/user/USB"));
    }

    #[test]
    fn test_snap_mounts_filtered_from_volumes() {
        let mounts_with_snaps = "\
/dev/sda1 / ext4 rw,relatime 0 0
/dev/loop0 /snap/bare/5 squashfs ro,nodev,relatime 0 0
/dev/loop2 /snap/firefox/7764 squashfs ro,nodev,relatime 0 0
/dev/loop8 /snap/snap-store/1271 squashfs ro,nodev,relatime 0 0
/dev/sdb1 /mnt/data xfs rw,relatime 0 0
tmpfs /run/user/1000 tmpfs rw,nosuid,nodev,relatime 0 0
portal /run/user/1000/doc fuse.portal rw 0 0
gvfsd-fuse /run/user/1000/gvfs fuse.gvfsd-fuse rw 0 0
/dev/vda1 /boot/efi vfat rw,relatime 0 0
";
        let mounts = linux_mounts::parse_proc_mounts_from_content(mounts_with_snaps);
        let volumes = get_mounted_volumes(&mounts);
        let paths: Vec<&str> = volumes.iter().map(|v| v.path.as_str()).collect();
        assert!(paths.contains(&"/mnt/data"), "Should include real mount");
        assert!(
            !paths.iter().any(|p| p.starts_with("/snap/")),
            "Should filter snap mounts"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("/boot/")),
            "Should filter boot mounts"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("/run/user/")),
            "Should filter user runtime mounts"
        );
    }

    #[test]
    fn test_is_submount() {
        let candidates = vec!["/mnt/cmdr", "/mnt/cmdr/cmdr/node_modules", "/media/user/USB"];
        assert!(is_submount("/mnt/cmdr/cmdr/node_modules", &candidates));
        assert!(!is_submount("/mnt/cmdr", &candidates));
        assert!(!is_submount("/media/user/USB", &candidates));
        // Not a submount just because of a shared prefix without a path separator
        assert!(!is_submount("/mnt/cmdr2", &["/mnt/cmdr"]));
    }

    #[test]
    fn test_bind_mounts_filtered_from_volumes() {
        let mounts_with_binds = "\
/dev/vda2 / ext4 rw,relatime 0 0
share /mnt/cmdr virtiofs rw,relatime 0 0
/dev/vda2 /mnt/cmdr/cmdr/node_modules ext4 rw,relatime 0 0
/dev/vda2 /mnt/cmdr/cmdr/apps/desktop/node_modules ext4 rw,relatime 0 0
/dev/sda1 /media/user/Ubuntu\\04025.10\\040arm64 iso9660 ro,relatime 0 0
";
        let mounts = linux_mounts::parse_proc_mounts_from_content(mounts_with_binds);
        let volumes = get_mounted_volumes(&mounts);
        let paths: Vec<&str> = volumes.iter().map(|v| v.path.as_str()).collect();

        assert!(paths.contains(&"/mnt/cmdr"), "Should include the parent mount");
        assert!(
            !paths.iter().any(|p| p.contains("node_modules")),
            "Should filter bind mounts nested under another volume"
        );
        assert!(
            paths.iter().any(|p| p.contains("Ubuntu")),
            "Should keep independent mounts"
        );
    }

    #[test]
    fn one_volume_id_publishes_one_mount_root() {
        // Two mounts of one CIFS share (and, below, a bind mount that isn't
        // nested under its twin, so `is_submount` can't see it) are separate
        // `/proc/mounts` rows deriving ONE volume ID. Publishing both would list
        // the volume twice while the registry roots that ID at exactly one path,
        // so the two rows couldn't both be honest about where they navigate.
        let content = "\
/dev/vda1 / ext4 rw,relatime 0 0
//192.168.1.111/naspi /mnt/naspi-second cifs rw,relatime 0 0
//192.168.1.111/naspi /mnt/naspi cifs rw,relatime 0 0
/dev/sdb1 /srv/exported-data xfs rw,relatime 0 0
/dev/sdb1 /mnt/data xfs rw,relatime 0 0
";
        let mounts = linux_mounts::parse_proc_mounts_from_content(content);
        // Stands in for `volume_id_for_mount`, reading the fixture instead of the
        // live mount table: same ladder (CIFS keys on `(server, port, share)`,
        // everything else on the device behind it).
        let volume_id = |path: &str| {
            let entry = mounts.iter().find(|e| e.mountpoint == path);
            match entry.and_then(|e| parse_smb_mount_source(&e.device)) {
                Some(info) => smb_volume_id(&info.server, info.port, &info.share),
                None => path_volume_id(entry.map_or(path, |e| e.device.as_str())),
            }
        };

        let volumes = get_mounted_volumes_with(&mounts, volume_id);
        let paths: Vec<&str> = volumes.iter().map(|v| v.path.as_str()).collect();
        assert_eq!(
            paths,
            ["/mnt/data", "/mnt/naspi"],
            "each filesystem publishes once, at its shortest mount root"
        );
    }

    #[test]
    fn test_get_mounted_volumes_filters_virtual() {
        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        for vol in &volumes {
            assert_ne!(vol.path, "/proc");
            assert_ne!(vol.path, "/sys");
            assert_ne!(vol.path, "/tmp");
        }
    }

    #[test]
    fn test_get_mounted_volumes_excludes_root() {
        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        assert!(
            !volumes.iter().any(|v| v.path == "/"),
            "Root should not be in mounted volumes"
        );
    }

    #[test]
    fn test_get_mounted_volumes_includes_real_fs() {
        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        assert!(volumes.iter().any(|v| v.path == "/home"), "Should include /home");
        assert!(
            volumes.iter().any(|v| v.path == "/mnt/data"),
            "Should include /mnt/data"
        );
    }

    #[test]
    fn test_removable_volume_is_ejectable() {
        // Set USER env var for this test
        let prev = std::env::var("USER").ok();
        // SAFETY: This test is not run concurrently with other tests that read USER.
        unsafe { std::env::set_var("USER", "testuser") };

        let mounts = parse_test_mounts();
        let volumes = get_mounted_volumes(&mounts);
        let usb = volumes.iter().find(|v| v.path.contains("USB"));
        assert!(usb.is_some(), "Should find USB volume");
        assert!(usb.unwrap().is_ejectable, "USB volume should be ejectable");
        assert_eq!(usb.unwrap().fs_type.as_deref(), Some("btrfs"));

        // Restore
        if let Some(prev) = prev {
            // SAFETY: Same as above; restoring original value.
            unsafe { std::env::set_var("USER", prev) };
        }
    }
}
