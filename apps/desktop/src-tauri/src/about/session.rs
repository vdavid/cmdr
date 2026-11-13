//! One card's gathering, from the plan to `factsSettled`.
//!
//! `planned` goes out before any job is submitted. Jobs run on the about pool;
//! whatever answers inside the grace lands together, so a local card is whole on
//! its first frame, and each later answer lands by itself, in any order. A row
//! still out at its budget lands as `Unknown(TimedOut)`, and its job's late answer
//! is dropped. A cancel stops queued jobs and drops every late answer.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_about::actions::ActionCaps;
use cmdr_about::card::{AboutCard, AboutFact, AboutPlan, Applied};
use cmdr_about::fact::{Fact, FactKind, UnknownWhy};
use cmdr_about::gather::{Job, Task, start_with};
use cmdr_about::headline::headline;
use cmdr_about::removal::consequences;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::event::{AboutUpdate, Derived};
use crate::ignore_poison::IgnorePoison;

/// How long a card waits for its jobs before its first frame goes up with
/// whatever answered, the context menu's grace. On a local disk every cheap row
/// answers well inside it.
pub(crate) const GRACE: Duration = Duration::from_millis(100);

/// How long the look before the plan may take: one `lstat`, a mount-table read,
/// and for a dotted folder one package lookup. Past it the volume isn't
/// answering.
pub(crate) const PROBE_BUDGET: Duration = Duration::from_millis(1_500);

/// Where a card's updates go. `false`: nobody is listening any more.
pub(crate) trait AboutSink: Send + Sync + 'static {
    fn send(&self, update: AboutUpdate) -> bool;
}

/// Runs a job somewhere other than the caller: the about pool, or a test's threads.
pub(crate) type Submit = Arc<dyn Fn(Task) + Send + Sync>;

/// A card ready to gather: its plan, the rows already answered, and a job per
/// row still out.
pub(crate) struct Gathered {
    pub(crate) plan: AboutPlan,
    /// Answered before any job ran (the probe's identity).
    pub(crate) ready: Vec<AboutFact>,
    pub(crate) jobs: Vec<(FactKind, Job<AboutFact>)>,
    /// What a row still out at the budget means.
    pub(crate) reach: Watch,
    /// What the item's volume lets the footer's actions do.
    pub(crate) actions: ActionCaps,
}

/// What a row still out at its budget means, by how the card reaches the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Watch {
    /// The OS answers each row itself: one still out timed out, alone.
    EachRow,
    /// Every row asks the item's volume, and the identity row is its own
    /// metadata call, the card's health check. Still out at the volume budget:
    /// the volume stopped answering, so the card says so once and every row still
    /// waiting lands `VolumeStoppedAnswering`.
    TheVolume,
}

/// How long the card waits, injectable so a test can run on milliseconds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub(crate) grace: Duration,
    /// A local row's budget ([`Watch::EachRow`]).
    pub(crate) row_budget: Duration,
    /// How long a volume has to answer before the card says it stopped
    /// ([`Watch::TheVolume`]).
    pub(crate) volume_budget: Duration,
}

/// The card as it's being filled, shared with the late route.
struct Session {
    state: Mutex<State>,
    /// What the item's volume lets the footer's actions do, for every derive.
    actions: ActionCaps,
    sink: Arc<dyn AboutSink>,
    cancel: CancellationToken,
    settled: Notify,
}

struct State {
    card: AboutCard,
    /// Rows that have their answer: a later one for them is dropped.
    answered: BTreeSet<FactKind>,
    /// `factsSettled` went out, or the card is gone: nothing more lands.
    done: bool,
}

impl Session {
    fn send(&self, update: AboutUpdate) {
        if !self.sink.send(update) {
            // The card's channel is gone without a cancel: stop the work anyway.
            self.cancel.cancel();
        }
    }

    /// Puts one answer on the card and tells the card, unless the card is gone,
    /// already settled, or already has this row's answer.
    fn land(&self, fact: AboutFact) {
        let mut state = self.state.lock_ignore_poison();
        let kind = fact.kind();
        if state.done || self.cancel.is_cancelled() || state.answered.contains(&kind) {
            return;
        }
        let outcome = outcome_of(&fact);
        if state.card.apply(fact.clone()) == Applied::NotPlanned {
            log::warn!(target: "about", "a {kind:?} answer has no row on this card; dropped");
            return;
        }
        state.answered.insert(kind);
        log::debug!(target: "about", "{kind:?} landed: {outcome}");
        let derived = derive(&state.card, &self.actions);
        self.send(AboutUpdate::Fact { fact, derived });
        self.settle_if_complete(&mut state);
    }

    /// Gives every row still out `Unknown(why)`.
    fn settle_rest(&self, why: UnknownWhy) {
        let mut state = self.state.lock_ignore_poison();
        self.settle_rest_locked(&mut state, why);
    }

    fn settle_rest_locked(&self, state: &mut State, why: UnknownWhy) {
        if state.done || self.cancel.is_cancelled() {
            return;
        }
        for fact in state.card.settle_pending(why) {
            let kind = fact.kind();
            log::debug!(target: "about", "{kind:?} landed: unknown ({why:?})");
            state.answered.insert(kind);
            let derived = derive(&state.card, &self.actions);
            self.send(AboutUpdate::Fact { fact, derived });
        }
        self.settle_if_complete(state);
    }

