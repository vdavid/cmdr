//! The gate's contract, on a fake clock and a fake index, with starts and stops the test holds
//! open by hand. No test waits out a real deadline.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use cmdr_index::{IndexVolumeKind, ROOT_VOLUME_ID, RemovableStop};

use super::release::{LateRelease, VolumeRelease};
use super::resume::{RESUME_SETTLE, ResumeCandidate, ResumeOwner, ResumeRefusal, ResumeVerdict};
use super::*;
use crate::test_support::wait_until;

/// How long a test waits for another thread to reach a point. Never a deadline under test.
const PATIENCE: Duration = Duration::from_secs(5);
/// The deadline a release gets when the test moves the clock past it by hand.
const DEADLINE: Duration = Duration::from_secs(7);

const A: &str = "vol-cmdr-test-a";
const B: &str = "vol-cmdr-test-b";
const C: &str = "vol-cmdr-test-c";

/// Counts a waiter parked at the gate, so a test knows a thread is waiting before it acts.
pub(super) struct ParkedMark<'a>(&'a AtomicUsize);

impl<'a> ParkedMark<'a> {
    pub(super) fn new(parked: &'a AtomicUsize) -> Self {
        parked.fetch_add(1, Ordering::SeqCst);
        Self(parked)
    }
}

impl Drop for ParkedMark<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl DriveRelease {
    fn is_unmount_pending(&self, volume_id: &str) -> bool {
        self.shared.gates.lock_ignore_poison().gate(volume_id).unmount_pending
    }

    fn running_stops(&self) -> usize {
        self.shared
            .gates
            .lock_ignore_poison()
            .stops
            .values()
            .filter(|slot| matches!(slot, release::StopSlot::Running))
            .count()
    }
}

#[derive(Default)]
struct FakeDoor {
    /// Volumes with a local external index.
    indexed: Mutex<HashSet<String>>,
    intent: Mutex<HashSet<String>>,
    intent_reads: AtomicUsize,
    /// Every resumed start, with its ticket while the start call is still "running".
    resumed: Mutex<Vec<(String, Option<Ticket>)>>,
    ejecting: Mutex<HashSet<String>>,
    unlisted: Mutex<HashSet<String>>,
}

impl FakeDoor {
    fn index(&self, volume_id: &str) {
        self.indexed.lock_ignore_poison().insert(volume_id.to_string());
    }

    fn intend(&self, volume_id: &str) {
        self.intent.lock_ignore_poison().insert(volume_id.to_string());
    }

    fn resumed_ids(&self) -> Vec<String> {
        self.resumed
            .lock_ignore_poison()
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// The resumed start of `volume_id` returns, so its ticket drops.
    fn finish_resumed_start(&self, volume_id: &str) {
        let ticket = self
            .resumed
            .lock_ignore_poison()
            .iter_mut()
            .find(|(id, ticket)| id == volume_id && ticket.is_some())
            .and_then(|(_, ticket)| ticket.take());
        assert!(ticket.is_some(), "no resumed start of {volume_id} is running");
        drop(ticket);
    }
}

impl IndexDoor for FakeDoor {
    fn volume_kind(&self, volume_id: &str) -> Option<IndexVolumeKind> {
        self.indexed
            .lock_ignore_poison()
            .contains(volume_id)
            .then_some(IndexVolumeKind::LocalExternal)
    }

    fn drives_to_resume(&self) -> Vec<String> {
        self.intent_reads.fetch_add(1, Ordering::SeqCst);
        self.intent.lock_ignore_poison().iter().cloned().collect()
    }

    fn start_resumed(&self, volume_id: String, ticket: Ticket) {
        self.resumed.lock_ignore_poison().push((volume_id, Some(ticket)));
    }

    fn is_ejecting(&self, volume_id: &str) -> bool {
        self.ejecting.lock_ignore_poison().contains(volume_id)
    }

