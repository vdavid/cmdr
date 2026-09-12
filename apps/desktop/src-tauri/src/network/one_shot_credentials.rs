//! Using a secret for one dial without ever writing it down.
//!
//! Every SFTP and WebDAV dial reads the secret store, deliberately: the connect
//! params carry ❌ no password field, so a secret can't leak through an IPC
//! argument, a log line, or a crash report. That leaves "connect once without
//! remembering" with nowhere to put the secret, and save → dial → delete is not
//! an answer: a Keychain entry that exists for a second is not what the user
//! asked for, and a crash between the two steps leaves it there forever.
//!
//! So a sign-in offers its secret through [`SecretOffer`], and a
//! `remember: false` offer dials against [`OneShotCredentials`]: the real store
//! with one `(service, scope)` answered from memory instead.
//!
//! ❗ **The offer ends with the attempt.** The volume a dial builds keeps the
//! host it was dialed with, so the wrapper outlives the attempt whatever we do;
//! what [`OneShotGuard`] does is empty it, so the reconnect that comes later
//! reads the real (empty) store and asks a person. That IS what "don't remember
//! it" means: `crates/cmdr-sftp/DETAILS.md` § "The two switches".

use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::{CredentialStore, CredentialsNotStored, StoredCredentials};

/// A secret a person just typed, and what they asked us to do with it.
///
/// ❗ The one carrier a secret may travel in. ❌ No `password` argument on any
/// connect params: the crates read the store, and this is how a secret that
/// isn't in the store reaches a dial.
///
/// Crosses IPC: the sign-in sheet is where a person types one, and the backend
/// reads the store for every dial that isn't answering a sign-in.
///
/// ❗ **Inbound only, and it does not derive `Debug` or `Serialize`.** It holds a
/// plaintext secret, so a derived `Debug` would put one a `{:?}` away from a log
/// line or a crash report, and a derived `Serialize` would let it ride an event
/// or an analytics property. `StoredCredentials` next door derives only `Clone`
/// for the same reason (`cmdr_fs::volume::host::credentials`, whose module header
/// carries the rule). `Deserialize` is what the IPC boundary actually needs.
#[derive(Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SecretOffer {
    /// The secret itself: a password, a key file's passphrase, whatever the
    /// account's rung wants. ❌ Never logged, never in an event, never a property.
    pub secret: String,
    /// The "Remember in Keychain" switch as the sheet showed it. `true` writes
    /// the secret before dialing; `false` keeps it in memory for this attempt
    /// only.
    pub remember: bool,
}

/// Prints the switch and ❌ never the secret, so `{:?}` on a struct holding one
/// of these stays safe to write.
impl std::fmt::Debug for SecretOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretOffer")
            .field("secret", &"<redacted>")
            .field("remember", &self.remember)
            .finish()
    }
}

// ============================================================================
// The wrapper
// ============================================================================

/// The secret store, with one entry answered from memory.
///
/// Forwards everything else, so a dial that reads a second entry (another
/// account, a wide server-level one) still sees what the user has actually
/// saved. ❗ It never writes: a `remember: false` dial exists so nothing is
/// persisted, and a backend that saved what it authenticated with would defeat
/// exactly that.
pub struct OneShotCredentials {
    inner: Arc<dyn CredentialStore>,
    /// `None` once the attempt is over. Read on every lookup, so disarming takes
    /// effect for whoever still holds the wrapper.
    offer: Mutex<Option<Offer>>,
}

/// The one key this wrapper answers, and what it answers with.
struct Offer {
    service: String,
    scope: Option<String>,
    credentials: StoredCredentials,
}

impl OneShotCredentials {
    /// Wraps `inner`, answering `(service, scope)` with `credentials` until
    /// [`forget`](Self::forget).
    pub fn new(
        inner: Arc<dyn CredentialStore>,
        service: &str,
        scope: Option<&str>,
        credentials: StoredCredentials,
    ) -> Self {
        Self {
            inner,
            offer: Mutex::new(Some(Offer {
                service: service.to_string(),
                scope: scope.map(str::to_string),
                credentials,
            })),
        }
    }

    /// Ends the offer. Every lookup from here on is the real store's answer.
    pub fn forget(&self) {
        *self.offer.lock_ignore_poison() = None;
    }
}

