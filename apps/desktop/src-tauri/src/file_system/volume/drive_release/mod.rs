//! One door for letting go of a drive, and one for starting its index.
//!
//! Every app-side stop of a removable drive's index Cmdr decides on (its own eject, the unmount
//! hooks) and every app-side start of a non-root index (the user's enable and rescan, the master
//! switch's resume loop, a search's cover walk) passes through [`DriveRelease`], so they're
//! serialized per volume id. The stops are one piece of work with different budgets, and a start
//! landing in the middle of one is how an index ends up on a drive an unmount is taking down.
//!
//! Per volume id, under one mutex and one condvar:
//!
//! - an **epoch**, which every release and every disable moves;
//! - at most one **ticket**, held from before a start's first check until its start call returns;
//! - a **pending resume** and an **unmount-pending** flag.
//!
//! `release.rs` lets go, `resume.rs` hands back what a stop's owner stopped.
//! `volume/DETAILS.md` § "One release, one start" has the model and the why.
//!
//! ❌ The root volume's launch and FDA starts don't come through here: the boot disk never
//! unmounts.

mod release;
mod resume;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use cmdr_index::{IndexVolumeKind, ROOT_VOLUME_ID};

use crate::ignore_poison::IgnorePoison;

pub(crate) use release::VolumeRelease;
pub(crate) use release::{LateRelease, Release};
// Resuming is macOS-only work (the unmount approver and the per-disk eject flight), and
// so are the tests that drive it, so the gates carry no `test` arm.
#[cfg(all(test, target_os = "macos"))]
pub(crate) use resume::RESUME_SETTLE;
#[cfg_attr(
    not(target_os = "macos"),
    expect(unused_imports, reason = "only the macOS unmount approver resumes through the gate")
)]
pub(crate) use resume::{ResumeBatch, ResumeCandidate, ResumeOwner};

/// How long a person's enable or rescan waits out an unmount in progress before it gives up on the
/// start. The slowest refused unmount measured took 27.8 s to answer (`diskarbitrationd` scans for
/// holders first, `volume/DETAILS.md` § "Eject"), so a refusal settles inside it.
pub(crate) const UNMOUNT_PENDING_WAIT: Duration = Duration::from_secs(30);

/// Which app-side start is asking. It decides what the start does about a drive that's leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartKind {
    /// The user turned indexing on for the drive (IPC or MCP).
    UserEnable,
    /// The user asked for a rescan of the drive (IPC or MCP).
    UserRescan,
    /// The master switch came back on and this drive's intent says it should index.
    MasterResume,
    /// A search is about to walk the drive's unindexed ground.
    SearchCover,
    /// The drive was renamed while it indexed, and its index restarts at the new mount point
    /// (`Index::follow_volume_move`). A restart is a start: an eject landing beside it must not
    /// find a fresh watcher on the drive.
    #[cfg(target_os = "macos")]
    DriveRenamed,
}

impl StartKind {
    /// Whether this start waits out an unmount in progress, because a person asked for it. The
    /// others skip the drive at once: nobody is owed a walk of a drive that's leaving.
    fn waits_out_an_unmount(self) -> bool {
        matches!(self, Self::UserEnable | Self::UserRescan)
    }
}

/// What a gated start did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Gated<T> {
    /// The start call ran, holding the volume's ticket, and this is its answer.
    Ran(T),
    /// No start call ran.
    Skipped(SkipReason),
}

/// Why a gated start ran no start call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// An unmount of the drive is pending or Cmdr's eject of it is in flight, or one just took the
    /// drive out of the mount table.
    DriveLeaving,
    /// Another start of the volume still held its ticket when the wait for it ran out.
    AnotherStartStillRunning,
}

