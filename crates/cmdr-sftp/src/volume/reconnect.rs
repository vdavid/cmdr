//! Coming back after the session drops, on terms the auth rung allows.
//!
//! There is no watcher on this backend, so nothing notices a dead session until
//! something tries to use it. The first operation that does calls
//! [`SftpVolume::note_lost_session`], which flips the state once, drops the
//! transport, and starts the backoff loop below. Everything after that is the
//! policy in [`crate::auth::reconnect_policy`] being obeyed.
//!
//! **Two independent switches decide what may happen, in this order.**
//!
//! 1. **"Reconnect automatically"**, the user's own per-server switch
//!    ([`SftpVolume::set_auto_reconnect`]). Off means ❌ no unattended dial, ever,
//!    whatever is stored and whatever rung proved the last session.
//! 2. **The rung's policy** ([`crate::auth::reconnect_policy`]), asked only once
//!    the switch has said yes.
//!
//! ❌ **Neither switch silently changes the other's meaning.** The second one,
//! "remember the secret", is exactly "the Keychain holds a secret for this
//! account" — so this module ❌ never writes one that wasn't already there, and
//! [`SftpVolumeInner::unattended_reconnect`] is how the frontend learns that a
//! switch is on and can't work.
//!
//! Three rules hold the rest of it up:
//!
//! - ❌ **Never loop on an authentication attempt.** Repeated refusals lock
//!   accounts. The two secret-backed rungs get exactly one unattended try each,
//!   latched by [`SftpVolumeInner::auth_attempt_spent`], and only a human clears
//!   it.
//! - ❗ **A secret dies with the dial it built.** An attended reconnect passes
//!   what the user typed straight to [`guarded_dial`]; the store is only ever
//!   REFRESHED, never seeded.
//! - ❗ **A dial is never dropped mid-handshake.** [`guarded_dial`] runs it in a
//!   task and awaits the join handle, so an abandoned connect detaches instead.
//!   Calling one OFF is a different thing and goes through the token instead, and
//!   no reconnect on this path has one, because nobody is watching an unattended
//!   redial.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::StoredCredentials;
use cmdr_fs::volume::{SelfHandle, VolumeError};
use log::{debug, info, warn};
use tokio_util::sync::CancellationToken;

use super::state::ConnectionState;
use super::{SftpVolume, SftpVolumeInner};
use crate::auth::{
    AuthRungUsed, ReconnectPolicy, UnattendedReconnect, reconnect_policy, redials_from_the_store, unattended_reconnect,
};
use crate::errors::SftpConnectError;
use crate::params::SftpConnectionParams;
use crate::transport::{self, DialOutcome};

/// How long the backend waits between its own reconnect attempts.
///
/// Bounded and growing: a handful of tries over a few minutes, then it stops
/// rather than hammering a server that is genuinely down. The frontend runs its
/// own cadence while a pane is open; the two coalesce on
/// [`SftpVolumeInner::reconnect_lock`].
const RECONNECT_BACKOFF: [Duration; 6] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

/// Dials without ever dropping the dial.
///
/// ❗ The future is handed to a task and the JOIN HANDLE is what gets awaited, so
/// a caller who goes away detaches the task instead of dropping the dial
/// mid-handshake. ❌ Never call [`transport::dial`] directly.
///
/// ❗ That detaching IS what the spawn is for. The panic it once routed around is
/// fixed in `openssh-sftp-client` 0.15.8, but a dial dropped inside the SFTP
/// hello detaches the ENGINE instead and leaves the server's session open for the
/// life of the process, and the phase deadline can't end it because nothing is
/// left polling the dial. `crates/cmdr-sftp/DETAILS.md` § "2. An abandoned
/// `Sftp::new`" carries the measurement, and what retiring this would take.
///
/// ❗ The spawn costs no cancellation, because calling a connect OFF goes
/// through `cancel` rather than through dropping this future: the token reaches
/// into the task, the dial answers `Cancelled`, and the join handle comes back
/// at once. A caller with nobody to cancel for it passes a token nothing holds.
pub(super) async fn guarded_dial(
    host: &VolumeHost,
    params: SftpConnectionParams,
    offered_secret: Option<String>,
    cancel: CancellationToken,
) -> Result<DialOutcome, SftpConnectError> {
    host.runtime()
        .spawn(transport::dial(params, host.clone(), offered_secret, cancel))
        .await
        .map_err(|join| SftpConnectError::Transport(join.to_string()))?
}

