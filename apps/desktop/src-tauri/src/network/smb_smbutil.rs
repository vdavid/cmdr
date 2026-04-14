//! macOS smbutil wrapper for SMB share listing.
//!
//! Provides fallback share listing using the macOS `smbutil` command when
//! the pure Rust smb-rs implementation fails (for example, with certain Samba servers).

#[cfg(target_os = "macos")]
use crate::network::smb_types::AuthMode;
use crate::network::smb_types::{ShareInfo, ShareListError, ShareListResult};
#[cfg(target_os = "macos")]
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
/// credentials stored in macOS Keychain (for example, from previous Finder connections).
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
            ShareListError::AuthRequired { .. } => ShareListError::AuthRequired {
                message: "Keychain credentials invalid or missing".to_string(),
            },
            ShareListError::ProtocolError { message } => ShareListError::AuthRequired {
                message: format!("No valid Keychain credentials: {}", message),
            },
            other => other,
        }
    })?;

    if shares.is_empty() {
        return Err(ShareListError::AuthRequired {
            message: "Keychain auth returned no shares".to_string(),
        });
    }

    debug!("smbutil with Keychain auth succeeded, got {} shares", shares.len());

    Ok(ShareListResult {
        shares,
        auth_mode: AuthMode::CredsRequired, // User is authenticated via Keychain
        from_cache: false,
    })
}

/// Linux fallback: no Keychain equivalent — return AuthRequired so the user gets a login prompt.
#[cfg(target_os = "linux")]
pub async fn list_shares_smbutil_authenticated_from_keychain(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::AuthRequired {
        message: "Stored credential lookup not available via smbclient fallback".to_string(),
    })
}

/// Stub for platforms with neither smbutil nor smbclient.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn list_shares_smbutil_authenticated_from_keychain(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::AuthRequired {
        message: "Keychain authentication not available on this platform".to_string(),
    })
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
            ShareListError::AuthRequired { .. } => ShareListError::AuthFailed {
                message: "Invalid username or password".to_string(),
            },
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

/// Linux fallback: delegate to `smbclient -L` (guest access).
#[cfg(target_os = "linux")]
pub async fn list_shares_smbutil(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<ShareListResult, ShareListError> {
    use log::debug;
    let host = ip_address.unwrap_or(hostname);
    debug!("smbutil not available on Linux, trying smbclient -L //{} -N", host);

    let shares = super::smb_smbclient::run_smbclient_list(host, port, None).await?;
    Ok(ShareListResult {
        shares,
        auth_mode: super::smb_types::AuthMode::GuestAllowed,
        from_cache: false,
    })
}

/// Stub for platforms with neither smbutil nor smbclient.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn list_shares_smbutil(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError {
        message: "No share listing fallback available on this platform".to_string(),
    })
}

/// Linux fallback: delegate to `smbclient -L -U` (authenticated access).
#[cfg(target_os = "linux")]
pub async fn list_shares_smbutil_with_auth(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
) -> Result<ShareListResult, ShareListError> {
    use log::debug;
    let host = ip_address.unwrap_or(hostname);
    debug!(
        "smbutil not available on Linux, trying smbclient -L //{} -U {}",
        host, username
    );

    let shares = super::smb_smbclient::run_smbclient_list(host, port, Some((username, password))).await?;
    Ok(ShareListResult {
        shares,
        auth_mode: super::smb_types::AuthMode::CredsRequired,
        from_cache: false,
    })
}

/// Stub for platforms with neither smbutil nor smbclient.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn list_shares_smbutil_with_auth(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
    _username: &str,
    _password: &str,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError {
        message: "No share listing fallback available on this platform".to_string(),
    })
}

/// Builds an SMB URL for smbutil commands.
/// Returns (url_for_command, safe_url_for_logging).
#[cfg(target_os = "macos")]
pub fn build_smbutil_url(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    credentials: Option<(&str, &str)>,
) -> (String, String) {
    // Prefer hostname over loopback IPs: macOS smbutil fails with //127.0.0.1:PORT
    // ("Broken pipe") but works with //localhost:PORT on non-standard ports.
    let host = match ip_address {
        Some(ip) if ip == "127.0.0.1" || ip == "::1" => hostname,
        Some(ip) => ip,
        None => hostname,
    };

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
    .map_err(|e| ShareListError::ProtocolError {
        message: format!("Failed to spawn smbutil: {}", e),
    })?
    .map_err(|e| ShareListError::ProtocolError {
        message: format!("Failed to run smbutil: {}", e),
    })?;

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
            return Err(ShareListError::AuthRequired {
                message: "smbutil: Authentication required".to_string(),
            });
        }
        return Err(ShareListError::ProtocolError {
            message: format!("smbutil failed: {}", stderr.trim()),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_smbutil_output(&stdout))
}

