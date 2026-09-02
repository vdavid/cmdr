//! The WebDAV servers this user has connected to, so the next time is one click.
//!
//! ❗ **A store of its own rather than a widened `KnownSftpServer`.** An SFTP
//! entry is keyed by `(host, port, username)` and carries a key file and an
//! agent switch, none of which a WebDAV server has; a WebDAV entry is keyed by
//! its base URL, which carries the scheme and the path an SFTP server has no
//! equivalent of. Sharing the type would leave two backends with fields that
//! mean different things in each.
//!
//! ❌ **No secret lives here.** The password goes to the `CredentialStore` seam
//! (`credential_store.rs`), keyed `service = "{scheme}://{host}:{port}"` (what
//! `WebdavConnectionParams::credential_service` yields) and
//! `scope = Some(username)`.
//!
//! An entry is keyed by `(url, username)`, with the URL normalized to a trailing
//! slash so `https://dav.example.test/remote.php/dav` and the same with a `/`
//! are one server. The volume id is derived from the URL's host and port plus
//! the username (`cmdr_fs::volume::webdav_volume_id`), so two accounts on one
//! server are two entries and two volumes rather than one of each.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use super::server_list_file;

use crate::ignore_poison::IgnorePoison;

/// One WebDAV server the user has connected to, and how to reach it again.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownWebdavServer {
    /// The base URL, as the user typed it, with a trailing slash. Scheme, host,
    /// port, and the path the server hangs its collections under.
    pub url: String,
    /// The account to sign in as. ❗ Part of the identity: two accounts on one
    /// server see different files under the same paths.
    pub username: String,
    /// What to call it in the UI. The user's own label, falling back to the host
    /// when they never gave one.
    pub display_name: String,
    /// The remote directory to open at, relative to the base URL's path.
    pub remote_root: String,
    /// Whether Cmdr may redial this server unattended when its session drops.
    ///
    /// ❗ **Independent of whether a secret is remembered**, which is the OTHER
    /// switch and lives in the Keychain rather than here
    /// (`has_webdav_credentials` is how to read it). Their combination has a real
    /// precondition: `get_webdav_unattended_reconnect` is what says so, per
    /// volume.
    ///
    /// ❗ Defaults to on, ❌ never to off, the same as SFTP's: reading a missing
    /// field as `false` would switch reconnects off under every server saved
    /// before the setting existed.
    /// ⚠️ `serde(default)` makes specta type this `autoReconnect?: boolean`. ❌
    /// Don't let a call site infer the default from that `undefined`:
    /// `getKnownWebdavServers` in `tauri-commands/webdav.ts` fills it in one
    /// place, and that is the only place the default is spelled on the frontend.
    #[serde(default = "reconnects_automatically")]
    pub auto_reconnect: bool,
    /// When this server was last connected to, ISO 8601, so a picker can sort by
    /// recency.
    pub last_connected_at: String,
}

/// What `auto_reconnect` is when a stored entry doesn't name it.
fn reconnects_automatically() -> bool {
    true
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownWebdavServersStore {
    /// Every server, in the order they were first added.
    #[serde(default)]
    pub known_webdav_servers: Vec<KnownWebdavServer>,
}

/// In-memory mirror of the file, so a picker render is not a disk read.
static KNOWN: OnceLock<Mutex<KnownWebdavServersStore>> = OnceLock::new();

/// Where the file lives once the app has told us its data dir.
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn known() -> &'static Mutex<KnownWebdavServersStore> {
    KNOWN.get_or_init(|| Mutex::new(KnownWebdavServersStore::default()))
}

/// The URL as the store keys it: a trailing slash, always, and the scheme and
/// host folded to lowercase.
///
/// Only the scheme and host fold: the path is the server's to interpret, and a
/// server may well tell `/Dav/` and `/dav/` apart. A user typing the same server
/// with and without its trailing slash gets one entry rather than two.
pub fn normalize_url(url: &str) -> String {
    let trimmed = url.trim();
    let (head, path) = match trimmed.find("://") {
        Some(at) => {
            let after_scheme = &trimmed[at + 3..];
            let path_start = after_scheme.find('/').map_or(trimmed.len(), |p| at + 3 + p);
            (&trimmed[..path_start], &trimmed[path_start..])
        }
        None => (trimmed, ""),
    };
    let mut normalized = head.to_ascii_lowercase();
    normalized.push_str(path);
    if !normalized.ends_with('/') {
        normalized.push('/');
    }
    normalized
}

/// Whether two entries describe the same server and account.
///
/// The URL is compared normalized ([`normalize_url`]) and the account is not
/// folded (a server may treat `Ada` and `ada` as two people). Same rule as
/// `cmdr_fs::volume::webdav_volume_id` folds the host, and it has to stay that
/// way or a server would get two entries and one volume.
fn same_server(entry: &KnownWebdavServer, url: &str, username: &str) -> bool {
    normalize_url(&entry.url) == normalize_url(url) && entry.username == username
}

/// Loads the store from the app's data dir into memory.
///
/// Call once at startup. A missing or unreadable file is an empty store: the
/// user's server list is a convenience, and losing it costs one re-entry rather
/// than access to anything.
pub fn load_known_webdav_servers<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some((store, path)) = server_list_file::load(app, "webdav_known_servers.json") else {
        return;
    };
    *known().lock_ignore_poison() = store;
    let _ = STORE_PATH.set(path);
}

/// Writes the in-memory store back, durably.
fn save() {
    let Some(path) = STORE_PATH.get() else {
        // Before `load_known_webdav_servers` there's nowhere to write. A test
        // binary lands here, and it still wants the in-memory half to work.
        return;
    };
    let store = known().lock_ignore_poison().clone();
    server_list_file::save(path, "the known WebDAV servers", &store);
}

/// Every server the user has connected to.
pub fn all() -> Vec<KnownWebdavServer> {
    known().lock_ignore_poison().known_webdav_servers.clone()
}

/// Adds `server`, or replaces the entry for the same `(url, username)`.
///
/// The URL is stored normalized, so the picker shows one spelling.
pub fn remember(mut server: KnownWebdavServer) {
    server.url = normalize_url(&server.url);
    {
        let mut store = known().lock_ignore_poison();
        match store
            .known_webdav_servers
            .iter_mut()
            .find(|entry| same_server(entry, &server.url, &server.username))
        {
            // Replacing rather than appending: a second entry for one pair would
            // show the same server twice in a picker, and the two would drift as
            // only one of them got updated.
            Some(existing) => *existing = server,
            None => store.known_webdav_servers.push(server),
        }
    }
    save();
}

/// Drops the entry for `(url, username)`, answering whether one was there.
///
/// ❌ Leaves the secret store alone: forgetting a server from a list is not the
/// same request as revoking its credential, and the command for that is separate
/// on purpose.
pub fn forget(url: &str, username: &str) -> bool {
    let removed = {
        let mut store = known().lock_ignore_poison();
        let before = store.known_webdav_servers.len();
        store
            .known_webdav_servers
            .retain(|entry| !same_server(entry, url, username));
        store.known_webdav_servers.len() != before
    };
    if removed {
        save();
    }
    removed
}

#[cfg(test)]
#[path = "webdav_known_servers_test.rs"]
mod webdav_known_servers_test;