    fn is_listed(&self, volume_id: &str) -> bool {
        !self.unlisted.lock_ignore_poison().contains(volume_id)
    }
}

struct Fixture {
    gate: DriveRelease,
    door: Arc<FakeDoor>,
}

fn fixture() -> Fixture {
    let door = Arc::new(FakeDoor::default());
    Fixture {
        gate: DriveRelease::with_fake_clock(Arc::clone(&door) as Arc<dyn IndexDoor>),
        door,
    }
}

fn ids(volume_ids: &[&str]) -> Vec<String> {
    volume_ids.iter().map(|id| id.to_string()).collect()
}

fn candidate(volume_id: &str, epoch: u64) -> ResumeCandidate {
    ResumeCandidate {
        volume_id: volume_id.to_string(),
        epoch,
    }
}

fn verdicts(expected: &[(&str, ResumeVerdict)]) -> Vec<(String, ResumeVerdict)> {
    expected
        .iter()
        .map(|(id, verdict)| (id.to_string(), *verdict))
        .collect()
}

/// What happened, in order, across threads.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<&'static str>>>);

impl Events {
    fn push(&self, event: &'static str) {
        self.0.lock_ignore_poison().push(event);
    }

    fn take(&self) -> Vec<&'static str> {
        std::mem::take(&mut *self.0.lock_ignore_poison())
    }
}

/// Every late release a release's recorder got.
#[derive(Clone, Default)]
struct LateLog(Arc<Mutex<Vec<LateRelease>>>);

impl LateLog {
    fn recorder(&self) -> impl Fn(LateRelease) + Send + Sync + 'static {
        let log = self.clone();
        move |late| log.0.lock_ignore_poison().push(late)
    }

    fn take(&self) -> Vec<LateRelease> {
        std::mem::take(&mut *self.0.lock_ignore_poison())
    }

    fn is_empty(&self) -> bool {
        self.0.lock_ignore_poison().is_empty()
    }
}

/// Stops that answer `Released` only once the test opens them, recording who was asked.
#[derive(Default)]
struct HeldStops {
    state: Mutex<HeldStopsState>,
    opened: Condvar,
}

#[derive(Default)]
struct HeldStopsState {
    asked: Vec<String>,
    open: HashSet<String>,
}

impl HeldStops {
    fn stop(self: &Arc<Self>) -> impl Fn(&str) -> RemovableStop + Send + Sync + 'static {
        let stops = Arc::clone(self);
        move |volume_id| {
            let mut state = stops.state.lock_ignore_poison();
            state.asked.push(volume_id.to_string());
            while !state.open.contains(volume_id) {
                state = stops
                    .opened
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            RemovableStop::Released
        }
    }

    fn open(&self, volume_id: &str) {
        self.state.lock_ignore_poison().open.insert(volume_id.to_string());
        self.opened.notify_all();
    }

    fn asked(&self) -> Vec<String> {
        self.state.lock_ignore_poison().asked.clone()
    }
}

/// A person's start of `volume_id` on its own thread, held inside its probe until the test sends on
/// the returned channel. The start stands an index up as it returns.
fn held_start(
    fx: &Fixture,
    volume_id: &'static str,
    events: &Events,
) -> (mpsc::Sender<()>, std::thread::JoinHandle<Gated<()>>) {
    let (probe_done, probe) = mpsc::channel::<()>();
    let gate = fx.gate.clone();
    let door = Arc::clone(&fx.door);
    let events = events.clone();
    let start = std::thread::spawn(move || {
        gate.start_blocking(volume_id, StartKind::UserEnable, || {
            let _ = probe.recv();
            door.index(volume_id);
            events.push("the start returned");
        })
    });
    wait_until(PATIENCE, "the start to take its ticket", || {
        fx.gate.holds_ticket(volume_id)
    });
    (probe_done, start)
}

/// Release `volume_id` with a stop that answers at once, and hand back the epoch it set.
fn released(fx: &Fixture, volume_id: &str) -> u64 {
    let release = fx.gate.release(
        &ids(&[volume_id]),
        fx.gate.now() + DEADLINE,
        |_| RemovableStop::Released,
        |_| {},
    );
    release.volumes[0].epoch
}

