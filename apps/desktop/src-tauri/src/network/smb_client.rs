//! SMB client for share enumeration.
//!
//! Uses the `smb` crate (smb-rs) to list shares on network hosts.
//! Implements connection pooling, caching, and authentication handling.

use log::debug;
use serde::{Deserialize, Serialize};
use smb::{Client, ClientConfig};
use smb_rpc::interface::ShareInfo1;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// Information about a discovered share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareInfo {
    /// Name of the share (for example, "Documents", "Media").
    pub name: String,
    /// Whether this is a disk share (true) or other type like printer/IPC.
    pub is_disk: bool,
    /// Optional description/comment for the share.
    pub comment: Option<String>,
}

/// Authentication mode detected for a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Guest access works for this host.
    GuestAllowed,
    /// Authentication is required (guest access failed).
    CredsRequired,
    /// Haven't checked yet or check failed.
    Unknown,
}

/// Result of a share listing operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareListResult {
    /// Shares found on the host (already filtered to disk shares only).
    pub shares: Vec<ShareInfo>,
    /// Authentication mode detected.
    pub auth_mode: AuthMode,
    /// Whether this result came from cache.
    pub from_cache: bool,
}

/// Error types for share listing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum ShareListError {
    /// Host is not reachable.
    HostUnreachable(String),
    /// Connection timed out.
    Timeout(String),
    /// Authentication required but no credentials provided.
    AuthRequired(String),
    /// Server requires SMB signing - guest access won't work.
    SigningRequired(String),
    /// Authentication failed with provided credentials.
    AuthFailed(String),
    /// Other SMB protocol error.
    ProtocolError(String),
    /// DNS/hostname resolution failed.
    ResolutionFailed(String),
}

impl std::fmt::Display for ShareListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostUnreachable(msg) => write!(f, "Host unreachable: {}", msg),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::AuthRequired(msg) => write!(f, "Authentication required: {}", msg),
            Self::SigningRequired(msg) => write!(f, "SMB signing required: {}", msg),
            Self::AuthFailed(msg) => write!(f, "Authentication failed: {}", msg),
            Self::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            Self::ResolutionFailed(msg) => write!(f, "Resolution failed: {}", msg),
        }
    }
}

// --- Cache ---

/// Cached share list with expiration.
struct CachedShares {
    result: ShareListResult,
    expires_at: Instant,
}

/// Share cache with configurable TTL.
static SHARE_CACHE: std::sync::OnceLock<Mutex<HashMap<String, CachedShares>>> = std::sync::OnceLock::new();

/// Default cache TTL (30 seconds) - used when no setting is provided.
const DEFAULT_CACHE_TTL_MS: u64 = 30_000;
/// Default list shares timeout (15 seconds) - used when no setting is provided.
const DEFAULT_LIST_SHARES_TIMEOUT_MS: u64 = 15_000;

fn get_share_cache() -> &'static Mutex<HashMap<String, CachedShares>> {
    SHARE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Gets cached shares for a host if still valid.
fn get_cached_shares(host_id: &str) -> Option<ShareListResult> {
    let cache = get_share_cache().lock().ok()?;
    let entry = cache.get(host_id)?;

    if Instant::now() < entry.expires_at {
        let mut result = entry.result.clone();
        result.from_cache = true;
        Some(result)
    } else {
        None
    }
}

/// Caches share list for a host with a configurable TTL.
fn cache_shares(host_id: &str, result: &ShareListResult, cache_ttl_ms: u64) {
    if let Ok(mut cache) = get_share_cache().lock() {
        // Clean up expired entries while we're here
        let now = Instant::now();
        cache.retain(|_, v| v.expires_at > now);

        let ttl = Duration::from_millis(cache_ttl_ms);
        cache.insert(
            host_id.to_string(),
            CachedShares {
                result: result.clone(),
                expires_at: now + ttl,
            },
        );
    }
}

/// Invalidates cache for a host.
#[allow(
    dead_code,
    reason = "Will be used when implementing cache invalidation on host disconnect"
)]
pub fn invalidate_cache(host_id: &str) {
    if let Ok(mut cache) = get_share_cache().lock() {
        cache.remove(host_id);
    }
}

