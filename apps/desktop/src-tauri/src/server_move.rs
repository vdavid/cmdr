//! A saved SFTP, WebDAV, or S3 server moving to a new address, and everything
//! that follows it there.
//!
//! A server that MOVED (a NAS on a new IP, `nas.local` → its Tailscale name, a new
//! port) keeps its pin, its settings, its password, its favorites, and the tabs
//! on it. The protocol and the account never move: another account is another
//! place, so an edit naming one is refused (`AccountChanged`).
//!
//! ❗ **The order is the crash story.** The password is COPIED to the new address
//! first, then the store moves in one write, then everything that follows it
//! (favorites, Go to path's recents, the live session, the panes), and the old
//! password is deleted LAST. A crash between any two steps leaves the password
//! reachable from whichever entry the store holds; the worst leftover is an
//! orphaned copy under a key nothing reads. A copy the store refuses refuses the
//! whole move (`SecretNotMoved`): a server whose password stayed behind would ask
//! for it again.
//!
//! ❗ **An address another saved server holds is refused, ❌ never merged**
//! (`AddressTaken`): a merge keeps one entry's settings, pin, and password and
//! silently drops the other's.
//!
//! ❗ **The old session is dropped WITHOUT `VolumeUnmounted`**, which sends a pane
//! home. A pane on the old place follows to the new one instead
//! (`ServerPlaceMoved`, and the frontend's `pane/server-move-follow.ts`), lands on
//! its `saved` row, and dials it there: the ordinary first-open flow, so a new
//! host's key or a missing password asks in the usual place.
//!
//! What a move takes along, and why each holder does what it does:
//! `docs/notes/server-address-move.md`.

use std::time::Duration;

use crate::favorites::store::FavoriteVolume;
use crate::network::keychain::{self, KeychainError, SmbCredentials};
use crate::network::s3_known_places::{self, KnownS3Place, S3ProviderChoice};
use crate::network::saved_server_fields::{self, Relocation, SavedServerOutcome};
use crate::network::sftp_known_servers::{self, KnownSftpServer};
use crate::network::webdav_known_servers::{self, KnownWebdavServer};
use crate::network::{s3_volume_wiring, sftp_volume_wiring, webdav_volume_wiring};
use crate::volume_broadcast::{self, MovedPlace, ServerPlaceMoved};

/// How long the secret store gets to answer, the same as every secret command.
const SECRET_STORE_BUDGET: Duration = Duration::from_secs(15);

/// Saves an edit to the saved SFTP server `saved`: in place when the address is
/// the same, else as a move.
pub async fn edit_sftp(saved: KnownSftpServer, edit: KnownSftpServer) -> SavedServerOutcome {
    if edit.username != saved.username {
        return SavedServerOutcome::AccountChanged;
    }
    let old_id = cmdr_fs::volume::sftp_volume_id(&saved.host, saved.port, &saved.username);
    let new_id = cmdr_fs::volume::sftp_volume_id(&edit.host, edit.port, &edit.username);
    if old_id == new_id {
        return sftp_volume_wiring::save_without_connecting(edit).await;
    }
    let Ok(start_folder) =
        saved_server_fields::start_folder_under_root(&edit.remote_root, edit.start_folder.as_deref())
    else {
        return SavedServerOutcome::StartFolderOutsideRoot;
    };
    let edit = KnownSftpServer { start_folder, ..edit };
    if let Some(holder) = sftp_known_servers::find(&edit.host, edit.port, &edit.username) {
        return SavedServerOutcome::AddressTaken { name: holder.label() };
    }
    let from = SecretKey {
        service: sftp_volume_wiring::credential_service(&saved.host, saved.port),
        scope: saved.username.clone(),
    };
    let to = SecretKey {
        service: sftp_volume_wiring::credential_service(&edit.host, edit.port),
        scope: edit.username.clone(),
    };
    let Ok(copied) = copy_secret(&from, &to).await else {
        return SavedServerOutcome::SecretNotMoved;
    };
    let old_prefix = cmdr_fs::volume::sftp_app_root(&saved.host, saved.port, &saved.username);
    let new_prefix = cmdr_fs::volume::sftp_app_root(&edit.host, edit.port, &edit.username);
    match sftp_known_servers::relocate(&saved.host, saved.port, &saved.username, edit) {
        Relocation::Moved { .. } => {}
        Relocation::NotFound => return SavedServerOutcome::Unreachable,
        Relocation::Taken(holder) => return SavedServerOutcome::AddressTaken { name: holder.label() },
    }
    follow(Move {
        old_prefix,
        new_prefix,
        places: vec![(old_id, new_id)],
    })
    .await;
    if copied == SecretCopy::Copied {
        delete_secret(from).await;
    }
    SavedServerOutcome::Saved
}

