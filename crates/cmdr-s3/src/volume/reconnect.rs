//! Coming back after a request finds the server gone.
//!
//! The same model as `crates/cmdr-webdav/src/volume/reconnect.rs`, whose
//! header has the reasoning: HTTP holds no session, so "disconnected" means the
//! last wire-touching request failed with a transport error, and the first
//! operation that sees one calls [`S3Volume::note_lost_session`].
//!
//! **Two independent switches decide what may happen, in this order.**
//!
//! 1. **"Reconnect automatically"**, the user's per-server switch. Off means
//!    ❌ no unattended probe, ever.
//! 2. **The store.** A probe needs the secret; nothing stored means nothing to
//!    try.
//!
//! ❌ **Never loop on an authentication attempt.** One unattended try, latched
//! by `auth_attempt_spent`; only a human clears it. ❗ **A typed secret dies with
//! the client it built.** The store is only ever REFRESHED, never seeded.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::host::credentials::StoredCredentials;
use cmdr_fs::volume::liveness;
use cmdr_fs::volume::secret_store;
use cmdr_fs::volume::{SelfHandle, VolumeError};
use log::{debug, info, warn};
use tokio_util::sync::CancellationToken;

use super::state::ConnectionState;
use super::{S3Volume, S3VolumeInner, UnattendedReconnect, build_and_probe};
use crate::refusal::S3ConnectError;
use crate::transport::S3Client;

/// Bounded and growing: a handful of tries over a few minutes, then it stops.
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
    /// The user's switch is off. ❗ Its own arm: the keys may be fine.
    AutoReconnectOff,
    /// Only a human moves this forward.
    NeedsUser,
    /// The network or the server. Worth trying again.
    Transient(VolumeError),
}

impl S3Volume {
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
        // the frontend fires on the event doesn't find the dead client still
        // installed. (`rebuild` treats one found under a non-`Connected` state
        // as dead anyway.)
        let dropped = match inner.client.try_write() {
            Ok(mut client) => {
                inner.drop_dead_client(&mut client);
                true
            }
            Err(_) => false,
        };
        inner.host.runtime().spawn(async move {
            if !dropped {
                inner.drop_dead_client(&mut *inner.client.write().await);
            }
            drop(inner);
            if auto_reconnect {
                run_reconnect_loop(handle).await;
            }
        });
    }
}

impl S3VolumeInner {
    /// Takes the installed client out as dead, and cuts every operation still
    /// waiting on it. ❗ Only under a non-`Connected` state: a late task must
    /// never take the fresh client a reconnect just installed.
    fn drop_dead_client(&self, client: &mut Option<Arc<S3Client>>) {
        if self.connection_state() == ConnectionState::Connected {
            return;
        }
        if let Some(dead) = client.take() {
            dead.liveness().declare_lost();
        }
    }

    /// Starts the silence watch over `client`'s waiting operations. ❗ Holds
    /// the client weakly: the watch must not keep a dropped client's pool alive.
    pub(super) fn watch_over(&self, client: &Arc<S3Client>) {
        let timings = *self.silence.read_ignore_poison();
        let liveness = Arc::clone(client.liveness());
        let client = Arc::downgrade(client);
        self.host
            .runtime()
            .spawn(liveness::watch(liveness, timings, "s3", move || {
                let client = client.upgrade();
                async move {
                    match client {
                        Some(client) => client.ping().await,
                        None => false,
                    }
                }
            }));
    }

