//! SMB share mounting on Linux using GVFS (`gio mount`).
//!
//! Uses the `gio mount` command for user-space SMB mounting, which works on
//! GNOME/GTK desktops without requiring root privileges. Mounts appear under
//! `/run/user/<uid>/gvfs/` or a similar GVFS-managed path.

use log::debug;
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Result of a successful mount operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MountResult {
    /// Path to the mounted share (for example, "/run/user/1000/gvfs/smb-share:...").
    pub mount_path: String,
    pub already_mounted: bool,
}

/// Errors that can occur during mount operations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

/// Extracts the `(server, share)` of any `smb://` mount URL on one `gio mount -l` line.
///
/// Lines look like `  Mount(0): docs on naspolya -> smb://naspolya/docs/`. The server is
/// stripped of any `user@` prefix and `:port` suffix so it compares cleanly against a
/// discovered host. Returns `None` for non-SMB or malformed lines.
fn parse_smb_mount(line: &str) -> Option<(String, String)> {
    let rest = line.split("smb://").nth(1)?;
    let url = rest.split_whitespace().next()?.trim_end_matches('/');
    let (host_part, share) = url.split_once('/')?;
    let host = host_part.rsplit('@').next().unwrap_or(host_part);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() || share.is_empty() {
        return None;
    }
    Some((host.to_string(), share.to_string()))
}

/// Scans `gio mount -l` output for a mount of `share` on a server that is the same
/// identity as `server` (mDNS name ↔ `.local` hostname ↔ IP, via the discovery state).
/// Returns the server name as it appears in the existing mount, so the caller can derive
/// the matching GVFS path. Identity-aware so a share mounted under one name (for example
/// by Nautilus using the hostname) is recognized when we look it up by another (the IP).
fn match_existing_smb_mount(
    stdout: &str,
    server: &str,
    share: &str,
    hosts: &[crate::network::NetworkHost],
) -> Option<String> {
    for line in stdout.lines() {
        if let Some((mount_server, mount_share)) = parse_smb_mount(line)
            && mount_share.eq_ignore_ascii_case(share)
            && crate::network::server_identity::same_server(&mount_server, server, hosts)
        {
            return Some(mount_server);
        }
    }
    None
}

/// Finds the GVFS mount path for an SMB share.
/// Checks `gio mount -l` output for an existing mount matching the server/share.
fn find_existing_mount(server: &str, share: &str) -> Option<String> {
    let output = Command::new("gio").args(["mount", "-l"]).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let hosts = crate::network::get_discovered_hosts();
    let mount_server = match_existing_smb_mount(&stdout, server, share, &hosts)?;
    // Derive from the server name the existing mount actually uses, so the GVFS path
    // matches even when it was mounted under a different alias than we looked up.
    Some(derive_gvfs_path(&mount_server, share))
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

    let output = run_gio_mount(&smb_url, username, password)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(classify_mount_error(&stderr, server, share));
    }

    // After successful mount, find the mount path
    let mount_path = find_existing_mount(server, share).unwrap_or_else(|| derive_gvfs_path(server, share));

    Ok(MountResult {
        mount_path,
        already_mounted: false,
    })
}

