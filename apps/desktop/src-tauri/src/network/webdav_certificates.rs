//! The certificates a person told Cmdr to trust although macOS doesn't: a NAS's
//! self-signed one, pinned per server, trust-on-first-use.
//!
//! The TLS twin of `sftp_host_keys.rs`, and the same shape for the same reasons:
//! the store outlives any one volume, it's the user's to inspect and clear, and
//! writing it durably is the app's business, so it reaches the backend through
//! `cmdr_fs::volume::host::certificates::TrustedCertificates`.
//!
//! ❗ **One pin per `(host, port)`**, the leaf's SHA-256. The backend trusts a
//! certificate macOS refused only when its fingerprint IS the pin
//! (`crates/cmdr-webdav/src/tls_trust.rs`), so a pin never widens trust beyond
//! the one certificate a person read.
//!
//! ❗ **Pinning records what the prompt showed, with no second handshake.**
//! Unlike an SSH host key, there's nothing to re-check: only that exact
//! fingerprint is ever trusted, so a pin of a certificate the server stopped
//! presenting trusts nothing, and the dial after it shows the new one as a
//! CHANGED certificate.
//!
//! ❌ **Nothing here touches the macOS Keychain's trust settings.** A pin is
//! Cmdr's, and trusting a certificate system-wide is a bigger promise than
//! "this file manager may talk to my NAS".

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use cmdr_fs::volume::host::certificates::TrustedCertificates;
use serde::{Deserialize, Serialize};

use super::server_list_file;
use crate::ignore_poison::IgnorePoison;
use crate::state_recovery::StateFile;

/// One pinned certificate.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrustedCertificate {
    /// The server, as the user addressed it.
    pub host: String,
    /// Its port. Part of the identity: two services on one NAS can serve two
    /// certificates.
    pub port: u16,
    /// The leaf certificate's SHA-256, uppercase hex pairs joined by colons.
    pub fingerprint: String,
    /// When it was pinned, ISO 8601, so a settings screen can show it.
    pub approved_at: String,
}

/// The whole store, as it sits on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedCertificatesStore {
    /// Every certificate the user has pinned.
    #[serde(default, deserialize_with = "crate::state_recovery::lenient_entries")]
    pub trusted_certificates: Vec<TrustedCertificate>,
}

/// In-memory mirror of the file, so a lookup during a handshake is not a disk
/// read.
static TRUSTED: OnceLock<Mutex<TrustedCertificatesStore>> = OnceLock::new();

/// Where the file lives once the app has told us its data dir.
static STORE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn trusted() -> &'static Mutex<TrustedCertificatesStore> {
    TRUSTED.get_or_init(|| Mutex::new(TrustedCertificatesStore::default()))
}

/// Loads the store from the app's data dir into memory.
///
/// Call once at startup, ❗ before any WebDAV volume is built: a dial that runs
/// first reads every pinned server as first contact. A missing file is an empty
/// store; one that doesn't read is set aside first (`server_list_file::read`).
pub fn load_trusted_certificates<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some((store, path)) =
        server_list_file::load(app, "known-webdav-certificates.json", StateFile::WebdavCertificates)
    else {
        return;
    };
    *trusted().lock_ignore_poison() = store;
    let _ = STORE_PATH.set(path);
}

/// Writes the in-memory store back, durably (`server_list_file::save`): a
/// half-written trust store would lose pins and turn every pinned NAS into a
/// fresh prompt.
fn save() {
    let Some(path) = STORE_PATH.get() else {
        // Before `load_trusted_certificates` there's nowhere to write. A test
        // binary lands here and wants the in-memory half to keep working.
        return;
    };
    let store = trusted().lock_ignore_poison().clone();
    server_list_file::save(path, "the trusted WebDAV certificates", &store);
}

/// Pins `fingerprint` as the certificate for `(host, port)`, replacing any pin
/// there. Called only once a person approved the certificate the prompt showed.
pub fn pin(host: &str, port: u16, fingerprint: &str) {
    {
        let mut store = trusted().lock_ignore_poison();
        let entry = TrustedCertificate {
            host: host.to_string(),
            port,
            fingerprint: fingerprint.to_string(),
            approved_at: chrono::Utc::now().to_rfc3339(),
        };
        match store
            .trusted_certificates
            .iter_mut()
            .find(|e| e.host == host && e.port == port)
        {
            // Replacing rather than appending: two pins for one server would make
            // the answer depend on iteration order, and a stale one could let an
            // old certificate back in after the user trusted a new one.
            Some(existing) => *existing = entry,
            None => store.trusted_certificates.push(entry),
        }
    }
    save();
}

/// Every certificate the user has pinned, newest write last, for a settings
/// screen listing what this machine trusts.
pub fn list() -> Vec<TrustedCertificate> {
    trusted().lock_ignore_poison().trusted_certificates.clone()
}

/// Drops the pin for `(host, port)`, so the next connection there is first
/// contact again. Answers whether anything was there.
pub fn forget(host: &str, port: u16) -> bool {
    let removed = {
        let mut store = trusted().lock_ignore_poison();
        let before = store.trusted_certificates.len();
        store
            .trusted_certificates
            .retain(|entry| !(entry.host == host && entry.port == port));
        store.trusted_certificates.len() != before
    };
    if removed {
        save();
    }
    removed
}

/// Answers a backend's "is a certificate pinned here?" from the durable store.
pub struct AppTrustedCertificates;

impl TrustedCertificates for AppTrustedCertificates {
    fn pinned(&self, host: &str, port: u16) -> Option<String> {
        trusted()
            .lock_ignore_poison()
            .trusted_certificates
            .iter()
            .find(|entry| entry.host == host && entry.port == port)
            .map(|entry| entry.fingerprint.clone())
    }
}

#[cfg(test)]
#[path = "webdav_certificates_test.rs"]
mod webdav_certificates_test;
