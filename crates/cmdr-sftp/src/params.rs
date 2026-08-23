//! How to reach one SFTP server, and how to reach it again.
//!
//! Its own module rather than a field bag inside `volume/`, because three
//! modules need it and two of them sit below the volume: `auth` builds its
//! ladder from it and `transport` dials with it. Anywhere else and those three
//! import each other in a circle.

use std::path::PathBuf;

/// Everything needed to open (and later re-open) one SFTP volume.
///
/// ❗ No secret lives here. A password and a key passphrase both come from the
/// `CredentialStore` seam at the moment a session is built and die with it; what
/// travels in this struct is the key file's PATH, which is a connection
/// parameter rather than a secret.
#[derive(Debug, Clone)]
pub struct SftpConnectionParams {
    /// The server, as the user typed it. Used verbatim for the connection and
    /// case-folded for the volume id.
    pub host: String,
    /// The TCP port. 22 everywhere but a fixture or a jump box.
    pub port: u16,
    /// The account to sign in as. ❗ Part of the volume's identity: two accounts
    /// on one server see different files under the same paths.
    pub username: String,
    /// The remote directory this volume is rooted at. Absolute, server-side.
    pub remote_root: PathBuf,
    /// A private key file to offer before falling back to a password.
    pub key_file: Option<PathBuf>,
    /// Whether the running ssh-agent may be asked. Off for a fixture that has to
    /// exercise one specific rung.
    pub use_agent: bool,
    /// Whether Cmdr may redial this server unattended when the session drops.
    ///
    /// ❗ The user's own per-server switch, and ❌ independent of whether a secret
    /// is remembered: off means nothing redials however full the store is, and on
    /// means the rung's policy decides (`auth::reconnect_policy`). Their
    /// combination has a real precondition, which `auth::UnattendedReconnect`
    /// states rather than implies.
    ///
    /// The starting value only; a live volume's copy moves with
    /// `SftpVolume::set_auto_reconnect`, so a settings change doesn't need a
    /// remount.
    pub auto_reconnect: bool,
}

impl SftpConnectionParams {
    /// Params for the common case: agent first, then whatever the store holds,
    /// and a dropped session comes back on its own.
    pub fn new(host: &str, port: u16, username: &str, remote_root: impl Into<PathBuf>) -> Self {
        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            remote_root: remote_root.into(),
            key_file: None,
            use_agent: true,
            auto_reconnect: true,
        }
    }

    /// The same params, offering `path` as a private key.
    #[must_use]
    pub fn with_key_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.key_file = Some(path.into());
        self
    }

    /// The same params with the ssh-agent left out of the ladder.
    #[must_use]
    pub fn without_agent(mut self) -> Self {
        self.use_agent = false;
        self
    }

    /// How this server is keyed in the secret store.
    ///
    /// ❗ `host:port` as the service and the username as the scope, never the
    /// host alone: two accounts on one server would share an entry, and a
    /// reconnect could retry the wrong account's secret straight into a lockout.
    pub fn credential_service(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
