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

/// Builds the `smb://` URL string `CFURLCreateWithString` accepts for a mount.
///
/// `CFURLCreateWithString` PARSES, it never escapes: hand it a string that isn't
/// already a valid RFC 3986 URL and it returns NULL, which is why a share named
/// `café` or `公開` couldn't be mounted at all. So the two data halves (server and
/// share) are percent-encoded here and the structure (scheme, `//`, the port colon,
/// the share separator) is assembled around them. ❌ Escaping the finished URL
/// string instead would eat the separators.
///
/// The escape set is RFC 3986's `unreserved` (`urlencoding::encode` keeps
/// `A-Za-z0-9-._~` and escapes the rest). Over-escaping is safe here — a reader
/// decodes back to the same bytes — while under-escaping is not: an unescaped `%`
/// in a share named `100%` reads as a truncated escape and the whole URL is
/// rejected, and a `#` or `?` would silently cut the name short.
///
/// **NFC first, for both halves.** macOS hands out decomposed (NFD) strings while
/// SMB servers store and answer with composed (NFC) ones, so one visible name is
/// two byte strings and two different escapes; the server only recognizes the NFC
/// one. Same normalization `cmdr_smb::volume::paths` applies to every path it
/// sends, applied here for the same reason.
///
/// An IPv6 literal is the one host that must not be escaped: it goes in brackets so
/// its colons can't be read as the port separator. mDNS hands us one whenever a host
/// advertises no IPv4 address (`mdns_discovery::extract_preferred_ip`).
fn build_smb_mount_url(server: &str, share: &str, port: u16) -> String {
    use unicode_normalization::UnicodeNormalization;

    let host = encode_url_host(server);
    let share: String = share.nfc().collect();
    let share = urlencoding::encode(&share);

    if port != 445 {
        format!("smb://{}:{}/{}", host, port, share)
    } else {
        format!("smb://{}/{}", host, share)
    }
}

/// Whether two spellings name the same share.
///
/// Compared on NFC, because the two sides reach us through different pipes and can
/// disagree on normalization for the same visible name: one comes from the server's
/// share list, the other from `statfs` or a `/Volumes` entry, and macOS APIs hand out
/// decomposed strings wherever the composed one wasn't what got written. A byte
/// compare splits the two spellings of `café` and reports a mounted share as
/// unmounted.
fn same_share_name(a: &str, b: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    a.nfc().eq(b.nfc())
}

/// Whether two spellings name the same server.
///
/// Case-insensitive because DNS is, and NFC-folded for the same cross-pipe reason
/// as [`same_share_name`]: one side is the name the caller was given (discovered or
/// typed), the other comes back from `statfs`, and macOS spells an accented name
/// decomposed there. A byte compare walks past that server's mounts.
fn same_server_name(a: &str, b: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    a.nfc()
        .flat_map(char::to_lowercase)
        .eq(b.nfc().flat_map(char::to_lowercase))
}

