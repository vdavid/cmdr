//! A saved SMB host moving to a new address, and the pending move each of its shares
//! waits on until its first mount there.
//!
//! ❗ **A share's new id isn't ours to mint.** It comes off the MOUNT's `statfs`, which
//! may spell the server as an IP, a hostname, or an mDNS service name, so a move can't
//! know it up front, and re-keying favorites and tabs to a guessed id would orphan them
//! when the first real mount rewrites it. So the move is in two halves:
//!
//! 1. **Save** ([`move_host`]): the host's manual entry, its share rows, and its
//!    passwords move to the new address. Each share KEEPS its old volume id, marked
//!    pending (`known_shares::pending_moves`, on disk). Favorites and tabs still name that
//!    id, and it now reaches the share at its new address: picking a favorite dials the
//!    saved row, which mounts there.
//! 2. **First mount** ([`complete`], from `smb_saved_shares::remember_mount`): the mount
//!    reports the real id, and favorites, tabs, history, and `lastUsedPaths` re-key to it
//!    (`ServerPlaceMoved`, carrying the live state so a following pane doesn't dial again).
//!
//! ❗ **A share still mounted from the old address refuses the move** (`ShareMounted`):
//! that's an OS mount Finder or another app may be using, so Cmdr doesn't take it down
//! behind the person's back, and a share mounted at the old address can't learn its id
//! at the new one. The refusal offers **"Eject and save"** ([`eject_and_move`]): the
//! person asks, and every saved share mounted from the old address comes down before the
//! move, the refusals that touch no mount asked first. A normal unmount goes first, and a
//! force one only when that didn't answer (a NAS that got a new IP leaves a DEAD mount,
//! whose normal unmount hangs); a refused one (files open: a LIVE mount) is never forced
//! and saves nothing (`file_system/volume/eject/escalate.rs`). The rest of the refusals
//! and the crash order are the other protocols' (`server_move.rs`).
//!
//! Why each holder does what it does: `docs/notes/server-address-move.md` § "SMB".

use cmdr_fs::log_private::LogPrivate;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::favorites::store::FavoriteVolume;
use crate::file_system::volume::ConnectionState;
use crate::file_system::volume::eject::{self, EjectError, TakenDown};
use crate::network::connect_wiring::{self, AttemptTable};
use crate::network::keychain::{self, KeychainError};
use crate::network::known_shares::{self, CompletedMove, KnownNetworkShare};
use crate::network::saved_server_fields::SavedServerOutcome;
use crate::network::server_identity::{self, SmbServer, credential_key, smb_server};
use crate::network::smb_saved_shares;
use crate::volume_broadcast::{self, MovedPlace, ServerPlaceMoved};

use super::{SECRET_STORE_BUDGET, SecretCopy, copy_secret_with, leave_behind, operations_need_any};

/// The saved SMB host an edit moves, as the servers listing filed it.
pub struct SmbHost {
    /// Where it dials now.
    pub server: SmbServer,
    /// Its sign-in history and saved shares, exactly as the listing filed them.
    pub rows: Vec<KnownNetworkShare>,
}

/// Moves the SMB host `host` to `to`. `others` is every OTHER saved SMB host with what
/// the UI calls it, for `AddressTaken`; `relocate` writes the manual store (an entry
/// another host holds at the new id is `Err` naming that host).
///
/// ❗ The order is the other protocols' crash story: refusals first (they touch
/// nothing), the passwords COPIED, the manual entry and the share rows moved, then the
/// old passwords deleted LAST.
pub async fn move_host(
    host: SmbHost,
    to: SmbServer,
    others: &[(SmbServer, String)],
    relocate: impl FnOnce() -> Result<(), String>,
) -> SavedServerOutcome {
    move_host_past(host, to, others, relocate, &[]).await
}

