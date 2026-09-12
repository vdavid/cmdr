//! "Connect directly": upgrading an OS-mounted SMB share to a direct smb2 session
//! because someone asked, and answering with where that left the volume.
//!
//! Three doors, one per place the password comes from: Cmdr's own store
//! ([`connect_directly`]), the sign-in sheet ([`connect_directly_with_credentials`]),
//! and the password Finder saved in the login keychain
//! ([`connect_directly_with_system_saved_password`]). The Tauri commands in
//! `commands::network` pass straight through; the MCP `upgrade_smb_to_direct` tool
//! and the indexer's `ensure_direct_smb` call [`connect_directly`] too.
//!
//! Nothing here takes an `AppHandle`, so the MCP executor (generic over `Runtime`)
//! can call it. The commands kick mDNS before delegating; MCP relies on something
//! else having started it.
//!
//! The auto-upgrade paths (the startup pass, the mount watcher) answer nobody, so
//! they go through `smb_upgrade::register_smb_volume` instead, and it's their
//! failure the OS-mount notice announces.

use crate::deadline::blocking_with_timeout;
use crate::file_system::volume::manager::get_volume_manager;
use crate::network::keychain;
use crate::network::smb_connect_failure::{Refusal, RefusedAt, SignInIdentity, UpgradeError, UpgradeFailure};
use crate::network::smb_server_address::{
    friendly_server_name, get_keychain_password, resolve_ip_to_hostname_with_wait,
};
use crate::network::smb_upgrade::{MOUNT_READ_LIMIT, try_smb_upgrade};
#[cfg(target_os = "macos")]
use crate::volumes::{SmbMountInfo, get_smb_mount_info};
#[cfg(target_os = "linux")]
use crate::volumes_linux::{SmbMountInfo, get_smb_mount_info};
use std::time::Duration;

/// How long an upgrade gives mDNS to name the server before looking up saved
/// credentials by its address alone.
const HOSTNAME_WAIT: Duration = Duration::from_millis(1500);

/// Where a "Connect directly" left the volume.
///
/// Every variant is an answer the caller words, a vanished volume included: a
/// share going away between the offer and the press (an unmount, an eject, a
/// network drop) is an ordinary outcome, not a breakdown. Flattened into a string
/// `Err`, it once left the OS-mount notice offering a retry on a volume that no
/// longer existed, press after press (ERR-SHUSC).
#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum UpgradeResult {
    /// The volume uses direct smb2 now, or already did.
    Success,
    /// Credentials needed: the caller asks for them.
    CredentialsNeeded {
        server: String,
        share: String,
        port: u16,
        /// Friendly display name for the server (mDNS hostname or IP).
        display_name: String,
        /// Username hint: the account a refused attempt went out as, else the OS
        /// mount's.
        username_hint: Option<String>,
        /// Why a credential is needed, which decides what the sign-in sheet says
        /// first.
        reason: CredentialsNeededReason,
    },
    /// Couldn't reach the server (DNS, network, unreachable, too slow).
    NetworkError {
        reason: UpgradeFailure,
        /// Friendly server name for the frontend to name in its copy.
        display_name: String,
    },
    /// No volume is registered under this id anymore, or nothing is mounted at its
    /// root: the share was unmounted, ejected, or dropped off the network. There's
    /// no connection left to upgrade, and no retry can make one.
    VolumeGone,
    /// Something is mounted at the volume's root, but not an SMB share, so there's
    /// nothing to upgrade.
    NotSmbMount,
    /// The OS mount behind the volume didn't answer a status read in time: its
    /// server went quiet, or the network dropped without the mount noticing yet.
    /// Nothing was dialed, and a later press may work.
    MountNotResponding,
}

/// Why "Connect directly" needs a credential.
///
/// Word-free, like `UpgradeFailure`: the frontend maps it to a `ConnectRefusalKind`
/// and the catalog words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CredentialsNeededReason {
    /// Nothing was offered: no credential is saved for the share, or the attempt
    /// went out as a guest and was turned away.
    NoCredential,
    /// The server didn't accept the account's password at sign-in.
    CredentialRejected,
    /// The account signed in, and the share doesn't let it open. The password is
    /// the one thing known to be right, so the fix is a different account.
    AccountNotPermitted,
}

impl From<Refusal> for CredentialsNeededReason {
    /// A guest turned away at either step offered nothing, and an account is the
    /// next step either way, so both ask for a sign-in. Only an account's refusal
    /// says something about the credential it offered.
    fn from(refusal: Refusal) -> Self {
        match (refusal.identity, refusal.at) {
            (SignInIdentity::Guest, _) => Self::NoCredential,
            (SignInIdentity::Account, RefusedAt::SignIn) => Self::CredentialRejected,
            (SignInIdentity::Account, RefusedAt::Share) => Self::AccountNotPermitted,
        }
    }
}

