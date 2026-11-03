//! One command family for "a server", protocol-agnostic wherever the UI is.
//!
//! The hub, the switcher, the sign-in sheet, and the pane banner all speak about
//! servers rather than about SFTP, WebDAV, and SMB, so this is the surface they
//! call. It is a FACADE over the per-protocol commands (`commands/sftp.rs`,
//! `commands/webdav.rs`, `commands/network.rs`), which keep their tests, their
//! docs, and their callers in the wiring.
//!
//! ❗ **Why a facade and not a rewrite.** The two backends' outcome enums are
//! already correct and tested, and a frontend that branches on a superset is one
//! `switch` instead of two half-matching ones. It is also where S3 plugs in
//! the S3 arm: one more [`ServerTarget`] arm, one more store, the same outcome
//! enum plus what S3 adds.
//!
//! ❗ **The three levels this family speaks in**
//! (`apps/desktop/src/lib/servers/DETAILS.md` § "The model: account, place,
//! pin"): an ACCOUNT is an endpoint plus an identity and is never
//! navigable; a PLACE is the mountable thing under it and is what becomes a
//! `VolumeInfo`; a PIN is whether a place shows in the volume switcher. SFTP and
//! WebDAV have exactly one place per account; SMB and S3 have many (an S3
//! account's buckets, plus its root when saved).
//!
//! The cores under these commands (listing, saving, editing, dialing) live in
//! `saved_servers/` (`saved_servers/CLAUDE.md`), where Ask Cmdr reaches them too.

use crate::network::nfs_volume_wiring;
use crate::network::one_shot_credentials::SecretOffer;
use crate::network::s3_volume_wiring;
use crate::network::saved_server_fields::{self, SavedServerOutcome};
use crate::network::sftp_volume_wiring;
use crate::network::webdav_volume_wiring;
use crate::network::{
    known_shares, manual_servers, nfs_known_places, s3_known_places, sftp_known_servers, webdav_known_servers,
};
use cmdr_sftp::SftpConnectionParams;

// Every core lives below `commands` (`saved_servers`), where Ask Cmdr reads and saves too.
use crate::saved_servers::{
    SavedEntry, nfs_places, outcome_from_s3, outcome_from_sftp, outcome_from_webdav, save_edit, save_target,
    saved_by_id, smb_hosts, target_ids, webdav_params,
};
pub use crate::saved_servers::{SavedPlaceRefusal, ServerConnectOutcome};
use crate::saved_servers::{SavedServer, ServerTarget};

// ============================================================================
// Listing
// ============================================================================

/// Every server the user has saved, across all three stores.
///
/// ❗ Cached state only, ❌ never the wire: the hub re-reads this on every
/// `volumes-changed`, and a probe here would turn a refresh into a round of
/// network traffic.
#[tauri::command]
#[specta::specta]
pub fn list_saved_servers(app: tauri::AppHandle) -> Vec<SavedServer> {
    crate::saved_servers::listing(
        manual_servers::all(&app),
        known_shares::get_all_known_shares(),
        &crate::network::fresh_discovered_hosts(),
    )
}

// ============================================================================
// Connecting
// ============================================================================

/// Dials a server the user has already saved, by the id its place carries.
///
/// ❗ **Only for a place with NO registered volume.** The frontend's connect flow
/// picks its move by the volume's standing; the two ways of picking wrong are
/// refused here rather than silently doing the wrong thing.
///
/// `attempt_id` is the CALLER's own name for this attempt, made before the call
/// so a cancel button is armed from the first millisecond;
/// [`cancel_server_connect`] takes the same one.
///
/// `username` is what a sign-in sheet's account field held, which only an SMB
/// share has (its `SignInShape` is `UsernamePassword`): the share is the place and
/// the account a field on it. SFTP and WebDAV ignore it, since their volume id IS
/// the account.
///
/// ❗ **One dial per place** (`saved_servers`): a second caller while a dial
/// is out (a pane's Enter and Ask Cmdr's `connect_volume`) shares its outcome
/// instead of dialing again, which would register a second volume.
#[tauri::command]
#[specta::specta]
pub async fn connect_saved_place(
    volume_id: String,
    attempt_id: String,
    secret: Option<SecretOffer>,
    username: Option<String>,
) -> Result<ServerConnectOutcome, SavedPlaceRefusal> {
    crate::saved_servers::connect_saved_place(&volume_id, &attempt_id, secret, username).await
}