/// What the gate asks the index and the host. The app's answers are [`AppIndexDoor`]; a test hands
/// in its own.
pub(crate) trait IndexDoor: Send + Sync + 'static {
    /// The kind the index classified the volume as, `None` when it isn't indexed.
    fn volume_kind(&self, volume_id: &str) -> Option<IndexVolumeKind>;
    /// Every volume whose persisted intent says it should be indexing and isn't
    /// (`Index::drives_to_resume`). It opens read connections, so ❌ never on an ask.
    fn drives_to_resume(&self) -> Vec<String>;
    /// Start a resumed volume's index, holding `ticket` until the start call returns.
    fn start_resumed(&self, volume_id: String, ticket: Ticket);
    /// Whether Cmdr's own eject of the volume is in flight.
    fn is_ejecting(&self, volume_id: &str) -> bool;
    /// Whether the volume is still registered and, for a mount-backed id, its root is still in the
    /// mount table. A table that can't be read counts as listed.
    fn is_listed(&self, volume_id: &str) -> bool;
}

/// What a volume's ticket is held for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TicketFor {
    /// A start call, from before its first check until it returns.
    Start,
    /// The user's disable, so no resume can slip in under it.
    Disable,
    /// A release's continuation, stopping the start whose ticket it waited on.
    LateStop,
}

/// One volume's state at the gate.
// DEFAULT-OK: a volume the gate has never seen has had no release, no start, and no unmount ask,
// which is exactly epoch 0, no ticket, and both flags clear. It says nothing about the disk.
#[derive(Default)]
struct Gate {
    /// Moves on every release and disable; a resume recorded against an older one is void.
    epoch: u64,
    ticket: Option<TicketFor>,
    /// The resume batch that has this volume and hasn't reached its checks yet. Any epoch move
    /// cancels it: that batch's candidate is void, and a newer one must start a batch of its own
    /// rather than join it.
    resume_batch: Option<u64>,
    /// An unmount approval asked about this volume and nothing has cleared it since.
    unmount_pending: bool,
    /// Releases whose deadline passed while a start held the ticket: they stop the volume once it
    /// drops.
    late_stops: Vec<release::LateStop>,
}

// DEFAULT-OK: an empty gate table, the state at launch before any stop or start came through.
#[derive(Default)]
struct Gates {
    volumes: HashMap<String, Gate>,
    last_epoch: u64,
    /// Every release's stops still in flight, by serial.
    stops: HashMap<u64, release::StopSlot>,
    last_stop: u64,
    last_resume_batch: u64,
}

impl Gates {
    fn gate(&mut self, volume_id: &str) -> &mut Gate {
        self.volumes.entry(volume_id.to_string()).or_default()
    }

    /// Epochs come from one counter across volumes, so no two moves ever read equal. A move
    /// cancels the volume's pending resume, whose candidate carries the epoch it just left.
    fn bump_epoch(&mut self, volume_id: &str) -> u64 {
        self.last_epoch += 1;
        let epoch = self.last_epoch;
        let gate = self.gate(volume_id);
        gate.epoch = epoch;
        gate.resume_batch = None;
        epoch
    }
}

/// Where the gate reads the time, and how it parks until something changes.
enum Clock {
    Real,
    /// Moved by hand, so no test waits out a real deadline.
    #[cfg(test)]
    Fake {
        origin: Instant,
        elapsed: Mutex<Duration>,
    },
}

impl Clock {
    fn now(&self) -> Instant {
        match self {
            Self::Real => Instant::now(),
            #[cfg(test)]
            Self::Fake { origin, elapsed } => *origin + *elapsed.lock_ignore_poison(),
        }
    }
}

struct Shared {
    gates: Mutex<Gates>,
    /// Notified on every change a waiter could be waiting for: a ticket dropping, a stop answering,
    /// a flag clearing, the ejecting set shrinking, the fake clock moving.
    changed: Condvar,
    clock: Clock,
    door: Arc<dyn IndexDoor>,
    #[cfg(test)]
    parked: std::sync::atomic::AtomicUsize,
}

