//! SMB client for share enumeration.
//!
//! Uses the `smb2` crate to list shares on network hosts.
//! Implements caching and authentication handling.

use log::{debug, warn};
use std::time::Duration;

// Re-export public types (re-exports of items used in smb_client's public API)
pub use super::smb_cache::get_cached_shares_auth_mode;
pub use super::smb_cache::invalidate_cache;
pub use super::smb_types::{AuthMode, ShareListError, ShareListResult};

// Internal imports
use super::smb_cache::{DEFAULT_CACHE_TTL_MS, DEFAULT_LIST_SHARES_TIMEOUT_MS, cache_shares, get_cached_shares};
use super::smb_connection::{try_list_shares_as_guest, try_list_shares_authenticated};
use super::smb_smbutil::{list_shares_smbutil, list_shares_smbutil_authenticated_from_keychain};
// macOS no longer shells out with explicit credentials (the URL-embedded password leaks into
// argv); only the non-macOS authed fallback (smbclient `-A` authfile) still uses this.
#[cfg(not(target_os = "macos"))]
use super::smb_smbutil::list_shares_smbutil_with_auth;
// Only the macOS arm classifies the raw smb2 failure of an authenticated listing
// (Linux retries via the smbclient authfile fallback instead).
#[cfg(target_os = "macos")]
use super::smb_util::classify_authenticated_error;
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
            // Protocol error (likely RPC incompatibility with Samba). Try the
            // platform fallback: smbutil (macOS) / smbclient (Linux).
            // Logged at warn! so it's visible in the default E2E log without
            // RUST_LOG=debug: we need this for diagnosing intermittent
            // SMB E2E failures where both paths fail and the user only sees
            // the secondary error.
            warn!(
                "smb2 list_shares failed (host={}, port={}, has_creds={}): ProtocolError({:?}); falling back to smbutil/smbclient",
                hostname,
                port,
                credentials.is_some(),
                message,
            );
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
                    Ok(inner) => {
                        // smb2 returned 0 shares or failed with creds.
                        //
                        // Linux: fall back to `smbclient -L -A <authfile>`. The password
                        // rides in a 0o600 temp file, never argv, so the fallback is safe.
                        //
                        // macOS: there's NO argv-free way to pass an explicit password to
                        // `smbutil view` (the URL is the only channel, which leaks the
                        // cleartext password into `ps`-readable argv). We don't shell out
                        // with credentials here. Surface the underlying smb2 failure instead;
                        // the user can still mount via the secure NetFS path.
                        #[cfg(target_os = "macos")]
                        {
                            let _ = &user; // keep bindings used across cfgs
                            let _ = &pass;
                            return match inner {
                                Ok(_) => Err(ShareListError::AuthFailed {
                                    message: "Invalid username or password".to_string(),
                                }),
                                Err(e) => {
                                    debug!("smb2 authenticated list failed: {}", e);
                                    // Authenticated context: a rejected session means
                                    // wrong credentials, not "authentication required".
                                    Err(classify_authenticated_error(&e))
                                }
                            };
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = inner;
                            debug!("smb2 auth returned empty or failed, trying smbclient with credentials");
                            return match list_shares_smbutil_with_auth(hostname, ip_address, port, user, pass).await {
                                Ok(result) => {
                                    debug!("smbclient with auth succeeded, got {} shares", result.shares.len());
                                    Ok(result)
                                }
                                Err(e) => {
                                    debug!("smbclient with auth also failed: {:?}", e);
                                    Err(e)
                                }
                            };
                        }
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

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Regression test for the srvsvc fragment-reassembly bug fixed in smb2 0.11.3.
    ///
    /// Older Samba / NAS firmware splits a `NetShareEnum` reply across multiple
    /// DCE/RPC fragments when there are many shares (or long comments). smb2 ≤ 0.11.2
    /// treated the resulting `STATUS_BUFFER_OVERFLOW` as fatal and returned a
    /// `ProtocolError`, which sent Cmdr down the macOS smbutil fallback (the leaky
    /// authed path we've since removed). 0.11.3 reassembles the fragments and follows
    /// the overflow, so the full share list comes back over the pure-Rust path.
    ///
    /// The `smb-consumer-50shares` container serves exactly 50 guest-accessible disk
    /// shares (`share_01`..`share_50`), enough to fragment the reply on a default Samba
    /// buffer. We go through Cmdr's own `list_shares` entry point (not smb2's API) so a
    /// regression in the fallback wiring or the smb2 pin would surface here, proving the
    /// user-facing symptom is gone end to end.
    #[tokio::test]
    #[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
    async fn smb_integration_many_shares_enumerate_via_smb2() {
        // Read the port straight from the env (default matches smb2's
        // `DEFAULT_50SHARES_PORT`) rather than `smb2::testing::many_shares_port()`,
        // so this test compiles without the `smb-e2e` feature — the same convention
        // the other `smb_integration_*` tests follow.
        let port: u16 = std::env::var("SMB_CONSUMER_50SHARES_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10483);
        // Use `localhost` rather than `127.0.0.1`: the SMB test harness uses `localhost`
        // to dodge the macOS smbutil/NetFS loopback quirk on non-standard ports.
        let host = "localhost";

        let result = list_shares(
            "smb-integration-50shares",
            host,
            None,
            port,
            None,    // guest
            None,    // default timeout
            Some(0), // no caching: force a live round-trip
        )
        .await
        .expect("listing 50 shares as guest should succeed via smb2");

        assert_eq!(
            result.shares.len(),
            50,
            "expected all 50 shares from the fragmented srvsvc reply, got {}: {:?}",
            result.shares.len(),
            result.shares.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            result.shares.iter().any(|s| s.name == "share_01"),
            "expected share_01 in the enumerated set"
        );
        assert!(
            result.shares.iter().any(|s| s.name == "share_50"),
            "expected share_50 (last fragment) in the enumerated set"
        );
    }
}