/// Parses smbutil view output to extract share information.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "Used by macOS smbutil path and tests")
)]
/// Example output:
/// ```text
/// Share                                           Type    Comments
/// -------------------------------
/// public                                          Disk
/// Documents                                       Disk    My documents
/// ```
pub fn parse_smbutil_output(output: &str) -> Vec<ShareInfo> {
    // Known smbutil share type keywords, used to locate the type column in each line.
    const TYPE_KEYWORDS: &[&str] = &["Disk", "Pipe", "Printer"];

    let mut shares = Vec::new();
    let mut in_shares_section = false;

    for line in output.lines() {
        // Detect header line
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

        if line.trim().is_empty() {
            continue;
        }

        // Find the type keyword in the line. smbutil pads the name with spaces before the type,
        // so we look for "  Disk", "  Pipe", or "  Printer" (preceded by at least two spaces).
        // This handles share names with spaces and multi-byte unicode characters correctly.
        let mut found_type_pos = None;
        let mut found_type_keyword = "";
        for keyword in TYPE_KEYWORDS {
            // Look for the keyword preceded by whitespace (at least 2 spaces before it)
            let search = format!("  {}", keyword);
            if let Some(pos) = line.rfind(&search) {
                let type_start = pos + 2; // skip the two leading spaces
                found_type_pos = Some(type_start);
                found_type_keyword = keyword;
                break;
            }
        }

        let (name, share_type, comment) = if let Some(type_start) = found_type_pos {
            let name = line[..type_start].trim_end().to_string();
            let after_type = &line[type_start + found_type_keyword.len()..];
            let comment_text = after_type.trim();
            let cmt = if comment_text.is_empty() {
                None
            } else {
                Some(comment_text.to_string())
            };
            (name, found_type_keyword.to_lowercase(), cmt)
        } else {
            // No known type keyword found — skip this line
            continue;
        };

        if name.is_empty() {
            continue;
        }

        // Skip hidden shares (ending with $) and non-disk shares
        if name.ends_with('$') {
            continue;
        }
        if share_type != "disk" {
            continue;
        }

        shares.push(ShareInfo {
            name,
            is_disk: true,
            comment,
        });
    }

    shares
}

#[cfg(all(test, target_os = "macos"))]
mod url_tests {
    use super::*;

    #[test]
    fn build_smbutil_url_uses_ip_when_not_loopback() {
        let (url, _) = build_smbutil_url("nas.local", Some("192.168.1.50"), 445, None);
        assert_eq!(url, "//192.168.1.50");
    }

    #[test]
    fn build_smbutil_url_falls_back_to_hostname_for_127_0_0_1() {
        let (url, _) = build_smbutil_url("localhost", Some("127.0.0.1"), 9445, None);
        assert_eq!(url, "//localhost:9445");
    }

    #[test]
    fn build_smbutil_url_falls_back_to_hostname_for_ipv6_loopback() {
        let (url, _) = build_smbutil_url("localhost", Some("::1"), 9445, None);
        assert_eq!(url, "//localhost:9445");
    }

    #[test]
    fn build_smbutil_url_uses_hostname_when_no_ip() {
        let (url, _) = build_smbutil_url("mynas.local", None, 445, None);
        assert_eq!(url, "//mynas.local");
    }

    #[test]
    fn build_smbutil_url_omits_port_for_445() {
        let (url, _) = build_smbutil_url("nas.local", Some("10.0.0.5"), 445, None);
        assert_eq!(url, "//10.0.0.5");
    }

    #[test]
    fn build_smbutil_url_includes_non_standard_port() {
        let (url, _) = build_smbutil_url("nas.local", Some("10.0.0.5"), 9445, None);
        assert_eq!(url, "//10.0.0.5:9445");
    }

    #[test]
    fn build_smbutil_url_with_credentials_and_loopback() {
        let (url, safe) = build_smbutil_url("localhost", Some("127.0.0.1"), 9446, Some(("testuser", "testpass")));
        assert_eq!(url, "//testuser:testpass@localhost:9446");
        assert_eq!(safe, "//testuser:***@localhost:9446");
    }

