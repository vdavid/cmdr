//! macOS smbutil wrapper for SMB share listing.
//!
//! Provides fallback share listing using the macOS `smbutil` command when
//! the pure Rust smb-rs implementation fails (e.g., with certain Samba servers).

use crate::network::smb_types::{AuthMode, ShareInfo, ShareListError, ShareListResult};
use log::debug;

/// Lists shares using macOS smbutil command as fallback.
/// This works with Samba servers that have RPC compatibility issues with smb-rs.
#[cfg(target_os = "macos")]
pub async fn list_shares_smbutil(
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
pub async fn list_shares_smbutil_authenticated_from_keychain(
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
pub async fn list_shares_smbutil_authenticated_from_keychain(
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
pub async fn list_shares_smbutil_with_auth(
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
pub async fn list_shares_smbutil(
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
pub async fn list_shares_smbutil_with_auth(
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
pub fn build_smbutil_url(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
) -> (String, String) {
    let host = ip_address.unwrap_or(hostname);

    match credentials {
        Some((username, password)) => {
            let encoded_username = urlencoding::encode(username);
            let encoded_password = urlencoding::encode(password);
            let url = if port == 445 {
                format!("//{}:{}@{}", encoded_username, encoded_password, host)
            } else {
                format!("//{}:{}@{}:{}", encoded_username, encoded_password, host, port)
            };
            let safe_url = if port == 445 {
                format!("//{}:***@{}", encoded_username, host)
            } else {
                format!("//{}:***@{}:{}", encoded_username, host, port)
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
pub fn parse_smbutil_output(output: &str) -> Vec<ShareInfo> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
