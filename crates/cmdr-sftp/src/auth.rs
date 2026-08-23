//! Which credential to offer a server, in what order, and what a dropped session
//! may do about it unattended.
//!
//! Pure policy. `transport.rs` is what executes a rung against `russh`; keeping
//! the order and the reconnect rules here means both are readable, and testable,
//! without a server.

use std::path::PathBuf;

use crate::SftpConnectionParams;

/// One way of proving who we are, in the order the ladder tries them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRung {
    /// The running ssh-agent signs for us. Nothing is stored and nothing is
    /// prompted, which is why it goes first.
    Agent,
    /// A private key file the user picked. Its PATH is a connection parameter;
    /// its passphrase, if it has one, is a secret and comes from the store.
    KeyFile(PathBuf),
    /// A password from the secret store.
    Password,
    /// The server drives the prompts. This is where 2FA lives.
    KeyboardInteractive,
}

/// The rung a live session was actually built on.
///
/// Recorded because it, not the ladder, decides what a dropped session may do on
/// its own: see [`reconnect_policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRungUsed {
    /// The ssh-agent signed.
    Agent,
    /// A key file, and whether unlocking it took a passphrase.
    KeyFile {
        /// `true` when the file was encrypted, so reconnecting needs a secret
        /// that is deliberately no longer held.
        passphrase_protected: bool,
    },
    /// A password from the store.
    Password,
    /// The server's own prompts.
    KeyboardInteractive,
}

/// What a session built on a given rung may do when it drops, ❗ once the
/// per-server auto-reconnect toggle has already said yes.
///
/// ❗ This is the SECOND gate, never the first. [`unattended_reconnect`] is where
/// the toggle is asked, and a toggle that is off outranks every row here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectPolicy {
    /// Rebuild the session as often as the backoff says. Nothing has to be
    /// asked, so nothing can be locked out.
    Freely,
    /// Re-read the secret store (the secret may have changed) and try ONCE. ❌
    /// Never a loop: repeated refusals lock accounts and trip rate limiters.
    RetryOnceFromStore,
    /// Only the user can move this forward. Report it and stop.
    NeedsCredentials,
}

/// Whether an unattended reconnect can actually happen, as this volume stands.
///
/// ❗ **The backend answers this so the frontend never infers it.** The two
/// toggles are independent — "remember the secret" is exactly "put it in the
/// Keychain", and "reconnect automatically" is exactly "may Cmdr redial
/// unattended" — but their COMBINATION has a real precondition, and this enum is
/// where it's stated: two rungs redial out of the secret store and can't do it
/// with nothing in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnattendedReconnect {
    /// The toggle is off. ❌ Nothing redials on its own, whatever is stored and
    /// whatever rung proved the session.
    TurnedOff,
    /// On, and it works: this rung needs no stored secret, or needs one and has
    /// it.
    Ready,
    /// ❗ On, and it cannot do anything: this rung redials out of the secret
    /// store and nothing is stored. This is the state a UI warns about, and the
    /// only honest way out is remembering the secret.
    NeedsStoredSecret,
    /// On, but this rung can never redial unattended however full the store is:
    /// the server asks the questions and there is nobody to answer them. ❌ Not
    /// [`Self::NeedsStoredSecret`], which would send the user off to remember a
    /// secret that still wouldn't buy a reconnect.
    RungCannot,
}

/// The rungs to offer, in order, for `params`.
///
/// Agent first because it costs the user nothing, then the key file they picked,
/// then the two that need a secret. A server that refuses a rung simply moves
/// the ladder along; only running out of rungs is a failure.
pub fn ladder(params: &SftpConnectionParams) -> Vec<AuthRung> {
    let mut rungs = Vec::with_capacity(4);
    if params.use_agent {
        rungs.push(AuthRung::Agent);
    }
    if let Some(path) = &params.key_file {
        rungs.push(AuthRung::KeyFile(path.clone()));
    }
    rungs.push(AuthRung::Password);
    rungs.push(AuthRung::KeyboardInteractive);
    rungs
}

/// What a dropped session built on `rung` may do unattended, ❗ asked only after
/// the toggle already said yes.
///
/// The two secret-backed rungs share a row on purpose: a password and a key
/// passphrase both come out of the same store, the store is re-read on every
/// dial, and a refusal on either one is a spent authentication attempt. So both
/// get exactly one try and then wait for a person.
pub fn reconnect_policy(rung: AuthRungUsed) -> ReconnectPolicy {
    match rung {
        // A vanished agent socket or a removed identity surfaces as a refusal on
        // the retry, which the reconnect loop reports as needing credentials.
        AuthRungUsed::Agent => ReconnectPolicy::Freely,
        AuthRungUsed::KeyFile {
            passphrase_protected: false,
        } => ReconnectPolicy::Freely,
        AuthRungUsed::KeyFile {
            passphrase_protected: true,
        }
        | AuthRungUsed::Password => ReconnectPolicy::RetryOnceFromStore,
        // The server asks the questions, so there is nobody to answer them.
        AuthRungUsed::KeyboardInteractive => ReconnectPolicy::NeedsCredentials,
    }
}

/// Whether an unattended reconnect can happen at all, given both toggles and the
/// rung the live session was built on.
///
/// ❗ **The toggle is asked first.** Off means off, so the rung and the store
/// never enter into it; that's what keeps neither toggle from silently changing
/// the other's meaning.
///
/// `secret_stored` is "the Keychain holds a secret for this account", which is
/// the whole meaning of the "remember the secret" toggle. ❗ Read it lazily: only
/// the two rungs that redial out of the store care, and a needless read is a
/// needless Keychain prompt.
pub(crate) fn unattended_reconnect(
    auto_reconnect: bool,
    rung: AuthRungUsed,
    secret_stored: bool,
) -> UnattendedReconnect {
    if !auto_reconnect {
        return UnattendedReconnect::TurnedOff;
    }
    match reconnect_policy(rung) {
        ReconnectPolicy::Freely => UnattendedReconnect::Ready,
        ReconnectPolicy::RetryOnceFromStore if secret_stored => UnattendedReconnect::Ready,
        ReconnectPolicy::RetryOnceFromStore => UnattendedReconnect::NeedsStoredSecret,
        ReconnectPolicy::NeedsCredentials => UnattendedReconnect::RungCannot,
    }
}

/// Whether a rung's unattended reconnect reads the secret store to do its work.
///
/// The lazy half of [`unattended_reconnect`]: a caller asks this before paying
/// for a Keychain read that only two rungs can use.
pub(crate) fn redials_from_the_store(rung: AuthRungUsed) -> bool {
    matches!(reconnect_policy(rung), ReconnectPolicy::RetryOnceFromStore)
}

#[cfg(test)]
#[path = "auth_test.rs"]
mod auth_test;
