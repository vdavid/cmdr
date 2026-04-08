//! SMB client for share enumeration.
//!
//! Uses the `smb2` crate to list shares on network hosts.
//! Implements caching and authentication handling.

use log::debug;
use std::time::Duration;

// Re-export public types (re-exports of items used in smb_client's public API)
pub use super::smb_cache::get_cached_shares_auth_mode;
pub use super::smb_types::{AuthMode, ShareListError, ShareListResult};

// Internal imports
use super::smb_cache::{DEFAULT_CACHE_TTL_MS, DEFAULT_LIST_SHARES_TIMEOUT_MS, cache_shares, get_cached_shares};
use super::smb_connection::{try_list_shares_as_guest, try_list_shares_authenticated};
use super::smb_smbutil::{
    list_shares_smbutil, list_shares_smbutil_authenticated_from_keychain, list_shares_smbutil_with_auth,
};
use super::smb_util::{classify_error, convert_shares, is_auth_error};

/// Lists shares on a network host.
///
/// Attempts guest access first, then uses provided credentials if guest fails.
/// Results are cached for the specified TTL.
///
/// # Arguments
/// * `host_id` - Unique identifier for the host (used for caching)
/// * `hostname` - Hostname to connect to (for example, "TEST_SERVER.local")
/// * `ip_address` - Optional resolved IP address (preferred over hostname)
/// * `credentials` - Optional (username, password) tuple for authenticated access
/// * `timeout_ms` - Timeout in milliseconds for the operation (default: 15000)
/// * `cache_ttl_ms` - Cache TTL in milliseconds (default: 30000)
pub async fn list_shares(
    host_id: &str,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
    timeout_ms: Option<u64>,
    cache_ttl_ms: Option<u64>,
) -> Result<ShareListResult, ShareListError> {
    // Only use cache for non-authenticated requests.
    // When credentials are provided, the user is explicitly authenticating
    // and expects fresh results (not cached guest attempt results).
    if credentials.is_none()
        && let Some(cached) = get_cached_shares(host_id)
    {
        return Ok(cached);
    }

    // Use provided timeout or default
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_LIST_SHARES_TIMEOUT_MS));

    // Try to list shares
    let result = list_shares_uncached(hostname, ip_address, port, credentials, timeout).await?;

    // Cache successful result with configurable TTL
    let ttl = cache_ttl_ms.unwrap_or(DEFAULT_CACHE_TTL_MS);
    cache_shares(host_id, &result, ttl);

    Ok(result)
}

/// Lists shares without checking cache.
/// Uses IP address when available to bypass mDNS resolution issues.
/// Falls back to smbutil on macOS when smb2 fails with protocol errors.
async fn list_shares_uncached(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
    timeout: Duration,
) -> Result<ShareListResult, ShareListError> {
    debug!(
        "list_shares_uncached: hostname={:?}, ip_address={:?}, port={}, has_creds={}",
        hostname,
        ip_address,
        port,
        credentials.is_some()
    );

    // Try smb2 first
    match list_shares_smb2(hostname, ip_address, port, credentials, timeout).await {
        Ok(result) => Ok(result),
        Err(ShareListError::ProtocolError { ref message }) => {
            // Protocol error (likely RPC incompatibility with Samba)
            // Try smbutil fallback on macOS
            debug!("smb2 failed with protocol error: {}, trying smbutil fallback", message);
            list_shares_smbutil(hostname, ip_address, port).await
        }
        Err(e) => Err(e),
    }
}

/// Lists shares using smb2 (pure Rust implementation).
async fn list_shares_smb2(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
    timeout: Duration,
) -> Result<ShareListResult, ShareListError> {
    debug!(
        "list_shares_smb2: hostname={:?}, ip={:?}, has_creds={}",
        hostname,
        ip_address,
        credentials.is_some()
    );

    // Outer timeout as safety net (smb2's config timeout covers TCP connect,
    // but not the full RPC exchange)
    let outer_timeout = timeout;
    // smb2's config timeout: slightly shorter so its typed Error::Timeout fires first
    let connect_timeout = timeout.saturating_sub(Duration::from_secs(2));

    // Try guest access first, then authenticated
    let (shares, auth_mode) = match try_list_shares_as_guest(hostname, ip_address, port, connect_timeout).await {
        Ok(shares) => {
            debug!("Guest access succeeded, got {} shares", shares.len());
            (shares, AuthMode::GuestAllowed)
        }
        Err(e) if is_auth_error(&e) => {
            debug!("Guest failed with auth error: {}", e);
            // Guest failed with auth error - try with credentials if provided
            if let Some((user, pass)) = credentials {
                debug!("Trying authenticated access with user: {}", user);

                match tokio::time::timeout(
                    outer_timeout,
                    try_list_shares_authenticated(hostname, ip_address, port, user, pass, connect_timeout),
                )
                .await
                {
                    Ok(Ok(shares)) if !shares.is_empty() => {
                        debug!("Authenticated access succeeded, got {} shares", shares.len());
                        (shares, AuthMode::CredsRequired)
                    }
                    Ok(Ok(_)) | Ok(Err(_)) => {
                        // smb2 returned 0 shares or failed - fall back to smbutil with auth
                        debug!("smb2 auth returned empty or failed, trying smbutil with credentials");
                        return match list_shares_smbutil_with_auth(hostname, ip_address, port, user, pass).await {
                            Ok(result) => {
                                debug!("smbutil with auth succeeded, got {} shares", result.shares.len());
                                Ok(result)
                            }
                            Err(e) => {
                                debug!("smbutil with auth also failed: {:?}", e);
                                Err(e)
                            }
                        };
                    }
                    Err(_timeout) => {
                        return Err(ShareListError::Timeout {
                            message: format!("Timeout after {}s", outer_timeout.as_secs()),
                        });
                    }
                }
            } else {
                // No explicit credentials provided - try smbutil which uses macOS Keychain
                debug!("No explicit credentials, trying smbutil with Keychain...");
                return match list_shares_smbutil_authenticated_from_keychain(hostname, ip_address, port).await {
                    Ok(result) => {
                        debug!("smbutil with Keychain succeeded, got {} shares", result.shares.len());
                        Ok(result)
                    }
                    Err(e) => {
                        debug!("smbutil with Keychain failed: {:?}, requiring manual login", e);
                        Err(ShareListError::AuthRequired {
                            message: "This server requires authentication to list shares".to_string(),
                        })
                    }
                };
            }
        }
        Err(e) => {
            debug!("Guest failed with non-auth error: {}", e);
            return Err(classify_error(&e));
        }
    };

    // Convert smb2 shares to Cmdr's ShareInfo type
    let converted_shares = convert_shares(shares);
    debug!("Converted {} shares", converted_shares.len());

    Ok(ShareListResult {
        shares: converted_shares,
        auth_mode,
        from_cache: false,
    })
}