#[derive(Default)]
struct FakeOwner {
    records: Mutex<Vec<ResumeCandidate>>,
    consumed: Mutex<Vec<String>>,
    moved_on: Mutex<HashSet<String>>,
    unlisted: Mutex<HashSet<String>>,
    ejected_elsewhere: Mutex<HashSet<String>>,
}

impl FakeOwner {
    fn record(&self, candidate: ResumeCandidate) {
        self.records.lock_ignore_poison().push(candidate);
    }

    fn snapshot(&self) -> Vec<ResumeCandidate> {
        self.records.lock_ignore_poison().clone()
    }

    fn consumed(&self) -> Vec<String> {
        self.consumed.lock_ignore_poison().clone()
    }
}

impl ResumeOwner for FakeOwner {
    fn name(&self) -> &'static str {
        "test owner"
    }

    fn still_owns(&self, candidate: &ResumeCandidate) -> bool {
        !self.moved_on.lock_ignore_poison().contains(&candidate.volume_id)
    }

    fn is_listed(&self, candidate: &ResumeCandidate) -> bool {
        !self.unlisted.lock_ignore_poison().contains(&candidate.volume_id)
    }

    fn is_ejected_by_another_owner(&self, candidate: &ResumeCandidate) -> bool {
        self.ejected_elsewhere
            .lock_ignore_poison()
            .contains(&candidate.volume_id)
    }

    fn consume(&self, candidate: &ResumeCandidate) {
        self.records
            .lock_ignore_poison()
            .retain(|record| record.volume_id != candidate.volume_id);
        self.consumed.lock_ignore_poison().push(candidate.volume_id.clone());
    }
}

// ── Release ──────────────────────────────────────────────────────────

#[test]
fn a_release_during_a_starts_probe_waits_for_its_ticket_then_stops_what_the_start_stood_up() {
    let fx = fixture();
    let events = Events::default();
    let (probe_done, start) = held_start(&fx, A, &events);

    let release = {
        let gate = fx.gate.clone();
        let events = events.clone();
        let deadline = fx.gate.now() + DEADLINE;
        std::thread::spawn(move || {
            gate.release(
                &ids(&[A]),
                deadline,
                move |_| {
                    events.push("the stop ran");
                    RemovableStop::Released
                },
                |_| {},
            )
        })
    };
    wait_until(PATIENCE, "the release to wait on the start's ticket", || {
        fx.gate.parked() == 1
    });
    assert!(
        events.take().is_empty(),
        "nothing is stopped while the start is still probing"
    );

    probe_done.send(()).expect("the start is listening");
    assert_eq!(start.join().expect("the start thread"), Gated::Ran(()));
    let release = release.join().expect("the release thread");

    assert_eq!(
        release.outcome(A),
        Some(VolumeRelease::Released { was_indexing: true }),
        "the stop runs once the start returned, and finds the index it stood up"
    );
    assert_eq!(events.take(), ["the start returned", "the stop ran"]);
}

#[test]
fn a_ticket_still_in_flight_at_the_deadline_is_still_releasing_and_its_continuation_stops_the_start_once_it_returns() {
    let fx = fixture();
    let events = Events::default();
    let (probe_done, start) = held_start(&fx, A, &events);
    let late = LateLog::default();

    let release = {
        let events = events.clone();
        fx.gate.release(
            &ids(&[A]),
            fx.gate.now(),
            move |_| {
                events.push("the stop ran");
                RemovableStop::Released
            },
            late.recorder(),
        )
    };
    assert_eq!(
        release.outcome(A),
        Some(VolumeRelease::StillReleasing),
        "a start in flight is work to stop, never NothingToStop"
    );
    assert!(
        events.take().is_empty(),
        "the start still holds the volume, so nothing stopped yet"
    );

    probe_done.send(()).expect("the start is listening");
    assert_eq!(start.join().expect("the start thread"), Gated::Ran(()));
    wait_until(PATIENCE, "the continuation to record its late release", || {
        !late.is_empty()
    });

    assert_eq!(
        late.take(),
        [LateRelease {
            volume_id: A.to_string(),
            outcome: VolumeRelease::Released { was_indexing: true },
            epoch: release.volumes[0].epoch,
        }]
    );
    assert_eq!(events.take(), ["the start returned", "the stop ran"]);
    wait_until(PATIENCE, "the continuation to hand the ticket back", || {
        !fx.gate.holds_ticket(A)
    });
}

