//! Coming back: the single-flight rebuild, what a stalled one reports, and the
//! backend's own backoff cadence. The rules are the parent module's header.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use log::{debug, info, warn};

use super::{DialRefusal, HttpBackend, HttpConnectionState, SessionClient};
use crate::ignore_poison::RwLockIgnorePoison;
use crate::volume::host::credentials::StoredCredentials;
use crate::volume::secret_store::SignIn;
use crate::volume::{SelfHandle, VolumeError, liveness, secret_store};

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
pub(super) enum Stalled {
    /// The user's switch is off. ❗ Its own arm: the credentials may be fine.
    AutoReconnectOff,
    /// Only a human moves this forward.
    NeedsUser,
    /// The network or the server. Worth trying again.
    Transient(VolumeError),
}

/// One rebuild attempt, under the single-flight lock. `offered` is a secret a
/// human just typed, which is what marks the attempt ATTENDED.
pub(super) async fn rebuild<B: HttpBackend>(owner: &B, offered: Option<String>) -> Result<(), Stalled> {
    let session = owner.session();
    let volume_id = session.connection.volume_id();
    let attended = offered.is_some();
    let gone = || Stalled::Transient(VolumeError::DeviceDisconnected(volume_id.to_string().into()));
    if session.unmounted.load(Ordering::Relaxed) {
        return Err(gone());
    }
    let _guard = session.reconnect_lock.lock().await;
    if session.unmounted.load(Ordering::Relaxed) {
        return Err(gone());
    }
    if !attended {
        {
            let mut installed = session.client.write().await;
            if installed.is_some() {
                // ❗ Live only while the state says so. Under any other state
                // it's the dead client whose drop is still on its way
                // (`note_lost_session` spawns it), and answering "fine" for it
                // would report a server that's still gone as back.
                if session.connection.state() == HttpConnectionState::Connected {
                    return Ok(());
                }
                session.drop_dead_client(&mut installed);
            }
        }
        if !session.auto_reconnect.load(Ordering::Relaxed) {
            return Err(Stalled::AutoReconnectOff);
        }
        if session.auth_attempt_spent.load(Ordering::Relaxed) {
            return Err(Stalled::NeedsUser);
        }
    }
    let secret = match offered {
        Some(secret) => secret,
        None => match stored_secret(owner).await {
            Some(stored) => stored.secret,
            None => return Err(Stalled::NeedsUser),
        },
    };
    match owner.dial(&secret).await {
        Ok(client) => {
            if session.unmounted.load(Ordering::Relaxed) {
                return Err(gone());
            }
            // ❗ Installed, given the next generation, and marked `Connected`
            // under ONE guard, so a late `drop_dead_client` can never take it
            // for the dead one, and no call reads the new client under the old
            // generation.
            let mut installed = session.client.write().await;
            *installed = Some(Arc::new(client));
            session.auth_attempt_spent.store(false, Ordering::Relaxed);
            session.connection.install();
            drop(installed);
            info!(target: "volume", "{} volume '{}' is back", B::BACKEND, volume_id);
            owner.client_installed();
            Ok(())
        }
        Err(DialRefusal::Rejected) => {
            session.auth_attempt_spent.store(true, Ordering::Relaxed);
            Err(Stalled::NeedsUser)
        }
        Err(DialRefusal::NeedsUser) => Err(Stalled::NeedsUser),
        Err(DialRefusal::Transient(error)) => Err(Stalled::Transient(error)),
    }
}

/// Reports the stall and turns it into the trait's vocabulary.
pub(super) fn report<B: HttpBackend>(owner: &B, stalled: Stalled) -> VolumeError {
    let connection = &owner.session().connection;
    match stalled {
        Stalled::AutoReconnectOff => {
            connection.emit_if_changed(HttpConnectionState::Disconnected);
            VolumeError::NotSupported
        }
        Stalled::NeedsUser => {
            connection.emit_if_changed(HttpConnectionState::NeedsCredentials);
            VolumeError::PermissionDenied {
                path: connection.volume_id().to_string(),
                raw_os_error: None,
            }
        }
        Stalled::Transient(error) => {
            connection.emit_if_changed(HttpConnectionState::Disconnected);
            error
        }
    }
}

