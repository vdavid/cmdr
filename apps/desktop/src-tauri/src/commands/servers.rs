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
//! later: one more [`ServerTarget`] arm, one more store, the same outcome enum
//! plus whatever S3 adds.
//!
//! ❗ **The three levels this family speaks in**
//! (`apps/desktop/src/lib/servers/DETAILS.md` § "The model: account, place,
//! pin"): an ACCOUNT is an endpoint plus an identity and is never
//! navigable; a PLACE is the mountable thing under it and is what becomes a
//! `VolumeInfo`; a PIN is whether a place shows in the volume switcher. SFTP and
//! WebDAV have exactly one place per account, SMB has many.

use super::sftp::SftpHostKeyIdentity;
use crate::network::one_shot_credentials::SecretOffer;
use crate::network::saved_server_fields::{self, SavedServerOutcome};
use crate::network::sftp_volume_wiring::{self, SftpConnection};
use crate::network::webdav_volume_wiring::{self, WebdavConnection};
use crate::network::{known_shares, manual_servers, sftp_known_servers, webdav_known_servers};
use cmdr_sftp::SftpConnectionParams;
use cmdr_webdav::WebdavConnectionParams;

mod wire;
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
    saved_servers(manual_servers::all(&app))
}

/// The listing, over a manual-server list the caller supplies.
///
/// Split out because the manual SMB store reads a FILE through an `AppHandle`,
/// and the union itself is a pure fold that a cell can drive.
fn saved_servers(manual: Vec<manual_servers::ManualServerEntry>) -> Vec<SavedServer> {
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
            }],
            id: volume_id,
            protocol: ServerProtocol::Sftp,
            display_name: label,
            name_source: ServerNameSource::of_account(&entry.display_name),
            address: format!("{}:{}", entry.host, entry.port),
            username: Some(entry.username),
            pinned: entry.pinned,
            last_connected_at: Some(entry.last_connected_at),
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
            }],
            id: volume_id,
            protocol: ServerProtocol::Webdav,
            display_name: label,
            name_source: ServerNameSource::of_account(&entry.display_name),
            address: entry.url,
            username: Some(entry.username),
            pinned: entry.pinned,
            last_connected_at: Some(entry.last_connected_at),
        });
    }

    servers.extend(smb_hosts(manual));
    servers
}

/// The SMB hosts, from the share store and the manually-typed list, deduped.
///
/// One row per HOST: `known_shares.rs` is server-level in practice (its only
/// writer stores an empty `share_name`), and a host the user typed by hand is
/// the same host it has a share row for.
fn smb_hosts(manual: Vec<manual_servers::ManualServerEntry>) -> Vec<SavedServer> {
    let mut hosts: Vec<SavedServer> = Vec::new();
    let mut push = |host: SavedServer| {
        if hosts
            .iter()
            .any(|existing| existing.address.eq_ignore_ascii_case(&host.address))
        {
            return;
        }
        hosts.push(host);
    };

    for share in known_shares::get_all_known_shares() {
        push(SavedServer {
            id: manual_servers::generate_server_id(&share.server_name, 445),
            protocol: ServerProtocol::Smb,
            display_name: share.server_name.clone(),
            // ❗ The mount's spelling, which nobody chose: the hub lets a
            // discovered Bonjour name outrank it.
            name_source: ServerNameSource::Fallback,
            address: share.server_name,
            // ❗ Not the share's username: that is per-share, and this row is the
            // HOST. A share's own account is asked for when it is mounted.
            username: None,
            pinned: false,
            last_connected_at: Some(share.last_connected_at),
            places: Vec::new(),
        });
    }
    for entry in manual {
        push(SavedServer {
            id: entry.id,
            protocol: ServerProtocol::Smb,
            display_name: entry.display_name,
            // ❗ The ADDRESS they typed, worn as a label: `manual_servers` derives
            // it (`host` or `host:port`) because SMB's add flow asks for nothing
            // else. So a Bonjour name outranks it, the same as a mount's.
            name_source: ServerNameSource::Fallback,
            address: entry.address,
            username: None,
            pinned: false,
            // `added_at` is when it was typed, ❌ not when it last answered.
            last_connected_at: None,
            places: Vec::new(),
        });
    }
    hosts
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
#[tauri::command]
#[specta::specta]
pub async fn connect_saved_place(
    volume_id: String,
    attempt_id: String,
    secret: Option<SecretOffer>,
) -> Result<ServerConnectOutcome, SavedPlaceRefusal> {
    if crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .is_some()
    {
        return Err(SavedPlaceRefusal::AlreadyConnected { volume_id });
    }
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
                sftp_volume_wiring::connect_and_register(
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
                webdav_volume_wiring::connect_and_register(
                    &entry.display_name,
                    entry.start_folder,
                    params,
                    &attempt_id,
                    secret,
                )
                .await,
            )
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
    sftp || webdav
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
    let root = crate::server_volumes::place_root(&volume_id);
    let dropped =
        sftp_volume_wiring::disconnect(&volume_id).await || webdav_volume_wiring::disconnect(&volume_id).await;
    if dropped {
        crate::volume_broadcast::emit_volume_gone(&volume_id, root.as_deref().unwrap_or_default());
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
    };
    if !forgotten {
        return false;
    }
    crate::volume_broadcast::emit_volume_gone(&id, root.as_deref().unwrap_or_default());
    // ❗ AFTER the gone event: the wiring requests its own `volumes-changed`, so
    // disconnecting first would put the republish ahead of the redirect.
    let _ = sftp_volume_wiring::disconnect(&id).await || webdav_volume_wiring::disconnect(&id).await;
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
    }
}

