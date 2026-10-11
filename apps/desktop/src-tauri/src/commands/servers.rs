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

use crate::network::one_shot_credentials::SecretOffer;
use crate::network::s3_volume_wiring;
use crate::network::saved_server_fields::{self, SavedServerOutcome};
use crate::network::sftp_volume_wiring;
use crate::network::webdav_volume_wiring;
use crate::network::{
    known_shares, manual_servers, s3_known_places, sftp_known_servers, smb_saved_shares, webdav_known_servers,
};
use cmdr_sftp::SftpConnectionParams;
use cmdr_webdav::WebdavConnectionParams;

mod outcomes;
mod s3_accounts;
mod saves;
mod smb_hosts;
mod wire;
use outcomes::{SavedEntry, outcome_from_s3, outcome_from_sftp, outcome_from_webdav, saved_by_id};
use s3_accounts::{s3_accounts, s3_place};
use saves::{save_edit, save_target};
use smb_hosts::smb_hosts;
pub use wire::{
    SavedPlace, SavedPlaceRefusal, SavedServer, ServerConnectOutcome, ServerNameSource, ServerProtocol, ServerTarget,
};

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
    saved_servers(
        manual_servers::all(&app),
        known_shares::get_all_known_shares(),
        &crate::network::fresh_discovered_hosts(),
    )
}

/// The listing, over a manual-server list the caller supplies.
///
/// Split out because the manual SMB store reads a FILE through an `AppHandle`,
/// and the union itself is a pure fold that a cell can drive: the share store and
/// the discovery list come in as arguments too, so a cell never races another
/// over the process-global copies.
fn saved_servers(
    manual: Vec<manual_servers::ManualServerEntry>,
    known: Vec<known_shares::KnownNetworkShare>,
    hosts: &[crate::network::NetworkHost],
) -> Vec<SavedServer> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // The app roots, from the module that mints them for the volume listing, so
    // the hub's rows and the switcher's rows spell one prefix.
    let app_roots: std::collections::HashMap<String, String> = crate::server_volumes::server_places()
        .into_iter()
        .map(|place| (place.id, place.app_root))
        .collect();
    let mut servers = Vec::new();

    for entry in sftp_known_servers::all() {
        let volume_id = cmdr_fs::volume::sftp_volume_id(&entry.host, entry.port, &entry.username);
        let Some(app_root) = app_roots.get(&volume_id).cloned() else {
            log::warn!(target: "volume", "a saved SFTP server has no place in the volume listing; leaving it out");
            continue;
        };
        let label = entry.label();
        servers.push(SavedServer {
            places: vec![SavedPlace {
                connected: manager.get(&volume_id).is_some(),
                volume_id: volume_id.clone(),
                name: label.clone(),
                pinned: entry.pinned,
                app_root,
                username: Some(entry.username.clone()),
                auto_reconnect: Some(entry.auto_reconnect),
            }],
            id: volume_id,
            protocol: ServerProtocol::Sftp,
            display_name: label,
            name_source: ServerNameSource::of_account(&entry.display_name),
            address: format!("{}:{}", entry.host, entry.port),
            username: Some(entry.username),
            pinned: entry.pinned,
            last_connected_at: Some(entry.last_connected_at),
            auto_reconnect: Some(entry.auto_reconnect),
        });
    }

    for entry in webdav_known_servers::all() {
        let Some(params) = webdav_params(&entry.url, &entry.username, "/") else {
            log::warn!(target: "volume", "a saved WebDAV server's address isn't a URL any more; leaving it out of the listing");
            continue;
        };
        let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &entry.username);
        let Some(app_root) = app_roots.get(&volume_id).cloned() else {
            log::warn!(target: "volume", "a saved WebDAV server has no place in the volume listing; leaving it out");
            continue;
        };
        let label = entry.label();
        servers.push(SavedServer {
            places: vec![SavedPlace {
                connected: manager.get(&volume_id).is_some(),
                volume_id: volume_id.clone(),
                name: label.clone(),
                pinned: entry.pinned,
                app_root,
                username: Some(entry.username.clone()),
                auto_reconnect: Some(entry.auto_reconnect),
            }],
            id: volume_id,
            protocol: ServerProtocol::Webdav,
            display_name: label,
            name_source: ServerNameSource::of_account(&entry.display_name),
            address: entry.url,
            username: Some(entry.username),
            pinned: entry.pinned,
            last_connected_at: Some(entry.last_connected_at),
            auto_reconnect: Some(entry.auto_reconnect),
        });
    }

    servers.extend(s3_accounts(manager, &app_roots));
    servers.extend(smb_hosts(manual, known, hosts));
    servers
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
#[tauri::command]
#[specta::specta]
pub async fn connect_saved_place(
    volume_id: String,
    attempt_id: String,
    secret: Option<SecretOffer>,
    username: Option<String>,
) -> Result<ServerConnectOutcome, SavedPlaceRefusal> {
    if crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .is_some()
    {
        return Err(SavedPlaceRefusal::AlreadyConnected { volume_id });
    }
    // ❗ Before the store read: a move landing between the read and the dial's
    // landing must still refuse it, or a dial to the old address (a host-key
    // approval answered after the move) saves it again (`connect_wiring::DialTicket`).
    let set_out = crate::network::connect_wiring::DialTicket::now();
    let Some(saved) = saved_by_id(&volume_id) else {
        return Err(SavedPlaceRefusal::NoSuchServer { volume_id });
    };
    Ok(match saved {
        SavedEntry::Sftp(entry) => {
            let mut params = SftpConnectionParams::new(&entry.host, entry.port, &entry.username, entry.remote_root);
            params.key_file = entry.key_file.map(std::path::PathBuf::from);
            params.use_agent = entry.use_agent;
            params.auto_reconnect = entry.auto_reconnect;
            outcome_from_sftp(
                sftp_volume_wiring::connect_and_register_since(
                    set_out,
                    &entry.display_name,
                    entry.start_folder,
                    params,
                    &attempt_id,
                    secret,
                )
                .await,
            )
        }
        SavedEntry::Webdav(entry) => {
            let Some(mut params) = webdav_params(&entry.url, &entry.username, &entry.remote_root) else {
                return Ok(ServerConnectOutcome::InvalidUrl);
            };
            params.auto_reconnect = entry.auto_reconnect;
            outcome_from_webdav(
                webdav_volume_wiring::connect_and_register_since(
                    set_out,
                    &entry.display_name,
                    entry.start_folder,
                    params,
                    &attempt_id,
                    secret,
                )
                .await,
            )
        }
        SavedEntry::S3(entry) => outcome_from_s3(
            s3_volume_wiring::connect_and_register_since(
                set_out,
                // Nothing typed: the account keeps the name it has.
                "",
                entry,
                &attempt_id,
                secret,
            )
            .await,
        ),
        // A saved SMB share: mounted as its account, then connected the usual way.
        SavedEntry::Smb(row) => {
            smb_saved_shares::connect_saved_share(row, set_out, &attempt_id, secret, username).await
        }
    })
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
    }
}

