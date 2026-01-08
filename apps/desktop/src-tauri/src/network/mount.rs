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
    /// Path where the share was mounted (e.g., "/Volumes/Documents")
    pub mount_path: String,
    /// Whether the share was already mounted (we didn't mount it ourselves)
    pub already_mounted: bool,
}

/// Errors that can occur during mount operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MountError {
    /// Host is unreachable
    HostUnreachable { message: String },
    /// Share not found on the server
    ShareNotFound { message: String },
    /// Authentication required (credentials needed)
    AuthRequired { message: String },
    /// Authentication failed (wrong credentials)
    AuthFailed { message: String },
    /// Permission denied
    PermissionDenied { message: String },
    /// Operation timed out
    Timeout { message: String },
    /// User cancelled the operation
    Cancelled { message: String },
    /// General protocol/network error
    ProtocolError { message: String },
    /// Mount path already exists but isn't a mountpoint
    MountPathConflict { message: String },
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
) -> Result<MountResult, MountError> {
    // Build SMB URL: smb://server/share
    let url_string = format!("smb://{}/{}", server, share);

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
    // EEXIST (17) means the share is already mounted - this is not an error
    if result == EEXIST {
        // Share is already mounted, return success with expected path
        return Ok(MountResult {
            mount_path: format!("/Volumes/{}", share),
            already_mounted: true,
        });
    }

    if result != 0 {
        return Err(error_from_code(result, share, server));
    }

    // Extract mount path from result
    let mount_path = if !mountpoints.is_null() {
        unsafe {
            // mountpoints is a CFArray of CFStrings
            let array = mountpoints as core_foundation::array::CFArrayRef;
            if core_foundation::array::CFArrayGetCount(array) > 0 {
                let path_ref = core_foundation::array::CFArrayGetValueAtIndex(array, 0);
                let cf_string = CFString::wrap_under_get_rule(path_ref as core_foundation::string::CFStringRef);
                let path = cf_string.to_string();
                // Release the array
                core_foundation::base::CFRelease(mountpoints);
                path
            } else {
                // Release the array even if empty
                core_foundation::base::CFRelease(mountpoints);
                // Fall back to expected path
                format!("/Volumes/{}", share)
            }
        }
    } else {
        // No mount points returned, use expected path
        format!("/Volumes/{}", share)
    };

    Ok(MountResult {
        mount_path,
        already_mounted: false,
    })
}

/// Mount timeout in seconds
const MOUNT_TIMEOUT_SECS: u64 = 20;

/// Async wrapper for mount_share_sync that runs in a blocking task with timeout.
pub async fn mount_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<MountResult, MountError> {
    let server_clone = server.clone();

    // Use timeout to prevent hanging indefinitely
    let mount_future = tokio::task::spawn_blocking(move || {
        mount_share_sync(&server, &share, username.as_deref(), password.as_deref())
    });

    match tokio::time::timeout(std::time::Duration::from_secs(MOUNT_TIMEOUT_SECS), mount_future).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) => Err(MountError::ProtocolError {
            message: format!("Mount task failed: {}", join_error),
        }),
        Err(_timeout) => Err(MountError::Timeout {
            message: format!(
                "Connection to \"{}\" timed out after {} seconds",
                server_clone, MOUNT_TIMEOUT_SECS
            ),
        }),
    }
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
        // Verify timeout is reasonable (10-60 seconds)
        assert!(MOUNT_TIMEOUT_SECS >= 10);
        assert!(MOUNT_TIMEOUT_SECS <= 60);
    }
}
