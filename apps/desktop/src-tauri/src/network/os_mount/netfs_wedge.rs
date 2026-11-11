//! Telling a wedged `NetAuthSysAgent` from a slow server, so a mount Cmdr gave up
//! on says what's really wrong (#604).
//!
//! macOS's network sign-in helper can get stuck waiting on an LSARPC reply that
//! never comes, and from then on every `NetFSMountURLSync` blocks inside it, for
//! every server. Only restarting the helper clears it (`killall NetAuthSysAgent`;
//! launchd starts it again on the next mount), and ❌ Cmdr never does that itself:
//! it tells the user how. The mechanism and how it was confirmed: `network/DETAILS.md` §
//! "A wedged `NetAuthSysAgent` hangs every NetFS mount".
//!
//! Two jobs:
//!
//! - **Reading a mount Cmdr gave up on** ([`read_given_up`]): one that waited at
//!   least [`SUSPECT_AFTER`] on a server whose SMB port accepts a connection is
//!   the helper, ❌ not the server, so it answers `SignInHelperStuck`.
//! - **Not piling on** ([`CallLedger`], [`refusal_while_stuck`]): the blocked
//!   thread stays inside NetFS after the mount gives up on it. While such a call
//!   is still stuck, a new mount gets the same answer right away instead of
//!   queueing another call on the wedged daemon. It stops as soon as the stuck
//!   call comes back, or the helper running now isn't the one it got stuck in.

use super::MountError;
use super::platform::CAN_WEDGE;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a mount must have waited before giving up on it can say anything
/// about the helper. A healthy NetFS mount takes a second or two (1.5 s right
/// after a restart, in the `DETAILS.md` evidence), so a cancel sooner than this
/// is someone changing their mind.
pub(crate) const SUSPECT_AFTER: Duration = Duration::from_secs(10);

/// How long the server gets to accept a TCP connection on its SMB port. A LAN
/// server answers in milliseconds; this only bounds how long Cancel can take.
const PROBE_LIMIT: Duration = Duration::from_secs(2);

/// One NetFS call's ticket in a [`CallLedger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CallId(u64);

/// A call a mount gave up on while the server answered.
struct StuckCall {
    since: Instant,
    /// The helper's pid when the call was found stuck, `None` when it couldn't be read.
    helper: Option<u32>,
}

#[derive(Default)]
struct Calls {
    next_id: u64,
    in_flight: BTreeSet<u64>,
    stuck: BTreeMap<u64, StuckCall>,
}

/// The NetFS calls still running, and which of them are stuck in the helper.
///
/// ❗ Process-wide in production ([`NETFS_CALLS`]) because the wedge is: one stuck
/// helper hangs every server's mount. Tests hold their own, so a stuck call one
/// files can't refuse another's mounts.
pub(crate) struct CallLedger {
    calls: Mutex<Calls>,
}

/// Every NetFS call this process has out.
pub(crate) static NETFS_CALLS: CallLedger = CallLedger::new();

impl CallLedger {
    pub(crate) const fn new() -> Self {
        Self {
            calls: Mutex::new(Calls {
                next_id: 0,
                in_flight: BTreeSet::new(),
                stuck: BTreeMap::new(),
            }),
        }
    }

    /// Files a NetFS call as running. ❗ Move the guard INTO the blocking closure,
    /// so it drops when NetFS lets the thread go, ❌ not when the mount gives up.
    pub(crate) fn start(&'static self) -> InFlightCall {
        let mut calls = self.calls.lock_ignore_poison();
        let id = calls.next_id;
        calls.next_id += 1;
        calls.in_flight.insert(id);
        InFlightCall { ledger: self, id }
    }

    /// Marks a call stuck in the helper running as `helper`. Answers `false` for a
    /// call that already came back, which isn't stuck whatever the server says.
    pub(crate) fn mark_stuck(&self, call: CallId, helper: Option<u32>) -> bool {
        let mut calls = self.calls.lock_ignore_poison();
        if !calls.in_flight.contains(&call.0) {
            return false;
        }
        calls.stuck.insert(
            call.0,
            StuckCall {
                since: Instant::now(),
                helper,
            },
        );
        true
    }

