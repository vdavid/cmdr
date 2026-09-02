//! Reaching a backend's secret store without stalling every other volume.
//!
//! ❗ **Every call here runs on a blocking task.** The store can put a Keychain
//! prompt in front of a read or a write, and a modal dialog on the async runtime
//! holds the whole runtime — every other volume's listings and transfers with
//! it. A backend that reaches `host.credentials()` directly from an async fn is
//! one user click away from freezing the app.

use crate::volume::host::VolumeHost;
use crate::volume::host::credentials::StoredCredentials;

/// The account's stored secret, or `None` when nothing is stored (and when the
/// store didn't answer at all, which sends the caller to the same place: ask).
pub async fn stored_secret(host: &VolumeHost, service: &str, username: &str) -> Option<StoredCredentials> {
    let host = host.clone();
    let service = service.to_string();
    let scope = username.to_string();
    tokio::task::spawn_blocking(move || host.credentials().credentials(&service, Some(&scope)))
        .await
        .ok()
        .flatten()
}

/// Whether a secret is stored, without reading it out.
pub async fn has_stored_secret(host: &VolumeHost, service: &str, username: &str) -> bool {
    stored_secret(host, service, username).await.is_some()
}

/// Brings a REMEMBERED secret up to date with the one the user just typed.
///
/// ❗ Refreshes, ❌ never seeds: a store with nothing in it is the user having
/// said no to remembering, and writing one anyway would put a password in the
/// Keychain they declined to leave there. The read and the write ride ONE
/// blocking task, so a store that prompts asks once.
///
/// Answers whether the new secret landed. A `false` costs only the "silent next
/// time" guarantee, ❌ never the connection that earned it, so callers log it
/// rather than failing.
pub async fn refresh_remembered_secret(host: &VolumeHost, service: &str, username: &str, secret: &str) -> bool {
    let host = host.clone();
    let service = service.to_string();
    let scope = username.to_string();
    let stored = StoredCredentials {
        username: username.to_string(),
        secret: secret.to_string(),
    };
    let written = tokio::task::spawn_blocking(move || {
        let store = host.credentials();
        if store.credentials(&service, Some(&scope)).is_none() {
            return Ok(());
        }
        store.save_credentials(&service, Some(&scope), &stored)
    })
    .await;
    matches!(written, Ok(Ok(())))
}
