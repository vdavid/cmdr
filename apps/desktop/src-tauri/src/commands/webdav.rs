//! The IPC surface for WebDAV servers: connecting, secrets, and the server list.
//!
//! Pass-throughs. The connect flow lives in `network::webdav_volume_wiring`, the
//! server list in `network::webdav_known_servers`, and the secret store in
//! `network::keychain`. What lives HERE is the wire vocabulary: every type a
//! command hands the frontend, in one file, so the sign-in UI can be built from
//! this plus `crates/cmdr-webdav/DETAILS.md`.
//!
//! ❌ **No result here is a string.** A sign-in UI branches on "the password was
//! refused" against "nothing was ever offered" against "the certificate isn't
//! trusted", and recovering any of those from a message breaks the first time
//! the copy is edited.

use serde::{Deserialize, Serialize};

use crate::network::keychain::{self, KeychainError};
use crate::network::webdav_known_servers::{self, KnownWebdavServer};
use crate::network::webdav_volume_wiring::{self, WebdavConnection};
use cmdr_webdav::{UnattendedReconnect, WebdavConnectionParams};

// ============================================================================
// The wire vocabulary
// ============================================================================

/// A live WebDAV volume, as the connect that made it saw it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedWebdavVolume {
    /// The id every listing, tab, saved path, and index entry is filed under.
    /// Derived from `host:port:username`, so two accounts on one server are two
    /// volumes.
    pub volume_id: String,
}

/// What connecting produced.
///
/// ❗ Every outcome is a variant, including the ones that read as failures: the
/// sign-in UI branches on all of them, and ❌ none may be recovered from a
/// message.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
// Snake-case variant names, the house style for a wire enum (`VolumeConnection`,
// `SftpConnectResult`, `KeychainError`). Field names inside stay camelCase, from
// each payload struct's own attribute.
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum WebdavConnectResult {
    /// A live volume, already registered and already in the server list.
    Connected(ConnectedWebdavVolume),
    /// The URL didn't parse, or its scheme is neither `http` nor `https`. ❗ Typed
    /// rather than a message: the form marks the field, and the user fixes it.
    InvalidUrl,
    /// The server refused the credential. ❗ Retrying with the same secret can
    /// lock the account; only a freshly typed one moves this forward.
    AuthenticationRejected,
    /// The server wants a credential and nothing is stored. ❗ Not a rejection,
    /// and saying "wrong password" to someone who has never entered one is what
    /// collapsing the two does.
    NeedsCredentials,
    /// The server challenged with no scheme this backend speaks (a Digest-only
    /// server). ❌ Don't offer "check your password" as the fix; the secret was
    /// never offered.
    AuthMethodUnsupported,
    /// The TLS certificate isn't trusted by the OS store. ❌ Not approvable from
    /// the app: the fix is trusting the CA where the OS keeps them.
    CertificateUntrusted,
    /// The URL answers HTTP, but not WebDAV.
    NotAWebdavServer,
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
    /// No route, refused, DNS, or a transport-level breakdown.
    Unreachable,
    /// `cancel_webdav_connect` was called for this attempt. ❗ Nothing was
    /// registered, remembered, or stored, so there is nothing to say about it
    /// beyond closing the dialog.
    Cancelled,
}

/// Whether a WebDAV volume can actually come back on its own as it stands.
///
/// ❗ **The backend's answer to "the switch is on and nothing happens".** The two
/// switches are independent — "remember the secret" is exactly a Keychain entry
/// (`has_webdav_credentials` reads it, `save_webdav_credentials` / delete move
/// it), and "reconnect automatically" is exactly this one — but their
/// COMBINATION has a precondition, and this enum is where it's said out loud.
/// ❌ Never derive it in the frontend from a credential check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WebdavUnattendedReconnect {
    /// On, and it works. Nothing to show.
    Possible,
    /// The switch is off. Nothing redials on its own, whatever is remembered. A
    /// person reconnects by hand, and that is the whole story.
    SwitchOff,
    /// ❗ On, and it can't do anything: this volume signs in from the secret store
    /// and nothing is stored. **This is the state a UI warns about**, and the way
    /// out is remembering the secret.
    NoStoredSecret,
}

impl From<UnattendedReconnect> for WebdavUnattendedReconnect {
    fn from(answer: UnattendedReconnect) -> Self {
        match answer {
            UnattendedReconnect::Possible => Self::Possible,
            UnattendedReconnect::SwitchOff => Self::SwitchOff,
            UnattendedReconnect::NoStoredSecret => Self::NoStoredSecret,
        }
    }
}

// ============================================================================
// Connecting
// ============================================================================

/// Parses what a sign-in form typed into the URL the backend dials.
///
/// `None` for anything that isn't an absolute `http` or `https` URL. ❗ Typed:
/// the command answers `InvalidUrl`, and no parse message crosses IPC.
fn parse_base_url(url: &str) -> Option<url::Url> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}

