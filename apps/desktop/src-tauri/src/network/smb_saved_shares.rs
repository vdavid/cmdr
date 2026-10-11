//! Saved SMB shares: recording a share when Cmdr mounts it, and bringing a saved
//! one back to life.
//!
//! The store is `known_shares.rs` (its share rows); this is the wiring around it.
//! Model, writers, and what may and may not write: `docs/specs/saved-smb-shares.md`.
//!
//! ❗ **Only Cmdr's own mounts write here.** The mount watcher, the startup adopter,
//! the pane-open upgrade, and "Connect directly" see mounts nobody asked Cmdr to
//! save (Finder's, macOS's at login), and a row for one of those would invent a
//! history. Their volume ids are the same `statfs` ids, so a saved row and a live
//! mount of one share dedupe by id without either writer knowing about the other.
//! The one other writer is an explicit "remember this place": favoriting a folder
//! on a share nothing saved ([`remember_favorited_share`]).

use std::time::Duration;

use crate::commands::servers::ServerConnectOutcome;
use crate::network::connect_wiring::{AttemptTable, DialTicket};
use crate::network::known_shares::{self, AuthOptions, ConnectionMode, KnownNetworkShare};
use crate::network::mount::MountError;
use crate::network::one_shot_credentials::SecretOffer;
use crate::network::os_mount_notice::FallbackNotice;
use crate::network::{keychain, smb_upgrade};

/// How long a Keychain read or write may take, the bound the secret commands use.
const SECRET_STORE_LIMIT: Duration = Duration::from_secs(15);

/// The attempts a sheet's Cancel can reach, by the caller's own attempt id.
static ATTEMPTS: AttemptTable = AttemptTable::new("an smb share");

/// Calls off the saved-share connect running under `attempt_id`. ❗ It stops the
/// WAITING: a kernel mount already under way can't be taken back and may still
/// finish, and then the share simply shows up mounted.
pub fn cancel_connect(attempt_id: &str) -> bool {
    ATTEMPTS.cancel(attempt_id)
}

/// Calls off every saved-share connect to `place`, a share whose server just moved
/// (`server_move::smb`). A courtesy only: [`connect_saved_share`]'s landing check is
/// what keeps one from saving the old address again.
pub fn cancel_dials_to(place: &str) -> usize {
    ATTEMPTS.cancel_dials_to(place)
}

/// What a mount through Cmdr went through with, for [`remember_mount`].
pub struct MountedShare<'a> {
    /// The name the person knows the server by (the discovery list's `name`).
    pub host_name: &'a str,
    /// What the mount dialed.
    pub address: &'a str,
    pub port: u16,
    pub share: &'a str,
    /// The account it signed in as, `None` for guest.
    pub username: Option<&'a str>,
    pub mount_path: &'a str,
}

/// Records a share Cmdr just mounted, with the volume id the mount itself has.
///
/// A mount that doesn't say which volume it is (not an SMB mount, or one whose
/// `statfs` didn't answer in time) is still recorded, without a place: the hub
/// can list it, and the next mount fills the id in.
///
/// ❗ The first mount of a share whose server MOVED completes the move: its id is the
/// one favorites and tabs re-key to (`server_move::smb::complete`).
pub async fn remember_mount(mounted: MountedShare<'_>) -> Option<String> {
    let volume_id = smb_upgrade::mounted_volume_id(mounted.mount_path).await;
    let completed = known_shares::remember_share(KnownNetworkShare {
        server_name: mounted.host_name.to_string(),
        share_name: mounted.share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: chrono::Utc::now().to_rfc3339(),
        last_connection_mode: if mounted.username.is_some() {
            ConnectionMode::Credentials
        } else {
            ConnectionMode::Guest
        },
        // What a MOUNT learns about the host's auth stance is nothing: a guest
        // mount going through doesn't say an account would have been refused.
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: mounted.username.map(str::to_string),
        address: Some(mounted.address.to_string()),
        port: (mounted.port != 445).then_some(mounted.port),
        volume_id: volume_id.clone(),
        mount_path: volume_id.as_ref().map(|_| mounted.mount_path.to_string()),
        pinned: false,
    });
    if let Some(completed) = completed {
        crate::server_move::smb::complete(completed).await;
    }
    volume_id
}

/// The landing key a moved SMB SERVER leaves behind (`server_move::smb`), for a mount that
/// names no saved place until it's up: the share-list mount. Keyed by the server's
/// `credential_key`, so the spellings a password already pairs (`nas.local`, `nas`) pair here too.
pub fn server_place(host: &str, port: u16) -> String {
    use crate::network::server_identity::{credential_key, smb_server};
    format!("smb-server:{}", credential_key(&smb_server(host, port)))
}