    /// How long the oldest call still stuck in the helper has been stuck, or `None`
    /// when no call is. `helper_now` is the helper's pid right now.
    ///
    /// A call found stuck in a helper that has since been restarted (a different
    /// pid, or none running) is forgotten for good: it says nothing about the new
    /// one, even if NetFS never lets its thread go. One whose helper pid couldn't
    /// be read keeps blocking until it comes back.
    pub(crate) fn stuck_call(&self, helper_now: Option<u32>) -> Option<Duration> {
        let mut calls = self.calls.lock_ignore_poison();
        calls.stuck.retain(|_, stuck| match stuck.helper {
            Some(then) => helper_now == Some(then),
            None => true,
        });
        calls.stuck.values().map(|stuck| stuck.since.elapsed()).max()
    }

    fn finish(&self, id: u64) {
        let mut calls = self.calls.lock_ignore_poison();
        calls.in_flight.remove(&id);
        if let Some(stuck) = calls.stuck.remove(&id) {
            // The evidence the next wedge report needs: did the stuck call ever come back, and when.
            log::info!(
                target: "network",
                "A NetFS mount found stuck came back {} s later, so new mounts go out again",
                stuck.since.elapsed().as_secs()
            );
        }
    }
}

/// One NetFS call still running. Dropping it files the call as back.
pub(crate) struct InFlightCall {
    ledger: &'static CallLedger,
    id: u64,
}

impl InFlightCall {
    pub(crate) fn id(&self) -> CallId {
        CallId(self.id)
    }
}

impl Drop for InFlightCall {
    fn drop(&mut self) {
        self.ledger.finish(self.id);
    }
}

/// The mount a verdict is about.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GivenUpMount<'a> {
    pub(crate) server: &'a str,
    pub(crate) share: &'a str,
    pub(crate) port: u16,
}

/// Why a mount stopped waiting on its NetFS call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GaveUpWhy {
    /// The mount's budget ran out.
    TimedOut,
    /// The user called it off.
    Cancelled,
}

/// The answer a new mount to `server` gets without calling NetFS, while an
/// earlier call is still stuck in the helper running as `helper_now`.
pub(crate) fn refusal_while_stuck(ledger: &CallLedger, helper_now: Option<u32>, server: &str) -> Option<MountError> {
    if !CAN_WEDGE {
        return None;
    }
    let stuck_for = ledger.stuck_call(helper_now)?;
    log::warn!(
        target: "network",
        "Not mounting on server={:?}: an earlier NetFS mount has been stuck in NetAuthSysAgent for {} s, and another would queue behind it",
        LogPrivate::host(server),
        stuck_for.as_secs()
    );
    Some(MountError::SignInHelperStuck {
        server: server.to_string(),
    })
}

/// What a mount that stopped waiting on `call` after `waited` answers.
///
/// `SignInHelperStuck` when it waited at least [`SUSPECT_AFTER`], the server's SMB
/// port accepts a connection, and the call is still running (filed stuck in
/// `ledger` under `helper_now`'s pid). Otherwise the plain answer: `Timeout` or
/// `Cancelled`.
pub(crate) async fn read_given_up(
    ledger: &CallLedger,
    call: CallId,
    why: GaveUpWhy,
    waited: Duration,
    mount: GivenUpMount<'_>,
    helper_now: impl FnOnce() -> Option<u32>,
) -> MountError {
    if CAN_WEDGE
        && waited >= SUSPECT_AFTER
        && server_answers(mount.server, mount.port).await
        && ledger.mark_stuck(call, helper_now())
    {
        log::warn!(
            target: "network",
            "Mounting share={:?} on server={:?} stopped after {} s ({why:?}) while the server answers on port {}, so NetAuthSysAgent looks wedged",
            LogPrivate::volume(mount.share),
            LogPrivate::host(mount.server),
            waited.as_secs(),
            mount.port
        );
        return MountError::SignInHelperStuck {
            server: mount.server.to_string(),
        };
    }
    match why {
        GaveUpWhy::TimedOut => MountError::Timeout {
            server: mount.server.to_string(),
        },
        GaveUpWhy::Cancelled => MountError::Cancelled {
            share: mount.share.to_string(),
        },
    }
}

/// Whether `server:port` accepts a TCP connection within [`PROBE_LIMIT`].
async fn server_answers(server: &str, port: u16) -> bool {
    matches!(
        tokio::time::timeout(PROBE_LIMIT, tokio::net::TcpStream::connect((server, port))).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
#[path = "netfs_wedge_test.rs"]
mod tests;
