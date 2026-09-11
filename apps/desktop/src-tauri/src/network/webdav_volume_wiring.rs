//! WebDAV volume wiring: turns "the user asked for this server" into a
//! registered `WebdavVolume`.
//!
//! ❗ **A backend never registers itself.** This module knows both the backend
//! and the volume registry, and neither of those knows this module — the same
//! shape `sftp_volume_wiring` and `network::smb_upgrade` take. The rule and its
//! rationale: `network/DETAILS.md` § "Backends never register themselves".
//!
//! The commands above this (`commands/webdav.rs`) are pass-throughs; the flow
//! lives here, because a connect is three things happening in one order: dial,
//! register (retiring any predecessor), and remember the server for next time.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_webdav::{UnattendedReconnect, WebdavConnectError, WebdavConnectionParams, WebdavVolume};

use super::connect_wiring::{self, AttemptTable};
use super::live_server_edit::{self, ConnectedPlace, PlaceEdit};
use super::one_shot_credentials::{self, SecretOffer};
use super::saved_server_fields::{self, SavedServerOutcome};
use super::webdav_known_servers::{self, KnownWebdavServer};

/// What a connect attempt produced, in the terms a sign-in UI branches on.
///
/// ❗ Every outcome is a variant, including the ones that look like failures.
/// The frontend has to tell "the password is wrong" from "nothing was ever
/// offered" from "the certificate isn't trusted", and ❌ none of those may be
/// recovered from a message.
pub enum WebdavConnection {
    /// A live volume, registered under `volume_id`.
    Connected {
        /// The id every listing, tab, and index entry is filed under.
        volume_id: String,
    },
    /// The server answered 401 to the credential offered. ❗ Retrying with the
    /// same secret can lock the account, so only a freshly typed one moves this
    /// forward.
    AuthenticationRejected,
    /// The server wants a credential and none was stored. ❗ Not the same as a
    /// rejection, and telling someone who has never entered a password that
    /// theirs is wrong is what collapsing the two does.
    NeedsCredentials,
    /// The server challenged with no scheme this backend speaks (a Digest-only
    /// server). ❗ Not a rejection either: the secret was never offered, so
    /// nothing about it is known to be wrong.
    AuthMethodUnsupported,
    /// The TLS handshake didn't trust the server's certificate. ❌ Not
    /// approvable from here: the fix is trusting the CA in the OS store.
    CertificateUntrusted,
    /// The URL answers HTTP, but not WebDAV: the `PROPFIND` probe came back
    /// without a `multistatus`, or isn't understood at that path.
    NotAWebdavServer,
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
    /// No route, refused, DNS, or a transport-level breakdown.
    Unreachable,
    /// The user called it off. ❗ Nothing was registered, remembered, or stored,
    /// so there is nothing to report and nothing to retry.
    Cancelled,
}

// ============================================================================
// Calling a connect off
// ============================================================================

/// The connect attempts a user could still call off, and the guard that empties
/// the table. The mechanism, and why each backend holds its OWN table:
/// `connect_wiring.rs`.
static ATTEMPTS: AttemptTable = AttemptTable::new("a webdav");

/// Calls off the connect filed under `attempt_id`, answering whether one was
/// running. An id nobody is holding is a plain `false`.
pub fn cancel_connect(attempt_id: &str) -> bool {
    ATTEMPTS.cancel(attempt_id)
}

