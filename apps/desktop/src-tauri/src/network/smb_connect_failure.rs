//! Why an smb2 connect attempt didn't get in, read by type: a [`Refusal`] (who the
//! attempt went out as, and at which step the server said no), or an
//! [`UpgradeFailure`] for a connection that never got that far.
//!
//! Shared by everything that dials a share and has to explain a no: the mount's
//! second opinion (`share_access`) and both upgrade paths (`smb_upgrade`,
//! `smb_connect_directly`). One reading, so the probe and the upgrade can't come to
//! different conclusions about the same answer. Decision and evidence: `DETAILS.md`
//! § "An auth rejection says what was actually rejected".
//!
//! **The step is the answer.** `cmdr_smb::is_auth_error` asks "would credentials
//! help?", and a refusal at either step says yes, so it can't tell them apart. But
//! `STATUS_ACCESS_DENIED` at SessionSetup means the server wouldn't sign this
//! identity in, while the same status at TreeConnect means it DID, and the share
//! turned it away. Collapsed, a guest a share refused was logged as "the server has
//! guest access turned off", and an account a share refused was asked for its
//! password over and over (ERR-SHUSC). [`RefusedAt::of`] reads the status AND the
//! command, ❌ never the message text.

use smb2::ErrorKind;
use smb2::types::Command;

/// Who an attempt went out as, which decides what a refusal means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignInIdentity {
    /// No username: the attempt went out as `Guest`, `SmbConnectionParams::new`'s
    /// default, and a NetFS mount with `Guest = true`.
    Guest,
    /// A username and password: typed, saved in Cmdr's store, or borrowed from
    /// Finder's.
    Account,
}

impl SignInIdentity {
    /// Mirrors `SmbConnectionParams::new`: no username means the attempt goes out
    /// as a guest. Someone who typed "guest" as their account offered an account.
    pub(crate) fn from_username(username: Option<&str>) -> Self {
        match username {
            Some(_) => Self::Account,
            None => Self::Guest,
        }
    }
}

/// The step at which a server turned an identity away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefusedAt {
    /// Session setup: the server wouldn't sign this identity in, before any share
    /// was named.
    SignIn,
    /// TreeConnect: the identity signed in, and the share wouldn't let it open.
    Share,
}

impl RefusedAt {
    /// Where `error` turned the attempt away, or `None` when it isn't a refusal.
    ///
    /// At TreeConnect only access denied counts: a missing share, or an answer
    /// TreeConnect doesn't give about access, is no verdict on the identity.
    /// Anywhere else, an auth-class answer came from signing in: the
    /// logon-rejection family, access denied, a server wanting signing a guest
    /// session can't give, or an auth exchange smb2 couldn't complete.
    pub(crate) fn of(error: &smb2::Error) -> Option<Self> {
        if matches!(
            error,
            smb2::Error::Protocol {
                command: Command::TreeConnect,
                ..
            }
        ) {
            return (error.kind() == ErrorKind::AccessDenied).then_some(Self::Share);
        }
        cmdr_smb::is_auth_error(error).then_some(Self::SignIn)
    }
}

/// A server saying no: who it said no to, and at which step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) identity: SignInIdentity,
    pub(crate) at: RefusedAt,
}

impl Refusal {
    /// The refusal `error` is for an attempt that went out as `identity`, or `None`
    /// when the server never gave a verdict on it.
    pub(crate) fn of(error: &smb2::Error, identity: SignInIdentity) -> Option<Self> {
        RefusedAt::of(error).map(|at| Self { identity, at })
    }

    /// What this refusal means and what would fix it, for the log line the next
    /// reader of an error report acts on.
    ///
    /// Four answers, because there are four different fixes. A guest refused at
    /// sign-in had no password to be wrong: telling that reader to fix a saved
    /// password sends them into Keychain for an entry that was never written
    /// (ERR-48RZX). Anyone refused at the SHARE signed in fine, so neither "guest
    /// access is off" nor "the password isn't accepted" is true (ERR-SHUSC).
    pub(crate) fn advice(self) -> &'static str {
        match (self.identity, self.at) {
            (SignInIdentity::Guest, RefusedAt::SignIn) => {
                "Nothing is saved for this share, so the attempt went out as a guest, and the server doesn't let guests sign in. Signing in with an account gets the direct connection."
            }
            (SignInIdentity::Guest, RefusedAt::Share) => {
                "Nothing is saved for this share, so the attempt went out as a guest. The server signed the guest in, but this share doesn't let guests open it. Signing in with an account that may open the share gets the direct connection."
            }
            (SignInIdentity::Account, RefusedAt::SignIn) => {
                "The server didn't accept the password for this account. Correcting the saved password, or signing in again, restores the direct connection."
            }
            (SignInIdentity::Account, RefusedAt::Share) => {
                "The account signed in, but this share doesn't let that account open it. Granting it access on the server, or signing in as a different account, gets the direct connection."
            }
        }
    }
}

