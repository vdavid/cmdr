//! What a stalled listing is waiting on: a server, a drive, or something we can't
//! name honestly. Picks the stalled-folder screen's wording, so it only says
//! "server" or "drive" when the mount proves it. `DETAILS.md` § "Stalled listings".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::file_system::volume::{BackendKind, Volume};

/// What a stalled listing's folder lives on, as far as the mount proves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StalledOn {
    /// A network share or a direct server connection (SMB, NFS, AFP, WebDAV, SFTP, S3, ...).
    Server,
    /// A known local disk: a block device, or a local filesystem type.
    Drive,
    /// Anything else: a phone, a FUSE or cloud mount, or a mount we couldn't read.
    Unknown,
}

/// How long the stall report waits for the symlink-resolving probe before it
/// settles for [`StalledOn::Unknown`]. A healthy local disk resolves in
/// microseconds; anything slower is the hung mount itself.
const RESOLVE_TIMEOUT: Duration = Duration::from_millis(500);

/// Classifies the folder a stalled listing is reading.
///
/// A direct backend names itself. A filesystem path is read off the kernel's mount
/// table ❌ never with a `statfs` on the path, which blocks on the very mount that
/// just stalled. The table lookup is lexical, so a path that reads as a local disk
/// is resolved through its symlinks first (`~/nas` → `/Volumes/nas`), off-thread
/// and bounded by [`RESOLVE_TIMEOUT`].
pub(crate) async fn stalled_on(volume: &Arc<dyn Volume>, path: &Path) -> StalledOn {
    if let Some(answer) = stalled_on_backend(volume.backend_kind()) {
        return answer;
    }
    let lexical = stalled_on_mount_of(path);
    if lexical != StalledOn::Drive {
        return lexical;
    }
    let path = path.to_path_buf();
    let resolved = tokio::task::spawn_blocking(move || stalled_on_mount_of(&resolve_symlinks(&path)));
    match tokio::time::timeout(RESOLVE_TIMEOUT, resolved).await {
        Ok(Ok(answer)) => answer,
        // Timed out (the probe sits on a hung mount) or the task panicked.
        _ => StalledOn::Unknown,
    }
}

/// What a backend says on its own, or `None` when the answer is in the mount under the path.
///
/// Exhaustive on purpose: a new backend doesn't compile until it answers.
fn stalled_on_backend(backend: BackendKind) -> Option<StalledOn> {
    match backend {
        BackendKind::Smb | BackendKind::Sftp | BackendKind::Webdav | BackendKind::S3 => Some(StalledOn::Server),
        // MTP is a USB cable and ADB is a cable or Wi-Fi: neither word fits both.
        BackendKind::Mtp | BackendKind::Adb => Some(StalledOn::Unknown),
        // A real filesystem, or a view inside a file on one: the mount knows.
        BackendKind::Local | BackendKind::Archive | BackendKind::GitPortal => None,
    }
}

/// Classifies a mount from its filesystem type and source, as the kernel's table lists them.
///
/// Both sides are allowlists, so a type nobody named reads as [`StalledOn::Unknown`]
/// and keeps the combined wording. ❌ Don't reuse `linux_mounts::is_network_fs_type`
/// here: it calls every unknown `fuse.*` network to pick the safer copy strategy,
/// which is the wrong bias for words a user reads.
fn classify_mount(fs_type: &str, source: &str) -> StalledOn {
    let network = matches!(
        fs_type.to_ascii_lowercase().as_str(),
        // macOS `f_fstypename`
        "smbfs" | "nfs" | "afpfs" | "webdav" | "ftp"
        // Linux `/proc/mounts`; GVFS's `fuse.gvfsd-fuse` holds phones too, so it isn't here.
        | "cifs" | "smb3" | "nfs4" | "ncpfs" | "afs" | "fuse.sshfs" | "fuse.rclone" | "fuse.s3fs"
    );
    if network {
        StalledOn::Server
    } else if crate::file_system::index_provider::mount_is_local_disk(Some(source), Some(fs_type)) {
        StalledOn::Drive
    } else {
        StalledOn::Unknown
    }
}

