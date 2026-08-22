//! The SFTP servers this user has connected to, so the next time is one click.
//!
//! ❗ **A store of its own rather than a widened `KnownNetworkShare`.** That type
//! is shaped around SMB: it carries a `share_name` an SFTP server has no
//! equivalent of, and an `AuthOptions` that can only say guest-or-credentials —
//! which can express neither a key file nor an ssh-agent. Bending it would leave
//! two backends sharing fields that mean different things in each.
//!
//! ❌ **No secret lives here.** A password and a key passphrase go to the
//! `CredentialStore` seam (`credential_store.rs`), keyed `service = "host:port"`
//! and `scope = Some(username)`. What an entry carries is the key file's PATH,
//! which is a connection parameter rather than a secret.
//!
//! An entry is keyed by `(host, port, username)`, the same triple the volume id
//! is derived from (`cmdr_fs::volume::sftp_volume_id`), so two accounts on one
//! server are two entries and two volumes rather than one of each.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::ignore_poison::IgnorePoison;

/// One SFTP server the user has connected to, and how to reach it again.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownSftpServer {
    /// The server, as the user typed it.
    pub host: String,
    /// Its port. 22 everywhere but a jump box or a container.
    pub port: u16,
    /// The account to sign in as. ❗ Part of the identity: two accounts on one
    /// server see different files under the same paths.
    pub username: String,
    /// What to call it in the UI. The user's own label, falling back to the host
    /// when they never gave one.
    pub display_name: String,
    /// The remote directory to open at. Absolute, server-side.
    pub remote_root: String,
    /// A private key file to offer. ❗ A path, not a secret: its passphrase (if
    /// it has one) lives in the secret store and dies with the session it
    /// unlocked.
    pub key_file: Option<String>,
    /// Whether the running ssh-agent may be asked.
    pub use_agent: bool,
    /// When this server was last connected to, ISO 8601, so a picker can sort by
    /// recency.
    pub last_connected_at: String,
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownSftpServersStore {
    /// Every server, in the order they were first added.
    #[serde(default)]
    pub known_sftp_servers: Vec<KnownSftpServer>,
}

/// In-memory mirror of the file, so a picker render is not a disk read.
static KNOWN: OnceLock<Mutex<KnownSftpServersStore>> = OnceLock::new();

/// Where the file lives once the app has told us its data dir.
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn known() -> &'static Mutex<KnownSftpServersStore> {
    KNOWN.get_or_init(|| Mutex::new(KnownSftpServersStore::default()))
}

/// Whether two entries describe the same server and account.
///
/// The host folds case (DNS is case-insensitive) and the account does not (POSIX
/// accounts are case-sensitive, so `Ada` and `ada` may be two people). Same rule
/// as `cmdr_fs::volume::sftp_volume_id`, and it has to stay that way or a server
/// would get two entries and one volume.
fn same_server(entry: &KnownSftpServer, host: &str, port: u16, username: &str) -> bool {
    entry.host.eq_ignore_ascii_case(host) && entry.port == port && entry.username == username
}

/// Loads the store from the app's data dir into memory.
///
/// Call once at startup. A missing or unreadable file is an empty store: the
/// user's server list is a convenience, and losing it costs one re-entry rather
/// than access to anything.
pub fn load_known_sftp_servers<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(dir) = crate::config::resolved_app_data_dir(app) else {
        return;
    };
    let path = dir.join("known-sftp-servers.json");
    // A `.tmp` left by a crash mid-write.
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }

    let store = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    *known().lock_ignore_poison() = store;
    let _ = STORE_PATH.set(path);
}

/// Writes the in-memory store back, durably (`config::durable_write_json`).
fn save() {
    let Some(path) = STORE_PATH.get() else {
        // Before `load_known_sftp_servers` there's nowhere to write. A test binary
        // lands here, and it still wants the in-memory half to work.
        return;
    };
    let store = known().lock_ignore_poison().clone();
    let Ok(json) = serde_json::to_string_pretty(&store) else {
        return;
    };
    if let Err(e) = crate::config::durable_write_json(path, &path.with_extension("json.tmp"), &json) {
        log::warn!(target: "volume", "couldn't write the known SFTP servers: {e}");
    }
}

/// Every server the user has connected to.
pub fn all() -> Vec<KnownSftpServer> {
    known().lock_ignore_poison().known_sftp_servers.clone()
}

/// Adds `server`, or replaces the entry for the same `(host, port, username)`.
pub fn remember(server: KnownSftpServer) {
    {
        let mut store = known().lock_ignore_poison();
        match store
            .known_sftp_servers
            .iter_mut()
            .find(|entry| same_server(entry, &server.host, server.port, &server.username))
        {
            // Replacing rather than appending: a second entry for one triple
            // would show the same server twice in a picker, and the two would
            // drift as only one of them got updated.
            Some(existing) => *existing = server,
            None => store.known_sftp_servers.push(server),
        }
    }
    save();
}

/// Drops the entry for `(host, port, username)`, answering whether one was there.
///
/// ❌ Leaves the secret store and the trusted host key alone: forgetting a server
/// from a list is not the same request as revoking its credential or its
/// identity, and the commands for those two are separate on purpose.
pub fn forget(host: &str, port: u16, username: &str) -> bool {
    let removed = {
        let mut store = known().lock_ignore_poison();
        let before = store.known_sftp_servers.len();
        store
            .known_sftp_servers
            .retain(|entry| !same_server(entry, host, port, username));
        store.known_sftp_servers.len() != before
    };
    if removed {
        save();
    }
    removed
}

#[cfg(test)]
#[path = "sftp_known_servers_test.rs"]
mod sftp_known_servers_test;