/// [`remember_mount`] for a mount that set out at `set_out` from a share list: one of a
/// server that moved away since is NOT remembered, since that would save the old
/// address again beside the moved host. The kernel mount itself stays; it's the person's.
pub async fn remember_mount_since(mounted: MountedShare<'_>, set_out: DialTicket) -> Option<String> {
    let place = server_place(mounted.address, mounted.port);
    let Some(_landing) = crate::network::connect_wiring::land_since(&[place], set_out).await else {
        log::info!(target: "volume", "a share-list mount landed after its server moved; not saving the old address");
        return None;
    };
    remember_mount(mounted).await
}

/// Records a share an ADD named (`smb://sven@host/Container`), before anything
/// mounted it: a row in the hub with no place yet.
///
/// ❗ Republishes the lists: the hub re-reads the saved stores on `volumes-changed`,
/// and an Add that changed only a share's account changed no volume, so the row
/// kept its old account until the pane left the hub.
pub fn remember_named_share(host_name: &str, share: &str, username: Option<&str>) {
    known_shares::remember_share(KnownNetworkShare {
        server_name: host_name.to_string(),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: chrono::Utc::now().to_rfc3339(),
        last_connection_mode: ConnectionMode::Credentials,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: username.map(str::to_string),
        address: None,
        port: None,
        volume_id: None,
        mount_path: None,
        pinned: false,
    });
    crate::volume_broadcast::emit_volumes_changed();
}

/// Saves the share mounted at `mount_path` when nothing has saved it yet, because a folder on it
/// was just favorited: a Finder-mounted share has no row, so once it unmounts nothing could dial
/// the favorite. Writer 4 of `docs/specs/saved-smb-shares.md`.
///
/// ❗ Unpinned (a favorite is no request to crowd the switcher), and a share that's already saved
/// is left exactly as it is. The server goes by the address the mount dialed, spelled the way a
/// manual server is in the discovery list (`manual_servers::discovery_name`, so `localhost:11481`
/// off 445), since nothing here knows a nicer name for it. ❗ That's the name a Cmdr mount of the
/// same server files its shares under; a port-less one would list as a second server in the hub. The account's password lives in Finder's Keychain item, not Cmdr's,
/// so the first offline pick asks once through the sign-in sheet.
///
/// Reads the mount table for `mount_path`, which the add gate just resolved as live. Runs inside
/// the add command's blocking, deadline-bound write.
pub fn remember_favorited_share(mount_path: &str, volume_id: &str) {
    #[cfg(target_os = "macos")]
    let info = crate::volumes::get_smb_mount_info(mount_path);
    #[cfg(target_os = "linux")]
    let info = crate::volumes_linux::get_smb_mount_info(mount_path);
    let Some(info) = info else {
        // A non-SMB mount has no source to read.
        log::debug!("No SMB mount source at {mount_path:?}; the favorite's share isn't saved");
        return;
    };
    let row = favorited_share_row(
        &info.server,
        info.port,
        &info.share,
        info.username.as_deref(),
        mount_path,
        volume_id,
    );
    if known_shares::remember_share_unless_saved(row) {
        log::info!("Saved the share {:?} on {:?} for a favorite", info.share, info.server);
    }
}

/// The row [`remember_favorited_share`] files: a share place no Cmdr mount went through, unpinned.
fn favorited_share_row(
    server: &str,
    port: u16,
    share: &str,
    username: Option<&str>,
    mount_path: &str,
    volume_id: &str,
) -> KnownNetworkShare {
    KnownNetworkShare {
        server_name: crate::network::manual_servers::discovery_name(server, port),
        share_name: share.to_string(),
        protocol: "smb".to_string(),
        last_connected_at: chrono::Utc::now().to_rfc3339(),
        last_connection_mode: if username.is_some() {
            ConnectionMode::Credentials
        } else {
            ConnectionMode::Guest
        },
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: username.map(str::to_string),
        address: Some(server.to_string()),
        port: (port != 445).then_some(port),
        volume_id: Some(volume_id.to_string()),
        mount_path: Some(mount_path.to_string()),
        pinned: false,
    }
}

