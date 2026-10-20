//! Coming back after the session drops, on terms the auth rung allows.
//!
//! There is no watcher on this backend, so nothing notices a dead session until
//! something tries to use it. The first operation that does calls
//! [`SftpVolume::note_lost_session`], which flips the state once (only if the
//! session it ran on is still the installed one), drops the transport, and
//! starts the backoff loop below. Each installed session gets the next
//! generation, which is what `Volume::connection_generation` reports. Everything after that is the
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
//!   what the user typed straight to [`transport::dial`]; the store is only ever
//!   REFRESHED, never seeded.
//! - ❗ **No reconnect on this path can be called off**, because nobody is
//!   watching an unattended redial: the dial gets a token nothing holds, and the
//!   phase budgets are the only backstop. Abandoning one is safe on its own
//!   terms (`transport::PendingEngine`).

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::volume::secret_store::{self, SignIn, SignedIn};
use cmdr_fs::volume::{SelfHandle, VolumeError};
use log::{debug, info, warn};
use tokio_util::sync::CancellationToken;

use super::state::ConnectionState;
use super::{SftpVolume, SftpVolumeInner};
use crate::auth::{
    AuthRungUsed, ReconnectPolicy, UnattendedReconnect, reconnect_policy, redials_from_the_store, unattended_reconnect,
};
use crate::errors::SftpConnectError;
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

/// How an attempt left a live session, which an attended sign-in needs to know
/// before it writes the typed secret down.
#[derive(Debug, Clone, Copy)]
enum Rebuilt {
    /// A live session was already there, so nothing was dialed.
    AlreadyLive,
    /// A fresh session came up on this rung.
    Dialed(AuthRungUsed),
}

/// Whether a session on `rung` was opened WITH the secret the dial carried. The
/// agent and an unencrypted key file take none, so a session on either proves
/// nothing about a typed one.
fn rung_took_the_secret(rung: AuthRungUsed) -> bool {
    match rung {
        AuthRungUsed::Agent
        | AuthRungUsed::KeyFile {
            passphrase_protected: false,
        } => false,
        AuthRungUsed::Password
        | AuthRungUsed::KeyboardInteractive
        | AuthRungUsed::KeyFile {
            passphrase_protected: true,
        } => true,
    }
}

