//! SMB share mounting using macOS NetFS.framework.
//!
//! Provides async mount operations with proper error handling and credential support.
//!
//! ## Credential handling: why we pass creds explicitly to NetFS
//!
//! `NetFSMountURLSync` accepts `user`, `passwd`, and an `openOptions` CFDictionary.
//! When `user`/`passwd` are both `NULL` and `openOptions` doesn't say otherwise,
//! NetFS falls back to looking up credentials in the system Keychain. If the lookup
//! misses (fresh host, fresh Docker container, brand-new NAS), the kernel `smbfs`
//! kext pops a credential dialog with the current OS user prefilled. That dialog
//! steals focus, blocks the caller, and looks like the app has frozen.
//!
//! Cmdr already collects credentials (or "guest") in the frontend. We pass them
//! down so NetFS never reaches the Keychain fallback:
//!
//! - **Credentialed mount**: build CFStrings from the supplied user + password and pass them as
//!   `user`/`passwd`. NetFS uses them directly.
//! - **Guest mount**: set `kNetFSUseGuestKey` (literal key `"Guest"`) to `kCFBooleanTrue` in
//!   `openOptions`. NetFS skips the Keychain and authenticates as guest. `user`/`passwd` stay
//!   `NULL` in this case, per Apple's NetFS docs.
//!
//! The constant `kNetFSUseGuestKey` is a `#define` in `<NetFS/NetFS.h>` (not an
//! exported symbol), so we recreate the CFString from the literal `"Guest"` at the
//! call site rather than linking to an `extern "C"` static.
//!
//! On top of that, every mount sets `UIOption = NoUI` (`kNAUIOptionKey = kNAUIOptionNoUI`):
//! even with explicit credentials, NetFS hands auth *failures* to NetAuthAgent, which
//! shows a system dialog and returns `kNetAuthErrorInternal` (-6600) when dismissed.
//! With `NoUI`, failures come back immediately as typed error codes and Cmdr renders
//! its own login flow. See `open_option_entries`.

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::ptr;

/// Result of a successful mount operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    /// For example, "/Volumes/Documents".
    pub mount_path: String,
    pub already_mounted: bool,
}

/// Errors that can occur during mount operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
/// NetAuth error codes (NetAuthAgent), documented in the comment block of `<NetFS/NetFS.h>`.
/// `kNetAuthErrorInternal` is what `NetFSMountURLSync` returns when authentication fails,
/// for example a guest mount against a creds-required server.
const KNETAUTH_ERROR_INTERNAL: i32 = -6600;
const KNETAUTH_ERROR_MOUNT_FAILED: i32 = -6602;
const KNETAUTH_ERROR_NO_SHARES_AVAILABLE: i32 = -6003;
const KNETAUTH_ERROR_GUEST_NOT_SUPPORTED: i32 = -6004;

