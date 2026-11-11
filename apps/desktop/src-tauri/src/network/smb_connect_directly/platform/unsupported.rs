//! The answer where the OS keeps no saved SMB passwords Cmdr can borrow: nothing saved. Linux and
//! Windows re-export it.

use super::super::{CredentialsNeededReason, MountedShare, UpgradeResult, claim_mounted_share, credentials_needed};
use crate::network::smb_server_address::friendly_server_name;

/// Nothing saved, so there's nothing to offer.
pub(crate) async fn system_has_saved_password(_volume_id: &str) -> bool {
    false
}

/// No system keychain to borrow from here, which is the macOS answer for "nothing
/// saved": ask for the password on the sign-in sheet.
pub(crate) async fn connect_directly_with_system_saved_password(volume_id: &str) -> UpgradeResult {
    match claim_mounted_share(volume_id).await {
        Ok(MountedShare { info, .. }) => {
            let display_name = friendly_server_name(&info.server);
            let hint = info.username.clone();
            credentials_needed(info, display_name, hint, CredentialsNeededReason::NoCredential)
        }
        Err(answer) => answer,
    }
}