/// Gets the cached auth mode for a host, if available.
pub fn get_cached_shares_auth_mode(host_id: &str) -> Option<AuthMode> {
    let cache = get_share_cache().lock().ok()?;
    let entry = cache.get(host_id)?;

    if Instant::now() < entry.expires_at {
        Some(entry.result.auth_mode)
    } else {
        None
    }
}

// --- Share Listing ---

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

/// Lists shares using macOS smbutil command as fallback.
/// This works with Samba servers that have RPC compatibility issues with smb-rs.
#[cfg(target_os = "macos")]
async fn list_shares_smbutil(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<ShareListResult, ShareListError> {
    let (url, _) = build_smbutil_url(hostname, ip_address, port, None);
    debug!("Running smbutil view -G -N {}", url);

    let shares = run_smbutil_view(&url, true).await?;

    Ok(ShareListResult {
        shares,
        auth_mode: AuthMode::GuestAllowed,
        from_cache: false,
    })
}

/// Lists shares using macOS Keychain credentials via smbutil.
/// This runs smbutil WITHOUT explicit credentials, allowing it to use
/// credentials stored in macOS Keychain (e.g., from previous Finder connections).
/// No -G (guest) flag, so smbutil will try to authenticate using Keychain.
#[cfg(target_os = "macos")]
async fn list_shares_smbutil_authenticated_from_keychain(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<ShareListResult, ShareListError> {
    let (url, _) = build_smbutil_url(hostname, ip_address, port, None);
    debug!("Running smbutil view -N {} (using Keychain)", url);

    let shares = run_smbutil_view(&url, false).await.map_err(|e| {
        // Convert generic auth errors to Keychain-specific messages
        match e {
            ShareListError::AuthRequired(_) => {
                ShareListError::AuthRequired("Keychain credentials invalid or missing".to_string())
            }
            ShareListError::ProtocolError(msg) => {
                ShareListError::AuthRequired(format!("No valid Keychain credentials: {}", msg))
            }
            other => other,
        }
    })?;

    if shares.is_empty() {
        return Err(ShareListError::AuthRequired(
            "Keychain auth returned no shares".to_string(),
        ));
    }

    debug!("smbutil with Keychain auth succeeded, got {} shares", shares.len());

    Ok(ShareListResult {
        shares,
        auth_mode: AuthMode::CredsRequired, // User is authenticated via Keychain
        from_cache: false,
    })
}

/// Fallback for non-macOS platforms - smbutil Keychain auth is not available.
#[cfg(not(target_os = "macos"))]
async fn list_shares_smbutil_authenticated_from_keychain(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::AuthRequired(
        "Keychain authentication not available on this platform".to_string(),
    ))
}

/// Lists shares using macOS smbutil command WITH credentials.
/// This is used when smb-rs authentication fails, but we have credentials.
#[cfg(target_os = "macos")]
async fn list_shares_smbutil_with_auth(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
) -> Result<ShareListResult, ShareListError> {
    let (url, safe_url) = build_smbutil_url(hostname, ip_address, port, Some((username, password)));
    debug!("Running smbutil view -N {}", safe_url);

    let shares = run_smbutil_view(&url, false).await.map_err(|e| {
        // Convert auth errors to AuthFailed for explicit credential attempts
        match e {
            ShareListError::AuthRequired(_) => ShareListError::AuthFailed("Invalid username or password".to_string()),
            other => other,
        }
    })?;

    debug!("smbutil with auth succeeded, got {} shares", shares.len());

    Ok(ShareListResult {
        shares,
        auth_mode: AuthMode::CredsRequired,
        from_cache: false,
    })
}

/// Fallback for non-macOS platforms - smbutil is not available.
#[cfg(not(target_os = "macos"))]
async fn list_shares_smbutil(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError(
        "smbutil fallback not available on this platform".to_string(),
    ))
}

/// Fallback for non-macOS platforms - smbutil with auth is not available.
#[cfg(not(target_os = "macos"))]
async fn list_shares_smbutil_with_auth(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
    _username: &str,
    _password: &str,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError(
        "smbutil fallback not available on this platform".to_string(),
    ))
}

