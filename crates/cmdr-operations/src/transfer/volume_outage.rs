//! Waiting out a volume that went away mid-transfer, instead of failing.
//!
//! A router restart or a NAS hiccup drops the share for longer than a per-file
//! retry can bridge (`retry.rs`: three attempts inside 1.25 s), and once a
//! backend marks its session `Disconnected` every call fails at once. Before
//! this, that ended the whole copy, and every job queued behind it for the same
//! share started, failed within a second, and piled up as a failed row (`#398`).
//!
//! Now a transfer that meets a transport failure on a volume whose session reads
//! [`ConnectionState::Disconnected`] waits for it instead: it reconnects the
//! volume itself, on a growing cadence, for up to [`OutagePolicy::budget`](super::super::outage_slot::OutagePolicy::budget) (fifteen
//! minutes), and then runs the work again. If the volume isn't back by then, the
//! transfer fails exactly as it did before.
//!
//! **Why this layer, and why no new `Volume` method.** The capability already
//! exists on the trait and every session-owning backend implements it: SMB,
//! SFTP, WebDAV, S3, and ADB each answer [`Volume::connection_state`] and each
//! rebuild their session in place, single-flight, on
//! [`Volume::attempt_reconnect`]. What's missing is PATIENCE that respects an
//! operation (a budget, its Cancel, its Pause, its progress UI), and only the
//! transfer layer has an operation. An `await_reconnect` on the trait would put
//! five copies of one backoff loop in five crates, none of which can see a
//! Cancel. So a backend owns ONE attempt and the transfer owns the waiting. A
//! volume with no session to lose (a local disk, an OS-mounted share, MTP)
//! answers `None` and is never waited for; its failures stay what they were.
//!
//! **The guarantees, each load-bearing:**
//!
//! - **Bounded.** Every wait ends: the volume comes back, the budget runs out,
//!   the person cancels, or the backend says it can't come back on its own (a
//!   changed password: [`ConnectionState::NeedsSignIn`] or an auth refusal).
//! - **Cancelable at once.** Every await races `stop_work`; a Cancel (or a
//!   sibling's failure draining the window) never waits out a reconnect attempt
//!   or a sleep, and drops the attempt in flight.
//! - **No attempt holds the wait.** Each reconnect gets
//!   [`OutagePolicy::attempt_timeout`](super::super::outage_slot::OutagePolicy::attempt_timeout)
//!   (15 s); one that hasn't returned by then is dropped and counts as still
//!   down. A single hung handshake once held a copy for the whole budget after
//!   the NAS was back (`#406`).
//! - **Pausable.** A Pause parks the wait on the pause gate and stops dialing.
//!   A Resume is a person's action, so the wait gets a fresh budget from there.
//! - **Shared per volume.** An outage record per lane ([`Outages`], one per
//!   engine, reached through the operation's scope) gives every waiter on that
//!   share the same deadline, so a concurrent window's eight
//!   tasks give up together, and a job queued behind one that gave up doesn't
//!   start a second fifteen-minute wait for the same outage. A job CREATED after
//!   the give-up (a Retry, a new copy) is a fresh question and gets a fresh wait.
//! - **Retries nothing it shouldn't.** Only a failure that is a
//!   [`VolumeError::transport_fault`] ever triggers a wait, so a refusal or a
//!   cancel is reported as it always was.
//! - **A late error is the old session's.** In a concurrent window one task's
//!   reconnect brings the share back while a sibling's call is still in flight
//!   on the session it replaced; that call's error then arrives with the volume
//!   already live. Each call snapshots [`Volume::connection_generation`] BEFORE
//!   it runs ([`StartedOn`]), and a volume that reads live on a NEWER
//!   generation answers [`OutageEnd::Back`] at once, with nothing to dial. Live
//!   on the same generation stays [`OutageEnd::NotWaitable`]: the error is
//!   real. It still spends one of the item's [`MAX_OUTAGES_PER_ITEM`], so a
//!   flapping link stays bounded, and it never joins the lane's outage record
//!   (another waiter may be in a newer outage there). A backend that answers
//!   `None` keeps the old behavior exactly. The snapshot over-approximates on
//!   purpose: a call that snapshotted while the volume was down, then ran on a
//!   sibling's fresh session and failed for real, gets one re-run it didn't
//!   need, which is safe (only re-runnable work reaches these waits) and
//!   bounded.
//! - **Says it was the connection.** A wait that ends the work on a server
//!   whose session is gone hands back what it saw ([`lost_connection`]), so the
//!   person reads "Lost the connection to {server}" rather than a device error.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::super::event_sinks::OperationEventSink;
use super::super::state::WriteOperationState;
use super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::transfer_probe::{TaskPhase, current_task_phase, set_task_phase};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ConnectionState, LaneKey, Volume, VolumeError};