/// Dials a server the user just typed, in add mode.
///
/// A successful dial registers the volume AND saves the server, so the second
/// use of it costs one keystroke.
#[tauri::command]
#[specta::specta]
pub async fn connect_server(
    target: ServerTarget,
    attempt_id: String,
    secret: Option<SecretOffer>,
) -> ServerConnectOutcome {
    match target {
        ServerTarget::Sftp {
            display_name,
            host,
            port,
            username,
            remote_root,
            start_folder,
            key_file,
            use_agent,
            auto_reconnect,
        } => {
            // ❗ Before the dial, so a refusal registers nothing and saves nothing.
            let Ok(start_folder) = saved_server_fields::start_folder_under_root(&remote_root, start_folder.as_deref())
            else {
                return ServerConnectOutcome::StartFolderOutsideRoot;
            };
            let mut params = SftpConnectionParams::new(&host, port, &username, remote_root);
            params.key_file = key_file.map(std::path::PathBuf::from);
            params.use_agent = use_agent;
            params.auto_reconnect = auto_reconnect;
            outcome_from_sftp(
                sftp_volume_wiring::connect_and_register(&display_name, start_folder, params, &attempt_id, secret)
                    .await,
            )
        }
        ServerTarget::Webdav {
            display_name,
            url,
            username,
            remote_root,
            start_folder,
            auto_reconnect,
        } => {
            let Some(mut params) = webdav_params(&url, &username, &remote_root) else {
                return ServerConnectOutcome::InvalidUrl;
            };
            let Ok(start_folder) = saved_server_fields::start_folder_under_root(&remote_root, start_folder.as_deref())
            else {
                return ServerConnectOutcome::StartFolderOutsideRoot;
            };
            params.auto_reconnect = auto_reconnect;
            outcome_from_webdav(
                webdav_volume_wiring::connect_and_register(&display_name, start_folder, params, &attempt_id, secret)
                    .await,
            )
        }
        ServerTarget::S3 {
            display_name,
            provider,
            access_key_id,
            bucket,
            auto_reconnect,
        } => outcome_from_s3(
            s3_volume_wiring::connect_and_register(
                &display_name,
                provider,
                &access_key_id,
                bucket.as_deref(),
                auto_reconnect,
                &attempt_id,
                secret,
            )
            .await,
        ),
        nfs @ ServerTarget::Nfs { .. } => nfs_places::connect(nfs, &attempt_id).await,
    }
}

/// Calls off the connect running under `attempt_id`, whichever protocol owns it.
///
/// Each backend holds its OWN attempt table, so this asks every one; an id nobody
/// holds answers `false`, which a cancel racing a finished connect legitimately does.
#[tauri::command]
#[specta::specta]
pub fn cancel_server_connect(attempt_id: String) -> bool {
    crate::saved_servers::cancel_connect(&attempt_id)
}

/// Drops a place's session and takes it out of the registry, answering whether
/// there was one.
///
/// ❗ The place stays SAVED. Disconnecting a pinned place leaves it as a `saved`
/// row in the switcher; forgetting is [`forget_server`].
///
/// ❗ Emits `VolumeUnmounted`, so a pane standing on the place goes home the way
/// it does after an eject. Without it the pane keeps a volume id the registry no
/// longer answers for, and every listing on it fails instead of redirecting. ❌
/// No ordering constraint against `volumes-changed` here, unlike
/// [`forget_server`]: the ROW survives a disconnect (it becomes `saved`), so the
/// consumer has nothing to race.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_place(volume_id: String) -> bool {
    disconnect_place_inner(&volume_id).await
}

