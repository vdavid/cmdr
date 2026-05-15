//! Linux smbclient wrapper for SMB share listing.
//!
//! Provides fallback share listing using the `smbclient` command (from the `samba-client`
//! package) when the pure Rust smb-rs implementation fails on Linux. This is the Linux
//! equivalent of the macOS `smbutil` fallback in `smb_smbutil.rs`.

use crate::network::smb_types::{ShareInfo, ShareListError};
use log::{debug, warn};
use std::process::Command;

/// Lists shares using `smbclient -L` and returns parsed disk shares.
///
/// Guest mode: pass `credentials: None` → uses `-N` (no password).
/// Authenticated: pass `credentials: Some((user, pass))` → uses `-U user%pass`.
pub async fn run_smbclient_list(
    host: &str,
    port: u16,
    credentials: Option<(&str, &str)>,
) -> Result<Vec<ShareInfo>, ShareListError> {
    let server = format!("//{}", host);
    let port_str = port.to_string();
    let creds_owned = credentials.map(|(u, p)| (u.to_string(), p.to_string()));

    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new("smbclient");
        cmd.arg("-L").arg(&server);
        if port_str != "445" {
            cmd.arg("-p").arg(&port_str);
        }
        match &creds_owned {
            Some((username, password)) => {
                cmd.arg("-U").arg(format!("{}%{}", username, password));
            }
            None => {
                cmd.arg("-N");
            }
        }
        cmd.output()
    })
    .await
    .map_err(|e| ShareListError::ProtocolError {
        message: format!("Failed to spawn smbclient task: {}", e),
    })?
    .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            #[cfg(target_os = "linux")]
            let install_command = super::linux_distro::smbclient_install_command();
            #[cfg(not(target_os = "linux"))]
            let install_command: Option<String> = None;

            ShareListError::MissingDependency {
                message: "smbclient is not installed. It's needed to connect to this server.".to_string(),
                install_command,
            }
        } else {
            ShareListError::ProtocolError {
                message: format!("Failed to run smbclient: {}", e),
            }
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    debug!(
        "smbclient exit={:?}, stdout_len={}, stderr_len={}",
        output.status.code(),
        stdout.len(),
        stderr.len()
    );

    if !output.status.success() {
        // Logged at warn! so the raw output is captured at default level,
        // crucial for diagnosing E2E flakes where smbclient is the last-resort
        // fallback and its stderr is the only direct window into what the
        // server actually told us. Truncated to 1 KiB each to avoid log
        // bloat on pathological output.
        let port_suffix = if port == 445 {
            String::new()
        } else {
            format!(":{}", port)
        };
        warn!(
            "smbclient -L //{}{} failed (exit={:?}, has_creds={}). stderr: {} | stdout: {}",
            host,
            port_suffix,
            output.status.code(),
            credentials.is_some(),
            truncate_for_log(&stderr, 1024),
            truncate_for_log(&stdout, 1024),
        );
        return Err(classify_smbclient_error(&stdout, &stderr, host, credentials.is_some()));
    }

    Ok(parse_smbclient_output(&stdout))
}

/// Truncates a string to at most `max` bytes for log output, appending an
/// ellipsis-with-byte-count when the input was longer. Operates on the raw
/// byte length to keep behaviour deterministic regardless of UTF-8 width.
fn truncate_for_log(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Cut at a char boundary at or before `max` so we never split a UTF-8 sequence.
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated, {} bytes total]", &s[..end], s.len())
}

/// Classifies smbclient error output into a typed error.
fn classify_smbclient_error(stdout: &str, stderr: &str, host: &str, has_creds: bool) -> ShareListError {
    let combined = format!("{} {}", stderr, stdout);

    if combined.contains("NT_STATUS_ACCESS_DENIED")
        || combined.contains("NT_STATUS_LOGON_FAILURE")
        || combined.contains("NT_STATUS_WRONG_PASSWORD")
    {
        return if has_creds {
            ShareListError::AuthFailed {
                message: "Invalid username or password".to_string(),
            }
        } else {
            ShareListError::AuthRequired {
                message: "This server requires authentication".to_string(),
            }
        };
    }

    if combined.contains("NT_STATUS_HOST_UNREACHABLE")
        || combined.contains("NT_STATUS_CONNECTION_REFUSED")
        || (combined.contains("Connection to") && combined.contains("failed"))
    {
        return ShareListError::HostUnreachable {
            message: format!("Cannot reach {}", host),
        };
    }

    if combined.contains("NT_STATUS_IO_TIMEOUT") {
        return ShareListError::Timeout {
            message: format!("Connection to {} timed out", host),
        };
    }

    ShareListError::ProtocolError {
        message: format!("smbclient failed: {}", stderr.trim()),
    }
}