impl Shared {
    /// Park until something at the gate changes or `until` passes. Wake-ups can be spurious, so
    /// every caller re-checks its condition in a loop.
    fn park<'a>(&self, gates: MutexGuard<'a, Gates>, until: Instant) -> MutexGuard<'a, Gates> {
        #[cfg(test)]
        let _parked = tests::ParkedMark::new(&self.parked);
        match &self.clock {
            Clock::Real => {
                let wait = until.saturating_duration_since(Instant::now());
                self.changed
                    .wait_timeout(gates, wait)
                    .map(|(gates, _)| gates)
                    .unwrap_or_else(|poisoned| poisoned.into_inner().0)
            }
            #[cfg(test)]
            Clock::Fake { .. } => self
                .changed
                .wait(gates)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }

    /// Wake every waiter. The lock is taken first, so a waiter between its check and its park
    /// can't miss the wake-up.
    fn notify(&self) {
        drop(self.gates.lock_ignore_poison());
        self.changed.notify_all();
    }
}

/// A volume's one start slot. Dropping it hands the slot back, or on to a release's continuation
/// waiting to stop what the start stood up.
pub(crate) struct Ticket {
    shared: Arc<Shared>,
    volume_id: String,
}

impl Ticket {
    /// Wraps a ticket already marked taken in the volume's gate.
    fn taken(shared: &Arc<Shared>, volume_id: &str) -> Self {
        Self {
            shared: Arc::clone(shared),
            volume_id: volume_id.to_string(),
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let late_stops = {
            let mut gates = self.shared.gates.lock_ignore_poison();
            let gate = gates.gate(&self.volume_id);
            if gate.late_stops.is_empty() {
                gate.ticket = None;
                Vec::new()
            } else {
                // Handed straight on, so no start waiting for the slot can take it between the
                // start that just returned and the stop that was waiting for it.
                gate.ticket = Some(TicketFor::LateStop);
                std::mem::take(&mut gate.late_stops)
            }
        };
        self.shared.changed.notify_all();
        if !late_stops.is_empty() {
            release::run_late_stops(Ticket::taken(&self.shared, &self.volume_id), late_stops);
        }
    }
}

/// The per-volume gate every app-side stop and non-root start goes through. Cheap to clone; every
/// clone is the same gate.
#[derive(Clone)]
pub(crate) struct DriveRelease {
    shared: Arc<Shared>,
}

impl DriveRelease {
    fn new(door: Arc<dyn IndexDoor>, clock: Clock) -> Self {
        Self {
            shared: Arc::new(Shared {
                gates: Mutex::new(Gates::default()),
                changed: Condvar::new(),
                clock,
                door,
                #[cfg(test)]
                parked: std::sync::atomic::AtomicUsize::new(0),
            }),
        }
    }

