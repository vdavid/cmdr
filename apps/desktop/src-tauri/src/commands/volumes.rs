//! Tauri commands for volume operations.
//!
//! One module for macOS and Linux. Everything genuinely per-platform lives a
//! layer down in `volumes/` and `volumes_linux/`; what's left up here is the
//! same on both, and the two spots that aren't are the `platform` alias and
//! [`NETWORK_FS_TYPE`].

use serde::Serialize;
use tokio::time::Duration;

use super::util::{TimedOut, blocking_with_timeout_flag};
use crate::location::{Location, ResolveLocationResult};
use crate::volume_listing;

#[cfg(target_os = "macos")]
use crate::volumes as platform;
#[cfg(target_os = "linux")]
use crate::volumes_linux as platform;

use platform::{DEFAULT_VOLUME_ID, LocationCategory, VolumeInfo, VolumeSpaceInfo};

const VOLUME_TIMEOUT: Duration = Duration::from_secs(2);

/// The `fs_type` the synthetic `network` volume reports: whatever the OS calls an
/// SMB mount, so a consumer classifying by fs type sees a share and not an unknown
/// filesystem.
#[cfg(target_os = "macos")]
const NETWORK_FS_TYPE: &str = "smbfs";
#[cfg(target_os = "linux")]
const NETWORK_FS_TYPE: &str = "cifs";

/// Result of resolving a path to its containing volume.
/// Unlike `TimedOut<Option<VolumeInfo>>`, `timed_out: true` means "the filesystem
/// didn't respond, we genuinely don't know" (not "here's a fallback").
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PathVolumeResolution {
    pub volume: Option<VolumeInfo>,
    pub timed_out: bool,
}

/// Lists all mounted volumes, including connected MTP devices, each enriched
/// with what its registered backend can do.
#[tauri::command]
#[specta::specta]
pub async fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    let (data, timed_out) = volume_listing::list_with_timeout(VOLUME_TIMEOUT).await;
    TimedOut { data, timed_out }
}

/// Gets the default volume ID (root filesystem).
#[tauri::command]
#[specta::specta]
pub fn get_default_volume_id() -> String {
    DEFAULT_VOLUME_ID.to_string()
}

/// Gets space information for a volume at the given path.
/// Returns total and available bytes for the volume.
/// For MTP paths (`mtp://`), fetches from the MTP connection manager instead of
/// asking the filesystem.
#[tauri::command]
#[specta::specta]
pub async fn get_volume_space(path: String) -> TimedOut<Option<VolumeSpaceInfo>> {
    if let Some((total_bytes, available_bytes)) = volume_listing::mtp_space_for_path(&path).await {
        return TimedOut {
            data: Some(VolumeSpaceInfo {
                total_bytes,
                available_bytes,
            }),
            timed_out: false,
        };
    }
    blocking_with_timeout_flag(VOLUME_TIMEOUT, None, move || platform::get_volume_space(&path)).await
}

/// Resolves a path to its containing volume without enumerating all volumes.
/// Reads the mount table for filesystem paths (`statfs` on macOS,
/// `/proc/self/mountinfo` on Linux; <1ms for local disks) and dispatches on
/// protocol for MTP/SMB paths. Returns `timed_out: true` if the filesystem
/// didn't respond within 2s.
#[tauri::command]
#[specta::specta]
pub async fn resolve_path_volume(path: String) -> PathVolumeResolution {
    let (volume, timed_out) = resolve_path_to_volume(path, VOLUME_TIMEOUT).await;
    PathVolumeResolution { volume, timed_out }
}

/// Resolves a path to a `Location` (`volume_id` + the path itself), the
/// canonical path→volume resolver for navigation edges. Shares the full
/// protocol dispatch with `resolve_path_volume`, so `mtp://` / `smb://` virtual
/// paths resolve correctly (calling `resolve_path_volume_fast` alone would
/// return `None` for them). `location: None` means no volume contains the path;
/// `timed_out: true` means the filesystem didn't respond.
#[tauri::command]
#[specta::specta]
pub async fn resolve_location(path: String) -> ResolveLocationResult {
    resolve_location_inner(path, VOLUME_TIMEOUT).await
}

/// Shared body of [`resolve_location`] with an injectable filesystem timeout.
/// Production passes `VOLUME_TIMEOUT`; tests pass a generous timeout so a
/// CPU-saturated box can't trip `timed_out` before the (sub-millisecond) mount-table
/// closure is even scheduled onto the blocking pool — the flake source.
async fn resolve_location_inner(path: String, fs_timeout: Duration) -> ResolveLocationResult {
    let (volume, timed_out) = resolve_path_to_volume(path.clone(), fs_timeout).await;
    ResolveLocationResult {
        location: volume.map(|v| Location { volume_id: v.id, path }),
        timed_out,
    }
}