/// Brings a saved share place to life: mounts it as its account, connects the
/// direct session the usual way, and records the mount. The SMB arm of
/// `commands::servers::connect_saved_place`.
///
/// ❗ A share saved "as sven" mounts as sven, with the password the sheet just
/// offered or the one the Keychain holds for the host. No password for it answers
/// `NeedsCredentials`, ❌ never a guest attempt, which on a `map to guest` Samba
/// would "work" and show the wrong files. A share saved without an account mounts
/// as guest. `username` is the sheet's account field, which wins: SMB's sheet lets
/// the account change (`SignInShape::UsernamePassword`), and the row then
/// remembers the new one.
///
/// ❗ A password is remembered only once the mount went through, SMB's rule
/// everywhere (`servers/DETAILS.md` § "Remember, and who decides where it
/// starts").
///
/// ❗ A mount to an address the share's server moved away from while it was out is
/// neither remembered nor answered as connected (`set_out` is when the caller read the
/// row, `connect_wiring::DialTicket`): remembering it would save the old address again
/// beside the moved one. The kernel mount itself can't be taken back.
pub async fn connect_saved_share(
    row: KnownNetworkShare,
    set_out: DialTicket,
    attempt_id: &str,
    secret: Option<SecretOffer>,
    username: Option<String>,
) -> ServerConnectOutcome {
    let place = known_shares::place_id(&row);
    let (cancel, attempt) = ATTEMPTS.register_dialing(attempt_id, vec![place], set_out);
    let mut row = row;
    if let Some(username) = username.filter(|u| !u.trim().is_empty()) {
        row.username = Some(username.trim().to_string());
    }
    let Some(address) = row.address.clone() else {
        // Only a mount fills an address in, and only a mounted row has a place.
        return ServerConnectOutcome::Unreachable;
    };
    let port = row.port.unwrap_or(445);

    let password = match (&row.username, &secret) {
        (None, _) => None,
        (Some(_), Some(offer)) => Some(offer.secret.clone()),
        (Some(username), None) => match stored_password(&row.server_name, username).await {
            Some(password) => Some(password),
            None => return ServerConnectOutcome::NeedsCredentials,
        },
    };

    let mount = crate::network::mount_share(
        address.clone(),
        row.share_name.clone(),
        row.username.clone(),
        password.clone(),
        port,
        None,
    );
    let result = tokio::select! {
        result = mount => result,
        () = cancel.cancelled() => return ServerConnectOutcome::Cancelled,
    };
    let mounted = match result {
        Ok(mounted) => mounted,
        Err(error) => return outcome_of(&error),
    };

    // The direct session, the way every mount Cmdr makes gets one. Someone is
    // looking at this share, so a fallback to the kernel mount is worth saying.
    smb_upgrade::register_smb_volume(
        &address,
        &row.share_name,
        &mounted.mount_path,
        row.username.as_deref(),
        password.as_deref(),
        port,
        FallbackNotice::Announce,
    )
    .await;

    let Some(_landing) = attempt.land().await else {
        return ServerConnectOutcome::Cancelled;
    };
    if let (Some(username), Some(offer)) = (&row.username, &secret)
        && offer.remember
    {
        store_password(&row.server_name, username, &offer.secret).await;
    }

    let volume_id = remember_mount(MountedShare {
        host_name: &row.server_name,
        address: &address,
        port,
        share: &row.share_name,
        username: row.username.as_deref(),
        mount_path: &mounted.mount_path,
    })
    .await;
    match volume_id.or(row.volume_id) {
        Some(volume_id) => ServerConnectOutcome::Connected { volume_id },
        None => ServerConnectOutcome::Unreachable,
    }
}

/// The password the Keychain holds for `username` on the host, or `None` when it
/// holds none, holds another account's, or didn't answer in time.
async fn stored_password(host_name: &str, username: &str) -> Option<String> {
    let host = host_name.to_string();
    let stored = crate::deadline::blocking_with_timeout(SECRET_STORE_LIMIT, None, move || {
        keychain::get_credentials(&host, None).ok()
    })
    .await?;
    (stored.username == username).then_some(stored.password)
}

/// Files the password the sheet offered, once the mount it opened went through. A
/// store that says no is logged and carried on from: the share is mounted, and
/// all that's lost is "silent next time".
async fn store_password(host_name: &str, username: &str, password: &str) {
    let (host, user, secret) = (host_name.to_string(), username.to_string(), password.to_string());
    let stored = crate::deadline::blocking_with_timeout(SECRET_STORE_LIMIT, false, move || {
        keychain::save_credentials(&host, None, &user, &secret).is_ok()
    })
    .await;
    if !stored {
        log::warn!("The share mounted, but the Keychain didn't store its password for host={host_name:?}");
    }
}

/// A mount's refusal, in the servers family's outcome vocabulary.
///
/// ❗ `AuthRequired` (a guest turned away) is `NeedsCredentials`, ❌ never a
/// rejection: nobody offered a password to be wrong. A share that turns a
/// signed-in account away reads as a rejection of that account, since the sheet's
/// answer is another password for the same place.
pub(crate) fn outcome_of(error: &MountError) -> ServerConnectOutcome {
    match error {
        MountError::AuthRequired { .. } => ServerConnectOutcome::NeedsCredentials,
        MountError::AuthFailed { .. } | MountError::PermissionDenied { .. } => {
            ServerConnectOutcome::AuthenticationRejected
        }
        MountError::Timeout { .. } => ServerConnectOutcome::TimedOut,
        MountError::Cancelled { .. } => ServerConnectOutcome::Cancelled,
        MountError::HostUnreachable { .. }
        | MountError::ShareNotFound { .. }
        | MountError::UnsupportedProtocol { .. }
        | MountError::MountRefused { .. }
        | MountError::MountMissing { .. }
        | MountError::GvfsMissing
        | MountError::Unexpected { .. } => {
            log::info!("A saved share didn't mount: {error:?}");
            ServerConnectOutcome::Unreachable
        }
    }
}

#[cfg(test)]
#[path = "smb_saved_shares_test.rs"]
mod tests;
