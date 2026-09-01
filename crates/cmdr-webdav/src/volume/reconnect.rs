//! Coming back after a request finds the server gone.
//!
//! HTTP holds no session, so "disconnected" means the last wire-touching
//! request failed with a transport error. The first operation that sees one
//! calls [`WebdavVolume::note_lost_session`], which flips the state once,
//! drops the client, and starts the backoff loop below.
//!
//! **Two independent switches decide what may happen, in this order.**
//!
//! 1. **"Reconnect automatically"**, the user's per-server switch. Off means
//!    ❌ no unattended probe, ever, whatever is stored.
//! 2. **The store.** A probe needs the secret; nothing stored means nothing
//!    to try, and the frontend learns that through `UnattendedReconnect`.
//!
//! ❌ **Never loop on an authentication attempt.** Repeated refusals lock
//! accounts. One unattended try, latched by `auth_attempt_spent`; only a human
//! clears it. ❗ **A typed secret dies with the client it built.** The store is
//! only ever REFRESHED, never seeded.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::volume::host::credentials::StoredCredentials;
use cmdr_fs::volume::{SelfHandle, VolumeError};
use log::{debug, info, warn};
use tokio_util::sync::CancellationToken;

use super::state::ConnectionState;
use super::{UnattendedReconnect, WebdavVolume, WebdavVolumeInner, build_and_probe};
use crate::errors::WebdavConnectError;

/// Bounded and growing: a handful of tries over a few minutes, then it stops
/// rather than hammering a server that is genuinely down.
const RECONNECT_BACKOFF: [Duration; 6] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

/// Why an attempt didn't leave a live client, in the terms the loop acts on.
#[derive(Debug)]
enum Stalled {
    /// The user's switch is off. ❗ Its own arm: the credentials may be fine.
    AutoReconnectOff,
    /// Only a human moves this forward.
    NeedsUser,
    /// The network or the server. Worth trying again.
    Transient(VolumeError),
}

impl WebdavVolume {
    /// Notices, once, that the server behind this volume is gone. ❗ Acts only
    /// on the `Connected` → `Disconnected` edge.
    pub(super) fn note_lost_session(&self, error: &VolumeError) {
        if !matches!(error, VolumeError::DeviceDisconnected(_)) {
            return;
        }
        if self.inner.connection_state() != ConnectionState::Connected {
            return;
        }
        if !self.inner.emit_if_changed(ConnectionState::Disconnected) {
            return;
        }
        let inner = Arc::clone(&self.inner);
        let handle = inner.self_handle();
        let auto_reconnect = inner.auto_reconnect.load(Ordering::Relaxed);
        // ❗ Dropped HERE when nothing holds the lock, so an `attempt_reconnect`
        // the frontend fires on the event it just got doesn't find the dead
        // client still installed and answer "fine" without probing.
        let dropped = inner.client.try_write().map(|mut client| client.take()).is_ok();
        inner.host.runtime().spawn(async move {
            if !dropped {
                inner.client.write().await.take();
            }
            drop(inner);
            if auto_reconnect {
                run_reconnect_loop(handle).await;
            }
        });
    }
}

