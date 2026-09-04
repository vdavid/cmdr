//! NSURL resource lookups (volume name, ejectable flag, capacities), the
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
pub(crate) fn get_bool_resource(url: &objc2_foundation::NSURL, key: &str) -> Option<bool> {
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

    #[test]
    fn volume_name_from_path_uses_last_component() {
        assert_eq!(volume_name_from_path("/"), "Macintosh HD");
        assert_eq!(volume_name_from_path("/Volumes/My Backup"), "My Backup");
    }
}
