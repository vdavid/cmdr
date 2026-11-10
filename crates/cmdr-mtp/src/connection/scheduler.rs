//! Foreground-priority device scheduler for MTP.
//!
//! PTP is protocol-serial: one bulk transaction at a time, which `mtp-rs` already
//! enforces with its internal per-transaction `operation_lock`. What `mtp-rs`
//! does NOT decide is Cmdr POLICY: when two Cmdr operations both want the device,
//! which goes first. Without a policy, a background index scan that holds the
//! per-device lock across a whole 9,000-file directory enumeration starves every
//! user-initiated foreground op (nav, copy, delete) for tens of seconds.
//!
//! This module is the policy: a per-device [`DevicePriorityGate`] that classifies
//! access into two priorities and guarantees foreground preempts background.
//!
//! ## The contract
//!
//! - **Foreground** (user nav / copy / delete / rename / move, plus resolving the
//!   CURRENT/visible folder): takes a [`ForegroundGuard`] before touching the
//!   device. The guard only COUNTS the op as foreground-pending; it does not by
//!   itself acquire the device lock (the existing `acquire_device_lock` still
//!   does that). A foreground op never waits on the gate — it only raises the
//!   pending count and contends for the device lock, which the scan releases at
//!   every unit boundary.
//! - **Background** (the index scan, resolving non-visible changes): between
//!   small units of work, calls [`DevicePriorityGate::background_yield_point`],
//!   which parks while any foreground op is pending and resumes the instant the
//!   last one drops its guard.
//!
//! ## Why this is deadlock-free and always progresses
//!
//! The gate's state (`foreground_pending` + a `Notify`) is touched WITHOUT holding
//! the device lock, and the device lock is the only OS lock — it's always released
//! at a unit boundary. So there's no lock-ordering cycle. Background always makes
//! progress when idle (`background_yield_point` returns immediately at zero
//! pending), and every parked scan unit is woken because the last foreground guard
//! drop decrements to zero and notifies them all. We re-read the counter after every wake
//! (`while` loop), so a stale or spurious wake just re-parks rather than
//! proceeding wrongly. Design history is in git (former
//! `docs/specs/mtp-device-scheduler-plan.md`).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Notify;

/// Per-device foreground/background access arbiter. Cheap to clone (`Arc` inside).
///
/// Owned per connected device by the `MtpConnectionManager` (one gate per
/// `device_id`). Pure logic: holds no device handle and does no I/O, so its
/// ordering is unit-testable with synthetic counters.
#[derive(Clone, Default)]
pub(super) struct DevicePriorityGate {
    inner: Arc<GateInner>,
}

#[derive(Default)]
struct GateInner {
    /// Count of foreground ops currently entered (waiting for, or holding, the
    /// device). A background unit yields while this is non-zero.
    foreground_pending: AtomicUsize,
    /// Woken when `foreground_pending` drops to zero, so every parked background
    /// unit re-checks and proceeds.
    drained: Notify,
    /// Count of read sessions that own the device lock BETWEEN calls (a
    /// held-open transfer in flight). While non-zero, a pending foreground op is
    /// waiting on that read, so the read must not yield to it.
    reads_holding_device: AtomicUsize,
}

impl DevicePriorityGate {
    /// Enter a foreground op: increments the pending count for the lifetime of the
    /// returned guard. Take this BEFORE acquiring the device lock on every
    /// foreground device op, so the background scan yields to it.
    pub(super) fn foreground_guard(&self) -> ForegroundGuard {
        self.inner.foreground_pending.fetch_add(1, Ordering::SeqCst);
        ForegroundGuard {
            inner: Arc::clone(&self.inner),
        }
    }

    /// `true` when at least one foreground op is currently pending.
    ///
    /// The index scan reads this same predicate at its yield boundary (via
    /// `background_yield_point`, which reads the counter inline); a running
    /// transfer asks [`transfer_should_yield`](Self::transfer_should_yield),
    /// which adds the held-open exception. Cheap (an atomic load), so it's safe
    /// to poll once per chunk.
    pub(super) fn foreground_pending(&self) -> bool {
        self.inner.foreground_pending.load(Ordering::SeqCst) > 0
    }

