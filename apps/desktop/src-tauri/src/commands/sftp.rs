//! The IPC surface for SFTP servers: host-key trust, secrets, and the server
//! list. Connecting itself goes through the protocol-agnostic
//! `commands::servers` facade now; this file keeps only what that facade has
//! no reason to widen.
//!
//! Pass-throughs. The connect flow lives in `network::sftp_volume_wiring`, the
//! trust store in `network::sftp_host_keys`, the server list in
//! `network::sftp_known_servers`, and the secret store in `network::keychain`.
//! `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend" covers the
//! sign-in flow end to end.
//!
//! ❗ **Two carriers for host-key approval, because there are two moments.** At
//! connect, `ServerConnectOutcome::NeedsHostKeyApproval` carries the fingerprint
//! and whether it's first contact or a CHANGED key. Mid-life, a payload-free
//! `VolumeConnection::NeedsHostKeyApproval` rides `volume-connection-changed`
//! and the user opens the server again to see the key.
//!
//! ❌ **No result here is a string.** A sign-in UI branches on "the key needs
//! approving" against "the password was refused" against "nothing was ever
//! offered", and recovering any of those from a message breaks the first time
//! the copy is edited.

use serde::{Deserialize, Serialize};

use crate::network::keychain::{self, KeychainError};
use crate::network::sftp_host_keys::{self, TrustedHostKey};
use crate::network::sftp_known_servers::{self, KnownSftpServer};
use crate::network::sftp_volume_wiring;
use cmdr_sftp::auth::UnattendedReconnect;
use cmdr_sftp::transport::HostKeyPrompt;
use cmdr_sftp::volume::HostKeyApproval;

// ============================================================================
// The wire vocabulary
// ============================================================================

/// Whether an SFTP volume can actually come back on its own as it stands.
///
/// ❗ **The backend's answer to "the switch is on and nothing happens".** The two
/// switches are independent — "remember the secret" is exactly a Keychain entry
/// (`has_sftp_credentials` reads it, `save_sftp_credentials` / delete move it),
/// and "reconnect automatically" is exactly this one — but their COMBINATION has
/// a precondition, and this enum is where it's said out loud. ❌ Never derive it
/// in the frontend from a rung plus a credential check: the rung is decided per
/// dial and the derivation goes stale the moment one lands elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SftpUnattendedReconnect {
    /// The switch is off. Nothing redials on its own, whatever is remembered. A
    /// person reconnects by hand, and that is the whole story.
    TurnedOff,
    /// On, and it works. Nothing to show.
    Ready,
    /// ❗ On, and it can't do anything: this volume signs in from the secret store
    /// and nothing is stored. **This is the state a UI warns about**, and the way
    /// out is remembering the secret.
    NeedsStoredSecret,
    /// On, but this server asks its own questions at every sign-in
    /// (keyboard-interactive, which is where 2FA lives), so no stored secret can
    /// buy an unattended reconnect. ❌ Don't offer "remember the secret" as the
    /// fix here; it isn't one.
    RungCannot,
}

impl From<UnattendedReconnect> for SftpUnattendedReconnect {
    fn from(answer: UnattendedReconnect) -> Self {
        match answer {
            UnattendedReconnect::TurnedOff => Self::TurnedOff,
            UnattendedReconnect::Ready => Self::Ready,
            UnattendedReconnect::NeedsStoredSecret => Self::NeedsStoredSecret,
            UnattendedReconnect::RungCannot => Self::RungCannot,
        }
    }
}

/// One host key, named the way a human checks it against `ssh-keygen -lf`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SftpHostKeyIdentity {
    /// The SSH key-type name the server presented.
    pub algorithm: String,
    /// Its OpenSSH `SHA256:…` fingerprint.
    pub fingerprint: String,
}

