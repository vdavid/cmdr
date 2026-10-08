//! The app's answer to the index's questions about mounted volumes.
//!
//! The index subsystems are being extracted into a crate that can't reach
//! `VolumeManager`, the platform mount probes, or the MTP session layer, so they
//! ask through `indexing::host::volumes::VolumeProvider` and this is what the app
//! installs at startup. A pure adapter: every decision below already lived
//! somewhere in `file_system/`, `volumes*/`, or `mtp/`, and stays there.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use cmdr_index::host::volumes::{
    EnsureDirectSmbFut, MountFacts, MountIdentity, ResolveMtpFut, ResolvedMtpObject, SmbUpgradeRefusal, VolumeProvider,
};

/// Whether `path` sits on anything but a known local disk: a network share (SMB,
/// NFS, AFP, WebDAV, ...), a FUSE or cloud mount (`pcloudfs`, rclone, sshfs), or a
/// mount the probe couldn't resolve.
///
/// One `statfs` (a `/proc/mounts` read on Linux), so ❌ never call it in a loop
/// over entries or on a hot read path: it can block for as long as the mount
/// takes to answer. `mount_facts` below is the index's caller (the walker
/// choice); `watcher::start_watching` is the other, deciding a watch's
/// [`WatchCoverage`](cmdr_fs::volume::WatchCoverage) once at arm time rather than
/// per query. Both want the same answer: a remote tree can't be walked at local
/// speed, and its other writers never reach FSEvents.
pub fn path_is_on_network_mount(path: &Path) -> bool {
    is_network(&super::filesystem_kind::probe_mount_for_path(path))
}

/// "Network" here means "not a known local disk" ([`mount_is_local_disk`]), so
/// FUSE and cloud mounts count too. Split out so one probe can answer both of
/// [`mount_facts`](AppVolumeProvider::mount_facts)' questions.
fn is_network(probe: &super::filesystem_kind::MountProbe) -> bool {
    !mount_is_local_disk(probe.source.as_deref(), probe.info.raw_type.as_deref())
}

/// Whether a mount is a KNOWN local disk: its source is a block device (`/dev/…`:
/// disks, partitions, attached disk images), or its type is a local disk type
/// whose source isn't a device node (ZFS datasets, a container's overlay root,
/// RAM-backed mounts).
///
/// An allowlist on purpose. Everything else (FUSE, cloud mounts like `pcloudfs`,
/// network shares, an unprobeable mount) reads as network, so the index picks
/// the network walker for it: a miss here is a slower walk of a local disk, while
/// a miss in a network denylist crawls a remote tree at local-walker speed.
/// ❌ Don't trust `MNT_LOCAL` instead: Xcode's `devicefs` is flagged local.
/// ❌ Don't fold this into `volumes::is_network_fs_type`: that one also picks the
/// volume-ID derivation, so widening it would re-ID existing FUSE volumes.
pub(crate) fn mount_is_local_disk(source: Option<&str>, fs_type: Option<&str>) -> bool {
    if source.is_some_and(|s| s.starts_with("/dev/")) {
        return true;
    }
    let Some(fs_type) = fs_type else { return false };
    matches!(
        fs_type.to_ascii_lowercase().as_str(),
        "apfs"
            | "hfs"
            | "exfat"
            | "msdos"
            | "vfat"
            | "ntfs"
            | "ntfs3"
            | "ufsd_ntfs"
            | "zfs"
            | "btrfs"
            | "bcachefs"
            | "ext2"
            | "ext3"
            | "ext4"
            | "xfs"
            | "f2fs"
            | "overlay"
            | "tmpfs"
            | "ramfs"
    )
}

/// Answers the index from the app's real volume registry and platform probes.
pub struct AppVolumeProvider;

impl VolumeProvider for AppVolumeProvider {
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>> {
        super::volume::manager::get_volume_manager().get(volume_id)
    }

    fn volume_ids(&self) -> Vec<String> {
        super::volume::manager::get_volume_manager()
            .list_volumes()
            .into_iter()
            .map(|(id, _name)| id)
            .collect()
    }

    fn mount_id_for_path(&self, path: &str) -> Option<String> {
        super::volume::manager::get_volume_manager().mount_id_for_path(path)
    }

    fn mount_facts(&self, path: &Path) -> MountFacts {
        // ONE `probe_mount_for_path` probe answers both questions: it can block on
        // a wedged mount, so don't grow this into two.
        let probe = super::filesystem_kind::probe_mount_for_path(path);
        MountFacts {
            is_network: is_network(&probe),
            inodes_trustworthy: probe.info.kind.has_stable_inodes(),
        }
    }

    // Both read the non-blocking mount table discovery and eject already read, so a
    // hung or dead drive can't stall the answer.
    fn mount_identity(&self, root: &Path) -> Option<MountIdentity> {
        let root = root.to_string_lossy();
        #[cfg(target_os = "macos")]
        {
            crate::volumes::mount_identity_at(&root).map(MountIdentity::from_raw)
        }
        #[cfg(target_os = "linux")]
        {
            super::linux_mounts::mount_device_at(&root).map(MountIdentity::from_raw)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = root;
            None
        }
    }

