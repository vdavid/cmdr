//! The IPC surface for WebDAV servers: secrets and the server list. Connecting
//! itself goes through the protocol-agnostic `commands::servers` facade now;
//! this file keeps only what that facade has no reason to widen.
//!
//! Pass-throughs. The connect flow lives in `network::webdav_volume_wiring`, the
//! server list in `network::webdav_known_servers`, and the secret store in
//! `network::keychain`. `crates/cmdr-webdav/DETAILS.md` § "Connecting from the
//! frontend" covers the sign-in flow end to end.
//!
//! ❌ **No result here is a string.** A sign-in UI branches on "the password was
//! refused" against "nothing was ever offered" against "the certificate isn't
//! trusted", and recovering any of those from a message breaks the first time
//! the copy is edited.

use serde::{Deserialize, Serialize};

use crate::network::keychain::{self, KeychainError};
use crate::network::webdav_known_servers::{self, KnownWebdavServer};
use crate::network::webdav_volume_wiring;
use cmdr_webdav::UnattendedReconnect;

// ============================================================================
// The wire vocabulary
// ============================================================================

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
// Secrets
// ============================================================================

/// How a server's secret is keyed in the store.
///
/// ❗ `{scheme}://{host}:{port}` as the service and the username as the scope,
/// ❌ never the host alone: two accounts on one server would share an entry, and
/// a reconnect could retry the wrong account's secret straight into a lockout.
/// ❗ Built by the crate's own `credential_service`, so what this writes is
/// exactly what the dial reads back (`webdav_volume_wiring::credential_service`,
/// which a move builds it through too). `None` when the URL isn't an
/// `http`/`https` URL.
fn credential_key(url: &str) -> Option<String> {
    webdav_volume_wiring::credential_service(url)
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
/// the switch back and `delete_webdav_credentials` turns it off (the frontend
/// reaches both through `servers.rs`), so there is no
/// second flag anywhere that could disagree with the store.
///
/// ❗ Remembering a secret is what makes unattended reconnects POSSIBLE; it
/// doesn't turn them on. That is the other switch
/// (`KnownWebdavServer::auto_reconnect`, moved by `update_saved_server`), and
/// `get_webdav_unattended_reconnect` is what says whether the two add up.
///
/// ❗ On a blocking task: the store can put a Keychain prompt in front of this,
/// and a modal dialog on the async runtime stalls every other volume.
#[tauri::command]
#[specta::specta]
pub async fn save_webdav_credentials(url: String, username: String, secret: String) -> Result<(), KeychainError> {
    let Some(service) = credential_key(&url) else {
        return Err(not_a_server_url());
    };
    crate::deadline::blocking_with_timeout(
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
pub(crate) async fn has_webdav_credentials(url: String, username: String) -> bool {
    let Some(service) = credential_key(&url) else {
        return false;
    };
    crate::deadline::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
        keychain::has_credentials(&service, Some(&username))
    })
    .await
}

/// Forgets the stored secret for one account on one server.
pub(crate) async fn delete_webdav_credentials(url: String, username: String) -> Result<(), KeychainError> {
    let Some(service) = credential_key(&url) else {
        return Err(not_a_server_url());
    };
    crate::deadline::blocking_with_timeout(
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

#[cfg(test)]
#[path = "webdav_test.rs"]
mod webdav_test;
