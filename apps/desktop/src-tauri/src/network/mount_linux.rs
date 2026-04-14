//! SMB share mounting on Linux using GVFS (`gio mount`).
//!
//! Uses the `gio mount` command for user-space SMB mounting, which works on
//! GNOME/GTK desktops without requiring root privileges. Mounts appear under
//! `/run/user/<uid>/gvfs/` or a similar GVFS-managed path.

use log::debug;
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Result of a successful mount operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    /// Path to the mounted share (for example, "/run/user/1000/gvfs/smb-share:...").
    pub mount_path: String,
    pub already_mounted: bool,
}

/// Errors that can occur during mount operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MountError {
    HostUnreachable { message: String },
    ShareNotFound { message: String },
    AuthRequired { message: String },
    AuthFailed { message: String },
    PermissionDenied { message: String },
    Timeout { message: String },
    Cancelled { message: String },
    ProtocolError { message: String },
    MountPathConflict { message: String },
}

/// Default mount timeout in milliseconds.
const DEFAULT_MOUNT_TIMEOUT_MS: u64 = 20_000;

/// Checks if `gio` is available on the system.
fn is_gio_available() -> bool {
    Command::new("gio").arg("version").output().is_ok()
}

/// Finds the GVFS mount path for an SMB share.
/// Checks `gio mount -l` output for an existing mount matching the server/share.
fn find_existing_mount(server: &str, share: &str) -> Option<String> {
    let output = Command::new("gio").args(["mount", "-l"]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let smb_url_lower = format!("smb://{}/{}", server.to_lowercase(), share.to_lowercase());

    // gio mount -l outputs lines like:
    //   Mount(0): share on server -> smb://server/share/
    // Look for the mount URL matching our target
    for line in stdout.lines() {
        let line_lower = line.to_lowercase();
        if line_lower.contains(&smb_url_lower) {
            // Found a matching mount — derive the GVFS path
            return Some(derive_gvfs_path(server, share));
        }
    }

    None
}

/// Derives the expected GVFS mount path for an SMB share.
/// GVFS mounts appear at `/run/user/<uid>/gvfs/smb-share:server=<server>,share=<share>`
fn derive_gvfs_path(server: &str, share: &str) -> String {
    let uid = unsafe { libc::getuid() };
    format!(
        "/run/user/{}/gvfs/smb-share:server={},share={}",
        uid,
        server.to_lowercase(),
        share.to_lowercase()
    )
}

/// Mount an SMB share synchronously using `gio mount`.
fn mount_share_sync(
    server: &str,
    share: &str,
    username: Option<&str>,
    password: Option<&str>,
    port: u16,
) -> Result<MountResult, MountError> {
    // Check if gio is available
    if !is_gio_available() {
        return Err(MountError::ProtocolError {
            message: "SMB mounting requires GVFS. Install gvfs-smb on your system.".to_string(),
        });
    }

    // Check if already mounted
    if let Some(mount_path) = find_existing_mount(server, share) {
        debug!("Share already mounted at {}", mount_path);
        return Ok(MountResult {
            mount_path,
            already_mounted: true,
        });
    }

    // Build the SMB URL (with port for non-standard)
    let server_part = if port != 445 {
        format!("{}:{}", server, port)
    } else {
        server.to_string()
    };
    let smb_url = if let Some(user) = username {
        format!("smb://{}@{}/{}", user, server_part, share)
    } else {
        format!("smb://{}/{}", server_part, share)
    };

    debug!("Mounting SMB share via gio: {}", smb_url);

    // gio mount with anonymous flag if no credentials
    let mut cmd = Command::new("gio");
    cmd.args(["mount", &smb_url]);

    if username.is_none() {
        cmd.arg("--anonymous");
    }

    // If we have a password, pass it via stdin using the askpass approach.
    // gio mount reads credentials interactively, so we need to handle that.
    // For non-interactive use, we set the password via an environment trick:
    // GIO_USE_FILE_MONITOR and pipe the password.
    if let Some(pass) = password {
        // Use echo to pipe the password to gio mount's stdin
        let shell_cmd = if username.is_some() {
            // gio mount prompts: Password:
            format!("echo '{}' | gio mount '{}'", escape_shell_arg(pass), smb_url)
        } else {
            format!("gio mount --anonymous '{}'", smb_url)
        };

        let output = Command::new("sh")
            .args(["-c", &shell_cmd])
            .output()
            .map_err(|e| MountError::ProtocolError {
                message: format!("Failed to run gio mount: {}", e),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(classify_mount_error(&stderr, server, share));
        }
    } else {
        let output = cmd.output().map_err(|e| MountError::ProtocolError {
            message: format!("Failed to run gio mount: {}", e),
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(classify_mount_error(&stderr, server, share));
        }
    }

    // After successful mount, find the mount path
    let mount_path = find_existing_mount(server, share).unwrap_or_else(|| derive_gvfs_path(server, share));

    Ok(MountResult {
        mount_path,
        already_mounted: false,
    })
}

/// Escapes a string for use in a shell single-quoted context.
fn escape_shell_arg(s: &str) -> String {
    // In single quotes, only single quote itself needs escaping: ' -> '\''
    s.replace('\'', "'\\''")
}

/// Classifies gio mount error output into a structured MountError.
fn classify_mount_error(stderr: &str, server: &str, share: &str) -> MountError {
    let lower = stderr.to_lowercase();

    if lower.contains("already mounted") {
        // Shouldn't normally get here since we check first, but handle gracefully
        MountError::ProtocolError {
            message: format!("Share \"{}\" on \"{}\" is already mounted", share, server),
        }
    } else if lower.contains("no such") || lower.contains("not found") || lower.contains("doesn't exist") {
        MountError::ShareNotFound {
            message: format!("Share \"{}\" not found on \"{}\"", share, server),
        }
    } else if lower.contains("authentication") || lower.contains("password") || lower.contains("login") {
        if lower.contains("failed") || lower.contains("invalid") || lower.contains("incorrect") {
            MountError::AuthFailed {
                message: "Invalid username or password".to_string(),
            }
        } else {
            MountError::AuthRequired {
                message: "Authentication required".to_string(),
            }
        }
    } else if lower.contains("permission denied") || lower.contains("access denied") {
        MountError::PermissionDenied {
            message: format!("Permission denied for \"{}\" on \"{}\"", share, server),
        }
    } else if lower.contains("timed out") || lower.contains("timeout") {
        MountError::Timeout {
            message: format!("Connection to \"{}\" timed out", server),
        }
    } else if lower.contains("host is down")
        || lower.contains("unreachable")
        || lower.contains("connection refused")
        || lower.contains("no route")
    {
        MountError::HostUnreachable {
            message: format!("Can't connect to \"{}\"", server),
        }
    } else if lower.contains("cancelled") || lower.contains("canceled") {
        MountError::Cancelled {
            message: "Mount operation was cancelled".to_string(),
        }
    } else {
        MountError::ProtocolError {
            message: format!("Mount failed: {}", stderr.trim()),
        }
    }
}

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

/// Unmounts all SMB shares from a given host.
///
/// Linux GVFS unmount via `gio mount -u` is not wired up yet — returns empty.
pub fn unmount_smb_shares_from_host(_server_name: &str, _server_ip: Option<&str>) -> Vec<String> {
    log::debug!("unmount_smb_shares_from_host not yet implemented on Linux");
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_gvfs_path() {
        let path = derive_gvfs_path("MyNAS", "Documents");
        assert!(path.contains("smb-share:server=mynas,share=documents"));
        assert!(path.starts_with("/run/user/"));
    }

    #[test]
    fn test_escape_shell_arg() {
        assert_eq!(escape_shell_arg("simple"), "simple");
        assert_eq!(escape_shell_arg("it's"), "it'\\''s");
        assert_eq!(escape_shell_arg("p@ss!word"), "p@ss!word");
    }

    #[test]
    fn test_classify_mount_error_auth() {
        let err = classify_mount_error("Authentication failed", "server", "share");
        match err {
            MountError::AuthFailed { .. } => (),
            _ => panic!("Expected AuthFailed, got {:?}", err),
        }
    }

    #[test]
    fn test_classify_mount_error_unreachable() {
        let err = classify_mount_error("Host is down", "server", "share");
        match err {
            MountError::HostUnreachable { .. } => (),
            _ => panic!("Expected HostUnreachable, got {:?}", err),
        }
    }

    #[test]
    fn test_classify_mount_error_not_found() {
        let err = classify_mount_error("Share doesn't exist on server", "server", "share");
        match err {
            MountError::ShareNotFound { .. } => (),
            _ => panic!("Expected ShareNotFound, got {:?}", err),
        }
    }

    #[test]
    fn test_classify_mount_error_timeout() {
        let err = classify_mount_error("Connection timed out", "server", "share");
        match err {
            MountError::Timeout { .. } => (),
            _ => panic!("Expected Timeout, got {:?}", err),
        }
    }

    #[test]
    fn test_classify_mount_error_cancelled() {
        let err = classify_mount_error("Operation was cancelled", "server", "share");
        match err {
            MountError::Cancelled { .. } => (),
            _ => panic!("Expected Cancelled, got {:?}", err),
        }
    }

    #[test]
    fn test_classify_mount_error_generic() {
        let err = classify_mount_error("Something unexpected happened", "server", "share");
        match err {
            MountError::ProtocolError { .. } => (),
            _ => panic!("Expected ProtocolError, got {:?}", err),
        }
    }

    #[test]
    fn test_timeout_constant() {
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS >= 10_000) };
        const { assert!(DEFAULT_MOUNT_TIMEOUT_MS <= 60_000) };
    }
}
