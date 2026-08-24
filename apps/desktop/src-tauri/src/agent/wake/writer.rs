//! The wake loop's own thread: the one place that owns the inbox.
//!
//! It holds three things nobody else may touch: the [`Inbox`], one long-lived write connection
//! to `main.db`, and the timer. Everything reaches it as a message, so no producer ever takes a
//! lock or opens a connection of its own.
//!
//! ⚠️ **It never blocks on a turn.** A wake is prepared here and RUN on its own thread
//! (`runner.rs`). Blocking here would leave the bounded rollup channel unserviced for the length
//! of a model call, dropping rollups wholesale — a different thing entirely from the
//! pathological-burst drop the bound sanctions.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

use super::channel::{self, FolderActivity, ForcedWake, WakeControl, WakeMessage};
use super::followup::{self, FollowUpQueue};
use super::importance::ImportanceCache;
use super::runner::{self, BackgroundTurn, ResolvedSlot};
use super::settings::{self, WakeSettings};
use super::snapshot::readiness_snapshot;
use super::{Inbox, PrepareOutcome, PrepareParams, persist, prepare_wake};
use crate::agent::chat::budget;
use crate::agent::chat::session::{AgentSlot, resolve_agent_llm, resolve_prompt_budget};
use crate::agent::store;

const LOG_TARGET: &str = "agent::wake";

/// How long the loop waits with nothing scheduled. It would be correct to wait forever (every
/// arrival is a message), but a bounded park means a clock jump or a missed re-arm costs one
/// minute of latency rather than a loop that never wakes again.
const IDLE_POLL: Duration = Duration::from_secs(60);

/// ⚠️ **How long a declined attempt waits before trying again, and why the loop needs one at
/// all.** A deadline that has passed stays passed. Without a backoff the park would compute to
/// zero, `recv_timeout` would return instantly, and the loop would spin a core flat for as long
/// as an overdue row sits there — the ordinary state for anybody without consent or an API key.
/// A gate or settings change clears it, so opening the gate is felt at once.
const DECLINED_WAKE_BACKOFF: Duration = Duration::from_secs(5 * 60);

/// Start the wake loop. Called once, from `agent::start`, after the store handle and the chat
/// runtime are registered.
///
/// Rollups the tap sent before now are already waiting in the channel and get consumed as soon
/// as the thread comes up.
pub fn start(app: AppHandle, db_path: PathBuf, data_dir: PathBuf) {
    let Some(receiver) = channel::take_receiver() else {
        log::warn!(target: LOG_TARGET, "the wake loop is already running; not starting a second one");
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("agent-wake-loop".to_string())
        .spawn(move || run(app, db_path, data_dir, receiver));
    if let Err(e) = spawned {
        log::warn!(target: LOG_TARGET, "the wake loop did not start, so nothing will be noticed: {e}");
    }
}

fn run(app: AppHandle, db_path: PathBuf, data_dir: PathBuf, receiver: Receiver<WakeMessage>) {
    // ⚠️ ONE connection, opened once. `open_write_connection` applies the WAL pragmas and runs
    // the whole migration ladder, so opening per admit would put that on the indexer's path
    // against a 5 s busy timeout.
    let conn = match store::open_write_connection(&db_path) {
        Ok(conn) => conn,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the wake loop has no store, so nothing will be noticed: {e}");
            return;
        }
    };

    let mut loop_state = WakeLoop {
        inbox: launch_inbox(&conn),
        importance: ImportanceCache::new(data_dir),
        settings: settings::load(&app),
        wake_in_flight: false,
        not_before: 0,
        forced: None,
        follow_ups: FollowUpQueue::default(),
        app,
        conn,
    };
    // ⚠️ Before the reconciled inbox is written back, not after. `agent::start` refreshes the
    // gates just before this thread comes up, so this is the first moment a launch can tell
    // that the rows it just read back belong to a purpose nobody has agreed to — which is what
    // every user looks like the launch after a `CONSENT_COPY_VERSION` bump.
    loop_state.purge_inbox_if_not_permitted();
    if let Err(e) = persist::save_all(&loop_state.conn, &loop_state.inbox) {
        log::warn!(target: LOG_TARGET, "the reconciled inbox was not written back: {e}");
    }

    loop {
        match receiver.recv_timeout(loop_state.park()) {
            Ok(WakeMessage::Rollup(activity)) => {
                channel::note_rollup_consumed();
                loop_state.admit(activity);
            }
            Ok(WakeMessage::Control(control)) => loop_state.handle_control(control),
            Err(RecvTimeoutError::Timeout) => {}
            // Only reachable if the process-global sender were dropped, which it never is.
            Err(RecvTimeoutError::Disconnected) => return,
        }
        loop_state.report_dropped_rollups();
        loop_state.try_wake();
        loop_state.try_follow_up();
    }
}