    /// Probes now, on the unattended terms. Single-flight.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        self.rebuild(None).await.map_err(|stalled| self.report(stalled))
    }

    /// The attended variant. `access_key_id` must be the key this volume IS:
    /// another key is another account, so it answers `NotSupported`.
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        access_key_id: String,
        secret: String,
    ) -> Result<(), VolumeError> {
        if access_key_id != self.params().access_key_id() {
            return Err(VolumeError::NotSupported);
        }
        self.refresh_remembered_secret(&access_key_id, &secret).await;
        self.auth_attempt_spent.store(false, Ordering::Relaxed);
        self.rebuild(Some(secret)).await.map_err(|stalled| self.report(stalled))
    }

    /// One rebuild attempt, under the single-flight lock. `offered` is a secret
    /// a human just typed, which marks the attempt ATTENDED.
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
        if !attended {
            let mut installed = self.client.write().await;
            if installed.is_some() {
                // ❗ Live only while the state says so: under any other state
                // it's the dead client whose drop is still on its way.
                if self.connection_state() == ConnectionState::Connected {
                    return Ok(());
                }
                self.drop_dead_client(&mut installed);
            }
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
        match build_and_probe(&self.params(), &secret, &CancellationToken::new()).await {
            Ok(client) => {
                if self.unmounted.load(Ordering::Relaxed) {
                    return Err(gone());
                }
                // ❗ Installed and marked `Connected` under ONE guard, so a late
                // `drop_dead_client` can never take it for the dead one.
                let mut installed = self.client.write().await;
                *installed = Some(Arc::new(client));
                self.auth_attempt_spent.store(false, Ordering::Relaxed);
                self.emit_if_changed(ConnectionState::Connected);
                drop(installed);
                info!(target: "volume", "s3 volume '{}' is back", self.volume_id);
                // An upload the outage cut off couldn't be aborted then.
                self.spawn_upload_sweep();
                Ok(())
            }
            Err(
                S3ConnectError::KeysRejected
                | S3ConnectError::AccessDenied
                | S3ConnectError::BucketListRefused
                | S3ConnectError::NeedsCredentials,
            ) => {
                self.auth_attempt_spent.store(true, Ordering::Relaxed);
                Err(Stalled::NeedsUser)
            }
            Err(S3ConnectError::Cancelled) => Err(Stalled::Transient(VolumeError::Cancelled(self.volume_id.clone()))),
            Err(S3ConnectError::TimedOut) => Err(Stalled::Transient(VolumeError::ConnectionTimeout(
                self.volume_id.clone(),
            ))),
            Err(
                S3ConnectError::Unreachable(_)
                | S3ConnectError::Transport(_)
                | S3ConnectError::InvalidProvider
                | S3ConnectError::NoSuchBucket
                | S3ConnectError::WrongRegion { .. }
                | S3ConnectError::ClockSkewed
                | S3ConnectError::CertificateUntrusted
                | S3ConnectError::NotAnS3Endpoint,
            ) => Err(gone()),
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

    /// The store's entry for this account. On a blocking task, inside
    /// `secret_store`.
    async fn stored_secret(&self) -> Option<StoredCredentials> {
        let params = self.params();
        secret_store::stored_secret(&self.host, &params.credential_service(), params.access_key_id()).await
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
                VolumeError::PermissionDenied {
                    path: self.volume_id.clone(),
                    raw_os_error: None,
                }
            }
            Stalled::Transient(error) => {
                self.emit_if_changed(ConnectionState::Disconnected);
                error
            }
        }
    }

    /// Brings a REMEMBERED secret up to date with the one the user just typed.
    /// ❗ Refreshes, ❌ never seeds.
    async fn refresh_remembered_secret(&self, access_key_id: &str, secret: &str) {
        let took = secret_store::refresh_remembered_secret(
            &self.host,
            &self.params().credential_service(),
            access_key_id,
            secret,
        )
        .await;
        if !took {
            warn!(
                target: "volume",
                "s3 volume '{}': the secret store didn't take the new secret; this connection still came up",
                self.volume_id
            );
        }
    }
}

/// The backend's own reconnect cadence. ❗ The handle is re-asked every
/// iteration: an ejected or superseded volume stops answering.
async fn run_reconnect_loop(handle: SelfHandle<S3VolumeInner>) {
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
                    info!(target: "volume", "s3 volume '{volume_id}' needs a person or is switched off; stopping the backoff");
                    return;
                }
                debug!(
                    target: "volume",
                    "s3 volume '{volume_id}': reconnect {}/{} didn't take",
                    attempt + 1,
                    RECONNECT_BACKOFF.len()
                );
            }
        }
    }
    info!(target: "volume", "s3 reconnect gave up after {} attempts", RECONNECT_BACKOFF.len());
}