/// Why an attempt didn't leave a live session, in the terms the loop acts on.
#[derive(Debug)]
enum Stalled {
    /// The user's "reconnect automatically" switch is off. ❗ Its own arm rather
    /// than [`Self::NeedsUser`]: the credentials may be perfectly fine, and
    /// telling the frontend they aren't would put a sign-in box in front of a
    /// setting.
    AutoReconnectOff,
    /// Only a human moves this forward. Retrying costs an authentication attempt
    /// and buys nothing.
    NeedsUser,
    /// The server presented a key we don't hold, or one that changed. ❗ Terminal
    /// and deliberately SILENT about credentials: a changed key is the shape a
    /// man-in-the-middle takes, and a sign-in prompt in front of one is how a
    /// password gets typed into it.
    HostKeyNeedsApproval,
    /// The network, the server, or the clock. Worth trying again.
    Transient(VolumeError),
}

impl SftpVolume {
    /// Notices, once, that the session behind this volume is gone.
    ///
    /// Called on every operation's error path (`volume_impl.rs` § noting), which
    /// is the only place this backend CAN learn it: with no watcher, a dead
    /// session is invisible until something asks it for something.
    ///
    /// ❗ Acts only on the `Connected` → `Disconnected` edge. Several in-flight
    /// operations see the same broken session at the same moment, and the ones
    /// that lose the swap must not each start a reconnect loop.
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
        // ❗ Read before the task, so the switch decides whether a backoff loop is
        // even born. `rebuild` refuses one anyway; not starting it is what keeps a
        // volume the user switched off from sleeping through six timers.
        let auto_reconnect = inner.auto_reconnect.load(Ordering::Relaxed);
        inner.host.runtime().spawn(async move {
            // Dropping the transport IS the shutdown; the engine's own drop
            // orders it and both its tasks exit. ❌ Never `Sftp::close()` here:
            // it hangs forever over a `russh` channel (`DETAILS.md` § hazard 1).
            inner.session.write().await.take();
            drop(inner);
            if auto_reconnect {
                run_reconnect_loop(handle).await;
            }
        });
    }
}

impl SftpVolumeInner {
    /// Rebuilds the session in place, if the rung this one was built on allows it
    /// unattended.
    ///
    /// Idempotent and single-flight: concurrent callers wait on
    /// `reconnect_lock`, and the second one finds a live session.
    pub(super) async fn do_attempt_reconnect(&self) -> Result<(), VolumeError> {
        self.rebuild(None).await.map_err(|stalled| self.report(stalled))
    }

