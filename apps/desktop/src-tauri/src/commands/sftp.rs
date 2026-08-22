//! The IPC surface for SFTP servers: connecting, host-key trust, secrets, and
//! the server list.
//!
//! Pass-throughs. The connect flow lives in `network::sftp_volume_wiring`, the
//! trust store in `network::sftp_host_keys`, the server list in
//! `network::sftp_known_servers`, and the secret store in `network::keychain`.
//! What lives HERE is the wire vocabulary: every type a command hands the
//! frontend, in one file, so the sign-in UI can be built from this plus
//! `crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".
//!
//! ❗ **Two carriers for host-key approval, because there are two moments.** At
//! connect, [`SftpConnectResult::NeedsHostKeyApproval`] carries the fingerprint
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
use crate::network::sftp_volume_wiring::{self, SftpConnection};
use cmdr_sftp::SftpConnectionParams;
use cmdr_sftp::auth::AuthRungUsed;
use cmdr_sftp::transport::HostKeyPrompt;
use cmdr_sftp::volume::HostKeyApproval;

// ============================================================================
// The wire vocabulary
// ============================================================================

/// Which credential proved a live session.
///
/// Flat where the backend's own enum nests, because the frontend's five banners
/// are exactly these five rows. What each may do when the session drops:
/// `crates/cmdr-sftp/DETAILS.md` § "What each rung may do, and what the frontend
/// sees".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SftpAuthRung {
    /// The ssh-agent signed. Comes back on its own until the identity goes away.
    Agent,
    /// An unencrypted key file. Comes back on its own.
    KeyFile,
    /// A passphrase-protected key file. ❗ Cannot come back unattended: the
    /// passphrase isn't held past the session it unlocked.
    EncryptedKeyFile,
    /// A password from the secret store. One unattended retry, then a person.
    Password,
    /// The server drove the prompts. Never unattended.
    KeyboardInteractive,
}

impl From<AuthRungUsed> for SftpAuthRung {
    fn from(rung: AuthRungUsed) -> Self {
        match rung {
            AuthRungUsed::Agent => Self::Agent,
            AuthRungUsed::KeyFile {
                passphrase_protected: false,
            } => Self::KeyFile,
            AuthRungUsed::KeyFile {
                passphrase_protected: true,
            } => Self::EncryptedKeyFile,
            AuthRungUsed::Password => Self::Password,
            AuthRungUsed::KeyboardInteractive => Self::KeyboardInteractive,
        }
    }
}

/// What a "Sign in" affordance may ask for on a volume built on a given rung.
///
/// ❗ The backend answers this rather than the frontend deriving it from the
/// rung, because getting it wrong ships a button that answers `NotSupported`
/// every time it's pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SftpSignInPrompt {
    /// ❌ Nothing to ask, so ❌ no sign-in button. An agent session and an
    /// unencrypted key file come back on their own; there is no secret a person
    /// could type that would help.
    Nothing,
    /// The account's password. Saved to the secret store on a successful sign-in,
    /// so the next reconnect is silent.
    Password,
    /// The passphrase on the key file. ❗ Used for that session and ❌ never
    /// saved: persisting it would undo what encrypting the key asked for.
    KeyPassphrase,
}

impl SftpSignInPrompt {
    /// What this rung's sign-in may offer.
    fn for_rung(rung: SftpAuthRung) -> Self {
        match rung {
            SftpAuthRung::Agent | SftpAuthRung::KeyFile => Self::Nothing,
            SftpAuthRung::EncryptedKeyFile => Self::KeyPassphrase,
            SftpAuthRung::Password | SftpAuthRung::KeyboardInteractive => Self::Password,
        }
    }
}

/// A live SFTP volume, and what the reconnect banner needs to know about it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedSftpVolume {
    /// The id every listing, tab, saved path, and index entry is filed under.
    /// Derived from `host:port:username`, so two accounts on one server are two
    /// volumes.
    pub volume_id: String,
    /// Which credential proved this session.
    pub rung: SftpAuthRung,
    /// What a "Sign in" affordance may ask for if this session later drops.
    pub sign_in: SftpSignInPrompt,
}