/// How a wait for a volume ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutageEnd {
    /// Every volume that was out is back: run the work again.
    Back,
    /// The budget ran out with the volume still gone. The caller fails the way
    /// it would have without waiting, and tells the person how long it waited
    /// ([`lost_connection`]).
    GaveUp {
        /// How long the lane's outage had lasted when it was given up on:
        /// every waiter on it shares one record, so this is how long the work
        /// waited, whoever started the wait.
        waited: Duration,
    },
    /// The person cancelled while the operation waited.
    Cancelled,
    /// Nothing to wait for: no listed volume is out in a way it can come back
    /// from on its own (no session at all, a session that needs a person to
    /// sign in, or a backend with no reconnect). The caller fails as before.
    NotWaitable,
}

/// How many separate outages one unit of work (a file, a directory level, a
/// pre-check) may wait out before a further one fails it as before.
///
/// A link that drops, comes back, and drops again is still worth riding out a
/// couple of times. One that does it over and over is broken in a way waiting
/// won't fix, and without a cap a flapping link could hold a file forever.
pub(crate) const MAX_OUTAGES_PER_ITEM: u32 = 3;

/// Who to tell that the operation started waiting, for the waits that happen
/// before the transfer probe is registered (the start gate), where nothing else
/// would speak for the operation. Waits inside the transfer don't need one: the
/// probe's heartbeat carries the wait to the UI within a second.
pub(crate) struct OutageAnnounce<'a> {
    pub sink: &'a dyn OperationEventSink,
    pub operation_id: &'a str,
    pub operation_type: WriteOperationType,
}

/// Which session each of a call's volumes was on when the call STARTED
/// ([`Volume::connection_generation`]), in the order of the call's volume list.
///
/// ❗ Taken right before the call, never after its error: by then a sibling's
/// reconnect may have moved the generation, and the snapshot would hide exactly
/// the race it exists to see.
#[derive(Debug, Clone)]
pub(crate) struct StartedOn(Vec<Option<u64>>);

impl StartedOn {
    /// The generations `volumes` are on right now.
    pub(crate) fn now(volumes: &[&Arc<dyn Volume>]) -> Self {
        Self(volumes.iter().map(|volume| volume.connection_generation()).collect())
    }

    /// Whether `volume`, at `index` in the call's list, was reconnected while
    /// the call ran: it reads live on a NEWER session than the call started on.
    /// Live on the same session means the error was that session's own, and a
    /// volume (or a snapshot) without a generation proves nothing.
    fn replaced(&self, index: usize, volume: &Arc<dyn Volume>) -> bool {
        let Some(Some(started)) = self.0.get(index) else {
            return false;
        };
        volume.connection_state().is_some_and(ConnectionState::is_live)
            && volume.connection_generation().is_some_and(|now| now > *started)
    }
}

/// Waits for every volume in `volumes` that's out, one after the other.
///
/// [`OutageEnd::NotWaitable`] when none of them is out in a way it can come back
/// from: the caller's failure is about something else, and stays its failure.
pub(crate) async fn wait_while_out(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    announce: Option<&OutageAnnounce<'_>>,
) -> OutageEnd {
    // No failing call to judge (the start gate), so no snapshot: nothing here
    // can prove an error stale.
    wait_out(state, volumes, &StartedOn(Vec::new()), announce).await
}