/// Saves an edit to the saved WebDAV server `saved`: in place when the base URL
/// is the same, else as a move.
///
/// ❗ The whole base URL is the address (scheme, host, port, and path): it's what
/// the client dials and what the store keys on. A move that keeps the host, port,
/// and account keeps the id too, so its paths stay as they are, but its entry,
/// its password's key (when the scheme changed), and its session still move.
pub async fn edit_webdav(saved: KnownWebdavServer, edit: KnownWebdavServer) -> SavedServerOutcome {
    if edit.username != saved.username {
        return SavedServerOutcome::AccountChanged;
    }
    if webdav_known_servers::normalize_url(&edit.url) == webdav_known_servers::normalize_url(&saved.url) {
        return webdav_volume_wiring::save_without_connecting(edit).await;
    }
    let (Some((old_host, old_port)), Some((new_host, new_port))) = (saved.endpoint(), edit.endpoint()) else {
        // A stored URL that no longer parses names no session, and a typed one
        // that doesn't parse never reaches here (the sheet refuses it first).
        return SavedServerOutcome::Unreachable;
    };
    let Ok(start_folder) =
        saved_server_fields::start_folder_under_root(&edit.remote_root, edit.start_folder.as_deref())
    else {
        return SavedServerOutcome::StartFolderOutsideRoot;
    };
    let edit = KnownWebdavServer { start_folder, ..edit };
    let (Some(from_service), Some(to_service)) = (
        webdav_volume_wiring::credential_service(&saved.url),
        webdav_volume_wiring::credential_service(&edit.url),
    ) else {
        return SavedServerOutcome::Unreachable;
    };
    let old_id = cmdr_fs::volume::webdav_volume_id(&old_host, old_port, &saved.username);
    let new_id = cmdr_fs::volume::webdav_volume_id(&new_host, new_port, &edit.username);
    let holder = webdav_known_servers::all().into_iter().find(|entry| {
        let is_saved = webdav_known_servers::normalize_url(&entry.url)
            == webdav_known_servers::normalize_url(&saved.url)
            && entry.username == saved.username;
        let same_place = entry.username == edit.username
            && (webdav_known_servers::normalize_url(&entry.url) == webdav_known_servers::normalize_url(&edit.url)
                || entry.endpoint() == Some((new_host.clone(), new_port)));
        !is_saved && same_place
    });
    if let Some(holder) = holder {
        return SavedServerOutcome::AddressTaken { name: holder.label() };
    }
    let from = SecretKey {
        service: from_service,
        scope: saved.username.clone(),
    };
    let to = SecretKey {
        service: to_service,
        scope: edit.username.clone(),
    };
    let copied = if from == to {
        SecretCopy::NothingStored
    } else {
        let Ok(copied) = copy_secret(&from, &to).await else {
            return SavedServerOutcome::SecretNotMoved;
        };
        copied
    };
    match webdav_known_servers::relocate(&saved.url, &saved.username, edit) {
        Relocation::Moved { .. } => {}
        Relocation::NotFound => return SavedServerOutcome::Unreachable,
        Relocation::Taken(holder) => return SavedServerOutcome::AddressTaken { name: holder.label() },
    }
    follow(Move {
        old_prefix: cmdr_fs::volume::webdav_app_root(&old_host, old_port, &saved.username),
        new_prefix: cmdr_fs::volume::webdav_app_root(&new_host, new_port, &saved.username),
        places: vec![(old_id, new_id)],
    })
    .await;
    if copied == SecretCopy::Copied {
        delete_secret(from).await;
    }
    SavedServerOutcome::Saved
}