/// `path` with every symlink resolved, through the deepest ancestor that resolves:
/// an archive's inner path (`a.zip/inner`) or a `.git` portal's virtual tree has no
/// directory of its own, so the rest is joined back on as written.
fn resolve_symlinks(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut current = path;
    loop {
        if let Ok(resolved) = std::fs::canonicalize(current) {
            return rest.iter().rev().fold(resolved, |acc, part| acc.join(part));
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Classifies the mount `path` lies on, read lexically off the kernel's mount table.
#[cfg(target_os = "macos")]
fn stalled_on_mount_of(path: &Path) -> StalledOn {
    match crate::volumes::mount_type_and_source_for(path) {
        Some((fs_type, source)) => classify_mount(&fs_type, &source),
        None => StalledOn::Unknown,
    }
}

/// Classifies the mount `path` lies on, read lexically off `/proc/mounts`.
#[cfg(target_os = "linux")]
fn stalled_on_mount_of(path: &Path) -> StalledOn {
    match crate::file_system::linux_mounts::mount_entry_for_path(path) {
        Some(entry) => classify_mount(&entry.fstype, &entry.device),
        None => StalledOn::Unknown,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn stalled_on_mount_of(_path: &Path) -> StalledOn {
    StalledOn::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_server_backends_are_servers() {
        for backend in [
            BackendKind::Smb,
            BackendKind::Sftp,
            BackendKind::Webdav,
            BackendKind::S3,
        ] {
            assert_eq!(stalled_on_backend(backend), Some(StalledOn::Server), "{backend:?}");
        }
    }

    #[test]
    fn phones_are_neither_server_nor_drive() {
        // MTP is a USB cable, ADB can be Wi-Fi: "server" or "drive" would each be wrong for one of them.
        for backend in [BackendKind::Mtp, BackendKind::Adb] {
            assert_eq!(stalled_on_backend(backend), Some(StalledOn::Unknown), "{backend:?}");
        }
    }

    #[test]
    fn filesystem_backends_defer_to_the_mount() {
        for backend in [BackendKind::Local, BackendKind::Archive, BackendKind::GitPortal] {
            assert_eq!(stalled_on_backend(backend), None, "{backend:?}");
        }
    }

    #[test]
    fn network_filesystems_are_servers() {
        let shares = [
            // macOS `f_fstypename`
            ("smbfs", "//david@192.0.2.9/naspi"),
            ("nfs", "server:/export"),
            ("afpfs", "afp://server/share"),
            ("webdav", "https://dav.example.com/files"),
            ("ftp", "ftp://server/"),
            // Linux `/proc/mounts`
            ("cifs", "//server/share"),
            ("smb3", "//server/share"),
            ("nfs4", "server:/export"),
            ("fuse.sshfs", "user@server:/home/user"),
        ];
        for (fs_type, source) in shares {
            assert_eq!(classify_mount(fs_type, source), StalledOn::Server, "{fs_type}");
        }
    }

    #[test]
    fn local_disks_are_drives() {
        let disks = [
            ("apfs", "/dev/disk3s1s1"),
            ("exfat", "/dev/disk5s1"),
            ("msdos", "/dev/disk6s1"),
            ("ext4", "/dev/sda1"),
            // A local type whose source isn't a device node.
            ("zfs", "tank/home"),
            ("btrfs", "/dev/nvme0n1p2"),
        ];
        for (fs_type, source) in disks {
            assert_eq!(classify_mount(fs_type, source), StalledOn::Drive, "{fs_type}");
        }
    }

    #[test]
    fn mounts_that_could_be_either_stay_unknown() {
        let unsure = [
            // GVFS's one FUSE mount holds SMB shares AND phones side by side.
            ("fuse.gvfsd-fuse", "gvfsd-fuse"),
            // macFUSE fronts sshfs and local images alike.
            ("macfuse", "sshfs@server"),
            // A cloud client's own mount, and an automount trigger before it fires.
            ("pcloudfs", "pCloud"),
            ("autofs", "map auto_home"),
            // A VM's host share.
            ("9p", "hostshare"),
            // An unreadable row.
            ("", ""),
        ];
        for (fs_type, source) in unsure {
            assert_eq!(classify_mount(fs_type, source), StalledOn::Unknown, "{fs_type:?}");
        }
    }

    #[test]
    fn resolves_a_symlink_into_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(resolve_symlinks(&link), std::fs::canonicalize(&target).unwrap());
    }

    #[test]
    fn keeps_a_virtual_tail_beyond_the_deepest_real_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let archive_inner = dir.path().join("a.zip").join("inner");
        assert_eq!(
            resolve_symlinks(&archive_inner),
            std::fs::canonicalize(dir.path()).unwrap().join("a.zip").join("inner")
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_temp_folder_reads_as_a_drive() {
        // Holds where the temp dir is a local disk: a dev Mac's APFS, a Linux box's ext4 or tmpfs.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(stalled_on_mount_of(&resolve_symlinks(dir.path())), StalledOn::Drive);
    }
}
