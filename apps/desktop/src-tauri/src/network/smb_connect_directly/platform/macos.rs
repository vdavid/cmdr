//! macOS: the login keychain, where Finder saves the SMB passwords it mounts with, read through
//! `crate::secrets::system_keychain_smb`.

use super::super::{
    CredentialsNeededReason, HOSTNAME_WAIT, MountedShare, UpgradeResult, claim_mounted_share, credentials_needed,
    find_mounted_share_within, read_mount,
};
use crate::network::keychain;
use crate::network::smb_connect_failure::UpgradeError;
use crate::network::smb_server_address::{discover_server, friendly_server_name};
use crate::network::smb_upgrade::{MOUNT_READ_LIMIT, try_smb_upgrade};

/// Finds the OS-mounted SMB share behind `volume_id`, or the answer to give
/// without dialing anything: `Success` for a volume that's already direct,
/// `VolumeGone` or `NotSmbMount` for one there's nothing to upgrade on, and
/// `MountNotResponding` for a mount that didn't answer in time.
///
/// Only the saved-password probe reads without asking (macOS alone has one); every
/// door that asks goes through [`claim_mounted_share`].
async fn find_mounted_share(volume_id: &str) -> Result<MountedShare, UpgradeResult> {
    find_mounted_share_within(volume_id, MOUNT_READ_LIMIT, read_mount).await
}

/// Upgrades `volume_id` with the password Finder saved in the login keychain, so
/// the user doesn't retype it, and copies it into Cmdr's own store on success so
/// the next reconnect is silent.
///
/// Reading it raises the macOS consent dialog (the frontend primes the user first;
/// the dialog's own text can't be customized), so this is user-initiated only: ❌
/// never call it at startup. Nothing readable, or access denied, answers
/// `CredentialsNeeded`, which falls back to the sign-in sheet.
pub(crate) async fn connect_directly_with_system_saved_password(volume_id: &str) -> UpgradeResult {
    use crate::network::smb_server_address::system_keychain_aliases;
    use crate::secrets::system_keychain_smb;

    let MountedShare { mount_path, info } = match claim_mounted_share(volume_id).await {
        Ok(share) => share,
        Err(answer) => return answer,
    };

    let discovered = discover_server(&info.server, HOSTNAME_WAIT).await;
    let hostname = discovered.hostname.as_deref();
    let display_name = friendly_server_name(&info.server);

    // Finder keys its entry by the name form it mounted with, often the mDNS
    // service name, so the query tries every alias discovery knows.
    let aliases = system_keychain_aliases(&info.server);
    let candidates = system_keychain_smb::server_query_candidates(&info.server, hostname, &aliases);

    // The read raises the consent dialog and blocks on the user: keep it off the
    // async worker pool.
    let creds = tokio::task::spawn_blocking(move || system_keychain_smb::read_password(&candidates))
        .await
        .ok()
        .flatten();
    let Some(creds) = creds else {
        let hint = info.username.clone();
        return credentials_needed(info, display_name, hint, CredentialsNeededReason::NoCredential);
    };

    let upgraded = try_smb_upgrade(
        &info,
        &mount_path,
        Some(&creds.username),
        Some(&creds.password),
        volume_id,
    )
    .await;
    match upgraded {
        Ok(()) => {
            // Keyed by hostname when known, else the server, like a typed password.
            let server_key = &crate::network::server_identity::smb_server(hostname.unwrap_or(&info.server), info.port);
            if let Err(e) = keychain::save_credentials(server_key, Some(&info.share), &creds.username, &creds.password)
            {
                log::warn!("Couldn't copy borrowed credentials into Cmdr's store: {}", e);
            }
            UpgradeResult::Success
        }
        Err(UpgradeError::Refused(refusal)) => {
            credentials_needed(info, display_name, Some(creds.username), refusal.into())
        }
        Err(UpgradeError::Network { reason, display_name }) => UpgradeResult::NetworkError { reason, display_name },
    }
}

/// Whether the login keychain holds an SMB password another app (Finder) saved for
/// `volume_id`'s server, so the frontend can decide whether to offer it.
///
/// An attributes-only read that ❌ never raises the consent dialog. `false` for
/// anything short of an OS-mounted share that answered, a mount that isn't
/// responding included: the offer just doesn't appear.
pub(crate) async fn system_has_saved_password(volume_id: &str) -> bool {
    use crate::network::smb_server_address::system_keychain_aliases;
    use crate::secrets::system_keychain_smb;

    let Ok(MountedShare { info, .. }) = find_mounted_share(volume_id).await else {
        return false;
    };
    // Whatever the discovery state already knows: this doesn't warm mDNS just to probe.
    let aliases = system_keychain_aliases(&info.server);
    let candidates = system_keychain_smb::server_query_candidates(&info.server, None, &aliases);
    // Prompt-free and fast, but still FFI, so it stays off the async worker.
    tokio::task::spawn_blocking(move || system_keychain_smb::account_for_any(&candidates).is_some())
        .await
        .unwrap_or(false)
}