/// Opens a WebDAV volume, or says what stands in the way.
///
/// On success the volume is registered under its id and the server is added to
/// the known-servers list, so a picker sees it next launch.
///
/// ❗ Secrets are ❌ NOT arguments. The password comes from the secret store
/// (`save_webdav_credentials`) at the moment the client is built and dies with
/// it.
///
/// ❗ `attempt_id` is the CALLER's own name for this attempt, and
/// `cancel_webdav_connect` takes the same one. A fresh value per call
/// (`crypto.randomUUID()`) is what a dialog wants, and it has to be made BEFORE
/// the call: this command doesn't answer until the connect is over, which is far
/// too late to arm a cancel button.
#[tauri::command]
#[specta::specta]
pub async fn connect_webdav_volume(
    display_name: String,
    url: String,
    username: String,
    remote_root: String,
    auto_reconnect: bool,
    attempt_id: String,
) -> WebdavConnectResult {
    let Some(base_url) = parse_base_url(&url) else {
        return WebdavConnectResult::InvalidUrl;
    };
    let mut params = WebdavConnectionParams::new(base_url, &username, remote_root);
    params.auto_reconnect = auto_reconnect;

    match webdav_volume_wiring::connect_and_register(&display_name, params, &attempt_id).await {
        WebdavConnection::Connected { volume_id } => {
            WebdavConnectResult::Connected(ConnectedWebdavVolume { volume_id })
        }
        WebdavConnection::AuthenticationRejected => WebdavConnectResult::AuthenticationRejected,
        WebdavConnection::NeedsCredentials => WebdavConnectResult::NeedsCredentials,
        WebdavConnection::AuthMethodUnsupported => WebdavConnectResult::AuthMethodUnsupported,
        WebdavConnection::CertificateUntrusted => WebdavConnectResult::CertificateUntrusted,
        WebdavConnection::NotAWebdavServer => WebdavConnectResult::NotAWebdavServer,
        WebdavConnection::TimedOut => WebdavConnectResult::TimedOut,
        WebdavConnection::Unreachable => WebdavConnectResult::Unreachable,
        WebdavConnection::Cancelled => WebdavConnectResult::Cancelled,
    }
}

/// Calls off the connect running under `attempt_id`, answering whether one was.
///
/// ❗ The way out of a connect that is going nowhere. The probe stops where it
/// stands, and `connect_webdav_volume` answers `cancelled`.
///
/// ❗ A cancelled connect leaves ❌ no volume registered, ❌ no server remembered,
/// and ❌ no secret written.
///
/// An id nobody is connecting under answers `false`: a cancel racing a connect
/// that just finished is ordinary, and there is nothing wrong to report.
#[tauri::command]
#[specta::specta]
pub async fn cancel_webdav_connect(attempt_id: String) -> bool {
    webdav_volume_wiring::cancel_connect(&attempt_id)
}

/// Drops a WebDAV volume's client and takes it out of the volume registry.
///
/// Answers whether there was a WebDAV volume under that id.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_webdav_volume(volume_id: String) -> bool {
    webdav_volume_wiring::disconnect(&volume_id).await
}

// ============================================================================
// Secrets
// ============================================================================

/// How a server's secret is keyed in the store.
///
/// ❗ `{scheme}://{host}:{port}` as the service and the username as the scope,
/// ❌ never the host alone: two accounts on one server would share an entry, and
/// a reconnect could retry the wrong account's secret straight into a lockout.
/// ❗ Built by the crate's own `credential_service`, so what this writes is
/// exactly what the dial reads back. `None` when the URL doesn't parse.
fn credential_key(url: &str, username: &str) -> Option<String> {
    let base_url = parse_base_url(url)?;
    Some(WebdavConnectionParams::new(base_url, username, "/").credential_service())
}

/// The answer when the secret store can't be asked about a URL that isn't one.
///
/// ❗ `Other`, ❌ not `AccessDenied`: a URL that never named a server is not the
/// same event as a user saying no.
fn not_a_server_url() -> KeychainError {
    KeychainError::Other("the address isn't a server URL".to_string())
}

/// Saves the secret for one account on one server.
///
/// ❗ **This command IS the "remember the secret" switch.** Its meaning is exactly
/// "put this in the Keychain" and ❌ nothing else: `has_webdav_credentials` reads
/// the switch back and `delete_webdav_credentials` turns it off, so there is no
/// second flag anywhere that could disagree with the store.
///
/// ❗ Remembering a secret is what makes unattended reconnects POSSIBLE; it
/// doesn't turn them on. That is the other switch
/// (`update_known_webdav_server`'s `auto_reconnect`), and
/// `get_webdav_unattended_reconnect` is what says whether the two add up.
///
/// ❗ On a blocking task: the store can put a Keychain prompt in front of this,
/// and a modal dialog on the async runtime stalls every other volume.
#[tauri::command]
#[specta::specta]
pub async fn save_webdav_credentials(url: String, username: String, secret: String) -> Result<(), KeychainError> {
    let Some(service) = credential_key(&url, &username) else {
        return Err(not_a_server_url());
    };
    crate::commands::util::blocking_with_timeout(
        std::time::Duration::from_secs(15),
        Err(keychain_timed_out()),
        move || keychain::save_credentials(&service, Some(&username), &username, &secret),
    )
    .await
}