/// Names the saved S3 account `account_id`, and moves it to `endpoint` when that
/// names a new one.
///
/// ❗ Only an "Other S3-compatible" endpoint moves: a self-hosted server on a new
/// address is the same storage, while a preset's region, account ID, or location
/// names DIFFERENT storage (an AWS bucket lives in one region; an R2 account ID is
/// another account), so changing one answers `AccountChanged`. The endpoint and
/// the secret are the account's, so every place under the key moves together.
pub async fn edit_s3_account(account_id: &str, name: &str, endpoint: Option<S3ProviderChoice>) -> SavedServerOutcome {
    let Some(saved) = s3_known_places::all().into_iter().find(|place| {
        place
            .params()
            .is_ok_and(|params| s3_known_places::account_id(&params) == account_id)
    }) else {
        return SavedServerOutcome::Unreachable;
    };
    let Some(endpoint) = endpoint.filter(|endpoint| *endpoint != saved.provider) else {
        s3_known_places::rename_account(account_id, name);
        return SavedServerOutcome::Saved;
    };
    if !matches!(saved.provider, S3ProviderChoice::Other { .. }) || !matches!(endpoint, S3ProviderChoice::Other { .. })
    {
        return SavedServerOutcome::AccountChanged;
    }
    let key = saved.access_key_id.clone();
    let moved = KnownS3Place {
        provider: endpoint.clone(),
        ..saved.clone()
    };
    let (Ok(old_params), Ok(new_params)) = (saved.params(), moved.params()) else {
        return SavedServerOutcome::Unreachable;
    };
    let new_account_id = s3_known_places::account_id(&new_params);
    let holder = s3_known_places::all().into_iter().find(|place| {
        place
            .params()
            .is_ok_and(|params| s3_known_places::account_id(&params) == new_account_id)
    });
    if let Some(holder) = holder.filter(|_| new_account_id != account_id) {
        return SavedServerOutcome::AddressTaken {
            name: s3_known_places::account_label(&holder),
        };
    }
    let (Some(from_service), Some(to_service)) = (
        s3_volume_wiring::credential_service(&saved.provider, &key),
        s3_volume_wiring::credential_service(&endpoint, &key),
    ) else {
        return SavedServerOutcome::Unreachable;
    };
    let from = SecretKey {
        service: from_service,
        scope: key.clone(),
    };
    let to = SecretKey {
        service: to_service,
        scope: key.clone(),
    };
    let copied = if from == to {
        SecretCopy::NothingStored
    } else {
        let Ok(copied) = copy_secret(&from, &to).await else {
            return SavedServerOutcome::SecretNotMoved;
        };
        copied
    };
    let previous = match s3_known_places::relocate_account(account_id, endpoint.clone()) {
        Relocation::Moved { previous } => previous,
        Relocation::NotFound => return SavedServerOutcome::Unreachable,
        Relocation::Taken(holder) => {
            let name = holder.first().map(s3_known_places::account_label).unwrap_or_default();
            return SavedServerOutcome::AddressTaken { name };
        }
    };
    s3_known_places::rename_account(&new_account_id, name);
    let places = previous
        .into_iter()
        .filter_map(|place| {
            let old_id = place.volume_id()?;
            let new_id = KnownS3Place {
                provider: endpoint.clone(),
                ..place
            }
            .volume_id()?;
            Some((old_id, new_id))
        })
        .collect();
    follow(Move {
        old_prefix: cmdr_fs::volume::s3_app_root(old_params.host(), old_params.port(), &key),
        new_prefix: cmdr_fs::volume::s3_app_root(new_params.host(), new_params.port(), &key),
        places,
    })
    .await;
    if copied == SecretCopy::Copied {
        delete_secret(from).await;
    }
    SavedServerOutcome::Saved
}

/// What moved: the app prefix every path on the server carried, and its places'
/// ids, old and new.
struct Move {
    old_prefix: String,
    new_prefix: String,
    /// `(old id, new id)` per place. The same pair twice when a WebDAV URL moved
    /// without its host, port, or account.
    places: Vec<(String, String)>,
}

