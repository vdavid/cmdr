//! Telling a server that went SILENT from one that is only slow.
//!
//! HTTP has no keepalive, and a black hole (a NAS asleep, Wi-Fi gone, a VPN
//! dropped) closes nothing, so a request on one waits for its own budget and
//! then answers a timeout that says nothing about the server. A timeout can't
//! be the signal either: a huge listing on a slow NAS times out just the same.
//!
//! So the client watches for silence instead. Every byte the server sends (a
//! response's headers, a body chunk) and every upload piece the socket takes
//! is [`Liveness::heard`]. While a request is waiting and the server has been
//! quiet for [`Timings::quiet`], the watchdog asks the server something cheap
//! on a FRESH connection. An answer, any answer, means the server is busy and
//! the wait goes on; [`Timings::unanswered_limit`] probes in a row going
//! unanswered means it's gone, and [`Liveness::declare_lost`] cuts every
//! waiting operation with the typed disconnected error.
//!
//! ❗ Nothing here names HTTP: the probe is a closure, so the ladder is tested
//! on a paused clock with no server at all (`liveness_test.rs`). The HTTP
//! backends (`cmdr-webdav`, `cmdr-s3`) share it, each naming itself in the one
//! log line through `watch`'s `backend`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;
use log::warn;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// How long silence lasts before the watchdog asks, and how long it waits for
/// the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timings {
    /// Silence from the server, with a request waiting, before a probe goes out.
    pub quiet: Duration,
    /// How long one probe may take to be answered.
    pub probe_budget: Duration,
    /// Unanswered probes in a row that make the server gone.
    pub unanswered_limit: u32,
}

impl Timings {
    /// 10 s of silence, then two probes of 10 s each: gone after 30 s, the
    /// silence `cmdr-smb` and `cmdr-sftp` allow too.
    pub const PRODUCTION: Self = Self {
        quiet: Duration::from_secs(10),
        probe_budget: Duration::from_secs(10),
        unanswered_limit: 2,
    };
}

/// One client's evidence that its server is still there.
pub struct Liveness {
    /// When the server last sent a byte, or took one.
    last_heard: Mutex<Instant>,
    /// Operations waiting on the server right now.
    in_flight: AtomicUsize,
    /// Whether a watchdog is running (or about to be) for them.
    watching: AtomicBool,
    /// Cancelled once, when the server is gone. Every waiting operation races
    /// it; the client it belongs to is never used again.
    lost: CancellationToken,
}

impl Liveness {
    /// A fresh watch: nothing waiting, the server last heard from now.
    pub fn new() -> Self {
        Self {
            last_heard: Mutex::new(Instant::now()),
            in_flight: AtomicUsize::new(0),
            watching: AtomicBool::new(false),
            lost: CancellationToken::new(),
        }
    }

    /// The server just proved it's there.
    pub fn heard(&self) {
        *self.last_heard.lock_ignore_poison() = Instant::now();
    }

    /// Cancelled when the server is gone.
    pub fn lost(&self) -> &CancellationToken {
        &self.lost
    }

    /// The server is gone: every waiting operation answers now.
    pub fn declare_lost(&self) {
        self.lost.cancel();
    }

    /// An operation starts waiting on the server. Hold the answer for as long
    /// as it waits, and start [`watch`] when it says so.
    ///
    /// ❗ The first waiter restarts the silence clock: time nobody was asking
    /// anything is not the server being quiet.
    pub fn begin(self: &Arc<Self>) -> Waiting {
        if self.in_flight.fetch_add(1, Ordering::SeqCst) == 0 {
            self.heard();
        }
        let needs_a_watch = !self.watching.swap(true, Ordering::SeqCst);
        Waiting {
            liveness: Arc::clone(self),
            needs_a_watch,
        }
    }

    fn quiet_since(&self, asked: Instant) -> bool {
        *self.last_heard.lock_ignore_poison() < asked
    }

    fn quiet_for(&self) -> Duration {
        self.last_heard.lock_ignore_poison().elapsed()
    }

    /// Whether the watchdog has anything left to watch over.
    ///
    /// ❗ The re-check after letting go is what keeps a waiter that arrived in
    /// between from going unwatched: it saw `watching` still set and started
    /// nothing, so this watchdog takes it on.
    fn keep_watching(&self) -> bool {
        if self.lost.is_cancelled() {
            return false;
        }
        if self.in_flight.load(Ordering::SeqCst) > 0 {
            return true;
        }
        self.watching.store(false, Ordering::SeqCst);
        self.in_flight.load(Ordering::SeqCst) > 0 && !self.watching.swap(true, Ordering::SeqCst)
    }
}

impl Default for Liveness {
    fn default() -> Self {
        Self::new()
    }
}

/// One operation waiting on the server. Dropping it ends the wait.
pub struct Waiting {
    liveness: Arc<Liveness>,
    needs_a_watch: bool,
}

impl Waiting {
    /// Whether nobody is watching yet, so the caller has to start [`watch`].
    pub fn needs_a_watch(&self) -> bool {
        self.needs_a_watch
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.liveness.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Watches over `liveness` while anything waits on it, and declares the
/// server lost when `probe` goes unanswered `unanswered_limit` times in a row.
///
/// `probe` answers whether the server responded at all; its budget is applied
/// here. `backend` names the backend in the log line (`webdav`, `s3`). Ends
/// when nothing waits any more, or once the server is lost.
pub async fn watch<P, F>(liveness: Arc<Liveness>, timings: Timings, backend: &'static str, mut probe: P)
where
    P: FnMut() -> F,
    F: Future<Output = bool>,
{
    let mut unanswered = 0;
    while liveness.keep_watching() {
        let quiet_for = liveness.quiet_for();
        if unanswered == 0 && quiet_for < timings.quiet {
            tokio::select! {
                () = tokio::time::sleep(timings.quiet - quiet_for) => {}
                () = liveness.lost.cancelled() => {}
            }
            continue;
        }
        let asked = Instant::now();
        let answered = tokio::select! {
            answered = tokio::time::timeout(timings.probe_budget, probe()) => answered.unwrap_or(false),
            // Somebody else found the server gone; the loop's head ends the watch.
            () = liveness.lost.cancelled() => continue,
        };
        if answered {
            liveness.heard();
        }
        // ❗ A byte on any waiting request while the probe was out is as good
        // as an answer.
        if answered || !liveness.quiet_since(asked) {
            unanswered = 0;
            continue;
        }
        unanswered += 1;
        if unanswered >= timings.unanswered_limit {
            warn!(
                target: "volume",
                "{backend} server silent for {:?} with requests waiting, and {} went unanswered: treating it as gone",
                liveness.quiet_for(),
                pluralize(u64::from(unanswered), "probe")
            );
            liveness.declare_lost();
        }
    }
}

#[cfg(test)]
#[path = "liveness_test.rs"]
mod liveness_test;
