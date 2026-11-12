//! `smbutil view`, the share-list fallback when smb2 can't list a host's shares (some Samba
//! servers have RPC compatibility issues with it).

use cmdr_smb::{ShareInfo, ShareListError, ShareListResult};
use log::debug;

use super::super::smbutil_output::{SmbutilOutcome, classify_smbutil_stderr, parse_smbutil_output};

/// Lists shares using macOS smbutil command as fallback.
/// This works with Samba servers that have RPC compatibility issues with smb-rs.
pub(crate) async fn list_shares_cli_as_guest(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<ShareListResult, ShareListError> {
    let url = build_smbutil_url(hostname, ip_address, port);
    debug!(
        "Running smbutil view: host={:?}, port={}, backend=smbutil, auth=guest",
        cmdr_fs::log_private::LogPrivate::host(hostname),
        port
    );

    let shares = run_smbutil_view(&url, true).await?;

    Ok(ShareListResult {
        shares,
        auth_mode: cmdr_smb::AuthMode::GuestAllowed,
        from_cache: false,
    })
}

/// Lists shares using macOS Keychain credentials via smbutil.
/// This runs smbutil WITHOUT explicit credentials, allowing it to use
/// credentials stored in macOS Keychain (for example, from previous Finder connections).
/// No -G (guest) flag, so smbutil will try to authenticate using Keychain.
pub(crate) async fn list_shares_cli_with_keychain(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<ShareListResult, ShareListError> {
    let url = build_smbutil_url(hostname, ip_address, port);
    debug!(
        "Running smbutil view: host={:?}, port={}, backend=smbutil, auth=keychain",
        cmdr_fs::log_private::LogPrivate::host(hostname),
        port
    );

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
        auth_mode: cmdr_smb::AuthMode::CredsRequired, // User is authenticated via Keychain
        from_cache: false,
    })
}

/// Builds a passwordless SMB URL for smbutil commands (guest or Keychain-backed).
///
/// Cmdr never embeds credentials in the smbutil URL: the cleartext password would land in the
/// `ps`-readable argv. So this only ever builds `//host` / `//host:port`.
fn build_smbutil_url(hostname: &str, ip_address: Option<&str>, port: u16) -> String {
    // Prefer hostname over loopback IPs: macOS smbutil fails with //127.0.0.1:PORT
    // ("Broken pipe") but works with //localhost:PORT on non-standard ports.
    let host = match ip_address {
        Some(ip) if ip == "127.0.0.1" || ip == "::1" => hostname,
        Some(ip) => ip,
        None => hostname,
    };

    if port == 445 {
        format!("//{}", host)
    } else {
        format!("//{}:{}", host, port)
    }
}

/// How long `smbutil view` gets before it's stopped.
///
/// This is the LAST thing tried when smb2 couldn't list a host's shares, and the
/// share browser's spinner is waiting on it. `smbutil` has no timeout flag and
/// never gives up on its own against a server that accepted the connection and
/// then went quiet, so this is the only thing standing between that server and a
/// spinner that spins until the user closes the window. Set well above the ~1 s a
/// working `smbutil` takes on a slow LAN: the generous number costs nothing on
/// the healthy path and only decides how long a dead one is allowed to look
/// alive.
const SMBUTIL_VIEW_LIMIT: std::time::Duration = std::time::Duration::from_secs(20);

/// Runs smbutil view command and returns parsed shares.
/// `use_guest` controls whether to use -G flag (guest access).
async fn run_smbutil_view(url: &str, use_guest: bool) -> Result<Vec<ShareInfo>, ShareListError> {
    // C locale so `classify_smbutil_stderr` keeps matching English strings,
    // whatever the user's system locale is.
    let mut cmd = crate::subprocess::command_in_c_locale("smbutil");
    cmd.arg("view").arg("-N");
    if use_guest {
        cmd.arg("-G");
    }
    // `url` is always passwordless (`//host` / `//host:port`): Cmdr never embeds
    // credentials in the smbutil URL because the cleartext password would leak into the
    // `ps`-readable argv, and `smbutil view` has no argv-free password channel. Authenticated
    // share listing rides smb2 (and on Linux, smbclient's 0o600 `-A` authfile); guest and
    // Keychain-backed listings both pass `-N` with no password. So nothing secret rides argv.
    cmd.arg(url);

    let output = crate::subprocess::output_within("smbutil view", SMBUTIL_VIEW_LIMIT, &mut cmd)
        .await
        .map_err(|e| ShareListError::ProtocolError {
            message: format!("smbutil view didn't answer: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!(
            "smbutil share listing stopped: source=cli, backend=smbutil, error_kind=exit, code={:?}, stderr={:?}, stdout={:?}",
            output.status.code(),
            cmdr_fs::log_detail::LogDetail(&stderr),
            cmdr_fs::log_detail::LogDetail(&stdout)
        );

        return match classify_smbutil_stderr(&stderr) {
            SmbutilOutcome::AuthRequired => Err(ShareListError::AuthRequired {
                message: "smbutil: Authentication required".to_string(),
            }),
            SmbutilOutcome::ProtocolError(msg) => Err(ShareListError::ProtocolError {
                message: format!("smbutil failed: {}", msg),
            }),
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_smbutil_output(&stdout))
}

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn build_smbutil_url_uses_ip_when_not_loopback() {
        let url = build_smbutil_url("testnas.local", Some("192.168.1.50"), 445);
        assert_eq!(url, "//192.168.1.50");
    }

    #[test]
    fn build_smbutil_url_falls_back_to_hostname_for_127_0_0_1() {
        let url = build_smbutil_url("localhost", Some("127.0.0.1"), 9445);
        assert_eq!(url, "//localhost:9445");
    }

    #[test]
    fn build_smbutil_url_falls_back_to_hostname_for_ipv6_loopback() {
        let url = build_smbutil_url("localhost", Some("::1"), 9445);
        assert_eq!(url, "//localhost:9445");
    }

    #[test]
    fn build_smbutil_url_uses_hostname_when_no_ip() {
        let url = build_smbutil_url("mynas.local", None, 445);
        assert_eq!(url, "//mynas.local");
    }

    #[test]
    fn build_smbutil_url_omits_port_for_445() {
        let url = build_smbutil_url("testnas.local", Some("10.0.0.5"), 445);
        assert_eq!(url, "//10.0.0.5");
    }

    #[test]
    fn build_smbutil_url_includes_non_standard_port() {
        let url = build_smbutil_url("testnas.local", Some("10.0.0.5"), 9445);
        assert_eq!(url, "//10.0.0.5:9445");
    }

    /// The URL builder no longer accepts credentials: Cmdr never embeds a password in the
    /// smbutil URL (it would leak into argv). This test pins that the built URL stays
    /// passwordless regardless of host/port shape.
    #[test]
    fn build_smbutil_url_never_carries_credentials() {
        let url = build_smbutil_url("testnas.local", Some("10.0.0.5"), 9446);
        assert!(!url.contains('@'), "smbutil URL must never embed credentials: {url}");
        assert_eq!(url, "//10.0.0.5:9446");
    }
}