/// Read the inbox back and settle it, logging what the user was NOT told and why.
fn launch_inbox(conn: &rusqlite::Connection) -> Inbox {
    let mut inbox = match persist::load(conn) {
        Ok(inbox) => inbox,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the stored inbox did not load, starting empty: {e}");
            return Inbox::default();
        }
    };
    let report = inbox.reconcile(now_secs());
    log::info!(
        target: LOG_TARGET,
        "the wake inbox reloaded with {} row(s): {} dropped as stale, {} deferred past the settle window",
        inbox.len(),
        report.dropped_stale,
        report.deferred
    );
    inbox
}

struct WakeLoop {
    app: AppHandle,
    conn: rusqlite::Connection,
    inbox: Inbox,
    importance: ImportanceCache,
    settings: WakeSettings,
    /// Whether a wake thread is running right now. See [`WakeLoop::try_wake`].
    wake_in_flight: bool,
    /// Unix seconds before which no wake is attempted, however overdue the inbox looks. See
    /// [`DECLINED_WAKE_BACKOFF`].
    not_before: u64,
    /// A [`WakeControl::ForceWake`] is outstanding: act on the inbox now, whatever the timer
    /// and the proactive toggle say, and on whatever the request narrows it to. Cleared once
    /// the attempt is actually made, so a force arriving while a wake runs lands on the next
    /// pass rather than being swallowed.
    forced: Option<ForcedWake>,
    /// Sweeps the user turned something down in, waiting out their coalescing window. One
    /// entry per sweep, which is what makes "reject all" one model call.
    follow_ups: FollowUpQueue,
}

impl WakeLoop {
    /// How long to park before the next thing that could need doing.
    ///
    /// Two clocks now: the inbox's next deadline and the earliest rejection whose coalescing
    /// window is about to close. Parking past the second would leave a follow-up sitting until
    /// something unrelated woke the loop.
    fn park(&self) -> Duration {
        let now = now_secs();
        park_with_follow_up(
            park_for(self.inbox.next_deadline(), self.not_before, now),
            self.follow_ups.next_due(),
            now,
        )
    }

    /// Note that a group in this sweep was turned down, so the agent can ask about it.
    ///
    /// ⚠️ **A closed gate DROPS the ask rather than parking it.** "Why did you say no?" is only
    /// worth asking while the answer is still in the user's head, and a question that surfaces
    /// the week they finally set an API key reads as the app having been sitting on it.
    fn note_rejection(&mut self, set_id: i64) {
        if !followup::may_ask(&self.settings, readiness_snapshot()) {
            log::debug!(target: LOG_TARGET, "a rejection went unasked about: the agent is not allowed to speak");
            return;
        }
        self.follow_ups.note(set_id, now_secs());
    }