/// The body of [`disconnect_place`], so the eject path can drop a place's
/// session without going through IPC.
///
/// ❗ The ONE way a place's session is dropped. Calling either wiring's
/// `disconnect` directly skips the `VolumeUnmounted` event above, which is what
/// sends a pane standing on the place home.
pub(crate) async fn disconnect_place_inner(volume_id: &str) -> bool {
    let root = crate::server_volumes::place_root(volume_id);
    let dropped = sftp_volume_wiring::disconnect(volume_id).await
        || webdav_volume_wiring::disconnect(volume_id).await
        || s3_volume_wiring::disconnect(volume_id).await
        || nfs_volume_wiring::disconnect(volume_id).await;
    if dropped {
        crate::volume_broadcast::emit_volume_gone(volume_id, root.as_deref().unwrap_or_default());
    }
    dropped
}

// ============================================================================
// Editing what is saved
// ============================================================================

/// Moves a place's pin, answering whether a saved place was there.
///
/// ❗ Emits `volumes-changed`: the switcher's Network group is exactly the pinned
/// and the connected places, so an unpin the list never hears about leaves a row
/// on screen that nothing will remove.
#[tauri::command]
#[specta::specta]
pub fn set_place_pinned(volume_id: String, pinned: bool) -> bool {
    set_place_pinned_inner(&volume_id, pinned)
}

/// The body of [`set_place_pinned`], so a cell can call it without a `String`.
fn set_place_pinned_inner(volume_id: &str, pinned: bool) -> bool {
    let Some(saved) = saved_by_id(volume_id) else {
        return false;
    };
    let moved = match saved {
        SavedEntry::Sftp(entry) => sftp_known_servers::set_pinned(&entry.host, entry.port, &entry.username, pinned),
        SavedEntry::Webdav(entry) => webdav_known_servers::set_pinned(&entry.url, &entry.username, pinned),
        SavedEntry::S3(_) => s3_known_places::set_pinned(volume_id, pinned),
        SavedEntry::Nfs(_) => nfs_known_places::set_pinned(volume_id, pinned),
        SavedEntry::Smb(_) => known_shares::set_share_pinned(volume_id, pinned),
    };
    if moved {
        crate::volume_broadcast::emit_volumes_changed();
    }
    moved
}

/// Moves a saved place's "Reconnect automatically" switch, answering whether a
/// saved place was there.
///
/// ❗ **The row menus' writer, narrower than [`update_saved_server`] on
/// purpose.** A checkbox flips ONE field, and sending the whole record would
/// write back every other field as the menu last read it, clobbering an edit the
/// sheet saved since. It still moves both copies the sheet's save moves (the
/// store and a connected volume's live switch), through each wiring's
/// `apply_auto_reconnect`. An unsaved place has nothing to persist and answers
/// `false`.
///
/// ❗ Emits `volumes-changed`, which is what makes an open servers hub re-read
/// the saved list its row menu shows.
#[tauri::command]
#[specta::specta]
pub fn set_place_auto_reconnect(volume_id: String, auto_reconnect: bool) -> bool {
    set_place_auto_reconnect_inner(&volume_id, auto_reconnect)
}

/// The body of [`set_place_auto_reconnect`], so a cell can call it without a `String`.
fn set_place_auto_reconnect_inner(volume_id: &str, on: bool) -> bool {
    let Some(saved) = saved_by_id(volume_id) else {
        return false;
    };
    let moved = match saved {
        SavedEntry::Sftp(entry) => {
            sftp_volume_wiring::apply_auto_reconnect(&entry.host, entry.port, &entry.username, on)
        }
        SavedEntry::Webdav(entry) => webdav_volume_wiring::apply_auto_reconnect(&entry.url, &entry.username, on),
        SavedEntry::S3(_) => s3_volume_wiring::apply_auto_reconnect(volume_id, on),
        SavedEntry::Nfs(_) => nfs_volume_wiring::apply_auto_reconnect(volume_id, on),
        // A share has no such switch: the kernel mount's own reconnect is macOS's.
        SavedEntry::Smb(_) => false,
    };
    if moved {
        crate::volume_broadcast::emit_volumes_changed();
    }
    moved
}