/// [`move_host`], past the places in `ejected`: shares whose old-address mount "Eject
/// and save" just took down. ❗ The registry may still hold one for a moment (the unmount
/// notification lands after the tool answers), and that must not refuse the move the
/// eject was for. The kernel's table has no such lag, so it's still asked.
async fn move_host_past(
    host: SmbHost,
    to: SmbServer,
    others: &[(SmbServer, String)],
    relocate: impl FnOnce() -> Result<(), String>,
    ejected: &[String],
) -> SavedServerOutcome {
    let shares: Vec<&KnownNetworkShare> = host.rows.iter().filter(|row| row.is_share()).collect();
    let places: Vec<String> = shares.iter().map(|row| known_shares::place_id(row)).collect();
    if let Some(refusal) = refusal_before_mounts(&to, others, &places) {
        return refusal;
    }
    if let Some(name) = mounted_share(&host.server, &shares, &places, ejected) {
        return SavedServerOutcome::ShareMounted { name };
    }

    let mut moving = connect_wiring::start_move().await;
    let from_name = smb_server(host.server.host(), host.server.port());
    let to_name = smb_server(to.host(), to.port());
    // ❗ One key for both spellings (`nas.local` → `nas`) is one Keychain entry: copying
    // it onto itself and deleting the "old" one would delete the password.
    let moves_secret = credential_key(&from_name) != credential_key(&to_name);
    let share_names: Vec<String> = shares.iter().map(|row| row.share_name.clone()).collect();
    let copied = if moves_secret {
        let Ok(copied) = copy_secrets(&from_name, &to_name, &share_names).await else {
            return SavedServerOutcome::SecretNotMoved;
        };
        copied
    } else {
        Vec::new()
    };
    if let Err(name) = relocate() {
        return SavedServerOutcome::AddressTaken { name };
    }
    known_shares::move_host_rows(&host.rows, &host.server, &to);
    leave_behind(&mut moving, &places);
    // A share-list mount names no place until it's up, so it lands on the server.
    moving.moved_away(&smb_saved_shares::server_place(host.server.host(), host.server.port()));
    drop(moving);
    announce_redial(&places);
    for share in copied {
        delete_secret(&from_name, share).await;
    }
    SavedServerOutcome::Saved
}

/// The refusals that touch no mount: the address another saved host holds, and an
/// operation on one of the host's shares. ❗ "Eject and save" asks these BEFORE any
/// eject: ejecting and then failing to save is the worst outcome it could have.
fn refusal_before_mounts(
    to: &SmbServer,
    others: &[(SmbServer, String)],
    places: &[String],
) -> Option<SavedServerOutcome> {
    let hosts = crate::network::fresh_discovered_hosts();
    if let Some((_, name)) = others.iter().find(|(server, _)| server.is(to, &hosts)) {
        return Some(SavedServerOutcome::AddressTaken { name: name.clone() });
    }
    if operations_need_any(&places.iter().map(String::as_str).collect::<Vec<_>>()) {
        return Some(SavedServerOutcome::OperationRunning);
    }
    None
}

/// A saved share of `server` that's still mounted from it: registered under its place
/// id (Cmdr's own mount, an adopted OS mount, a GVFS one) and not in `ejected`, or in
/// the kernel's mount table from the server.
fn mounted_share(
    server: &SmbServer,
    shares: &[&KnownNetworkShare],
    places: &[String],
    ejected: &[String],
) -> Option<String> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    if let Some(at) = places
        .iter()
        .position(|place| !ejected.contains(place) && manager.get(place).is_some())
    {
        return Some(shares[at].share_name.clone());
    }
    let mounted = server_identity::smb_shares_mounted_from(std::slice::from_ref(server));
    shares
        .iter()
        .find(|row| mounted.iter().any(|share| same_share_name(share, &row.share_name)))
        .map(|row| row.share_name.clone())
}

/// Two spellings of one share name: case- and NFC-folded, as `statfs` hands macOS's
/// decomposed form back.
fn same_share_name(a: &str, b: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    let fold = |name: &str| name.nfc().flat_map(char::to_lowercase).collect::<String>();
    fold(a) == fold(b)
}

/// Tells a pane standing on one of the moved shares to dial it again, now at the new
/// address: the same id on both sides, so nothing re-keys yet. A pane that was dialing
/// the old address (the move called that off) or reading "unreachable" there redials.
fn announce_redial(places: &[String]) {
    for place in places {
        let Some(row) = known_shares::share_by_volume_id(place) else {
            continue;
        };
        let Some(mount_path) = row.mount_path.clone() else {
            continue;
        };
        volume_broadcast::emit_server_place_moved(ServerPlaceMoved {
            old_prefix: mount_path.clone(),
            new_prefix: mount_path.clone(),
            places: vec![MovedPlace {
                old_volume_id: place.clone(),
                new_volume_id: place.clone(),
                new_root: mount_path.clone(),
                new_landing: mount_path,
                name: share_label(&row.share_name, &row.server_name),
                connection_state: None,
            }],
        });
    }
}

/// What the volume list calls a share: its mount row's spelling, `{share} on {server}`.
fn share_label(share: &str, server: &str) -> String {
    crate::network::smb_share_label::filed(share, server, None, None)
}

// ============================================================================
// Eject and save
// ============================================================================

/// What "Eject and save" answered.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum EjectAndSaveOutcome {
    /// The move ran, every old-address mount gone first, or it refused before any
    /// eject (`answer` says which).
    Settled { answer: SavedServerOutcome },
    /// A share's eject didn't go through, so nothing was saved. Shares ejected before
    /// it stay ejected.
    EjectRefused {
        /// The share whose eject didn't go through.
        name: String,
        /// Why, worded by the frontend's eject copy.
        error: EjectError,
    },
    /// The sheet was closed mid-way. Shares already ejected stay ejected; nothing was
    /// saved.
    Cancelled,
}