    /// The budget ran out with rows still out. Through the volume with no
    /// identity yet, the volume stopped answering: the card says so once, then
    /// every row still waiting says the same quietly. Otherwise each row timed
    /// out. One lock for the whole decision, so an identity landing at the last
    /// moment is never followed by "the volume stopped".
    fn at_budget(&self, reach: Watch) {
        let mut state = self.state.lock_ignore_poison();
        if state.done || self.cancel.is_cancelled() {
            return;
        }
        let volume_silent = reach == Watch::TheVolume && state.card.pending().contains(&FactKind::Identity);
        if volume_silent {
            log::debug!(target: "about", "the item's volume didn't answer within its budget");
            self.send(AboutUpdate::VolumeStoppedAnswering);
            self.settle_rest_locked(&mut state, UnknownWhy::VolumeStoppedAnswering);
        } else {
            self.settle_rest_locked(&mut state, UnknownWhy::TimedOut);
        }
    }

    fn settle_if_complete(&self, state: &mut State) {
        if !state.done && state.card.pending().is_empty() {
            state.done = true;
            self.send(AboutUpdate::FactsSettled);
            self.settled.notify_one();
        }
    }

    fn is_done(&self) -> bool {
        self.state.lock_ignore_poison().done
    }

    fn close(&self) {
        self.state.lock_ignore_poison().done = true;
    }
}

/// The headline, removal row, and actions for the card as it stands.
fn derive(card: &AboutCard, actions: &ActionCaps) -> Derived {
    Derived {
        headline: headline(card),
        removal: consequences(card),
        actions: cmdr_about::actions::actions(card, actions),
    }
}

/// A fact's outcome for a log line: its state and reason, ❌ never its value (a
/// download address or a sender in `cmdr.log` rides the next error report).
fn outcome_of(fact: &AboutFact) -> String {
    fn state<T>(fact: &Fact<T>) -> String {
        match fact {
            Fact::Known(_) => "known".to_string(),
            Fact::Unknown(unknown) => format!("unknown ({:?})", unknown.why()),
            Fact::NotApplicable(why) => format!("not applicable ({why:?})"),
            Fact::Pending => "pending".to_string(),
        }
    }
    match fact {
        AboutFact::Identity(fact) => state(fact),
        AboutFact::RealType(fact) => state(fact),
        AboutFact::Bundle(fact) => state(fact),
        AboutFact::Signature(fact) => state(fact),
        AboutFact::Download(fact) => state(fact),
        AboutFact::CmdrOperation(fact) => state(fact),
        AboutFact::LastOpened(fact) => state(fact),
        AboutFact::DefaultApp(fact) => state(fact),
        AboutFact::Importance(fact) => state(fact),
        AboutFact::Owner(fact) => state(fact),
        AboutFact::Git(fact) => state(fact),
        AboutFact::RecursiveSize(fact) => state(fact),
        AboutFact::FolderDigest(fact) => state(fact),
        AboutFact::AppData(fact) => state(fact),
        AboutFact::GitSnapshot(fact) => state(fact),
    }
}

/// Gathers `gathered` into `sink` until every row has its answer or `cancel`
/// trips. Returns once nothing more will be sent.
pub(crate) async fn run(
    gathered: Gathered,
    sink: Arc<dyn AboutSink>,
    cancel: CancellationToken,
    submit: Submit,
    timing: Timing,
) {
    let Gathered {
        plan,
        ready,
        jobs,
        reach,
        actions,
    } = gathered;
    let card = AboutCard::new(&plan);
    let session = Arc::new(Session {
        state: Mutex::new(State {
            card,
            answered: BTreeSet::new(),
            done: false,
        }),
        actions,
        sink,
        cancel: cancel.clone(),
        settled: Notify::new(),
    });
    if cancel.is_cancelled() {
        return;
    }
    let derived = derive(&session.state.lock_ignore_poison().card, &session.actions);
    session.send(AboutUpdate::Planned { plan, derived });
    for fact in ready {
        session.land(fact);
    }
    // A card whose every row the probe answered is settled already; one with no
    // rows at all settles here.
    {
        let mut state = session.state.lock_ignore_poison();
        session.settle_if_complete(&mut state);
    }
    if session.is_done() {
        return;
    }

    let started = tokio::time::Instant::now();
    let gathering = start_with(jobs, move |task| submit(task));
    let stop_jobs = gathering.cancel_handle();
    let late = Arc::clone(&session);
    let collected = tokio::task::spawn_blocking(move || {
        gathering.collect(std::time::Instant::now() + timing.grace, move |_kind, fact| {
            late.land(fact)
        })
    })
    .await;
    let Ok(collected) = collected else {
        // The collector panicked: nothing will route the answers, so say so.
        session.settle_rest(UnknownWhy::TimedOut);
        stop_jobs.cancel();
        return;
    };
    for (_, fact) in collected.ready {
        session.land(fact);
    }

    let budget_ends = started
        + match reach {
            Watch::EachRow => timing.row_budget,
            Watch::TheVolume => timing.volume_budget,
        };
    loop {
        if session.is_done() {
            break;
        }
        tokio::select! {
            () = cancel.cancelled() => {
                stop_jobs.cancel();
                session.close();
                return;
            }
            () = tokio::time::sleep_until(budget_ends) => {
                session.at_budget(reach);
            }
            () = session.settled.notified() => {}
        }
    }
    // Settled: a job still running past its budget answers into nothing.
    stop_jobs.cancel();
}