/// Dials `params`, and on success registers the volume and remembers the server.
///
/// `attempt_id` is the caller's own name for this attempt, and what
/// [`cancel_connect`] needs to call it off. ❗ A cancelled connect leaves
/// nothing behind: no volume, no saved server, no secret.
///
/// `secret` is what a sign-in sheet just collected, and `None` is every dial that
/// isn't answering one (the dial then reads the store, as it always has). ❗
/// `remember: false` runs the dial against a store wrapper that answers this one
/// account from memory and forgets it when the attempt ends, so ❌ no secret
/// reaches the Keychain and none is held by the volume:
/// `one_shot_credentials.rs`.
///
/// Every dial goes through `cmdr_webdav::connect_webdav_volume`, which is where
/// the probe (one `PROPFIND Depth: 0` on the root) and the credential lookup
/// live. Calling one OFF goes through the token, which is what makes it answer
/// `Cancelled`.
///
/// `display_name` and `start_folder` aren't connection params: they travel
/// beside `params` into the saved entry, which a connect rebuilds whole. ❗ So a
/// caller that doesn't set them passes the SAVED values, or a connect would wipe
/// what an edit stored. A start folder the root no longer holds is dropped here
/// rather than saved. The volume is named by the label
/// (`saved_server_fields::server_label`), so an unnamed server's tab and switcher
/// row both read `username@host`.
pub async fn connect_and_register(
    display_name: &str,
    start_folder: Option<String>,
    params: WebdavConnectionParams,
    attempt_id: &str,
    secret: Option<SecretOffer>,
) -> WebdavConnection {
    let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &params.username);
    let start_folder = saved_server_fields::start_folder_for_root(&params.remote_root.to_string_lossy(), start_folder);
    let (host, _offer) =
        one_shot_credentials::host_for_dial(&params.credential_service(), &params.username, secret).await;
    let (cancel, _attempt) = ATTEMPTS.register(attempt_id);
    let label = saved_server_fields::server_label(display_name, &params.username, params.host());
    let outcome = cmdr_webdav::connect_webdav_volume(&label, &volume_id, params.clone(), host, cancel).await;

    let volume = match outcome {
        Ok(volume) => volume,
        Err(e) => return failed(e),
    };

    connect_wiring::install_retiring_incumbent(&volume_id, Arc::new(volume)).await;
    webdav_known_servers::remember(KnownWebdavServer {
        url: params.base_url.to_string(),
        username: params.username.clone(),
        display_name: display_name.to_string(),
        remote_root: params.remote_root.to_string_lossy().to_string(),
        start_folder,
        auto_reconnect: params.auto_reconnect,
        // A first connect pins the new place; `remember` carries the stored value
        // across for a server that is already saved, so a reconnect can't re-pin
        // one the user unpinned.
        pinned: true,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
    });
    log::info!(target: "volume", "registered WebDAV volume {volume_id}");
    WebdavConnection::Connected { volume_id }
}

/// The typed connect errors, widened into the outcome the frontend branches on.
///
/// The diagnostic string stays in the log: ❌ no backend prose reaches a user,
/// and the frontend's own copy is what a person reads.
fn failed(error: WebdavConnectError) -> WebdavConnection {
    match error {
        WebdavConnectError::AuthenticationRejected => WebdavConnection::AuthenticationRejected,
        WebdavConnectError::NeedsCredentials => WebdavConnection::NeedsCredentials,
        WebdavConnectError::AuthMethodUnsupported => WebdavConnection::AuthMethodUnsupported,
        WebdavConnectError::CertificateUntrusted => WebdavConnection::CertificateUntrusted,
        WebdavConnectError::NotAWebdavServer => WebdavConnection::NotAWebdavServer,
        WebdavConnectError::TimedOut => WebdavConnection::TimedOut,
        WebdavConnectError::Cancelled => WebdavConnection::Cancelled,
        WebdavConnectError::Unreachable(what) | WebdavConnectError::Transport(what) => {
            log::info!(target: "volume", "a webdav connection didn't come up: {what}");
            WebdavConnection::Unreachable
        }
    }
}

