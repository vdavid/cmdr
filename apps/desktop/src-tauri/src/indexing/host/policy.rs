//! "May background work run right now?" — the one question the index asks its host
//! before doing anything the user might be waiting behind.
//!
//! The host owns the priority order (user-interactive work > file transfers >
//! indexing) and the signals behind it; the index owns what to do with the answer.
//! Drive-index scanning and media enrichment both read this seam and both stand
//! aside the same way, at their own between-units boundary.
//!
//! ## The dispatch rule: one call per batch, never per entry
//!
//! [`HostPolicy::clearance`] returns a [`WorkClearance`], a plain `Copy` value with
//! no allocation and no borrow. That's deliberate: it means a caller takes **one**
//! snapshot at a batch boundary (a listing top-up, a between-images gate, a resume
//! poll) and reads it as many times as it likes, instead of paying a virtual call
//! per entry.
//!
//! ❌ **No index code may consult this seam on a per-entry path.** A scan visits
//! millions of entries; a `dyn` call per entry is a measurable cost on the hot path
//! and it defeats the point of caching a snapshot. If you find yourself wanting a
//! per-entry policy question, restructure the call to hoist it, don't add the
//! question. `scan_pace_tests::the_policy_is_consulted_per_listing_not_per_entry`
//! pins this with a counting fake over a real scan.
//!
//! ## Not here: the FDA gate
//!
//! Whether the app is still waiting on the user's Full Disk Access decision reaches
//! the index as a plain `bool` argument to `should_auto_start_indexing`, not as a
//! method here. It's asked once at startup, by a pure function, so a trait would be
//! ceremony. `DETAILS.md` § "The host policy seam".

use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The host's answer for one volume, at one moment.
///
/// `Copy` on purpose — see the dispatch rule in the module docs. Every field is a
/// decision, never a raw timestamp: the elapsed-versus-threshold rule belongs to the
/// host, which is where the clock and the signals live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkClearance {
    /// No foreground activity **anywhere** in the app for the requested idle window.
    /// The right scope for work with no deadline that competes for the whole
    /// machine, like on-device image enrichment.
    pub(crate) app_idle: bool,
    /// No foreground activity **on the volume asked about** for the requested idle
    /// window. The right scope for work that contends for one share's connection,
    /// like a network index scan: browsing a local folder is no reason to slow a NAS.
    ///
    /// A volume nobody has browsed reads as idle, so a first scan starts at full
    /// speed rather than standing aside for a navigation that never happened.
    pub(crate) volume_idle: bool,
    /// A user-initiated write operation (copy, move, delete, drag-out) is touching
    /// the volume right now. The user asked for it and is watching a progress bar,
    /// so background work on the same volume stands aside until it ends.
    pub(crate) transfer_active: bool,
}

impl WorkClearance {
    /// Nothing is competing: full speed. The answer a host with no signals gives,
    /// and the shape every "is anything in the way?" check compares against.
    pub(crate) const CLEAR: Self = Self {
        app_idle: true,
        volume_idle: true,
        transfer_active: false,
    };
}

/// The host's background-work priority signals.
pub(crate) trait HostPolicy: Send + Sync {
    /// Whether background work may run at full speed against `volume_id` right now,
    /// treating the volume (and the app) as busy for `idle_threshold` after the last
    /// foreground activity.
    ///
    /// Must be cheap: callers take a snapshot at every batch boundary of a running
    /// scan. ❌ Don't do I/O, take a contended lock, or block here.
    fn clearance(&self, volume_id: &str, idle_threshold: Duration) -> WorkClearance;
}

/// The host that never asks for anything: used until one is installed, and by every
/// test that isn't about pacing. Matches the behavior of the real signals with no
/// activity recorded, which is what test binaries saw before this seam existed.
pub(crate) struct AlwaysClear;

impl HostPolicy for AlwaysClear {
    fn clearance(&self, _volume_id: &str, _idle_threshold: Duration) -> WorkClearance {
        WorkClearance::CLEAR
    }
}

static INSTALLED: OnceLock<Arc<dyn HostPolicy>> = OnceLock::new();

/// A [`set_host_policy`] call that arrived after one was already installed.
#[derive(Debug)]
pub(crate) struct HostPolicyAlreadySet;