/// Drops a server from the saved list, answering whether one was there.
///
/// ❗ **Also drops the session and unregisters the volume**, because a forgotten
/// server is gone: leaving the session up would keep a row in the switcher that
/// no store knows about and no "Forget" can reach a second time. A tab standing
/// on it becomes a home tab (`apps/desktop/src-tauri/src/commands/DETAILS.md`
/// § `servers.rs`).
///
/// ❗ **`VolumeUnmounted` goes out BEFORE `volumes-changed`.** The pane's
/// consumer is what redirects it home, and `volumes-changed` is what takes the
/// row out of the store; the other order would leave the pane standing on a
/// volume nothing can name. The order is written here rather than relied on:
/// `volumes-changed` is debounced and this is not, so it holds either way, but a
/// future undebounce shouldn't be able to break it silently.
///
/// ❌ Leaves the stored secret alone: forgetting a server from a list is not the
/// same request as revoking its credential, and [`forget_server_secret`] is that
/// one.
#[tauri::command]
#[specta::specta]
pub async fn forget_server(id: String) -> bool {
    let Some(saved) = saved_by_id(&id) else {
        return false;
    };
    let root = crate::server_volumes::place_root(&id);
    let forgotten = match saved {
        SavedEntry::Sftp(entry) => sftp_known_servers::forget(&entry.host, entry.port, &entry.username),
        SavedEntry::Webdav(entry) => webdav_known_servers::forget(&entry.url, &entry.username),
        SavedEntry::S3(_) => s3_known_places::forget(&id),
        SavedEntry::Nfs(_) => nfs_known_places::forget(&id),
        SavedEntry::Smb(row) => {
            // ❗ The row and its pin, and nothing else: a mounted share stays
            // mounted, so there's no pane to send home and no session to drop.
            let forgotten = known_shares::forget_rows(std::slice::from_ref(&row)) > 0;
            if forgotten {
                crate::volume_broadcast::emit_volumes_changed();
            }
            return forgotten;
        }
    };
    if !forgotten {
        return false;
    }
    crate::volume_broadcast::emit_volume_gone(&id, root.as_deref().unwrap_or_default());
    // ❗ AFTER the gone event: the wiring requests its own `volumes-changed`, so
    // disconnecting first would put the republish ahead of the redirect.
    let _ = sftp_volume_wiring::disconnect(&id).await
        || webdav_volume_wiring::disconnect(&id).await
        || s3_volume_wiring::disconnect(&id).await
        || nfs_volume_wiring::disconnect(&id).await;
    crate::volume_broadcast::emit_volumes_changed();
    true
}

/// Whether a secret is remembered for the place `id` names.
///
/// ❗ The protocol-agnostic reader behind the sign-in sheet's Remember box: a
/// caller that branched on protocol to answer would be one more place that has to
/// know SFTP from WebDAV. A store that didn't answer in time reads as `false`,
/// the same harmless collapse the per-protocol readers make.
///
/// ❌ **Not for a context menu.** It is `get_credentials(…).is_ok()` underneath,
/// and every read of a Keychain entry can raise a system prompt. A sheet opening
/// is a moment a person is already waiting through; a right-click is not, so the
/// row's menu offers "Forget saved password" unconditionally and lets
/// [`forget_server_secret`] report whether there was one.
#[tauri::command]
#[specta::specta]
pub async fn has_server_secret(id: String, app: tauri::AppHandle) -> bool {
    has_server_secret_among(&id, manual_servers::all(&app)).await
}

/// [`has_server_secret`] over the manual SMB entries `manual`, so a test needs no app.
pub(crate) async fn has_server_secret_among(id: &str, manual: Vec<manual_servers::ManualServerEntry>) -> bool {
    let Some(saved) = saved_by_id(id) else {
        // An SMB host's listing id names no place, but its shares' passwords.
        return smb_secret(id, manual, crate::network::keychain::has_server_credentials).await;
    };
    match saved {
        SavedEntry::Sftp(entry) => super::sftp::has_sftp_credentials(entry.host, entry.port, entry.username).await,
        SavedEntry::Webdav(entry) => super::webdav::has_webdav_credentials(entry.url, entry.username).await,
        SavedEntry::S3(entry) => super::s3::has_s3_credentials(entry.provider, entry.access_key_id).await,
        // `AUTH_SYS` has no secret to remember.
        SavedEntry::Nfs(_) => false,
        SavedEntry::Smb(_) => smb_secret(id, manual, crate::network::keychain::has_server_credentials).await,
    }
}