    /// The gate's clock. A caller computing a deadline for [`Self::release`] reads the time here, so
    /// a test moves the deadline and the gate together.
    #[cfg_attr(
        all(not(test), not(target_os = "macos")),
        expect(dead_code, reason = "only the macOS unmount approver times itself by the gate")
    )]
    pub(crate) fn now(&self) -> Instant {
        self.shared.clock.now()
    }

    /// Whether a start, a disable, or a late stop holds `volume_id`'s ticket right now. An ask with
    /// no time left counts a ticket in flight as work it can't wait for.
    #[cfg_attr(
        all(not(test), not(target_os = "macos")),
        expect(dead_code, reason = "only the macOS unmount approver asks without waiting")
    )]
    pub(crate) fn holds_ticket(&self, volume_id: &str) -> bool {
        self.shared.gates.lock_ignore_poison().gate(volume_id).ticket.is_some()
    }

    /// How many threads are parked at the gate: a test knows a waiter arrived before it acts.
    #[cfg(test)]
    pub(crate) fn parked(&self) -> usize {
        self.shared.parked.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// A gate on a fake clock over an injected index, so no test waits out a real deadline.
    #[cfg(test)]
    pub(crate) fn with_fake_clock(door: Arc<dyn IndexDoor>) -> Self {
        Self::new(
            door,
            Clock::Fake {
                origin: Instant::now(),
                elapsed: Mutex::new(Duration::ZERO),
            },
        )
    }

    /// A gate on the real clock over an injected index, for a test that drives real unmounts.
    /// Only the macOS real-image lane has one.
    #[cfg(test)]
    #[cfg_attr(
        not(target_os = "macos"),
        expect(dead_code, reason = "only the macOS real-image lane drives real unmounts")
    )]
    pub(crate) fn with_door(door: Arc<dyn IndexDoor>) -> Self {
        Self::new(door, Clock::Real)
    }

    /// Move the fake clock on, waking every waiter.
    #[cfg(test)]
    pub(crate) fn advance(&self, by: Duration) {
        {
            let _gates = self.shared.gates.lock_ignore_poison();
            let Clock::Fake { elapsed, .. } = &self.shared.clock else {
                panic!("only a fake clock moves by hand");
            };
            *elapsed.lock_ignore_poison() += by;
        }
        self.shared.changed.notify_all();
    }

    /// Run `call`, a start of `volume_id`'s index, holding the volume's ticket for the whole call.
    ///
    /// Blocks while it waits for the ticket, so call it on a thread that may wait (a search's own
    /// thread), ❌ never a DA or GCD queue. The root volume passes straight through.
    pub(crate) fn start_blocking<T>(&self, volume_id: &str, kind: StartKind, call: impl FnOnce() -> T) -> Gated<T> {
        if volume_id == ROOT_VOLUME_ID {
            return Gated::Ran(call());
        }
        match self.take_start_ticket(volume_id, kind) {
            Ok(ticket) => {
                let answer = call();
                // ❗ Only now: the start call has returned, so whatever it stood up is visible to
                // the next stop.
                drop(ticket);
                Gated::Ran(answer)
            }
            Err(reason) => Gated::Skipped(reason),
        }
    }

    /// [`start_blocking`](Self::start_blocking) for an async start call. The wait for the ticket
    /// runs on a blocking thread; the call runs on the caller's task, holding the ticket until it
    /// returns.
    pub(crate) async fn start<T, Fut>(&self, volume_id: &str, kind: StartKind, call: impl FnOnce() -> Fut) -> Gated<T>
    where
        Fut: Future<Output = T>,
    {
        if volume_id == ROOT_VOLUME_ID {
            return Gated::Ran(call().await);
        }
        let gate = self.clone();
        let id = volume_id.to_string();
        let taken = tauri::async_runtime::spawn_blocking(move || gate.take_start_ticket(&id, kind))
            .await
            .expect("the ticket wait holds no user code, so only a bug in the gate can panic it");
        match taken {
            Ok(ticket) => {
                let answer = call().await;
                drop(ticket);
                Gated::Ran(answer)
            }
            Err(reason) => Gated::Skipped(reason),
        }
    }

    /// Take `volume_id`'s ticket for a start of `kind`, waiting at most [`UNMOUNT_PENDING_WAIT`] for
    /// a start in flight to return and, for a person's start, for an unmount to settle.
    fn take_start_ticket(&self, volume_id: &str, kind: StartKind) -> Result<Ticket, SkipReason> {
        let shared = &self.shared;
        let wait_ends = shared.clock.now() + UNMOUNT_PENDING_WAIT;
        let mut waited_out_an_unmount = false;
        let mut gates = shared.gates.lock_ignore_poison();
        loop {
            let leaving = gates.gate(volume_id).unmount_pending || shared.door.is_ejecting(volume_id);
            if leaving && !kind.waits_out_an_unmount() {
                log::info!(target: "drive_release", "Not starting {volume_id} for {kind:?}: an unmount of it is under way");
                return Err(SkipReason::DriveLeaving);
            }
            waited_out_an_unmount |= leaving;
            let gate = gates.gate(volume_id);
            if !leaving && gate.ticket.is_none() {
                gate.ticket = Some(TicketFor::Start);
                break;
            }
            if shared.clock.now() >= wait_ends {
                let reason = if leaving {
                    SkipReason::DriveLeaving
                } else {
                    SkipReason::AnotherStartStillRunning
                };
                log::warn!(
                    target: "drive_release",
                    "Not starting {volume_id} for {kind:?}: still waiting after {} s ({reason:?})",
                    UNMOUNT_PENDING_WAIT.as_secs()
                );
                return Err(reason);
            }
            gates = shared.park(gates, wait_ends);
        }
        drop(gates);

        let ticket = Ticket::taken(shared, volume_id);
        // An unmount that settled may have landed. Asked while holding the ticket, so a release
        // arriving now waits for this answer.
        if waited_out_an_unmount && !shared.door.is_listed(volume_id) {
            log::warn!(target: "drive_release", "Not starting {volume_id} for {kind:?}: it left the mount table while this waited");
            return Err(SkipReason::DriveLeaving);
        }
        Ok(ticket)
    }

    /// The user's per-drive disable: `call` (`Index::disable_volume`) runs after any start in
    /// flight returns, and a resume recorded before it can never bring the drive back.
    pub(crate) async fn disable<T: Send + 'static>(
        &self,
        volume_id: &str,
        call: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let gate = self.clone();
        let id = volume_id.to_string();
        tauri::async_runtime::spawn_blocking(move || gate.disable_blocking(&id, call))
            .await
            .expect("a disable that panicked has already logged it, and the command can't answer for it")
    }

    fn disable_blocking<T>(&self, volume_id: &str, call: impl FnOnce() -> T) -> T {
        let shared = &self.shared;
        let ticket = {
            let mut gates = shared.gates.lock_ignore_poison();
            // Moved first, so a resume that hasn't taken its ticket yet is void already.
            gates.bump_epoch(volume_id);
            let wait_ends = shared.clock.now() + UNMOUNT_PENDING_WAIT;
            loop {
                let gate = gates.gate(volume_id);
                if gate.ticket.is_none() {
                    gate.ticket = Some(TicketFor::Disable);
                    break Some(Ticket::taken(shared, volume_id));
                }
                if shared.clock.now() >= wait_ends {
                    // The index's own disable records its request on a transient phase, so it still
                    // wins over the start; it just can't wait for it any longer.
                    log::warn!(
                        target: "drive_release",
                        "Disabling {volume_id} without waiting further for the start in flight ({} s)",
                        UNMOUNT_PENDING_WAIT.as_secs()
                    );
                    break None;
                }
                gates = shared.park(gates, wait_ends);
            }
        };
        let answer = call();
        // Moved again before the ticket drops: a release that waited on this disable recorded an
        // epoch in between, and nothing it records may bring the drive back either.
        shared.gates.lock_ignore_poison().bump_epoch(volume_id);
        drop(ticket);
        answer
    }
}