    fn is_mounted(&self, identity: MountIdentity) -> Option<bool> {
        #[cfg(target_os = "macos")]
        {
            crate::volumes::has_mount_identity(identity.raw())
        }
        #[cfg(target_os = "linux")]
        {
            super::linux_mounts::device_is_mounted(identity.raw())
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = identity;
            None
        }
    }

    fn mount_points(&self) -> Option<Vec<std::path::PathBuf>> {
        // The whole table, ❌ never `registrable_mount_roots`: the boot scan has to
        // stop at another account's mount too, which the registry leaves out.
        #[cfg(target_os = "macos")]
        let roots = crate::volumes::mount_roots();
        #[cfg(target_os = "linux")]
        let roots = crate::volumes_linux::mount_roots();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let roots: Option<Vec<String>> = Some(Vec::new());
        roots.map(|roots| roots.into_iter().map(std::path::PathBuf::from).collect())
    }

    fn smb_volume_id_for_path(&self, path: &str) -> Option<String> {
        smb_volume_id_for_path(path)
    }

    fn volume_used_bytes(&self, path: &Path) -> Option<u64> {
        super::volume::backends::get_space_info_for_path(path)
            .map(|info| info.used_bytes())
            .map_err(|e| log::warn!("Failed to read volume used bytes (tier-2 will degrade): {e}"))
            .ok()
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn ensure_direct_smb(&self, volume_id: &str) -> EnsureDirectSmbFut<'_> {
        use crate::network::smb_connect_directly::{UpgradeResult, connect_directly};
        let volume_id = volume_id.to_string();
        Box::pin(async move {
            match connect_directly(&volume_id).await {
                UpgradeResult::Success => Ok(()),
                UpgradeResult::CredentialsNeeded { .. } => Err(SmbUpgradeRefusal::CredentialsNeeded),
                UpgradeResult::NetworkError { reason, display_name } => Err(SmbUpgradeRefusal::Failed(
                    format!("couldn't reach {display_name} ({reason:?})").into(),
                )),
                UpgradeResult::VolumeGone => Err(SmbUpgradeRefusal::Failed(
                    "the volume isn't mounted anymore".to_string().into(),
                )),
                UpgradeResult::NotSmbMount => Err(SmbUpgradeRefusal::Failed(
                    "the volume isn't an SMB mount".to_string().into(),
                )),
                UpgradeResult::MountNotResponding => Err(SmbUpgradeRefusal::Failed(
                    "the volume's mount isn't responding".to_string().into(),
                )),
            }
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn ensure_direct_smb(&self, _volume_id: &str) -> EnsureDirectSmbFut<'_> {
        Box::pin(async move {
            Err(SmbUpgradeRefusal::Failed(
                "SMB is unsupported on this platform".to_string().into(),
            ))
        })
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn resolve_mtp_object(&self, device_id: &str, storage_id: u32, handle: u32) -> ResolveMtpFut<'_> {
        let device_id = device_id.to_string();
        Box::pin(async move {
            crate::mtp::connection_manager()
                .resolve_object_for_index(&device_id, storage_id, handle)
                .await
                .map(|obj| ResolvedMtpObject {
                    path: obj.path,
                    is_directory: obj.is_directory,
                    size: obj.size,
                    modified_at: obj.modified_at,
                })
                .map_err(|e| format!("{e:?}").into())
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn resolve_mtp_object(&self, _device_id: &str, _storage_id: u32, handle: u32) -> ResolveMtpFut<'_> {
        Box::pin(async move { Err(format!("MTP is unsupported on this platform (handle {handle})").into()) })
    }
}

/// Map an SMB mount path to its index volume id, if the path is on an SMB mount.
///
/// Returns `Some(smb_volume_id(server, port, share))` when `path` resolves to an
/// `smbfs`/`cifs` mount, else `None`. Keyed by `(server, port, share)` (via
/// `smb_volume_id`), the SAME id the `VolumeManager` registers the share under, so
/// a listing under `/Volumes/<share>` resolves to the SMB volume's index, not
/// `root`. Platform-split because the mount-info probe lives in the macOS-only
/// `volumes` / Linux-only `volumes_linux` module.
pub(crate) fn smb_volume_id_for_path(path: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    use crate::volumes::get_smb_mount_info;
    #[cfg(target_os = "linux")]
    use crate::volumes_linux::get_smb_mount_info;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let info = get_smb_mount_info(path)?;
        Some(cmdr_fs::volume::smb_volume_id(&info.server, info.port, &info.share))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod real_image;

#[cfg(test)]
mod tests {
    use super::*;

    /// A local path is not an SMB mount. The negative case is what keeps a
    /// `/Users/...` listing routing to the local index instead of a share's.
    #[test]
    fn a_local_path_has_no_smb_volume_id() {
        assert!(smb_volume_id_for_path("/Users/someone/Documents").is_none());
        assert!(smb_volume_id_for_path("/").is_none());
    }

    /// The local disk must never classify as a network mount, or the local scanner
    /// would refuse to walk it. Its inodes are also trustworthy, which is what lets
    /// the rename pre-pass match files across a rename.
    #[test]
    fn the_local_disk_is_a_trustworthy_non_network_mount() {
        let facts = AppVolumeProvider.mount_facts(Path::new("/"));
        assert!(!facts.is_network);
        assert!(facts.inodes_trustworthy);
    }

    /// Block-device-backed mounts are local whatever their type: disks, partitions,
    /// and attached disk images (verified on macOS 26 with `hdiutil attach`,
    /// 2026-09-30: exFAT, FAT, and HFS+ images report `/dev/diskNs1` sources).
    #[test]
    fn a_block_device_backed_mount_is_a_local_disk() {
        for (source, fs_type) in [
            ("/dev/disk3s1s1", "apfs"),
            ("/dev/disk4s1", "exfat"),
            ("/dev/disk5s1", "msdos"),
            ("/dev/disk6s1", "hfs"),
            ("/dev/sda1", "ext4"),
            ("/dev/sdb1", "fuseblk"),
            ("/dev/mapper/vg-root", "xfs"),
        ] {
            assert!(
                mount_is_local_disk(Some(source), Some(fs_type)),
                "{fs_type} from {source} is a local disk"
            );
        }
    }

    /// A known local disk type is local even when its source isn't a device node:
    /// ZFS datasets, a container's overlay root, and RAM-backed mounts.
    #[test]
    fn a_known_local_type_is_a_local_disk_without_a_device_source() {
        for (source, fs_type) in [
            ("tank/home", "zfs"),
            ("overlay", "overlay"),
            ("tmpfs", "tmpfs"),
            ("rpool/ROOT/ubuntu", "ZFS"),
        ] {
            assert!(
                mount_is_local_disk(Some(source), Some(fs_type)),
                "{fs_type} from {source} is a local disk"
            );
        }
    }

    /// The regression: FUSE and cloud filesystems aren't local disks, so a drive
    /// index scoped into one picks the network walker instead of crawling the
    /// remote tree at local-walker speed. Network shares stay network, and so does
    /// FUSE-T posing as `nfs`.
    #[test]
    fn fuse_cloud_and_network_mounts_are_not_local_disks() {
        for (source, fs_type) in [
            ("pCloud", "pcloudfs"),
            ("rclone:remote", "macfuse"),
            ("sshfs@host:/home/me", "osxfuse"),
            ("sshfs@host:/home/me", "fuse.sshfs"),
            ("//me@nas/share", "smbfs"),
            ("nas:/export", "nfs"),
            ("fuse-t:/remote", "nfs"),
            ("https://dav.example.com", "webdav"),
            ("devicefs", "devicefs"),
        ] {
            assert!(
                !mount_is_local_disk(Some(source), Some(fs_type)),
                "{fs_type} from {source} must not read as a local disk"
            );
        }
    }

    /// A mount that couldn't be probed isn't known to be local, so it gets the
    /// network walker: slower if it was local after all, but it can't crawl a wire.
    #[test]
    fn an_unprobed_mount_is_not_a_local_disk() {
        assert!(!mount_is_local_disk(None, None));
    }

    /// The index's presence seam reads the kernel mount table by filesystem: the boot
    /// volume's root names its filesystem and that filesystem reads mounted, a plain
    /// folder names none (how a drive's lingering mount-point folder reads once the
    /// drive is gone), and an identity nothing has is mounted nowhere.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn presence_answers_from_the_mount_table() {
        let boot = AppVolumeProvider
            .mount_identity(Path::new("/"))
            .expect("`/` is a mount point");
        assert_eq!(AppVolumeProvider.is_mounted(boot), Some(true));
        let folder = cmdr_fs::testing::TestDir::new("index-provider-presence");
        assert_eq!(
            AppVolumeProvider.mount_identity(&folder),
            None,
            "a plain folder isn't a mount point"
        );
        assert_eq!(
            AppVolumeProvider.is_mounted(MountIdentity::from_raw(u64::MAX)),
            Some(false)
        );
    }

    /// The index's boot-tree cut reads the real kernel table through this: it lists
    /// the boot disk's own `/` (which the index then leaves out), so an empty answer
    /// can only mean the read went wrong.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn mount_points_come_from_the_kernel_table() {
        let points = AppVolumeProvider.mount_points().expect("the mount table reads");
        assert!(points.iter().any(|p| p == Path::new("/")), "`/` is mounted: {points:?}");
    }

    /// The negative half, against a REAL filesystem: a FAT32 mount's inodes are
    /// derived rather than stored, so the rename pre-pass must not trust them. This
    /// is the mapping the index-side scan tests take as given.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "attaches a disk image; run with --run-ignored all"]
    fn a_real_fat32_mount_detects_as_inode_untrusted() {
        use cmdr_index::testing::external_drive_fixture::{DiskImageFilesystem, DiskImageFixture};

        let fixture = DiskImageFixture::attach(DiskImageFilesystem::Fat32, "CMDRFACTS").expect("attach FAT32");
        let facts = AppVolumeProvider.mount_facts(fixture.mount_point());
        assert!(
            !facts.inodes_trustworthy,
            "a real FAT32 mount must detect as inode-untrusted"
        );
    }
}