impl SftpVolume {
    /// Notices, once, that the session behind this volume is gone.
    ///
    /// Called on every operation's error path (`volume_impl.rs` § noting), which
    /// is the only place this backend CAN learn it: with no watcher, a dead
    /// session is invisible until something asks it for something.
    ///
    /// ❗ Acts only on the `Connected` → `Disconnected` edge, and only for the
    /// session the failing call RAN on (`ran_on`, read before the call started).
    /// Several in-flight operations see the same broken session at the same
    /// moment, and the ones that lose the swap must not each start a reconnect
    /// loop; a call that started on a session a reconnect has since replaced
    /// must not take the fresh one down.
    pub(super) fn note_lost_session(&self, error: &VolumeError, ran_on: u64) {
        if !matches!(error, VolumeError::DeviceDisconnected(_)) {
            return;
        }
        if !self.inner.connection.lose(ran_on) {
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
            inner.drop_dead_session().await;
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
        self.rebuild(None)
            .await
            .map(|_| ())
            .map_err(|stalled| self.report(stalled))
    }

    /// The attended variant: the user just typed a secret, so the unattended
    /// policy doesn't apply. Refused where [`check_sign_in`](Self::check_sign_in) says.
    pub(super) async fn do_reconnect_with_credentials(&self, sign_in: SignIn) -> Result<SignedIn, VolumeError> {
        self.check_sign_in(&sign_in)?;
        self.auth_attempt_spent.store(false, Ordering::Relaxed);
        let rebuilt = self
            .rebuild(Some(sign_in.secret.clone()))
            .await
            .map_err(|stalled| self.report(stalled))?;
        // ❗ Written only once the typed secret WORKED (#530): a typo written
        // before the dial replaced the one that did. A session that was already
        // live, or a rung that took no secret (the agent signed first), proved
        // nothing about it.
        if let Rebuilt::Dialed(rung) = rebuilt
            && rung_took_the_secret(rung)
        {
            return Ok(self.file_sign_in(&sign_in).await);
        }
        Ok(SignedIn::NOTHING_FILED)
    }

    /// Files a sign-in that already connected, ❌ without dialing: the sheet's
    /// "Try again" after a refused save (#603). A second sign-in would find the
    /// session live and file nothing. Refused where the sign-in itself is.
    pub(super) async fn do_remember_sign_in(&self, sign_in: SignIn) -> Result<SignedIn, VolumeError> {
        self.check_sign_in(&sign_in)?;
        Ok(self.file_sign_in(&sign_in).await)
    }

    /// Whether a typed secret can mend this volume at all.
    ///
    /// - `username` must be the account this volume IS. Two accounts on one
    ///   server see different files under the same paths and carry different
    ///   volume ids (`sftp_volume_id(host, port, username)`), so signing in as
    ///   another one means opening another volume rather than mending this one.
    /// - An agent or an unencrypted key file is missing no secret, so there is
    ///   nothing a typed one can fix. ❗ The frontend must not offer a sign-in
    ///   there: a button that answers `NotSupported` every time is worse than
    ///   no button. The other three rungs take one, ❌ with no special case for
    ///   the passphrase: what decides whether it's written down is the
    ///   "remember the secret" switch, not the rung.
    fn check_sign_in(&self, sign_in: &SignIn) -> Result<(), VolumeError> {
        if sign_in.username != self.params().username {
            return Err(VolumeError::NotSupported);
        }
        match self.auth_rung() {
            AuthRungUsed::Agent
            | AuthRungUsed::KeyFile {
                passphrase_protected: false,
            } => Err(VolumeError::NotSupported),
            AuthRungUsed::Password
            | AuthRungUsed::KeyboardInteractive
            | AuthRungUsed::KeyFile {
                passphrase_protected: true,
            } => Ok(()),
        }
    }

    /// One rebuild attempt, under the single-flight lock.
    ///
    /// `offered` is a secret a human just typed. Its presence is what marks the
    /// attempt ATTENDED, which is the whole difference the policy is about: a
    /// rung that may not reconnect on its own may always reconnect with a person
    /// watching.
    async fn rebuild(&self, offered: Option<String>) -> Result<Rebuilt, Stalled> {
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
            if self.connection.state() == ConnectionState::Connected {
                return Ok(Rebuilt::AlreadyLive);
            }
            // ❗ The dead session a lost-session notice hasn't got round to
            // dropping yet: that happens on a spawned task, and the frontend's
            // reconnect fires on the very event that notice sent. Answering
            // "fine" here would tell it a server that is still gone is back.
            self.drop_dead_session().await;
        }
        if !attended {
            self.check_unattended_policy()?;
        }

        // ❗ A token nobody else holds, so nothing can call this off: a reconnect
        // has no user watching it, and the backoff loop's own gates
        // (`check_unattended_policy`, `unmounted`) are what stop it.
        match transport::dial(self.params(), self.host.clone(), offered, CancellationToken::new()).await {
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
                // ❗ Installed, given the next generation, and marked
                // `Connected` under ONE guard, so `drop_dead_session` never sees
                // the new session beside a state that still says the old one
                // died, and no call reads the new session under the old
                // generation.
                let mut session = self.session.write().await;
                *session = Some(Arc::new(connection));
                self.set_auth_rung(rung);
                self.auth_attempt_spent.store(false, Ordering::Relaxed);
                self.connection.install();
                drop(session);
                info!(target: "volume", "sftp volume '{}' is back", self.volume_id);
                Ok(Rebuilt::Dialed(rung))
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

    /// Drops the installed session if it's the one that died.
    ///
    /// ❗ Only while the state says it's gone. The lost-session notice runs this
    /// on a spawned task, and by the time it gets the lock a reconnect may
    /// already have installed a FRESH session; taking that one would leave a
    /// volume reporting `Connected` with nothing behind it. A fresh session is
    /// installed under the same guard that marks it `Connected`, so the state
    /// read here is never stale.
    async fn drop_dead_session(&self) {
        let mut session = self.session.write().await;
        if self.connection.state() != ConnectionState::Connected {
            session.take();
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
        if self.connection.state() != ConnectionState::Disconnected || self.unmounted.load(Ordering::Relaxed) {
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
        let params = self.params();
        secret_store::has_stored_secret(&self.host, &params.credential_service(), &params.username).await
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
                self.connection.emit_if_changed(ConnectionState::Disconnected);
                VolumeError::NotSupported
            }
            Stalled::NeedsUser => {
                self.connection.emit_if_changed(ConnectionState::NeedsCredentials);
                VolumeError::PermissionDenied {
                    path: self.volume_id.clone(),
                    raw_os_error: None,
                }
            }
            Stalled::HostKeyNeedsApproval => {
                // ❗ Its own state rather than `NeedsCredentials`: the frontend
                // must send the user to look at the key, never to a sign-in box.
                // The key itself doesn't ride this event (the wire enum is
                // payload-free); the user opens the server again and the connect
                // command's typed outcome carries the fingerprint.
                self.connection.emit_if_changed(ConnectionState::NeedsHostKeyApproval);
                warn!(
                    target: "volume",
                    "sftp volume '{}' presented a host key that isn't the trusted one; not reconnecting",
                    self.volume_id
                );
                VolumeError::PermissionDenied {
                    path: self.volume_id.clone(),
                    raw_os_error: None,
                }
            }
            Stalled::Transient(error) => {
                self.connection.emit_if_changed(ConnectionState::Disconnected);
                error
            }
        }
    }

    /// Files the secret a sign-in connected with, the way its Remember box asks:
    /// on saves it (seeding an empty store), off only refreshes an entry still
    /// there (`secret_store::keep_signed_in_secret`). "Remember the secret" means
    /// exactly "the Keychain holds one for this account", so the box decides,
    /// ❌ never the dial on its own.
    ///
    /// A refusal is non-fatal: only the "silent next time" guarantee is lost,
    /// and the answer says so when a checked box was refused.
    async fn file_sign_in(&self, sign_in: &SignIn) -> SignedIn {
        let took = secret_store::keep_signed_in_secret(&self.host, &self.params().credential_service(), sign_in).await;
        if !took {
            warn!(
                target: "volume",
                "sftp volume '{}': the secret store didn't take the new secret; this session still came up",
                self.volume_id
            );
        }
        SignedIn::after(sign_in, took)
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
        if inner.connection.state() == ConnectionState::Connected {
            debug!(target: "volume", "sftp volume '{}' is already back; loop done", inner.volume_id);
            return;
        }
        let volume_id = inner.volume_id.clone();
        match inner.rebuild(None).await {
            Ok(_) => return,
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

#[cfg(test)]
#[path = "connection_drop_test.rs"]
mod connection_drop_test;