impl CredentialStore for OneShotCredentials {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        let offered = {
            let offer = self.offer.lock_ignore_poison();
            offer.as_ref().and_then(|offer| {
                (offer.service == service && offer.scope.as_deref() == scope).then(|| offer.credentials.clone())
            })
        };
        // ❗ The offer wins over a stored entry for the same key: a person typing
        // a password is correcting the one that's saved, and letting the stale
        // one answer first is how a dial gets refused with the password the user
        // just replaced.
        offered.or_else(|| self.inner.credentials(service, scope))
    }

    fn save_credentials(
        &self,
        _service: &str,
        _scope: Option<&str>,
        _credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        // The documented "the store said no" answer, which every backend already
        // logs and carries on from. ❌ Not a forward: this dial's whole point is
        // that nothing is written.
        Err(CredentialsNotStored)
    }
}

/// Holds an offer open. Dropping it forgets the secret.
///
/// ❗ Keep it alive for exactly the dial: `let (host, _offer) = …;` in the
/// wiring, so it goes when the connect function returns, however it returns.
pub struct OneShotGuard(Arc<OneShotCredentials>);

impl Drop for OneShotGuard {
    fn drop(&mut self) {
        self.0.forget();
    }
}

/// `host` with its secret store wrapped, plus the guard that ends the offer.
#[must_use]
pub fn offer_for_one_dial(
    host: VolumeHost,
    service: &str,
    scope: Option<&str>,
    credentials: StoredCredentials,
) -> (VolumeHost, OneShotGuard) {
    // The host's own store stays behind the wrapper, so every OTHER key a dial
    // reads is the real answer.
    let inner = Arc::new(HostCredentials(host.clone()));
    let wrapper = Arc::new(OneShotCredentials::new(inner, service, scope, credentials));
    (host.with_credentials(wrapper.clone()), OneShotGuard(wrapper))
}

/// The host's own secret store, as something the wrapper can hold.
///
/// A `VolumeHost` hands out its seams by reference, so forwarding needs a value
/// that owns a host and asks it per call.
struct HostCredentials(VolumeHost);

impl CredentialStore for HostCredentials {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        self.0.credentials().credentials(service, scope)
    }

    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        self.0.credentials().save_credentials(service, scope, credentials)
    }
}

// ============================================================================
// What the wiring calls
// ============================================================================

/// The host one dial should run against, given what the sign-in sheet offered.
///
/// - No offer: the app's ordinary host. The dial reads the store, which is every
///   connect that isn't answering a sign-in.
/// - `remember: true`: the secret is written first, then the same ordinary host.
///   Identical to what a save-then-connect round-trip did, one round-trip
///   shorter.
/// - `remember: false`: a host whose secret store answers this one
///   `(service, account)` from memory and ❗ forgets it the moment the returned
///   guard drops, which is the end of the attempt.
///
/// ❗ A store that declines the write falls back to the wrapper: the user typed a
/// secret and this dial has to use it. All that's lost is "silent next time",
/// which is what `CredentialsNotStored` means everywhere else.
pub async fn host_for_dial(
    service: &str,
    account: &str,
    offer: Option<SecretOffer>,
) -> (VolumeHost, Option<OneShotGuard>) {
    let host = crate::volume_host::host();
    let Some(offer) = offer else {
        return (host, None);
    };
    let credentials = StoredCredentials {
        username: account.to_string(),
        secret: offer.secret,
    };
    if offer.remember {
        let (service, account, secret) = (service.to_string(), account.to_string(), credentials.secret.clone());
        // ❗ On a blocking task with a deadline, like every other write: the store
        // can put a Keychain prompt in front of this, and a modal dialog on the
        // async runtime stalls every volume.
        let written = crate::deadline::blocking_with_timeout(
            std::time::Duration::from_secs(15),
            Err(crate::network::keychain::KeychainError::Other(
                "the secret store didn't answer".to_string(),
            )),
            move || crate::network::keychain::save_credentials(&service, Some(&account), &account, &secret),
        )
        .await;
        match written {
            Ok(()) => return (host, None),
            Err(e) => log::warn!(
                target: "volume",
                "the secret store wouldn't remember this server ({e}); connecting with the secret this once"
            ),
        }
    }
    let (host, guard) = offer_for_one_dial(host, service, Some(account), credentials);
    (host, Some(guard))
}

#[cfg(test)]
#[path = "one_shot_credentials_test.rs"]
mod one_shot_credentials_test;