/// Whether a secret is stored for one account on one server.
///
/// ❗ There is deliberately no command that HANDS the secret to the frontend: the
/// backend reads the store itself at the moment it builds a client, and a secret
/// that crosses IPC is a secret in a renderer process.
///
/// A store that didn't answer in time reads as `false`, which is the one place
/// collapsing a timeout into its fallback is harmless: both answers send the
/// frontend to the same place, which is to ask.
#[tauri::command]
#[specta::specta]
pub async fn has_webdav_credentials(url: String, username: String) -> bool {
    let Some(service) = credential_key(&url, &username) else {
        return false;
    };
    crate::commands::util::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
        keychain::has_credentials(&service, Some(&username))
    })
    .await
}

/// Forgets the stored secret for one account on one server.
#[tauri::command]
#[specta::specta]
pub async fn delete_webdav_credentials(url: String, username: String) -> Result<(), KeychainError> {
    let Some(service) = credential_key(&url, &username) else {
        return Err(not_a_server_url());
    };
    crate::commands::util::blocking_with_timeout(
        std::time::Duration::from_secs(15),
        Err(keychain_timed_out()),
        move || keychain::delete_credentials(&service, Some(&username)),
    )
    .await
}

/// The answer when the secret store didn't come back in time.
///
/// ❗ `Other`, ❌ not `AccessDenied`: a store that never answered is not the same
/// event as a user saying no, and the frontend words those differently.
fn keychain_timed_out() -> KeychainError {
    KeychainError::Other("the secret store didn't answer".to_string())
}

// ============================================================================
// The known-servers list
// ============================================================================

/// Every WebDAV server the user has connected to.
#[tauri::command]
#[specta::specta]
pub fn get_known_webdav_servers() -> Vec<KnownWebdavServer> {
    webdav_known_servers::all()
}

/// Adds a server, or replaces the entry for the same `(url, username)`.
///
/// `connect_webdav_volume` already does this on every successful connection;
/// this is for editing one without connecting (renaming it, changing its root).
#[tauri::command]
#[specta::specta]
// Flat parameters rather than a struct, so the generated TS call site names each
// one; the shape mirrors `connect_webdav_volume`.
pub fn update_known_webdav_server(
    url: String,
    username: String,
    display_name: String,
    remote_root: String,
    auto_reconnect: bool,
) {
    // ❗ The live volume too, when there is one: the saved entry is the durable
    // copy, and a switch that only took effect on the next connect would read as
    // ignored.
    if let Some(base_url) = parse_base_url(&url) {
        let params = WebdavConnectionParams::new(base_url, &username, "/");
        let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &params.username);
        // It answers whether that volume happened to be MOUNTED, and editing a saved server while it isn't is
        // ordinary; the durable entry written below is what the caller asked for either way.
        // allowed-discarded-outcome: "no such mounted volume" is the common case here, not a failure to report.
        webdav_volume_wiring::apply_auto_reconnect(&volume_id, auto_reconnect);
    }
    webdav_known_servers::remember(KnownWebdavServer {
        url,
        username,
        display_name,
        remote_root,
        auto_reconnect,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// Whether a WebDAV volume can actually come back on its own as it stands.
///
/// ❗ **Ask this when a banner renders**: the answer depends on what is in the
/// secret store at that moment.
///
/// `null` when nothing WebDAV is registered under that id. That is the honest
/// answer rather than a guess: a saved server the user hasn't connected to yet
/// gets no warning, which is right — nothing is known to warn about.
///
/// ❗ May read the secret store (on a blocking task), so ❌ don't poll it.
#[tauri::command]
#[specta::specta]
pub async fn get_webdav_unattended_reconnect(volume_id: String) -> Option<WebdavUnattendedReconnect> {
    webdav_volume_wiring::unattended_reconnect(&volume_id)
        .await
        .map(WebdavUnattendedReconnect::from)
}

/// Drops a server from the list, answering whether one was there.
///
/// ❌ Leaves the stored secret alone: forgetting a server from a list isn't the
/// same request as revoking its credential. `delete_webdav_credentials` is that.
#[tauri::command]
#[specta::specta]
pub fn forget_known_webdav_server(url: String, username: String) -> bool {
    webdav_known_servers::forget(&url, &username)
}

#[cfg(test)]
#[path = "webdav_test.rs"]
mod webdav_test;
