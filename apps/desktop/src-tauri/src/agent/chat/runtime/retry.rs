//! One model call of a turn, opened, read to its end, and sent again while the provider is
//! overloaded and nothing of the reply has streamed yet.
//!
//! An overloaded provider (HTTP 503, Gemini's "high demand"; Anthropic's 529) usually takes the
//! same request a few seconds later, so the call is tried up to [`OVERLOAD_ATTEMPTS`] times,
//! with an exponential, jittered backoff (about 2, 4, then 8 s), or the provider's own
//! `Retry-After` capped at [`RETRY_AFTER_CAP`]. Each wait tells the rail
//! (`ProviderBusyRetrying`) and races Stop.
//!
//! ⚠️ **Only before anything streamed.** genai delivers the 503 as the stream's FIRST error,
//! after the call has opened, so the check sits on the consumed stream
//! ([`StreamOutcome::overloaded_before_streaming`]) as well as on a call that failed to open. A
//! reply that already showed text or a tool line is never sent again: the rail would show it
//! twice, and a tool call may already be under way.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::LOG_TARGET;
use super::consume::{StreamOutcome, consume_stream};
use super::events::{AgentChatEvent, ChatEventSink, emit};
use crate::agent::chat::context::{AssembledPrompt, STREAM_STALL_TIMEOUT};
use crate::agent::llm::types::AgentLlmError;
use crate::agent::llm::{AgentLlm, CallSettings};

/// How many times one model call is tried while the provider is overloaded: the first try plus
/// three retries.
pub(super) const OVERLOAD_ATTEMPTS: u32 = 4;

/// The wait before the first retry; each later one doubles it (2, 4, then 8 s), give or take
/// [`JITTER`].
const FIRST_BACKOFF: Duration = Duration::from_secs(2);

/// How far either side of its nominal wait a backoff may land, as a share of it, so many clients
/// turned away by the same overload don't all come back in the same second.
const JITTER: f64 = 0.25;

/// The longest a provider's `Retry-After` may hold the turn. Past it the user is better served by
/// the busy line and their own retry than by a rail that sits silent for minutes.
pub(super) const RETRY_AFTER_CAP: Duration = Duration::from_secs(30);

/// How one model call ended, for `drive` to act on.
pub(super) enum CallEnd {
    /// The call opened and its stream was read to its end (or its error, or its stall).
    Streamed(StreamOutcome),
    /// The call never opened, with this typed reason.
    OpenFailed(AgentLlmError),
    /// The call didn't open within the stall window.
    NeverOpened,
    /// Stop arrived before the call opened, or during a backoff.
    Cancelled,
}

impl CallEnd {
    /// The overload a retry may answer, with its `Retry-After`, or `None` for any other ending.
    fn overload(&self) -> Option<Option<Duration>> {
        match self {
            CallEnd::OpenFailed(AgentLlmError::Overloaded { retry_after, .. }) => Some(*retry_after),
            CallEnd::Streamed(outcome) => outcome.overloaded_before_streaming(),
            _ => None,
        }
    }
}

/// Make one model call of the turn, retrying it while the provider is overloaded and nothing has
/// streamed. `busy_retries` counts each retry, for the turn's analytics. `AssistantStarted` goes
/// out once, the first time the call opens.
#[allow(
    clippy::too_many_arguments,
    reason = "the seam, the prompt, the call settings, the sink, Stop, and the tally's counter are each their own concern"
)]
pub(super) async fn call_model(
    llm: &dyn AgentLlm,
    prompt: &AssembledPrompt,
    call: CallSettings,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
    busy_retries: &mut u32,
) -> CallEnd {
    let mut attempt = 1;
    let mut announced = false;
    loop {
        let end = call_once(llm, prompt, call, sink, cancel, &mut announced).await;
        let Some(retry_after) = end.overload() else {
            return end;
        };
        if attempt >= OVERLOAD_ATTEMPTS {
            log::warn!(target: LOG_TARGET, "the provider was still overloaded on try {attempt} of {OVERLOAD_ATTEMPTS}; ending the turn as busy");
            return end;
        }
        attempt += 1;
        *busy_retries = busy_retries.saturating_add(1);
        let wait = wait_before(attempt, retry_after, rand::random::<f64>());
        log::info!(
            target: LOG_TARGET,
            "the provider is overloaded; trying the call again in {} ms (try {attempt} of {OVERLOAD_ATTEMPTS})",
            wait.as_millis()
        );
        emit(
            sink,
            AgentChatEvent::ProviderBusyRetrying {
                attempt,
                attempts: OVERLOAD_ATTEMPTS,
            },
        );
        tokio::select! {
            biased;
            () = cancel.cancelled() => return CallEnd::Cancelled,
            () = tokio::time::sleep(wait) => {}
        }
    }
}