    #[test]
    fn build_smbutil_url_with_credentials_standard_port() {
        let (url, safe) = build_smbutil_url("nas.local", Some("10.0.0.5"), 445, Some(("admin", "s3cret")));
        assert_eq!(url, "//admin:s3cret@10.0.0.5");
        assert_eq!(safe, "//admin:***@10.0.0.5");
    }
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn parse_multiple_shares_filters_hidden_and_non_disk() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Public                                          Disk    System default share
Web                                             Disk
Multimedia                                      Disk    System default share
IPC$                                            Pipe    IPC Service (NAS Server)
home                                            Disk    Home
ADMIN$                                          Disk    Admin share

6 shares listed
";
        let shares = parse_smbutil_output(output);

        assert_eq!(shares.len(), 4);
        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Public"));
        assert!(names.contains(&"Web"));
        assert!(names.contains(&"Multimedia"));
        assert!(names.contains(&"home"));
        assert!(!names.contains(&"IPC$"));
        assert!(!names.contains(&"ADMIN$"));
        assert!(shares.iter().all(|s| s.is_disk));

        let public = shares.iter().find(|s| s.name == "Public").unwrap();
        assert_eq!(public.comment.as_deref(), Some("System default share"));

        let web = shares.iter().find(|s| s.name == "Web").unwrap();
        assert!(web.comment.is_none());
    }

    #[test]
    fn parse_empty_output_returns_no_shares() {
        // Header + separator but no share lines
        let output = "\
Share                                           Type    Comments
-------------------------------

0 shares listed
";
        let shares = parse_smbutil_output(output);
        assert!(shares.is_empty());
    }

    #[test]
    fn parse_completely_empty_string() {
        assert!(parse_smbutil_output("").is_empty());
    }

    #[test]
    fn parse_single_share() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Backups                                         Disk    Time Machine backups

1 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "Backups");
        assert_eq!(shares[0].comment.as_deref(), Some("Time Machine backups"));
        assert!(shares[0].is_disk);
    }

    #[test]
    fn parse_share_names_with_spaces() {
        let output = "\
Share                                           Type    Comments
-------------------------------
My Documents                                    Disk    Personal files
Shared Media Files                              Disk
Time Machine                                    Disk    TM backups

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 3);

        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"My Documents"));
        assert!(names.contains(&"Shared Media Files"));
        assert!(names.contains(&"Time Machine"));

        let docs = shares.iter().find(|s| s.name == "My Documents").unwrap();
        assert_eq!(docs.comment.as_deref(), Some("Personal files"));
    }

    #[test]
    fn parse_unicode_share_names() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Données                                         Disk    French data
共有フォルダ                                     Disk    Japanese folder
Müsik                                           Disk

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 3);

        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Données"));
        assert!(names.contains(&"共有フォルダ"));
        assert!(names.contains(&"Müsik"));
    }

    #[test]
    fn parse_special_characters_in_names() {
        let output = "\
Share                                           Type    Comments
-------------------------------
Music & Videos                                  Disk    Media collection
Work (2024)                                     Disk
Tom's Files                                     Disk    Personal

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 3);

        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Music & Videos"));
        assert!(names.contains(&"Work (2024)"));
        assert!(names.contains(&"Tom's Files"));
    }

    #[test]
    fn parse_output_without_trailing_summary() {
        // Some macOS versions may omit the "N shares listed" line
        let output = "\
Share                                           Type    Comments
-------------------------------
Photos                                          Disk    Family photos
Videos                                          Disk
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "Photos");
        assert_eq!(shares[1].name, "Videos");
    }

    #[test]
    fn parse_only_ipc_and_printer_shares_returns_empty() {
        let output = "\
Share                                           Type    Comments
-------------------------------
IPC$                                            Pipe    IPC Service
print$                                          Printer Printer Drivers
Canon MF640                                     Printer Office printer

3 shares listed
";
        let shares = parse_smbutil_output(output);
        assert!(shares.is_empty());
    }

    #[test]
    fn parse_preamble_text_before_header_is_ignored() {
        // smbutil may print server info before the share table
        let output = "\
OS (PC Network Program 1.0)
Lanman (Windows for Workgroups 3.1a)
Share                                           Type    Comments
-------------------------------
Documents                                       Disk

1 shares listed
";
        let shares = parse_smbutil_output(output);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "Documents");
    }
}
