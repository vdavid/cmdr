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

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The host's answer for one volume, at one moment.
///
/// `Copy` on purpose — see the dispatch rule in the module docs. Every field is a
/// decision, never a raw timestamp: the elapsed-versus-threshold rule belongs to the
/// host, which is where the clock and the signals live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkClearance {
    /// No foreground activity **anywhere** in the app for the requested idle window.
    /// The right scope for work with no deadline that competes for the whole
    /// machine, like on-device image enrichment.
    pub app_idle: bool,
    /// No foreground activity **on the volume asked about** for the requested idle
    /// window. The right scope for work that contends for one share's connection,
    /// like a network index scan: browsing a local folder is no reason to slow a NAS.
    ///
    /// A volume nobody has browsed reads as idle, so a first scan starts at full
    /// speed rather than standing aside for a navigation that never happened.
    pub volume_idle: bool,
    /// A user-initiated write operation (copy, move, delete, drag-out) is touching
    /// the volume right now. The user asked for it and is watching a progress bar,
    /// so background work on the same volume stands aside until it ends.
    pub transfer_active: bool,
}

impl WorkClearance {
    /// Nothing is competing: full speed. The answer a host with no signals gives,
    /// and the shape every "is anything in the way?" check compares against.
    pub const CLEAR: Self = Self {
        app_idle: true,
        volume_idle: true,
        transfer_active: false,
    };
}

/// One directory a pane is showing right now.
///
/// Only what the index acts on: which volume it's on and where. The host's listing
/// cache knows more (listing id, entry count, age); none of it changes an
/// aggregation decision, so none of it crosses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenListing {
    /// The volume the listing is on. The only reliable way to tell a path on the
    /// scanned volume from a same-looking path on another one.
    pub volume_id: String,
    /// The directory being shown, as the host knows it (not yet firmlink-normalized).
    pub path: PathBuf,
}

/// The host's background-work priority signals.
pub trait HostPolicy: Send + Sync {
    /// Whether background work may run at full speed against `volume_id` right now,
    /// treating the volume (and the app) as busy for `idle_threshold` after the last
    /// foreground activity.
    ///
    /// Must be cheap: callers take a snapshot at every batch boundary of a running
    /// scan. ❌ Don't do I/O, take a contended lock, or block here.
    fn clearance(&self, volume_id: &str, idle_threshold: Duration) -> WorkClearance;

    /// Every directory a pane is showing right now.
    ///
    /// The other half of "what has the user's attention": mid-scan partial
    /// aggregation uses it to punch exactly the folders being looked at through the
    /// depth cap, so sizes appear where the user is rather than in scan order.
    ///
    /// Unlike [`clearance`](Self::clearance) this allocates, and it's asked on the
    /// scan-progress reporter's 500 ms tick. ❌ Not from anything faster.
    fn open_listings(&self) -> Vec<OpenListing>;

    /// The folders on `volume_id` that matter most to this user, best guess first.
    ///
    /// The third "what has the user's attention" question, and the one that sets a
    /// walk ORDER: an index that walks these before the rest of the volume is useful
    /// minutes before it is complete. ⚠️ Nothing in the crate asks yet — today's walk
    /// takes a volume as one bulk scan. **Order is the whole payload.** Nothing is
    /// promised about the paths beyond "walk them first": they carry no scope, so a
    /// host that answers differently between two calls changes what gets indexed
    /// first and never what gets indexed at all.
    ///
    /// The host owns which signals count (where the panes were last session, the
    /// user's favorites, the standard home folders) and owes the index a list that
    /// is deduplicated, free of paths below an earlier entry, and short. It is asked
    /// per volume so a host can answer for the boot drive without a share's phases
    /// inheriting somebody's home folder.
    ///
    /// Asked when the index needs it rather than once at startup, so an edited
    /// favorites list or a new session's tabs land without a restart. So the same
    /// cost rule as [`clearance`](Self::clearance) applies: cheap, ❌ no I/O on a
    /// contended path and no blocking lock. A host that needs to stat things caches
    /// its answer behind a short TTL.
    fn priority_roots(&self, volume_id: &str) -> Vec<PathBuf>;
}

/// The host that never asks for anything: used until one is installed, and by every
/// test that isn't about pacing. Matches the behavior of the real signals with no
/// activity recorded, which is what test binaries saw before this seam existed.
pub struct AlwaysClear;

impl HostPolicy for AlwaysClear {
    fn clearance(&self, _volume_id: &str, _idle_threshold: Duration) -> WorkClearance {
        WorkClearance::CLEAR
    }

    fn open_listings(&self) -> Vec<OpenListing> {
        Vec::new()
    }

    fn priority_roots(&self, _volume_id: &str) -> Vec<PathBuf> {
        Vec::new()
    }
}

static INSTALLED: OnceLock<Arc<dyn HostPolicy>> = OnceLock::new();

