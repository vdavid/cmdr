//! The cooperative stop a copy scan honors: Cancel, and the pause that rides
//! along with it.
//!
//! A scan of a cold NAS share or a phone over USB is the minutes-long part of a
//! transfer, so it is where somebody presses Cancel. Every backend's walk has to
//! be able to answer that, and the only thing every backend shares is this
//! crate: `crates/` carry no `tauri` and know nothing about a write operation
//! (`index-crate-isolation` enforces it). So the vocabulary lives here and the
//! owner of a scan implements [`ScanStopSignal`] above it.
//!
//! ❗ A walk doesn't reach for this directly. It threads a
//! [`ScanBoundary`](super::ScanBoundary), which carries the stop next to the
//! counts and turns "report an entry" and "may I keep going?" into one call.
//! [`ScanStop`] is what a walk running inside `spawn_blocking` reaches for,
//! since it has no `.await` to park on.

use std::pin::Pin;
use std::sync::Arc;

/// What a scan's owner offers a walk: the two questions a stop boundary asks.
///
/// One implementor per owner, not per backend. In the app that's a write
/// operation's cancel intent plus its pause gate; a test owner is a pair of
/// flags. A `Volume` backend never writes one.
pub trait ScanStopSignal: Send + Sync {
    /// Cheap: `true` when this boundary must consult
    /// [`stop_or_park`](Self::stop_or_park).
    ///
    /// ❗ A live, unpaused owner has to answer `false` in an atomic load or two.
    /// That is the whole reason the boundary can sit on the per-entry path, so an
    /// implementation that took a lock or looked something up here would put the
    /// cost of stopping on every entry of every scan.
    fn is_stopping_or_paused(&self) -> bool;

    /// The boundary itself: `true` means stop now, `false` means carry on with
    /// the next entry. Parks the calling task while the owner is paused.
    ///
    /// Reached only when [`is_stopping_or_paused`](Self::is_stopping_or_paused)
    /// said yes, so an implementation may be as expensive as parking. It owes the
    /// whole ordering: **stop outranks pause** (a stopping scan never parks), and
    /// the stop is **re-read after waking** (a cancel that lands WHILE parked is
    /// answered at this boundary, not one entry later).
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn stop_or_park<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

    /// The same answer, parking the calling THREAD instead of the calling task.
    ///
    /// For a walk that runs inside `spawn_blocking` and so has no `.await` to
    /// park on: the local backend's `WalkDir` loop is the one in the tree.
    fn stop_or_park_blocking(&self) -> bool;
}

/// A scan's stop signal, or nothing at all.
///
/// Held in an `Arc` rather than borrowed so a backend that walks inside
/// `spawn_blocking` can carry one into the closure. The clone is paid once per
/// scan call, never per entry.
// DEFAULT-OK: the empty value is a scan nobody can stop, which is what a walk
// with no owner (a single-path `scan_for_copy`, a test fixture) genuinely has.
#[derive(Clone, Default)]
pub struct ScanStop(Option<Arc<dyn ScanStopSignal>>);

impl ScanStop {
    /// Nothing can stop this scan.
    pub fn none() -> Self {
        Self(None)
    }

    /// The scan answers to `signal`.
    pub fn new(signal: Arc<dyn ScanStopSignal>) -> Self {
        Self(Some(signal))
    }

    /// `true` when this scan answers to somebody. For a backend deciding whether
    /// a per-entry boundary is worth threading into an inner loop at all.
    pub fn is_armed(&self) -> bool {
        self.0.is_some()
    }

    /// The boundary: `true` means stop now. Parks the calling task while the
    /// owner is paused.
    ///
    /// One branch when nothing can stop the scan; one branch plus the owner's
    /// cheap check when something can.
    pub async fn should_stop(&self) -> bool {
        let Some(signal) = &self.0 else { return false };
        if !signal.is_stopping_or_paused() {
            return false;
        }
        signal.stop_or_park().await
    }

    /// [`should_stop`](Self::should_stop) for a walk on a blocking thread: same
    /// answer, parking the thread rather than the task.
    pub fn should_stop_blocking(&self) -> bool {
        let Some(signal) = &self.0 else { return false };
        if !signal.is_stopping_or_paused() {
            return false;
        }
        signal.stop_or_park_blocking()
    }
}

impl std::fmt::Debug for ScanStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The signal is a trait object with nothing printable on it; whether one
        // is armed is the only thing a debug line can honestly say.
        f.debug_tuple("ScanStop")
            .field(&if self.0.is_some() { "armed" } else { "none" })
            .finish()
    }
}

/// A stop signal a test drives by hand: `stop()` to cancel, `pause()` /
/// `resume()` to park a walk and let it go.
///
/// Behind the `testing` feature so other crates' tests can pin their backend's
/// walk against the same owner the app's real one stands in for.
// DEFAULT-OK: every field is a flag or a counter about THIS signal's own state,
// and all-zero is the truthful opening one — a live, unpaused owner that has been
// asked nothing yet. Nothing here describes a disk.
#[cfg(any(test, feature = "testing"))]
#[derive(Default)]
pub struct TestScanStop {
    stopping: std::sync::atomic::AtomicBool,
    paused: std::sync::atomic::AtomicBool,
    /// Boundaries asked so far, so a test can pin the GRANULARITY of a walk and
    /// not merely that it stops at all.
    asks: std::sync::atomic::AtomicUsize,
    notify: tokio::sync::Notify,
}

#[cfg(any(test, feature = "testing"))]
impl TestScanStop {
    /// A signal that is neither stopping nor paused.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A signal that is already stopping, for the common "does this walk honor
    /// it at all" assertion.
    pub fn already_stopping() -> Arc<Self> {
        let stop = Self::new();
        stop.stop();
        stop
    }

    /// Cancel: every boundary from here on answers "stop", parked ones included.
    pub fn stop(&self) {
        self.stopping.store(true, std::sync::atomic::Ordering::Release);
        self.notify.notify_waiters();
    }

    /// Pause: the next boundary parks until [`resume`](Self::resume) or [`stop`](Self::stop).
    pub fn pause(&self) {
        self.paused.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Lets a parked walk carry on.
    pub fn resume(&self) {
        self.paused.store(false, std::sync::atomic::Ordering::Release);
        self.notify.notify_waiters();
    }

    /// How many stop boundaries the walk has asked so far.
    pub fn asks(&self) -> usize {
        self.asks.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Whether a walk is parked on this signal right now.
    pub fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[cfg(any(test, feature = "testing"))]
impl ScanStopSignal for TestScanStop {
    fn is_stopping_or_paused(&self) -> bool {
        self.asks.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.stopping.load(std::sync::atomic::Ordering::Acquire) || self.is_paused()
    }

    fn stop_or_park<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            // Same ordering the real owner owes: stop first, then park, then
            // re-read the stop after waking.
            if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
                return true;
            }
            while self.is_paused() {
                let notified = self.notify.notified();
                if !self.is_paused() || self.stopping.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                notified.await;
            }
            self.stopping.load(std::sync::atomic::Ordering::Acquire)
        })
    }

    fn stop_or_park_blocking(&self) -> bool {
        if self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            return true;
        }
        while self.is_paused() && !self.stopping.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.stopping.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "scan_stop_test.rs"]
mod scan_stop_test;
