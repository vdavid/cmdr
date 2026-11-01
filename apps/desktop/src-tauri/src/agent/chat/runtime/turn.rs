//! The turn driver: one `respond`-to-answer loop, crash-safe and within budget.
//!
//! Pure of Tauri — it needs only the seams ([`AgentLlm`], [`ToolDispatcher`]), a write
//! `Connection`, and [`TurnParams`] — so it is fully unit-testable with a temp DB and fakes.
//! The crash / mid-stream persistence contract it implements is in the module docs
//! (`runtime/mod.rs`, cases (a)–(d)).

use std::path::Path;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::analytics;
use super::consume::{StreamOutcome, consume_stream};
use super::cost::meter_cost;
use super::dispatch::{CallContext, ToolDispatcher, dispatch_ok, dispatch_until_stopped};
use super::events::{AgentChatEvent, AgentErrorKind, ChatEventSink, ResolvedCall, emit};
use super::limits::{LoopGuard, RoundJudgement, unanswered_wrap_up, wrap_up_limit};
use super::repeats::{FailedCalls, Repeat};
use super::types::{TurnParams, TurnResult, TurnTally, UserTurn};
use super::{LOG_TARGET, context};
use crate::agent::chat::context::{
    BACKGROUND_MAX_TOOL_TURNS, ElisionFacts, PrefixInputs, STREAM_STALL_TIMEOUT, WrapUp, wrap_up_note,
};
use crate::agent::chat::message_view::{MessageView, to_message_view};
use crate::agent::chat::spend_pause::{PauseOutcome, next_pause_at};
use crate::agent::chat::system_prompt::system_prompt;
use crate::agent::line::{LineRef, outcome_of, repeat_marks, subject_of};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{
    AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentToolResult, AgentUsage, ToolDeclaration,
};
use crate::agent::store::{self, AgentStoreError, TurnLimit, TurnLimitKind, TurnStats};