/// Takes everything that names the old address along, once the store already
/// holds the new one: the favorites and Go to path's recents (on disk), the live
/// session (dropped), then the panes (`ServerPlaceMoved`).
///
/// ❗ The session goes BEFORE the event: a pane follows the event straight to the
/// place's `saved` row and dials it, and a dial that still found the old volume
/// registered under the same id (a WebDAV path move) would answer "already
/// connected" for a session about to go. ❗ And through the wiring's own
/// `disconnect`, ❌ never `disconnect_place_inner`: its `VolumeUnmounted` sends a
/// pane home, and this pane is following the place instead.
async fn follow(moved: Move) {
    let places = crate::server_volumes::server_places();
    let mut announced = Vec::new();
    let mut favorites = Vec::new();
    for (old_id, new_id) in &moved.places {
        let Some(place) = places.iter().find(|place| place.id == *new_id) else {
            continue;
        };
        announced.push(MovedPlace {
            old_volume_id: old_id.clone(),
            new_volume_id: new_id.clone(),
            new_root: place.app_root.clone(),
            new_landing: place.landing_path.clone().unwrap_or_else(|| place.app_root.clone()),
            name: place.name.clone(),
        });
        favorites.push((
            old_id.clone(),
            FavoriteVolume {
                id: new_id.clone(),
                root: place.app_root.clone(),
                name: place.name.clone(),
            },
        ));
    }

    let (old_prefix, new_prefix) = (moved.old_prefix.clone(), moved.new_prefix.clone());
    let rewritten = tokio::task::spawn_blocking(move || {
        crate::favorites::store::follow_server_move(&old_prefix, &new_prefix, &favorites);
        crate::go_to_path::history::follow_server_move(&old_prefix, &new_prefix);
    })
    .await;
    if rewritten.is_err() {
        log::warn!(target: "volume", "following a moved server's favorites and recents broke down");
    }

    for (old_id, _) in &moved.places {
        let dropped = sftp_volume_wiring::disconnect(old_id).await
            || webdav_volume_wiring::disconnect(old_id).await
            || s3_volume_wiring::disconnect(old_id).await;
        if dropped {
            log::info!(target: "volume", "dropped {old_id}'s session at its old address");
        }
    }

    volume_broadcast::emit_server_place_moved(ServerPlaceMoved {
        old_prefix: moved.old_prefix,
        new_prefix: moved.new_prefix,
        places: announced,
    });
}

// ============================================================================
// The password
// ============================================================================

/// Where a secret lives in the store: the service, and the account as the scope.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SecretKey {
    service: String,
    scope: String,
}

/// What copying a server's password to its new address did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretCopy {
    /// Nothing was stored at the old address, so nothing moves.
    NothingStored,
    /// The password is stored at the new address too.
    Copied,
}

/// Copies the password at `from` to `to`, on a blocking task (the store can put a
/// Keychain prompt in front of either call). `Err` when the store refused or
/// didn't answer: the move must not go on without it.
async fn copy_secret(from: &SecretKey, to: &SecretKey) -> Result<SecretCopy, ()> {
    let (from, to) = (from.clone(), to.clone());
    let copied = crate::deadline::blocking_with_timeout(
        SECRET_STORE_BUDGET,
        Err(KeychainError::Other("the secret store didn't answer".to_string())),
        move || {
            copy_secret_with(
                || keychain::get_credentials(&from.service, Some(&from.scope)),
                |creds| keychain::save_credentials(&to.service, Some(&to.scope), &creds.username, &creds.password),
            )
        },
    )
    .await;
    copied.map_err(|e| {
        log::warn!(target: "volume", "couldn't copy a moved server's password: kind={}", e.kind());
    })
}

/// The decision half of [`copy_secret`], over a read and a write.
fn copy_secret_with(
    read: impl FnOnce() -> Result<SmbCredentials, KeychainError>,
    write: impl FnOnce(&SmbCredentials) -> Result<(), KeychainError>,
) -> Result<SecretCopy, KeychainError> {
    let creds = match read() {
        Ok(creds) => creds,
        Err(KeychainError::NotFound(_)) => return Ok(SecretCopy::NothingStored),
        Err(e) => return Err(e),
    };
    write(&creds)?;
    Ok(SecretCopy::Copied)
}

/// Deletes the password left at the old address, once the store no longer names
/// it. Best effort: one that stays is an orphan nothing reads, ❌ never a reason
/// to undo a move that landed.
async fn delete_secret(key: SecretKey) {
    let deleted = crate::deadline::blocking_with_timeout(
        SECRET_STORE_BUDGET,
        Err(KeychainError::Other("the secret store didn't answer".to_string())),
        move || keychain::delete_credentials(&key.service, Some(&key.scope)),
    )
    .await;
    if let Err(e) = deleted {
        log::warn!(target: "volume", "a moved server's password stays at its old address: kind={}", e.kind());
    }
}

#[cfg(test)]
#[path = "server_move_test.rs"]
mod server_move_test;