/// A mount of one of the host's saved shares from its OLD address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OldMount {
    /// Registered under the share's place id, so it goes through the eject pipeline.
    Volume { share: String, volume_id: String },
    /// In the kernel's mount table only (Finder or another app mounted it).
    Path { share: String, mount_path: String },
}

impl OldMount {
    fn share(&self) -> &str {
        match self {
            Self::Volume { share, .. } | Self::Path { share, .. } => share,
        }
    }
}

/// The "Eject and save" runs a sheet can still call off, by the sheet's attempt id.
static EJECT_AND_SAVE_ATTEMPTS: AttemptTable = AttemptTable::new("an SMB eject and save");

/// Files `attempt_id` as an "Eject and save" the sheet can call off, handing back the
/// token it runs under and the guard that takes the entry out again.
pub fn register_eject_and_save(attempt_id: &str) -> (CancellationToken, connect_wiring::AttemptGuard) {
    EJECT_AND_SAVE_ATTEMPTS.register(attempt_id)
}

/// Calls off the "Eject and save" running under `attempt_id`. ❗ It stops before the
/// next eject and before the save; an unmount already under way can't be taken back.
pub fn cancel_eject_and_save(attempt_id: &str) -> bool {
    EJECT_AND_SAVE_ATTEMPTS.cancel(attempt_id)
}

/// "Eject and save": [`move_host`], after taking down every saved share of the host
/// still mounted from its old address. The person asked for it on the `ShareMounted`
/// refusal, so ❗ Cmdr still never unmounts on its own initiative.
///
/// ❗ The order is the point: the refusals that touch no mount first (an eject followed
/// by a refused save is the worst outcome), then each mount, a normal unmount first and
/// a force only when that one didn't answer (`eject::take_down_smb_volume`), then the
/// move, which re-asks every refusal. A refused eject stops it, saving nothing.
pub async fn eject_and_move(
    host: SmbHost,
    to: SmbServer,
    others: &[(SmbServer, String)],
    relocate: impl FnOnce() -> Result<(), String>,
    cancel: &CancellationToken,
) -> EjectAndSaveOutcome {
    eject_and_move_with(host, to, others, relocate, cancel, take_down).await
}

/// [`eject_and_move`] with the takedown a parameter, so a test drives the ordering
/// without a mount.
pub(crate) async fn eject_and_move_with<T, Fut>(
    host: SmbHost,
    to: SmbServer,
    others: &[(SmbServer, String)],
    relocate: impl FnOnce() -> Result<(), String>,
    cancel: &CancellationToken,
    mut take_down: T,
) -> EjectAndSaveOutcome
where
    T: FnMut(OldMount) -> Fut,
    Fut: Future<Output = Result<TakenDown, EjectError>>,
{
    let shares: Vec<&KnownNetworkShare> = host.rows.iter().filter(|row| row.is_share()).collect();
    let places: Vec<String> = shares.iter().map(|row| known_shares::place_id(row)).collect();
    if let Some(answer) = refusal_before_mounts(&to, others, &places) {
        return EjectAndSaveOutcome::Settled { answer };
    }
    let mut ejected = Vec::new();
    for mount in old_mounts(&host.server, &shares, &places) {
        if cancel.is_cancelled() {
            return EjectAndSaveOutcome::Cancelled;
        }
        match take_down(mount.clone()).await {
            Ok(how) => {
                log::info!(
                    target: "volume",
                    "{} came down from its server's old address ({how:?})",
                    LogPrivate::volume(mount.share())
                );
                if let OldMount::Volume { volume_id, .. } = mount {
                    ejected.push(volume_id);
                }
            }
            Err(error) => {
                return EjectAndSaveOutcome::EjectRefused {
                    name: mount.share().to_string(),
                    error,
                };
            }
        }
    }
    if cancel.is_cancelled() {
        return EjectAndSaveOutcome::Cancelled;
    }
    let answer = move_host_past(host, to, others, relocate, &ejected).await;
    EjectAndSaveOutcome::Settled { answer }
}

/// Every mount of one of the host's saved shares from its old address: the registered
/// ones first (through the eject pipeline, so the registry and the index stay right),
/// then each kernel mount none of them is rooted at.
fn old_mounts(server: &SmbServer, shares: &[&KnownNetworkShare], places: &[String]) -> Vec<OldMount> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let mut mounts = Vec::new();
    let mut roots = Vec::new();
    for (row, place) in shares.iter().zip(places) {
        if let Some(volume) = manager.get(place) {
            roots.push(volume.root().to_string_lossy().to_string());
            mounts.push(OldMount::Volume {
                share: row.share_name.clone(),
                volume_id: place.clone(),
            });
        }
    }
    for (mount_path, share) in server_identity::smb_share_mounts_from(std::slice::from_ref(server)) {
        let Some(row) = shares.iter().find(|row| same_share_name(&share, &row.share_name)) else {
            continue;
        };
        if !roots.contains(&mount_path) {
            mounts.push(OldMount::Path {
                share: row.share_name.clone(),
                mount_path,
            });
        }
    }
    mounts
}

