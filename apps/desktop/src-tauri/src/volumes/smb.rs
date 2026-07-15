//! SMB mount parsing and enrichment: pulling `(server, share, user, port)` out
//! of a `statfs` mount source, tagging volumes with their SMB connection state,
//! and deriving the SMB-aware volume ID.

use super::*;

/// Returns true if the filesystem type is SMB (macOS `smbfs` or Linux `cifs`).
/// Enriches volume entries with SMB connection state from the `VolumeManager`.
///
/// For each volume, looks up the registered `Volume` in `VolumeManager` and reads
/// its `smb_connection_state()` if any. SMB shares without a direct smb2 session
/// (typical OS-mounted shares before auto-upgrade) are tagged as `OsMount` so
/// the FE picker can show the yellow indicator.
///
/// Used by the `list_volumes` IPC call, the `volumes-changed` push, and the MCP
/// `cmdr://state` resource — all three need the same enrichment, so it lives in
/// one place. Add new enrichment fields here, not at each call site.
pub fn enrich_smb_connection_state(volumes: &mut [LocationInfo]) {
    let manager = crate::file_system::get_volume_manager();
    for vol in volumes.iter_mut() {
        if let Some(registered) = manager.get(&vol.id) {
            vol.smb_connection_state = registered.smb_connection_state();
        }

        // SMB shares without a direct smb2 connection show as OsMount (yellow).
        // This covers pre-existing mounts registered as LocalPosixVolume at startup.
        if vol.smb_connection_state.is_none() && is_smb_fs_type(vol.fs_type.as_deref()) {
            vol.smb_connection_state = Some(SmbConnectionState::OsMount);
        }
    }
}

/// Information about an SMB mount extracted from `statfs`.
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

/// Extracts SMB server, share, and username from a mount path via `statfs`.
///
/// On macOS, `statfs.f_mntfromname` for SMB mounts looks like:
/// - `//user@192.168.1.111/share` (authenticated)
/// - `//192.168.1.111/share` (guest)
///
/// Returns `None` if the path is not an SMB mount or parsing fails.
pub fn get_smb_mount_info(mount_path: &str) -> Option<SmbMountInfo> {
    use std::ffi::CString;

    let c_path = CString::new(mount_path).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
    // SAFETY: `c_path` is a valid NUL-terminated C string from `mount_path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };

    // Check filesystem type is SMB
    let fs_type: String = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();
    if !is_smb_fs_type(Some(&fs_type)) {
        return None;
    }

    // Extract mount source (for example, "//david@192.168.1.111/naspi")
    let mount_from: String = stat
        .f_mntfromname
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8 as char)
        .collect();

    parse_smb_mount_source(&mount_from)
}

/// Parses an SMB mount source string like `//user@host/share` or `//host/share`.
pub(crate) fn parse_smb_mount_source(source: &str) -> Option<SmbMountInfo> {
    // Strip leading "//"
    let rest = source.strip_prefix("//")?;

    // Split into "user@host/share" or "host/share"
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

/// Volume ID for a mount path, SMB-aware.
///
/// For SMB mounts (smbfs), the ID is keyed by `(server, port, share)` via
/// [`smb_volume_id`], not by the path-shape. Two SMB shares with the same
/// case-folded name on different servers (a NAS sharing `Public`, a Docker
/// container sharing `public`) thus get distinct IDs, instead of colliding on
/// `volumespublic`. See [`smb_volume_id`] for the full rationale.
///
/// Falls back to [`path_to_id`] for non-SMB mounts and for SMB mounts where
/// `statfs` no longer recovers the mount info (typical right after unmount).
/// The unmount path should generally use [`VolumeManager::find_by_root`]
/// instead, which doesn't depend on `statfs`.
pub(crate) fn volume_id_for_mount(mount_path: &str) -> String {
    if let Some(info) = get_smb_mount_info(mount_path) {
        smb_volume_id(&info.server, info.port, &info.share)
    } else {
        path_to_id(mount_path)
    }
}