/// Forgets a server's remembered secret, answering whether the store accepted
/// the removal.
///
/// ❗ Leaves the SERVER saved: "stop remembering my password" and "forget this
/// server" are two requests, and the sheet offers them separately.
#[tauri::command]
#[specta::specta]
pub async fn forget_server_secret(id: String, app: tauri::AppHandle) -> bool {
    forget_server_secret_among(&id, manual_servers::all(&app)).await
}

/// [`forget_server_secret`] over the manual SMB entries `manual`, so a test needs no app.
pub(crate) async fn forget_server_secret_among(id: &str, manual: Vec<manual_servers::ManualServerEntry>) -> bool {
    let Some(saved) = saved_by_id(id) else {
        // An SMB host's listing id: every password of the host and its shares.
        return smb_secret(id, manual, forget_smb_entries).await;
    };
    match saved {
        SavedEntry::Sftp(entry) => super::sftp::delete_sftp_credentials(entry.host, entry.port, entry.username)
            .await
            .is_ok(),
        SavedEntry::Webdav(entry) => super::webdav::delete_webdav_credentials(entry.url, entry.username)
            .await
            .is_ok(),
        // ❗ The ACCOUNT's secret: every other place under this key stops
        // remembering it too, since they share the one entry.
        SavedEntry::S3(entry) => super::s3::delete_s3_credentials(entry.provider, entry.access_key_id)
            .await
            .is_ok(),
        // Nothing stored, so nothing to remove.
        SavedEntry::Nfs(_) => false,
        // SMB keeps one password per HOST, plus a share's own beside another
        // account's: both go, under every name the host's password can be filed by.
        SavedEntry::Smb(_) => smb_secret(id, manual, forget_smb_entries).await,
    }
}

/// Runs `act` over the SMB secret scope of `id` (a host's listing id or a share's
/// place id, `smb_hosts::smb_secret_scope`) on a blocking task. Nothing saved, or a
/// store that didn't answer in time, is `false`.
async fn smb_secret(
    id: &str,
    manual: Vec<manual_servers::ManualServerEntry>,
    act: fn(&[String], &[String]) -> bool,
) -> bool {
    let id = id.to_string();
    cmdr_fs::deadline::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
        let hosts = crate::network::fresh_discovered_hosts();
        smb_hosts::smb_secret_scope(&id, manual, &hosts).is_some_and(|scope| act(&scope.names, &scope.shares))
    })
    .await
}

/// Forgets the SMB entries under `names` and `shares`, answering whether one was there.
fn forget_smb_entries(names: &[String], shares: &[String]) -> bool {
    crate::network::keychain::forget_server_credentials(names, shares).is_ok_and(|gone| gone > 0)
}

/// Saves an edited server, or adds one without connecting.
///
/// ❗ Takes a [`ServerTarget`], the same shape the add sheet collects, because an
/// edit and an add differ only in whether the fields arrived prefilled. A saved
/// server's PIN is not in it: `remember` preserves the stored pin on a replace,
/// and [`set_place_pinned`] is the one writer that moves one.
///
/// ❗ `editing` is the id of the saved place the edit was raised on (`None` for
/// "Add anyway"). The BACKEND decides whether the edit moved the address: in
/// place it saves as always, else it moves the server and everything that
/// follows it (`server_move.rs`). The frontend never compares addresses.
///
/// ❗ Answers a typed [`SavedServerOutcome`]; a refusal writes nothing at all. A
/// connected place's edit applies live: `network/live_server_edit.rs`.
///
/// ❗ A saved edit republishes the volume list, whatever it changed. The rows
/// carry the label and the landing, and neither an unconnected place's edit nor
/// a start-folder-only one moves anything in the registry that would announce
/// it. The servers hub re-reads the saved list on the same broadcast. A second
/// request beside the live install's own coalesces in the debounce.
#[tauri::command]
#[specta::specta]
pub async fn update_saved_server(server: ServerTarget, editing: Option<String>) -> SavedServerOutcome {
    let outcome = match editing {
        Some(id) => save_edit(&id, server).await,
        None => save_target(server).await,
    };
    if outcome == SavedServerOutcome::Saved {
        crate::volume_broadcast::emit_volumes_changed();
    }
    outcome
}