/// Takes one old-address mount down, a normal unmount first and a force only on no
/// answer (`file_system/volume/eject/escalate.rs`).
async fn take_down(mount: OldMount) -> Result<TakenDown, EjectError> {
    match mount {
        OldMount::Volume { volume_id, .. } => eject::take_down_smb_volume(&volume_id).await,
        OldMount::Path { mount_path, .. } => eject::take_down_smb_mount(&mount_path).await,
    }
}

/// Completes a share's pending move at its first mount at the new address: favorites
/// re-key to the id the mount reported, and the panes follow (`ServerPlaceMoved`).
///
/// ❗ The new id is LIVE (it just mounted), so the event carries its state: a pane
/// following it to a `saved` row would dial a share that's already up. Go to path's
/// recents hold OS paths under the mount, which name no server, so they stay.
pub async fn complete(moved: CompletedMove) {
    let name = share_label(&moved.share_name, &moved.server_name);
    let volume = FavoriteVolume {
        id: moved.new_volume_id.clone(),
        root: moved.new_mount_path.clone(),
        name: name.clone(),
    };
    let old_id = moved.old_volume_id.clone();
    let followed =
        tokio::task::spawn_blocking(move || crate::favorites::store::follow_share_move(&old_id, &volume)).await;
    if followed.is_err() {
        log::warn!(target: "volume", "following a moved share's favorites broke down");
    }
    let state = crate::file_system::volume::manager::get_volume_manager()
        .get(&moved.new_volume_id)
        .and_then(|volume| volume.connection_state())
        .unwrap_or(ConnectionState::OsMount);
    log::info!(
        target: "volume",
        "{} mounted at its server's new address; {} follows to {}",
        LogPrivate::volume(&moved.share_name),
        moved.old_volume_id,
        moved.new_volume_id
    );
    volume_broadcast::emit_server_place_moved(ServerPlaceMoved {
        old_prefix: moved.old_mount_path.unwrap_or_else(|| moved.new_mount_path.clone()),
        new_prefix: moved.new_mount_path.clone(),
        places: vec![MovedPlace {
            old_volume_id: moved.old_volume_id,
            new_volume_id: moved.new_volume_id,
            new_root: moved.new_mount_path.clone(),
            new_landing: moved.new_mount_path,
            name,
            connection_state: Some(state),
        }],
    });
}

// ============================================================================
// The passwords
// ============================================================================

/// Copies the host's server-level password and each share's from `from` to `to`
/// (server names the way `keychain` keys them), answering the shares whose entry it
/// copied (`None` is the server-level one). `Err` when the store refused or didn't
/// answer: the move must not go on without them.
async fn copy_secrets(from: &str, to: &str, shares: &[String]) -> Result<Vec<Option<String>>, ()> {
    let mut copied = Vec::new();
    for share in std::iter::once(None).chain(shares.iter().cloned().map(Some)) {
        let (from, to, key) = (from.to_string(), to.to_string(), share.clone());
        let outcome = cmdr_fs::deadline::blocking_with_timeout(
            SECRET_STORE_BUDGET,
            Err(KeychainError::Other("the secret store didn't answer".to_string())),
            move || {
                copy_secret_with(
                    || keychain::get_credentials(&from, key.as_deref()),
                    |creds| keychain::save_credentials(&to, key.as_deref(), &creds.username, &creds.password),
                )
            },
        )
        .await;
        match outcome {
            Ok(SecretCopy::Copied) => copied.push(share),
            Ok(SecretCopy::NothingStored) => {}
            Err(e) => {
                log::warn!(target: "volume", "couldn't copy a moved SMB server's password: kind={}", e.kind());
                return Err(());
            }
        }
    }
    Ok(copied)
}

/// Deletes one password left at the old address. Best effort, like
/// `server_move::delete_secret`: one that stays is an orphan nothing reads.
async fn delete_secret(server: &str, share: Option<String>) {
    let server = server.to_string();
    let deleted = cmdr_fs::deadline::blocking_with_timeout(
        SECRET_STORE_BUDGET,
        Err(KeychainError::Other("the secret store didn't answer".to_string())),
        move || keychain::delete_credentials(&server, share.as_deref()),
    )
    .await;
    if let Err(e) = deleted {
        log::warn!(target: "volume", "a moved SMB server's password stays at its old address: kind={}", e.kind());
    }
}

#[cfg(test)]
#[path = "server_move_smb_test.rs"]
mod tests;