/// Drive one turn to completion, persisting crash-safely and staying within budget.
/// Pure of Tauri: it needs only the seams (`llm`, `dispatcher`), a write `Connection`,
/// and the params — so it is fully unit-testable with a temp DB and fakes.
///
/// Everything that happens ONCE per turn, however it ended, happens here rather than in
/// `drive`, because `drive` has a dozen early returns and this is the one place all of them
/// meet: the `TurnFinished` row, the ONE terminal event (`Done`, `Failed`, or `Stopped`, each
/// carrying what the turn spent), and the anonymous `ask_cmdr_turn` event (`analytics.rs`).
/// Scattering any of them across those returns is how one gets missed.
pub async fn run_turn(
    llm: &dyn AgentLlm,
    dispatcher: &dyn ToolDispatcher,
    conn: &rusqlite::Connection,
    tools: &[ToolDeclaration],
    params: &TurnParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> TurnResult {
    let started = Instant::now();
    let mut tally = TurnTally::default();
    let end = drive(llm, dispatcher, conn, tools, params, sink, cancel, &mut tally).await;
    let wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let stats = tally.stats(wall_ms, matches!(end, TurnEnd::Cancelled));
    if let Some(stats) = stats {
        record_turn_finished(conn, params, stats);
    }
    let result = end.result();
    emit(sink, end.into_event(stats));
    analytics::turn_finished(params, &result, &tally);
    result
}

/// How `drive` ended, before `run_turn` turns it into the turn's one terminal event.
enum TurnEnd {
    Answered {
        message_id: i64,
        seq: i64,
        stop: AgentStopReason,
        usage: AgentUsage,
    },
    /// `detail` is the source error's own wording, for display.
    Failed {
        kind: AgentErrorKind,
        detail: Option<String>,
    },
    Cancelled,
}

impl TurnEnd {
    fn failed(kind: AgentErrorKind) -> Self {
        TurnEnd::Failed { kind, detail: None }
    }

    fn result(&self) -> TurnResult {
        match self {
            TurnEnd::Answered { message_id, .. } => TurnResult::Answered {
                assistant_message_id: *message_id,
            },
            TurnEnd::Failed { kind, .. } => TurnResult::Failed(*kind),
            TurnEnd::Cancelled => TurnResult::Cancelled,
        }
    }

    fn into_event(self, turn: Option<TurnStats>) -> AgentChatEvent {
        match self {
            TurnEnd::Answered {
                message_id,
                seq,
                stop,
                usage,
            } => AgentChatEvent::Done {
                message_id,
                seq,
                stop,
                usage,
                turn,
            },
            TurnEnd::Failed { kind, detail } => AgentChatEvent::Failed { kind, detail, turn },
            TurnEnd::Cancelled => AgentChatEvent::Stopped { turn },
        }
    }
}

/// Persist what a tool-running turn spent, after its last row. A persist problem is logged and
/// dropped: a footer is worth no turn, and a reload rebuilds all but the wall time from the rows.
fn record_turn_finished(conn: &rusqlite::Connection, params: &TurnParams<'_>, stats: TurnStats) {
    let event = store::ConversationEvent::TurnFinished { stats };
    if let Err(e) = store::append_event(conn, params.conversation_id, &event, params.now_secs) {
        log::warn!(target: LOG_TARGET, "could not record what this turn spent: {e}");
    }
}

/// The loop itself. `tally` counts as it goes, so an early return still reports the
/// numbers the turn actually reached.
#[allow(
    clippy::too_many_arguments,
    reason = "the seams, the store, the params, and the tally are each a separate concern; bundling them \
              into a struct would only move the list"
)]
async fn drive(
    llm: &dyn AgentLlm,
    dispatcher: &dyn ToolDispatcher,
    conn: &rusqlite::Connection,
    tools: &[ToolDeclaration],
    params: &TurnParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
    tally: &mut TurnTally,
) -> TurnEnd {
    // The working transcript mirrors the durable rows; assembly reads it, persistence
    // writes the DB. Load the persisted history, then (for a new turn) the pending user
    // message — held in memory until the first `End` so a failed first attempt records
    // nothing (crash case b).
    let mut transcript = match load_transcript(conn, params.conversation_id) {
        Ok(history) => history,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "load transcript failed: {e}");
            return TurnEnd::Failed {
                kind: AgentErrorKind::Provider,
                detail: Some(e.to_string()),
            };
        }
    };
    // Built ONCE, from the history as it stood before this turn: the memory notes it carries
    // depend on what the thread already carried, and the row persisted later has to be the
    // exact message every call of this turn sent.
    let mut unpersisted_user = params.user.map(|user| user_parts(&user, params, &transcript));
    if let Some(parts) = &unpersisted_user {
        transcript.push(AgentMessage {
            role: AgentRole::User,
            parts: parts.clone(),
            at: params.now_secs,
        });
    }

    let mut slot_recorded = false;
    let mut trim_announced = false;
    // What already came back with a problem this turn, so an identical retry doesn't cost
    // another provider round trip for the same answer (`repeats.rs`).
    let mut failed_calls = FailedCalls::default();
    // Every call of this turn so far, with the row that made it, for the repeat marks: the same
    // function the reload projection runs over the stored rows, so the two can't disagree.
    let mut turn_calls: Vec<(i64, AgentToolCall)> = Vec::new();
    // A background turn has a round cap; one the user started has none (they watch it, Stop
    // ends it, and the loop guard and the spend pause bound it).
    let round_cap = (!params.origin.user_started()).then_some(BACKGROUND_MAX_TOOL_TURNS);
    // The loop guard's memory: how often each call was sent, and how many rounds in a row found
    // nothing new. Set when it fires, so the NEXT call is the wrap-up.
    let mut guard = LoopGuard::default();
    let mut guard_fired: Option<WrapUp> = None;
    // Where the spend pause next asks, `None` when this turn never pauses.
    let mut next_pause = params
        .spend_pause
        .as_ref()
        .map(|pause| next_pause_at(0, pause.threshold_micros));
    let memory_root = dispatcher.memory_root();
    // Read once per turn: a dev build reads the prompt file on each call, and every model call
    // of one turn must send the same prefix bytes.
    let system_prompt = system_prompt();

    loop {
        // Cancellation and every per-answer limit are checked at the top, so no `respond`
        // fires once the user cancelled, and past the round cap or the loop guard only the one
        // wrap-up call does: a runaway loop is impossible.
        if cancel.is_cancelled() {
            return TurnEnd::Cancelled;
        }
        // Past a limit: ONE more call, carrying the wrap-up note, whose reply ends the turn
        // whatever it holds. Every round's results were paid for, so the user gets an answer
        // from them rather than a notice over unread results.
        let wrap_up = guard_fired.or_else(|| {
            round_cap
                .filter(|cap| tally.tool_turns >= *cap)
                .map(|_| WrapUp::RoundCap)
        });
        if let Some(reason) = wrap_up {
            tally.limit = Some(wrap_up_limit(reason));
        } else if let (Some(pause), Some(at)) = (params.spend_pause.as_ref(), next_pause)
            && tally.cost_micros >= at
        {
            // Between rounds, where nothing runs: the paused turn spends nothing while it asks.
            emit(
                sink,
                AgentChatEvent::SpendPaused {
                    cost_micros: tally.cost_micros,
                },
            );
            match pause.slot.pause(tally.cost_micros, cancel).await {
                PauseOutcome::KeepGoing => {
                    emit(sink, AgentChatEvent::SpendResumed);
                    next_pause = Some(next_pause_at(tally.cost_micros, pause.threshold_micros));
                }
                PauseOutcome::Stopped => return TurnEnd::Cancelled,
            }
        }

        let prefix = PrefixInputs {
            system_prompt: &system_prompt,
            cmdr_md: params.cmdr_md,
            tools,
        };
        let mut assembled = context::assemble_prompt(
            &prefix,
            &transcript,
            params.envelope,
            params.offset,
            params.prompt_budget,
        );
        // The note rides this call only, after everything (so the cached prefix holds), and
        // never joins the transcript: the thread's history stays what was actually said.
        if let Some(reason) = wrap_up {
            assembled.messages.push(AgentMessage {
                role: AgentRole::User,
                parts: vec![AgentPart::Text(wrap_up_note(reason))],
                at: params.now_secs,
            });
        }
        announce_context_pressure(&assembled.elision, sink, &mut trim_announced);
        // A result the prompt dropped is a result the model never read: withdraw it as
        // evidence before the call goes out, so nothing downstream can cite its contents.
        if !assembled.elision.elided_call_ids.is_empty() {
            dispatcher.revoke_evidence(&assembled.elision.elided_call_ids);
        }

        // Raced against Stop: a provider slow to answer (a long queue) would otherwise hold the
        // turn, and its spend, until it did. Dropping the call drops the request. And bounded by
        // the stall window, so a call that never opens can't hold the turn forever either.
        let opened = tokio::select! {
            biased;
            () = cancel.cancelled() => return TurnEnd::Cancelled,
            opened = tokio::time::timeout(STREAM_STALL_TIMEOUT, llm.respond(
                &assembled.system,
                &assembled.tools,
                &assembled.messages,
                params.call,
                cancel.clone(),
            )) => opened,
        };
        let Ok(opened) = opened else {
            log::warn!(target: LOG_TARGET, "the model call didn't open within the stall window; ending it as stalled");
            return stalled(tally);
        };
        let stream = match opened {
            Ok(stream) => stream,
            Err(error) => {
                // The call never opened, so it never reached `End`: nothing is
                // persisted (crash case b). Surface the typed error plus the
                // provider's own wording for display.
                let detail = error.detail().map(str::to_string);
                return TurnEnd::Failed {
                    kind: AgentErrorKind::from(error),
                    detail,
                };
            }
        };

        emit(sink, AgentChatEvent::AssistantStarted);
        let StreamOutcome {
            final_message,
            stop,
            usage,
            stream_error,
            stalled: went_quiet,
        } = consume_stream(stream, sink, cancel).await;

        let mut message = match final_message {
            Some(message) => message,
            None => {
                // The stream ended without an `End`: partial assistant text is discarded
                // (crash case a). A user cancel is a clean stop, not a failure.
                if cancel.is_cancelled() {
                    return TurnEnd::Cancelled;
                }
                if went_quiet {
                    log::warn!(target: LOG_TARGET, "the model stream went quiet for the stall window; ending it as stalled");
                    return stalled(tally);
                }
                let detail = stream_error.as_ref().and_then(|e| e.detail().map(str::to_string));
                let kind = stream_error
                    .map(AgentErrorKind::from)
                    .unwrap_or(AgentErrorKind::UnfinishedReply);
                return TurnEnd::Failed { kind, detail };
            }
        };

        // `End` arrived, but the turn carries nothing at all: no text and no tool call.
        // That is what a degenerate provider reply reduces to once a nameless tool call is
        // dropped on the way in (`genai_impl`). Persisting it would show a blank bubble and
        // call it an answer, so it takes the dropped-stream path instead: nothing is
        // written, and a retry starts clean.
        if message.parts.is_empty() {
            log::warn!(target: LOG_TARGET, "assistant turn ended with no content; treating it as unfinished");
            return TurnEnd::failed(AgentErrorKind::UnfinishedReply);
        }

        // The wrap-up reply's tool calls never run, so they're never persisted either (a call
        // with no result row would break the next turn's replay). What prose it wrote stays;
        // with no prose left there's nothing to keep, and the turn ends on the limit.
        let wanted_more = wrap_up.is_some() && message.parts.iter().any(|part| matches!(part, AgentPart::ToolCall(_)));
        if let Some(reason) = wrap_up {
            message.parts.retain(|part| !matches!(part, AgentPart::ToolCall(_)));
            if !message.parts.iter().any(|part| matches!(part, AgentPart::Text(_))) {
                log::info!(target: LOG_TARGET, "the wrap-up reply still wanted a tool; ending on the limit");
                return TurnEnd::failed(unanswered_wrap_up(reason));
            }
        }

        // A completed `respond`: record what the slot changed (first `End` only, BEFORE the
        // user row so the event lines sit between the turns), persist the user row (first
        // `End` only), then the assistant row (content written only now), then meter this
        // call's cost.
        if !slot_recorded {
            slot_recorded = true;
            record_model_transition(conn, params, sink);
            record_chat_memory_transition(conn, params, sink);
        }
        if let Some(user) = params.user
            && let Some(parts) = unpersisted_user.take()
        {
            match store::append_message(
                conn,
                params.conversation_id,
                AgentRole::User,
                &parts,
                &user.search_text(),
                None,
                None,
                params.now_secs,
            ) {
                Ok((message_id, seq)) => {
                    let opener = opener_view(message_id, seq, parts, params.now_secs);
                    emit(sink, AgentChatEvent::UserPersisted { opener });
                }
                Err(e) => return persist_failed(e),
            }
        }

        let assistant_search = search_text(&message.parts);
        let (assistant_id, assistant_seq) = match store::append_message(
            conn,
            params.conversation_id,
            AgentRole::Assistant,
            &message.parts,
            &assistant_search,
            Some(usage.prompt_tokens),
            Some(usage.completion_tokens),
            params.now_secs,
        ) {
            Ok(ids) => ids,
            Err(e) => return persist_failed(e),
        };
        tally.prompt_tokens = tally.prompt_tokens.saturating_add(usage.prompt_tokens);
        tally.completion_tokens = tally.completion_tokens.saturating_add(usage.completion_tokens);
        transcript.push(message.clone());
        tally.cost_micros = tally.cost_micros.saturating_add(meter_cost(conn, params, usage));
        emit(sink, AgentChatEvent::CostMetered);
        report_context_usage(conn, params.conversation_id, &assembled.elision, sink);

        // Terminal vs. another tool turn.
        let calls: Vec<&AgentToolCall> = message
            .parts
            .iter()
            .filter_map(|part| match part {
                AgentPart::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect();
        if wanted_more && let Some(reason) = wrap_up {
            return TurnEnd::failed(unanswered_wrap_up(reason));
        }
        if calls.is_empty() {
            if let Some(limit) = tally.limit.as_mut() {
                limit.answered = true;
            }
            return TurnEnd::Answered {
                message_id: assistant_id,
                seq: assistant_seq,
                stop,
                usage,
            };
        }

        // Dispatch each tool call, persisting its result on its own row, then loop.
        tally.tool_turns += 1;
        turn_calls.extend(calls.iter().map(|call| (assistant_id, (*call).clone())));
        emit(
            sink,
            AgentChatEvent::ToolCallsResolved {
                message_id: assistant_id,
                calls: resolve_calls(assistant_id, &calls, &turn_calls, memory_root.as_deref()),
            },
        );
        // The loop guard judges the round once all its calls are answered, so no tool call is
        // left without a result row for the next turn to load.
        let mut round = RoundJudgement::default();
        for call in calls {
            let sent = guard.sent(call);
            let (result, proposal) = match failed_calls.judge(call) {
                Repeat::Fresh => {
                    let turn = CallContext {
                        origin: params.origin,
                        cancel,
                    };
                    let dispatch = dispatch_until_stopped(dispatcher, call, turn).await;
                    let ok = dispatch_ok(&dispatch.result);
                    if !ok {
                        failed_calls.record_failure(call, &dispatch.result.content);
                    }
                    round.note(sent, ok);
                    (dispatch.result, dispatch.proposal)
                }
                Repeat::Refuse(content) | Repeat::Stuck(content) => {
                    round.note(sent, false);
                    (repeat_result(call, content), None)
                }
            };
            let outcome = outcome_of(&call.tool, &result);
            let subject = subject_of(call, Some(&result.content), memory_root.as_deref());
            let tool_message = AgentMessage {
                role: AgentRole::Tool,
                parts: vec![AgentPart::ToolResult(result)],
                at: params.now_secs,
            };
            if let Err(e) = store::append_message(
                conn,
                params.conversation_id,
                AgentRole::Tool,
                &tool_message.parts,
                "",
                None,
                None,
                params.now_secs,
            ) {
                return persist_failed(e);
            }
            tally.tool_calls = tally.tool_calls.saturating_add(1);
            // Only now that the row exists: whoever fetches this result on the event finds it.
            emit(
                sink,
                AgentChatEvent::ToolCallFinished {
                    message_id: assistant_id,
                    call_id: call.call_id.clone(),
                    outcome,
                    subject,
                },
            );
            if let Some(proposal) = proposal {
                tally.proposals += 1;
                emit(sink, AgentChatEvent::ProposalReady { proposal });
            }
            transcript.push(tool_message);
        }

        // Going in circles, the model won't get past this on its own: the next call answers from
        // what it has, and the notice says why.
        guard_fired = guard.judge_round(round);
    }
}

/// End a call the provider went quiet on: the turn ends timed out, and its stats name the stall
/// window, so the rail says what happened and offers Keep going.
fn stalled(tally: &mut TurnTally) -> TurnEnd {
    tally.limit = Some(TurnLimit {
        kind: TurnLimitKind::Stalled,
        max: u32::try_from(STREAM_STALL_TIMEOUT.as_secs()).unwrap_or(u32::MAX),
        answered: false,
    });
    TurnEnd::failed(AgentErrorKind::Timeout)
}

/// One round's calls as `ToolCallsResolved` lists them: what each looks at from its arguments,
/// and the earlier identical call of the turn it repeats. `turn_calls` already holds this round.
fn resolve_calls(
    message_id: i64,
    calls: &[&AgentToolCall],
    turn_calls: &[(i64, AgentToolCall)],
    memory_root: Option<&Path>,
) -> Vec<ResolvedCall> {
    let ordered: Vec<(i64, &AgentToolCall)> = turn_calls.iter().map(|(id, call)| (*id, call)).collect();
    let marks = repeat_marks(&ordered);
    calls
        .iter()
        .map(|call| ResolvedCall {
            call_id: call.call_id.clone(),
            tool: call.tool.as_wire_name().to_string(),
            subject: subject_of(call, None, memory_root),
            repeat_of: marks
                .get(&LineRef {
                    message_id,
                    call_id: call.call_id.clone(),
                })
                .cloned(),
        })
        .collect()
}

/// A new user turn as it is sent AND persisted: the envelope it was sent with, rendered once,
/// then the memory files that changed since the thread last carried them (if any), then the
/// opener. Storing both is what lets every later call replay this turn byte for byte
/// (`context::build_messages`), so the provider's prompt cache holds across turns; re-rendering
/// the envelope would also move with the clock offset and the envelope's own format.
///
/// `history` is the thread before this turn. Memory sits HERE rather than in the system string
/// so that a note saved mid-chat moves only what comes after the last call
/// (`context::memory_notes_for_turn`).
fn user_parts(user: &UserTurn<'_>, params: &TurnParams<'_>, history: &[AgentMessage]) -> Vec<AgentPart> {
    let mut parts = vec![AgentPart::Envelope(context::render_envelope(
        params.envelope,
        params.offset,
    ))];
    parts.extend(context::memory_notes_for_turn(history, params.memory).map(AgentPart::MemoryNotes));
    parts.push(user.part());
    parts
}

/// The persisted opener as the view history shows, for `UserPersisted`.
fn opener_view(message_id: i64, seq: i64, parts: Vec<AgentPart>, created_at: i64) -> MessageView {
    to_message_view(store::StoredMessage {
        id: message_id,
        seq,
        content: store::StoredContent::Message {
            role: AgentRole::User,
            parts,
        },
        text_for_search: String::new(),
        prompt_tokens: None,
        completion_tokens: None,
        created_at,
    })
}

/// The tool result for a call the repeat breaker held back: it was never dispatched, but the
/// transcript still needs a result row against its call, and the model still needs to read
/// what the call came back with the first time.
fn repeat_result(call: &AgentToolCall, content: serde_json::Value) -> AgentToolResult {
    log::warn!(
        target: LOG_TARGET,
        "the model re-sent an identical {} call that already came back with a problem; not running it again",
        call.tool.as_wire_name() // allowed-log-private: an agent tool's wire name of our own
    );
    AgentToolResult {
        call_id: call.call_id.clone(),
        content,
        elided: false,
    }
}

/// Tell the user how full the model's view got on one completed call, and remember it with the
/// thread, so reopening the chat shows the last known figure instead of the baseline.
///
/// Called after every `End`, so a tool-running turn moves the gauge round by round; its last
/// call is its largest assembly (each tool result joins the same prompt), which is what the
/// thread keeps. A call that never reached `End` reports nothing: the user is looking at an
/// error line, and the previous call's stored figure stays the last thing actually measured. A
/// persist problem here is logged and dropped — a gauge is worth no turn.
fn report_context_usage(conn: &rusqlite::Connection, conversation_id: i64, facts: &ElisionFacts, sink: &ChatEventSink) {
    emit(
        sink,
        AgentChatEvent::ContextUsage {
            estimated_tokens: facts.estimated_tokens,
            budget_tokens: facts.budget,
            elided_results: facts.elided_results,
        },
    );
    if let Err(e) = store::set_conversation_context_usage(conn, conversation_id, facts.estimated_tokens, facts.budget) {
        log::warn!(target: LOG_TARGET, "could not record this turn's context usage: {e}");
    }
}

/// Make a context drop LOUD. Assembly is pure, so it hands back [`ElisionFacts`]; this is
/// where they become a log line and (once per turn) a user-visible notice.
///
/// Two distinct cases, on purpose:
/// - the budget forced history out (something the user may still be relying on left the
///   model's view) ⇒ warn + one `ContextTrimmed` event per turn;
/// - the prompt overran the budget with nothing safe left to drop ⇒ warn only. The rail
///   already nudges "this chat is getting long", and on a small local window this would
///   otherwise fire on every turn.
fn announce_context_pressure(facts: &ElisionFacts, sink: &ChatEventSink, announced: &mut bool) {
    if facts.budget_forced() {
        log::warn!(
            target: LOG_TARGET,
            "context budget dropped history: {} tool result(s) (~{} tokens) elided at threshold {}, prompt ~{} tokens against a {}-token budget",
            facts.elided_results,
            facts.elided_tokens,
            facts.threshold,
            facts.estimated_tokens,
            facts.budget
        );
        if !*announced {
            *announced = true;
            emit(
                sink,
                AgentChatEvent::ContextTrimmed {
                    elided_results: facts.elided_results,
                    approx_tokens: facts.elided_tokens,
                },
            );
        }
    }
    if facts.over_budget() {
        log::warn!(
            target: LOG_TARGET,
            "context over budget with nothing safe left to elide: prompt ~{} tokens against a {}-token budget (the turn's own results are never dropped)",
            facts.estimated_tokens,
            facts.budget
        );
    }
}

fn persist_failed(error: AgentStoreError) -> TurnEnd {
    log::warn!(target: LOG_TARGET, "persisting a chat message failed: {error}");
    TurnEnd::Failed {
        kind: AgentErrorKind::Provider,
        detail: Some(error.to_string()),
    }
}

/// On the turn's first completed `respond`: if the effective model differs from the
/// conversation's last recorded one, persist a UI-facing model-change event row (BEFORE
/// this turn's user row, so the line sits between the turns) and tell the live rail;
/// then stamp `last_model`. Running at the first `End` on purpose: a failed first
/// attempt records nothing (crash case b), and the next successful turn re-runs this
/// comparison — the event is deferred, never lost. A conversation with no recorded model
/// yet (its first turn) only stamps; there is nothing to switch from.
fn record_model_transition(conn: &rusqlite::Connection, params: &TurnParams<'_>, sink: &ChatEventSink) {
    let last = match store::conversation_last_model(conn, params.conversation_id) {
        Ok(last) => last,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading the conversation's last model failed: {e}");
            return;
        }
    };
    if last.as_deref() == Some(params.model.as_str()) {
        return;
    }
    if last.is_some() {
        let event = store::ConversationEvent::ModelChanged {
            model: params.model.clone(),
        };
        match store::append_event(conn, params.conversation_id, &event, params.now_secs) {
            Ok((message_id, seq)) => emit(
                sink,
                AgentChatEvent::ModelChanged {
                    message_id,
                    seq,
                    model: params.model.clone(),
                },
            ),
            Err(e) => log::warn!(target: LOG_TARGET, "recording the model-change event failed: {e}"),
        }
    }
    if let Err(e) = store::set_conversation_last_model(conn, params.conversation_id, &params.model) {
        log::warn!(target: LOG_TARGET, "stamping the conversation's model failed: {e}");
    }
}

