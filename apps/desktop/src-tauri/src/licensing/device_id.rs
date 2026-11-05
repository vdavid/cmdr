//! Stable, hashed device identifier for fair-use license tracking.
//!
//! Generates a one-way hash of the platform id (`IOPlatformUUID` on macOS, `/etc/machine-id`
//! on Linux), prefixed with a version tag.
//! The hash is salted with `"cmdr:"` so it can't be correlated across products.
//! Result format: `v1:<64-char hex SHA-256>`.

use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Cached device ID (computed once per session).
static DEVICE_ID: OnceLock<Option<String>> = OnceLock::new();

/// Returns a stable, hashed device identifier, or `None` if the platform UUID can't be read.
///
/// The result is cached in memory: the hardware UUID won't change during a session.
pub fn get_device_id() -> Option<String> {
    DEVICE_ID.get_or_init(compute_device_id).clone()
}

fn compute_device_id() -> Option<String> {
    read_platform_uuid().map(|uuid| hash_platform_uuid(&uuid))
}

/// Salts and hashes a raw platform id into the wire format `v1:<64-char hex>`.
fn hash_platform_uuid(uuid: &str) -> String {
    let salted = format!("cmdr:{uuid}");
    let hash = Sha256::digest(salted.as_bytes());
    // sha2 0.11 returns a `hybrid_array::Array` newtype that doesn't impl `LowerHex`,
    // so format the bytes by hand.
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("v1:{hex}")
}

/// Read `IOPlatformUUID` from the IOKit registry via FFI.
#[cfg(target_os = "macos")]
fn read_platform_uuid() -> Option<String> {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceGetMatchingService(main_port: u32, matching: *const c_void) -> u32;
        fn IOServiceMatching(name: *const std::ffi::c_char) -> *const c_void;
        fn IORegistryEntryCreateCFProperty(
            entry: u32,
            key: CFStringRef,
            allocator: *const c_void,
            options: u32,
        ) -> *const c_void;
        fn IOObjectRelease(object: u32) -> i32;
    }

    // SAFETY: `c"IOPlatformExpertDevice"` is a valid NUL-terminated C string. The matching dict
    // from `IOServiceMatching` is consumed by `IOServiceGetMatchingService` (it takes ownership),
    // so we must NOT release it ourselves. The returned `service` registry-entry handle is balanced
    // by `IOObjectRelease`. `IORegistryEntryCreateCFProperty` returns the value under the Create
    // rule, null-checked, then handed to `wrap_under_create_rule`, which takes that single ownership
    // and releases it once on drop. `key` is a live CFString for the call.
    unsafe {
        let matching = IOServiceMatching(c"IOPlatformExpertDevice".as_ptr());
        if matching.is_null() {
            log::warn!("IOServiceMatching returned null");
            return None;
        }

        // kIOMasterPortDefault / kIOMainPortDefault = 0
        let service = IOServiceGetMatchingService(0, matching);
        // IOServiceMatching result is consumed by IOServiceGetMatchingService: don't CFRelease it.
        if service == 0 {
            log::warn!("IOServiceGetMatchingService found no platform expert");
            return None;
        }

        let key = CFString::new("IOPlatformUUID");
        let cf_value = IORegistryEntryCreateCFProperty(service, key.as_concrete_TypeRef(), std::ptr::null(), 0);
        IOObjectRelease(service);

        if cf_value.is_null() {
            log::warn!("IORegistryEntryCreateCFProperty returned null for IOPlatformUUID");
            return None;
        }

        let cf_string = CFString::wrap_under_create_rule(cf_value as CFStringRef);
        Some(cf_string.to_string())
    }
}

/// Read the systemd machine id, falling back to the D-Bus copy on systems without systemd.
///
/// It changes on reinstall and is shared by cloned VMs, so those count as new or the same
/// device respectively. Accepted (David, 2026-11-05, #980).
#[cfg(target_os = "linux")]
fn read_platform_uuid() -> Option<String> {
    use std::path::Path;
    let id = read_machine_id(&[Path::new("/etc/machine-id"), Path::new("/var/lib/dbus/machine-id")]);
    if id.is_none() {
        log::warn!("No usable machine-id found; license checks will carry no device id");
    }
    id
}

