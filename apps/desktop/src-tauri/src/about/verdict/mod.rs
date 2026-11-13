//! The card's verdict: two or three sentences a model writes from the card's facts,
//! streamed onto the card's own channel beside them.
//!
//! The order is the consent guarantee: the gate (setting, then slot) → the reuse and the
//! start ceiling → the wait for the facts → the sensitive gate and the look inside → the
//! prompt → the call. A refusal anywhere before the call reads nothing for a model and
//! sends nothing. ❌ The verdict text is never stored and never logged: only its cost
//! reaches `main.db`, on the reserved `about_this` row. Docs: `../DETAILS.md` § The
//! verdict.

pub(crate) mod gate;
pub(crate) mod limits;
pub(crate) mod peek;
pub(crate) mod prompt;
mod watched;

#[cfg(test)]
mod tests;

use std::time::Duration;

use cmdr_about::fact::Fact;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use super::event::{AboutUpdate, VerdictContents, VerdictCost, VerdictEnding, VerdictProblem, VerdictUnavailable};
use super::model::{GateReach, for_model, gate_within};
use super::session::{ItemReach, PROBE_BUDGET};
use crate::agent::llm::types::{AgentDelta, AgentLlmError};
use gate::Ready;
use limits::{CacheKey, Finished, Limits};
use peek::ReadFn;
pub(crate) use watched::{Phase, Watched};

/// How long the verdict waits for the facts to settle before it goes with what's in. A
/// slow row (a big repo's status walk, a search on a busy machine) mustn't hold it.
pub(crate) const FACTS_CAP: Duration = Duration::from_millis(1_500);

/// How long the look inside may take.
pub(crate) const PEEK_BUDGET: Duration = Duration::from_secs(2);

/// What the gate answered for a card.
pub(crate) type Decision = Result<Ready, VerdictUnavailable>;

/// One card's verdict, ready to run beside its facts.
pub(crate) struct VerdictRun<'a> {
    pub(crate) request_id: String,
    pub(crate) volume_id: String,
    /// The resolver's path for the item.
    pub(crate) path: String,
    /// How far the sensitive gate may follow the path: only as far as the card reaches.
    pub(crate) gate_reach: GateReach,
    pub(crate) item: ItemReach,
    /// The card's own token: `about_cancel` (or quitting) trips it, and then the verdict
    /// sends nothing more.
    pub(crate) card: CancellationToken,
    /// The app's language, which the model answers in.
    pub(crate) locale: String,
    pub(crate) limits: &'a Limits,
    pub(crate) read: ReadFn,
    pub(crate) facts_cap: Duration,
}