/// Calls off the connect running under `attempt_id`, whichever protocol owns it.
///
/// Each backend holds its OWN attempt table, so this asks both; an id nobody is
/// connecting under answers `false`, which a cancel racing a finished connect
/// legitimately does.
#[tauri::command]
#[specta::specta]
pub fn cancel_server_connect(attempt_id: String) -> bool {
    let sftp = sftp_volume_wiring::cancel_connect(&attempt_id);
    let webdav = webdav_volume_wiring::cancel_connect(&attempt_id);
    let s3 = s3_volume_wiring::cancel_connect(&attempt_id);
    let smb = smb_saved_shares::cancel_connect(&attempt_id);
    sftp || webdav || s3 || smb
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
        || s3_volume_wiring::disconnect(volume_id).await;
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
        || s3_volume_wiring::disconnect(&id).await;
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
pub async fn has_server_secret(id: String) -> bool {
    let Some(saved) = saved_by_id(&id) else {
        return false;
    };
    match saved {
        SavedEntry::Sftp(entry) => super::sftp::has_sftp_credentials(entry.host, entry.port, entry.username).await,
        SavedEntry::Webdav(entry) => super::webdav::has_webdav_credentials(entry.url, entry.username).await,
        SavedEntry::S3(entry) => super::s3::has_s3_credentials(entry.provider, entry.access_key_id).await,
        SavedEntry::Smb(row) => {
            crate::deadline::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
                crate::network::keychain::has_credentials(&row.server_name, None)
            })
            .await
        }
    }
}

/// Forgets a server's remembered secret, answering whether the store accepted
/// the removal.
///
/// ❗ Leaves the SERVER saved: "stop remembering my password" and "forget this
/// server" are two requests, and the sheet offers them separately.
#[tauri::command]
#[specta::specta]
pub async fn forget_server_secret(id: String) -> bool {
    let Some(saved) = saved_by_id(&id) else {
        return false;
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
        // SMB keeps one password per HOST, which is what a share's sign-in wrote.
        SavedEntry::Smb(row) => {
            crate::deadline::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
                crate::network::keychain::delete_credentials(&row.server_name, None).is_ok()
            })
            .await
        }
    }
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
    match server {
        ServerTarget::Sftp {
            host, port, username, ..
        } => Some(cmdr_fs::volume::sftp_volume_id(&host, port, &username)),
        ServerTarget::Webdav { url, username, .. } => {
            let params = webdav_params(&url, &username, "/")?;
            Some(cmdr_fs::volume::webdav_volume_id(
                params.host(),
                params.port(),
                &username,
            ))
        }
        ServerTarget::S3 {
            provider,
            access_key_id,
            bucket,
            ..
        } => s3_place(provider, access_key_id, bucket, true).volume_id(),
    }
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
    let Some(group) = smb_hosts::smb_host_group(&id, manual_servers::all(&app)) else {
        return Ok(false);
    };
    let names = group.credential_names(&crate::network::fresh_discovered_hosts());
    let gone = crate::network::keychain::forget_server_credentials(&names, &group.share_names())?;
    Ok(gone > 0)
}

// ============================================================================
// Shared plumbing
// ============================================================================

/// Connection params for a saved WebDAV entry, or `None` when its address isn't
/// an `http`/`https` URL.
pub(super) fn webdav_params(url: &str, username: &str, remote_root: &str) -> Option<WebdavConnectionParams> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| WebdavConnectionParams::new(parsed, username, remote_root))
}

#[cfg(test)]
#[path = "servers_test.rs"]
mod servers_test;

#[cfg(test)]
#[path = "server_moves_test.rs"]
mod server_moves_test;