/// Value of a NetFS `openOptions` entry.
#[derive(Debug, PartialEq)]
enum OpenOptionValue {
    /// `kCFBooleanTrue`
    True,
    /// A CFString value.
    Str(&'static str),
}

/// Decides which entries go into the NetFS `openOptions` dictionary.
///
/// `UIOption = NoUI` (`kNAUIOptionKey = kNAUIOptionNoUI`) is ALWAYS set: Cmdr owns all
/// auth UI. Without it, NetFS hands auth failures to NetAuthAgent, which shows a system
/// dialog ("You entered an invalid username or password...") on top of Cmdr, blocks the
/// mount call while it's open, and then returns `kNetAuthErrorInternal` (-6600). With
/// `NoUI`, the same failure comes back immediately as a typed error code that we map in
/// `error_from_code` and render in our own login flow.
///
/// All three keys (`UIOption`, `Guest`, `ForceNewSession`) are `#define`s in
/// `<NetFS/NetFS.h>`, not exported symbols, so the caller recreates CFStrings from these
/// literals rather than linking `extern "C"` statics.
fn open_option_entries(want_guest: bool, want_force_new_session: bool) -> Vec<(&'static str, OpenOptionValue)> {
    let mut entries = vec![("UIOption", OpenOptionValue::Str("NoUI"))];
    if want_guest {
        entries.push(("Guest", OpenOptionValue::True));
    }
    if want_force_new_session {
        entries.push(("ForceNewSession", OpenOptionValue::True));
    }
    entries
}

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
        EACCES | EAUTH | KNETAUTH_ERROR_INTERNAL => MountError::AuthFailed {
            message: "Invalid username or password".to_string(),
        },
        ENETFSNOAUTHMECHSUPP => MountError::AuthRequired {
            message: "Authentication required".to_string(),
        },
        KNETAUTH_ERROR_GUEST_NOT_SUPPORTED => MountError::AuthRequired {
            message: format!("\"{}\" doesn't allow guest access. Sign in to connect.", server_name),
        },
        KNETAUTH_ERROR_NO_SHARES_AVAILABLE => MountError::ShareNotFound {
            message: format!("No shares available on \"{}\"", server_name),
        },
        KNETAUTH_ERROR_MOUNT_FAILED => MountError::ProtocolError {
            message: format!("\"{}\" refused to mount \"{}\"", server_name, share_name),
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
    // If this exact share (same server identity + port) is already mounted, return it
    // directly instead of going through NetFS. The identity check matters: the existing
    // mount may be keyed by a different name for the same server (mDNS service name vs
    // IP), in which case a second NetFS call would "disambiguate" into mounting a
    // doomed second copy with a fresh session instead of reusing this one.
    if let Some(existing) = find_mount_path_for_share(server, share, port) {
        return Ok(MountResult {
            mount_path: existing,
            already_mounted: true,
        });
    }

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

    // Check if the default mount path is already taken by a different server.
    // If so, pick a disambiguated path (public-1, public-2, ...) like Finder does.
    let explicit_mount_path = disambiguated_mount_path(server, share, port);

    // Build openOptions. `open_option_entries` decides the content:
    //   - `UIOption = NoUI`, always: Cmdr owns all auth UI; NetAuthAgent must never pop
    //     a system dialog (see the helper's doc comment).
    //   - `Guest = true` for guest mounts (no credentials): NetFS authenticates as guest
    //     without consulting the Keychain.
    //   - `ForceNewSession = true` when disambiguating against an existing same-name
    //     mount: macOS opens a fresh SMB session instead of reusing the existing one
    //     (different server, so the existing session would be wrong).
    let want_guest = cf_user.is_none() && cf_pass.is_none();
    let want_force_new_session = explicit_mount_path.is_some();
    let entries = open_option_entries(want_guest, want_force_new_session);
    let open_options = unsafe {
        let dict = core_foundation::dictionary::CFDictionaryCreateMutable(
            ptr::null(),
            0, // no capacity limit
            &core_foundation::dictionary::kCFTypeDictionaryKeyCallBacks,
            &core_foundation::dictionary::kCFTypeDictionaryValueCallBacks,
        );
        for (key, value) in &entries {
            // The dictionary retains keys and values (kCFTypeDictionary*CallBacks), so
            // dropping the temporary CFStrings after SetValue is fine.
            let cf_key = CFString::new(key);
            match value {
                OpenOptionValue::True => core_foundation::dictionary::CFDictionarySetValue(
                    dict,
                    cf_key.as_concrete_TypeRef() as *const c_void,
                    core_foundation::boolean::kCFBooleanTrue as *const c_void,
                ),
                OpenOptionValue::Str(s) => {
                    let cf_value = CFString::new(s);
                    core_foundation::dictionary::CFDictionarySetValue(
                        dict,
                        cf_key.as_concrete_TypeRef() as *const c_void,
                        cf_value.as_concrete_TypeRef() as *const c_void,
                    );
                }
            }
        }
        dict as *const c_void
    };

    // Prepare output array for mount points
    let mut mountpoints: *const c_void = ptr::null();

    // Call NetFSMountURLSync. Mount path is NULL even when disambiguating;
    // NetFS auto-creates the mount point in /Volumes/ (we can't mkdir there).
    // With `ForceNewSession`, NetFS treats this as a separate server and picks
    // a disambiguated name (public-1, public-2, etc.) automatically.
    // With `Guest`, NetFS authenticates as guest without consulting Keychain.
    let result = unsafe {
        NetFSMountURLSync(
            cf_url.as_concrete_TypeRef() as *const c_void,
            ptr::null(), // Let NetFS choose/create the mount point
            cf_user
                .as_ref()
                .map(|s| s.as_concrete_TypeRef() as *const c_void)
                .unwrap_or(ptr::null()),
            cf_pass
                .as_ref()
                .map(|s| s.as_concrete_TypeRef() as *const c_void)
                .unwrap_or(ptr::null()),
            open_options,
            ptr::null(), // No special mount options
            &mut mountpoints,
        )
    };

    // Release open options dictionary if we created one
    if !open_options.is_null() {
        unsafe { core_foundation::base::CFRelease(open_options) };
    }

    // Check result
    if result != 0 && result != EEXIST {
        return Err(error_from_code(result, share, server));
    }

    let already_mounted = result == EEXIST;

    // Extract mount path from the mountpoints array. On both success (0) and
    // EEXIST (17), macOS may return the actual path (which can be disambiguated,
    // for example `/Volumes/public-1` when `/Volumes/public` is already taken by
    // a different server). Fall back to scanning /Volumes/ for the mount.
    // Prefer: explicit path we chose → NetFS output → /Volumes/ scan → hardcoded fallback.
    // The explicit path is most reliable because we already validated it.
    let mount_path = explicit_mount_path
        .or_else(|| extract_mount_path(mountpoints))
        .or_else(|| find_mount_path_for_share(server, share, port))
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

/// Returns a disambiguated mount path if `/Volumes/{share}` is already taken by a
/// different server. Returns `None` if the default path is available or already
/// belongs to this server (EEXIST case).
///
/// Follows Finder's convention: `public-1`, `public-2`, etc.
fn disambiguated_mount_path(server: &str, share: &str, port: u16) -> Option<String> {
    use crate::volumes::get_smb_mount_info;

    let default_path = format!("/Volumes/{}", share);
    if !std::path::Path::new(&default_path).exists() {
        return None; // Default path is free
    }

    // Check if the existing mount is from the same server+port. Identity-aware: the
    // mount source may name the server differently than we do (mDNS service name vs
    // IP), and a string mismatch here would force a second mount of the same share.
    if let Some(info) = get_smb_mount_info(&default_path)
        && crate::network::server_identity::same_server_live(&info.server, server)
        && info.share == share
        && info.port == port
    {
        return None; // Same server: let NetFS handle EEXIST
    }

    // Collision: find the next available suffix
    for n in 1..100 {
        let candidate = format!("/Volumes/{}-{}", share, n);
        if !std::path::Path::new(&candidate).exists() {
            log::info!(
                "Mount path /Volumes/{} taken by another server, using {}",
                share,
                candidate
            );
            return Some(candidate);
        }
        // If this suffixed path exists and belongs to this server, reuse it
        if let Some(info) = get_smb_mount_info(&candidate)
            && crate::network::server_identity::same_server_live(&info.server, server)
            && info.share == share
            && info.port == port
        {
            return Some(candidate); // Already mounted here
        }
    }

    None // Give up after 100 attempts, let NetFS handle it
}

/// Finds the mount path for a server+share+port by scanning `/Volumes/` with `statfs`.
///
/// Handles disambiguated paths: if `server` has share `public` but `/Volumes/public`
/// belongs to a different server, macOS may have mounted it at `/Volumes/public-1`.
/// This function finds the right one by checking each mount's source via `statfs`,
/// comparing servers by identity (mDNS name ↔ IP), not by string. The port check keeps
/// same-named shares on different ports apart (Docker test containers on `localhost`).
fn find_mount_path_for_share(server: &str, share: &str, port: u16) -> Option<String> {
    use crate::volumes::get_smb_mount_info;

    let entries = std::fs::read_dir("/Volumes").ok()?;

    for entry in entries.flatten() {
        let path = entry.path().to_string_lossy().to_string();
        // Check paths that start with the share name (for example, "public", "public-1")
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with(share) {
            continue;
        }
        if let Some(info) = get_smb_mount_info(&path)
            && crate::network::server_identity::same_server_live(&info.server, server)
            && info.share == share
            && info.port == port
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
                // allowed-error-string-match: testing Display content of MountError::ShareNotFound message field
                assert!(message.contains("Share1"));
                // allowed-error-string-match: testing Display content of MountError::ShareNotFound message field
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

    /// NetAuth error codes (NetAuthAgent, documented in `<NetFS/NetFS.h>`) must map to
    /// typed errors, not the opaque `ProtocolError` catch-all. -6600 is what
    /// `NetFSMountURLSync` returns when authentication fails (observed in the wild with
    /// a guest mount against a creds-required NAS); routing it to `AuthFailed` is what
    /// lets the frontend offer the login form instead of a dead-end error pane.
    #[test]
    fn test_netauth_error_codes() {
        let err = error_from_code(-6600, "naspi", "naspolya");
        assert!(
            matches!(err, MountError::AuthFailed { .. }),
            "kNetAuthErrorInternal (-6600) should be AuthFailed, got {:?}",
            err
        );

        let err = error_from_code(-6004, "naspi", "naspolya");
        assert!(
            matches!(err, MountError::AuthRequired { .. }),
            "kNetAuthErrorGuestNotSupported (-6004) should be AuthRequired, got {:?}",
            err
        );

        let err = error_from_code(-6003, "naspi", "naspolya");
        assert!(
            matches!(err, MountError::ShareNotFound { .. }),
            "kNetAuthErrorNoSharesAvailable (-6003) should be ShareNotFound, got {:?}",
            err
        );

        // kNetAuthErrorMountFailed means auth SUCCEEDED but the mount step failed, so it
        // must NOT map to an auth-class error (that would loop the user into a pointless
        // login form). It stays a ProtocolError, just with a readable message.
        let err = error_from_code(-6602, "naspi", "naspolya");
        assert!(
            matches!(err, MountError::ProtocolError { .. }),
            "kNetAuthErrorMountFailed (-6602) should stay ProtocolError, got {:?}",
            err
        );
    }

    /// `UIOption = NoUI` must be set on EVERY mount, regardless of guest/credentialed
    /// mode. Without it, NetFS hands auth failures to NetAuthAgent, which pops a system
    /// dialog ("You entered an invalid username or password...") on top of Cmdr and then
    /// returns `kNetAuthErrorInternal`. Cmdr owns all auth UI.
    #[test]
    fn test_open_options_always_suppress_system_ui() {
        for (guest, force_new_session) in [(false, false), (true, false), (false, true), (true, true)] {
            let entries = open_option_entries(guest, force_new_session);
            assert!(
                entries.contains(&("UIOption", OpenOptionValue::Str("NoUI"))),
                "UIOption=NoUI missing for guest={guest}, force_new_session={force_new_session}: {entries:?}"
            );
            assert_eq!(
                entries.iter().any(|(key, _)| *key == "Guest"),
                guest,
                "Guest key presence should match guest={guest}"
            );
            assert_eq!(
                entries.iter().any(|(key, _)| *key == "ForceNewSession"),
                force_new_session,
                "ForceNewSession key presence should match force_new_session={force_new_session}"
            );
        }
    }

    #[test]
    fn test_timeout_constant() {
        // Verify default timeout is reasonable (10-60 seconds)
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS >= 10_000) };
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS <= 60_000) };
    }

    /// Regression test for the macOS NetFS guest-mount credential dialog.
    ///
    /// Asserts a guest mount completes within a tight wall-clock budget. A
    /// blocking kernel `smbfs` prompt waits for user input indefinitely, so a
    /// sub-budget completion is the proxy for "no dialog appeared." Gated to
    /// macOS because Linux uses gvfs, which has neither the dialog nor this
    /// mount path.
    ///
    /// We don't add a paired auth-success / auth-failure test here because
    /// NetFS caches SMB sessions across calls — once `testuser`+`testpass`
    /// authenticates once, subsequent calls (even with wrong creds) ride the
    /// cached session, so a tight harness can't reliably distinguish "creds
    /// passed correctly" from "session reused" without forcibly tearing down
    /// the session. The guest path is what regressed in real use and is what
    /// this test guards. Manual end-to-end coverage for the auth path runs
    /// via `pnpm dev` against the same Docker containers.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_mount_guest_no_dialog() {
        use std::time::{Duration, Instant};

        let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10480);
        // Use `localhost` rather than `127.0.0.1`: NetFS itself handles either,
        // but the wider SMB test harness uses `localhost` to dodge the smbutil
        // loopback quirk on non-standard ports.
        let host = "localhost".to_string();

        // Pre-clean any stale mount from a previous run so we exercise the
        // real first-mount path (the one that pops the dialog when broken).
        let _ = std::process::Command::new("diskutil")
            .args(["unmount", "force", "/Volumes/public"])
            .output();

        // 10 s budget: a real credential dialog blocks the call indefinitely,
        // so this picks up the regression even under cold Docker startup.
        let budget = Duration::from_secs(10);
        let start = Instant::now();
        let result = mount_share(host.clone(), "public".to_string(), None, None, port, Some(8_000)).await;
        let elapsed = start.elapsed();

        // Always try to unmount so a successful mount doesn't linger between runs.
        if let Ok(ref ok) = result {
            let _ = std::process::Command::new("diskutil")
                .args(["unmount", "force", &ok.mount_path])
                .output();
        }

        assert!(
            elapsed < budget,
            "guest mount took {:?} (budget {:?}); a credential dialog probably blocked NetFS",
            elapsed,
            budget
        );
        let mount_result = result.unwrap_or_else(|e| panic!("guest mount against {host}:{port} failed: {e:?}"));
        assert!(
            mount_result.mount_path.starts_with("/Volumes/"),
            "expected /Volumes/* mount path, got {}",
            mount_result.mount_path
        );
    }

    /// Regression test for the SMB volume-ID-per-mount fix.
    ///
    /// `path_to_id` lowercases the mount path, so two SMB shares with the same
    /// case-folded name on different servers (a NAS sharing `Public`, a Docker
    /// container sharing `public`) used to collide on `volumespublic`. The
    /// collision cross-contaminated `lastUsedPaths` and tab state and surfaced
    /// as wrong-case paths flowing into `SmbVolume::list_directory`, producing
    /// `STATUS_OBJECT_PATH_NOT_FOUND` from the server. After the fix, the ID
    /// is keyed by `(server, port, share)`, so two same-named shares on
    /// different ports/hosts must produce distinct IDs.
    ///
    /// Exercises the real OS-mount → `resolve_path_volume_fast` path against
    /// the Docker guest container, then asserts the resulting volume ID is in
    /// the new `smb-…-…-…` shape rather than the legacy path-shape form.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_volume_id_is_per_mount_not_per_path_shape() {
        use std::time::Duration;

        let port: u16 = std::env::var("SMB_CONSUMER_GUEST_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10480);
        let host = "localhost".to_string();

        // Pre-clean to exercise the cold mount path.
        let _ = std::process::Command::new("diskutil")
            .args(["unmount", "force", "/Volumes/public"])
            .output();

        let mount_result = mount_share(host.clone(), "public".to_string(), None, None, port, Some(8_000))
            .await
            .unwrap_or_else(|e| panic!("guest mount against {host}:{port} failed: {e:?}"));

        // Poll for NetFS to register the mount so statfs reports the SMB info. A
        // fixed sleep here raced the OS settling and flaked in BOTH debug and
        // release (the magic-timer-wait anti-pattern — see docs/testing.md). We
        // wait for the settled, SMB-shaped id: an early statfs can briefly report
        // the path-shape id (`volumespublic`) before the SMB mount info lands.
        // The ceiling is generous (20s) because NetFS settle time stretches under
        // the parallel load of the full slow-check suite (Linux tests + both e2e
        // lanes running concurrently); the early break keeps the common case fast,
        // so the budget only ever elapses on a genuine failure.
        let mut volume = None;
        for _ in 0..200 {
            if let Some(v) = crate::volumes::resolve_path_volume_fast(&mount_result.mount_path)
                && v.id.starts_with("smb-")
            {
                volume = Some(v);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Unmount before assertions so a panic doesn't leak the mount.
        let _ = std::process::Command::new("diskutil")
            .args(["unmount", "force", &mount_result.mount_path])
            .output();

        let volume =
            volume.expect("resolve_path_volume_fast should return an smb- volume within 20s of a fresh SMB mount");

        // The pre-fix ID was `volumespublic`, which is what `path_to_id` produces
        // for `/Volumes/public`. The new ID encodes server, port, and share.
        assert_ne!(
            volume.id, "volumespublic",
            "expected SMB-shaped ID, got the path-shape one (regression)"
        );
        assert!(
            volume.id.starts_with("smb-"),
            "expected SMB-shaped ID (smb-...), got {}",
            volume.id
        );
        assert!(
            volume.id.contains(&format!("-{port}-")),
            "expected ID to embed the port ({port}); got {}",
            volume.id
        );
        assert!(
            volume.id.ends_with("-public"),
            "expected ID to end with the share name; got {}",
            volume.id
        );
    }
}