    /// The attended variant: the user just typed a secret, so the unattended
    /// policy doesn't apply.
    ///
    /// `username` must be the account this volume IS. Two accounts on one server
    /// see different files under the same paths and carry different volume ids
    /// (`sftp_volume_id(host, port, username)`), so signing in as another one
    /// means opening another volume rather than mending this one.
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        username: String,
        password: String,
    ) -> Result<(), VolumeError> {
        if username != self.params.username {
            return Err(VolumeError::NotSupported);
        }
        match self.auth_rung() {
            // Neither rung is missing a secret, so there is nothing a typed one
            // can fix. ❗ The frontend must not offer a sign-in here: a button
            // that answers `NotSupported` every time is worse than no button.
            AuthRungUsed::Agent
            | AuthRungUsed::KeyFile {
                passphrase_protected: false,
            } => Err(VolumeError::NotSupported),
            // The dial gets the typed secret directly, so a keychain that refuses
            // the write still reconnects now. The same three rungs, ❌ with no
            // special case for the passphrase: what decides whether it's written
            // down is the "remember the secret" switch, not the rung.
            AuthRungUsed::Password
            | AuthRungUsed::KeyboardInteractive
            | AuthRungUsed::KeyFile {
                passphrase_protected: true,
            } => {
                self.refresh_remembered_secret(&username, &password).await;
                self.auth_attempt_spent.store(false, Ordering::Relaxed);
                self.rebuild(Some(password))
                    .await
                    .map_err(|stalled| self.report(stalled))
            }
        }
    }

    /// One rebuild attempt, under the single-flight lock.
    ///
    /// `offered` is a secret a human just typed. Its presence is what marks the
    /// attempt ATTENDED, which is the whole difference the policy is about: a
    /// rung that may not reconnect on its own may always reconnect with a person
    /// watching.
    async fn rebuild(&self, offered: Option<String>) -> Result<(), Stalled> {
        let attended = offered.is_some();
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(Stalled::Transient(VolumeError::DeviceDisconnected(
                self.volume_id.clone(),
            )));
        }
        let _guard = self.reconnect_lock.lock().await;
        // The wait for the lock can be a whole handshake long, and an unmount or
        // a winning attempt can land inside it.
        if self.unmounted.load(Ordering::Relaxed) {
            return Err(Stalled::Transient(VolumeError::DeviceDisconnected(
                self.volume_id.clone(),
            )));
        }
        if self.session.read().await.is_some() {
            return Ok(());
        }
        if !attended {
            self.check_unattended_policy()?;
        }

        // ❗ A token nobody else holds, so nothing can call this off: a reconnect
        // has no user watching it, and the backoff loop's own gates
        // (`check_unattended_policy`, `unmounted`) are what stop it.
        match guarded_dial(&self.host, self.params.clone(), offered, CancellationToken::new()).await {
            Ok(DialOutcome::Connected { connection, rung }) => {
                // Between the dial starting and the session landing, the user may
                // have ejected the volume. Installing here would leave a live SSH
                // connection nobody can reach.
                if self.unmounted.load(Ordering::Relaxed) {
                    drop(connection);
                    return Err(Stalled::Transient(VolumeError::DeviceDisconnected(
                        self.volume_id.clone(),
                    )));
                }
                *self.session.write().await = Some(Arc::new(connection));
                self.set_auth_rung(rung);
                self.auth_attempt_spent.store(false, Ordering::Relaxed);
                self.emit_if_changed(ConnectionState::Connected);
                info!(target: "volume", "sftp volume '{}' is back", self.volume_id);
                Ok(())
            }
            Ok(DialOutcome::NeedsHostKeyApproval(_)) => Err(Stalled::HostKeyNeedsApproval),
            Err(SftpConnectError::HostKeyRevoked { .. }) => Err(Stalled::HostKeyNeedsApproval),
            Err(SftpConnectError::AuthenticationRejected) => {
                // The one unattended password attempt, spent. Only a human clears
                // this, through `do_reconnect_with_credentials`.
                self.auth_attempt_spent.store(true, Ordering::Relaxed);
                Err(Stalled::NeedsUser)
            }
            Err(SftpConnectError::NeedsCredentials) => Err(Stalled::NeedsUser),
            // Unreachable as this stands, since the token above is nobody's.
            // Reported as a transient loss rather than `unreachable!()` so a
            // future caller that DOES hand one in gets a retry, not a panic.
            Err(SftpConnectError::Cancelled) => Err(Stalled::Transient(VolumeError::Cancelled(self.volume_id.clone()))),
            Err(SftpConnectError::TimedOut) => Err(Stalled::Transient(VolumeError::ConnectionTimeout(
                self.volume_id.clone(),
            ))),
            Err(SftpConnectError::Unreachable(what) | SftpConnectError::Transport(what)) => {
                Err(Stalled::Transient(VolumeError::DeviceDisconnected(what)))
            }
        }
    }

    /// Whether this volume may dial on its own right now: the switch first, then
    /// the rung.
    fn check_unattended_policy(&self) -> Result<(), Stalled> {
        // ❗ Asked first, so "off" never depends on the rung or on what the store
        // holds. That ordering IS the promise that neither switch changes the
        // other's meaning.
        if !self.auto_reconnect.load(Ordering::Relaxed) {
            return Err(Stalled::AutoReconnectOff);
        }
        match reconnect_policy(self.auth_rung()) {
            ReconnectPolicy::Freely => Ok(()),
            // ❌ Never a second unattended attempt. The store is re-read on every
            // dial, so the ONE try already carries whatever the user changed.
            ReconnectPolicy::RetryOnceFromStore => {
                if self.auth_attempt_spent.load(Ordering::Relaxed) {
                    Err(Stalled::NeedsUser)
                } else {
                    Ok(())
                }
            }
            ReconnectPolicy::NeedsCredentials => Err(Stalled::NeedsUser),
        }
    }

    /// Starts the backoff loop for a volume that is down, if one is worth
    /// starting.
    ///
    /// ❗ Only from `Disconnected`. `Connected` has nothing to rebuild, and
    /// `NeedsCredentials` / `NeedsHostKeyApproval` are states a person moves
    /// forward — a loop against either would spend authentication attempts, or
    /// dial a server whose key stopped matching.
    pub(super) fn start_reconnect_loop_if_down(&self) {
        if self.connection_state() != ConnectionState::Disconnected || self.unmounted.load(Ordering::Relaxed) {
            return;
        }
        let handle = self.self_handle();
        self.host.runtime().spawn(run_reconnect_loop(handle));
    }

    /// Whether an unattended reconnect can happen as this volume stands, for the
    /// frontend to warn on.
    ///
    /// ❗ The secret store is asked only for the rungs that redial out of it: on
    /// macOS a read can put a Keychain prompt in front of the user, and a banner
    /// rendering is no reason for one. ❗ On a blocking task when it IS asked.
    pub(super) async fn unattended_reconnect(&self) -> UnattendedReconnect {
        let on = self.auto_reconnect.load(Ordering::Relaxed);
        let rung = self.auth_rung();
        let secret_stored = if on && redials_from_the_store(rung) {
            self.stored_secret_exists().await
        } else {
            false
        };
        unattended_reconnect(on, rung, secret_stored)
    }

    /// Whether the Keychain holds a secret for this account, which is the whole
    /// meaning of the "remember the secret" switch.
    ///
    /// ❗ On a blocking task: the store may put a prompt in front of this, and a
    /// modal dialog on the async runtime stalls every other volume. ❌ The secret
    /// itself is dropped on the spot; only its existence comes back.
    async fn stored_secret_exists(&self) -> bool {
        let host = self.host.clone();
        let service = self.params.credential_service();
        let scope = self.params.username.clone();
        tokio::task::spawn_blocking(move || host.credentials().credentials(&service, Some(&scope)).is_some())
            .await
            .unwrap_or(false)
    }

    /// Reports the stall and turns it into the trait's vocabulary.
    ///
    /// ❗ `PermissionDenied` for everything a human owns, which is what stops the
    /// loop below: it is the one answer where trying again can only make things
    /// worse.
    fn report(&self, stalled: Stalled) -> VolumeError {
        match stalled {
            // ❗ `Disconnected`, which is the plain truth, and `NotSupported`,
            // which has exactly one source: this volume doesn't do unattended
            // reconnects. ❌ Never `NeedsCredentials` — nothing is wrong with the
            // credentials, and a frontend that got one would open a sign-in box
            // over a setting the user chose.
            Stalled::AutoReconnectOff => {
                self.emit_if_changed(ConnectionState::Disconnected);
                VolumeError::NotSupported
            }
            Stalled::NeedsUser => {
                self.emit_if_changed(ConnectionState::NeedsCredentials);
                VolumeError::PermissionDenied(self.volume_id.clone())
            }
            Stalled::HostKeyNeedsApproval => {
                // ❗ Its own state rather than `NeedsCredentials`: the frontend
                // must send the user to look at the key, never to a sign-in box.
                // The key itself doesn't ride this event (the wire enum is
                // payload-free); the user opens the server again and the connect
                // command's typed outcome carries the fingerprint.
                self.emit_if_changed(ConnectionState::NeedsHostKeyApproval);
                warn!(
                    target: "volume",
                    "sftp volume '{}' presented a host key that isn't the trusted one; not reconnecting",
                    self.volume_id
                );
                VolumeError::PermissionDenied(self.volume_id.clone())
            }
            Stalled::Transient(error) => {
                self.emit_if_changed(ConnectionState::Disconnected);
                error
            }
        }
    }

    /// Brings a REMEMBERED secret up to date with the one the user just typed.
    ///
    /// ❗ **Refreshes, ❌ never seeds.** "Remember the secret" means exactly "the
    /// Keychain holds one for this account", so a store with nothing in it is the
    /// user having said no, and writing the typed secret there would switch the
    /// other toggle on behind their back. `save_sftp_credentials` is how a person
    /// says yes.
    ///
    /// ❗ Both halves on ONE blocking task: the store may put a Keychain prompt in
    /// front of either, and a modal dialog on the async runtime stalls every other
    /// volume. A refusal is non-fatal — only the "silent next time" guarantee is
    /// lost.
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
                "sftp volume '{}': the secret store didn't take the new secret; this session still came up",
                self.volume_id
            );
        }
    }
}

