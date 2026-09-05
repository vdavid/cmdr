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
    EnsureDirectSmbFut, MountFacts, ResolveMtpFut, ResolvedMtpObject, SmbUpgradeRefusal, VolumeProvider,
};

/// Whether `path` sits on a network filesystem (SMB, NFS, AFP, WebDAV, ...).
///
/// One `statfs`, so ❌ never call it in a loop over entries or on a hot read
/// path: it can block for as long as the mount takes to answer. `mount_facts`
/// below is the index's caller; `watcher::start_watching` is the other, deciding
/// a watch's [`WatchCoverage`](cmdr_fs::volume::WatchCoverage) once at arm time
/// rather than per query.
///
/// The kind → network mapping is per-platform, which is why this lives app-side
/// rather than as a predicate `cmdr-fs` could offer.
pub fn path_is_on_network_mount(path: &Path) -> bool {
    is_network(&super::filesystem_kind::detect_filesystem_for_path(path))
}

/// The kind → network mapping, split out so one probe can answer both of
/// [`mount_facts`](AppVolumeProvider::mount_facts)' questions.
fn is_network(info: &cmdr_fs::filesystem_kind::FilesystemInfo) -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::volumes::is_network_fs_type(info.raw_type.as_deref())
    }
    #[cfg(target_os = "linux")]
    {
        info.raw_type
            .as_deref()
            .map(super::linux_mounts::is_network_fs_type)
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = info;
        false
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
        // ONE `detect_filesystem_for_path` probe answers both questions: it can
        // block on a wedged mount, so don't grow this into two.
        let info = super::filesystem_kind::detect_filesystem_for_path(path);
        MountFacts {
            is_network: is_network(&info),
            inodes_trustworthy: info.kind.has_stable_inodes(),
        }
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
        use crate::network::smb_upgrade::UpgradeResult;
        let volume_id = volume_id.to_string();
        Box::pin(async move {
            match crate::commands::network::upgrade_to_smb_volume_inner(volume_id).await {
                Ok(UpgradeResult::Success) => Ok(()),
                Ok(UpgradeResult::CredentialsNeeded { .. }) => Err(SmbUpgradeRefusal::CredentialsNeeded),
                Ok(UpgradeResult::NetworkError { reason, display_name }) => Err(SmbUpgradeRefusal::Failed(
                    format!("couldn't reach {display_name} ({reason:?})").into(),
                )),
                Err(e) => Err(SmbUpgradeRefusal::Failed(e.to_string().into())),
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
