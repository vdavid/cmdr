//! SMB share mounting using macOS NetFS.framework.
//!
//! Provides async mount operations with proper error handling and credential support.

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::ptr;

/// Result of a successful mount operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    /// For example, "/Volumes/Documents".
    pub mount_path: String,
    pub already_mounted: bool,
}

/// Errors that can occur during mount operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MountError {
    HostUnreachable {
        message: String,
    },
    ShareNotFound {
        message: String,
    },
    AuthRequired {
        message: String,
    },
    AuthFailed {
        message: String,
    },
    PermissionDenied {
        message: String,
    },
    Timeout {
        message: String,
    },
    Cancelled {
        message: String,
    },
    ProtocolError {
        message: String,
    },
    /// Path already exists but isn't a mountpoint.
    MountPathConflict {
        message: String,
    },
}

// NetFS.framework FFI declarations
// These are manually declared since NetFS isn't in standard Rust crates.
#[link(name = "NetFS", kind = "framework")]
unsafe extern "C" {
    /// Synchronous mount function (simpler for our use case with tokio spawn_blocking).
    fn NetFSMountURLSync(
        url: *const c_void,              // CFURLRef
        mountpath: *const c_void,        // CFURLRef - NULL for auto
        user: *const c_void,             // CFStringRef - NULL for URL creds
        passwd: *const c_void,           // CFStringRef - NULL for URL creds
        open_options: *const c_void,     // CFMutableDictionaryRef
        mount_options: *const c_void,    // CFMutableDictionaryRef
        mountpoints: *mut *const c_void, // CFArrayRef*
    ) -> i32;
}

/// Error codes from NetFS.framework
const ENETFSNOSHARESAVAIL: i32 = -5998;
const ENETFSNOAUTHMECHSUPP: i32 = -5997;
const ENETFSNOPROTOVERSSUPP: i32 = -5996;
const USER_CANCELLED_ERR: i32 = -128;
const ENOENT: i32 = 2;
const EEXIST: i32 = 17; // Share already mounted
const EACCES: i32 = 13;
const ETIMEDOUT: i32 = 60;
const ECONNREFUSED: i32 = 61;
const EHOSTUNREACH: i32 = 65;
const EAUTH: i32 = 80;

/// Map NetFS/POSIX error codes to user-friendly MountError.
/// Note: EEXIST (17) is handled specially in mount_share_sync, not here.
fn error_from_code(code: i32, share_name: &str, server_name: &str) -> MountError {
    match code {
        USER_CANCELLED_ERR => MountError::Cancelled {
            message: "Mount operation was cancelled".to_string(),
        },
        ENOENT => MountError::ShareNotFound {
            message: format!("Share \"{}\" not found on \"{}\"", share_name, server_name),
        },
        ENETFSNOSHARESAVAIL => MountError::ShareNotFound {
            message: format!("No shares available on \"{}\"", server_name),
        },
        EACCES | EAUTH => MountError::AuthFailed {
            message: "Invalid username or password".to_string(),
        },
        ENETFSNOAUTHMECHSUPP => MountError::AuthRequired {
            message: "Authentication required".to_string(),
        },
        ETIMEDOUT => MountError::Timeout {
            message: format!("Connection to \"{}\" timed out", server_name),
        },
        ECONNREFUSED | EHOSTUNREACH => MountError::HostUnreachable {
            message: format!("Can't connect to \"{}\"", server_name),
        },
        ENETFSNOPROTOVERSSUPP => MountError::ProtocolError {
            message: "Incompatible SMB protocol version".to_string(),
        },
        _ => MountError::ProtocolError {
            message: format!("Mount failed with error code {}", code),
        },
    }
}