/// What approving a host key produced.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum SftpHostKeyApprovalResult {
    /// Recorded. Dialing again (`connectServer` / `connectSavedPlace`) walks past the prompt.
    Recorded,
    /// ❗ Nothing was recorded: the server presents a different key now than the
    /// one that was approved. Carries what it presents, so the flow starts over
    /// on the real key rather than silently trusting it.
    Superseded(HostKeyPrompt),
    /// The server couldn't be re-asked, so nothing was recorded. Approving is a
    /// live question, and an unanswered one is not a yes.
    Unreachable,
}

// ============================================================================
// Connecting
// ============================================================================

/// Calls off the connect running under `attempt_id`, answering whether one was.
///
/// ❗ The way out of a connect that is going nowhere. A dial can hold for up to
/// 30 s across its three phases, and this ends the user's wait at once: the key
/// exchange and the auth ladder stop where they stand, and a cancel landing in
/// the SFTP hello lets the engine finish quietly on its own and throws away what
/// it built (`crates/cmdr-sftp/DETAILS.md` § "Cancelling a connect").
///
/// ❗ A cancelled connect leaves ❌ no volume registered, ❌ no server remembered,
/// and ❌ no secret written. The connect command (`connectServer` /
/// `connectSavedPlace`) answers `cancelled`.
///
/// An id nobody is connecting under answers `false`: a cancel racing a connect
/// that just finished is ordinary, and there is nothing wrong to report.
#[tauri::command]
#[specta::specta]
pub async fn cancel_sftp_connect(attempt_id: String) -> bool {
    sftp_volume_wiring::cancel_connect(&attempt_id)
}

/// Drops an SFTP volume's session and takes it out of the volume registry.
///
/// Answers whether there was an SFTP volume under that id. ❗ Dropping the
/// session IS the shutdown; there is no `close()` to call, and the one the
/// protocol crate offers hangs forever over an SSH channel.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_sftp_volume(volume_id: String) -> bool {
    sftp_volume_wiring::disconnect(&volume_id).await
}

// ============================================================================
// Host-key trust
// ============================================================================

/// Records a host key the user approved, ❗ only if the server still presents it.
///
/// The second half of the two-phase flow. Time passes between the prompt and the
/// click, so the fingerprint is re-checked against a fresh key exchange before
/// anything is written: that is what stops an approval being replayed against a
/// key the user never read. The re-check offers no credential, so it can never
/// spend an authentication attempt.
///
/// After a `Recorded`, dial again (`connectServer` / `connectSavedPlace`) for a fresh dial.
#[tauri::command]
#[specta::specta]
pub async fn approve_sftp_host_key(
    host: String,
    port: u16,
    algorithm: String,
    fingerprint: String,
) -> SftpHostKeyApprovalResult {
    match sftp_volume_wiring::approve_host_key(&host, port, &algorithm, &fingerprint).await {
        Ok(HostKeyApproval::Recorded) => SftpHostKeyApprovalResult::Recorded,
        Ok(HostKeyApproval::Superseded(prompt)) => SftpHostKeyApprovalResult::Superseded(prompt),
        Err(e) => {
            log::info!(target: "volume", "couldn't re-check an sftp host key before approving it: {e:?}");
            SftpHostKeyApprovalResult::Unreachable
        }
    }
}

/// Drops the approval for `(host, port, algorithm)`, so the next connection to
/// that server is first contact again.
///
/// Answers whether anything was there. ❌ Doesn't touch `~/.ssh/known_hosts`: a
/// server trusted through that file stays trusted, and `ssh-keygen -R` is what
/// forgets one of those.
#[tauri::command]
#[specta::specta]
pub fn forget_sftp_host_key(host: String, port: u16, algorithm: String) -> bool {
    sftp_host_keys::forget_trusted_host_key(&host, port, &algorithm)
}

/// Every SSH host key this machine has approved, for a settings screen.
#[tauri::command]
#[specta::specta]
pub fn list_trusted_sftp_host_keys() -> Vec<TrustedHostKey> {
    sftp_host_keys::list_trusted_host_keys()
}

// ============================================================================
// Secrets
// ============================================================================

