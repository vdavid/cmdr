//! Windows: no OS mount yet (`WNetAddConnection2W` is the one to add) and no share-list CLI, so
//! each answers the way an OS without them does: a mount is refused, and smb2's own listing is
//! the only one.

use cmdr_smb::{ShareListError, ShareListResult};

use crate::network::os_mount::{MountError, MountResult};
use crate::network::server_identity::SmbServer;

/// No OS mount yet.
pub(crate) fn mount_share_sync(
    server: &str,
    share: &str,
    _username: Option<&str>,
    _password: Option<&str>,
    _port: u16,
) -> Result<MountResult, MountError> {
    Err(MountError::Unexpected {
        server: server.to_string(),
        share: share.to_string(),
        detail: "SMB mounting isn't supported on this platform".to_string(),
    })
}

/// Nothing Cmdr mounted, so nothing to unmount.
pub(crate) fn unmount_smb_shares_from_host(_targets: &[SmbServer]) -> Vec<String> {
    Vec::new()
}

/// No NetFS sign-in helper.
pub(crate) const CAN_WEDGE: bool = false;

/// No NetFS sign-in helper.
pub(crate) fn sign_in_helper_pid() -> Option<u32> {
    None
}

/// No share-list CLI yet.
pub(crate) async fn list_shares_cli_as_guest(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::ProtocolError {
        message: "No share listing fallback available on this platform".to_string(),
    })
}

/// No share-list CLI yet.
pub(crate) async fn list_shares_cli_with_keychain(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
) -> Result<ShareListResult, ShareListError> {
    Err(ShareListError::AuthRequired {
        message: "Keychain authentication not available on this platform".to_string(),
    })
}

/// No share-list CLI yet.
pub(crate) async fn list_shares_cli_with_password(
    _hostname: &str,
    _ip_address: Option<&str>,
    _port: u16,
    _username: &str,
    _password: &str,
) -> Option<Result<ShareListResult, ShareListError>> {
    Some(Err(ShareListError::ProtocolError {
        message: "No share listing fallback available on this platform".to_string(),
    }))
}
