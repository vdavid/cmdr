//! SMB mount parsing and enrichment: pulling `(server, share, user, port)` out
//! of a `statfs` mount source, tagging volumes with their SMB connection state,
//! and deriving the SMB-aware volume ID.

use super::{LocationInfo, SmbConnectionState, is_smb_fs_type};

/// Enriches discovered locations with everything only the registered `Volume`
/// knows: its capability surface and its SMB connection state.
///
/// Discovery builds a `LocationInfo` from the mount (name, path, icon, fs type);
/// what the BACKEND can do lives on the `Volume` in `VolumeManager`, and this is
/// where the two meet. For each location it looks up the registered volume by id
/// and copies `capabilities()` plus `smb_connection_state()` across. A location
/// with no registered volume (a favorite, or one discovery found before
/// registration) keeps `capabilities: None`, and the frontend falls back to its
/// per-kind defaults. SMB shares without a direct smb2 session (typical
/// OS-mounted shares before auto-upgrade) are tagged `OsMount` so the picker can
/// show the yellow indicator.
///
/// Used by the `list_volumes` IPC call, the `volumes-changed` push, and the MCP
/// `cmdr://state` resource — all three need the same enrichment, so it lives in
/// one place. Add new enrichment fields here, not at each call site.
pub fn enrich_from_volume_registry(volumes: &mut [LocationInfo]) {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    for vol in volumes.iter_mut() {
        if let Some(registered) = manager.get(&vol.id) {
            vol.capabilities = Some(registered.capabilities());
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

// Deriving a volume ID from a mount lives in `ids.rs`, which owns the rule for
// every mount kind (SMB, other network, local) in one place.

#[cfg(test)]
mod enrichment_tests {
    use super::*;
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_fs::volume::InMemoryVolume;
    use std::sync::Arc;

    fn location(id: &str) -> LocationInfo {
        LocationInfo {
            id: id.to_string(),
            name: "Test".to_string(),
            path: "/tmp/enrichment-test".to_string(),
            category: crate::volumes::LocationCategory::AttachedVolume,
            icon: None,
            is_ejectable: false,
            fs_type: Some("apfs".to_string()),
            supports_trash: true,
            is_read_only: false,
            is_disk_image: false,
            smb_connection_state: None,
            usb_speed: None,
            capabilities: None,
        }
    }

    /// The one wiring that makes the whole published capability surface reach the
    /// user: discovery builds the `LocationInfo`, the registry knows the backend,
    /// and this is where they meet. Nothing downstream can tell a silently-empty
    /// `capabilities` from "no backend registered" — the frontend just falls back
    /// to its per-kind defaults and the volume's real answer never lands.
    #[test]
    fn a_registered_backend_publishes_its_capabilities_onto_the_location() {
        let id = "enrichment-test-registered";
        get_volume_manager().register(id, Arc::new(InMemoryVolume::new("Test")));

        let mut locations = vec![location(id)];
        enrich_from_volume_registry(&mut locations);

        let published = locations[0].capabilities.expect("a registered backend must publish");
        assert!(published.is_writable, "InMemoryVolume is writable and must say so");
        assert!(published.can_export, "InMemoryVolume exports and must say so");

        get_volume_manager().unregister(id);
    }

    /// A location with no backend (a favorite, or one discovery found before
    /// registration) carries `None` rather than a guess, which is what lets the
    /// frontend fall back to its per-kind defaults.
    #[test]
    fn a_location_with_no_registered_backend_stays_unanswered() {
        let mut locations = vec![location("enrichment-test-unregistered")];
        enrich_from_volume_registry(&mut locations);

        assert!(locations[0].capabilities.is_none());
    }
}