/// The same comparison for the CHAT MEMORY SIZE: this turn's resolved prompt budget against
/// the one the conversation last used. A change means every reply from here on sees a
/// different amount of the chat, which a user who shrank the setting would otherwise only
/// notice as vaguer answers.
///
/// Its own comparison and its own event beside [`record_model_transition`], deliberately: a
/// model switch usually moves the budget too, and "switched to X" doesn't say by how much.
/// Both facets stamp only on the first `End`, so a failed attempt records neither and the
/// next successful turn re-runs both comparisons.
fn record_chat_memory_transition(conn: &rusqlite::Connection, params: &TurnParams<'_>, sink: &ChatEventSink) {
    let last = match store::conversation_last_chat_memory(conn, params.conversation_id) {
        Ok(last) => last,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading the conversation's last chat memory size failed: {e}");
            return;
        }
    };
    if last == Some(params.chat_memory_tokens) {
        return;
    }
    if last.is_some() {
        let event = store::ConversationEvent::ChatMemoryChanged {
            chat_memory_tokens: params.chat_memory_tokens,
        };
        match store::append_event(conn, params.conversation_id, &event, params.now_secs) {
            Ok((message_id, seq)) => emit(
                sink,
                AgentChatEvent::ChatMemoryChanged {
                    message_id,
                    seq,
                    chat_memory_tokens: params.chat_memory_tokens,
                },
            ),
            Err(e) => log::warn!(target: LOG_TARGET, "recording the chat-memory-change event failed: {e}"),
        }
    }
    if let Err(e) = store::set_conversation_last_chat_memory(conn, params.conversation_id, params.chat_memory_tokens) {
        log::warn!(target: LOG_TARGET, "stamping the conversation's chat memory size failed: {e}");
    }
}

/// Load a conversation's persisted messages as the working transcript. Event rows are
/// UI-facing timeline entries (a model or chat-memory change), NOT transcript content —
/// they never reach a provider, so they're filtered out here.
fn load_transcript(conn: &rusqlite::Connection, conversation_id: i64) -> Result<Vec<AgentMessage>, AgentStoreError> {
    const ALL: u32 = 10_000;
    let stored = store::list_messages(conn, conversation_id, ALL, 0)?;
    Ok(stored
        .into_iter()
        .filter_map(|m| match m.content {
            store::StoredContent::Message { role, parts } => Some(AgentMessage {
                role,
                parts,
                at: m.created_at,
            }),
            store::StoredContent::Event(_) => None,
        })
        .collect())
}

/// The FTS text for a message: its prose (user + assistant text parts) only, never a
/// tool blob or reasoning state. Extracted at the call site per the store contract.
fn search_text(parts: &[AgentPart]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            AgentPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
