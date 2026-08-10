//! Which identity a macOS mount gets keyed by, and the syscalls that recover it.
//!
//! The ID scheme itself lives in `cmdr_fs::volume::ids`; this module only decides
//! WHICH constructor a given mount goes through, and gets the volume UUID out of
//! macOS. Every site that derives an ID from a mount funnels through
//! [`volume_id_for`] so `get_attached_volumes` and `resolve_path_volume_fast`
//! can't drift into handing one volume two IDs.

use super::{SmbMountInfo, get_smb_mount_info, is_network_fs_type};
use crate::file_system::volume::{local_volume_id, path_volume_id, smb_volume_id};

/// The one rule that turns a mount into a volume ID. Pure: every input is
/// already-gathered mount data, so it's unit-testable and can't block.
///
/// In order:
///
/// 1. An SMB mount keys on `(server, port, share)`, never on the path shape.
/// 2. Any other network mount keys on its path. We deliberately don't ask a
///    network volume for a UUID: that's an NSURL round-trip that hangs forever on
///    a dead mount, which is the exact failure `DETAILS.md` § "Hung mounts"
///    exists to prevent.
/// 3. A local mount keys on its filesystem UUID, falling back to its path when
///    the volume has none (tmpfs, some FUSE mounts, the odd disk image).
pub(crate) fn volume_id_for(
    mount_path: &str,
    fs_type: Option<&str>,
    smb: Option<&SmbMountInfo>,
    uuid: Option<&str>,
) -> String {
    if let Some(info) = smb {
        return smb_volume_id(&info.server, info.port, &info.share);
    }
    if is_network_fs_type(fs_type) {
        return path_volume_id(mount_path);
    }
    local_volume_id(uuid, mount_path)
}

/// Volume ID for a mount path, gathering the identity itself.
///
/// For callers that hold nothing but a path (the mount watcher, one-off
/// resolution). Costs a `statfs` plus, for a local mount, one NSURL resource
/// read. Callers that already have the mount's `statfs` data should call
/// [`volume_id_for`] directly rather than paying for it twice.
///
/// # Gotcha: this can't recover an UNMOUNTED volume's ID
///
/// Both lookups need the volume to still be mounted, so after an unmount this
/// returns the path-derived fallback rather than the ID the volume was
/// registered under. The unmount path uses `VolumeManager::find_by_root`
/// instead, which looks the registration up by `Volume::root()`.
pub(crate) fn volume_id_for_mount(mount_path: &str) -> String {
    let smb = get_smb_mount_info(mount_path);
    let fs_type = super::get_fs_type(mount_path);
    let uuid = match smb.is_some() || is_network_fs_type(fs_type.as_deref()) {
        true => None,
        false => super::get_volume_uuid_for_path(mount_path),
    };
    volume_id_for(mount_path, fs_type.as_deref(), smb.as_ref(), uuid.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smb_info(server: &str, port: u16, share: &str) -> SmbMountInfo {
        SmbMountInfo {
            server: server.to_string(),
            share: share.to_string(),
            username: None,
            port,
        }
    }

    #[test]
    fn an_smb_mount_keys_on_the_share_not_the_path() {
        // The same share reached at two mount points is one volume; two same-named
        // shares on different servers are two.
        assert_eq!(
            volume_id_for(
                "/Volumes/naspi",
                Some("smbfs"),
                Some(&smb_info("nas", 445, "naspi")),
                None
            ),
            volume_id_for(
                "/Volumes/naspi-1",
                Some("smbfs"),
                Some(&smb_info("nas", 445, "naspi")),
                None
            ),
        );
        assert_ne!(
            volume_id_for(
                "/Volumes/public",
                Some("smbfs"),
                Some(&smb_info("nas", 445, "Public")),
                None
            ),
            volume_id_for(
                "/Volumes/public",
                Some("smbfs"),
                Some(&smb_info("localhost", 10494, "public")),
                None
            ),
        );
    }

    #[test]
    fn a_local_mount_keys_on_its_uuid() {
        // macOS mounts a second same-named disk at `/Volumes/Backup 1`; the volume
        // must keep the index and saved paths it had at `/Volumes/Backup`.
        assert_eq!(
            volume_id_for("/Volumes/Backup", Some("apfs"), None, Some("A1B2-C3D4")),
            volume_id_for("/Volumes/Backup 1", Some("apfs"), None, Some("A1B2-C3D4")),
        );
        assert_ne!(
            volume_id_for("/Volumes/Backup", Some("apfs"), None, Some("A1B2-C3D4")),
            volume_id_for("/Volumes/Backup", Some("apfs"), None, Some("A1B2-C3D5")),
        );
    }

    #[test]
    fn a_local_mount_without_a_uuid_falls_back_to_its_path() {
        assert_eq!(
            volume_id_for("/Volumes/Ramdisk", Some("tmpfs"), None, None),
            path_volume_id("/Volumes/Ramdisk"),
        );
        // And confusable names still separate, which is the whole point.
        assert_ne!(
            volume_id_for("/Volumes/My Disk", Some("tmpfs"), None, None),
            volume_id_for("/Volumes/My_Disk", Some("tmpfs"), None, None),
        );
    }

    #[test]
    fn a_non_smb_network_mount_keys_on_its_path() {
        // NFS and WebDAV get no UUID probe (it would hang on a dead mount), so the
        // path is all we have. Callers must not pass one either.
        assert_eq!(
            volume_id_for("/Volumes/nfs-share", Some("nfs"), None, None),
            path_volume_id("/Volumes/nfs-share"),
        );
    }

    #[test]
    fn the_boot_volume_keeps_its_literal_id() {
        assert_eq!(volume_id_for("/", Some("apfs"), None, Some("A1B2-C3D4")), "root");
        assert_eq!(volume_id_for("/", Some("apfs"), None, None), "root");
    }
}