static GATE: LazyLock<DriveRelease> = LazyLock::new(|| DriveRelease::new(Arc::new(AppIndexDoor), Clock::Real));

/// The app's one gate.
pub(crate) fn gate() -> &'static DriveRelease {
    &GATE
}

/// Wake the gate's waiters to read the ejecting set again: an eject flight landed.
pub(crate) fn notify_ejecting_changed() {
    GATE.shared.notify();
}

/// The gate's answers in the app: the real index, the eject flights, and the mount table.
struct AppIndexDoor;

impl IndexDoor for AppIndexDoor {
    fn volume_kind(&self, volume_id: &str) -> Option<IndexVolumeKind> {
        crate::index_host::index().volume_kind(volume_id)
    }

    fn drives_to_resume(&self) -> Vec<String> {
        crate::index_host::index().drives_to_resume()
    }

    fn start_resumed(&self, volume_id: String, ticket: Ticket) {
        tauri::async_runtime::spawn(async move {
            match crate::index_host::index().start_volume(&volume_id).await {
                Ok(outcome) => log::info!(target: "drive_release", "Resumed the index for {volume_id}: {outcome:?}"),
                Err(e) => {
                    log::warn!(target: "drive_release", "Resuming the index for {volume_id} didn't go through: {e}")
                }
            }
            drop(ticket);
        });
    }

    fn is_ejecting(&self, volume_id: &str) -> bool {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        return super::eject::is_ejecting(volume_id);
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = volume_id;
            false
        }
    }

    fn is_listed(&self, volume_id: &str) -> bool {
        let Some(volume) = super::manager::get_volume_manager().get(volume_id) else {
            return false;
        };
        if !cmdr_fs::volume::VolumeScheme::of(volume_id).is_mount_backed() {
            return true;
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        return super::eject::is_still_mounted(&volume.root().to_string_lossy());
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = volume;
            true
        }
    }
}