    /// Fold one rollup into the inbox and write the row it touched.
    ///
    /// The importance lookup happens HERE rather than at the tap: it is SQLite behind a shared
    /// cache, and the live loop may touch neither.
    fn admit(&mut self, activity: FolderActivity) {
        let now = now_secs();
        let importance = self
            .importance
            .lookup(&activity.volume_id, &activity.folder, Instant::now());
        let bundle = activity.into_bundle();
        let folder = bundle.folder.clone();
        let window_start = bundle.window_start;
        if !self
            .inbox
            .admit_if_permitted(readiness_snapshot(), bundle, importance, self.settings.hot_delay, now)
        {
            // Without consent the pipeline stores NOTHING, and that is the whole gate.
            return;
        }
        let touched = self
            .inbox
            .rows()
            .iter()
            .find(|row| row.bundle.folder == folder && row.bundle.window_start == window_start);
        if let Some(row) = touched
            && let Err(e) = persist::save_row(&self.conn, row)
        {
            log::warn!(target: LOG_TARGET, "an inbox row was not persisted, so a restart will forget it: {e}");
        }
    }

    fn handle_control(&mut self, control: WakeControl) {
        match control {
            WakeControl::SettingsChanged => self.reload_settings(),
            WakeControl::ReadinessChanged => self.purge_inbox_if_not_permitted(),
            WakeControl::WakeFinished => self.wake_in_flight = false,
            WakeControl::ForceWake(request) => self.forced = Some(request),
            WakeControl::SweepRejected { set_id } => self.note_rejection(set_id),
        }
        // Every control message is a reason the last decision may no longer hold: the gate the
        // wake was refused by may have opened, or the wake it was waiting on may have finished.
        self.not_before = 0;
    }

    /// Throw the backlog away, on disk as well as in memory, when the gates stopped permitting
    /// it to be stored.
    ///
    /// ⚠️ **The disk half is the point.** `agent_inbox` rows are folder paths, counts, and
    /// timestamps: a record of what the user has been doing. A revoke, or the bump that
    /// un-accepts everybody when the consent copy changes, withdraws the purpose that record
    /// was kept for, so it goes rather than sitting there until somebody re-accepts.
    fn purge_inbox_if_not_permitted(&mut self) {
        let dropped = self.inbox.purge_if_not_permitted(readiness_snapshot());
        if dropped == 0 {
            return;
        }
        log::info!(
            target: LOG_TARGET,
            "{dropped} waiting inbox row(s) were dropped: nobody has consented to a record of them being kept"
        );
        if let Err(e) = persist::clear(&self.conn) {
            log::warn!(target: LOG_TARGET, "the unconsented inbox rows are still on disk: {e}");
        }
    }

    /// Re-read the cadence and the proactive gate, and push the new cadence across the rows
    /// already waiting.
    ///
    /// ⚠️ **The re-pricing half is not optional.** `Inbox::admit` merges min-only (the
    /// starvation guard), so a LENGTHENED cadence would reach only bundles that arrive AFTER
    /// the change: somebody who asks for a calmer agent would keep being woken on the old
    /// schedule by everything already queued. The park is recomputed by the loop right after
    /// this returns, so a moved deadline re-arms the timer with it.
    fn reload_settings(&mut self) {
        let previous = self.settings;
        self.settings = settings::load(&self.app);
        if previous.hot_delay != self.settings.hot_delay {
            self.inbox.reprice(previous.hot_delay, self.settings.hot_delay);
            if let Err(e) = persist::save_all(&self.conn, &self.inbox) {
                log::warn!(target: LOG_TARGET, "the re-priced inbox was not written back: {e}");
            }
        }
    }

    /// Say how much the bound cost, if anything. Silent when nothing dropped, so a quiet run
    /// stays quiet.
    fn report_dropped_rollups(&self) {
        let dropped = channel::take_dropped_rollups();
        if dropped > 0 {
            log::warn!(
                target: LOG_TARGET,
                "{dropped} folder rollup(s) were dropped: the wake loop fell behind a burst. \
                 Signal only — those folders will change again."
            );
        }
    }

