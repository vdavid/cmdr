//! Attached-volume enumeration: snapshotting the kernel mount table via the
//! non-blocking `getfsstat`, filtering it to user-facing `/Volumes/*` entries,
//! and enriching only local mounts (network mounts stay non-blocking so a hung
//! mount can't stall discovery). See `DETAILS.md` § "Hung mounts".

use super::*;

/// One entry from the kernel mount table, as returned by `getfsstat(MNT_NOWAIT)`.
///
/// Every field comes straight out of `statfs` with no follow-up syscall, so
/// building one never talks to the backing filesystem. That is the whole reason
/// discovery uses `getfsstat` instead of NSFileManager's volume enumeration: the
/// enumeration `getattrlist`s every mount, which blocks 30s–forever on a hung
/// network mount and froze the app at launch. See `DETAILS.md` § "Hung mounts".
struct MountEntry {
    /// Mount point, e.g. `/Volumes/naspi` (`f_mntonname`).
    mount_point: String,
    /// Filesystem type, e.g. `apfs`, `exfat`, `smbfs` (`f_fstypename`).
    fs_type: String,
    /// Mount source, e.g. `//david@192.168.1.111/naspi` for SMB (`f_mntfromname`).
    mount_from: String,
    /// Whether the mount carries the `MNT_RDONLY` flag.
    is_read_only: bool,
}

/// Snapshot the kernel mount table without blocking on any mount.
///
/// `getfsstat(MNT_NOWAIT)` returns cached mount metadata and never round-trips to
/// a filesystem, so a wedged network mount can't stall it — unlike `MNT_WAIT`,
/// which is what makes plain `df` hang. Returns an empty list if the syscall fails.
fn enumerate_mounts() -> Vec<MountEntry> {
    // First pass: ask how many mounts exist (null buffer writes nothing).
    // SAFETY: `getfsstat(NULL, 0, flags)` is the documented count query; with a
    // null buffer and zero size the kernel only returns the mount count.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count <= 0 {
        return Vec::new();
    }

    // A few slots of slack in case a mount appears between the two calls.
    let capacity = count as usize + 4;
    let mut buf: Vec<libc::statfs> = Vec::with_capacity(capacity);
    let bufsize = (capacity * size_of::<libc::statfs>()) as libc::c_int;
    // SAFETY: `buf` has room for `capacity` `statfs` records and `bufsize` matches
    // that byte length, so the kernel fills at most `capacity` records and returns
    // how many it wrote.
    let filled = unsafe { libc::getfsstat(buf.as_mut_ptr(), bufsize, libc::MNT_NOWAIT) };
    if filled <= 0 {
        return Vec::new();
    }
    // SAFETY: the kernel initialized `filled` records; clamp to our capacity in
    // case the mount table grew past the slack between the two calls.
    unsafe { buf.set_len((filled as usize).min(capacity)) };

    buf.iter()
        .map(|s| MountEntry {
            mount_point: cstr_field_to_string(&s.f_mntonname),
            fs_type: cstr_field_to_string(&s.f_fstypename),
            mount_from: cstr_field_to_string(&s.f_mntfromname),
            is_read_only: (s.f_flags & libc::MNT_RDONLY as u32) != 0,
        })
        .collect()
}

