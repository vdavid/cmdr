//! The app's answer to a backend's "have we met this server before?" question.
//!
//! SSH trust is trust-on-first-use: the security of every later connection rests
//! on recognizing the same host key next time. A backend can't own that store —
//! it outlives any one volume, it's the user's to inspect and clear, and writing
//! it durably is the app's business — so it arrives through
//! `cmdr_fs::volume::host::host_keys::HostKeys` and this is what the app
//! installs.
//!
//! ❗ **A new store rather than a widened `KnownNetworkShare`.** That type's
//! `share_name` and its guest-versus-credentials `AuthOptions` can't express key
//! or agent auth, and a host key isn't a share at all.
//!
//! ❌ **Nothing here writes `~/.ssh/known_hosts`.** The backend reads that file
//! as a fallback so a server the user's terminal already reaches doesn't ask
//! again, but it belongs to `ssh`, and a file manager appending to it is a
//! surprise nobody asked for.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use cmdr_fs::volume::host::host_keys::{HostKeyVerdict, HostKeys};
use serde::{Deserialize, Serialize};

use crate::ignore_poison::IgnorePoison;

/// One approved host key.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrustedHostKey {
    /// The server, as the user addressed it.
    pub host: String,
    /// Its port. Part of the identity: a jump box and a container on one machine
    /// are different servers.
    pub port: u16,
    /// The SSH key-type name (`ssh-ed25519`, `rsa-sha2-512`). ❗ Part of the key
    /// too: a server may hold several types and present any of them, so a store
    /// keyed by host alone reports a changed key on a healthy server.
    pub algorithm: String,
    /// The OpenSSH `SHA256:…` fingerprint, which is what a human compares
    /// against `ssh-keygen -lf`.
    pub fingerprint: String,
    /// When it was approved, ISO 8601, so the settings screen can show it.
    pub approved_at: String,
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedHostKeysStore {
    /// Every key the user has approved.
    #[serde(default)]
    pub trusted_host_keys: Vec<TrustedHostKey>,
}

/// In-memory mirror of the file, so a lookup during key exchange is not a disk
/// read.
static TRUSTED: OnceLock<Mutex<TrustedHostKeysStore>> = OnceLock::new();

/// Where the file lives once the app has told us its data dir.
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn trusted() -> &'static Mutex<TrustedHostKeysStore> {
    TRUSTED.get_or_init(|| Mutex::new(TrustedHostKeysStore::default()))
}

/// Loads the store from the app's data dir into memory.
///
/// Call once at startup. A missing or unreadable file is an empty store: every
/// server then reads as first contact, which costs one approval and never
/// trusts anything it shouldn't.
pub fn load_trusted_host_keys<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(dir) = crate::config::resolved_app_data_dir(app) else {
        return;
    };
    let path = dir.join("known-sftp-hosts.json");
    // A `.tmp` left by a crash mid-write.
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }

    let store = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    *trusted().lock_ignore_poison() = store;
    let _ = STORE_PATH.set(path);
}

/// Writes the in-memory store back, durably.
///
/// Temp file, fsync, rename, parent fsync (`config::durable_write_json`): a
/// half-written trust store would either lose approvals or, worse, be
/// unparseable and read as "nothing is trusted", turning every mounted server
/// into a fresh security prompt.
fn save() {
    let Some(path) = STORE_PATH.get() else {
        // Before `load_trusted_host_keys` there's nowhere to write. A test binary
        // and a bench both land here, and both want the in-memory half to keep
        // working.
        return;
    };
    let store = trusted().lock_ignore_poison().clone();
    let Ok(json) = serde_json::to_string_pretty(&store) else {
        return;
    };
    if let Err(e) = crate::config::durable_write_json(path, &path.with_extension("json.tmp"), &json) {
        log::warn!(target: "volume", "couldn't write the trusted SSH host keys: {e}");
    }
}

/// Answers a backend's host-key questions from the durable store.
pub struct AppHostKeys;

impl HostKeys for AppHostKeys {
    fn verdict(&self, host: &str, port: u16, algorithm: &str, fingerprint: &str) -> HostKeyVerdict {
        let store = trusted().lock_ignore_poison();
        match store
            .trusted_host_keys
            .iter()
            .find(|entry| entry.host == host && entry.port == port && entry.algorithm == algorithm)
        {
            Some(entry) if entry.fingerprint == fingerprint => HostKeyVerdict::Matches,
            Some(_) => HostKeyVerdict::Changed,
            None => HostKeyVerdict::Unknown,
        }
    }

    fn trusted_algorithms(&self, host: &str, port: u16) -> Vec<String> {
        let store = trusted().lock_ignore_poison();
        let mut algorithms: Vec<String> = store
            .trusted_host_keys
            .iter()
            .filter(|entry| entry.host == host && entry.port == port)
            .map(|entry| entry.algorithm.clone())
            .collect();
        algorithms.sort();
        algorithms.dedup();
        algorithms
    }

    fn record(&self, host: &str, port: u16, algorithm: &str, fingerprint: &str) {
        {
            let mut store = trusted().lock_ignore_poison();
            let entry = TrustedHostKey {
                host: host.to_string(),
                port,
                algorithm: algorithm.to_string(),
                fingerprint: fingerprint.to_string(),
                approved_at: chrono::Utc::now().to_rfc3339(),
            };
            match store
                .trusted_host_keys
                .iter_mut()
                .find(|e| e.host == host && e.port == port && e.algorithm == algorithm)
            {
                // Replacing rather than appending: two entries for one triple
                // would make the verdict depend on iteration order, and a stale
                // one could answer `Matches` for a key the user replaced.
                Some(existing) => *existing = entry,
                None => store.trusted_host_keys.push(entry),
            }
        }
        save();
    }
}

#[cfg(test)]
#[path = "sftp_host_keys_test.rs"]
mod sftp_host_keys_test;