/// Why a direct connection couldn't be established, as a typed reason rather
/// than a sentence.
///
/// Word-free by design: the frontend renders the copy from the message catalog
/// (`$lib/error-messages/` convention — classification in Rust, words on the frontend).
/// A raw `No route to host (os error 65)` has no business reaching a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeFailure {
    /// Nothing answered on the SMB port: the server is off, asleep, or not on
    /// this network right now.
    Unreachable,
    /// It answered, but the handshake ran out of time.
    TooSlow,
    /// It answered and then something we can't act on went wrong.
    Unexpected,
}

impl UpgradeFailure {
    /// Classifies a connect failure by io kind and smb2 error kind, never by
    /// message text.
    pub(crate) fn from_smb_error(err: &smb2::Error) -> Self {
        use std::io::ErrorKind as Io;
        if let smb2::Error::Io(io_err) = err {
            return match io_err.kind() {
                Io::HostUnreachable | Io::NetworkUnreachable | Io::ConnectionRefused | Io::NotConnected => {
                    Self::Unreachable
                }
                Io::TimedOut => Self::TooSlow,
                _ => Self::Unexpected,
            };
        }
        match err.kind() {
            ErrorKind::TimedOut => Self::TooSlow,
            ErrorKind::ConnectionLost => Self::Unreachable,
            _ => Self::Unexpected,
        }
    }
}

/// Why `try_smb_upgrade` didn't install a session.
#[derive(Debug)]
pub(crate) enum UpgradeError {
    /// The server turned the attempt away: who it went out as, and at which step.
    Refused(Refusal),
    /// No verdict on the identity: the server wasn't reached, or answered
    /// something that isn't a refusal.
    Network {
        reason: UpgradeFailure,
        display_name: String,
    },
}

impl UpgradeError {
    /// Reads a failed connect for an attempt that went out as `username` (`None`
    /// is a guest).
    pub(crate) fn from_connect_error(err: &smb2::Error, username: Option<&str>, display_name: String) -> Self {
        match Refusal::of(err, SignInIdentity::from_username(username)) {
            Some(refusal) => Self::Refused(refusal),
            None => Self::Network {
                reason: UpgradeFailure::from_smb_error(err),
                display_name,
            },
        }
    }
}

/// Where a failed direct connection leaves the user, which is what decides how
/// loud the log is about it.
pub(crate) enum DirectConnectOutcome {
    /// Nobody will be asked anything: the share stays on the macOS kernel mount
    /// for the rest of the session, at kernel-mount speed and without the direct
    /// session's control surface. Always a WARN, because nothing else in the app
    /// will ever mention it.
    StaysOnKernelMount,
    /// The caller gets the failure and surfaces it. A refusal here is ordinary
    /// flow (the "Connect directly" sheet asks for an account), so it's an INFO.
    SurfacedToCaller,
}

/// Writes down why a direct smb2 connection didn't happen, naming who was turned
/// away and where.
///
/// **Why it's a function and not two `warn!`s.** Both upgrade paths end here, so
/// the log answers "why is this share on the slow path" the same way whichever
/// path ran. And it names a refusal, which [`UpgradeFailure`] can't: that type
/// crosses IPC to drive the network-error copy (a refusal reaches
/// `CredentialsNeeded` instead), so it has no refusal variant and reads a
/// `STATUS_LOGON_FAILURE` as `Unexpected`. A stale Keychain password once looked
/// exactly like a flaky server that way, while the share sat silently on the
/// kernel mount at a fraction of the speed.
///
/// `username` is who the attempt went out as (`None` is a guest), which is what
/// separates the four refusals: see [`Refusal::advice`].
pub(crate) fn log_direct_connect_failure(
    server: &str,
    share: &str,
    err: &smb2::Error,
    outcome: DirectConnectOutcome,
    username: Option<&str>,
) {
    let Some(refusal) = Refusal::of(err, SignInIdentity::from_username(username)) else {
        let reason = UpgradeFailure::from_smb_error(err);
        match outcome {
            DirectConnectOutcome::StaysOnKernelMount => log::warn!(
                target: "smb_fallback",
                "Couldn't establish an smb2 connection for {server}/{share} ({reason:?}): {err}. Staying on the macOS kernel mount."
            ),
            DirectConnectOutcome::SurfacedToCaller => log::warn!(
                target: "smb_fallback",
                "Couldn't establish an smb2 connection for {server}/{share} ({reason:?}): {err}"
            ),
        }
        return;
    };
    let who = username.map_or_else(|| "a guest".to_string(), |name| format!("\"{name}\""));
    let step = match refusal.at {
        RefusedAt::SignIn => "sign-in",
        RefusedAt::Share => "the share",
    };
    let advice = refusal.advice();
    match outcome {
        DirectConnectOutcome::StaysOnKernelMount => log::warn!(
            target: "smb_fallback",
            "{server}/{share} turned {who} away at {step} ({err}), so it stays on the macOS kernel mount: slower, and Cmdr can't manage the connection. {advice}"
        ),
        DirectConnectOutcome::SurfacedToCaller => log::info!(
            target: "smb_fallback",
            "{server}/{share} turned {who} away at {step} ({err}); asking for a sign-in. {advice}"
        ),
    }
}

#[cfg(test)]
#[path = "smb_connect_failure_test.rs"]
mod tests;