/// Tells the index which host to ask about background-work priority. Call once at
/// startup. A second call keeps the first policy, so a late caller can't change the
/// answer under a scan that's already pacing itself against it.
pub(crate) fn set_host_policy(policy: Arc<dyn HostPolicy>) -> Result<(), HostPolicyAlreadySet> {
    INSTALLED.set(policy).map_err(|_| HostPolicyAlreadySet)
}

/// The installed host policy, or [`AlwaysClear`] when nothing was installed.
///
/// Prefer capturing the result once, where a piece of work is set up (the way
/// `ScanPacer` does), over calling this deep inside a loop.
pub(crate) fn current() -> Arc<dyn HostPolicy> {
    if let Some(installed) = INSTALLED.get() {
        return Arc::clone(installed);
    }
    static FALLBACK: OnceLock<Arc<dyn HostPolicy>> = OnceLock::new();
    Arc::clone(FALLBACK.get_or_init(|| Arc::new(AlwaysClear)))
}

/// A controllable host for tests: set the signals, count the questions.
///
/// This is the seam's write half. The real signals live in process-global maps that
/// tests can only nudge and never reset, so anything that needs a volume to *become*
/// busy and then quiet drives one of these instead.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct FakeHostPolicy {
    app_busy: std::sync::atomic::AtomicBool,
    volume_busy: std::sync::atomic::AtomicBool,
    transfer_running: std::sync::atomic::AtomicBool,
    /// How many times [`HostPolicy::clearance`] has been asked. The evidence for the
    /// per-batch-not-per-entry rule.
    calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl FakeHostPolicy {
    /// A host with nothing competing, wrapped for injection.
    pub(crate) fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The user is browsing (this volume, and therefore the app too).
    pub(crate) fn note_foreground_activity(&self) {
        self.app_busy.store(true, std::sync::atomic::Ordering::SeqCst);
        self.volume_busy.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The user stopped browsing and the idle window has elapsed.
    pub(crate) fn note_foreground_quiet(&self) {
        self.app_busy.store(false, std::sync::atomic::Ordering::SeqCst);
        self.volume_busy.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// A user-initiated transfer started on this volume.
    pub(crate) fn note_transfer_started(&self) {
        self.transfer_running.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The transfer finished (any exit path).
    pub(crate) fn note_transfer_finished(&self) {
        self.transfer_running.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// How many clearance questions this host has been asked.
    pub(crate) fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl HostPolicy for FakeHostPolicy {
    fn clearance(&self, _volume_id: &str, _idle_threshold: Duration) -> WorkClearance {
        use std::sync::atomic::Ordering::SeqCst;
        self.calls.fetch_add(1, SeqCst);
        WorkClearance {
            app_idle: !self.app_busy.load(SeqCst),
            volume_idle: !self.volume_busy.load(SeqCst),
            transfer_active: self.transfer_running.load(SeqCst),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no host installed, background work runs at full speed rather than
    /// standing aside forever. A "busy" default would silently stall every scan in
    /// every test binary and every tool that never installs a policy.
    #[test]
    fn an_uninstalled_policy_reads_as_clear() {
        assert_eq!(
            current().clearance("root", Duration::from_secs(2)),
            WorkClearance::CLEAR
        );
    }

    /// The fake's write half has to actually move the answer, or every test built on
    /// it would pass vacuously.
    #[test]
    fn the_fake_reports_what_was_noted() {
        let fake = FakeHostPolicy::shared();
        let ask = || fake.clearance("root", Duration::from_secs(2));

        assert_eq!(ask(), WorkClearance::CLEAR, "nothing noted yet");

        fake.note_foreground_activity();
        assert_eq!(
            ask(),
            WorkClearance {
                app_idle: false,
                volume_idle: false,
                transfer_active: false
            }
        );

        fake.note_foreground_quiet();
        fake.note_transfer_started();
        assert_eq!(
            ask(),
            WorkClearance {
                app_idle: true,
                volume_idle: true,
                transfer_active: true
            }
        );

        fake.note_transfer_finished();
        assert_eq!(ask(), WorkClearance::CLEAR);
        assert_eq!(fake.call_count(), 4, "every ask is counted");
    }
}
