//! Holding a walk back until it has run long enough to be worth telling anyone
//! about.
//!
//! Covering a drive in phases walks one frontier root at a time, and most of them
//! are small: a run over a real boot disk announces 50–150 branches per phase, and
//! the majority are done in well under a second. Forwarding each one would flicker
//! an hourglass onto every affected row and off it again before anyone could read
//! it, which is worse than showing nothing.
//!
//! So the rule is "don't announce a walk that finishes inside a second", and it
//! lives HERE rather than in the frontend or in `cmdr-index`. It's a presentation
//! decision, so the crate has no business making it (it reports what it is doing);
//! and it's a rule, so it belongs where it is unit-testable rather than spread
//! across component lifetimes. The frontend then renders exactly what it is told:
//! no timers, no suppression, no cleanup to get wrong.
//!
//! The end of a walk is NEVER held back. A start that was suppressed simply has no
//! end to send; a start that was announced gets its end immediately, so a row can't
//! be left wearing an hourglass for a walk that stopped.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_index::IndexEvent;

use crate::ignore_poison::IgnorePoison;

/// How long a branch has to stay under the walker before anyone hears about it.
///
/// One second is the shortest wait that reads as deliberate rather than as a
/// glitch: below it an appearing-and-vanishing glyph is noise, and above it the
/// long walks (`~/Library` alone is over a minute) would still be announced just
/// as well, only later.
pub(crate) const ANNOUNCE_AFTER: Duration = Duration::from_secs(1);

/// Where a held-back event goes once it's cleared to fly.
type Forward = Arc<dyn Fn(IndexEvent) + Send + Sync>;

/// One volume's branch, waiting out its debounce or already announced.
struct PendingBranch {
    /// Bumped by every new start on this volume, so a timer that fires for a
    /// superseded branch can tell and do nothing.
    generation: u64,
    roots: Vec<String>,
    announced: bool,
}

/// Decides which coverage-branch events reach the frontend, and when.
pub(crate) struct WalkAnnouncer {
    delay: Duration,
    forward: Forward,
    /// One entry per volume with a walk in flight. The phase machine runs its
    /// walks one at a time per volume, so a second start on the same volume
    /// supersedes the first rather than joining it.
    branches: Mutex<HashMap<String, PendingBranch>>,
}

impl WalkAnnouncer {
    /// An announcer that forwards through `forward` after the shipping delay.
    pub(crate) fn new(forward: Forward) -> Arc<Self> {
        Self::with_delay(ANNOUNCE_AFTER, forward)
    }

    /// The same, with the wait set explicitly. For tests, which would otherwise
    /// spend a real second per case.
    pub(crate) fn with_delay(delay: Duration, forward: Forward) -> Arc<Self> {
        Arc::new(Self {
            delay,
            forward,
            branches: Mutex::new(HashMap::new()),
        })
    }

    /// Take one coverage-branch event off the crate and decide its fate.
    ///
    /// ❌ Anything else is none of this type's business: the sink routes every
    /// other event straight through.
    pub(crate) fn observe(self: &Arc<Self>, event: IndexEvent) {
        match event {
            IndexEvent::CoverageBranchStarted { volume_id, roots } => self.started(volume_id, roots),
            IndexEvent::CoverageBranchEnded { volume_id, roots } => self.ended(&volume_id, roots),
            // The sink hands us only the two above; anything else means the two
            // have drifted apart, and dropping it silently is how that stays
            // invisible.
            other => (self.forward)(other),
        }
    }

    /// This volume's run ended, however it ended. Any branch still in flight goes
    /// out as an end (if it was ever announced) and the volume is forgotten.
    pub(crate) fn run_ended(&self, volume_id: &str) {
        let Some(pending) = self.branches.lock_ignore_poison().remove(volume_id) else {
            return;
        };
        if pending.announced {
            (self.forward)(IndexEvent::CoverageBranchEnded {
                volume_id: volume_id.to_string(),
                roots: pending.roots,
            });
        }
    }