    /// Prepare a wake if one is due, and hand it to its own thread.
    fn try_wake(&mut self) {
        let now = now_secs();
        let forced = self.forced.is_some();
        if !forced && (now < self.not_before || !self.inbox.due_at(now)) {
            return;
        }
        // ⚠️ Whatever happens below, don't come straight back: the deadline that brought us here
        // has passed and will keep having passed, and a zero-length park is a spin. This is set
        // BEFORE every remaining early return, the in-flight one included.
        self.not_before = now.saturating_add(DECLINED_WAKE_BACKOFF.as_secs());

        // At most one wake in flight. A second prepared while the first runs would queue behind
        // the same conversation lock for a whole model call, holding rows the first could have
        // carried. `WakeFinished` clears the stamp above, so the next one starts as soon as this
        // one ends rather than waiting out the backoff. A force outlives the wait: the flag
        // stays set so the request lands once the running wake reports finished.
        if self.wake_in_flight {
            return;
        }
        let request = self.forced.take();

        // The fourth gate, beside the three in `readiness.rs`. Checked before anything is
        // resolved, so an opted-out user's inbox costs nothing beyond the rows. Silent: this is
        // the shipped default, and one log line every five minutes for it would be noise. A
        // forced wake is a developer asking for one, so it is the one thing that skips it — the
        // three gates that protect the USER are still checked below.
        if !forced && !self.settings.proactive {
            return;
        }
        let readiness = readiness_snapshot();
        if !readiness.may_wake() {
            runner::record_outcome("not_ready", None, self.inbox.len(), 0);
            return;
        }

        // Resolved BEFORE the thread is opened, so a wake with nowhere to think declines
        // without leaving an empty conversation behind.
        let Some(slot) = self.resolve_slot() else {
            runner::record_outcome("unavailable", None, self.inbox.len(), 0);
            return;
        };

        // Here rather than where the force arrived: a force held behind a running wake waits
        // out a whole model call, and the inbox keeps filling for all of it.
        if let Some(only_folder) = request.as_ref().and_then(|request| request.only_folder.as_deref()) {
            self.isolate_inbox_to(only_folder);
        }

        match prepare_wake(
            &self.conn,
            &mut self.inbox,
            &PrepareParams {
                readiness,
                now_secs: now as i64,
                digest_budget_tokens: budget::wake_digest_budget(slot.prompt_budget),
                ignore_deadlines: forced,
            },
        ) {
            PrepareOutcome::Ready(prepared) => {
                self.wake_in_flight = true;
                runner::spawn(
                    self.app.clone(),
                    slot.into_resolved(prepared.conversation_id),
                    BackgroundTurn::Wake(prepared),
                );
            }
            PrepareOutcome::NotReady(gap) => {
                log::debug!(target: LOG_TARGET, "the wake gate closed between the check and the prepare: {gap:?}");
                runner::record_outcome("not_ready", None, self.inbox.len(), 0);
            }
            PrepareOutcome::NothingDue => runner::record_outcome("nothing_due", None, self.inbox.len(), 0),
            PrepareOutcome::Unavailable => runner::record_outcome("unavailable", None, self.inbox.len(), 0),
        }
    }

    /// Cut the inbox down to the one folder a forced wake staged, on disk as well as in memory.
    ///
    /// ⚠️ **Reachable only from a `playwright-e2e` force** (`ForcedWake::only_folder`), which is
    /// the one caller that can say what the wake is supposed to cover. A test's premise is "the
    /// digest reports what I staged", and the indexer's tap feeds this same inbox from whatever
    /// else the suite is doing, so the rows it put there are dropped rather than reported on.
    fn isolate_inbox_to(&mut self, folder: &str) {
        let dropped = self.inbox.retain_folder(folder);
        if dropped == 0 {
            return;
        }
        log::debug!(target: LOG_TARGET, "a forced wake dropped {dropped} inbox row(s) it did not stage");
        if let Err(e) = persist::save_all(&self.conn, &self.inbox) {
            log::warn!(target: LOG_TARGET, "the rows a forced wake dropped are still on disk: {e}");
        }
    }