/// Saves a server without dialing it: an edit, or an add that doesn't connect.
///
/// ❗ The saved entry is the durable copy and a CONNECTED place holds a live one,
/// so an edit moves both, with no re-probe: the label and the root through an
/// instance sharing the client, the "reconnect automatically" switch at once,
/// and the root the next unattended re-probe asks for. The live client confirms
/// a moved root or start folder first, and ❗ a refusal moves NOTHING, the store
/// included. The order and the refusals: `live_server_edit.rs`.
pub async fn save_without_connecting(server: KnownWebdavServer) -> SavedServerOutcome {
    let Ok(start_folder) =
        saved_server_fields::start_folder_under_root(&server.remote_root, server.start_folder.as_deref())
    else {
        return SavedServerOutcome::StartFolderOutsideRoot;
    };
    let server = KnownWebdavServer { start_folder, ..server };
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // A URL no dial could open has no volume id, so nothing is connected under it.
    // Typed rather than a guess at the id's shape, the same way `disconnect` asks.
    let connected = server.endpoint().and_then(|(host, port)| {
        let volume_id = cmdr_fs::volume::webdav_volume_id(&host, port, &server.username);
        let live = manager
            .get(&volume_id)
            .filter(|volume| volume.as_any().is::<WebdavVolume>())?;
        Some((
            volume_id,
            cmdr_fs::volume::webdav_app_root(&host, port, &server.username),
            live,
        ))
    });
    let Some((volume_id, app_prefix, live)) = connected else {
        // Not connected: the store is the only copy, and the next connect dials it.
        webdav_known_servers::remember(server);
        return SavedServerOutcome::Saved;
    };
    let webdav = live
        .as_any()
        .downcast_ref::<WebdavVolume>()
        .expect("filtered to a WebdavVolume above");
    let place = ConnectedPlace {
        volume_id,
        live: Arc::clone(&live),
        app_prefix,
        saved_start_folder: webdav_known_servers::find(&server.url, &server.username)
            .and_then(|saved| saved.start_folder),
    };
    let label = server.label();
    let edit = PlaceEdit {
        label: &label,
        remote_root: &server.remote_root,
        start_folder: server.start_folder.as_deref(),
    };
    let accepted = match live_server_edit::check(
        &place,
        &edit,
        |name: &str, root: &Path| Arc::new(webdav.sharing_connection(name, root)) as Arc<dyn Volume>,
        live_server_edit::CHECK_BUDGET,
    )
    .await
    {
        Ok(accepted) => accepted,
        Err(refusal) => return refusal,
    };
    webdav.set_auto_reconnect(server.auto_reconnect);
    webdav.set_redial_root(Path::new(&server.remote_root));
    webdav_known_servers::remember(server);
    accepted.install(manager);
    SavedServerOutcome::Saved
}

/// Whether an unattended reconnect can actually happen for a mounted volume.
///
/// `None` when nothing WebDAV is registered under that id, which is the honest
/// answer: the precondition is a fact about a live session, and there is no
/// session to have one.
pub async fn unattended_reconnect(volume_id: &str) -> Option<UnattendedReconnect> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume = manager.get(volume_id)?;
    let webdav = volume.as_any().downcast_ref::<WebdavVolume>()?;
    Some(webdav.unattended_reconnect().await)
}

/// Drops a WebDAV volume's client and takes it out of the registry.
///
/// Answers whether there was one to disconnect. HTTP has no session to close:
/// `WebdavVolume::disconnect` drops the client, and its pooled connections go
/// with it.
pub async fn disconnect(volume_id: &str) -> bool {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let Some(volume) = manager.get(volume_id) else {
        return false;
    };
    // Typed rather than a guess at the id's shape: only a `WebdavVolume` has a
    // client to drop, and asking the value itself is what makes "is this a
    // WebDAV volume?" a fact instead of a string match.
    let Some(webdav) = volume.as_any().downcast_ref::<WebdavVolume>() else {
        return false;
    };
    webdav.disconnect().await;
    manager.unregister(volume_id);
    crate::volume_broadcast::emit_volumes_changed();
    log::info!(target: "volume", "disconnected WebDAV volume {volume_id}");
    true
}

#[cfg(test)]
#[path = "webdav_volume_wiring_test.rs"]
mod webdav_volume_wiring_test;