/// Builds an SMB URL for smbutil commands.
/// Returns (url_for_command, safe_url_for_logging).
fn build_smbutil_url(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
) -> (String, String) {
    let host = ip_address.unwrap_or(hostname);

    match credentials {
        Some((username, password)) => {
            let encoded_password = urlencoding::encode(password);
            let url = if port == 445 {
                format!("//{}:{}@{}", username, encoded_password, host)
            } else {
                format!("//{}:{}@{}:{}", username, encoded_password, host, port)
            };
            let safe_url = if port == 445 {
                format!("//{}:***@{}", username, host)
            } else {
                format!("//{}:***@{}:{}", username, host, port)
            };
            (url, safe_url)
        }
        None => {
            let url = if port == 445 {
                format!("//{}", host)
            } else {
                format!("//{}:{}", host, port)
            };
            (url.clone(), url)
        }
    }
}

/// Runs smbutil view command and returns parsed shares.
/// `use_guest` controls whether to use -G flag (guest access).
#[cfg(target_os = "macos")]
async fn run_smbutil_view(url: &str, use_guest: bool) -> Result<Vec<ShareInfo>, ShareListError> {
    use std::process::Command;

    let url_owned = url.to_string();
    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new("smbutil");
        cmd.arg("view").arg("-N");
        if use_guest {
            cmd.arg("-G");
        }
        cmd.arg(&url_owned).output()
    })
    .await
    .map_err(|e| ShareListError::ProtocolError(format!("Failed to spawn smbutil: {}", e)))?
    .map_err(|e| ShareListError::ProtocolError(format!("Failed to run smbutil: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!(
            "smbutil failed: exit={:?}, stderr={}, stdout={}",
            output.status.code(),
            stderr,
            stdout
        );

        if stderr.contains("Authentication error") || stderr.contains("rejected the authentication") {
            return Err(ShareListError::AuthRequired(
                "smbutil: Authentication required".to_string(),
            ));
        }
        return Err(ShareListError::ProtocolError(format!(
            "smbutil failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_smbutil_output(&stdout))
}

/// Parses smbutil view output to extract share information.
/// Example output:
/// ```text
/// Share                                           Type    Comments
/// -------------------------------
/// public                                          Disk
/// Documents                                       Disk    My documents
/// ```
fn parse_smbutil_output(output: &str) -> Vec<ShareInfo> {
    let mut shares = Vec::new();
    let mut in_shares_section = false;

    for line in output.lines() {
        // Skip header and separator
        if line.starts_with("Share") && line.contains("Type") {
            in_shares_section = true;
            continue;
        }
        if line.starts_with("---") {
            continue;
        }
        if line.contains("shares listed") {
            break;
        }

        if !in_shares_section {
            continue;
        }

        // Parse share line: NAME (padded)  TYPE  COMMENT
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Split by multiple spaces (columns are space-padded)
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let name = parts[0].to_string();
        let share_type = parts[1].to_lowercase();

        // Skip hidden shares (ending with $) and non-disk shares
        if name.ends_with('$') {
            continue;
        }
        if share_type != "disk" {
            continue;
        }

        // Comment is everything after the type
        let comment = if parts.len() > 2 {
            Some(parts[2..].join(" "))
        } else {
            None
        };

        shares.push(ShareInfo {
            name,
            is_disk: true,
            comment,
        });
    }

    shares
}

/// Establishes SMB connection and returns the name to use for IPC operations.
/// Connects via IP address when available (preferred), falling back to hostname resolution.
async fn establish_smb_connection<'a>(
    client: &Client,
    server_name: &'a str,
    hostname: &'a str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<&'a str, String> {
    if let Some(ip) = ip_address {
        // Use IP address for connection to bypass mDNS resolution issues
        let socket_addr: SocketAddr = format!("{}:{}", ip, port)
            .parse()
            .map_err(|e| format!("Invalid IP {}: {}", ip, e))?;

        debug!(
            "Connecting to server_name='{}' at socket_addr='{}'",
            server_name, socket_addr
        );

        client
            .connect_to_address(server_name, socket_addr)
            .await
            .map_err(|e| format!("Connect to {} failed: {}", ip, e))?;

        debug!(
            "connect_to_address succeeded, now calling ipc_connect with server_name='{}'",
            server_name
        );

        // After connect_to_address, use server_name for IPC (without .local)
        Ok(server_name)
    } else {
        // No IP - try hostname resolution (may fail for .local)
        debug!("No IP address provided, using hostname='{}' for ipc_connect", hostname);
        Ok(hostname)
    }
}

/// Attempts to list shares as guest (anonymous).
/// Connects via IP address when available (preferred), falling back to hostname resolution.
async fn try_list_shares_as_guest(
    client: &Client,
    server_name: &str,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    timeout_duration: Duration,
) -> Result<Vec<ShareInfo1>, String> {
    timeout(timeout_duration, async {
        let connect_name = establish_smb_connection(client, server_name, hostname, ip_address, port).await?;

        // Connect to IPC$ with "Guest" user
        debug!("Calling ipc_connect with connect_name='{}'", connect_name);
        client
            .ipc_connect(connect_name, "Guest", String::new())
            .await
            .map_err(|e| format!("IPC connect failed: {}", e))?;

        // List shares
        client
            .list_shares(connect_name)
            .await
            .map_err(|e| format!("list_shares failed: {}", e))
    })
    .await
    .map_err(|_| format!("Timeout after {}s", timeout_duration.as_secs()))?
}

/// Attempts to list shares with credentials.
/// Connects via IP address when available (preferred), falling back to hostname resolution.
#[allow(clippy::too_many_arguments, reason = "Internal function, parameters are all needed")]
async fn try_list_shares_authenticated(
    client: &Client,
    server_name: &str,
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
    timeout_duration: Duration,
) -> Result<Vec<ShareInfo1>, String> {
    timeout(timeout_duration, async {
        let connect_name = establish_smb_connection(client, server_name, hostname, ip_address, port).await?;

        // Connect to IPC$ with credentials
        client
            .ipc_connect(connect_name, username, password.to_string())
            .await
            .map_err(|e| format!("IPC connect failed: {}", e))?;

        // List shares
        client
            .list_shares(connect_name)
            .await
            .map_err(|e| format!("list_shares failed: {}", e))
    })
    .await
    .map_err(|_| format!("Timeout after {}s", timeout_duration.as_secs()))?
}

/// Checks if an error is an authentication error (including signing requirement).
fn is_auth_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("logon failure")
        || lower.contains("access denied")
        || lower.contains("auth")
        || lower.contains("0xc000006d") // STATUS_LOGON_FAILURE
        || lower.contains("signing is required") // SMB signing required
}

/// Classifies an error string into a ShareListError.
fn classify_error(err: &str) -> ShareListError {
    let lower = err.to_lowercase();

    if lower.contains("timeout") {
        ShareListError::Timeout(err.to_string())
    } else if lower.contains("no route") || lower.contains("unreachable") || lower.contains("connection refused") {
        ShareListError::HostUnreachable(err.to_string())
    } else if lower.contains("signing is required") || lower.contains("not signed or encrypted") {
        // Server requires SMB signing - guest/anonymous access won't work
        ShareListError::SigningRequired(err.to_string())
    } else if lower.contains("logon failure") || lower.contains("0xc000006d") {
        ShareListError::AuthFailed(err.to_string())
    } else if lower.contains("access denied") || lower.contains("auth") {
        ShareListError::AuthRequired(err.to_string())
    } else {
        ShareListError::ProtocolError(err.to_string())
    }
}

/// Filters raw SMB share info to show only disk shares.
fn filter_disk_shares(shares: Vec<ShareInfo1>) -> Vec<ShareInfo> {
    shares
        .into_iter()
        .filter_map(|share| {
            // Get the share name
            let name = extract_share_name(&share);

            // Skip hidden/admin shares (ending with $)
            if name.ends_with('$') {
                return None;
            }

            // Check if it's a disk share (type 0 in SMB)
            let share_type_str = format!("{:?}", share.share_type);
            let is_disk = share_type_str.contains("Disk") || share_type_str.contains("DiskTree");

            if !is_disk {
                return None;
            }

            // Extract comment
            let comment = extract_share_comment(&share);

            Some(ShareInfo {
                name,
                is_disk: true,
                comment,
            })
        })
        .collect()
}

/// Extracts the share name from SMB share info.
fn extract_share_name(share: &ShareInfo1) -> String {
    // The netname is an NdrPtr<NdrString<u16>>
    // Use Debug format and clean up
    let debug_str = format!("{:?}", share.netname);
    clean_ndr_string(&debug_str)
}

/// Extracts the comment from SMB share info.
fn extract_share_comment(share: &ShareInfo1) -> Option<String> {
    let debug_str = format!("{:?}", share.remark);
    let cleaned = clean_ndr_string(&debug_str);
    if cleaned.is_empty() || cleaned == "None" {
        None
    } else {
        Some(cleaned)
    }
}

/// Cleans up an NDR string from Debug format.
fn clean_ndr_string(debug_str: &str) -> String {
    // NDR strings come out as things like:
    // Some(NdrAlign { inner: NdrString("Documents") })
    // We extract just the string content
    if let Some(start) = debug_str.find('"')
        && let Some(end) = debug_str.rfind('"')
        && start < end
    {
        return debug_str[start + 1..end].to_string();
    }
    debug_str.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ndr_string() {
        assert_eq!(
            clean_ndr_string(r#"Some(NdrAlign { inner: NdrString("Documents") })"#),
            "Documents"
        );
        assert_eq!(clean_ndr_string(r#""Media""#), "Media");
        assert_eq!(clean_ndr_string("None"), "None");
    }

    #[test]
    fn test_is_auth_error() {
        assert!(is_auth_error("Logon Failure (0xc000006d)"));
        assert!(is_auth_error("access denied"));
        assert!(is_auth_error("Authentication failed"));
        assert!(!is_auth_error("Connection refused"));
        assert!(!is_auth_error("Timeout"));
    }

    #[test]
    fn test_classify_error() {
        match classify_error("Timeout after 15s") {
            ShareListError::Timeout(_) => {}
            e => panic!("Expected Timeout, got {:?}", e),
        }

        match classify_error("no route to host") {
            ShareListError::HostUnreachable(_) => {}
            e => panic!("Expected HostUnreachable, got {:?}", e),
        }

        match classify_error("Logon Failure (0xc000006d)") {
            ShareListError::AuthFailed(_) => {}
            e => panic!("Expected AuthFailed, got {:?}", e),
        }
    }

    #[test]
    fn test_cache_operations() {
        let host_id = "test-host-cache";

        // Initially no cache
        assert!(get_cached_shares(host_id).is_none());

        // Cache something
        let result = ShareListResult {
            shares: vec![ShareInfo {
                name: "TestShare".to_string(),
                is_disk: true,
                comment: None,
            }],
            auth_mode: AuthMode::GuestAllowed,
            from_cache: false,
        };
        cache_shares(host_id, &result, DEFAULT_CACHE_TTL_MS);

        // Should be cached now
        let cached = get_cached_shares(host_id);
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert!(cached.from_cache);
        assert_eq!(cached.shares.len(), 1);
        assert_eq!(cached.shares[0].name, "TestShare");

        // Invalidate
        invalidate_cache(host_id);
        assert!(get_cached_shares(host_id).is_none());
    }

    #[test]
    fn test_parse_smbutil_output() {
        let output = r#"Share                                           Type    Comments
-------------------------------
Public                                          Disk    System default share
Web                                             Disk    
Multimedia                                      Disk    System default share
IPC$                                            Pipe    IPC Service (NAS Server)
home                                            Disk    Home
ADMIN$                                          Disk    Admin share

6 shares listed
"#;

        let shares = parse_smbutil_output(output);

        // Should have 4 disk shares (excluding IPC$ and ADMIN$)
        assert_eq!(shares.len(), 4);

        // Check names
        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Public"));
        assert!(names.contains(&"Web"));
        assert!(names.contains(&"Multimedia"));
        assert!(names.contains(&"home"));
        assert!(!names.contains(&"IPC$"));
        assert!(!names.contains(&"ADMIN$"));

        // Check that all are marked as disk
        assert!(shares.iter().all(|s| s.is_disk));

        // Check comments
        let public = shares.iter().find(|s| s.name == "Public").unwrap();
        assert_eq!(public.comment.as_deref(), Some("System default share"));

        let web = shares.iter().find(|s| s.name == "Web").unwrap();
        assert!(web.comment.is_none());
    }
}