    fn started(self: &Arc<Self>, volume_id: String, roots: Vec<String>) {
        let generation = {
            let mut branches = self.branches.lock_ignore_poison();
            let generation = branches.get(&volume_id).map_or(0, |b| b.generation + 1);
            let superseded = branches.insert(
                volume_id.clone(),
                PendingBranch {
                    generation,
                    roots: roots.clone(),
                    announced: false,
                },
            );
            // A walk that never reported its end can't leave the row it lit stuck
            // that way, so close it before the new one opens.
            if let Some(previous) = superseded.filter(|b| b.announced) {
                (self.forward)(IndexEvent::CoverageBranchEnded {
                    volume_id: volume_id.clone(),
                    roots: previous.roots,
                });
            }
            generation
        };
        // The wait itself. A task rather than a thread because there are 50–150 of
        // these per phase and nearly all of them are thrown away; `ended` doesn't
        // wake it, it simply finds the generation moved on and says nothing.
        let announcer = Arc::clone(self);
        let delay = self.delay;
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            announcer.wait_is_over(&volume_id, generation);
        });
    }

    /// The branch has been under the walker long enough. Announce it, unless a
    /// newer walk took its place or it ended while we waited.
    fn wait_is_over(&self, volume_id: &str, generation: u64) {
        let roots = {
            let mut branches = self.branches.lock_ignore_poison();
            match branches.get_mut(volume_id) {
                Some(branch) if branch.generation == generation && !branch.announced => {
                    branch.announced = true;
                    branch.roots.clone()
                }
                _ => return,
            }
        };
        (self.forward)(IndexEvent::CoverageBranchStarted {
            volume_id: volume_id.to_string(),
            roots,
        });
    }

    fn ended(&self, volume_id: &str, roots: Vec<String>) {
        let announced = self
            .branches
            .lock_ignore_poison()
            .remove(volume_id)
            .is_some_and(|b| b.announced);
        if announced {
            (self.forward)(IndexEvent::CoverageBranchEnded {
                volume_id: volume_id.to_string(),
                roots,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use cmdr_index::{IndexEvent, IndexEventKind};

    use super::*;
    use crate::test_support::wait_until;

    /// A short wait, so a test spends milliseconds proving the rule rather than
    /// seconds waiting out the shipping one.
    const TEST_DELAY: Duration = Duration::from_millis(120);

    /// Long enough that a broken suppression shows up, short enough to stay a
    /// unit test.
    const SETTLE: Duration = Duration::from_millis(600);

    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<IndexEvent>>,
    }

    impl Recorder {
        fn forward(self: &Arc<Self>) -> Forward {
            let recorder = Arc::clone(self);
            Arc::new(move |event| recorder.events.lock_ignore_poison().push(event))
        }

        fn kinds(&self) -> Vec<IndexEventKind> {
            self.events.lock_ignore_poison().iter().map(IndexEvent::kind).collect()
        }

        fn roots(&self) -> Vec<Vec<String>> {
            self.events
                .lock_ignore_poison()
                .iter()
                .filter_map(|e| match e {
                    IndexEvent::CoverageBranchStarted { roots, .. } | IndexEvent::CoverageBranchEnded { roots, .. } => {
                        Some(roots.clone())
                    }
                    _ => None,
                })
                .collect()
        }
    }

    fn started(volume_id: &str, root: &str) -> IndexEvent {
        IndexEvent::CoverageBranchStarted {
            volume_id: volume_id.to_string(),
            roots: vec![root.to_string()],
        }
    }

    fn ended(volume_id: &str, root: &str) -> IndexEvent {
        IndexEvent::CoverageBranchEnded {
            volume_id: volume_id.to_string(),
            roots: vec![root.to_string()],
        }
    }

    #[test]
    fn a_walk_that_keeps_going_is_announced_once_the_wait_is_over() {
        let recorder = Arc::new(Recorder::default());
        let announcer = WalkAnnouncer::with_delay(TEST_DELAY, recorder.forward());

        announcer.observe(started("root", "/Users/someone/Library"));

        assert_eq!(recorder.kinds(), Vec::new(), "nothing goes out before the wait is over");
        wait_until(SETTLE, "the branch is announced", || {
            recorder.kinds() == vec![IndexEventKind::CoverageBranchStarted]
        });
        assert_eq!(recorder.roots(), vec![vec!["/Users/someone/Library".to_string()]]);
    }

    #[test]
    fn a_walk_that_finishes_inside_the_wait_is_never_announced_at_all() {
        let recorder = Arc::new(Recorder::default());
        let announcer = WalkAnnouncer::with_delay(TEST_DELAY, recorder.forward());

        announcer.observe(started("root", "/opt/small"));
        announcer.observe(ended("root", "/opt/small"));

        // Well past the wait: a start held back and then cancelled must not
        // surface late either, or the row lights up for a walk that is over.
        std::thread::sleep(SETTLE); // allowed-test-sleep: proving that NOTHING happens needs elapsed time, and there is no state to poll for
        assert_eq!(
            recorder.kinds(),
            Vec::new(),
            "a sub-second walk is invisible from end to end"
        );
    }

    #[test]
    fn an_announced_walk_always_gets_its_end_and_gets_it_immediately() {
        let recorder = Arc::new(Recorder::default());
        let announcer = WalkAnnouncer::with_delay(TEST_DELAY, recorder.forward());

        announcer.observe(started("root", "/Users/someone/Downloads"));
        wait_until(SETTLE, "the branch is announced", || !recorder.kinds().is_empty());

        announcer.observe(ended("root", "/Users/someone/Downloads"));

        assert_eq!(
            recorder.kinds(),
            vec![
                IndexEventKind::CoverageBranchStarted,
                IndexEventKind::CoverageBranchEnded
            ],
            "the end rides out on the calling thread, with no wait of its own"
        );
    }

    #[test]
    fn a_run_that_ends_mid_walk_takes_the_hourglass_with_it() {
        let recorder = Arc::new(Recorder::default());
        let announcer = WalkAnnouncer::with_delay(TEST_DELAY, recorder.forward());

        announcer.observe(started("root", "/Users/someone/Documents"));
        wait_until(SETTLE, "the branch is announced", || !recorder.kinds().is_empty());

        announcer.run_ended("root");

        assert_eq!(
            recorder.kinds(),
            vec![
                IndexEventKind::CoverageBranchStarted,
                IndexEventKind::CoverageBranchEnded
            ],
        );
    }

    #[test]
    fn a_run_that_ends_before_anything_was_announced_says_nothing() {
        let recorder = Arc::new(Recorder::default());
        let announcer = WalkAnnouncer::with_delay(TEST_DELAY, recorder.forward());

        announcer.observe(started("root", "/opt/small"));
        announcer.run_ended("root");

        std::thread::sleep(SETTLE); // allowed-test-sleep: same as above, absence needs elapsed time
        assert_eq!(recorder.kinds(), Vec::new());
    }

    #[test]
    fn one_volumes_walk_never_lights_up_another_volumes_rows() {
        let recorder = Arc::new(Recorder::default());
        let announcer = WalkAnnouncer::with_delay(TEST_DELAY, recorder.forward());

        announcer.observe(started("root", "/Users/someone/Music"));
        announcer.observe(started("smb-nas", "/Volumes/nas/photos"));
        wait_until(SETTLE, "both branches are announced", || recorder.kinds().len() == 2);

        announcer.observe(ended("root", "/Users/someone/Music"));

        assert_eq!(recorder.kinds().len(), 3, "ending one volume's walk leaves the other's");
        assert_eq!(
            recorder.roots().last(),
            Some(&vec!["/Users/someone/Music".to_string()]),
            "and the end names the volume's own ground"
        );
    }
}
