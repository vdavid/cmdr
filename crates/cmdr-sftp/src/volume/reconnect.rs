//! Coming back after the session drops, on terms the auth rung allows.
//!
//! There is no watcher on this backend, so nothing notices a dead session until
//! something tries to use it. The first operation that does calls
//! [`SftpVolume::note_lost_session`], which flips the state once, drops the
//! transport, and starts the backoff loop below. Everything after that is the
//! policy in [`crate::auth::reconnect_policy`] being obeyed.
//!
//! Three rules hold this module up:
//!
//! - ❌ **Never loop on an authentication attempt.** Repeated wrong passwords
//!   lock accounts. The password rung gets exactly one unattended try, latched by
//!   [`SftpVolumeInner::password_attempt_spent`], and only a human clears it.
//! - ❗ **A secret dies with the dial it built.** An attended reconnect passes
//!   what the user typed straight to [`guarded_dial`]; only a PASSWORD is also
//!   written to the store, because that is where a password already lives.
//!   ❌ Never a key passphrase — persisting one would quietly turn the rung that
//!   deliberately cannot reconnect unattended into one that can.
//! - ❗ **A dial is never dropped mid-handshake.** [`guarded_dial`] runs it in a
//!   task and awaits the join handle, because a cancelled connect panics inside
//!   the engine (`transport.rs`).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::StoredCredentials;
use cmdr_fs::volume::{SelfHandle, VolumeError};
use log::{debug, info, warn};

use super::state::ConnectionState;
use super::{SftpVolume, SftpVolumeInner};
use crate::auth::{AuthRungUsed, ReconnectPolicy, reconnect_policy};
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
/// a caller who goes away detaches the task instead of cancelling it. Cancelling
/// a connect mid-handshake panics a task inside `openssh-sftp-client`
/// (`transport.rs` § hazard 2). ❌ Never call [`transport::dial`] directly.
pub(super) async fn guarded_dial(
    host: &VolumeHost,
    params: SftpConnectionParams,
    offered_secret: Option<String>,
) -> Result<DialOutcome, SftpConnectError> {
    host.runtime()
        .spawn(transport::dial(params, host.clone(), offered_secret))
        .await
        .map_err(|join| SftpConnectError::Transport(join.to_string()))?
}

/// Why an attempt didn't leave a live session, in the terms the loop acts on.
#[derive(Debug)]
enum Stalled {
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
        inner.host.runtime().spawn(async move {
            // Dropping the transport IS the shutdown; the engine's own drop
            // orders it and both its tasks exit (`transport.rs` § hazard 1).
            inner.session.write().await.take();
            drop(inner);
            run_reconnect_loop(handle).await;
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
            // The store is where a password already lives, so saving it is what
            // makes the NEXT reconnect silent. The dial gets it directly too, so
            // a keychain that refuses the write still reconnects now.
            AuthRungUsed::Password | AuthRungUsed::KeyboardInteractive => {
                self.save_password(&username, &password).await;
                self.password_attempt_spent.store(false, Ordering::Relaxed);
                self.rebuild(Some(password))
                    .await
                    .map_err(|stalled| self.report(stalled))
            }
            // ❌ A key passphrase is NOT saved. Persisting it would make this rung
            // reconnect unattended forever after, which is the opposite of what
            // putting a passphrase on a key means.
            AuthRungUsed::KeyFile {
                passphrase_protected: true,
            } => {
                self.password_attempt_spent.store(false, Ordering::Relaxed);
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

        match guarded_dial(&self.host, self.params.clone(), offered).await {
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
                self.password_attempt_spent.store(false, Ordering::Relaxed);
                self.emit_if_changed(ConnectionState::Connected);
                info!(target: "volume", "sftp volume '{}' is back", self.volume_id);
                Ok(())
            }
            Ok(DialOutcome::NeedsHostKeyApproval(_)) => Err(Stalled::HostKeyNeedsApproval),
            Err(SftpConnectError::HostKeyRevoked { .. }) => Err(Stalled::HostKeyNeedsApproval),
            Err(SftpConnectError::AuthenticationRejected) => {
                // The one unattended password attempt, spent. Only a human clears
                // this, through `do_reconnect_with_credentials`.
                self.password_attempt_spent.store(true, Ordering::Relaxed);
                Err(Stalled::NeedsUser)
            }
            Err(SftpConnectError::NeedsCredentials) => Err(Stalled::NeedsUser),
            Err(SftpConnectError::TimedOut) => Err(Stalled::Transient(VolumeError::ConnectionTimeout(
                self.volume_id.clone(),
            ))),
            Err(SftpConnectError::Unreachable(what) | SftpConnectError::Transport(what)) => {
                Err(Stalled::Transient(VolumeError::DeviceDisconnected(what)))
            }
        }
    }

    /// What the rung this session was built on may do on its own.
    fn check_unattended_policy(&self) -> Result<(), Stalled> {
        match reconnect_policy(self.auth_rung()) {
            ReconnectPolicy::Freely => Ok(()),
            // ❌ Never a second unattended attempt. The store is re-read on every
            // dial, so the ONE try already carries whatever the user changed.
            ReconnectPolicy::RetryOnceFromStore => {
                if self.password_attempt_spent.load(Ordering::Relaxed) {
                    Err(Stalled::NeedsUser)
                } else {
                    Ok(())
                }
            }
            ReconnectPolicy::NeedsCredentials => Err(Stalled::NeedsUser),
        }
    }

    /// Reports the stall and turns it into the trait's vocabulary.
    ///
    /// ❗ `PermissionDenied` for everything a human owns, which is what stops the
    /// loop below: it is the one answer where trying again can only make things
    /// worse.
    fn report(&self, stalled: Stalled) -> VolumeError {
        match stalled {
            Stalled::NeedsUser => {
                self.emit_if_changed(ConnectionState::NeedsCredentials);
                VolumeError::PermissionDenied(self.volume_id.clone())
            }
            Stalled::HostKeyNeedsApproval => {
                // The volume goes quiet rather than asking for a password: see
                // `Stalled::HostKeyNeedsApproval`. The frontend learns there is a
                // key to look at when the user opens the server again, which runs
                // the full approval flow.
                self.emit_if_changed(ConnectionState::Disconnected);
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

    /// Writes the password the user typed to the secret store.
    ///
    /// ❗ On a blocking task: the store may put a Keychain prompt in front of
    /// this, and blocking the async runtime on a modal dialog stalls every other
    /// volume. A refusal is non-fatal — only the "silent next time" guarantee is
    /// lost.
    async fn save_password(&self, username: &str, password: &str) {
        let host = self.host.clone();
        let service = self.params.credential_service();
        let scope = username.to_string();
        let stored = StoredCredentials {
            username: username.to_string(),
            secret: password.to_string(),
        };
        let saved =
            tokio::task::spawn_blocking(move || host.credentials().save_credentials(&service, Some(&scope), &stored))
                .await;
        if !matches!(saved, Ok(Ok(()))) {
            warn!(
                target: "volume",
                "sftp volume '{}': the secret store didn't take the new password; this session still came up",
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
                let needs_a_person = !matches!(stalled, Stalled::Transient(_));
                inner.report(stalled);
                if needs_a_person {
                    info!(
                        target: "volume",
                        "sftp volume '{volume_id}' needs a person; stopping the backoff so nothing burns an authentication attempt"
                    );
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