#[test]
fn one_deadline_bounds_every_volume_and_a_stuck_stop_records_its_late_release() {
    let fx = fixture();
    let events = Events::default();
    fx.door.index(B);
    let (probe_done, start) = held_start(&fx, A, &events);
    let stops = Arc::new(HeldStops::default());
    let late = LateLog::default();

    let release = {
        let gate = fx.gate.clone();
        let stop = stops.stop();
        let recorder = late.recorder();
        let deadline = fx.gate.now() + DEADLINE;
        std::thread::spawn(move || gate.release(&ids(&[A, B]), deadline, stop, recorder))
    };
    wait_until(PATIENCE, "B's stop to start at once", || stops.asked() == [B]);

    // A's start returns 3 s in, so its stop starts late.
    fx.gate.advance(Duration::from_secs(3));
    probe_done.send(()).expect("the start is listening");
    assert_eq!(start.join().expect("the start thread"), Gated::Ran(()));
    wait_until(PATIENCE, "A's stop to start once its ticket dropped", || {
        stops.asked() == [B, A]
    });
    stops.open(B);
    wait_until(PATIENCE, "B's stop to answer", || fx.gate.running_stops() == 1);

    fx.gate.advance(DEADLINE - Duration::from_secs(3));
    let release = release.join().expect("the release thread");
    assert_eq!(
        release.outcome(A),
        Some(VolumeRelease::StillReleasing),
        "A's stop started 3 s late and still ends at the one deadline"
    );
    assert_eq!(release.outcome(B), Some(VolumeRelease::Released { was_indexing: true }));
    assert!(late.is_empty(), "B answered in time, and A hasn't answered at all");

    stops.open(A);
    wait_until(PATIENCE, "A's stuck stop to record its late release", || {
        !late.is_empty()
    });
    assert_eq!(
        late.take(),
        [LateRelease {
            volume_id: A.to_string(),
            outcome: VolumeRelease::Released { was_indexing: true },
            epoch: release.volumes[0].epoch,
        }]
    );
}

// ── Resume ───────────────────────────────────────────────────────────

#[test]
fn a_release_before_a_resume_takes_its_ticket_aborts_the_resume() {
    let fx = fixture();
    fx.door.intend(A);
    let owner = Arc::new(FakeOwner::default());

    let epoch = released(&fx, A);
    let batch = fx.gate.resume(vec![candidate(A, epoch)], owner.clone());
    released(&fx, A);
    fx.gate.advance(RESUME_SETTLE);

    assert_eq!(
        batch.wait(),
        verdicts(&[(A, ResumeVerdict::NotResumed(ResumeRefusal::EpochMoved))])
    );
    assert!(fx.door.resumed_ids().is_empty());
    assert_eq!(
        owner.consumed(),
        [A],
        "a record is spent even when its resume fails a check"
    );
}

#[test]
fn two_resumes_of_one_volume_start_it_once_and_every_later_one_joins() {
    let fx = fixture();
    fx.door.intend(A);
    let approver = Arc::new(FakeOwner::default());
    let flight = Arc::new(FakeOwner::default());
    let epoch = released(&fx, A);

    let first_idle = fx.gate.resume(vec![candidate(A, epoch)], approver.clone());
    let second_idle = fx.gate.resume(vec![candidate(A, epoch)], approver);
    assert_eq!(second_idle.wait(), verdicts(&[(A, ResumeVerdict::Joined)]));
    let flights_resume = fx.gate.resume(vec![candidate(A, epoch)], flight.clone());
    assert_eq!(flights_resume.wait(), verdicts(&[(A, ResumeVerdict::Joined)]));

    fx.gate.advance(RESUME_SETTLE);
    assert_eq!(first_idle.wait(), verdicts(&[(A, ResumeVerdict::Started)]));

    // While that start is still running, another candidate joins it too.
    let during_the_start = fx.gate.resume(vec![candidate(A, epoch)], flight);
    assert_eq!(during_the_start.wait(), verdicts(&[(A, ResumeVerdict::Joined)]));
    assert_eq!(fx.door.resumed_ids(), [A]);
}