/// Parses `smbclient -L` output to extract share information.
///
/// Example output:
/// ```text
///     Sharename       Type      Comment
///     ---------       ----      -------
///     Public          Disk      System default share
///     Documents       Disk
///     IPC$            IPC       IPC Service (NAS Server)
///
/// SMB1 disabled -- no workgroup available
/// ```
pub fn parse_smbclient_output(output: &str) -> Vec<ShareInfo> {
    let mut shares = Vec::new();
    let mut in_shares_section = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Sharename") && trimmed.contains("Type") {
            in_shares_section = true;
            continue;
        }

        if trimmed.starts_with("---") {
            continue;
        }

        if !in_shares_section || trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            break;
        }

        let name = parts[0].to_string();
        let share_type = parts[1].to_lowercase();

        // If not a known share type, we've left the share section
        if share_type != "disk" && share_type != "ipc" && share_type != "printer" {
            break;
        }

        // Skip hidden shares (ending with $) and non-disk shares
        if name.ends_with('$') || share_type != "disk" {
            continue;
        }

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
    fn test_parse_basic() {
        let output = "\tSharename       Type      Comment\n\
                       \t---------       ----      -------\n\
                       \tPublic          Disk      System default share\n\
                       \tWeb             Disk\n\
                       \tMultimedia      Disk      System default share\n\
                       \tIPC$            IPC       IPC Service (NAS Server)\n\
                       \thome            Disk      Home\n\
                       \tADMIN$          Disk      Admin share\n\
                       \n\
                       SMB1 disabled -- no workgroup available\n";

        let shares = parse_smbclient_output(output);

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
    fn test_parse_empty() {
        let output = "\tSharename       Type      Comment\n\
                       \t---------       ----      -------\n\
                       \n\
                       SMB1 disabled -- no workgroup available\n";

        let shares = parse_smbclient_output(output);
        assert!(shares.is_empty());
    }

    #[test]
    fn test_parse_with_printer() {
        let output = "\tSharename       Type      Comment\n\
                       \t---------       ----      -------\n\
                       \tDocuments       Disk      My docs\n\
                       \tprint$          Printer   Printer Drivers\n\
                       \tIPC$            IPC       IPC Service\n";

        let shares = parse_smbclient_output(output);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "Documents");
    }

    #[test]
    fn test_parse_raspberry_pi() {
        let output = "\tSharename       Type      Comment\n\
                       \t---------       ----      -------\n\
                       \tpi              Disk      Pi shared folder\n\
                       \tIPC$            IPC       IPC Service (Samba 4.13.13-Debian)\n";

        let shares = parse_smbclient_output(output);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].name, "pi");
        assert_eq!(shares[0].comment.as_deref(), Some("Pi shared folder"));
    }

    #[test]
    fn test_classify_auth_errors() {
        let err = classify_smbclient_error("", "NT_STATUS_ACCESS_DENIED", "host", false);
        assert!(matches!(err, ShareListError::AuthRequired { .. }));

        let err = classify_smbclient_error("", "NT_STATUS_LOGON_FAILURE", "host", true);
        assert!(matches!(err, ShareListError::AuthFailed { .. }));
    }

    #[test]
    fn truncate_for_log_passes_short_unchanged() {
        assert_eq!(truncate_for_log("hello", 10), "hello");
    }

    #[test]
    fn truncate_for_log_truncates_long() {
        let s = "a".repeat(2000);
        let out = truncate_for_log(&s, 1024);
        assert!(out.starts_with(&"a".repeat(1024)));
        assert!(out.contains("[truncated, 2000 bytes total]"));
    }

    #[test]
    fn truncate_for_log_respects_utf8_boundary() {
        let s = "abc中文".to_string();
        let out = truncate_for_log(&s, 4);
        // Byte 3 is 'c'; byte 4 starts '中' (multi-byte). We must back off to 3.
        assert_eq!(out, "abc…[truncated, 9 bytes total]");
    }

    #[test]
    fn test_classify_network_errors() {
        let err = classify_smbclient_error("", "NT_STATUS_HOST_UNREACHABLE", "host", false);
        assert!(matches!(err, ShareListError::HostUnreachable { .. }));

        let err = classify_smbclient_error("", "Connection to host failed", "host", false);
        assert!(matches!(err, ShareListError::HostUnreachable { .. }));

        let err = classify_smbclient_error("", "NT_STATUS_IO_TIMEOUT", "host", false);
        assert!(matches!(err, ShareListError::Timeout { .. }));
    }
}
