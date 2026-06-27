//! Detect disk-image-backed volumes (a mounted `.dmg`) via DiskArbitration.
//!
//! A mounted disk image is a transient, app-install-style volume: indexing it,
//! tracking its free space, or treating it as a copy target is meaningless. We
//! flag it on `LocationInfo::is_disk_image` so the UI can suppress the index
//! badge, the first-connect index prompt, and both free-space bars.
//!
//! The signal is DiskArbitration's `DADeviceModel` description value, which
//! reports "Disk Image" for any `hdiutil`-attached image regardless of its
//! filesystem (APFS/HFS) or read-only flag. Read-only is NOT a reliable proxy:
//! a writable APFS `.dmg` reports `is_read_only == false`.

/// Does a DiskArbitration `DADeviceModel` value denote a disk image?
///
/// DiskArbitration has no typed device-kind enum; the device model is an
/// IOKit string, and disk images carry the stable identifier "Disk Image"
/// (not localized user-facing copy). (verified on macOS 15.5, hdiutil-attached
/// APFS image: `DADeviceModel == "Disk Image"`, `DADeviceProtocol == "Virtual
/// Interface"`, 2026-06-27)
pub fn device_model_is_disk_image(model: &str) -> bool {
    model == "Disk Image"
}

/// Whether the volume mounted at `path` is backed by a disk image (`.dmg`).
///
/// Queries DiskArbitration synchronously (no run loop needed for a one-shot
/// description copy). Cheap relative to the per-volume NSURL/icon work in
/// `get_attached_volumes`. Call only for local mounts: `DADiskCreateFromVolumePath`
/// resolves the path, which can stall on a hung network mount.
#[cfg(target_os = "macos")]
pub fn is_disk_image_mount(path: &str) -> bool {
    use core_foundation::base::{CFType, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation::url::{CFURL, CFURLRef};
    use std::ffi::c_void;
    use std::path::Path;

    #[link(name = "DiskArbitration", kind = "framework")]
    unsafe extern "C" {
        fn DASessionCreate(allocator: *const c_void) -> *const c_void;
        fn DADiskCreateFromVolumePath(
            allocator: *const c_void,
            session: *const c_void,
            path: CFURLRef,
        ) -> *const c_void;
        fn DADiskCopyDescription(disk: *const c_void) -> *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
    }

    let Some(url) = CFURL::from_path(Path::new(path), true) else {
        return false;
    };

    // SAFETY: `DASessionCreate` returns a session under the Create rule (balanced by the
    // `_session` wrapper's drop). The session ref is passed live into `DADiskCreateFromVolumePath`,
    // whose disk result is likewise Create-rule (balanced by `_disk`). `url` is a live CFURL for
    // the call. `DADiskCopyDescription` returns a CFDictionary under the Create rule (balanced by
    // `_desc`). `CFDictionaryGetValue` reads under the Get rule: the value belongs to the dict, so
    // `wrap_under_get_rule` retains+releases its own reference. All three refs outlive their uses.
    unsafe {
        let session_ref = DASessionCreate(std::ptr::null());
        if session_ref.is_null() {
            return false;
        }
        let _session = CFType::wrap_under_create_rule(session_ref as CFTypeRef);

        let disk_ref = DADiskCreateFromVolumePath(std::ptr::null(), session_ref, url.as_concrete_TypeRef());
        if disk_ref.is_null() {
            return false;
        }
        let _disk = CFType::wrap_under_create_rule(disk_ref as CFTypeRef);

        let desc_ref = DADiskCopyDescription(disk_ref);
        if desc_ref.is_null() {
            return false;
        }
        let _desc = CFType::wrap_under_create_rule(desc_ref as CFTypeRef);

        let key = CFString::new("DADeviceModel");
        let value = CFDictionaryGetValue(desc_ref, key.as_concrete_TypeRef() as *const c_void);
        if value.is_null() {
            return false;
        }
        let model = CFString::wrap_under_get_rule(value as CFStringRef);
        device_model_is_disk_image(&model.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_disk_image_device_model() {
        assert!(device_model_is_disk_image("Disk Image"));
    }

    #[test]
    fn rejects_non_disk_image_device_models() {
        assert!(!device_model_is_disk_image("")); // no model reported
        assert!(!device_model_is_disk_image("APPLE SSD AP1024Z"));
        assert!(!device_model_is_disk_image("My Passport"));
        assert!(!device_model_is_disk_image("disk image")); // case-sensitive, exact
    }

    /// End-to-end FFI check against a real mounted disk image. Mounts a throwaway
    /// `.dmg` via `hdiutil`, so it's `#[ignore]` (run locally, not in CI):
    /// `cargo test -p cmdr is_disk_image_mount_detects_real_dmg -- --ignored`.
    #[test]
    #[ignore = "mounts a real DMG via hdiutil; run locally"]
    #[cfg(target_os = "macos")]
    fn is_disk_image_mount_detects_real_dmg() {
        use std::process::Command;

        let dir = std::env::temp_dir().join("cmdr-dmg-probe");
        let _ = std::fs::create_dir_all(&dir);
        let dmg = dir.join("probe.dmg");

        let create = Command::new("hdiutil")
            .args([
                "create",
                "-size",
                "8m",
                "-fs",
                "APFS",
                "-volname",
                "CmdrDmgProbe",
                "-ov",
            ])
            .arg(&dmg)
            .output()
            .expect("hdiutil create");
        assert!(create.status.success(), "hdiutil create failed");

        let attach = Command::new("hdiutil")
            .args(["attach", "-nobrowse"])
            .arg(&dmg)
            .output()
            .expect("hdiutil attach");
        assert!(attach.status.success(), "hdiutil attach failed");

        let mount_point = "/Volumes/CmdrDmgProbe";
        let detected = is_disk_image_mount(mount_point);

        // Always detach before asserting, so a failure doesn't leak the mount.
        let _ = Command::new("hdiutil").args(["detach", mount_point]).output();
        let _ = std::fs::remove_file(&dmg);

        assert!(detected, "expected {mount_point} to be detected as a disk image");
        // A non-disk-image path must read false.
        assert!(!is_disk_image_mount("/"), "root volume must not be a disk image");
    }
}