/// [`wait_while_out`], for a failure from a call that started on `started_on`.
async fn wait_out(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    announce: Option<&OutageAnnounce<'_>>,
) -> OutageEnd {
    let mut back = false;
    for (index, volume) in volumes.iter().enumerate() {
        if started_on.replaced(index, volume) {
            // Someone else's reconnect (a sibling's wait, a pane's backoff)
            // already brought it back: the error was the old session's. ❌ Not
            // `Outages::back`: this waiter never joined the lane's record, and
            // another may be in a NEWER outage on it.
            log::info!(
                target: "copy",
                "volume outage: volumeName={:?} was reconnected while the call ran; running it again",
                LogPrivate::volume(volume.name()),
            );
            back = true;
            continue;
        }
        if volume.connection_state() != Some(ConnectionState::Disconnected) {
            continue;
        }
        match wait_for_volume(state, volume, announce).await {
            OutageEnd::Back => back = true,
            // A volume that's out but can't be waited for is the same answer as
            // one that isn't out: look at the next.
            OutageEnd::NotWaitable => {}
            ended => return ended,
        }
    }
    if back { OutageEnd::Back } else { OutageEnd::NotWaitable }
}

/// Runs `call`, and when it fails on a transport error while one of `volumes`
/// is out, waits for the volume and runs it again, up to
/// [`MAX_OUTAGES_PER_ITEM`] times.
///
/// ❗ Only for a call that's safe to run twice: a read (a listing, a `stat`), or
/// a create whose second run answers `AlreadyExists` and is read as "there".
/// A write that may have half-landed goes through the per-file runner
/// (`volume/stream_pipe.rs`), which clears the attempt's partial first.
pub(crate) async fn through_outages<T, E, F, Fut>(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    mut call: F,
) -> Result<T, E>
where
    E: OutageFailure,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut outages = 0;
    loop {
        let started_on = StartedOn::now(volumes);
        let error = match call().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        match wait_after_failure(state, error.volume_error(), volumes, &started_on, &mut outages).await {
            OutageEnd::Back => {}
            OutageEnd::Cancelled => return Err(error.into_cancelled()),
            ended @ (OutageEnd::GaveUp { .. } | OutageEnd::NotWaitable) => {
                let lost = lost_connection(ended, error.volume_error(), volumes);
                return Err(error.with_lost_connection(lost));
            }
        }
    }
}

/// What [`through_outages`] needs of a call's error: the `VolumeError` to judge
/// the outage by, the same failure recast as the cancel that ended a wait, and
/// a place for what the wait learned. A bare `VolumeError`, or one that also
/// carries where it happened (`volume::transfer_error::PathedVolumeError`),
/// which keeps that through the wait and words a lost server connection as one.
pub(crate) trait OutageFailure {
    fn volume_error(&self) -> &VolumeError;
    fn into_cancelled(self) -> Self;
    /// The same failure, carrying the connection that ended the work. A failure
    /// with nowhere to keep it (a bare `VolumeError`) stays what it was.
    fn with_lost_connection(self, _lost: Option<LostConnection>) -> Self
    where
        Self: Sized,
    {
        self
    }
}

/// The server connection an item's work ended on, as the outage wait saw it:
/// the fact behind "Lost the connection to {server}"
/// (`WriteOperationError::ServerConnectionLost`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LostConnection {
    /// The server's volume, named as the volume names itself.
    pub(crate) volume_name: String,
    /// How long the work waited for it before it stopped. `None` when it didn't
    /// wait this time: a sign-in only a person can give, or an item that had
    /// already ridden out its [`MAX_OUTAGES_PER_ITEM`].
    pub(crate) gave_up_after: Option<Duration>,
}

/// What a wait that ENDED an item's work learned about its connection, for the
/// error the person reads: `Some` only for a transport fault while a server
/// volume's session reads not live (`Disconnected`, or a sign-in it needs).
///
/// ❗ Typed facts only, ❌ never a path or a message: the backend says it's a
/// server ([`BackendKind::is_server`](cmdr_fs::volume::BackendKind::is_server)),
/// so a phone over ADB keeps its device wording and a local disk or MTP (no
/// session) keeps whatever its error says. A server that reads LIVE proves no
/// lost connection either: the error was about something else, and keeps its
/// own words.
pub(crate) fn lost_connection(
    ended: OutageEnd,
    error: &VolumeError,
    volumes: &[&Arc<dyn Volume>],
) -> Option<LostConnection> {
    let gave_up_after = match ended {
        OutageEnd::GaveUp { waited } => Some(waited),
        OutageEnd::NotWaitable => None,
        OutageEnd::Back | OutageEnd::Cancelled => return None,
    };
    error.transport_fault()?;
    let server = volumes.iter().find(|volume| {
        volume.backend_kind().is_server() && volume.connection_state().is_some_and(|state| !state.is_live())
    })?;
    Some(LostConnection {
        volume_name: server.name().to_owned(),
        gave_up_after,
    })
}

