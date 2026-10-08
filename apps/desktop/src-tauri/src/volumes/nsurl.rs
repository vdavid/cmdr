//! NSURL resource lookups (volume name, ejectability, capacities), the
//! path-derived volume-name fallback, per-path icon fetching, and volume-space
//! reporting. The blocking macOS enrichment layer, only run for local mounts.

use crate::file_system::volume::SpaceInfo;
use std::path::Path;

/// Display name derived purely from a mount path: the last path component, or
/// "Macintosh HD" for the boot volume. The non-blocking fallback used for network
/// mounts (`network_id_and_name`) and when an NSURL localized-name lookup misses.
pub(crate) fn volume_name_from_path(path: &str) -> String {
    if path == "/" {
        return "Macintosh HD".to_string();
    }
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// Get the display name for a volume.
pub(crate) fn get_volume_name(url: &objc2_foundation::NSURL, path: &str) -> String {
    // Try localized name first
    if let Some(name) = get_string_resource(url, "NSURLVolumeLocalizedNameKey") {
        return name;
    }
    if let Some(name) = get_string_resource(url, "NSURLVolumeNameKey") {
        return name;
    }
    // Fallback to path-based name
    volume_name_from_path(path)
}

/// The volume's filesystem UUID, the identity its volume ID keys on.
///
/// `None` for a volume that has none (tmpfs, most FUSE mounts, some disk
/// images), which sends `volume_id_for` to the path-derived fallback.
///
/// ❌ Never call this for a network mount: it's an NSURL round-trip that can hang
/// for minutes on a dead one. `ids::volume_id_for_mount` gates it on the
/// filesystem type; `DETAILS.md` § "Hung mounts" is why.
pub(crate) fn get_volume_uuid(url: &objc2_foundation::NSURL) -> Option<String> {
    get_string_resource(url, "NSURLVolumeUUIDStringKey")
}

/// [`get_volume_uuid`] for a caller that holds a path rather than an `NSURL`.
pub(crate) fn get_volume_uuid_for_path(path: &str) -> Option<String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    // Drain the autoreleased NSURL/NSString: callers run on helper threads that
    // have no AppKit pool of their own.
    autoreleasepool(|_| get_volume_uuid(&NSURL::fileURLWithPath(&NSString::from_str(path))))
}

/// What macOS reports about the disk behind a local volume, each `None` when it gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MediaFacts {
    /// `NSURLVolumeIsEjectableKey`: the MEDIA ejects from its drive under software control. True
    /// for a disk image and most USB sticks; false for an external disk macOS sees as fixed (a
    /// Thunderbolt NVMe SSD, some USB bridges).
    pub(crate) ejectable: Option<bool>,
    /// `NSURLVolumeIsInternalKey`: the disk sits on an internal bus.
    pub(crate) internal: Option<bool>,
}

/// Whether a local volume with these `facts` is one a person can eject. Pure.
///
/// Ejectable media, or a disk macOS places on an external bus. ❗ Only an explicit "not
/// internal" counts: an internal volume stays without a button (Cmdr has no way to mount it
/// back), and so does a disk macOS wouldn't place.
pub(crate) fn offers_eject(facts: MediaFacts) -> bool {
    facts.ejectable == Some(true) || facts.internal == Some(false)
}

/// Whether the local volume mounted at `mount_point` is one a person can eject. The one answer
/// both `get_attached_volumes` and `resolve_path_volume_fast` give, so the switcher's button and
/// the eject command's own check can't disagree.
///
/// ❌ Never call this for a network mount: NSURL resource lookups can hang for minutes on a dead
/// one (`DETAILS.md` § "Hung mounts").
pub(crate) fn is_volume_ejectable(url: &objc2_foundation::NSURL, mount_point: &str) -> bool {
    // The boot volume never ejects, whatever disk the Mac booted from.
    if mount_point == "/" {
        return false;
    }
    offers_eject(MediaFacts {
        ejectable: get_bool_resource(url, "NSURLVolumeIsEjectableKey"),
        internal: get_bool_resource(url, "NSURLVolumeIsInternalKey"),
    })
}

/// Get icon for a path as base64-encoded WebP.
///
/// Returns `None` while the FDA decision is pending. NSWorkspace icon
/// resolution touches several TCC-gated services (MediaLibrary, AppData,
/// Desktop/Documents/Downloads/Pictures/Movies/Music) even when the input
/// path itself isn't on those lists, so during onboarding we skip the
/// fetch and let the frontend fall back to a generic folder/volume icon.
/// `start_indexing_after_fda_decision` (deny path) and a fresh launch with
/// FDA granted (allow path) both clear the gate and re-emit
/// `volumes-changed`, populating icons.
pub(crate) fn get_icon_for_path(path: &str) -> Option<String> {
    if crate::fda_gate::is_fda_pending_runtime() {
        return None;
    }
    crate::icons::get_icon_for_path(path)
}

/// Get a resource value from an NSURL and convert it using the provided extractor.
fn get_nsurl_resource<T>(
    url: &objc2_foundation::NSURL,
    key: &str,
    extractor: impl FnOnce(objc2::rc::Retained<objc2::runtime::AnyObject>) -> Option<T>,
) -> Option<T> {
    use objc2::rc::Retained;
    use objc2_foundation::NSString;

    let key = NSString::from_str(key);
    let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
    // SAFETY: `url` is a live `NSURL` and `key` a live `NSString`; `getResourceValue:forKey:error:`
    // writes the looked-up value into `value` (left `None` when the key is absent) and the cached
    // resource value is autoreleased into the caller's pool. We only read `value` after success.
    let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

    if success.is_ok() {
        value.and_then(extractor)
    } else {
        None
    }
}

