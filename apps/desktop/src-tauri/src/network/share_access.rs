//! A second opinion on a mount that found no share: the server's own.
//!
//! The kernel mount can't tell two different answers apart. NetFS reports `ENOENT`
//! both for a share that doesn't exist and for one that exists but won't let this
//! identity open it, so a share guests can LIST but not OPEN read as "not found",
//! and the frontend showed a dead end where it should have asked for a sign-in
//! (ERR-SHUSC). The server itself answers the question precisely, so when a mount
//! comes back `ShareNotFound` we ask it: one short smb2 session with the same
//! identity, one TreeConnect, and read which status came back at which command.
//!
//! Two pure halves, so every case is a unit test: [`verdict_from`] reads the smb2
//! answer by TYPE (❌ never by message text), and [`clarified`] decides what that
//! means for the identity the mount used. Evidence and the decision table:
//! `DETAILS.md` § "A share that says not found".

use super::mount::MountError;
use cmdr_smb::volume::SmbConnectionParams;
use smb2::ErrorKind;
use smb2::types::Command;
use std::time::Duration;

/// The longest the probe may take, however much of the mount's budget is left.
///
/// A healthy server answers all three steps in well under a second on a LAN. The
/// number only decides how long a server that went quiet between the mount and
/// the probe gets to look alive before the mount's own answer stands.
const PROBE_LIMIT: Duration = Duration::from_secs(5);

/// Who a mount went out as, which decides what a refusal means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignInIdentity {
    /// No username: NetFS mounted with `Guest = true`, and the probe goes out as
    /// `Guest`, which is `SmbConnectionParams::new`'s default.
    Guest,
    /// A username and password the user offered.
    Account,
}

/// One mount attempt, as the probe needs to repeat it.
///
/// ❌ No `Debug`: it carries the password.
pub(crate) struct ShareAttempt {
    /// NFC-folded by `SmbConnectionParams::new`, so the probe asks for the share the
    /// way the mount URL spells it.
    params: SmbConnectionParams,
    identity: SignInIdentity,
}

impl ShareAttempt {
    pub(crate) fn new(server: &str, share: &str, port: u16, username: Option<&str>, password: Option<&str>) -> Self {
        let identity = match username {
            Some(_) => SignInIdentity::Account,
            None => SignInIdentity::Guest,
        };
        Self {
            params: SmbConnectionParams::new(server, share, port, username, password),
            identity,
        }
    }

    /// The server as the attempt names it, for a message about the whole mount.
    pub(crate) fn server(&self) -> &str {
        &self.params.server
    }
}

/// What the server said when asked directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareVerdict {
    /// TreeConnect went through: this identity may open the share.
    Opens,
    /// The session came up, and TreeConnect answered access denied.
    RefusedAtShare,
    /// Session setup itself turned this identity away, before any share was named.
    RefusedAtSignIn,
    /// TreeConnect answered that no share goes by that name.
    NoSuchShare,
    /// Anything else: unreachable, timed out, or an answer this doesn't read.
    Unknown,
}

/// Reads the probe's outcome by status and command.
pub(crate) fn verdict_from(outcome: &Result<(), smb2::Error>) -> ShareVerdict {
    let error = match outcome {
        Ok(()) => return ShareVerdict::Opens,
        Err(error) => error,
    };
    if matches!(
        error,
        smb2::Error::Protocol {
            command: Command::TreeConnect,
            ..
        }
    ) {
        return match error.kind() {
            ErrorKind::AccessDenied => ShareVerdict::RefusedAtShare,
            ErrorKind::NotFound => ShareVerdict::NoSuchShare,
            _ => ShareVerdict::Unknown,
        };
    }
    // Not a TreeConnect answer, so an auth-class one came from the sign-in step:
    // the logon-rejection family, access denied, or a server that wants signing a
    // guest session can't give.
    if cmdr_smb::is_auth_error(error) {
        ShareVerdict::RefusedAtSignIn
    } else {
        ShareVerdict::Unknown
    }
}

/// What a mount's "not found" really was, given what the server said.
///
/// - A GUEST turned away (at the share or at sign-in) is a credential question:
///   `AuthRequired`, which opens the sign-in sheet.
/// - An ACCOUNT turned away at the share signed in fine and isn't allowed in:
///   `PermissionDenied`, so the sheet asks for a different account instead of
///   blaming the password.
/// - An ACCOUNT the probe couldn't sign in keeps the mount's answer. NetFS answers
///   a wrong password with an auth code, never `ENOENT`, so a refusal here says
///   more about how the probe spelled the account than about the share.
/// - Everything else keeps the mount's answer: the share really is missing, the
///   probe got in (so the "not found" is something this can't explain), or the
///   server didn't answer.
pub(crate) fn clarified(original: MountError, attempt: &ShareAttempt, verdict: ShareVerdict) -> MountError {
    let share = &attempt.params.share_name;
    let server = &attempt.params.server;
    match (attempt.identity, verdict) {
        (SignInIdentity::Guest, ShareVerdict::RefusedAtShare | ShareVerdict::RefusedAtSignIn) => {
            MountError::AuthRequired {
                message: format!("\"{server}\" doesn't let guests open \"{share}\". Sign in to connect."),
            }
        }
        (SignInIdentity::Account, ShareVerdict::RefusedAtShare) => MountError::PermissionDenied {
            message: format!(
                "\"{}\" signed in to \"{server}\" but isn't allowed to open \"{share}\"",
                attempt.params.username
            ),
        },
        (SignInIdentity::Account, ShareVerdict::RefusedAtSignIn)
        | (_, ShareVerdict::Opens | ShareVerdict::NoSuchShare | ShareVerdict::Unknown) => original,
    }
}

/// Asks the server why a mount found no share, within `budget`, and answers with
/// the error the mount should report.
///
/// A budget that's already spent keeps the mount's answer without dialing: the
/// probe runs INSIDE the mount's timeout, never after it.
pub(crate) async fn clarify_share_not_found(
    original: MountError,
    attempt: &ShareAttempt,
    budget: Duration,
) -> MountError {
    let limit = budget.min(PROBE_LIMIT);
    if limit.is_zero() {
        log::info!(
            "NetFS found no share \"{}\" on {}, and the mount's budget is spent, so the server isn't asked why",
            attempt.params.share_name,
            attempt.params.server
        );
        return original;
    }

    // Bounding the future is safe here, unlike around `spawn_blocking`: dropping it
    // drops the probe's own socket, and nothing else shares that session.
    let outcome = tokio::time::timeout(limit, cmdr_smb::try_open_share(&attempt.params, limit))
        .await
        .unwrap_or(Err(smb2::Error::Timeout));
    let verdict = verdict_from(&outcome);
    let answer = match &outcome {
        Ok(()) => "opened".to_string(),
        Err(error) => error.to_string(),
    };
    log::info!(
        "NetFS found no share \"{}\" on {} as {:?}; asked directly, the server answered {answer} ({verdict:?})",
        attempt.params.share_name,
        attempt.params.server,
        attempt.identity,
    );
    clarified(original, attempt, verdict)
}

#[cfg(test)]
#[path = "share_access_test.rs"]
mod tests;