/// The OS mount a direct session would take over from.
#[derive(Debug)]
struct MountedShare {
    mount_path: String,
    info: SmbMountInfo,
}

/// What the OS says is at a volume's root.
#[derive(Debug)]
enum MountRead {
    /// An SMB mount, and what the OS records about it.
    Smb(SmbMountInfo),
    /// Something else is mounted there, or a plain directory was left behind.
    NotSmb,
    /// Nothing is there: macOS removes a `/Volumes` mount point along with its
    /// mount.
    Gone,
}

/// Reads what's mounted at `mount_path`.
///
/// Blocking, and for as long as a network mount's server stays quiet: `statfs` and
/// `stat` on a hung mount wait 30-120 s.
fn read_mount(mount_path: &str) -> MountRead {
    if let Some(info) = get_smb_mount_info(mount_path) {
        return MountRead::Smb(info);
    }
    // No SMB mount there. Whether the root exists at all is what separates a share
    // that just went away from a volume that never was one.
    if std::path::Path::new(mount_path).exists() {
        MountRead::NotSmb
    } else {
        MountRead::Gone
    }
}

/// Finds the OS-mounted SMB share behind `volume_id`, or the answer to give
/// without dialing anything: `Success` for a volume that's already direct,
/// `VolumeGone` or `NotSmbMount` for one there's nothing to upgrade on, and
/// `MountNotResponding` for a mount that didn't answer in time.
async fn find_mounted_share(volume_id: &str) -> Result<MountedShare, UpgradeResult> {
    find_mounted_share_within(volume_id, MOUNT_READ_LIMIT, read_mount).await
}

/// [`find_mounted_share`], with the mount read and its limit passed in so a test
/// can stand in a mount that never answers.
async fn find_mounted_share_within(
    volume_id: &str,
    limit: Duration,
    read: impl FnOnce(&str) -> MountRead + Send + 'static,
) -> Result<MountedShare, UpgradeResult> {
    let Some(volume) = get_volume_manager().get(volume_id) else {
        return Err(UpgradeResult::VolumeGone);
    };
    if volume.backend_kind() == cmdr_fs::volume::BackendKind::Smb {
        return Err(UpgradeResult::Success);
    }
    let mount_path = volume.root().to_string_lossy().to_string();
    let path = mount_path.clone();
    // ❗ Only this read is bounded, ❌ never the whole flow: the saved-password door
    // goes on to wait for the person's answer to the Keychain consent dialog.
    let Some(mount) = blocking_with_timeout(limit, None, move || Some(read(&path))).await else {
        log::warn!("The mount at {mount_path} didn't answer a status read within {limit:?}; not dialing");
        return Err(UpgradeResult::MountNotResponding);
    };
    match mount {
        MountRead::Smb(info) => Ok(MountedShare { mount_path, info }),
        MountRead::NotSmb => Err(UpgradeResult::NotSmbMount),
        MountRead::Gone => Err(UpgradeResult::VolumeGone),
    }
}

/// The `CredentialsNeeded` answer for the share `info` describes.
fn credentials_needed(
    info: SmbMountInfo,
    display_name: String,
    username_hint: Option<String>,
    reason: CredentialsNeededReason,
) -> UpgradeResult {
    UpgradeResult::CredentialsNeeded {
        server: info.server,
        share: info.share,
        port: info.port,
        display_name,
        username_hint,
        reason,
    }
}