impl OutageFailure for VolumeError {
    fn volume_error(&self) -> &VolumeError {
        self
    }

    fn into_cancelled(self) -> Self {
        cancelled()
    }
}

/// After `error` from a call that started on `started_on`, waits for whichever
/// of `volumes` went away, when that's what `error` was: a
/// [`VolumeError::transport_fault`], with this item's outage budget
/// ([`MAX_OUTAGES_PER_ITEM`], counted in `outages`) not spent. Once the work has
/// stopped (`state.stop_work`: a cancel, or the failure drain) nothing waits,
/// and the fault reads as the stop ([`OutageEnd::Cancelled`]).
/// [`OutageEnd::NotWaitable`] for every other failure, which the caller reports
/// as it always did.
pub(crate) async fn wait_after_failure(
    state: &Arc<WriteOperationState>,
    error: &VolumeError,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
) -> OutageEnd {
    if error.transport_fault().is_none() {
        return OutageEnd::NotWaitable;
    }
    wait_after_shortfall(state, volumes, started_on, outages).await
}

/// [`wait_after_failure`] for work no transfer probe speaks for yet (a copy's
/// pre-flight scan, a compress's planning walk), where nothing else would carry
/// the wait to the UI: `announce` hears it as it starts, so the progress reads
/// `Reconnecting` while it lasts, and again once the volume is back, so the
/// progress stops saying so the moment it's over rather than at the walk's next
/// tick.
pub(crate) async fn wait_after_failure_announced(
    state: &Arc<WriteOperationState>,
    error: &VolumeError,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
    announce: &OutageAnnounce<'_>,
) -> OutageEnd {
    if error.transport_fault().is_none() {
        return OutageEnd::NotWaitable;
    }
    let ended = wait_after(state, volumes, started_on, outages, Some(announce)).await;
    if ended == OutageEnd::Back {
        announce_wait(state, announce);
    }
    ended
}

/// [`wait_after_failure`] for a call that came up short with no error to
/// classify: a best-effort apply (`Volume::apply_metadata`) reports a drop as a
/// refused class, the same as a server's refusal. So the volumes answer
/// instead: one that reads `Disconnected`, or live on a newer session than the
/// call started on, is waited for; [`OutageEnd::NotWaitable`] when none is, and
/// the shortfall stands. ❗ Only for a call that's safe to run again.
pub(crate) async fn wait_after_shortfall(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
) -> OutageEnd {
    wait_after(state, volumes, started_on, outages, None).await
}

/// The wait both [`wait_after_shortfall`] and [`wait_after_failure_announced`]
/// take once the failure reads as a possible outage.
async fn wait_after(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
    announce: Option<&OutageAnnounce<'_>>,
) -> OutageEnd {
    // A call the share cut off while the work was stopping is the stop's: read
    // as `NotWaitable`, a Cancel reached the person as "Lost the connection".
    if state.work_stopped() {
        return OutageEnd::Cancelled;
    }
    if *outages >= MAX_OUTAGES_PER_ITEM {
        return OutageEnd::NotWaitable;
    }
    let ended = wait_out(state, volumes, started_on, announce).await;
    if ended == OutageEnd::Back {
        *outages += 1;
    }
    ended
}

/// The error a cancel during a wait reports: the cancel is why the work stopped,
/// so the post-loop must see it as one and emit `write-cancelled`, never log the
/// person's own click as a failed transfer.
pub(crate) fn cancelled() -> VolumeError {
    VolumeError::Cancelled("Operation cancelled by user".into())
}