/// Returns the first usable machine id among `paths`, in order. Unreadable files are skipped.
#[cfg(any(target_os = "linux", test))]
fn read_machine_id(paths: &[&std::path::Path]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok().and_then(|s| parse_machine_id(&s)))
}

/// Trims a machine-id file's contents; empty or `uninitialized` (machine-id(5)) means none.
#[cfg(any(target_os = "linux", test))]
fn parse_machine_id(contents: &str) -> Option<String> {
    let id = contents.trim();
    (!id.is_empty() && id != "uninitialized").then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn returns_some_on_macos() {
        let id = get_device_id();
        assert!(id.is_some(), "get_device_id() should return Some on macOS");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn matches_expected_format() {
        let id = get_device_id().expect("should return Some on macOS");
        assert!(id.starts_with("v1:"), "should start with 'v1:' prefix, got: {id}");
        let hex_part = &id[3..];
        assert_eq!(
            hex_part.len(),
            64,
            "hex part should be 64 chars, got: {}",
            hex_part.len()
        );
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "hex part should be lowercase hex, got: {hex_part}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn returns_stable_value() {
        let first = get_device_id();
        let second = get_device_id();
        assert_eq!(
            first, second,
            "get_device_id() should return the same value on repeated calls"
        );
    }

    #[test]
    fn hash_is_deterministic() {
        let result = hash_platform_uuid("TEST-UUID-1234");
        assert_eq!(result, hash_platform_uuid("TEST-UUID-1234"));
        assert!(result.starts_with("v1:"));
        assert_eq!(result.len(), 3 + 64); // "v1:" + 64 hex chars
        assert_ne!(result, hash_platform_uuid("OTHER-UUID"));
    }

    #[test]
    fn hash_salts_with_the_product_prefix() {
        // Regression anchor: changing the salt or format re-keys every device on the server.
        let hex: String = Sha256::digest(b"cmdr:abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash_platform_uuid("abc"), format!("v1:{hex}"));
    }

    const MACHINE_ID: &str = "4c4c4544003a3110804ac2c04f483532";

    #[test]
    fn machine_id_is_trimmed() {
        assert_eq!(
            parse_machine_id(&format!("{MACHINE_ID}\n")),
            Some(MACHINE_ID.to_owned())
        );
        assert_eq!(
            parse_machine_id(&format!("  {MACHINE_ID} \r\n")),
            Some(MACHINE_ID.to_owned())
        );
    }

    #[test]
    fn empty_or_uninitialized_machine_id_is_none() {
        assert_eq!(parse_machine_id(""), None);
        assert_eq!(parse_machine_id("  \n"), None);
        // systemd writes this on a first boot before the id is committed (machine-id(5)).
        assert_eq!(parse_machine_id("uninitialized\n"), None);
    }

    #[test]
    fn reads_the_first_usable_machine_id_file() {
        let dir = crate::test_support::TestDir::new("machine_id");
        let etc = dir.join("etc-machine-id");
        let dbus = dir.join("dbus-machine-id");
        std::fs::write(&dbus, "dbusid\n").expect("write dbus machine-id");

        // Primary missing: falls back.
        assert_eq!(read_machine_id(&[&etc, &dbus]), Some("dbusid".to_owned()));

        // Primary empty: falls back.
        std::fs::write(&etc, "\n").expect("write empty machine-id");
        assert_eq!(read_machine_id(&[&etc, &dbus]), Some("dbusid".to_owned()));

        // Primary present: wins.
        std::fs::write(&etc, format!("{MACHINE_ID}\n")).expect("write machine-id");
        assert_eq!(read_machine_id(&[&etc, &dbus]), Some(MACHINE_ID.to_owned()));
    }

    #[test]
    fn no_machine_id_file_is_none() {
        let dir = crate::test_support::TestDir::new("machine_id_missing");
        assert_eq!(read_machine_id(&[&dir.join("a"), &dir.join("b")]), None);
    }
}
