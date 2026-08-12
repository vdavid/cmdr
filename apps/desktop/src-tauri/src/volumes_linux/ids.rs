//! Which identity a Linux mount gets keyed by, and the reads that recover it.
//!
//! The ID scheme itself lives in `cmdr_fs::volume::ids`; this module only decides
//! WHICH constructor a given mount goes through, and gets the filesystem UUID out
//! of `/dev/disk/by-uuid`. Every site that derives an ID from a mount funnels
//! through [`volume_id_for_mount`] so discovery and `resolve_path_volume_fast`
//! can't drift into handing one volume two IDs.

use super::linux_mounts;
use super::smb::{get_smb_mount_info, parse_gvfs_smb_dirname};
use std::path::Path;

pub(crate) use crate::file_system::volume::{local_volume_id, smb_volume_id};

/// Volume ID for a mount path: the Linux twin of `volumes::ids::volume_id_for`.
///
/// Same ladder, same reasons (that module carries the rationale):
///
/// 1. A CIFS mount or GVFS SMB share keys on `(server, port, share)`, never on
///    the path shape, so two same-named shares on different servers don't
///    collide.
/// 2. Any other mount keys on its filesystem UUID via [`volume_uuid_for_mount`],
///    which survives a remount at a different mount point.
/// 3. Failing that, on its mount path.
///
/// # Gotcha: this can't recover an UNMOUNTED volume's ID
///
/// Every branch reads the mount table, so after an unmount this returns the
/// path-derived fallback rather than the ID the volume was registered under. The
/// unmount path uses `VolumeManager::find_by_root` instead.
pub(crate) fn volume_id_for_mount(mount_path: &str) -> String {
    // CIFS mount: /proc/mounts records the source as `//server[:port]/share`.
    if let Some(info) = get_smb_mount_info(mount_path) {
        return smb_volume_id(&info.server, info.port, &info.share);
    }
    // GVFS SMB share: /run/user/<uid>/gvfs/smb-share:server=...,share=...
    // GVFS doesn't expose the port, so default to 445. Mixing custom-port GVFS
    // mounts on the same host+share isn't something GVFS supports today.
    if let Some(dirname) = Path::new(mount_path).file_name().and_then(|n| n.to_str())
        && let Some((server, share)) = parse_gvfs_smb_dirname(dirname)
    {
        return smb_volume_id(&server, 445, &share);
    }
    local_volume_id(volume_uuid_for_mount(mount_path).as_deref(), mount_path)
}

/// The filesystem UUID backing `mount_path`, by matching its `/proc/mounts`
/// device against the `/dev/disk/by-uuid` symlinks.
///
/// `None` when the mount has no block device behind it (tmpfs, FUSE, a network
/// export), when udev isn't populating `by-uuid` (a slim container, which is
/// exactly what the Linux E2E suite runs in), or when the filesystem carries no
/// UUID. Every caller falls back to the mount path, so `None` is ordinary rather
/// than exceptional.
///
/// Only touches `/proc` and `/dev`, never the mounted filesystem itself, so a
/// hung network mount can't block it.
fn volume_uuid_for_mount(mount_path: &str) -> Option<String> {
    let mounts = linux_mounts::parse_proc_mounts();
    let device = mounts.iter().find(|entry| entry.mountpoint == mount_path)?;
    // `/proc/mounts` may name the device through a symlink (`/dev/disk/by-label/…`)
    // while `by-uuid` links to the real node, so compare canonicalized paths.
    let device_node = std::fs::canonicalize(&device.device).ok()?;
    for entry in std::fs::read_dir("/dev/disk/by-uuid").ok()?.flatten() {
        if std::fs::canonicalize(entry.path()).is_ok_and(|target| target == device_node) {
            return entry.file_name().to_str().map(str::to_string);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::path_volume_id;

    #[test]
    fn path_ids_separate_confusable_mount_points() {
        assert_eq!(path_volume_id("/"), "root");
        assert_ne!(path_volume_id("/mnt/data"), path_volume_id("/mnt/Data"));
        assert_ne!(
            path_volume_id("/run/media/user/My-Drive"),
            path_volume_id("/run/media/user/My Drive")
        );
    }

    #[test]
    fn a_mount_with_no_block_device_has_no_uuid() {
        // `/proc` is mounted from `proc`, not a device node, so nothing in
        // `/dev/disk/by-uuid` can match it. Runs anywhere: no fixture, no udev.
        assert_eq!(volume_uuid_for_mount("/proc"), None);
        assert_eq!(volume_uuid_for_mount("/definitely/not/mounted"), None);
    }
}