#[test]
fn a_release_while_a_resume_settles_voids_it_and_a_newer_candidate_resumes_the_volume() {
    // A refused `unmountDisk`: the first volume's ask stops A and its idle queues a resume; the
    // second volume's ask releases A again while that resume still settles, and the idle after
    // the refusal offers A with the newer epoch. Joining the void batch would lose A for good.
    let fx = fixture();
    fx.door.intend(A);
    let owner = Arc::new(FakeOwner::default());

    let first_idle = fx.gate.resume(vec![candidate(A, released(&fx, A))], owner.clone());
    let newer = released(&fx, A);
    let second_idle = fx.gate.resume(vec![candidate(A, newer)], owner);
    fx.gate.advance(RESUME_SETTLE);

    assert_eq!(
        first_idle.wait(),
        verdicts(&[(A, ResumeVerdict::NotResumed(ResumeRefusal::EpochMoved))])
    );
    assert_eq!(second_idle.wait(), verdicts(&[(A, ResumeVerdict::Started)]));
    assert_eq!(fx.door.resumed_ids(), [A]);
}

#[test]
fn an_idle_after_a_consumed_record_starts_nothing() {
    let fx = fixture();
    fx.door.intend(A);
    fx.door.intend(B);
    let owner = Arc::new(FakeOwner::default());
    owner.record(candidate(A, released(&fx, A)));
    owner.record(candidate(B, released(&fx, B)));
    owner.unlisted.lock_ignore_poison().insert(B.to_string());

    let first_idle = fx.gate.resume(owner.snapshot(), owner.clone());
    fx.gate.advance(RESUME_SETTLE);
    assert_eq!(
        first_idle.wait(),
        verdicts(&[
            (A, ResumeVerdict::Started),
            (B, ResumeVerdict::NotResumed(ResumeRefusal::NotListed))
        ])
    );
    fx.door.finish_resumed_start(A);
    assert!(
        owner.snapshot().is_empty(),
        "both records are spent, the refused one too"
    );

    // A stopped first scan would get `force_scan` from a second start.
    let next_idle = fx.gate.resume(owner.snapshot(), owner.clone());
    fx.gate.advance(RESUME_SETTLE);
    assert!(next_idle.wait().is_empty());
    assert_eq!(fx.door.resumed_ids(), [A]);
}

#[test]
fn a_resume_reads_intent_once_per_batch_and_starts_only_what_every_check_vouches_for() {
    let fx = fixture();
    let owner = Arc::new(FakeOwner::default());
    let volumes = ["vol-cmdr-test-ok", "vol-cmdr-test-ok-too", "vol-cmdr-test-no-intent"];
    let checked = [
        "vol-cmdr-test-moved-on",
        "vol-cmdr-test-ejected",
        "vol-cmdr-test-pending",
    ];
    for volume_id in volumes.iter().chain(&checked) {
        if *volume_id != "vol-cmdr-test-no-intent" {
            fx.door.intend(volume_id);
        }
    }
    let release = fx.gate.release(
        &ids(&[&volumes[..], &checked[..]].concat()),
        fx.gate.now() + DEADLINE,
        |_| RemovableStop::Released,
        |_| {},
    );
    owner.moved_on.lock_ignore_poison().insert(checked[0].to_string());
    owner
        .ejected_elsewhere
        .lock_ignore_poison()
        .insert(checked[1].to_string());

    let candidates = release
        .volumes
        .iter()
        .map(|volume| candidate(&volume.volume_id, volume.epoch))
        .collect();
    let batch = fx.gate.resume(candidates, owner);
    fx.gate.set_unmount_pending(&ids(&[checked[2]]));
    fx.gate.advance(RESUME_SETTLE);

    assert_eq!(
        batch.wait(),
        verdicts(&[
            (volumes[0], ResumeVerdict::Started),
            (volumes[1], ResumeVerdict::Started),
            (volumes[2], ResumeVerdict::NotResumed(ResumeRefusal::NoIntent)),
            (checked[0], ResumeVerdict::NotResumed(ResumeRefusal::OwnerMovedOn)),
            (
                checked[1],
                ResumeVerdict::NotResumed(ResumeRefusal::EjectedByAnotherOwner)
            ),
            (checked[2], ResumeVerdict::NotResumed(ResumeRefusal::UnmountPending)),
        ])
    );
    assert_eq!(fx.door.intent_reads.load(Ordering::SeqCst), 1);
    assert_eq!(fx.door.resumed_ids(), [volumes[0], volumes[1]]);
}