/// Get a boolean resource value from an NSURL.
fn get_bool_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<bool> {
    use objc2_foundation::NSNumber;
    get_nsurl_resource(url, key, |obj| obj.downcast::<NSNumber>().ok().map(|n| n.boolValue()))
}

/// Get a string resource value from an NSURL.
fn get_string_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<String> {
    use objc2_foundation::NSString;
    get_nsurl_resource(url, key, |obj| obj.downcast::<NSString>().ok().map(|s| s.to_string()))
}

/// Get a u64 resource value from an NSURL (for capacity values).
fn get_u64_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<u64> {
    use objc2_foundation::NSNumber;
    get_nsurl_resource(url, key, |obj| {
        obj.downcast::<NSNumber>().ok().map(|n| n.unsignedLongLongValue())
    })
}

/// Get space information for a volume containing the given path.
///
/// A mounted filesystem always has a capacity, so this is always
/// [`SpaceInfo::Bounded`]. NSURL reports no used figure of its own, so
/// [`SpaceInfo::bounded`] derives it.
pub fn get_volume_space(path: &str) -> Option<SpaceInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::NSURL;

    // Drain autoreleased ObjC objects (NSURL, NSString, NSNumber).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(path));

        let total = get_u64_resource(&url, "NSURLVolumeTotalCapacityKey")?;
        let available = get_u64_resource(&url, "NSURLVolumeAvailableCapacityForImportantUsageKey")
            .filter(|&v| v > 0)
            .or_else(|| get_u64_resource(&url, "NSURLVolumeAvailableCapacityKey"))?;

        Some(SpaceInfo::bounded(total, available))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boot_volume_reports_a_uuid() {
        // `NSURLVolumeUUIDStringKey` is a stringly-typed API: a typo returns `None`
        // forever, which looks like nothing at all while every volume silently
        // drops back to a path-derived ID. macOS always gives the boot volume a
        // UUID, so this is what proves the key still resolves.
        let uuid = get_volume_uuid_for_path("/").expect("macOS reports a UUID for the boot volume");
        assert!(!uuid.trim().is_empty(), "got: {uuid:?}");
    }

    #[test]
    fn a_path_that_does_not_exist_has_no_uuid() {
        assert_eq!(get_volume_uuid_for_path("/Volumes/definitely-not-mounted-xyz"), None);
    }

    #[test]
    fn the_boot_volume_reports_a_bounded_capacity_it_has_not_filled() {
        let space = get_volume_space("/").expect("macOS reports space for the boot volume");

        let SpaceInfo::Bounded {
            total_bytes,
            available_bytes,
            ..
        } = space
        else {
            panic!("a mounted filesystem always has a capacity, got {space:?}");
        };
        assert!(total_bytes > 0, "total should be positive");
        assert!(available_bytes > 0, "available should be positive");
        assert!(available_bytes <= total_bytes, "available should be at most total");
    }

    #[test]
    fn test_get_volume_space_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let space = get_volume_space(home.to_str().unwrap());
        assert!(space.is_some(), "Should get space info for home directory");
    }

    #[test]
    fn test_get_volume_space_nonexistent() {
        // Nonexistent paths return None - the NSURL resource API doesn't resolve to ancestor volumes
        let space = get_volume_space("/nonexistent/path/that/does/not/exist");
        assert!(space.is_none(), "Nonexistent paths should return None");
    }

    fn facts(ejectable: Option<bool>, internal: Option<bool>) -> MediaFacts {
        MediaFacts { ejectable, internal }
    }

    /// The drive that had no eject button: a Thunderbolt SSD is fixed media on an
    /// external bus, so its media flag says no while the disk unplugs like any other.
    #[test]
    fn an_external_disk_with_fixed_media_offers_eject() {
        assert!(offers_eject(facts(Some(false), Some(false))));
    }

    #[test]
    fn ejectable_media_offers_eject_wherever_it_sits() {
        // A USB stick, and a disk image or the built-in SD reader (both internal).
        assert!(offers_eject(facts(Some(true), Some(false))));
        assert!(offers_eject(facts(Some(true), Some(true))));
        assert!(offers_eject(facts(Some(true), None)));
    }

    #[test]
    fn an_internal_disk_with_fixed_media_offers_none() {
        assert!(!offers_eject(facts(Some(false), Some(true))));
    }

    /// Only an explicit "not internal" counts: a disk macOS wouldn't place must not
    /// grow a button that unmounts something nobody can plug back in.
    #[test]
    fn a_disk_macos_would_not_place_offers_none() {
        assert!(!offers_eject(facts(Some(false), None)));
        assert!(!offers_eject(facts(None, None)));
    }

    #[test]
    fn the_boot_volume_answers_both_media_keys_and_never_ejects() {
        use objc2_foundation::{NSString, NSURL};

        // Both keys are stringly typed: a typo answers `None` forever, which would
        // read as "no button" on every external fixed disk.
        let url = NSURL::fileURLWithPath(&NSString::from_str("/"));
        assert!(get_bool_resource(&url, "NSURLVolumeIsEjectableKey").is_some());
        assert!(get_bool_resource(&url, "NSURLVolumeIsInternalKey").is_some());
        assert!(!is_volume_ejectable(&url, "/"));
    }

    #[test]
    fn volume_name_from_path_uses_last_component() {
        assert_eq!(volume_name_from_path("/"), "Macintosh HD");
        assert_eq!(volume_name_from_path("/Volumes/My Backup"), "My Backup");
    }
}