/// Shared body for `resolve_path_volume` and `resolve_location`: resolves a path
/// to its containing volume via protocol dispatch (`mtp://` → matching connected
/// storage, `smb://` → the virtual `network` volume) or, for filesystem paths,
/// the mount table under `fs_timeout`. Returns the volume (if any) and whether it
/// timed out.
async fn resolve_path_to_volume(path: String, fs_timeout: Duration) -> (Option<VolumeInfo>, bool) {
    // MTP protocol dispatch
    if path.starts_with("mtp://") {
        return (volume_listing::mtp_volume_for_path(&path).await, false);
    }

    // SMB/network protocol paths → return the virtual network volume
    if path.starts_with("smb://") {
        return (
            Some(VolumeInfo {
                id: "network".to_string(),
                name: "Network".to_string(),
                path: "smb://".to_string(),
                category: LocationCategory::Network,
                icon: None,
                is_ejectable: false,
                fs_type: Some(NETWORK_FS_TYPE.to_string()),
                supports_trash: false,
                mount_is_read_only: false,
                is_disk_image: false,
                smb_connection_state: None,
                usb_speed: None,
                capabilities: None,
            }),
            false,
        );
    }

    // Filesystem paths: resolve via the mount table with a timeout. A path INSIDE an
    // archive resolves to the PARENT drive (display semantics — the FE holds the parent
    // drive id, never a per-archive id), so read the `.zip`'s real location, not the
    // inner path (which isn't a real FS path). The boundary check runs inside the
    // timeout-wrapped closure so its stat can't block IPC on a hung mount.
    let result = blocking_with_timeout_flag(fs_timeout, None, move || {
        let fs_path = match crate::file_system::volume::backends::archive::confirm_archive_boundary(
            std::path::Path::new(&path),
        ) {
            Some((zip_path, _inner)) => zip_path,
            None => std::path::PathBuf::from(&path),
        };
        platform::resolve_path_volume_fast(&fs_path.to_string_lossy())
    })
    .await;
    (result.data, result.timed_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A generous filesystem timeout for the resolve tests: the local mount-table read
    /// completes in well under a millisecond, but the production 2 s `VOLUME_TIMEOUT`
    /// can elapse before the blocking closure is scheduled on a CPU-saturated box,
    /// flaking the `timed_out` assertion. An hour is deterministic for this work.
    const TEST_FS_TIMEOUT: Duration = Duration::from_secs(3600);

    #[tokio::test]
    async fn resolve_location_local_dir_returns_root_volume() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().to_string_lossy().to_string();

        let result = resolve_location_inner(path.clone(), TEST_FS_TIMEOUT).await;

        assert!(!result.timed_out);
        let location = result.location.expect("local dir should resolve to a volume");
        // The temp dir lives on the boot volume.
        assert_eq!(location.volume_id, DEFAULT_VOLUME_ID);
        // The resolved path is the input path (the dir the caller wants to land on).
        assert_eq!(location.path, path);
    }

    #[tokio::test]
    async fn resolve_location_local_file_returns_root_volume() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, b"hi").expect("write temp file");
        let path = file_path.to_string_lossy().to_string();

        let result = resolve_location_inner(path.clone(), TEST_FS_TIMEOUT).await;

        assert!(!result.timed_out);
        let location = result.location.expect("local file should resolve to a volume");
        assert_eq!(location.volume_id, DEFAULT_VOLUME_ID);
        assert_eq!(location.path, path);
    }

    #[tokio::test]
    async fn resolve_location_inside_an_archive_returns_the_parent_drive() {
        // A path INSIDE a `.zip` resolves to the parent drive (display semantics),
        // not `None` — so restoring a pane deep-linked inside an archive works. The
        // inner path isn't a real FS path, so this only works by resolving the
        // `.zip`'s real location. Ran on macOS only while the Linux command was a
        // separate copy that skipped the boundary check, which is how that platform
        // shipped without it.
        let dir = tempfile::tempdir().expect("create temp dir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");
        let inner = zip.join("docs/readme.txt");
        let path = inner.to_string_lossy().to_string();

        let result = resolve_location_inner(path.clone(), TEST_FS_TIMEOUT).await;

        assert!(!result.timed_out);
        let location = result
            .location
            .expect("archive-inner path should resolve to the parent drive");
        assert_eq!(location.volume_id, DEFAULT_VOLUME_ID);
        // The returned path is the full inner path the caller wants to land on.
        assert_eq!(location.path, path);
    }

    #[tokio::test]
    async fn resolve_location_unresolvable_mtp_path_returns_none() {
        // No MTP device is connected in tests, so the protocol-dispatch branch
        // finds no matching storage and yields `location: None` (proving
        // `resolve_location` runs the full dispatch, not just the local helper).
        let result = resolve_location("mtp://no-such-device/1/folder".to_string()).await;

        assert!(!result.timed_out);
        assert!(result.location.is_none());
    }

    #[tokio::test]
    async fn resolve_location_smb_path_returns_the_network_volume() {
        // An `smb://` path has no mount table entry to find, so it resolves by protocol
        // dispatch to the synthetic `network` volume rather than to nothing.
        let result = resolve_path_volume("smb://server/share/file.txt".to_string()).await;

        assert!(!result.timed_out);
        let volume = result.volume.expect("an smb:// path resolves to the network volume");
        assert_eq!(volume.id, "network");
        assert_eq!(volume.category, LocationCategory::Network);
        // The OS's own name for an SMB mount, so the fs-type predicates recognize it.
        assert_eq!(volume.fs_type.as_deref(), Some(NETWORK_FS_TYPE));
        assert!(!volume.supports_trash, "a network share has no trash");
    }
}
