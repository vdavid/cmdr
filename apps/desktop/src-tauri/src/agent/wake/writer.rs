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

use super::channel::{self, FolderActivity, WakeControl, WakeMessage};
use super::importance::ImportanceCache;
use super::runner::{self, ResolvedSlot};
use super::settings::{self, WakeSettings};
use super::snapshot::readiness_snapshot;
use super::{Inbox, PrepareOutcome, PrepareParams, persist, prepare_wake};
use crate::agent::chat::budget;
use crate::agent::chat::session::{resolve_agent_llm, resolve_prompt_budget};
use crate::agent::store;

const LOG_TARGET: &str = "agent::wake";

/// How long the loop waits with nothing scheduled. It would be correct to wait forever (every
/// arrival is a message), but a bounded park means a clock jump or a missed re-arm costs one
/// minute of latency rather than a loop that never wakes again.
const IDLE_POLL: Duration = Duration::from_secs(60);

/// ⚠️ **How long a declined attempt waits before trying again, and why the loop needs one at
/// all.** A deadline that has passed stays passed. Without a backoff the park would compute to
/// zero, `recv_timeout` would return instantly, and the loop would spin a core flat for as long
/// as an overdue row sits there — which is the DEFAULT state in M1, since `askCmdr.proactive`
/// ships false. A gate or settings change clears it, so opening the gate is felt at once.
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
        app,
        conn,
    };
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
}

impl WakeLoop {
    /// How long to park before the next thing that could need doing.
    fn park(&self) -> Duration {
        park_for(self.inbox.next_deadline(), self.not_before, now_secs())
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
            WakeControl::ReadinessChanged => {}
            WakeControl::WakeFinished => self.wake_in_flight = false,
        }
        // Every control message is a reason the last decision may no longer hold: the gate the
        // wake was refused by may have opened, or the wake it was waiting on may have finished.
        self.not_before = 0;
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
        if now < self.not_before || !self.inbox.due_at(now) {
            return;
        }
        // ⚠️ Whatever happens below, don't come straight back: the deadline that brought us here
        // has passed and will keep having passed, and a zero-length park is a spin. This is set
        // BEFORE every remaining early return, the in-flight one included.
        self.not_before = now.saturating_add(DECLINED_WAKE_BACKOFF.as_secs());

        // At most one wake in flight. A second prepared while the first runs would queue behind
        // the same conversation lock for a whole model call, holding rows the first could have
        // carried. `WakeFinished` clears the stamp above, so the next one starts as soon as this
        // one ends rather than waiting out the backoff.
        if self.wake_in_flight {
            return;
        }

        // The fourth gate, beside the three in `readiness.rs`. Checked before anything is
        // resolved, so an opted-out user's inbox costs nothing beyond the rows. Silent: this is
        // M1's default state, and one log line every five minutes for it would be noise.
        if !self.settings.proactive {
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

        match prepare_wake(
            &self.conn,
            &mut self.inbox,
            &PrepareParams {
                readiness,
                now_secs: now as i64,
                digest_budget_tokens: budget::wake_digest_budget(slot.prompt_budget),
            },
        ) {
            PrepareOutcome::Ready(prepared) => {
                self.wake_in_flight = true;
                runner::spawn(self.app.clone(), slot.into_resolved(prepared.conversation_id), prepared);
            }
            PrepareOutcome::NotReady(gap) => {
                log::debug!(target: LOG_TARGET, "the wake gate closed between the check and the prepare: {gap:?}");
                runner::record_outcome("not_ready", None, self.inbox.len(), 0);
            }
            PrepareOutcome::NothingDue => runner::record_outcome("nothing_due", None, self.inbox.len(), 0),
            PrepareOutcome::Unavailable => runner::record_outcome("unavailable", None, self.inbox.len(), 0),
        }
    }

    /// The provider, the model, and the budget this wake would think with — resolved the same
    /// way a rail send resolves them, and read FRESH, so a wake never thinks with a different
    /// window than the rail would.
    fn resolve_slot(&self) -> Option<PendingSlot> {
        let (llm, provider, model) = match resolve_agent_llm(&self.app) {
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
    /// from it alone is zero-length, and a zero-length `recv_timeout` returns instantly. With
    /// `askCmdr.proactive` shipping false, an overdue row the loop declines to act on is the
    /// DEFAULT state in M1 — this is the difference between a parked thread and a hot core.
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
}