/// A [`set_host_policy`] call that arrived after one was already installed.
#[derive(Debug)]
pub struct HostPolicyAlreadySet;

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
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Default)]
pub struct FakeHostPolicy {
    app_busy: std::sync::atomic::AtomicBool,
    volume_busy: std::sync::atomic::AtomicBool,
    transfer_running: std::sync::atomic::AtomicBool,
    /// How many times [`HostPolicy::clearance`] has been asked. The evidence for the
    /// per-batch-not-per-entry rule.
    calls: std::sync::atomic::AtomicUsize,
    /// What [`HostPolicy::open_listings`] reports.
    open_listings: std::sync::RwLock<Vec<OpenListing>>,
    /// What [`HostPolicy::priority_roots`] reports, per volume, in the order noted.
    priority_roots: std::sync::RwLock<Vec<(String, PathBuf)>>,
}

#[cfg(any(test, feature = "testing"))]
impl FakeHostPolicy {
    /// A host with nothing competing, wrapped for injection.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The user is browsing (this volume, and therefore the app too).
    pub fn note_foreground_activity(&self) {
        self.app_busy.store(true, std::sync::atomic::Ordering::SeqCst);
        self.volume_busy.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The user stopped browsing and the idle window has elapsed.
    pub fn note_foreground_quiet(&self) {
        self.app_busy.store(false, std::sync::atomic::Ordering::SeqCst);
        self.volume_busy.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// A user-initiated transfer started on this volume.
    pub fn note_transfer_started(&self) {
        self.transfer_running.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The transfer finished (any exit path).
    pub fn note_transfer_finished(&self) {
        self.transfer_running.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// The user has a pane open on `path` on `volume_id`.
    pub fn note_open_listing(&self, volume_id: impl Into<String>, path: impl Into<PathBuf>) -> &Self {
        use cmdr_fs::ignore_poison::RwLockIgnorePoison;
        self.open_listings.write_ignore_poison().push(OpenListing {
            volume_id: volume_id.into(),
            path: path.into(),
        });
        self
    }

    /// This user cares about `path` on `volume_id`, after everything noted so far.
    pub fn note_priority_root(&self, volume_id: impl Into<String>, path: impl Into<PathBuf>) -> &Self {
        use cmdr_fs::ignore_poison::RwLockIgnorePoison;
        self.priority_roots
            .write_ignore_poison()
            .push((volume_id.into(), path.into()));
        self
    }

    /// How many clearance questions this host has been asked.
    pub fn call_count(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "testing"))]
impl HostPolicy for FakeHostPolicy {
    fn open_listings(&self) -> Vec<OpenListing> {
        use cmdr_fs::ignore_poison::RwLockIgnorePoison;
        self.open_listings.read_ignore_poison().clone()
    }

    fn priority_roots(&self, volume_id: &str) -> Vec<PathBuf> {
        use cmdr_fs::ignore_poison::RwLockIgnorePoison;
        self.priority_roots
            .read_ignore_poison()
            .iter()
            .filter(|(id, _)| id == volume_id)
            .map(|(_, path)| path.clone())
            .collect()
    }

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

    /// The open-listing half of the fake, and the shape `collect_hot_paths` filters
    /// on: a listing carries its volume, so a same-looking path on another volume
    /// can be told apart.
    #[test]
    fn the_fake_reports_the_listings_it_was_given() {
        let fake = FakeHostPolicy::shared();
        assert!(fake.open_listings().is_empty(), "no panes open yet");

        fake.note_open_listing("root", "/Users/david")
            .note_open_listing("smb-naspi", "/Volumes/naspi/media");

        assert_eq!(
            fake.open_listings(),
            vec![
                OpenListing {
                    volume_id: "root".into(),
                    path: PathBuf::from("/Users/david")
                },
                OpenListing {
                    volume_id: "smb-naspi".into(),
                    path: PathBuf::from("/Volumes/naspi/media")
                },
            ]
        );
    }

    /// With nothing installed, the reporter sees no open panes rather than
    /// panicking. Partial aggregation then simply has no hot paths to punch, which
    /// is the correct degradation.
    #[test]
    fn an_uninstalled_policy_reports_no_open_listings() {
        assert!(current().open_listings().is_empty());
    }

    /// No host to ask means no opinion on order, which leaves a walk to take the
    /// volume in whatever order it would have used anyway. Every test binary and
    /// dev tool runs this way.
    #[test]
    fn an_uninstalled_policy_reports_no_priority_roots() {
        assert!(current().priority_roots("root").is_empty());
    }

    /// The roots half of the fake: order survives, and a volume only ever hears
    /// about its own roots (a share must not inherit the boot drive's home folder).
    #[test]
    fn the_fake_reports_its_priority_roots_in_order_per_volume() {
        let fake = FakeHostPolicy::shared();
        assert!(fake.priority_roots("root").is_empty(), "nothing noted yet");

        fake.note_priority_root("root", "/Users/david/Downloads")
            .note_priority_root("smb-naspi", "/Volumes/naspi/media")
            .note_priority_root("root", "/Users/david");

        assert_eq!(
            fake.priority_roots("root"),
            vec![PathBuf::from("/Users/david/Downloads"), PathBuf::from("/Users/david"),]
        );
        assert_eq!(
            fake.priority_roots("smb-naspi"),
            vec![PathBuf::from("/Volumes/naspi/media")]
        );
    }
}
