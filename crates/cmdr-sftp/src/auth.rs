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

/// What a session built on a given rung may do when it drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectPolicy {
    /// Rebuild the session as often as the backoff says. Nothing has to be
    /// asked, so nothing can be locked out.
    Freely,
    /// Re-read the secret store (the password may have changed) and try ONCE. ❌
    /// Never a loop: repeated wrong passwords lock accounts.
    RetryOnceFromStore,
    /// Only the user can move this forward. Report it and stop.
    NeedsCredentials,
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

/// What a dropped session built on `rung` may do unattended.
///
/// The passphrase case is the one worth stating out loud: a passphrase is a
/// secret, so it isn't held past the session it unlocked, which means an
/// encrypted key file genuinely cannot reconnect on its own however convenient
/// that would be.
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
        } => ReconnectPolicy::NeedsCredentials,
        AuthRungUsed::Password => ReconnectPolicy::RetryOnceFromStore,
        // The server asks the questions, so there is nobody to answer them.
        AuthRungUsed::KeyboardInteractive => ReconnectPolicy::NeedsCredentials,
    }
}

#[cfg(test)]
#[path = "auth_test.rs"]
mod auth_test;