/// Mount an SMB share to the local filesystem.
///
/// This is a synchronous function that should be called from a spawn_blocking context.
/// It uses NetFSMountURLSync which handles the mount operation synchronously.
/// NetFS automatically detects if the share is already mounted and returns the existing path.
///
/// # Arguments
/// * `server` - Server hostname or IP address
/// * `share` - Name of the share to mount
/// * `username` - Optional username for authentication
/// * `password` - Optional password for authentication
///
/// # Returns
/// * `Ok(MountResult)` - Mount successful, with path to mount point
/// * `Err(MountError)` - Mount failed with specific error type
pub fn mount_share_sync(
    server: &str,
    share: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) -> Result<MountResult, MountError> {
    // Build SMB URL: smb://server/share (with port for non-standard)
    let url_string = if port != 445 {
        format!("smb://{}:{}/{}", server, port, share)
    } else {
        format!("smb://{}/{}", server, share)
    };

    // Create URL from string using CFURLCreateWithString
    let cf_url_string = CFString::new(&url_string);
    let cf_url = unsafe {
        let url_ref =
            core_foundation::url::CFURLCreateWithString(ptr::null(), cf_url_string.as_concrete_TypeRef(), ptr::null());
        if url_ref.is_null() {
            return Err(MountError::ProtocolError {
                message: format!("Failed to create URL: {}", url_string),
            });
        }
        CFURL::wrap_under_create_rule(url_ref)
    };

    // Prepare credentials
    let cf_user = username.map(CFString::new);
    let cf_pass = password.map(CFString::new);

    // Prepare output array for mount points
    let mut mountpoints: *const c_void = ptr::null();

    // Call NetFSMountURLSync
    let result = unsafe {
        NetFSMountURLSync(
            cf_url.as_concrete_TypeRef() as *const c_void,
            ptr::null(), // NULL for auto mount path
            cf_user
                .as_ref()
                .map(|s| s.as_concrete_TypeRef() as *const c_void)
                .unwrap_or(ptr::null()),
            cf_pass
                .as_ref()
                .map(|s| s.as_concrete_TypeRef() as *const c_void)
                .unwrap_or(ptr::null()),
            ptr::null(), // No special open options
            ptr::null(), // No special mount options
            &mut mountpoints,
        )
    };

    // Check result
    if result != 0 && result != EEXIST {
        return Err(error_from_code(result, share, server));
    }

    let already_mounted = result == EEXIST;

    // Extract mount path from the mountpoints array. On both success (0) and
    // EEXIST (17), macOS may return the actual path (which can be disambiguated,
    // for example `/Volumes/public-1` when `/Volumes/public` is already taken by
    // a different server). Fall back to scanning /Volumes/ for the mount.
    let mount_path = extract_mount_path(mountpoints)
        .or_else(|| find_mount_path_for_share(server, share))
        .unwrap_or_else(|| format!("/Volumes/{}", share));

    Ok(MountResult {
        mount_path,
        already_mounted,
    })
}

/// Default mount timeout in milliseconds
const DEFAULT_MOUNT_TIMEOUT_MS: u64 = 20_000;

/// Async wrapper for mount_share_sync that runs in a blocking task with timeout.
pub async fn mount_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<MountResult, MountError> {
    let server_clone = server.clone();
    let timeout_duration = std::time::Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_MOUNT_TIMEOUT_MS));

    // Use timeout to prevent hanging indefinitely
    let mount_future = tokio::task::spawn_blocking(move || {
        mount_share_sync(&server, &share, username.as_deref(), password.as_deref(), port)
    });

    match tokio::time::timeout(timeout_duration, mount_future).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) => Err(MountError::ProtocolError {
            message: format!("Mount task failed: {}", join_error),
        }),
        Err(_timeout) => Err(MountError::Timeout {
            message: format!(
                "Connection to \"{}\" timed out after {} seconds",
                server_clone,
                timeout_duration.as_secs()
            ),
        }),
    }
}

