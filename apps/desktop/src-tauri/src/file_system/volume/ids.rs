//! Volume ID helpers.
//!
//! Builds the stable, collision-free string IDs that key per-volume state
//! (`lastUsedPaths`, tab `volumeId` fields, the `VolumeManager` registry).
//! `mod.rs` re-exports both functions, so callers keep importing
//! `volume::path_to_id` / `volume::smb_volume_id` unchanged.

use super::DEFAULT_VOLUME_ID;

/// Convert a mount path to a safe ID string.
///
/// **Don't use this for SMB mounts.** Use [`smb_volume_id`] instead: it keys by
/// the underlying mount (server, port, share) rather than the path-shape, so two
/// SMB shares with the same case-folded name on different servers don't collide.
pub(crate) fn path_to_id(path: &str) -> String {
    if path == "/" {
        return DEFAULT_VOLUME_ID.to_string();
    }
    path.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase()
}

/// Build a stable, collision-free volume ID for an SMB mount.
///
/// Format: `smb-{server}-{port}-{share}`, with dots in the server (IPs) replaced
/// by `-`, everything else stripped down to `[a-z0-9-]`, and both server and share
/// lowercased.
///
/// # Why not [`path_to_id`]?
///
/// Path-based IDs lowercase the mount path, so two SMB shares with the same
/// case-folded name on different servers (a NAS sharing `Public`, a Docker
/// container sharing `public`) collide on `volumespublic`. The collision
/// cross-contaminates `lastUsedPaths`, tab `volumeId` fields, and any other
/// per-volume state, which surfaces as wrong-cased paths flowing into
/// `SmbVolume::list_directory` and the server returning
/// `STATUS_OBJECT_PATH_NOT_FOUND`. Keying by (server, port, share) instead of
/// path-shape prevents the collision at the root.
///
/// # Case folding
///
/// - Server: DNS hostnames are case-insensitive, so `Naspolya` and `naspolya` are the same host.
///   Lowercased.
/// - Share: SMB is case-insensitive for share names per the protocol (Windows and Samba default),
///   so `Public` and `public` on the same server are the same share. Lowercased.
/// - Port: literal, no folding.
pub fn smb_volume_id(server: &str, port: u16, share: &str) -> String {
    fn sanitize(s: &str) -> String {
        s.chars()
            .map(|c| if c == '.' { '-' } else { c })
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_lowercase()
    }
    format!("smb-{}-{}-{}", sanitize(server), port, sanitize(share))
}

#[cfg(test)]
mod id_tests {
    use super::*;

    #[test]
    fn smb_volume_id_distinguishes_servers_with_same_share_name() {
        // The exact bug that motivated per-mount IDs: QNAP's `Public` share and a
        // Docker container's `public` share would both collide on `volumespublic`
        // under the old path-shape ID scheme, cross-contaminating `lastUsedPaths`,
        // tabs, and per-volume state. They must produce distinct IDs.
        let qnap = smb_volume_id("Naspolya", 445, "Public");
        let docker = smb_volume_id("localhost", 10494, "public");
        assert_ne!(qnap, docker);
    }

    #[test]
    fn smb_volume_id_is_stable_for_identical_inputs() {
        // Same logical mount → same ID across calls (required for `lastUsedPaths`
        // and tab state to roundtrip).
        let a = smb_volume_id("naspolya", 445, "naspi");
        let b = smb_volume_id("naspolya", 445, "naspi");
        assert_eq!(a, b);
    }

    #[test]
    fn smb_volume_id_treats_server_case_insensitively() {
        // DNS hostnames are case-insensitive; mounting `smb://Naspolya/...` and
        // `smb://naspolya/...` is the same mount, so the IDs must match.
        assert_eq!(
            smb_volume_id("Naspolya", 445, "naspi"),
            smb_volume_id("naspolya", 445, "naspi")
        );
    }

    #[test]
    fn smb_volume_id_treats_share_case_insensitively() {
        // The SMB protocol treats share names case-insensitively (Windows/Samba
        // default). Two mounts of the same server with case-only-different shares
        // are the same share.
        assert_eq!(
            smb_volume_id("naspolya", 445, "Public"),
            smb_volume_id("naspolya", 445, "public")
        );
    }

    #[test]
    fn smb_volume_id_distinguishes_ports() {
        // Same host, same share name, different port = different server in
        // practice (typical with reverse proxies and dev fixtures on localhost).
        assert_ne!(
            smb_volume_id("localhost", 10480, "public"),
            smb_volume_id("localhost", 10494, "public")
        );
    }

    #[test]
    fn smb_volume_id_handles_ip_addresses_without_collision() {
        // IPs with dots must not be silently squashed in a way that lets two
        // different IPs collide.
        assert_ne!(
            smb_volume_id("192.168.1.111", 445, "naspi"),
            smb_volume_id("192.168.1.112", 445, "naspi")
        );
    }

    #[test]
    fn smb_volume_id_does_not_collide_with_path_based_ids() {
        // No realistic local volume path should ever produce the same ID as an
        // SMB mount. The `smb-` prefix is the contract.
        let smb = smb_volume_id("localhost", 10494, "public");
        let local = path_to_id("/Volumes/Smb");
        assert_ne!(smb, local);
        assert!(smb.starts_with("smb-"), "got: {smb}");
    }

    #[test]
    fn path_to_id_still_works_for_non_smb_paths() {
        // Sanity: the path-based helper is unchanged for local volumes.
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/Volumes/External"), "volumesexternal");
    }
}