/// The backend's own reconnect cadence, for the times no pane is open on the
/// volume and nothing else is driving one.
///
/// ❗ The handle is re-asked every iteration. A volume that was ejected, or
/// superseded by a newer instance, stops answering, and a loop that kept running
/// against it would report a healthy volume's id as down.
async fn run_reconnect_loop(handle: SelfHandle<SftpVolumeInner>) {
    for (attempt, delay) in RECONNECT_BACKOFF.iter().enumerate() {
        tokio::time::sleep(*delay).await;

        let Some(inner) = still_worth_reconnecting(&handle) else {
            return;
        };
        if inner.connection_state() == ConnectionState::Connected {
            debug!(target: "volume", "sftp volume '{}' is already back; loop done", inner.volume_id);
            return;
        }
        let volume_id = inner.volume_id.clone();
        match inner.rebuild(None).await {
            Ok(()) => return,
            Err(stalled) => {
                let switched_off = matches!(stalled, Stalled::AutoReconnectOff);
                let stop = !matches!(stalled, Stalled::Transient(_));
                inner.report(stalled);
                if stop {
                    if switched_off {
                        // The user turned it off while the loop was sleeping.
                        info!(target: "volume", "sftp volume '{volume_id}' no longer reconnects automatically; stopping the backoff");
                    } else {
                        info!(
                            target: "volume",
                            "sftp volume '{volume_id}' needs a person; stopping the backoff so nothing burns an authentication attempt"
                        );
                    }
                    return;
                }
                debug!(
                    target: "volume",
                    "sftp volume '{volume_id}': reconnect {}/{} didn't take",
                    attempt + 1,
                    RECONNECT_BACKOFF.len()
                );
            }
        }
    }
    info!(
        target: "volume",
        "sftp reconnect gave up after {} attempts; the next operation or the frontend's own retry starts it again",
        RECONNECT_BACKOFF.len()
    );
}

/// The volume, while it is still worth acting for: allocated, still the
/// registry's, and not ejected.
fn still_worth_reconnecting(handle: &SelfHandle<SftpVolumeInner>) -> Option<Arc<SftpVolumeInner>> {
    let inner = handle.live()?;
    if inner.unmounted.load(Ordering::Relaxed) {
        return None;
    }
    Some(inner)
}

#[cfg(test)]
#[path = "reconnect_test.rs"]
mod reconnect_test;