/// Open the call and read its stream, once. Raced against Stop: a provider slow to answer (a long
/// queue) would otherwise hold the turn, and its spend, until it did. Dropping the call drops the
/// request. And bounded by the stall window, so a call that never opens can't hold the turn
/// forever either.
async fn call_once(
    llm: &dyn AgentLlm,
    prompt: &AssembledPrompt,
    call: CallSettings,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
    announced: &mut bool,
) -> CallEnd {
    let opened = tokio::select! {
        biased;
        () = cancel.cancelled() => return CallEnd::Cancelled,
        opened = tokio::time::timeout(STREAM_STALL_TIMEOUT, llm.respond(
            &prompt.system,
            &prompt.tools,
            &prompt.messages,
            call,
            cancel.clone(),
        )) => opened,
    };
    let stream = match opened {
        Err(_) => return CallEnd::NeverOpened,
        Ok(Err(error)) => return CallEnd::OpenFailed(error),
        Ok(Ok(stream)) => stream,
    };
    if !*announced {
        *announced = true;
        emit(sink, AgentChatEvent::AssistantStarted);
    }
    CallEnd::Streamed(consume_stream(stream, sink, cancel).await)
}

/// How long to wait before try `attempt` (2 for the first retry): the provider's `Retry-After`
/// when it sent one, capped at [`RETRY_AFTER_CAP`], else the doubling backoff, moved by up to
/// [`JITTER`] either way. `unit` is a draw from `0.0..1.0`, passed in so the arithmetic is
/// testable.
fn wait_before(attempt: u32, retry_after: Option<Duration>, unit: f64) -> Duration {
    if let Some(asked) = retry_after {
        return asked.min(RETRY_AFTER_CAP);
    }
    let doublings = attempt.saturating_sub(2).min(16);
    let nominal = FIRST_BACKOFF.saturating_mul(1 << doublings);
    nominal.mul_f64(1.0 - JITTER + 2.0 * JITTER * unit.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The decided policy: about 2, 4, then 8 s between the four tries.
    #[test]
    fn the_backoff_doubles_from_two_seconds() {
        assert_eq!(wait_before(2, None, 0.5), Duration::from_secs(2));
        assert_eq!(wait_before(3, None, 0.5), Duration::from_secs(4));
        assert_eq!(wait_before(4, None, 0.5), Duration::from_secs(8));
    }

    /// The jitter moves a wait by at most a quarter either way, so a crowd turned away by one
    /// overload spreads out rather than coming back together.
    #[test]
    fn the_jitter_stays_within_a_quarter_of_the_wait() {
        assert_eq!(wait_before(3, None, 0.0), Duration::from_secs(3));
        assert_eq!(wait_before(3, None, 1.0), Duration::from_secs(5));
        assert_eq!(
            wait_before(3, None, 7.0),
            Duration::from_secs(5),
            "an out-of-range draw is clamped"
        );
    }

    /// A provider's own `Retry-After` wins, jitter-free, but never holds the turn past 30 s.
    #[test]
    fn retry_after_is_honored_up_to_its_cap() {
        assert_eq!(
            wait_before(2, Some(Duration::from_secs(5)), 0.9),
            Duration::from_secs(5)
        );
        assert_eq!(wait_before(2, Some(Duration::ZERO), 0.9), Duration::ZERO);
        assert_eq!(wait_before(2, Some(Duration::from_secs(600)), 0.9), RETRY_AFTER_CAP);
    }
}