// ── Start ────────────────────────────────────────────────────────────

#[test]
fn a_persons_start_during_an_unmount_waits_it_out_and_starts_once_it_clears_while_listed() {
    for kind in [StartKind::UserEnable, StartKind::UserRescan] {
        let fx = fixture();
        fx.gate.set_unmount_pending(&ids(&[A]));
        let start = {
            let gate = fx.gate.clone();
            std::thread::spawn(move || {
                let inner = gate.clone();
                gate.start_blocking(A, kind, move || inner.is_unmount_pending(A))
            })
        };
        wait_until(PATIENCE, "the start to wait on the unmount", || fx.gate.parked() == 1);
        fx.gate.clear_unmount_pending(&ids(&[A]));

        assert_eq!(
            start.join().expect("the start thread"),
            Gated::Ran(false),
            "{kind:?} starts after the flag cleared, never under it"
        );
    }

    // Cmdr's own eject in flight is waited out the same way.
    let fx = fixture();
    fx.door.ejecting.lock_ignore_poison().insert(A.to_string());
    let start = {
        let gate = fx.gate.clone();
        std::thread::spawn(move || gate.start_blocking(A, StartKind::UserEnable, || ()))
    };
    wait_until(PATIENCE, "the start to wait on the eject", || fx.gate.parked() == 1);
    fx.door.ejecting.lock_ignore_poison().remove(A);
    fx.gate.shared.notify();
    assert_eq!(start.join().expect("the start thread"), Gated::Ran(()));
}

#[test]
fn a_persons_start_runs_nothing_past_the_unmount_wait_or_once_the_drive_left_the_mount_table() {
    let fx = fixture();
    fx.gate.set_unmount_pending(&ids(&[A]));
    let start = {
        let gate = fx.gate.clone();
        std::thread::spawn(move || gate.start_blocking(A, StartKind::UserEnable, || panic!("no start may run")))
    };
    wait_until(PATIENCE, "the start to wait on the unmount", || fx.gate.parked() == 1);
    fx.gate.advance(UNMOUNT_PENDING_WAIT);
    assert_eq!(
        start.join().expect("the start thread"),
        Gated::Skipped(SkipReason::DriveLeaving)
    );
    assert!(!fx.gate.holds_ticket(A));

    let fx = fixture();
    fx.gate.set_unmount_pending(&ids(&[A]));
    fx.door.unlisted.lock_ignore_poison().insert(A.to_string());
    let start = {
        let gate = fx.gate.clone();
        std::thread::spawn(move || gate.start_blocking(A, StartKind::UserRescan, || panic!("no start may run")))
    };
    wait_until(PATIENCE, "the start to wait on the unmount", || fx.gate.parked() == 1);
    fx.gate.clear_unmount_pending(&ids(&[A]));
    assert_eq!(
        start.join().expect("the start thread"),
        Gated::Skipped(SkipReason::DriveLeaving)
    );
    assert!(!fx.gate.holds_ticket(A));
}