/// Waits for ONE volume that reads `Disconnected`. See the module docs.
async fn wait_for_volume(
    state: &Arc<WriteOperationState>,
    volume: &Arc<dyn Volume>,
    announce: Option<&OutageAnnounce<'_>>,
) -> OutageEnd {
    let policy = state.outage_policy;
    let lane = volume.lane_key();
    let name = volume.name().to_owned();
    let outages = state.scope().stores().outages();
    let (mut deadline, _membership) = match Outages::join(outages, &lane, state.born, policy.budget) {
        Joined::Waiting { deadline, membership } => (deadline, membership),
        Joined::AlreadyGaveUp { waited } => {
            // This outage was waited out already, by work queued before it gave
            // up. One look before agreeing: the volume may be back by now, and
            // with no pane on it, nothing else would have noticed.
            log::info!(
                target: "copy",
                "volume outage: volumeName={name:?} already timed out for work queued before; trying it once",
                name = LogPrivate::volume(&name)
            );
            return match attempt_once(state, volume, None).await {
                Attempt::Reconnected => {
                    outages.back(&lane);
                    OutageEnd::Back
                }
                Attempt::Cancelled => OutageEnd::Cancelled,
                Attempt::Failed | Attempt::Hopeless => OutageEnd::GaveUp { waited },
            };
        }
    };

    let _slot = state.outage.begin(&name);
    let _phase = TaskPhaseGuard::enter(TaskPhase::WaitingForVolume);
    log::warn!(
        target: "copy",
        "volume outage: volumeName={name:?} went away; waiting up to {:?} for it to come back",
        deadline.saturating_duration_since(Instant::now()),
        name = LogPrivate::volume(&name),
    );
    if let Some(announce) = announce {
        announce_wait(state, announce);
    }

    let started = Instant::now();
    let mut failed_attempts = 0;
    let mut next_attempt = Instant::now();
    loop {
        if state.work_stopped() {
            return OutageEnd::Cancelled;
        }
        if state.pause_gate.is_paused() {
            // A paused operation dials nothing and its budget stops with it:
            // whoever paused it is deciding, and a Resume is a fresh question.
            if state.stop_or_park_async().await {
                return OutageEnd::Cancelled;
            }
            deadline = deadline.max(Instant::now() + policy.budget);
            next_attempt = Instant::now();
            continue;
        }
        match volume.connection_state() {
            Some(live) if live.is_live() => return came_back(outages, &lane, &name, started),
            Some(ConnectionState::Disconnected) => {}
            // A sign-in, a host key, or no session at all: not something time
            // fixes. The pane offers the sign-in; the transfer fails as before.
            _ => return OutageEnd::NotWaitable,
        }
        let now = Instant::now();
        if now >= deadline {
            let waited = outages.gave_up(&lane);
            log::warn!(
                target: "copy",
                "volume outage: volumeName={name:?} didn't come back within {:?}; giving up",
                now.saturating_duration_since(started),
                name = LogPrivate::volume(&name),
            );
            return OutageEnd::GaveUp { waited };
        }
        if now >= next_attempt {
            match attempt_once(state, volume, Some(deadline)).await {
                Attempt::Reconnected => return came_back(outages, &lane, &name, started),
                Attempt::Cancelled => return OutageEnd::Cancelled,
                Attempt::Hopeless => return OutageEnd::NotWaitable,
                Attempt::Failed => {
                    failed_attempts += 1;
                    next_attempt = Instant::now() + policy.delay_after(failed_attempts);
                }
            }
            continue;
        }
        // Sleep to the next attempt, the deadline, or the next look (a pause, or
        // a reconnect someone else made), whichever comes first. A cancel ends
        // it at once.
        let wake = next_attempt.min(deadline).min(now + policy.poll_every);
        tokio::select! {
            biased;
            () = state.stop_work.cancelled() => return OutageEnd::Cancelled,
            () = tokio::time::sleep_until(wake) => {}
        }
    }
}

/// Holds the current copy task (if any) in a phase, and puts back the one it
/// was in when it drops: a walker that waited goes back to `walking`, not to a
/// wait that's over. A no-op outside a copy task.
struct TaskPhaseGuard(Option<TaskPhase>);

impl TaskPhaseGuard {
    fn enter(phase: TaskPhase) -> Self {
        let previous = current_task_phase();
        set_task_phase(phase);
        Self(previous)
    }
}

impl Drop for TaskPhaseGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            set_task_phase(previous);
        }
    }
}