/// Saves an edited server, or adds one without connecting.
///
/// ❗ Takes a [`ServerTarget`], the same shape the add sheet collects, because an
/// edit and an add differ only in whether the fields arrived prefilled. A saved
/// server's PIN is not in it: `remember` preserves the stored pin on a replace,
/// and [`set_place_pinned`] is the one writer that moves one.
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
pub async fn update_saved_server(server: ServerTarget) -> SavedServerOutcome {
    let outcome = save_target(server).await;
    if outcome == SavedServerOutcome::Saved {
        crate::volume_broadcast::emit_volumes_changed();
    }
    outcome
}

/// The per-protocol save behind [`update_saved_server`].
///
/// ❗ Calls each wiring's `save_without_connecting` directly: there is no
/// per-protocol IPC command behind it any more (nothing but its own test ever
/// called `update_known_sftp_server` / `update_known_webdav_server`), so this
/// facade builds the known-servers entry itself instead of forwarding.
async fn save_target(server: ServerTarget) -> SavedServerOutcome {
    match server {
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
            sftp_volume_wiring::save_without_connecting(sftp_known_servers::KnownSftpServer {
                host,
                port,
                username,
                display_name,
                remote_root,
                start_folder,
                key_file,
                use_agent,
                auto_reconnect,
                // Only reachable for a NEW entry: editing a saved server leaves its pin
                // alone, which is `remember`'s rule, and a server nobody saved yet is
                // being saved for the first time here.
                pinned: true,
                last_connected_at: chrono::Utc::now().to_rfc3339(),
            })
            .await
        }
        ServerTarget::Webdav {
            display_name,
            url,
            username,
            remote_root,
            start_folder,
            auto_reconnect,
        } => {
            webdav_volume_wiring::save_without_connecting(webdav_known_servers::KnownWebdavServer {
                url,
                username,
                display_name,
                remote_root,
                start_folder,
                auto_reconnect,
                // Same rule as the SFTP arm above.
                pinned: true,
                last_connected_at: chrono::Utc::now().to_rfc3339(),
            })
            .await
        }
    }
}

// ============================================================================
// Shared plumbing
// ============================================================================

/// A saved entry, found by the id its place carries.
enum SavedEntry {
    Sftp(sftp_known_servers::KnownSftpServer),
    Webdav(webdav_known_servers::KnownWebdavServer),
}

/// The saved entry whose derived volume id is `volume_id`.
///
/// ❗ Derived rather than stored: the id funnel (`cmdr_fs::volume::ids`) is the
/// one place an id is minted, so looking one up means re-deriving it from the
/// same tuple rather than keeping a second copy that can drift.
fn saved_by_id(volume_id: &str) -> Option<SavedEntry> {
    let sftp = sftp_known_servers::all()
        .into_iter()
        .find(|entry| cmdr_fs::volume::sftp_volume_id(&entry.host, entry.port, &entry.username) == volume_id);
    if let Some(entry) = sftp {
        return Some(SavedEntry::Sftp(entry));
    }
    webdav_known_servers::all()
        .into_iter()
        .find(|entry| {
            webdav_params(&entry.url, &entry.username, "/").is_some_and(|params| {
                cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &entry.username) == volume_id
            })
        })
        .map(SavedEntry::Webdav)
}

/// Connection params for a saved WebDAV entry, or `None` when its address isn't
/// an `http`/`https` URL.
fn webdav_params(url: &str, username: &str, remote_root: &str) -> Option<WebdavConnectionParams> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| WebdavConnectionParams::new(parsed, username, remote_root))
}

/// The SFTP wiring's outcome, widened into the superset.
fn outcome_from_sftp(connection: SftpConnection) -> ServerConnectOutcome {
    match connection {
        // The rung is dropped here: it is a fact about THIS dial, and the
        // superset's consumers ask `get_sftp_unattended_reconnect` when a banner
        // renders rather than deriving one from a stale rung.
        SftpConnection::Connected { volume_id, .. } => ServerConnectOutcome::Connected { volume_id },
        SftpConnection::NeedsHostKeyApproval(prompt) => ServerConnectOutcome::NeedsHostKeyApproval(prompt),
        SftpConnection::HostKeyRevoked { algorithm, fingerprint } => {
            ServerConnectOutcome::HostKeyRevoked(SftpHostKeyIdentity { algorithm, fingerprint })
        }
        SftpConnection::AuthenticationRejected => ServerConnectOutcome::AuthenticationRejected,
        SftpConnection::NeedsCredentials => ServerConnectOutcome::NeedsCredentials,
        SftpConnection::TimedOut => ServerConnectOutcome::TimedOut,
        SftpConnection::Unreachable => ServerConnectOutcome::Unreachable,
        SftpConnection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}

/// The WebDAV wiring's outcome, widened into the superset.
fn outcome_from_webdav(connection: WebdavConnection) -> ServerConnectOutcome {
    match connection {
        WebdavConnection::Connected { volume_id } => ServerConnectOutcome::Connected { volume_id },
        WebdavConnection::AuthenticationRejected => ServerConnectOutcome::AuthenticationRejected,
        WebdavConnection::NeedsCredentials => ServerConnectOutcome::NeedsCredentials,
        // ❗ Its own variant, ❌ never folded into `AuthenticationRejected`: a
        // Digest-only server never saw the password.
        WebdavConnection::AuthMethodUnsupported => ServerConnectOutcome::AuthMethodUnsupported,
        WebdavConnection::CertificateUntrusted => ServerConnectOutcome::CertificateUntrusted,
        WebdavConnection::NotAWebdavServer => ServerConnectOutcome::NotAWebdavServer,
        WebdavConnection::TimedOut => ServerConnectOutcome::TimedOut,
        WebdavConnection::Unreachable => ServerConnectOutcome::Unreachable,
        WebdavConnection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}

#[cfg(test)]
#[path = "servers_test.rs"]
mod servers_test;