/// Runs the verdict until it ends, the card closes, or something stops it. Every update
/// goes to `watched`'s card channel.
pub(crate) async fn run(verdict: VerdictRun<'_>, decision: Decision, watched: &Watched) {
    let started = tokio::time::Instant::now();
    let card = verdict.card.clone();
    // The plan goes first on the channel, so the verdict's own first word follows it.
    if !watched.wait_for(Phase::Planned, &card).await {
        return;
    }
    let ready = match decision {
        Ok(ready) => ready,
        Err(why) => {
            log::debug!(target: "about", "no verdict: {why:?}");
            watched.send_verdict(&card, AboutUpdate::VerdictUnavailable { why });
            return;
        }
    };

    let key = cache_key(&verdict, &ready, watched);
    if let Some(earlier) = verdict.limits.recall(&key) {
        log::debug!(target: "about", "the verdict is from earlier; no call");
        watched.send_verdict(&card, AboutUpdate::VerdictPlanned);
        watched.send_verdict(
            &card,
            AboutUpdate::VerdictFromEarlier {
                contents: earlier.contents,
                text: earlier.text,
            },
        );
        return;
    }
    if verdict.limits.admit(std::time::Instant::now()).is_err() {
        log::debug!(target: "about", "no verdict: too many started in the last minute");
        watched.send_verdict(
            &card,
            AboutUpdate::VerdictUnavailable {
                why: VerdictUnavailable::TooManyRequests,
            },
        );
        return;
    }
    watched.send_verdict(&card, AboutUpdate::VerdictPlanned);

    // Its own token under the card's: Stop, a consent revoke, a policy narrowing, or a
    // newer verdict trip only this one; closing the card trips both.
    let stop = card.child_token();
    let _claim = verdict.limits.claim(&verdict.request_id, stop.clone());

    tokio::select! {
        biased;
        () = stop.cancelled() => return stopped(watched, &card),
        _ = watched.wait_for(Phase::Settled, &stop) => {}
        () = tokio::time::sleep_until(started + verdict.facts_cap) => {}
    }

    let gated = gate_within(&verdict.path, verdict.gate_reach, PROBE_BUDGET).await;
    if stop.is_cancelled() {
        return stopped(watched, &card);
    }
    let (contents, peek) = match gated.protected() {
        Some(_) => {
            if let Some(refusal) = gated.refusal() {
                crate::tools::sensitive::note_refusal(crate::security_log::ContentTool::AboutVerdict, refusal);
            }
            (VerdictContents::Protected, None)
        }
        None if verdict.item.bytes_readable => peek::look(&verdict.path, verdict.read.clone(), PEEK_BUDGET).await,
        None => (VerdictContents::None, None),
    };
    if stop.is_cancelled() {
        return stopped(watched, &card);
    }

    let snapshot = watched.card();
    let still_gathering = snapshot.pending();
    let projection = for_model(&snapshot, &verdict.path, gated);
    let message = prompt::user_message(&projection, &still_gathering, peek.as_ref(), &verdict.locale);
    let system = ready.variant.system();
    log::debug!(
        target: "about",
        "verdict going out: contents {contents:?}, {} row(s) still out, {:?}",
        still_gathering.len(),
        ready.variant
    );
    watched.send_verdict(&card, AboutUpdate::VerdictGiven { contents });

    let messages = [message];
    let opened = tokio::select! {
        biased;
        () = stop.cancelled() => return stopped(watched, &card),
        opened = ready.llm.respond(&system, &[], &messages, ready.call, stop.clone()) => opened,
    };
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(error) => return failed(watched, &card, &error),
    };
    let mut text = String::new();
    loop {
        let next = tokio::select! {
            biased;
            () = stop.cancelled() => return stopped(watched, &card),
            next = stream.next() => next,
        };
        match next {
            Some(Ok(AgentDelta::Text(piece))) => {
                text.push_str(&piece);
                watched.send_verdict(&card, AboutUpdate::VerdictDelta { text: piece });
            }
            Some(Ok(AgentDelta::End { usage, .. })) => {
                let meter = ready.meter.clone();
                let priced = tokio::task::spawn_blocking(move || meter(usage)).await.ok();
                let cost = priced.map(|priced| VerdictCost {
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    cost_micros: priced.cost_micros,
                    priced: priced.priced,
                });
                log::debug!(
                    target: "about",
                    "verdict done: {} chars, {} prompt and {} completion tokens",
                    text.chars().count(),
                    usage.prompt_tokens,
                    usage.completion_tokens
                );
                // A reasoning model can spend its whole reply on thinking and say nothing
                // (Haiku 5.5 at its default effort under a 120-token cap, the smoke run of
                // 2026-11-13): that's no verdict, ❌ never an empty "done", nor one cached.
                // Its input was still billed, so the cost rides along.
                let ending = if text.trim().is_empty() {
                    VerdictEnding::Failed {
                        problem: VerdictProblem::NoAnswer,
                    }
                } else {
                    verdict.limits.remember(key, Finished { contents, text });
                    VerdictEnding::Done
                };
                watched.send_verdict(&card, AboutUpdate::VerdictEnd { ending, cost });
                return;
            }
            Some(Ok(AgentDelta::ReasoningTick | AgentDelta::KeepAlive | AgentDelta::ToolCallStarted { .. })) => {}
            Some(Err(error)) => return failed(watched, &card, &error),
            // The stream ended with no `End`: its cancel fired.
            None => return stopped(watched, &card),
        }
    }
}

/// What makes a reopen the same question.
fn cache_key(verdict: &VerdictRun<'_>, ready: &Ready, watched: &Watched) -> CacheKey {
    let (modified, size) = match watched.card().identity {
        Some(Fact::Known(identity)) => (identity.modified, identity.logical_size),
        _ => (None, None),
    };
    CacheKey {
        volume_id: verdict.volume_id.clone(),
        path: verdict.path.clone(),
        node: verdict.item.node,
        modified,
        size,
        provider: ready.provider,
        model: ready.model.clone(),
    }
}

/// The verdict stopped before its end. A card that closed hears nothing; one still open
/// keeps the text so far.
fn stopped(watched: &Watched, card: &CancellationToken) {
    log::debug!(target: "about", "the verdict stopped");
    watched.send_verdict(
        card,
        AboutUpdate::VerdictEnd {
            ending: VerdictEnding::Stopped,
            cost: None,
        },
    );
}

fn failed(watched: &Watched, card: &CancellationToken, error: &AgentLlmError) {
    let problem = problem_of(error);
    log::debug!(target: "about", "the verdict's call failed: {problem:?}");
    watched.send_verdict(
        card,
        AboutUpdate::VerdictEnd {
            ending: VerdictEnding::Failed { problem },
            cost: None,
        },
    );
}

/// The provider's typed failure in the card's words.
fn problem_of(error: &AgentLlmError) -> VerdictProblem {
    match error {
        AgentLlmError::NoKey | AgentLlmError::AuthFailed(_) => VerdictProblem::AuthFailed,
        AgentLlmError::RateLimited(_) | AgentLlmError::ModelNotInPlan(_) => VerdictProblem::RateLimited,
        AgentLlmError::Unavailable | AgentLlmError::Timeout => VerdictProblem::Unreachable,
        AgentLlmError::Overloaded { .. } => VerdictProblem::Overloaded,
        AgentLlmError::Managed(_) => VerdictProblem::Managed,
        AgentLlmError::NotConfigured
        | AgentLlmError::RequestTooLarge(_)
        | AgentLlmError::BudgetExhausted
        | AgentLlmError::Provider(_) => VerdictProblem::Other,
    }
}

/// Stops the verdict in flight: cloud AI turned off, a policy narrowed, the app is
/// quitting. The card stays open with the text so far.
pub(crate) fn stop_all() -> usize {
    limits::LIMITS.stop_all()
}