/// Convert a NUL-terminated `c_char` array from `statfs` into a `String`
/// (UTF-8 lossy, since mount points and volume names can be non-ASCII).
fn cstr_field_to_string(field: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Whether a mount point should surface as an attached volume in the switcher.
///
/// Mirrors the old NSFileManager filter: only `/Volumes/*`, never the boot
/// volume, the hidden system volumes, or cloud-storage placeholder mounts. Also
/// skips dot-prefixed hidden volumes (e.g. `/Volumes/.timemachine`) that
/// NSFileManager's `SkipHiddenVolumes` used to drop. Pure, so it's unit-testable.
fn is_attached_volume_path(path: &str) -> bool {
    if !path.starts_with("/Volumes/") {
        return false;
    }
    if path.starts_with("/System") || path.contains("/Preboot") || path.contains("/Recovery") {
        return false;
    }
    if path.contains("/Library/CloudStorage") {
        return false;
    }
    // Hidden volume (leading dot on the `/Volumes` child), e.g. `/Volumes/.timemachine`.
    if let Some(name) = Path::new(path).file_name().and_then(|n| n.to_str())
        && name.starts_with('.')
    {
        return false;
    }
    true
}

/// Metadata for a LOCAL mount, resolved via blocking macOS APIs (NSURL +
/// DiskArbitration + NSWorkspace). Only computed for local mounts — network
/// mounts skip it so a hung mount never blocks discovery.
struct LocalVolumeMeta {
    name: String,
    is_ejectable: bool,
    icon: Option<String>,
    is_disk_image: bool,
}

/// The `(id, display_name)` for a network mount, from the non-blocking
/// `f_mntfromname`. SMB mounts key their ID on `(server, port, share)` and read
/// as "share on server"; other network mounts fall back to a path-derived ID and
/// name. No syscalls: everything comes from the `getfsstat` snapshot. Pure.
fn network_id_and_name(mount: &MountEntry) -> (String, String) {
    if is_smb_fs_type(Some(&mount.fs_type))
        && let Some(info) = parse_smb_mount_source(&mount.mount_from)
    {
        let display = crate::network::smb_upgrade::friendly_server_name(&info.server);
        let name = format!("{} on {}", info.share, display);
        return (smb_volume_id(&info.server, info.port, &info.share), name);
    }
    (
        path_to_id(&mount.mount_point),
        volume_name_from_path(&mount.mount_point),
    )
}

/// Classify one mount-table entry into a switcher [`LocationInfo`], or `None` if
/// it isn't a user-facing attached volume.
///
/// Network mounts (SMB, NFS, WebDAV, …) are built purely from the non-blocking
/// `statfs` data already in `mount`; `resolve_local` is NOT called for them. That
/// is the guarantee a hung network mount can't block discovery of the other
/// volumes. Local mounts call `resolve_local` for their name, ejectability, icon,
/// and disk-image status (safe: local disks don't hang). Splitting the blocking
/// enrichment behind a closure also keeps the classification unit-testable.
fn build_attached_location(
    mount: &MountEntry,
    resolve_local: impl FnOnce(&str) -> LocalVolumeMeta,
) -> Option<LocationInfo> {
    let path = mount.mount_point.as_str();
    if !is_attached_volume_path(path) {
        return None;
    }
    let fs_type = mount.fs_type.clone();
    let supports_trash = supports_trash_for_fs_type(Some(&fs_type));

    let (id, name, is_ejectable, icon, is_disk_image) = if is_network_fs_type(Some(&fs_type)) {
        // Network mount: derive everything from the non-blocking snapshot. Never
        // touch NSURL / NSWorkspace / DiskArbitration here — those are exactly the
        // calls that hang on a dead mount. `is_ejectable` is cosmetically moot for
        // network mounts (the eject affordance keys on `smbConnectionState`, and
        // the eject flow forces it true), so a safe `false` costs nothing.
        let (id, name) = network_id_and_name(mount);
        (id, name, false, None, false)
    } else {
        // Local mount: safe to run the blocking enrichment.
        let meta = resolve_local(path);
        (
            path_to_id(path),
            meta.name,
            meta.is_ejectable,
            meta.icon,
            meta.is_disk_image,
        )
    };

    Some(LocationInfo {
        id,
        name,
        path: path.to_string(),
        category: LocationCategory::AttachedVolume,
        icon,
        is_ejectable,
        fs_type: Some(fs_type),
        supports_trash,
        is_read_only: mount.is_read_only,
        is_disk_image,
        smb_connection_state: None,
        usb_speed: None,
    })
}

/// Get attached volumes (external drives, USB, network mounts, etc.).
///
/// Enumerates via the non-blocking `getfsstat` snapshot, then enriches only LOCAL
/// mounts through blocking macOS APIs. A hung network mount contributes its
/// getfsstat-derived entry and never blocks the others. See `DETAILS.md`
/// § "Hung mounts".
pub fn get_attached_volumes() -> Vec<LocationInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    // Drain autoreleased ObjC objects from the per-local-mount NSURL enrichment.
    // Called from spawn_blocking / helper threads that lack AppKit's pool.
    autoreleasepool(|_| {
        let mut volumes: Vec<LocationInfo> = enumerate_mounts()
            .iter()
            .filter_map(|mount| {
                build_attached_location(mount, |path| {
                    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                    LocalVolumeMeta {
                        name: get_volume_name(&url, path),
                        is_ejectable: get_bool_resource(&url, "NSURLVolumeIsEjectableKey").unwrap_or(false),
                        icon: get_icon_for_path(path),
                        is_disk_image: disk_image::is_disk_image_mount(path),
                    }
                })
            })
            .collect();

        // Sort alphabetically
        volumes.sort_by_key(|a| a.name.to_lowercase());
        volumes
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Hung-mount guard: getfsstat-based discovery (Bug: dead mount froze launch)
    // ========================================================================

    fn mount(mount_point: &str, fs_type: &str, mount_from: &str, is_read_only: bool) -> MountEntry {
        MountEntry {
            mount_point: mount_point.to_string(),
            fs_type: fs_type.to_string(),
            mount_from: mount_from.to_string(),
            is_read_only,
        }
    }

    /// A `resolve_local` that fails the test if invoked. Used to prove a network
    /// mount is classified WITHOUT any blocking NSURL/DiskArbitration/NSWorkspace
    /// call — the guarantee that a hung mount can't stall discovery.
    fn forbidden_resolver(_path: &str) -> LocalVolumeMeta {
        panic!("resolve_local must NOT run for a network mount");
    }

    #[test]
    fn attached_volume_path_filter_matches_switcher_rules() {
        assert!(is_attached_volume_path("/Volumes/MyDrive"));
        assert!(is_attached_volume_path("/Volumes/naspi"));
        // Boot volume, system, cloud, and non-/Volumes paths are excluded.
        assert!(!is_attached_volume_path("/"));
        assert!(!is_attached_volume_path("/System/Volumes/Data"));
        assert!(!is_attached_volume_path("/Volumes/Recovery"));
        assert!(!is_attached_volume_path("/Users/david"));
        assert!(!is_attached_volume_path("/Volumes/Foo/Library/CloudStorage/Dropbox"));
        // Hidden volumes (NSFileManager used to drop these via SkipHiddenVolumes).
        assert!(!is_attached_volume_path("/Volumes/.timemachine"));
    }

    #[test]
    fn smb_mount_classifies_without_blocking_enrichment() {
        // A wedged SMB mount must be classified purely from getfsstat data; the
        // blocking resolver must never run, so a dead NAS can't stall discovery.
        let m = mount("/Volumes/naspi", "smbfs", "//david@192.168.1.111/naspi", false);
        let loc = build_attached_location(&m, forbidden_resolver).expect("SMB mount is an attached volume");

        assert_eq!(loc.id, smb_volume_id("192.168.1.111", 445, "naspi"));
        assert!(loc.name.contains("naspi"), "name shows the share: {}", loc.name);
        assert!(loc.name.contains(" on "), "name shows 'share on server': {}", loc.name);
        assert_eq!(loc.fs_type.as_deref(), Some("smbfs"));
        assert_eq!(loc.category, LocationCategory::AttachedVolume);
        assert!(!loc.is_ejectable, "network mounts take the safe non-blocking default");
        assert!(loc.icon.is_none());
        assert!(!loc.is_disk_image);
    }

    #[test]
    fn nfs_mount_classifies_without_blocking_enrichment() {
        let m = mount("/Volumes/export", "nfs", "server:/export", true);
        let loc = build_attached_location(&m, forbidden_resolver).expect("NFS mount is an attached volume");
        assert_eq!(loc.id, path_to_id("/Volumes/export"));
        assert_eq!(loc.name, "export");
        assert!(loc.is_read_only, "MNT_RDONLY flag propagates from getfsstat");
        assert_eq!(loc.fs_type.as_deref(), Some("nfs"));
    }

    #[test]
    fn local_mount_runs_the_enrichment_closure() {
        // Local mounts DO get the (safe) blocking enrichment; here we inject a
        // fake so the test stays hermetic and asserts the values flow through.
        let m = mount("/Volumes/USB", "exfat", "/dev/disk4s1", false);
        let loc = build_attached_location(&m, |path| {
            assert_eq!(path, "/Volumes/USB");
            LocalVolumeMeta {
                name: "My USB".to_string(),
                is_ejectable: true,
                icon: Some("icon-data".to_string()),
                is_disk_image: false,
            }
        })
        .expect("local mount is an attached volume");

        assert_eq!(loc.id, path_to_id("/Volumes/USB"));
        assert_eq!(loc.name, "My USB");
        assert!(loc.is_ejectable);
        assert_eq!(loc.icon.as_deref(), Some("icon-data"));
        assert_eq!(loc.fs_type.as_deref(), Some("exfat"));
    }

    #[test]
    fn filtered_mount_yields_no_location() {
        // The boot volume and system mounts are dropped before any enrichment.
        assert!(build_attached_location(&mount("/", "apfs", "/dev/disk3s1", false), forbidden_resolver).is_none());
        assert!(
            build_attached_location(&mount("/System/Volumes/Data", "apfs", "x", false), forbidden_resolver).is_none()
        );
    }

    #[test]
    fn enumerate_mounts_finds_the_boot_volume() {
        // getfsstat should always return at least the root mount on a live system,
        // and it must never block (this test would hang if it did).
        let mounts = enumerate_mounts();
        assert!(!mounts.is_empty(), "getfsstat returned no mounts");
        assert!(mounts.iter().any(|m| m.mount_point == "/"), "root mount missing");
    }
}