impl WebdavVolumeInner {
    /// Probes now, on the unattended terms. Single-flight.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        self.rebuild(None).await.map_err(|stalled| self.report(stalled))
    }

    /// The attended variant. `username` must be the account this volume IS:
    /// another account is another volume, so it answers `NotSupported`.
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        username: String,
        password: String,
    ) -> Result<(), VolumeError> {
        if username != self.params.username {
            return Err(VolumeError::NotSupported);
        }
        self.refresh_remembered_secret(&username, &password).await;
        self.auth_attempt_spent.store(false, Ordering::Relaxed);
        self.rebuild(Some(password))
            .await
            .map_err(|stalled| self.report(stalled))
    }

    /// One rebuild attempt, under the single-flight lock. `offered` is a secret
    /// a human just typed, which is what marks the attempt ATTENDED.
    async fn rebuild(&self, offered: Option<String>) -> Result<(), Stalled> {
        let attended = offered.is_some();
        let gone = || Stalled::Transient(VolumeError::DeviceDisconnected(self.volume_id.clone()));
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(gone());
        }
        let _guard = self.reconnect_lock.lock().await;
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(gone());
        }
        if !attended && self.client.read().await.is_some() {
            // Still installed, so nothing to rebuild; the state may lag it.
            self.emit_if_changed(ConnectionState::Connected);
            return Ok(());
        }
        if !attended {
            if !self.auto_reconnect.load(Ordering::Relaxed) {
                return Err(Stalled::AutoReconnectOff);
            }
            if self.auth_attempt_spent.load(Ordering::Relaxed) {
                return Err(Stalled::NeedsUser);
            }
        }
        let secret = match offered {
            Some(secret) => secret,
            None => match self.stored_secret().await {
                Some(stored) => stored.secret,
                None => return Err(Stalled::NeedsUser),
            },
        };
        match build_and_probe(&self.params, &secret, &CancellationToken::new()).await {
            Ok(client) => {
                if self.unmounted.load(Ordering::Relaxed) {
                    return Err(gone());
                }
                *self.client.write().await = Some(Arc::new(client));
                self.auth_attempt_spent.store(false, Ordering::Relaxed);
                self.emit_if_changed(ConnectionState::Connected);
                info!(target: "volume", "webdav volume '{}' is back", self.volume_id);
                Ok(())
            }
            Err(WebdavConnectError::AuthenticationRejected | WebdavConnectError::AuthMethodUnsupported) => {
                self.auth_attempt_spent.store(true, Ordering::Relaxed);
                Err(Stalled::NeedsUser)
            }
            Err(WebdavConnectError::NeedsCredentials) => Err(Stalled::NeedsUser),
            Err(WebdavConnectError::Cancelled) => {
                Err(Stalled::Transient(VolumeError::Cancelled(self.volume_id.clone())))
            }
            Err(WebdavConnectError::TimedOut) => Err(Stalled::Transient(VolumeError::ConnectionTimeout(
                self.volume_id.clone(),
            ))),
            Err(WebdavConnectError::Unreachable(what) | WebdavConnectError::Transport(what)) => {
                Err(Stalled::Transient(VolumeError::DeviceDisconnected(what)))
            }
            Err(WebdavConnectError::CertificateUntrusted | WebdavConnectError::NotAWebdavServer) => Err(
                Stalled::Transient(VolumeError::DeviceDisconnected(self.volume_id.clone())),
            ),
        }
    }

    /// Starts the backoff loop for a volume that is down. ❗ Only from
    /// `Disconnected`: `NeedsCredentials` is a state a person moves forward.
    pub(super) fn start_reconnect_loop_if_down(&self) {
        if self.connection_state() != ConnectionState::Disconnected || self.unmounted.load(Ordering::Relaxed) {
            return;
        }
        let handle = self.self_handle();
        self.host.runtime().spawn(run_reconnect_loop(handle));
    }

    /// Whether an unattended reconnect can happen as this volume stands.
    pub(super) async fn unattended_reconnect(&self) -> UnattendedReconnect {
        if !self.auto_reconnect.load(Ordering::Relaxed) {
            return UnattendedReconnect::SwitchOff;
        }
        if self.stored_secret().await.is_some() {
            UnattendedReconnect::Possible
        } else {
            UnattendedReconnect::NoStoredSecret
        }
    }

    /// The store's entry for this account. ❗ On a blocking task: the store may
    /// put a Keychain prompt in front of this.
    async fn stored_secret(&self) -> Option<StoredCredentials> {
        let host = self.host.clone();
        let service = self.params.credential_service();
        let scope = self.params.username.clone();
        tokio::task::spawn_blocking(move || host.credentials().credentials(&service, Some(&scope)))
            .await
            .ok()
            .flatten()
    }

    /// Reports the stall and turns it into the trait's vocabulary.
    fn report(&self, stalled: Stalled) -> VolumeError {
        match stalled {
            Stalled::AutoReconnectOff => {
                self.emit_if_changed(ConnectionState::Disconnected);
                VolumeError::NotSupported
            }
            Stalled::NeedsUser => {
                self.emit_if_changed(ConnectionState::NeedsCredentials);
                VolumeError::PermissionDenied(self.volume_id.clone())
            }
            Stalled::Transient(error) => {
                self.emit_if_changed(ConnectionState::Disconnected);
                error
            }
        }
    }

    /// Brings a REMEMBERED secret up to date with the one the user just typed.
    /// ❗ Refreshes, ❌ never seeds: a store with nothing in it is the user
    /// having said no.
    async fn refresh_remembered_secret(&self, username: &str, secret: &str) {
        let host = self.host.clone();
        let service = self.params.credential_service();
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
        if !matches!(written, Ok(Ok(()))) {
            warn!(
                target: "volume",
                "webdav volume '{}': the secret store didn't take the new secret; this connection still came up",
                self.volume_id
            );
        }
    }
}

/// The backend's own reconnect cadence. ❗ The handle is re-asked every
/// iteration: an ejected or superseded volume stops answering.
async fn run_reconnect_loop(handle: SelfHandle<WebdavVolumeInner>) {
    for (attempt, delay) in RECONNECT_BACKOFF.iter().enumerate() {
        tokio::time::sleep(*delay).await;
        let Some(inner) = handle.live().filter(|inner| !inner.unmounted.load(Ordering::Relaxed)) else {
            return;
        };
        if inner.connection_state() == ConnectionState::Connected {
            return;
        }
        let volume_id = inner.volume_id.clone();
        match inner.rebuild(None).await {
            Ok(()) => return,
            Err(stalled) => {
                let stop = !matches!(stalled, Stalled::Transient(_));
                inner.report(stalled);
                if stop {
                    info!(target: "volume", "webdav volume '{volume_id}' needs a person or is switched off; stopping the backoff");
                    return;
                }
                debug!(
                    target: "volume",
                    "webdav volume '{volume_id}': reconnect {}/{} didn't take",
                    attempt + 1,
                    RECONNECT_BACKOFF.len()
                );
            }
        }
    }
    info!(target: "volume", "webdav reconnect gave up after {} attempts", RECONNECT_BACKOFF.len());
}
