//! How to reach one WebDAV server, and how to reach it again.

use std::path::PathBuf;

use url::Url;

/// Everything needed to open (and later re-open) one WebDAV volume.
///
/// ❗ No secret lives here. The password comes from the `CredentialStore` seam
/// at the moment a client is built and dies with it.
#[derive(Debug, Clone)]
pub struct WebdavConnectionParams {
    /// Scheme, host, port, and the path prefix every request goes under, e.g.
    /// `https://cloud.example.com/remote.php/dav/files/ada/`. ❗ Always ends in
    /// a slash: [`Self::new`] normalizes it, because `Url::join` treats the last
    /// segment of a slash-less path as a file and replaces it.
    pub base_url: Url,
    /// The account to sign in as. ❗ Part of the volume's identity: two accounts
    /// on one server see different files under the same paths.
    pub username: String,
    /// The directory this volume is rooted at, UNDER the base URL's path.
    /// Absolute-looking: `/` is the base URL itself, `/Photos` is
    /// `base_url + "Photos/"`. Empty, `.`, and `/` all mean the root.
    pub remote_root: PathBuf,
    /// Whether Cmdr may re-probe this server unattended when a request finds
    /// it gone. ❗ The user's own per-server switch, and ❌ independent of
    /// whether a secret is remembered (`volume::UnattendedReconnect`).
    pub auto_reconnect: bool,
}

impl WebdavConnectionParams {
    /// Params for the common case: a dropped connection comes back on its own.
    pub fn new(base_url: Url, username: &str, remote_root: impl Into<PathBuf>) -> Self {
        let mut base_url = base_url;
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        Self {
            base_url,
            username: username.to_string(),
            remote_root: remote_root.into(),
            auto_reconnect: true,
        }
    }

    /// How this server is keyed in the secret store: `scheme://host:port`, with
    /// the effective port (443 or 80 when the URL names none). The scope is
    /// `Some(username)`.
    ///
    /// ❗ Never the host alone: two accounts on one server would share an
    /// entry, and a reconnect could retry the wrong account's secret straight
    /// into a lockout.
    pub fn credential_service(&self) -> String {
        format!("{}://{}:{}", self.base_url.scheme(), self.host(), self.port())
    }

    /// The server's host name, as the URL spells it.
    pub fn host(&self) -> &str {
        self.base_url.host_str().unwrap_or_default()
    }

    /// The effective port: explicit, else the scheme's default.
    pub fn port(&self) -> u16 {
        self.base_url.port_or_known_default().unwrap_or(80)
    }
}

#[cfg(test)]
#[path = "params_test.rs"]
mod params_test;
