//! NSURL resource lookups (volume name, ejectability), the path-derived
//! volume-name fallback, and per-path icon fetching. The blocking macOS
//! enrichment layer, only run for local mounts. The resource readers and the
//! volume-space query are the local backend's (`cmdr_local::{nsurl, space}`).

use cmdr_local::nsurl::{get_bool_resource, get_string_resource};
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