/// How a server's secret is keyed in the store.
///
/// ❗ `host:port` as the service and the username as the scope, ❌ never the host
/// alone: two accounts on one server would share an entry, and a reconnect could
/// retry the wrong account's secret straight into a lockout.
fn credential_key(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

/// Saves the secret for one account on one server.
///
/// ❗ **This command IS the "remember the secret" switch.** Its meaning is exactly
/// "put this in the Keychain" and ❌ nothing else: `has_sftp_credentials` reads
/// the switch back and `delete_sftp_credentials` turns it off, so there is no
/// second flag anywhere that could disagree with the store.
///
/// ❗ **One entry per account, whatever the rung uses it for.** The auth ladder
/// reads this same entry and offers it as the password on the password and
/// keyboard-interactive rungs, and as the key file's passphrase on the key-file
/// rung. So a passphrase-protected key needs its passphrase here to connect the
/// FIRST time.
///
/// ❗ Remembering a secret is what makes unattended reconnects POSSIBLE on the
/// password and encrypted-key rungs; it doesn't turn them on. That is the other
/// switch (`KnownSftpServer::auto_reconnect`, moved by `update_saved_server`),
/// and `get_sftp_unattended_reconnect` is what says whether the two add up.
///
/// ❗ On a blocking task: the store can put a Keychain prompt in front of this,
/// and a modal dialog on the async runtime stalls every other volume.
#[tauri::command]
#[specta::specta]
pub async fn save_sftp_credentials(
    host: String,
    port: u16,
    username: String,
    secret: String,
) -> Result<(), KeychainError> {
    let service = credential_key(&host, port);
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
/// backend reads the store itself at the moment it builds a session, and a
/// secret that crosses IPC is a secret in a renderer process.
///
/// A store that didn't answer in time reads as `false`, which is the one place
/// collapsing a timeout into its fallback is harmless: both answers send the
/// frontend to the same place, which is to ask.
#[tauri::command]
#[specta::specta]
pub async fn has_sftp_credentials(host: String, port: u16, username: String) -> bool {
    let service = credential_key(&host, port);
    crate::deadline::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
        keychain::has_credentials(&service, Some(&username))
    })
    .await
}

/// Forgets the stored secret for one account on one server.
#[tauri::command]
#[specta::specta]
pub async fn delete_sftp_credentials(host: String, port: u16, username: String) -> Result<(), KeychainError> {
    let service = credential_key(&host, port);
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

/// Every SFTP server the user has connected to.
#[tauri::command]
#[specta::specta]
pub fn get_known_sftp_servers() -> Vec<KnownSftpServer> {
    sftp_known_servers::all()
}

/// Whether an SFTP volume can actually come back on its own as it stands.
///
/// ❗ **Ask this when a banner renders**, the same way `get_volume_sign_in_state`
/// is asked: the rung is decided per DIAL, so an answer captured at connect goes
/// stale the moment a reconnect lands on another rung.
///
/// `null` when nothing SFTP is registered under that id. That is the honest
/// answer rather than a guess: without a live session there is no rung, and the
/// precondition depends on one. A saved server the user hasn't connected to yet
/// therefore gets no warning, which is right — nothing is known to warn about.
///
/// ❗ May read the secret store (on a blocking task, and only for the rungs that
/// redial out of it), so ❌ don't poll it.
#[tauri::command]
#[specta::specta]
pub async fn get_sftp_unattended_reconnect(volume_id: String) -> Option<SftpUnattendedReconnect> {
    sftp_volume_wiring::unattended_reconnect(&volume_id)
        .await
        .map(SftpUnattendedReconnect::from)
}

/// Drops a server from the list, answering whether one was there.
///
/// ❌ Leaves the stored secret and the trusted host key alone: forgetting a
/// server from a list isn't the same request as revoking its credential or its
/// identity. `delete_sftp_credentials` and `forget_sftp_host_key` are those.
#[tauri::command]
#[specta::specta]
pub fn forget_known_sftp_server(host: String, port: u16, username: String) -> bool {
    sftp_known_servers::forget(&host, port, &username)
}

#[cfg(test)]
#[path = "sftp_test.rs"]
mod sftp_test;
