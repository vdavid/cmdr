//! The app's answer to a backend's "what's stored for this server?" question.
//!
//! A backend that authenticates can't reach `network::keychain` from its own
//! crate, so it asks through `cmdr_fs::volume::host::credentials::CredentialStore`
//! and this is what the app installs. A pure adapter: the account-name format,
//! the in-memory cache, and the secret store underneath are all still
//! `keychain`'s.
//!
//! The seam's `(service, scope)` pair is `(server, share)` here. `scope: None`
//! means the server-wide entry, which is what a sign-in saves so one password
//! covers every share.

use cmdr_fs::volume::host::credentials::{CredentialStore, CredentialsNotStored, StoredCredentials};

use super::keychain;

/// Answers a backend's credential questions from the OS secret store.
pub struct KeychainCredentials;

impl CredentialStore for KeychainCredentials {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        // A miss and an unreadable store are the same answer to a backend: it
        // falls back to what it was handed, or connects as a guest.
        match keychain::get_credentials(service, scope) {
            Ok(stored) => Some(StoredCredentials {
                username: stored.username,
                secret: stored.password,
            }),
            Err(e) => {
                // DEBUG, not WARN: "nothing stored for this server" is the
                // ordinary answer for a guest share, and every reconnect asks.
                log::debug!(target: "volume", "no stored credentials for {service}: {e}");
                None
            }
        }
    }

    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        // The typed reason stops here — the seam carries only "it didn't land" —
        // so this is the last place that can say WHY, and a sign-in that silently
        // stops being remembered is exactly what someone would file a bug about.
        keychain::save_credentials(service, scope, &credentials.username, &credentials.secret).map_err(|e| {
            log::warn!(target: "volume", "the secret store wouldn't keep the credentials for {service}: {e}");
            CredentialsNotStored
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(username: &str, secret: &str) -> StoredCredentials {
        StoredCredentials {
            username: username.to_string(),
            secret: secret.to_string(),
        }
    }

    /// Both halves have to survive the round trip through the real store, and the
    /// secret has to land in `secret` rather than in whatever the SMB-era type
    /// calls its second field.
    #[test]
    fn a_saved_secret_comes_back_intact() {
        let server = "credential-store-test.local";
        KeychainCredentials
            .save_credentials(server, None, &creds("ada", "pa55"))
            .expect("the test store always accepts");

        let read = KeychainCredentials
            .credentials(server, None)
            .expect("what we just saved");
        assert_eq!(read.username, "ada");
        assert_eq!(read.secret, "pa55");
    }

    /// Narrow-then-wide is the conventional lookup, so the two scopes must be
    /// separate entries: a share-level password can't be answered by the
    /// server-level one or a user would silently sign in to the wrong share.
    #[test]
    fn a_share_scoped_entry_is_separate_from_the_server_wide_one() {
        let server = "credential-store-scopes.local";
        KeychainCredentials
            .save_credentials(server, None, &creds("wide", "wide-secret"))
            .expect("the test store always accepts");
        KeychainCredentials
            .save_credentials(server, Some("photos"), &creds("narrow", "narrow-secret"))
            .expect("the test store always accepts");

        assert_eq!(
            KeychainCredentials
                .credentials(server, Some("photos"))
                .unwrap()
                .username,
            "narrow"
        );
        assert_eq!(KeychainCredentials.credentials(server, None).unwrap().username, "wide");
    }

    /// Nothing stored reads as `None`, not as an error a backend has to classify.
    #[test]
    fn nothing_stored_reads_as_a_miss() {
        assert!(
            KeychainCredentials
                .credentials("credential-store-never-saved.local", None)
                .is_none()
        );
    }
}