/// Renders `server` as a URL authority host: an IPv6 literal in brackets, anything
/// else NFC-normalized and percent-encoded. See [`build_smb_mount_url`].
fn encode_url_host(server: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    let bare = server
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(server);
    // A zone id (`fe80::1%en0`) isn't part of the address literal, so parse without it.
    let (address, zone) = match bare.split_once('%') {
        Some((address, zone)) => (address, Some(zone)),
        None => (bare, None),
    };
    if address.parse::<std::net::Ipv6Addr>().is_ok() {
        // RFC 6874: the zone delimiter is written `%25` inside a URL, so the literal
        // `%` can't be mistaken for the start of an escape.
        return match zone {
            Some(zone) => format!("[{}%25{}]", address, urlencoding::encode(zone)),
            None => format!("[{}]", address),
        };
    }
    let normalized: String = server.nfc().collect();
    urlencoding::encode(&normalized).into_owned()
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

    let url_string = build_smb_mount_url(server, share, port);

    // Create URL from string using CFURLCreateWithString
    let cf_url_string = CFString::new(&url_string);
    // SAFETY: `cf_url_string` is a live CFString passed by its concrete ref; the null
    // allocator and null base URL are accepted by `CFURLCreateWithString`. The call follows
    // the Create rule (returns +1), so we null-check the result and hand the owning reference
    // to `wrap_under_create_rule`, which takes that single ownership and releases it once on drop.
    let cf_url = unsafe {
        let url_ref =
            core_foundation::url::CFURLCreateWithString(ptr::null(), cf_url_string.as_concrete_TypeRef(), ptr::null());
        if url_ref.is_null() {
            log::warn!("CFURLCreateWithString rejected the mount URL {url_string}");
            return Err(MountError::ProtocolError {
                message: format!("Can't build an address for \"{}\" on \"{}\"", share, server),
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
    // SAFETY: `CFDictionaryCreateMutable` with the null allocator and the kCFType key/value
    // callbacks returns an owning (+1) dictionary whose callbacks retain every key and value
    // on `CFDictionarySetValue`. Each `cf_key`/`cf_value` CFString and `kCFBooleanTrue` is a
    // live CFType for the duration of its `SetValue` call, so the dictionary's retain keeps
    // them alive after the temporaries drop. The +1 dictionary is released below via `CFRelease`.
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
    // SAFETY: `cf_url` and `open_options` are live CF objects for the call. `cf_user`/`cf_pass`
    // are passed as borrowed CFString refs or null (a guest mount), both accepted. `mountpoints`
    // is a valid out-param; on success NetFS writes a +1 CFArray there, which `extract_mount_path`
    // releases. The two null pointers (mount path, mount options) are accepted defaults.
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
        // SAFETY: guarded non-null, `open_options` is the +1 dictionary created above and not
        // yet released, so this balances its single Create-rule retain exactly once.
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

/// Async wrapper for `mount_share_sync` that runs in a blocking task with timeout.
/// The timeout and its typed failures live in `crate::network::mount_within`, shared
/// with the Linux backend.
pub async fn mount_share(
    server: String,
    share: String,
    username: Option<String>,
    password: Option<String>,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<MountResult, MountError> {
    let server_clone = server.clone();
    crate::network::mount_within(server_clone, timeout_ms, move || {
        mount_share_sync(&server, &share, username.as_deref(), password.as_deref(), port)
    })
    .await
}

/// Extracts the mount path from a `NetFSMountURLSync` mountpoints CFArray.
///
/// Returns `None` if the pointer is null or the array is empty.
fn extract_mount_path(mountpoints: *const c_void) -> Option<String> {
    if mountpoints.is_null() {
        return None;
    }
    // SAFETY: `mountpoints` is the non-null +1 CFArray NetFS wrote (Create rule), valid for this
    // call. `CFArrayGetCount`/`CFArrayGetValueAtIndex` borrow it without transferring ownership,
    // and the element at index 0 is a CFString we wrap with the Get rule (no +1, so we don't
    // release it). We then release the array itself once via `CFRelease`, balancing its Create-rule
    // +1. Keeping the array's Create-rule release distinct from the element's Get-rule borrow is
    // what makes the ownership sound.
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
        && same_share_name(&info.share, share)
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
            && same_share_name(&info.share, share)
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
    use unicode_normalization::UnicodeNormalization;

    let entries = std::fs::read_dir("/Volumes").ok()?;
    // The prefix check runs on NFC for the same reason the mount URL does: `readdir`
    // reports whatever the volume stored, while `share` arrives from the server's own
    // share list, and the two can spell one visible name differently. A raw
    // `starts_with` between the two spellings of `café` never matches, which would
    // leave a non-ASCII share looking unmounted.
    let share_nfc: String = share.nfc().collect();

    for entry in entries.flatten() {
        let path = entry.path().to_string_lossy().to_string();
        // Check paths that start with the share name (for example, "public", "public-1")
        let file_name: String = entry.file_name().to_string_lossy().nfc().collect();
        if !file_name.starts_with(&share_nfc) {
            continue;
        }
        if let Some(info) = get_smb_mount_info(&path)
            && crate::network::server_identity::same_server_live(&info.server, server)
            && same_share_name(&info.share, share)
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

    for entry in entries.flatten() {
        let mount_path = entry.path().to_string_lossy().to_string();
        let Some(info) = get_smb_mount_info(&mount_path) else {
            continue;
        };

        let matches = same_server_name(&info.server, server_name)
            || server_ip.is_some_and(|ip| same_server_name(&info.server, ip));

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
#[path = "mount_test.rs"]
mod tests;