/// Extracts the mount path from a `NetFSMountURLSync` mountpoints CFArray.
///
/// Returns `None` if the pointer is null or the array is empty.
fn extract_mount_path(mountpoints: *const c_void) -> Option<String> {
    if mountpoints.is_null() {
        return None;
    }
    unsafe {
        let array = mountpoints as core_foundation::array::CFArrayRef;
        let result = if core_foundation::array::CFArrayGetCount(array) > 0 {
            let path_ref = core_foundation::array::CFArrayGetValueAtIndex(array, 0);
            let cf_string = CFString::wrap_under_get_rule(path_ref as core_foundation::string::CFStringRef);
            Some(cf_string.to_string())
        } else {
            None
        };
        core_foundation::base::CFRelease(mountpoints);
        result
    }
}

/// Finds the mount path for a server+share by scanning `/Volumes/` with `statfs`.
///
/// Handles disambiguated paths: if `server` has share `public` but `/Volumes/public`
/// belongs to a different server, macOS may have mounted it at `/Volumes/public-1`.
/// This function finds the right one by checking each mount's source via `statfs`.
fn find_mount_path_for_share(server: &str, share: &str) -> Option<String> {
    use crate::volumes::get_smb_mount_info;

    let entries = std::fs::read_dir("/Volumes").ok()?;
    let server_lower = server.to_lowercase();

    for entry in entries.flatten() {
        let path = entry.path().to_string_lossy().to_string();
        // Check paths that start with the share name (for example, "public", "public-1")
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with(share) {
            continue;
        }
        if let Some(info) = get_smb_mount_info(&path)
            && info.server.to_lowercase() == server_lower
            && info.share == share
        {
            return Some(path);
        }
    }
    None
}

/// Unmounts all SMB shares mounted from a given server.
///
/// Iterates `/Volumes/`, uses `statfs` to find SMB mounts whose server matches
/// the given `server_name` or `server_ip`. Unmounts each via `diskutil unmount`.
/// Returns the list of mount paths that were successfully unmounted.
pub fn unmount_smb_shares_from_host(server_name: &str, server_ip: Option<&str>) -> Vec<String> {
    use crate::volumes::get_smb_mount_info;
    use std::fs;

    let mut unmounted = Vec::new();

    let Ok(entries) = fs::read_dir("/Volumes") else {
        return unmounted;
    };

    let server_name_lower = server_name.to_lowercase();

    for entry in entries.flatten() {
        let mount_path = entry.path().to_string_lossy().to_string();
        let Some(info) = get_smb_mount_info(&mount_path) else {
            continue;
        };

        let server_lower = info.server.to_lowercase();
        let matches =
            server_lower == server_name_lower || server_ip.is_some_and(|ip| server_lower == ip.to_lowercase());

        if !matches {
            continue;
        }

        log::info!("Unmounting SMB share at {}", mount_path);
        let output = std::process::Command::new("diskutil")
            .args(["unmount", &mount_path])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                log::info!("Unmounted {}", mount_path);
                unmounted.push(mount_path);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                log::warn!("Failed to unmount {}: {}", mount_path, stderr.trim());
            }
            Err(e) => {
                log::warn!("Failed to run diskutil unmount for {}: {}", mount_path, e);
            }
        }
    }

    unmounted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_from_code() {
        let err = error_from_code(USER_CANCELLED_ERR, "test", "server");
        match err {
            MountError::Cancelled { .. } => (),
            _ => panic!("Expected Cancelled error"),
        }

        let err = error_from_code(ENOENT, "Share1", "Server1");
        match err {
            MountError::ShareNotFound { message } => {
                assert!(message.contains("Share1"));
                assert!(message.contains("Server1"));
            }
            _ => panic!("Expected ShareNotFound error"),
        }

        let err = error_from_code(EAUTH, "test", "server");
        match err {
            MountError::AuthFailed { .. } => (),
            _ => panic!("Expected AuthFailed error"),
        }

        let err = error_from_code(EHOSTUNREACH, "test", "server");
        match err {
            MountError::HostUnreachable { .. } => (),
            _ => panic!("Expected HostUnreachable error"),
        }
    }

    #[test]
    fn test_timeout_constant() {
        // Verify default timeout is reasonable (10-60 seconds)
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS >= 10_000) };
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS <= 60_000) };
    }
}