/// What connecting produced.
///
/// ❗ Every outcome is a variant, including the ones that read as failures: the
/// sign-in UI branches on all of them, and ❌ none may be recovered from a
/// message.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
// Snake-case variant names, the house style for a wire enum (`VolumeConnection`,
// `ConnectionMode`, `KeychainError`). Field names inside stay camelCase, from
// each payload struct's own attribute.
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum SftpConnectResult {
    /// A live volume, already registered and already in the server list.
    Connected(ConnectedSftpVolume),
    /// The server's host key needs a human. ❗ No session is held across the
    /// prompt: the dial has been dropped, and approving is followed by calling
    /// `connect_sftp_volume` again.
    NeedsHostKeyApproval(HostKeyPrompt),
    /// The key is explicitly revoked in `~/.ssh/known_hosts`. ❌ Not approvable
    /// at all: a revocation says this exact key is known to be compromised.
    HostKeyRevoked(SftpHostKeyIdentity),
    /// Every rung was refused. ❗ Retrying with the same secret can lock the
    /// account; only a freshly typed one moves this forward.
    AuthenticationRejected,
    /// Nothing was ever offered: no agent, no readable key file, no stored
    /// secret. ❗ Not a rejection, and saying "wrong password" to someone who has
    /// never entered one is what collapsing the two does.
    NeedsCredentials,
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
    /// No route, refused, DNS, or a server with no SFTP subsystem.
    Unreachable,
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
    /// Recorded. Call `connect_sftp_volume` again and it walks past the prompt.
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

/// Opens an SFTP volume, or says what stands in the way.
///
/// On success the volume is registered under its id and the server is added to
/// the known-servers list, so a picker sees it next launch.
///
/// ❗ Secrets are ❌ NOT arguments. A password and a key passphrase come from the
/// secret store (`save_sftp_credentials`) at the moment the session is built and
/// die with it; what travels here is the key file's PATH, which is a connection
/// parameter.
#[tauri::command]
#[specta::specta]
pub async fn connect_sftp_volume(
    display_name: String,
    host: String,
    port: u16,
    username: String,
    remote_root: String,
    key_file: Option<String>,
    use_agent: bool,
) -> SftpConnectResult {
    let mut params = SftpConnectionParams::new(&host, port, &username, remote_root);
    params.key_file = key_file.map(std::path::PathBuf::from);
    params.use_agent = use_agent;

    match sftp_volume_wiring::connect_and_register(&display_name, params).await {
        SftpConnection::Connected { volume_id, rung } => {
            let rung = SftpAuthRung::from(rung);
            SftpConnectResult::Connected(ConnectedSftpVolume {
                volume_id,
                rung,
                sign_in: SftpSignInPrompt::for_rung(rung),
            })
        }
        SftpConnection::NeedsHostKeyApproval(prompt) => SftpConnectResult::NeedsHostKeyApproval(prompt),
        SftpConnection::HostKeyRevoked { algorithm, fingerprint } => {
            SftpConnectResult::HostKeyRevoked(SftpHostKeyIdentity { algorithm, fingerprint })
        }
        SftpConnection::AuthenticationRejected => SftpConnectResult::AuthenticationRejected,
        SftpConnection::NeedsCredentials => SftpConnectResult::NeedsCredentials,
        SftpConnection::TimedOut => SftpConnectResult::TimedOut,
        SftpConnection::Unreachable => SftpConnectResult::Unreachable,
    }
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
/// After a `Recorded`, call `connect_sftp_volume` again for a fresh dial.
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
/// ❗ **One entry per account, whatever the rung uses it for.** The auth ladder
/// reads this same entry and offers it as the password on the password and
/// keyboard-interactive rungs, and as the key file's passphrase on the key-file
/// rung. So a passphrase-protected key needs its passphrase here to connect the
/// FIRST time; after that, an attended sign-in passes a typed one straight
/// through without saving.
///
/// ❗ Saving a passphrase does NOT make that rung reconnect unattended:
/// `cmdr_sftp::auth::reconnect_policy` gates on the rung, not on whether a
/// secret exists, so a passphrase-protected key still stops and asks. What it
/// costs is having the passphrase in the secret store at all, which is a
/// question for the sign-in UI to put to the user rather than one this command
/// answers.
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
    crate::commands::util::blocking_with_timeout(std::time::Duration::from_secs(15), false, move || {
        keychain::has_credentials(&service, Some(&username))
    })
    .await
}

/// Forgets the stored secret for one account on one server.
#[tauri::command]
#[specta::specta]
pub async fn delete_sftp_credentials(host: String, port: u16, username: String) -> Result<(), KeychainError> {
    let service = credential_key(&host, port);
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

/// Every SFTP server the user has connected to.
#[tauri::command]
#[specta::specta]
pub fn get_known_sftp_servers() -> Vec<KnownSftpServer> {
    sftp_known_servers::all()
}

/// Adds a server, or replaces the entry for the same `(host, port, username)`.
///
/// `connect_sftp_volume` already does this on every successful connection; this
/// is for editing one without connecting (renaming it, changing its root or its
/// key file).
#[tauri::command]
#[specta::specta]
// Seven flat parameters rather than a struct, so the generated TS call site names
// each one; the shape mirrors `connect_sftp_volume`.
#[allow(
    clippy::too_many_arguments,
    reason = "one argument per saved-server field, mirroring the connect command"
)]
pub fn update_known_sftp_server(
    host: String,
    port: u16,
    username: String,
    display_name: String,
    remote_root: String,
    key_file: Option<String>,
    use_agent: bool,
) {
    sftp_known_servers::remember(KnownSftpServer {
        host,
        port,
        username,
        display_name,
        remote_root,
        key_file,
        use_agent,
        last_connected_at: chrono::Utc::now().to_rfc3339(),
    });
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