    /// Marks a read session as owning the device lock across calls, for the
    /// lifetime of the returned hold. Taken only while mtp-rs's download holds
    /// the PTP session (`FileDownload::holds_session`).
    pub(super) fn read_holding_device(&self) -> ReadHold {
        self.inner.reads_holding_device.fetch_add(1, Ordering::SeqCst);
        ReadHold {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Whether a running transfer should stop at its checkpoint for foreground
    /// work: a foreground op pends AND no read owns the device lock between
    /// calls.
    ///
    /// The second half is what keeps a held-open transfer from deadlocking. The
    /// foreground op it would yield to is queued on the device lock that very
    /// transfer holds, so parking it parks the op too, until the 300 s lock wait
    /// fails the op. Between windows nothing is held, so the yield works as
    /// before.
    pub(super) fn transfer_should_yield(&self) -> bool {
        self.foreground_pending() && self.inner.reads_holding_device.load(Ordering::SeqCst) == 0
    }

    /// Background yield point: park while any foreground op is pending, returning
    /// once the device is clear of foreground work. Call between small units of
    /// background work (each scan metadata batch, the scan's GetObjectHandles
    /// step). Returns immediately when no foreground op is pending, so an idle
    /// scan never stalls.
    ///
    /// Lost-wakeup-free by construction, for any number of parked units. A walk keeps
    /// many listings in flight and each parks here, so the drain wakes them ALL
    /// (`Notify::notify_waiters`); ❌ never `notify_one`, which frees one and leaves the
    /// rest asleep until the next foreground op. `notify_waiters` stores no permit, so a
    /// unit registers (`enable`) BEFORE reading the counter: a drain after the read
    /// reaches the registered future, and a drain before it shows up as zero. We re-read
    /// the counter after every wake, so a stale one just re-parks.
    pub(super) async fn background_yield_point(&self) {
        loop {
            let mut drained = std::pin::pin!(self.inner.drained.notified());
            drained.as_mut().enable();
            if self.inner.foreground_pending.load(Ordering::SeqCst) == 0 {
                return;
            }
            drained.await;
        }
    }
}

/// RAII guard marking a foreground op as pending. Dropping it decrements the
/// count and, when it reaches zero, wakes any parked background unit.
pub(super) struct ForegroundGuard {
    inner: Arc<GateInner>,
}

impl Drop for ForegroundGuard {
    fn drop(&mut self) {
        // `fetch_sub` returns the PREVIOUS value; we hit zero when it was 1.
        if self.inner.foreground_pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.inner.drained.notify_waiters();
        }
    }
}

/// RAII mark of a read session owning the device lock between calls. See
/// [`DevicePriorityGate::transfer_should_yield`].
pub(super) struct ReadHold {
    inner: Arc<GateInner>,
}

impl Drop for ReadHold {
    fn drop(&mut self) {
        self.inner.reads_holding_device.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::DevicePriorityGate;
    use std::time::Duration;

    #[test]
    fn no_pending_by_default() {
        let gate = DevicePriorityGate::default();
        assert!(!gate.foreground_pending());
    }

    #[test]
    fn a_transfer_never_yields_to_the_op_waiting_on_its_own_held_transfer() {
        let gate = DevicePriorityGate::default();
        let fg = gate.foreground_guard();
        assert!(gate.transfer_should_yield(), "between windows ⇒ yield");
        let hold = gate.read_holding_device();
        assert!(
            !gate.transfer_should_yield(),
            "the pending op waits on this read's lock ⇒ yielding would park both"
        );
        drop(hold);
        assert!(gate.transfer_should_yield(), "hold released ⇒ yield again");
        drop(fg);
        assert!(!gate.transfer_should_yield(), "nothing pending ⇒ no yield");
    }

    #[test]
    fn guard_raises_and_drops_pending_count() {
        let gate = DevicePriorityGate::default();
        assert!(!gate.foreground_pending());
        let g1 = gate.foreground_guard();
        assert!(gate.foreground_pending(), "one guard ⇒ pending");
        let g2 = gate.foreground_guard();
        assert!(gate.foreground_pending(), "two guards ⇒ still pending");
        drop(g1);
        assert!(gate.foreground_pending(), "one guard remains ⇒ still pending");
        drop(g2);
        assert!(!gate.foreground_pending(), "all guards dropped ⇒ clear");
    }

    #[tokio::test]
    async fn yield_point_returns_immediately_when_idle() {
        let gate = DevicePriorityGate::default();
        // No foreground pending: must not park. Wrap in a tight timeout so a
        // regression (parking forever) fails fast rather than hanging the suite.
        tokio::time::timeout(Duration::from_millis(500), gate.background_yield_point())
            .await
            .expect("idle yield point must return immediately");
    }

    #[tokio::test]
    async fn yield_point_parks_until_foreground_drains() {
        let gate = DevicePriorityGate::default();
        let guard = gate.foreground_guard();

        // Spawn the background waiter; it must NOT complete while the guard is held.
        let g2 = gate.clone();
        let waiter = tokio::spawn(async move {
            g2.background_yield_point().await;
        });

        // allowed-test-sleep: negative assertion. The waiter must still be parked, so the window is
        // the evidence; a "still blocked" state has no event to wait on
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiter.is_finished(), "background must park while foreground pends");

        // Drop the foreground guard: the waiter must wake and finish promptly.
        drop(guard);
        tokio::time::timeout(Duration::from_millis(500), waiter)
            .await
            .expect("waiter must wake within timeout after foreground drains")
            .expect("waiter task must not panic");
    }

