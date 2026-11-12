//! Linux: GVFS's `gio mount` (`mount.rs`), `smbclient -L` for share lists (`smbclient.rs`), and
//! no NetFS sign-in helper to wedge.

mod distro;
mod mount;
mod smbclient;

use cmdr_smb::{ShareListError, ShareListResult};

pub(crate) use mount::{mount_share_sync, unmount_smb_shares_from_host};

/// `gio mount` keeps the plain answers: no sign-in helper sits between it and the server.
pub(crate) const CAN_WEDGE: bool = false;

/// A GVFS mount (or a CIFS one the user made), and no Local Network permission: only a firewall
/// refuses a route.
pub(crate) const MOUNT_LOG_WORDS: super::MountLogWords = super::MountLogWords {
    mount: "the OS mount (GVFS or CIFS)",
    machine: "this computer",
    route_blockers: "a firewall rule",
};

/// No sign-in helper.
pub(crate) fn sign_in_helper_pid() -> Option<u32> {
    None
}

/// Linux fallback: delegate to `smbclient -L` (guest access).
pub(crate) async fn list_shares_cli_as_guest(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
) -> Result<ShareListResult, ShareListError> {
    use log::debug;
    let host = ip_address.unwrap_or(hostname);
    debug!(
        "Trying share-list fallback: host={:?}, port={}, backend=smbclient, auth=guest",
        cmdr_fs::log_private::LogPrivate::host(host),
        port
    );

    let shares = smbclient::run_smbclient_list(host, port, None).await?;
    Ok(ShareListResult {
        shares,
        auth_mode: cmdr_smb::AuthMode::GuestAllowed,
        from_cache: false,
    })
}

/// Linux fallback: no Keychain equivalent; return AuthRequired so the user gets a login prompt.
pub(crate) async fn list_shares_cli_with_keychain(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::AuthRequired {
        message: "Stored credential lookup not available via smbclient fallback".to_string(),
    })
}

/// `smbclient -L -A <authfile>`: the password rides in a 0o600 temp file, never argv.
pub(crate) async fn list_shares_cli_with_password(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
) -> Option<Result<ShareListResult, ShareListError>> {
    Some(smbclient_with_password(hostname, ip_address, port, username, password).await)
}

/// `smbclient -L -U` (authenticated access).
async fn smbclient_with_password(
    hostname: &str,
    ip_address: Option<&str>,
    port: u16,
    username: &str,
    password: &str,
) -> Result<ShareListResult, ShareListError> {
    use log::debug;
    let host = ip_address.unwrap_or(hostname);
    debug!(
        "Trying share-list fallback: host={:?}, port={}, user={:?}, backend=smbclient, auth=credentials",
        cmdr_fs::log_private::LogPrivate::host(host),
        port,
        cmdr_fs::log_private::LogPrivate::user(username)
    );

    let shares = smbclient::run_smbclient_list(host, port, Some((username, password))).await?;
    Ok(ShareListResult {
        shares,
        auth_mode: cmdr_smb::AuthMode::CredsRequired,
        from_cache: false,
    })
}