/// The id the servers listing gives the account `server` names, through the same
/// id funnel the listing uses (`cmdr_fs::volume::ids`), or `None` for a WebDAV
/// address that isn't an `http`/`https` URL.
///
/// ❗ How a caller finds the row a save just made. ❌ Never by comparing the
/// address it typed with the listed one: the stores normalize (a WebDAV URL gains
/// its trailing slash), so the typed spelling missed a row that was there, and
/// "Add anyway" reported a saved server as not saved.
#[tauri::command]
#[specta::specta]
pub fn saved_server_id(server: ServerTarget) -> Option<String> {
    target_ids::place_id(server)
}

/// Names the saved S3 account the listing calls `id`, and moves it to `endpoint`
/// when that names a new one. An empty name unnames it, so the UI calls it
/// `key id@host` again. An account nothing is saved under answers `unreachable`.
///
/// ❗ Only an "Other S3-compatible" endpoint moves (a self-hosted server on a new
/// address): a preset's region or account ID names other STORAGE, so changing one
/// answers `account_changed`. The move takes every place under the key along
/// (`server_move.rs`).
///
/// ❗ Its own command rather than a [`ServerTarget`] arm, like
/// [`update_saved_smb_host`]: the account is no place to save, and a target with
/// no bucket would save the account ROOT as a new place. A bucket's name is the
/// bucket's own, so an account is the only S3 thing a person names.
///
/// ❗ Emits `volumes-changed`, which is what makes an open servers hub re-read
/// the saved list and the switcher relabel the account root.
#[tauri::command]
#[specta::specta]
pub async fn update_saved_s3_account(
    id: String,
    name: String,
    endpoint: Option<s3_known_places::S3ProviderChoice>,
) -> SavedServerOutcome {
    let outcome = crate::server_move::edit_s3_account(&id, &name, endpoint).await;
    if outcome == SavedServerOutcome::Saved {
        crate::volume_broadcast::emit_volumes_changed();
    }
    outcome
}

/// Names the saved SMB host the listing calls `id`, sets the account it's used
/// with, and moves it to `address` when that names another server. An empty name
/// unnames it, so the UI calls it by its address again; no `username` clears the
/// account. A host nobody saved any more answers `unreachable`.
///
/// ❗ By the listing's id ALONE: the host's address and port come from the same
/// listing the row was drawn from ([`smb_hosts::smb_host_group`]), so an edit can
/// only land on the host it was opened on. A host only the share history knew is
/// saved where its mount dialed (naming it is what saves it: `manual_servers` §
/// `name_server_entry_at_path`).
///
/// ❗ A new address MOVES the host, its saved shares, and its passwords, and each
/// share's favorites and tabs follow at its first mount there
/// (`server_move_smb.rs`). The backend decides whether it's a move; the sheet
/// sends what the field holds.
///
/// ❗ Its own command rather than a [`ServerTarget`] arm: an SMB host is a
/// manual-server entry, not an account with a place to dial.
///
/// ❗ Emits `volumes-changed`, which is what makes an open servers hub re-read
/// the saved list.
#[tauri::command]
#[specta::specta]
pub async fn update_saved_smb_host(
    id: String,
    name: String,
    username: Option<String>,
    address: Option<String>,
    app: tauri::AppHandle,
) -> SavedServerOutcome {
    let edit = manual_servers::HostEdit { name, username };
    let outcome = smb_hosts::edit_host(&id, edit, address.as_deref(), &app).await;
    if outcome == SavedServerOutcome::Saved {
        crate::volume_broadcast::emit_volumes_changed();
    }
    outcome
}