/// Upgrades `volume_id` with the credentials Cmdr stored for its share, or answers
/// `CredentialsNeeded` when there are none or the server turns them down.
pub(crate) async fn connect_directly(volume_id: &str) -> UpgradeResult {
    let MountedShare { mount_path, info } = match find_mounted_share(volume_id).await {
        Ok(share) => share,
        Err(answer) => return answer,
    };
    log::info!(
        "Upgrading volume {} to SmbVolume: server={}, share={}, user={:?}",
        volume_id,
        info.server,
        info.share,
        info.username
    );

    // The mount source carries the IP, but Cmdr keys its Keychain entries by the
    // mDNS hostname, so look under both. The short wait gives mDNS a chance to
    // warm up, so nobody is asked for a password they already saved.
    let hostname = resolve_ip_to_hostname_with_wait(&info.server, HOSTNAME_WAIT).await;
    let display_name = friendly_server_name(&info.server);
    let Some((username, password)) = get_keychain_password(&info.server, hostname.as_deref(), &info.share).await else {
        log::info!("No stored credentials found, requesting credentials from user");
        let hint = info.username.clone();
        return credentials_needed(info, display_name, hint, CredentialsNeededReason::NoCredential);
    };
    log::info!("Found Keychain credentials for user={}", username);

    let upgraded = try_smb_upgrade(&info, &mount_path, Some(&username), Some(&password), volume_id).await;
    match upgraded {
        Ok(()) => UpgradeResult::Success,
        Err(UpgradeError::Refused(refusal)) => {
            log::info!("Stored credentials didn't work ({refusal:?}), requesting new credentials");
            credentials_needed(info, display_name, Some(username), refusal.into())
        }
        Err(UpgradeError::Network { reason, display_name }) => UpgradeResult::NetworkError { reason, display_name },
    }
}

/// Upgrades `volume_id` with the credentials the user typed into the sign-in
/// sheet, saving them to the Keychain on success when `remember_in_keychain` asks.
pub(crate) async fn connect_directly_with_credentials(
    volume_id: &str,
    username: Option<String>,
    password: Option<String>,
    remember_in_keychain: bool,
) -> UpgradeResult {
    let MountedShare { mount_path, info } = match find_mounted_share(volume_id).await {
        Ok(share) => share,
        Err(answer) => return answer,
    };

    // Resolved so a remembered password is saved under the hostname, not the raw IP.
    let hostname = resolve_ip_to_hostname_with_wait(&info.server, HOSTNAME_WAIT).await;
    let display_name = friendly_server_name(&info.server);

    let upgraded = try_smb_upgrade(&info, &mount_path, username.as_deref(), password.as_deref(), volume_id).await;
    match upgraded {
        Ok(()) => {
            if remember_in_keychain && let (Some(u), Some(p)) = (&username, &password) {
                let server_key = hostname.as_deref().unwrap_or(&info.server);
                if let Err(e) = keychain::save_credentials(server_key, Some(&info.share), u, p) {
                    log::warn!("Couldn't save credentials to Keychain: {}", e);
                }
            }
            UpgradeResult::Success
        }
        Err(UpgradeError::Refused(refusal)) => credentials_needed(info, display_name, username, refusal.into()),
        Err(UpgradeError::Network { reason, display_name }) => UpgradeResult::NetworkError { reason, display_name },
    }
}

/// Upgrades `volume_id` with the password Finder saved in the login keychain, so
/// the user doesn't retype it, and copies it into Cmdr's own store on success so
/// the next reconnect is silent.
///
/// Reading it raises the macOS consent dialog (the frontend primes the user first;
/// the dialog's own text can't be customized), so this is user-initiated only: ❌
/// never call it at startup. Nothing readable, or access denied, answers
/// `CredentialsNeeded`, which falls back to the sign-in sheet.
#[cfg(target_os = "macos")]
pub(crate) async fn connect_directly_with_system_saved_password(volume_id: &str) -> UpgradeResult {
    use crate::network::smb_server_address::system_keychain_aliases;
    use crate::secrets::system_keychain_smb;

    let MountedShare { mount_path, info } = match find_mounted_share(volume_id).await {
        Ok(share) => share,
        Err(answer) => return answer,
    };

    let hostname = resolve_ip_to_hostname_with_wait(&info.server, HOSTNAME_WAIT).await;
    let display_name = friendly_server_name(&info.server);

    // Finder keys its entry by the name form it mounted with, often the mDNS
    // service name, so the query tries every alias discovery knows.
    let aliases = system_keychain_aliases(&info.server);
    let candidates = system_keychain_smb::server_query_candidates(&info.server, hostname.as_deref(), &aliases);

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
            let server_key = hostname.as_deref().unwrap_or(&info.server);
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
#[cfg(target_os = "macos")]
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

/// No system keychain to borrow from here, which is the macOS answer for "nothing
/// saved": ask for the password on the sign-in sheet.
#[cfg(not(target_os = "macos"))]
pub(crate) async fn connect_directly_with_system_saved_password(volume_id: &str) -> UpgradeResult {
    match find_mounted_share(volume_id).await {
        Ok(MountedShare { info, .. }) => {
            let display_name = friendly_server_name(&info.server);
            let hint = info.username.clone();
            credentials_needed(info, display_name, hint, CredentialsNeededReason::NoCredential)
        }
        Err(answer) => answer,
    }
}

#[cfg(test)]
#[path = "smb_connect_directly_test.rs"]
mod tests;