#[test]
fn an_idle_clears_every_unmount_pending_flag_and_wakes_the_start_waiting_it_out() {
    let fx = fixture();
    fx.gate.set_unmount_pending(&ids(&[A, B]));
    let start = {
        let gate = fx.gate.clone();
        std::thread::spawn(move || gate.start_blocking(A, StartKind::UserEnable, || "started"))
    };
    wait_until(PATIENCE, "the start to wait on the unmount", || fx.gate.parked() == 1);

    fx.gate.clear_all_unmount_pending();

    assert_eq!(start.join().expect("the start thread"), Gated::Ran("started"));
    assert!(!fx.gate.is_unmount_pending(A));
    assert!(
        !fx.gate.is_unmount_pending(B),
        "every flag clears, not only the waited-on one"
    );
}

#[test]
fn the_background_starts_skip_a_leaving_drive_at_once() {
    let fx = fixture();
    fx.gate.set_unmount_pending(&ids(&[A, ROOT_VOLUME_ID]));
    fx.door.ejecting.lock_ignore_poison().insert(B.to_string());

    #[cfg_attr(
        not(target_os = "macos"),
        allow(unused_mut, reason = "only macOS has a rename start to add")
    )]
    let mut kinds = vec![StartKind::MasterResume, StartKind::SearchCover];
    #[cfg(target_os = "macos")]
    kinds.push(StartKind::DriveRenamed);
    for kind in kinds {
        for volume_id in [A, B] {
            assert_eq!(
                fx.gate.start_blocking(volume_id, kind, || panic!("no start may run")),
                Gated::Skipped(SkipReason::DriveLeaving),
                "{kind:?} on {volume_id}"
            );
        }
        assert_eq!(fx.gate.start_blocking(C, kind, || "walked"), Gated::Ran("walked"));
        assert_eq!(
            fx.gate.start_blocking(ROOT_VOLUME_ID, kind, || "walked"),
            Gated::Ran("walked"),
            "the boot disk never unmounts, so it never waits at the gate"
        );
    }
}

#[tokio::test]
async fn an_async_start_holds_its_ticket_until_its_start_call_returns() {
    let fx = fixture();
    let gate = fx.gate.clone();
    let answer = fx
        .gate
        .start(A, StartKind::UserEnable, || async move { gate.holds_ticket(A) })
        .await;
    assert_eq!(answer, Gated::Ran(true));
    assert!(!fx.gate.holds_ticket(A));
}

// ── Disable ──────────────────────────────────────────────────────────

#[test]
fn a_disable_during_a_resumed_start_waits_for_it_and_has_the_last_word() {
    let fx = fixture();
    fx.door.intend(A);
    let owner = Arc::new(FakeOwner::default());
    let epoch = released(&fx, A);
    let batch = fx.gate.resume(vec![candidate(A, epoch)], owner.clone());
    fx.gate.advance(RESUME_SETTLE);
    assert_eq!(batch.wait(), verdicts(&[(A, ResumeVerdict::Started)]));

    let events = Events::default();
    let disable = {
        let gate = fx.gate.clone();
        let events = events.clone();
        std::thread::spawn(move || gate.disable_blocking(A, move || events.push("disabled")))
    };
    wait_until(PATIENCE, "the disable to wait on the resumed start", || {
        fx.gate.parked() == 1
    });
    assert!(events.take().is_empty(), "the disable waits for the start in flight");

    events.push("the resumed start returned");
    fx.door.finish_resumed_start(A);
    disable.join().expect("the disable thread");
    assert_eq!(events.take(), ["the resumed start returned", "disabled"]);
    assert!(fx.gate.epoch(A) > epoch, "the disable moved the epoch");

    // A resume recorded before the disable can never bring the drive back.
    let late_idle = fx.gate.resume(vec![candidate(A, epoch)], owner);
    fx.gate.advance(RESUME_SETTLE);
    assert_eq!(
        late_idle.wait(),
        verdicts(&[(A, ResumeVerdict::NotResumed(ResumeRefusal::EpochMoved))])
    );
    assert_eq!(fx.door.resumed_ids(), [A]);
}
