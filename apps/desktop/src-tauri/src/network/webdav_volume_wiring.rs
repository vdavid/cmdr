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

use std::sync::Arc;

use cmdr_webdav::{UnattendedReconnect, WebdavConnectError, WebdavConnectionParams, WebdavVolume};

use super::connect_wiring::{self, AttemptTable};
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
/// Every dial goes through `cmdr_webdav::connect_webdav_volume`, which is where
/// the probe (one `PROPFIND Depth: 0` on the root) and the credential lookup
/// live. Calling one OFF goes through the token, which is what makes it answer
/// `Cancelled`.
pub async fn connect_and_register(
    display_name: &str,
    params: WebdavConnectionParams,
    attempt_id: &str,
) -> WebdavConnection {
    let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &params.username);
    let (cancel, _attempt) = ATTEMPTS.register(attempt_id);
    let outcome = cmdr_webdav::connect_webdav_volume(
        display_name,
        &volume_id,
        params.clone(),
        crate::volume_host::host(),
        cancel,
    )
    .await;

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
        auto_reconnect: params.auto_reconnect,
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

/// Moves the "reconnect automatically" switch on a volume that is already
/// mounted, answering whether there was a WebDAV volume under that id.
///
/// ❗ The saved-server entry is the durable copy and the volume holds a live one,
/// so editing a server that happens to be open has to move both. Without this, a
/// switch the user flipped would take effect on the next connect and not before.
pub fn apply_auto_reconnect(volume_id: &str, on: bool) -> bool {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let Some(volume) = manager.get(volume_id) else {
        return false;
    };
    // Typed rather than a guess at the id's shape, the same way `disconnect` asks.
    let Some(webdav) = volume.as_any().downcast_ref::<WebdavVolume>() else {
        return false;
    };
    webdav.set_auto_reconnect(on);
    true
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
