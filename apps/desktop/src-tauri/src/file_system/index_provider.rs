//! The app's answer to the index's questions about mounted volumes.
//!
//! The index subsystems are being extracted into a crate that can't reach
//! `VolumeManager`, the platform mount probes, or the MTP session layer, so they
//! ask through `indexing::host::volumes::VolumeProvider` and this is what the app
//! installs at startup. A pure adapter: every decision below already lived
//! somewhere in `file_system/`, `volumes*/`, or `mtp/`, and stays there.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::log_error::LogError;
use cmdr_fs::volume::Volume;

use cmdr_index::host::volumes::{
    EnsureDirectSmbFut, MountFacts, MountIdentity, MountPoint, ResolveMtpFut, ResolvedMtpObject, SmbUpgradeRefusal,
    VolumeProvider,
};
use cmdr_local::filesystem_kind::{is_network, probe_mount_for_path};

/// The [`MountClass`](cmdr_fs::volume::MountClass) of the mount at `root`, for a
/// `LocalPosixVolume` being registered on it.
///
/// ❗ Built from the SAME two facts `Index::start_volume`'s routing reads, so the
/// published `can_be_indexed` and the router can't disagree: [`is_network`]
/// (via `mount_facts`) is the local-external walker's test, and
/// [`smb_volume_id_for_path`] is the SMB gate's. Blocking like both (a `statfs`,
/// and for a network mount the SMB lookup), so call it where registration already
/// probes the mount, ❌ never per query.
pub(crate) fn mount_class_at(root: &Path) -> cmdr_fs::volume::MountClass {
    classify_mount(!is_network(&probe_mount_for_path(root)), || {
        smb_volume_id_for_path(&root.to_string_lossy()).is_some()
    })
}

/// The pure half of [`mount_class_at`]. The SMB question is asked only for a
/// mount that isn't a local disk, since it costs a second lookup.
fn classify_mount(is_local_disk: bool, is_smb_share: impl FnOnce() -> bool) -> cmdr_fs::volume::MountClass {
    use cmdr_fs::volume::MountClass;
    if is_local_disk {
        MountClass::LocalDisk
    } else if is_smb_share() {
        MountClass::SmbShare
    } else {
        MountClass::OtherNetwork
    }
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
        let probe = probe_mount_for_path(path);
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
            cmdr_local::linux_mounts::mount_device_at(&root).map(MountIdentity::from_raw)
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
            cmdr_local::linux_mounts::device_is_mounted(identity.raw())
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = identity;
            None
        }
    }

    fn mount_points(&self) -> Option<Vec<MountPoint>> {
        // The whole table, ❌ never `registrable_mount_roots`: the boot scan has to
        // stop at another account's mount too, which the registry leaves out.
        #[cfg(target_os = "macos")]
        let rows = crate::volumes::mount_rows();
        #[cfg(target_os = "linux")]
        let rows = crate::volumes_linux::mount_rows();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let rows: Option<Vec<(String, String, String)>> = Some(Vec::new());
        rows.map(|rows| {
            rows.into_iter()
                .map(|(path, source, fs_type)| MountPoint {
                    path: path.into(),
                    source: Some(source),
                    fs_type: Some(fs_type),
                })
                .collect()
        })
    }

    fn smb_volume_id_for_path(&self, path: &str) -> Option<String> {
        smb_volume_id_for_path(path)
    }

    fn volume_used_bytes(&self, path: &Path) -> Option<u64> {
        cmdr_local::space::get_space_info_for_path(path)
            .map(|info| info.used_bytes())
            .map_err(|e| {
                log::warn!(
                    "Failed to read volume used bytes (tier-2 will degrade): {}",
                    LogError(&e)
                )
            })
            .ok()
    }

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

    /// The class a registered mount publishes is the router's own answer: the
    /// local walker for a local disk, the SMB gate for a share, nothing for the
    /// rest. A local disk never pays for the SMB lookup.
    #[test]
    fn a_mount_is_classed_the_way_the_index_would_route_it() {
        use cmdr_fs::volume::MountClass;
        assert_eq!(
            classify_mount(true, || panic!("a local disk never asks about SMB")),
            MountClass::LocalDisk
        );
        assert_eq!(classify_mount(false, || true), MountClass::SmbShare);
        assert_eq!(classify_mount(false, || false), MountClass::OtherNetwork);
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
        let root = points
            .iter()
            .find(|p| p.path == Path::new("/"))
            .unwrap_or_else(|| panic!("`/` is mounted: {points:?}"));
        assert!(
            root.fs_type.as_deref().is_some_and(|t| !t.is_empty()),
            "each row carries its type, which the Linux cut decides by: {root:?}"
        );
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