/// Starts the silence watch over `client`'s waiting operations. ❗ Holds the
/// client weakly: the watch must not keep a dropped client's connection pool
/// alive.
pub(super) fn watch_over<B: HttpBackend>(owner: &B, client: &Arc<B::Client>) {
    let timings = *owner.session().silence.read_ignore_poison();
    let liveness = Arc::clone(client.liveness());
    let client = Arc::downgrade(client);
    owner
        .host()
        .runtime()
        .spawn(liveness::watch(liveness, timings, B::BACKEND, move || {
            let client = client.upgrade();
            async move {
                match client {
                    Some(client) => client.ping().await,
                    None => false,
                }
            }
        }));
}

/// Starts the backoff loop for a volume that is down. ❗ Only from
/// `Disconnected`: `NeedsCredentials` is a state a person moves forward.
pub(super) fn start_reconnect_loop_if_down<B: HttpBackend>(owner: &B) {
    let session = owner.session();
    if session.connection.state() != HttpConnectionState::Disconnected || session.unmounted.load(Ordering::Relaxed) {
        return;
    }
    owner.host().runtime().spawn(run_reconnect_loop(session.self_handle()));
}

/// The store's entry for this account. On a blocking task, inside
/// `secret_store`: the store may put a Keychain prompt in front of this.
pub(super) async fn stored_secret<B: HttpBackend>(owner: &B) -> Option<StoredCredentials> {
    let key = owner.credential_key();
    secret_store::stored_secret(owner.host(), &key.service, &key.scope).await
}

/// Files the secret a sign-in connected with, the way its Remember box asks
/// (`secret_store::keep_signed_in_secret`): on saves it, off only refreshes an
/// entry still there. Answers whether the store took it.
pub(super) async fn keep_signed_in_secret<B: HttpBackend>(owner: &B, sign_in: &SignIn) -> bool {
    let key = owner.credential_key();
    let took = secret_store::keep_signed_in_secret(owner.host(), &key.service, sign_in).await;
    if !took {
        warn!(
            target: "volume",
            "{} volume '{}': the secret store didn't take the new secret; this connection still came up",
            B::BACKEND,
            owner.session().connection.volume_id()
        );
    }
    took
}

/// The backend's own reconnect cadence. ❗ The handle is re-asked every
/// iteration: an ejected or superseded volume stops answering.
pub(super) async fn run_reconnect_loop<B: HttpBackend>(handle: SelfHandle<B>) {
    for (attempt, delay) in RECONNECT_BACKOFF.iter().enumerate() {
        tokio::time::sleep(*delay).await;
        let Some(owner) = handle
            .live()
            .filter(|owner| !owner.session().unmounted.load(Ordering::Relaxed))
        else {
            return;
        };
        let session = owner.session();
        if session.connection.state() == HttpConnectionState::Connected {
            return;
        }
        let volume_id = session.connection.volume_id().to_string();
        match rebuild(&*owner, None).await {
            Ok(()) => return,
            Err(stalled) => {
                let stop = !matches!(stalled, Stalled::Transient(_));
                report(&*owner, stalled);
                if stop {
                    info!(
                        target: "volume",
                        "{} volume '{volume_id}' needs a person or is switched off; stopping the backoff",
                        B::BACKEND
                    );
                    return;
                }
                debug!(
                    target: "volume",
                    "{} volume '{volume_id}': reconnect {}/{} didn't take",
                    B::BACKEND,
                    attempt + 1,
                    RECONNECT_BACKOFF.len()
                );
            }
        }
    }
    info!(target: "volume", "{} reconnect gave up after {} attempts", B::BACKEND, RECONNECT_BACKOFF.len());
}
