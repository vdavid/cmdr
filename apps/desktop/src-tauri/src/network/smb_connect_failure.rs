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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeFailure {
    /// Nothing answered on the SMB port: the server is off, asleep, or not on
    /// this network right now. A DFS namespace whose every target refused lands
    /// here too: the namespace is real, its storage isn't reachable, and both
    /// are worth another attempt once the network or the server changes.
    Unreachable,
    /// It answered, but the handshake ran out of time.
    TooSlow,
    /// The server has no share by that name, and no DFS namespace behind it
    /// either.
    ///
    /// The one failure here that **repeating cannot fix**: the same identity
    /// asking the same server for the same share gets the same answer, so a
    /// retry is not worth offering. Everything else in this enum is a condition
    /// that can change on its own.
    ShareNotOnServer,
    /// It answered and then something we can't act on went wrong.
    Unexpected,
    /// Something on this Mac refused Cmdr's own route to a server the Mac itself
    /// can reach: the macOS Local Network permission (stuck on in ERR-XGS9X, fixed
    /// by switching it off and on), or a firewall app. Read by [`Self::of_dial`],
    /// ❌ never from the errno alone.
    BlockedByThisMac,
}

/// The longest one dial attempt may take to fail and still count as this Mac
/// refusing the route.
///
/// A refusal inside this Mac never leaves it: ERR-XGS9X's came back in 1–3 ms,
/// every attempt. An `EHOSTUNREACH` from the network takes a round trip at least,
/// and usually seconds (a router answers only once its own ARP gives up). 250 ms
/// sits two orders of magnitude above the first, for a CPU-starved tokio worker,
/// and well below the second.
pub(crate) const BLOCKED_DIAL_CEILING: std::time::Duration = std::time::Duration::from_millis(250);

/// What the share's kernel mount said when the upgrade read it right before dialing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MountEvidence {
    /// Its `statfs` answered as an SMB mount under `MOUNT_READ_LIMIT`: the server
    /// is reachable from this Mac, over the kernel's own connection.
    Answered,
    /// Nothing vouches for the server: no SMB mount answered there.
    NoAnswer,
}

/// A direct connect that didn't get in, after `connect_with_retry` spent its
/// retries.
#[derive(Debug)]
pub(crate) struct FailedDial {
    /// What the last attempt failed with.
    pub(crate) error: smb2::Error,
    /// The longest any one attempt took to fail, checked against
    /// [`BLOCKED_DIAL_CEILING`].
    pub(crate) slowest_attempt: std::time::Duration,
}

/// Whether `err` is the kernel refusing a route: every address answered
/// `EHOSTUNREACH` or `ENETUNREACH`, read by io kind.
fn is_refused_route(err: &smb2::Error) -> bool {
    use std::io::ErrorKind as Io;
    let refused = |kind: Io| matches!(kind, Io::HostUnreachable | Io::NetworkUnreachable);
    match err {
        smb2::Error::Io(io_err) => refused(io_err.kind()),
        smb2::Error::ConnectFailed { attempts, .. } => {
            !attempts.is_empty() && attempts.iter().all(|a| a.error_kind.is_some_and(refused))
        }
        _ => false,
    }
}