    /// Ask about one sweep the user turned down, if its coalescing window has closed.
    ///
    /// ⚠️ **Shares `wake_in_flight` with a wake.** At most one background turn at a time,
    /// whichever kind: two would queue behind each other's conversation locks and spend the
    /// user's money in parallel for no benefit. The queue keeps until the running one reports
    /// finished.
    fn try_follow_up(&mut self) {
        if self.wake_in_flight {
            return;
        }
        // Re-checked here as well as at the ask: a gate can close during the window.
        if !followup::may_ask(&self.settings, readiness_snapshot()) {
            self.follow_ups.clear();
            return;
        }
        let now = now_secs();
        let Some((set_id, since)) = self.follow_ups.take_due(now) else {
            return;
        };
        let Some(prepared) = followup::prepare(&self.conn, set_id, since as i64) else {
            return;
        };
        // Resolved the same way a wake resolves it, and only once there is something to say, so
        // a follow-up with nowhere to think costs nothing.
        let Some(slot) = self.resolve_slot() else {
            runner::record_outcome("followup_unavailable", None, 0, 0);
            return;
        };
        self.wake_in_flight = true;
        runner::spawn(
            self.app.clone(),
            slot.into_resolved(prepared.conversation_id),
            BackgroundTurn::FollowUp(prepared),
        );
    }

    /// The provider, the model, and the budget this wake would think with — resolved the same
    /// way a rail send resolves them, and read FRESH, so a wake never thinks with a different
    /// window than the rail would.
    fn resolve_slot(&self) -> Option<PendingSlot> {
        let (llm, provider, model) = match resolve_agent_llm(&self.app, AgentSlot::Wake) {
            Ok(resolved) => resolved,
            Err(kind) => {
                log::debug!(target: LOG_TARGET, "a wake came due with no provider configured: {kind:?}");
                return None;
            }
        };
        let prompt_budget = match resolve_prompt_budget(&self.app, provider, &model) {
            Ok(tokens) => tokens,
            Err(refusal) => {
                log::warn!(target: LOG_TARGET, "a wake came due with a budget it cannot use: {refusal:?}");
                return None;
            }
        };
        Some(PendingSlot {
            llm,
            provider,
            model,
            prompt_budget,
        })
    }
}

/// A resolved slot whose LLM is not built yet, because the conversation id it must be keyed on
/// does not exist until the prepare step succeeds.
struct PendingSlot {
    llm: crate::agent::chat::session::ResolvedAgentLlm,
    provider: crate::agent::llm::types::ProviderTag,
    model: String,
    prompt_budget: usize,
}

impl PendingSlot {
    fn into_resolved(self, conversation_id: i64) -> ResolvedSlot {
        ResolvedSlot {
            llm: self.llm.into_llm(conversation_id),
            provider: self.provider,
            model: self.model,
            prompt_budget: self.prompt_budget,
        }
    }
}

/// How long to park, given what is waiting and when a wake may next be attempted.
///
/// ⚠️ Capped at [`IDLE_POLL`] and floored by `not_before`. The floor is the load-bearing half: a
/// deadline in the past yields a zero-length park, and a zero-length `recv_timeout` returns
/// instantly, so without it an overdue row the loop declines to act on spins a core flat.
/// Fold a waiting follow-up's coalescing window into the park the inbox asked for.
///
/// ⚠️ **An OVERDUE follow-up must not shorten the park**, which is the same spin trap
/// [`park_for`] guards for the inbox and it bites for a different reason. A window that has
/// closed and is still waiting means this pass declined to act on it, and the only reason it
/// can is that a background turn is already running. That turn takes minutes, and a
/// zero-length `recv_timeout` for its whole duration would spin a core flat. The
/// `WakeFinished` control message wakes the loop the moment it can be acted on.
fn park_with_follow_up(wake: Duration, next_follow_up: Option<u64>, now: u64) -> Duration {
    match next_follow_up {
        Some(due) if due > now => wake.min(Duration::from_secs(due - now)),
        _ => wake,
    }
}

