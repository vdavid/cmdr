//! The OS secret store, for a backend that authenticates.
//!
//! A backend needs stored credentials at two moments: building its first session
//! (the user signed in earlier and expects it to be silent this time), and
//! rebuilding one after a drop (the password may have been changed in the
//! meantime, so a reconnect re-reads rather than trusting its cached copy).
//!
//! ❌ **Never hold credentials longer than the session you're building.** The
//! store is the durable copy; a backend keeps only what its live connection
//! needs, and never writes either field to a log, an event, or an analytics
//! property.

/// A username and its secret, as the store holds them.
///
/// `secret` rather than "password" because a backend's secret isn't always one:
/// it's an S3 secret access key, an FTP password, an SFTP passphrase. The store
/// treats it as opaque bytes-in-a-string either way.
#[derive(Clone)]
pub struct StoredCredentials {
    /// The account name. An access key id, for a backend that calls it that.
    pub username: String,
    /// The secret half. ❌ Never logged, never in an event, never a property.
    pub secret: String,
}

/// The store declined to keep the credentials.
///
/// A backend logs this and carries on: the session it's building already holds
/// what it needs, and the only thing lost is "silent next time". ❌ Don't fail a
/// connection over it and don't retry in a loop; on macOS a denial means the
/// user said no to a Keychain prompt.
#[derive(Debug)]
pub struct CredentialsNotStored;

/// Stored credentials, scoped by service and optionally narrower.
///
/// `service` is the thing being authenticated against, in whatever form the
/// backend already has (a hostname, an endpoint). `scope` narrows it to one
/// resource on that service — a share, a bucket, a path — and `None` means the
/// service-wide entry.
///
/// The conventional lookup is "narrow first, then wide": try
/// `credentials(server, Some(share))`, and fall back to
/// `credentials(server, None)`. A save from a sign-in form goes to the wide
/// entry, so one password covers every share on the server.
///
/// Cmdr answers this from the macOS Keychain; a test or a tool answers nothing
/// (`NoCredentials`).
pub trait CredentialStore: Send + Sync {
    /// What's stored for `service` (and `scope`, when given), if anything.
    ///
    /// `None` covers both "nothing stored" and "the store couldn't be read".
    /// Neither is actionable differently: a backend falls back to whatever
    /// credentials it was handed, or connects as a guest.
    ///
    /// **May block.** On macOS this is a Keychain call, which can put a prompt
    /// in front of the user. ❌ Don't call it on the async runtime directly;
    /// hand it to a blocking task.
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials>;

    /// Remembers `credentials` for `service` (and `scope`, when given),
    /// replacing whatever was there.
    ///
    /// **May block**, same as [`credentials`](Self::credentials).
    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored>;
}

/// There's no secret store: nothing is remembered and nothing can be recalled.
///
/// A backend under this host connects with whatever it was handed, which is
/// exactly what a test fixture with a literal password wants.
pub(super) struct NoCredentials;

impl CredentialStore for NoCredentials {
    fn credentials(&self, _service: &str, _scope: Option<&str>) -> Option<StoredCredentials> {
        None
    }

    fn save_credentials(
        &self,
        _service: &str,
        _scope: Option<&str>,
        _credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        Err(CredentialsNotStored)
    }
}

#[cfg(any(test, feature = "testing"))]
pub use in_memory::InMemoryCredentials;

#[cfg(any(test, feature = "testing"))]
mod in_memory {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::{CredentialStore, CredentialsNotStored, StoredCredentials};
    use crate::ignore_poison::IgnorePoison;

    /// A [`CredentialStore`] in a `HashMap`, so a
    /// reconnect test can prove that a fresh session re-read the store rather
    /// than reusing the password it started with.
    #[derive(Default)]
    pub struct InMemoryCredentials {
        entries: Mutex<HashMap<(String, Option<String>), StoredCredentials>>,
    }

    impl InMemoryCredentials {
        /// An empty store.
        pub fn new() -> Self {
            Self::default()
        }

        /// Pre-seeds one entry, as if the user had signed in earlier.
        pub fn with_entry(self, service: &str, scope: Option<&str>, username: &str, secret: &str) -> Self {
            self.entries.lock_ignore_poison().insert(
                (service.to_string(), scope.map(str::to_string)),
                StoredCredentials {
                    username: username.to_string(),
                    secret: secret.to_string(),
                },
            );
            self
        }
    }

    impl CredentialStore for InMemoryCredentials {
        fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
            self.entries
                .lock_ignore_poison()
                .get(&(service.to_string(), scope.map(str::to_string)))
                .cloned()
        }

        fn save_credentials(
            &self,
            service: &str,
            scope: Option<&str>,
            credentials: &StoredCredentials,
        ) -> Result<(), CredentialsNotStored> {
            self.entries
                .lock_ignore_poison()
                .insert((service.to_string(), scope.map(str::to_string)), credentials.clone());
            Ok(())
        }
    }
}