/// [`update_saved_smb_host`] for "Eject and save", the button on its `share_mounted`
/// refusal: a move first takes down every saved share still mounted from the old
/// address, then saves. `attempt_id` is the sheet's own, so its Cancel
/// ([`cancel_server_connect`]) can call the run off before the next eject and before
/// the save.
///
/// ❗ The person asked for this, so Cmdr still never unmounts on its own initiative;
/// the order and the force rule: `server_move_smb.rs`, `eject/escalate.rs`. Emits
/// `volumes-changed` on a save, like [`update_saved_smb_host`].
#[tauri::command]
#[specta::specta]
pub async fn eject_and_update_saved_smb_host(
    id: String,
    name: String,
    username: Option<String>,
    address: Option<String>,
    attempt_id: String,
    app: tauri::AppHandle,
) -> crate::server_move::smb::EjectAndSaveOutcome {
    use crate::server_move::smb::{EjectAndSaveOutcome, register_eject_and_save};

    let (cancel, _attempt) = register_eject_and_save(&attempt_id);
    let edit = manual_servers::HostEdit { name, username };
    let outcome = smb_hosts::eject_and_edit_host(&id, edit, address.as_deref(), &cancel, &app).await;
    if matches!(
        outcome,
        EjectAndSaveOutcome::Settled {
            answer: SavedServerOutcome::Saved
        }
    ) {
        crate::volume_broadcast::emit_volumes_changed();
    }
    outcome
}

/// Forgets the saved SMB host the listing calls `id`: its manual entry, its
/// sign-in history, and every share saved under it. Answers whether anything was
/// there.
///
/// ❗ Exactly the store rows the listing filed under that host
/// ([`smb_hosts::smb_host_group`]), found by the listing's id alone: nothing
/// another host filed goes with it, not even a server on another port of the
/// same machine. Rows only: nothing is unmounted and no password is touched
/// ("Forget saved password" is its own request).
///
/// ❗ Emits `volumes-changed`: a pinned share of the host leaves the switcher.
#[tauri::command]
#[specta::specta]
pub fn forget_saved_smb_host(id: String, app: tauri::AppHandle) -> bool {
    let Some(group) = smb_hosts::smb_host_group(&id, manual_servers::all(&app)) else {
        return false;
    };
    let manual = group.manual && manual_servers::remove_manual_server(&id, &app).is_ok();
    let rows = known_shares::forget_rows(&group.rows);
    let forgotten = manual || rows > 0;
    if forgotten {
        crate::volume_broadcast::emit_volumes_changed();
    }
    forgotten
}

/// Forgets every password stored for the saved SMB host the listing calls `id`, the
/// "Also forget the saved password" box on Forget server. Answers whether any entry
/// was there.
///
/// ❗ Call it BEFORE [`forget_saved_smb_host`]: the host's names come off the rows
/// that command takes away. Every name the password can be filed under goes, on the
/// host's own port ([`smb_hosts::SmbHostGroup::credential_names`]), with each saved
/// share's share-level entry and the in-memory cache; a server on another port of
/// the same machine keeps its own. Deleting reads nothing, so it raises no prompt to
/// read a password.
#[tauri::command]
#[specta::specta]
pub fn forget_saved_smb_host_password(
    id: String,
    app: tauri::AppHandle,
) -> Result<bool, crate::network::keychain::KeychainError> {
    let hosts = crate::network::fresh_discovered_hosts();
    let Some(scope) = smb_hosts::smb_secret_scope(&id, manual_servers::all(&app), &hosts) else {
        return Ok(false);
    };
    let gone = crate::network::keychain::forget_server_credentials(&scope.names, &scope.shares)?;
    Ok(gone > 0)
}

// ============================================================================
// Shared plumbing
// ============================================================================

#[cfg(test)]
#[path = "servers_test.rs"]
mod servers_test;

#[cfg(test)]
#[path = "server_moves_test.rs"]
mod server_moves_test;