/// Runs `gio mount <url>`, feeding the password (when present) through the child's
/// stdin instead of a shell command line.
///
/// `gio mount` reads the password interactively from stdin when the URL carries a
/// username. The previous implementation piped it via `sh -c "echo 'PASS' | gio …"`,
/// which placed the cleartext password in the process argument list (`ps` /
/// `/proc/<pid>/cmdline`) for the lifetime of the call — the same argv leak the macOS
/// smbutil path is careful to avoid. Spawning `gio` directly and writing the password
/// to its stdin keeps the secret off argv entirely. A mount with no username is a
/// guest/anonymous mount (`--anonymous`), which never prompts, so no stdin is needed.
fn run_gio_mount(
    smb_url: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<std::process::Output, MountError> {
    use std::io::Write;
    use std::process::Stdio;

    let fail = |e: std::io::Error| MountError::ProtocolError {
        message: format!("Failed to run gio mount: {}", e),
    };

    let mut cmd = Command::new("gio");
    // `LC_ALL=C` keeps stderr English so `classify_mount_error` matches.
    cmd.env("LC_ALL", "C").args(["mount", smb_url]);
    if username.is_none() {
        cmd.arg("--anonymous");
    }

    // Only an authenticated mount needs a password, and gio reads it (one line) from
    // stdin. Everything else gets a null stdin so gio can't block waiting on input.
    let password = username.and(password);
    cmd.stdin(if password.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(fail)?;

    if let Some(pass) = password {
        let mut stdin = child.stdin.take().expect("stdin is piped when a password is present");
        // Best-effort: if gio closed stdin early, `wait_with_output` surfaces the real
        // error. Dropping `stdin` after the write sends EOF so gio stops reading.
        let _ = stdin.write_all(format!("{}\n", pass).as_bytes());
    }

    child.wait_with_output().map_err(fail)
}

/// Classifies `gio mount` stderr into a structured `MountError`.
///
/// `gio mount` has no granular exit codes (every failure exits 1) and no
/// machine-readable error channel. Its stderr is the only signal we get, so
/// this is the canonical "third-party CLI with no typed error surface" case
/// the no-error-string-match rule's opt-out exists for. The subprocess MUST
/// be run with `LC_ALL=C` so stderr stays English; otherwise the substring
/// table below would silently miss-classify on localized systems. See
/// `classify_mount_error_snapshot_*` tests for the pinned wording per
/// `gio` / `glib` version we currently support.
fn classify_mount_error(stderr: &str, server: &str, share: &str) -> MountError {
    /// One phrase from `gio mount`'s English stderr.
    type Needle = &'static str;
    let needles_lower = stderr.to_lowercase();
    // Lookup helper: kept private to this fn so future callers can't smuggle
    // their own free-form classification through it. String matching is unavoidable
    // here: `gio mount` (glib) gives no exit-code granularity and no typed error
    // output. English is forced via `LC_ALL=C` on the subprocess, and the snapshot
    // tests `classify_mount_error_snapshot_*` pin the matched phrases. (The fn doc
    // covers the full rationale; if a flagged shape ever lands here, re-add the
    // `allowed-error-string-match:` opt-out on the exact line that trips.)
    let has_any = |phrases: &[Needle]| -> bool { phrases.iter().any(|p| needles_lower.contains(p)) };

    // Order matters: the auth-required check has to run after the auth-failed
    // pre-check otherwise "Authentication failed" matches the broader bucket.
    let already_mounted: &[Needle] = &["already mounted"];
    let not_found: &[Needle] = &["no such", "not found", "doesn't exist"];
    let auth_words: &[Needle] = &["authentication", "password", "login"];
    let failed_words: &[Needle] = &["failed", "invalid", "incorrect"];
    let permission: &[Needle] = &["permission denied", "access denied"];
    let timeout: &[Needle] = &["timed out", "timeout"];
    let unreachable: &[Needle] = &["host is down", "unreachable", "connection refused", "no route"];
    let cancelled: &[Needle] = &["cancelled", "canceled"];

    if has_any(already_mounted) {
        // Shouldn't normally get here since we check first, but handle gracefully
        MountError::ProtocolError {
            message: format!("Share \"{}\" on \"{}\" is already mounted", share, server),
        }
    } else if has_any(not_found) {
        MountError::ShareNotFound {
            message: format!("Share \"{}\" not found on \"{}\"", share, server),
        }
    } else if has_any(auth_words) {
        if has_any(failed_words) {
            MountError::AuthFailed {
                message: "Invalid username or password".to_string(),
            }
        } else {
            MountError::AuthRequired {
                message: "Authentication required".to_string(),
            }
        }
    } else if has_any(permission) {
        MountError::PermissionDenied {
            message: format!("Permission denied for \"{}\" on \"{}\"", share, server),
        }
    } else if has_any(timeout) {
        MountError::Timeout {
            message: format!("Connection to \"{}\" timed out", server),
        }
    } else if has_any(unreachable) {
        MountError::HostUnreachable {
            message: format!("Can't connect to \"{}\"", server),
        }
    } else if has_any(cancelled) {
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
/// Linux GVFS unmount via `gio mount -u` is not wired up yet; returns empty.
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
    fn test_parse_smb_mount_line() {
        // The shape `gio mount -l` emits.
        assert_eq!(
            parse_smb_mount("  Mount(0): docs on naspolya -> smb://naspolya/docs/"),
            Some(("naspolya".to_string(), "docs".to_string()))
        );
        // user@ and :port decorations are stripped from the server.
        assert_eq!(
            parse_smb_mount("Mount(1): x -> smb://david@192.168.1.111:445/naspi"),
            Some(("192.168.1.111".to_string(), "naspi".to_string()))
        );
        // Non-SMB and malformed lines yield nothing.
        assert_eq!(parse_smb_mount("Drive(0): SSD"), None);
        assert_eq!(parse_smb_mount("-> smb://serveronly"), None);
    }

    /// The existing-mount lookup must recognize a share already mounted under a different
    /// name for the same server (for example, Nautilus mounted it by hostname while we
    /// look it up by IP). Identity comes from the discovery state, mirroring macOS.
    #[test]
    fn test_match_existing_smb_mount_is_identity_aware() {
        use crate::network::{HostSource, NetworkHost};
        let hosts = [NetworkHost {
            id: "naspolya".into(),
            name: "Naspolya".into(),
            hostname: Some("naspolya.local".into()),
            ip_address: Some("192.168.1.111".into()),
            port: 445,
            source: HostSource::Discovered,
        }];
        let lines = "Mount(0): naspi on naspolya -> smb://naspolya.local/naspi/";

        // Looking up by IP finds the hostname-mounted share via discovery identity.
        let hit = match_existing_smb_mount(lines, "192.168.1.111", "naspi", &hosts);
        assert_eq!(hit.as_deref(), Some("naspolya.local"), "expected identity match by IP");

        // A genuinely different share name does not match.
        assert!(match_existing_smb_mount(lines, "192.168.1.111", "other", &hosts).is_none());
        // A different server (no identity link) does not match.
        assert!(match_existing_smb_mount(lines, "192.168.1.150", "naspi", &[]).is_none());
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

    // ── `gio mount` stderr snapshots ────────────────────────────────────────
    //
    // These pin the actual stderr wording `gio mount` (glib 2.74+) emits on
    // Ubuntu / Debian / Fedora with `LC_ALL=C`. Captured from a one-shot run
    // against `gvfs 1.54.x`. If a new glib version reshapes the wording, these
    // tests fail loudly so we update `classify_mount_error` (the opt-out site
    // for the no-error-string-match rule) before the change ships.

    #[test]
    fn classify_mount_error_snapshot_auth_required_empty_password() {
        // glib emits this when the server requires auth and we sent anonymous.
        let stderr = "Error mounting location: Password required to access the share";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::AuthRequired { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_auth_failed_invalid_credentials() {
        let stderr = "Error mounting location: Authentication failed: invalid login or password";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::AuthFailed { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_share_not_found_no_such_file() {
        let stderr = "Error mounting location: No such file or directory";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::ShareNotFound { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_permission_denied_explicit() {
        let stderr = "Error mounting location: Permission denied";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::PermissionDenied { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_host_unreachable_no_route() {
        let stderr = "Error mounting location: Failed to connect to server: No route to host";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::HostUnreachable { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_host_unreachable_connection_refused() {
        let stderr = "Error mounting location: Connection refused";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::HostUnreachable { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_timeout_explicit() {
        let stderr = "Error mounting location: Connection timed out";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::Timeout { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_cancelled_by_user() {
        let stderr = "Error mounting location: Operation was cancelled";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::Cancelled { .. }
        ));
    }

    #[test]
    fn classify_mount_error_snapshot_already_mounted_fallback() {
        let stderr = "Error mounting location: Location is already mounted";
        assert!(matches!(
            classify_mount_error(stderr, "server", "share"),
            MountError::ProtocolError { .. }
        ));
    }
}