fn park_for(next_deadline: Option<u64>, not_before: u64, now: u64) -> Duration {
    let Some(due) = next_deadline else {
        return IDLE_POLL;
    };
    Duration::from_secs(due.max(not_before).saturating_sub(now)).min(IDLE_POLL)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ The spin guard. A deadline that has passed keeps having passed, so a park computed
    /// from it alone is zero-length, and a zero-length `recv_timeout` returns instantly. An
    /// overdue row the loop declines to act on is the ordinary state for anybody without
    /// consent or an API key — this is the difference between a parked thread and a hot core.
    #[test]
    fn an_overdue_row_the_loop_declined_parks_instead_of_spinning() {
        let overdue = Some(1_780_000_000);
        let now = 1_780_000_500;

        assert_eq!(park_for(overdue, 0, now), Duration::ZERO, "the deadline alone says now");
        assert_eq!(
            park_for(overdue, now + 30, now),
            Duration::from_secs(30),
            "the backoff is what keeps the thread asleep"
        );
        assert_eq!(
            park_for(overdue, now + DECLINED_WAKE_BACKOFF.as_secs(), now),
            IDLE_POLL,
            "a longer backoff still re-checks once a minute; `try_wake`'s own guard holds the rest"
        );
    }

    /// A deadline still ahead is honoured to the second, so the agent stays as attentive as the
    /// cadence setting asks.
    #[test]
    fn a_future_deadline_is_parked_for_exactly() {
        assert_eq!(park_for(Some(1_780_000_030), 0, 1_780_000_000), Duration::from_secs(30));
    }

    /// An empty inbox, or one holding only cold rows, waits out the idle poll rather than
    /// forever: a clock jump or a missed re-arm then costs a minute, not the rest of the run.
    #[test]
    fn nothing_waiting_falls_back_to_the_idle_poll() {
        assert_eq!(park_for(None, 0, 1_780_000_000), IDLE_POLL);
        assert_eq!(
            park_for(Some(1_780_009_999), 0, 1_780_000_000),
            IDLE_POLL,
            "and a distant deadline is capped, so the loop re-checks its own arithmetic"
        );
    }

    /// A rejection's coalescing window is the second clock the loop parks against, so a window
    /// closing soon has to shorten the wait: otherwise the ask sits until something unrelated
    /// wakes the loop.
    #[test]
    fn a_coalescing_window_closing_soon_shortens_the_park() {
        let now = 1_780_000_000;

        assert_eq!(
            park_with_follow_up(IDLE_POLL, Some(now + 5), now),
            Duration::from_secs(5)
        );
        assert_eq!(
            park_with_follow_up(Duration::from_secs(2), Some(now + 5), now),
            Duration::from_secs(2),
            "and the shorter of the two clocks wins"
        );
    }

    /// ⚠️ The follow-up half of the spin guard. A window that closed and is STILL waiting means
    /// a background turn is running, and that turn takes minutes: a zero-length park for its
    /// whole duration would spin a core flat. `WakeFinished` is what wakes the loop instead.
    #[test]
    fn an_overdue_follow_up_the_loop_declined_parks_instead_of_spinning() {
        let now = 1_780_000_000;

        assert_eq!(park_with_follow_up(IDLE_POLL, Some(now - 300), now), IDLE_POLL);
        assert_eq!(park_with_follow_up(IDLE_POLL, Some(now), now), IDLE_POLL);
        assert_eq!(park_with_follow_up(IDLE_POLL, None, now), IDLE_POLL);
    }
}