/// The io side of a failed dial for its log line: the kind per address, and the
/// raw errno where the error still carries one.
///
/// smb2's per-address `ConnectAttempt` keeps the io kind but not the errno, so a
/// `ConnectFailed` line names kinds only (`HostUnreachable` is `EHOSTUNREACH`).
pub(crate) fn dial_detail(err: &smb2::Error) -> String {
    match err {
        smb2::Error::Io(io_err) => match io_err.raw_os_error() {
            Some(errno) => format!("io_kind={:?}, errno={errno}", io_err.kind()),
            None => format!("io_kind={:?}", io_err.kind()),
        },
        smb2::Error::ConnectFailed { attempts, .. } => {
            let kinds: Vec<String> = attempts
                .iter()
                .map(|a| {
                    a.error_kind
                        .map_or_else(|| "no answer".to_string(), |kind| format!("{kind:?}"))
                })
                .collect();
            format!("io_kind=[{}]", kinds.join(", "))
        }
        _ => "io_kind=none".to_string(),
    }
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
        // ❌ Read the kind AND the command, like `RefusedAt::of` above: the same
        // kind at another command is another thing entirely. At TreeConnect,
        // `NotFound` is `STATUS_BAD_NETWORK_NAME`, the server saying it has no
        // such share.
        //
        // Two look-alikes deliberately do NOT arrive here, both because smb2
        // gives them their own variants: a scale-out cluster's redirect reuses
        // the same status (`Error::ShareRedirected`), and a DFS namespace whose
        // targets are all down is `Error::DfsNoReachableTarget`, which
        // classifies as `ConnectionLost` and so falls through to `Unreachable`
        // below. Both would otherwise be read as "no such share", which is the
        // one thing neither of them means.
        if matches!(
            err,
            smb2::Error::Protocol {
                command: Command::TreeConnect,
                ..
            }
        ) && err.kind() == ErrorKind::NotFound
        {
            return Self::ShareNotOnServer;
        }
        match err.kind() {
            ErrorKind::TimedOut => Self::TooSlow,
            ErrorKind::ConnectionLost => Self::Unreachable,
            _ => Self::Unexpected,
        }
    }

    /// Classifies a dial whose retries are spent, with what the share's kernel
    /// mount said just before it.
    ///
    /// [`Self::BlockedByThisMac`] when ALL three hold, else [`Self::from_smb_error`]:
    /// every address refused the route (`EHOSTUNREACH` / `ENETUNREACH`), every
    /// attempt failed within [`BLOCKED_DIAL_CEILING`], and the mount answered. The
    /// errno alone can't say it: a server that's off, or a router giving up on it,
    /// answers `EHOSTUNREACH` too. The mount answering is what says the server is
    /// there, and the speed says the refusal never left this Mac.
    pub(crate) fn of_dial(dial: &FailedDial, mount: MountEvidence) -> Self {
        if mount == MountEvidence::Answered
            && dial.slowest_attempt <= BLOCKED_DIAL_CEILING
            && is_refused_route(&dial.error)
        {
            return Self::BlockedByThisMac;
        }
        Self::from_smb_error(&dial.error)
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
    /// Reads a failed dial for an attempt that went out as `username` (`None` is a
    /// guest), with what the share's kernel mount said before it.
    pub(crate) fn from_failed_dial(
        dial: &FailedDial,
        mount: MountEvidence,
        username: Option<&str>,
        display_name: String,
    ) -> Self {
        match Refusal::of(&dial.error, SignInIdentity::from_username(username)) {
            Some(refusal) => Self::Refused(refusal),
            None => Self::Network {
                reason: UpgradeFailure::of_dial(dial, mount),
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
/// separates the four refusals: see [`Refusal::advice`]. `reason` is the caller's
/// [`UpgradeFailure::of_dial`] reading, the same one it acts on, so the log can't
/// name a different one.
pub(crate) fn log_direct_connect_failure(
    server: &str,
    share: &str,
    dial: &FailedDial,
    reason: UpgradeFailure,
    outcome: DirectConnectOutcome,
    username: Option<&str>,
) {
    let err = &dial.error;
    let Some(refusal) = Refusal::of(err, SignInIdentity::from_username(username)) else {
        let io = dial_detail(err);
        let slowest = dial.slowest_attempt;
        let why = match reason {
            UpgradeFailure::BlockedByThisMac => {
                " The share's kernel mount answered, so the server is reachable from this Mac: something on it (the Local Network permission, a firewall app) refused Cmdr's own connection."
            }
            _ => "",
        };
        match outcome {
            DirectConnectOutcome::StaysOnKernelMount => log::warn!(
                target: "smb_fallback",
                "Couldn't establish an smb2 connection for server={server:?}, share={share:?} ({reason:?}, {io}, slowest_attempt={slowest:?}): {err}. Staying on the macOS kernel mount.{why}"
            ),
            DirectConnectOutcome::SurfacedToCaller => log::warn!(
                target: "smb_fallback",
                "Couldn't establish an smb2 connection for server={server:?}, share={share:?} ({reason:?}, {io}, slowest_attempt={slowest:?}): {err}.{why}"
            ),
        }
        return;
    };
    let who = username.map_or_else(|| "a guest".to_string(), |name| format!("user={name:?}"));
    let step = match refusal.at {
        RefusedAt::SignIn => "sign-in",
        RefusedAt::Share => "the share",
    };
    let advice = refusal.advice();
    match outcome {
        DirectConnectOutcome::StaysOnKernelMount => log::warn!(
            target: "smb_fallback",
            "server={server:?}, share={share:?} turned {who} away at {step} ({err}), so it stays on the macOS kernel mount: slower, and Cmdr can't manage the connection. {advice}"
        ),
        DirectConnectOutcome::SurfacedToCaller => log::info!(
            target: "smb_fallback",
            "server={server:?}, share={share:?} turned {who} away at {step} ({err}); asking for a sign-in. {advice}"
        ),
    }
}

#[cfg(test)]
#[path = "smb_connect_failure_test.rs"]
mod tests;
