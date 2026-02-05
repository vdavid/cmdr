//! SMB client for share enumeration.
//!
//! Uses the `smb` crate (smb-rs) to list shares on network hosts.
//! Implements connection pooling, caching, and authentication handling.

use log::debug;
use smb::{Client, ClientConfig};
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
use super::smb_util::{classify_error, filter_disk_shares, is_auth_error};

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
/// Uses IP address when available to bypass mDNS resolution issues with smb-rs.
/// Falls back to smbutil on macOS when smb-rs fails with protocol errors.
async fn list_shares_uncached(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
    timeout: Duration,
) -> Result<ShareListResult, ShareListError> {
    // Debug log the incoming params
    debug!(
        "list_shares_uncached: hostname={:?}, ip_address={:?}, port={}, has_creds={}",
        hostname,
        ip_address,
        port,
        credentials.is_some()
    );

    // Try smb-rs first
    match list_shares_smb_rs(hostname, ip_address, port, credentials, timeout).await {
        Ok(result) => Ok(result),
        Err(ShareListError::ProtocolError(ref msg)) => {
            // Protocol error (likely RPC incompatibility with Samba)
            // Try smbutil fallback on macOS
            debug!("smb-rs failed with protocol error: {}, trying smbutil fallback", msg);
            list_shares_smbutil(hostname, ip_address, port).await
        }
        Err(e) => Err(e),
    }
}

/// Lists shares using smb-rs (pure Rust implementation).
async fn list_shares_smb_rs(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
    timeout: Duration,
) -> Result<ShareListResult, ShareListError> {
    // Create SMB client with unsigned guest access allowed
    // (some servers like Samba don't require signing for anonymous access)
    let mut config = ClientConfig::default();
    config.connection.allow_unsigned_guest_access = true;
    let client = Client::new(config);

    // Determine the server name to use for SMB protocol
    // When we have an IP, use it as the server name for smb-rs connection lookup
    // (smb-rs associates connections by server name, and hostname lookup can fail)
    let server_name = if let Some(ip) = ip_address {
        ip
    } else {
        hostname.strip_suffix(".local").unwrap_or(hostname)
    };

    debug!(
        "list_shares_smb_rs: server_name={}, has_creds={}",
        server_name,
        credentials.is_some()
    );

    // Try guest access first, then authenticated
    let (shares, auth_mode) =
        match try_list_shares_as_guest(&client, server_name, hostname, ip_address, port, timeout).await {
            Ok(shares) => {
                debug!("Guest access succeeded, got {} raw shares", shares.len());
                (shares, AuthMode::GuestAllowed)
            }
            Err(e) if is_auth_error(&e) => {
                debug!("Guest failed with auth error: {}", e);
                // Guest failed with auth error - try with credentials if provided
                if let Some((user, pass)) = credentials {
                    debug!("Trying authenticated access with user: {}", user);

                    // IMPORTANT: Create a fresh client for authenticated attempt.
                    // smb-rs reuses connections internally, so if we use the same client,
                    // the failed guest connection can interfere with the auth attempt.
                    let mut auth_config = ClientConfig::default();
                    auth_config.connection.allow_unsigned_guest_access = false; // Require proper auth
                    let auth_client = Client::new(auth_config);

                    match try_list_shares_authenticated(
                        &auth_client,
                        server_name,
                        hostname,
                        ip_address,
                        port,
                        user,
                        pass,
                        timeout,
                    )
                    .await
                    {
                        Ok(shares) if !shares.is_empty() => {
                            // smb-rs auth worked and returned shares
                            debug!("Authenticated access succeeded, got {} raw shares", shares.len());
                            (shares, AuthMode::CredsRequired)
                        }
                        Ok(_) | Err(_) => {
                            // smb-rs returned 0 shares or failed - fall back to smbutil with auth
                            // This handles cases where smb-rs internally falls back to guest
                            debug!("smb-rs auth returned empty or failed, trying smbutil with credentials");
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
                    }
                } else {
                    // No explicit credentials provided - try smbutil which uses macOS Keychain
                    // This allows seamless login when user has previously connected via Finder
                    debug!("No explicit credentials, trying smbutil with Keychain...");
                    return match list_shares_smbutil_authenticated_from_keychain(hostname, ip_address, port).await {
                        Ok(result) => {
                            debug!("smbutil with Keychain succeeded, got {} shares", result.shares.len());
                            Ok(result)
                        }
                        Err(e) => {
                            debug!("smbutil with Keychain failed: {:?}, requiring manual login", e);
                            Err(ShareListError::AuthRequired(
                                "This server requires authentication to list shares".to_string(),
                            ))
                        }
                    };
                }
            }
            Err(e) => {
                debug!("Guest failed with non-auth error: {}", e);
                return Err(classify_error(&e));
            }
        };

    // Filter to disk shares only
    let filtered_shares = filter_disk_shares(shares);
    debug!(
        "After filtering: {} disk shares (from {} raw)",
        filtered_shares.len(),
        filtered_shares.len()
    );

    Ok(ShareListResult {
        shares: filtered_shares,
        auth_mode,
        from_cache: false,
    })
}