fn came_back(outages: &Outages, lane: &LaneKey, name: &str, started: Instant) -> OutageEnd {
    outages.back(lane);
    log::info!(
        target: "copy",
        "volume outage: volumeName={name:?} is back after {:?}; carrying on",
        started.elapsed(),
        name = LogPrivate::volume(name),
    );
    OutageEnd::Back
}

enum Attempt {
    Reconnected,
    Failed,
    /// The backend can't come back on its own: no reconnect at all, or a
    /// credential only a person can fix.
    Hopeless,
    Cancelled,
}

/// One reconnect, raced against a cancel and (when given) the deadline, and
/// bounded by the policy's attempt timeout: an attempt that never returns (an
/// SMB handshake a server accepted and never answered, `#406`) is dropped and
/// counts as still down, so the next tick dials afresh instead of the whole
/// wait sitting on it until the budget runs out.
async fn attempt_once(
    state: &Arc<WriteOperationState>,
    volume: &Arc<dyn Volume>,
    deadline: Option<Instant>,
) -> Attempt {
    let until_deadline = async {
        match deadline {
            Some(at) => tokio::time::sleep_until(at).await,
            None => std::future::pending().await,
        }
    };
    let attempt_timeout = state.outage_policy.attempt_timeout;
    let result = tokio::select! {
        biased;
        () = state.stop_work.cancelled() => return Attempt::Cancelled,
        // The loop reads the deadline next and gives up; an attempt still
        // dialing then is dropped, which the backends' single-flight tolerates.
        () = until_deadline => return Attempt::Failed,
        result = tokio::time::timeout(attempt_timeout, volume.attempt_reconnect()) => match result {
            Ok(result) => result,
            Err(_elapsed) => {
                log::warn!(
                    target: "copy",
                    "volume outage: a reconnect attempt to volumeName={:?} didn't return within {attempt_timeout:?}; \
                     dropping it and trying again on the next tick",
                    LogPrivate::volume(volume.name()),
                );
                return Attempt::Failed;
            }
        },
    };
    match result {
        Ok(()) => Attempt::Reconnected,
        // No reconnect story at all, or the server refused the saved
        // credentials (the backend already asked the pane to offer Sign in, and
        // dialing again risks locking the account).
        Err(VolumeError::NotSupported | VolumeError::PermissionDenied { .. } | VolumeError::NeedsPassword { .. }) => {
            Attempt::Hopeless
        }
        Err(error) => {
            log::debug!(target: "copy", "volume outage: reconnect attempt failed: {error}", error = LogError(&error));
            Attempt::Failed
        }
    }
}

/// Tells the windows the operation started waiting: the newest tick it emitted,
/// re-sent so `enrich_progress` classifies it (`Reconnecting`, with the volume's
/// name), or an opening scan tick when it hasn't emitted one yet.
fn announce_wait(state: &Arc<WriteOperationState>, announce: &OutageAnnounce<'_>) {
    let event = state.last_progress().unwrap_or_else(|| {
        WriteProgressEvent::new(
            announce.operation_id.to_owned(),
            announce.operation_type,
            WriteOperationPhase::Scanning,
            None,
            0,
            0,
            0,
            0,
        )
    });
    state.emit_progress_via_sink(announce.sink, event);
}

// ============================================================================
// The lane's outage record
// ============================================================================

/// One outage per lane (`Volume::lane_key`, the physical resource: one share,
/// one server, one device), shared by every operation on one engine waiting
/// on it.
pub(crate) struct Outages {
    records: Mutex<HashMap<LaneKey, Outage>>,
    /// The next outage's id.
    next_id: AtomicU64,
}

struct Outage {
    /// Tells a stale membership from this record's, so a waiter leaving an
    /// outage that ended (and was replaced) can't remove the new one.
    id: u64,
    started: Instant,
    gave_up_at: Option<std::time::Instant>,
    /// How long the outage had lasted when it was given up on, kept with the
    /// give-up so a job queued behind it can say how long Cmdr waited.
    waited: Duration,
    waiters: usize,
}

enum Joined {
    Waiting { deadline: Instant, membership: Membership },
    AlreadyGaveUp { waited: Duration },
}

