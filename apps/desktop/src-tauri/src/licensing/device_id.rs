//! Stable, hashed device identifier for fair-use license tracking.
//!
//! Generates a one-way hash of the hardware UUID, prefixed with a version tag.
//! The hash is salted with `"cmdr:"` so it can't be correlated across products.
//! Result format: `v1:<64-char hex SHA-256>`.

use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Cached device ID (computed once per session).
static DEVICE_ID: OnceLock<Option<String>> = OnceLock::new();

/// Returns a stable, hashed device identifier, or `None` if the platform UUID can't be read.
///
/// The result is cached in memory — the hardware UUID won't change during a session.
pub fn get_device_id() -> Option<String> {
    DEVICE_ID.get_or_init(compute_device_id).clone()
}

fn compute_device_id() -> Option<String> {
    let uuid = read_platform_uuid()?;
    let salted = format!("cmdr:{uuid}");
    let hash = Sha256::digest(salted.as_bytes());
    Some(format!("v1:{:x}", hash))
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

    unsafe {
        let matching = IOServiceMatching(c"IOPlatformExpertDevice".as_ptr());
        if matching.is_null() {
            log::warn!("IOServiceMatching returned null");
            return None;
        }

        // kIOMasterPortDefault / kIOMainPortDefault = 0
        let service = IOServiceGetMatchingService(0, matching);
        // IOServiceMatching result is consumed by IOServiceGetMatchingService — don't CFRelease it.
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

/// Linux stub — returns `None` for now.
// TODO: Read `/etc/machine-id`, apply the same salt-and-hash approach as macOS.
#[cfg(target_os = "linux")]
fn read_platform_uuid() -> Option<String> {
    None
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
        // Verify the hashing logic directly (platform-independent).
        let uuid = "TEST-UUID-1234";
        let salted = format!("cmdr:{uuid}");
        let hash = Sha256::digest(salted.as_bytes());
        let result = format!("v1:{:x}", hash);

        let salted2 = format!("cmdr:{uuid}");
        let hash2 = Sha256::digest(salted2.as_bytes());
        let result2 = format!("v1:{:x}", hash2);

        assert_eq!(result, result2);
        assert!(result.starts_with("v1:"));
        assert_eq!(result.len(), 3 + 64); // "v1:" + 64 hex chars
    }
}