    #[tokio::test]
    async fn yield_point_resumes_after_multiple_foreground_ops() {
        let gate = DevicePriorityGate::default();
        let g_a = gate.foreground_guard();
        let g_b = gate.foreground_guard();

        let g2 = gate.clone();
        let waiter = tokio::spawn(async move {
            g2.background_yield_point().await;
        });

        // allowed-test-sleep: lets the waiter park before the first guard drops, so the second
        // window below is measuring the remaining guard rather than a waiter that never started
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(g_a);
        // allowed-test-sleep: negative assertion. One guard is still held, so the waiter must stay
        // parked, and the window is the only evidence of that
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiter.is_finished(), "still parked while the second op pends");

        drop(g_b);
        tokio::time::timeout(Duration::from_millis(500), waiter)
            .await
            .expect("waiter wakes once the LAST foreground op drains")
            .expect("waiter task must not panic");
    }

    /// A walk keeps many listings in flight, and each parks here on its own. If the drain
    /// woke just one, the rest slept until the next foreground op, which on an idle pane is
    /// never (Pixel 9 Pro XL, 2026-10-30: the walk froze the moment browsing stopped).
    #[tokio::test]
    async fn every_parked_unit_wakes_when_foreground_drains() {
        let gate = DevicePriorityGate::default();
        let guard = gate.foreground_guard();

        let waiters: Vec<_> = (0..8)
            .map(|_| {
                let gate = gate.clone();
                tokio::spawn(async move { gate.background_yield_point().await })
            })
            .collect();

        // allowed-test-sleep: lets every waiter park before the drain; a wake that lands first
        // would pass without proving anything about parked waiters
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(guard);

        for waiter in waiters {
            tokio::time::timeout(Duration::from_millis(500), waiter)
                .await
                .expect("every parked unit wakes once the foreground drains")
                .expect("waiter task must not panic");
        }
    }

    #[tokio::test]
    async fn foreground_op_that_starts_and_ends_does_not_lose_a_later_wake() {
        // A foreground op that comes and goes while the scan is between yield
        // points must not leave the scan permanently parked on the next yield.
        let gate = DevicePriorityGate::default();

        // Op comes and goes entirely before any background parks.
        drop(gate.foreground_guard());

        // Now the scan yields: nothing pending, must return immediately.
        tokio::time::timeout(Duration::from_millis(500), gate.background_yield_point())
            .await
            .expect("a settled foreground op must leave the gate clear");
    }
}