impl Outages {
    /// No lane is in an outage, which is true until a waiter starts one.
    pub(crate) fn new() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Join the lane's current outage, or start one.
    ///
    /// - None running: start one now, with the full budget.
    /// - One running: share its deadline.
    /// - One that gave up, for an operation created BEFORE it gave up and within
    ///   a budget of it: [`Joined::AlreadyGaveUp`]. That's a job queued behind
    ///   the one that waited, and the outage it would wait out has already been
    ///   waited out once.
    /// - One that gave up, for an operation created after it (a Retry, a new
    ///   copy) or long after: a fresh outage, because that's a fresh question.
    ///
    /// The membership holds the record table until it leaves, so it outlives
    /// nothing but its own wait.
    fn join(outages: &Arc<Self>, lane: &LaneKey, born: std::time::Instant, budget: Duration) -> Joined {
        let mut records = outages.records.lock_ignore_poison();
        let now = Instant::now();
        if let Some(outage) = records.get_mut(lane) {
            match outage.gave_up_at {
                None => {
                    outage.waiters += 1;
                    return Joined::Waiting {
                        deadline: outage.started + budget,
                        membership: Membership {
                            outages: Arc::clone(outages),
                            lane: lane.clone(),
                            id: outage.id,
                        },
                    };
                }
                Some(gave_up_at) if born <= gave_up_at && gave_up_at.elapsed() < budget => {
                    return Joined::AlreadyGaveUp { waited: outage.waited };
                }
                Some(_) => {}
            }
        }
        let id = outages.next_id.fetch_add(1, Ordering::Relaxed);
        records.insert(
            lane.clone(),
            Outage {
                id,
                started: now,
                gave_up_at: None,
                waited: Duration::ZERO,
                waiters: 1,
            },
        );
        Joined::Waiting {
            deadline: now + budget,
            membership: Membership {
                outages: Arc::clone(outages),
                lane: lane.clone(),
                id,
            },
        }
    }

    /// Whether the lane's outage was already waited out and given up on, as
    /// [`join`](Self::join) judges it for an operation born at `born`: a
    /// stopped operation's cleanup then gets one look and no second wait
    /// (`leftover_cleanup.rs`).
    pub(crate) fn gave_up_on(&self, lane: &LaneKey, born: std::time::Instant, budget: Duration) -> bool {
        self.records
            .lock_ignore_poison()
            .get(lane)
            .and_then(|outage| outage.gave_up_at)
            .is_some_and(|gave_up_at| born <= gave_up_at && gave_up_at.elapsed() < budget)
    }

    /// The volume is back: the outage is over for everyone.
    fn back(&self, lane: &LaneKey) {
        self.records.lock_ignore_poison().remove(lane);
    }

    /// The outage outlasted its budget. Kept, so the jobs queued behind the one
    /// that waited don't each wait again (see [`join`](Self::join)). Answers how
    /// long it had lasted: the first waiter to give up fixes it, so every
    /// waiter on the lane reports the same wait.
    fn gave_up(&self, lane: &LaneKey) -> Duration {
        let mut records = self.records.lock_ignore_poison();
        let Some(outage) = records.get_mut(lane) else {
            return Duration::ZERO;
        };
        if outage.gave_up_at.is_none() {
            outage.gave_up_at = Some(std::time::Instant::now());
            outage.waited = outage.started.elapsed();
        }
        outage.waited
    }

    fn leave(&self, membership: &Membership) {
        let mut records = self.records.lock_ignore_poison();
        let Some(outage) = records.get_mut(&membership.lane) else {
            return;
        };
        if outage.id != membership.id {
            return;
        }
        outage.waiters = outage.waiters.saturating_sub(1);
        // Everyone who waited stopped (cancelled, or the work ended some other
        // way) without a verdict: there's no outage left to share. A give-up
        // stays, for the queued jobs that come after it.
        if outage.waiters == 0 && outage.gave_up_at.is_none() {
            records.remove(&membership.lane);
        }
    }
}

/// A waiter's place in its lane's outage; leaving drops it, whichever way the
/// wait ended.
struct Membership {
    outages: Arc<Outages>,
    lane: LaneKey,
    id: u64,
}

impl Drop for Membership {
    fn drop(&mut self) {
        self.outages.leave(self);
    }
}

#[cfg(test)]
#[path = "volume_outage_tests.rs"]
mod tests;
