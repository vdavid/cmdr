//! The turn driver: one `respond`-to-answer loop, crash-safe and within budget.
//!
//! Pure of Tauri — it needs only the seams ([`AgentLlm`], [`ToolDispatcher`]), a write
//! `Connection`, and [`TurnParams`] — so it is fully unit-testable with a temp DB and fakes.
//! The crash / mid-stream persistence contract it implements is in the module docs
//! (`runtime/mod.rs`, cases (a)–(d)).

use futures_util::StreamExt;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::analytics;
use super::cost::meter_cost;
use super::dispatch::{ToolDispatcher, dispatch_ok};
use super::events::{AgentChatEvent, AgentErrorKind, ChatEventSink, emit};
use super::types::{TurnParams, TurnResult, TurnTally};
use super::{LOG_TARGET, context};
use crate::agent::chat::context::{ElisionFacts, MAX_TOOL_TURNS, MAX_WALL_TIME, PrefixInputs};
use crate::agent::chat::system_prompt::SYSTEM_PROMPT;
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentUsage, ToolDeclaration,
};
use crate::agent::store::{self, AgentStoreError};

/// Drive one turn to completion, persisting crash-safely and staying within budget.
/// Pure of Tauri: it needs only the seams (`llm`, `dispatcher`), a write `Connection`,
/// and the params — so it is fully unit-testable with a temp DB and fakes.
///
/// The anonymous `ask_cmdr_turn` event is reported here rather than in either caller,
/// because `drive` has a dozen early returns and this is the ONE place all of them meet.
/// Without it the agent's funnel starts at the proposal layer, where a zero can't be told
/// apart from an unused feature (`analytics.rs`).
pub async fn run_turn(
    llm: &dyn AgentLlm,
    dispatcher: &dyn ToolDispatcher,
    conn: &rusqlite::Connection,
    tools: &[ToolDeclaration],
    params: &TurnParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> TurnResult {
    let mut tally = TurnTally::default();
    let result = drive(llm, dispatcher, conn, tools, params, sink, cancel, &mut tally).await;
    analytics::turn_finished(params, &result, &tally);
    result
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
) -> TurnResult {
    // The working transcript mirrors the durable rows; assembly reads it, persistence
    // writes the DB. Load the persisted history, then (for a new turn) the pending user
    // message — held in memory until the first `End` so a failed first attempt records
    // nothing (crash case b).
    let mut transcript = match load_transcript(conn, params.conversation_id) {
        Ok(history) => history,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "load transcript failed: {e}");
            emit(
                sink,
                AgentChatEvent::Failed {
                    kind: AgentErrorKind::Provider,
                    detail: Some(e.to_string()),
                },
            );
            return TurnResult::Failed(AgentErrorKind::Provider);
        }
    };
    let mut user_needs_persist = false;
    if let Some(user) = params.user {
        transcript.push(AgentMessage {
            role: AgentRole::User,
            parts: vec![user.part()],
            at: params.now_secs,
        });
        user_needs_persist = true;
    }

    let started = Instant::now();
    let mut model_recorded = false;
    let mut trim_announced = false;

    loop {
        // Cancellation and both budgets are checked at the top, so no `respond` fires
        // once the user cancelled or a budget is spent — a runaway loop is impossible.
        if cancel.is_cancelled() {
            return TurnResult::Cancelled;
        }
        if started.elapsed() >= MAX_WALL_TIME || tally.tool_turns >= MAX_TOOL_TURNS {
            emit(
                sink,
                AgentChatEvent::Failed {
                    kind: AgentErrorKind::BudgetExhausted,
                    detail: None,
                },
            );
            return TurnResult::Failed(AgentErrorKind::BudgetExhausted);
        }

        let prefix = PrefixInputs {
            system_prompt: SYSTEM_PROMPT,
            cmdr_md: params.cmdr_md,
            memory: params.memory,
            tools,
        };
        let assembled = context::assemble_prompt(
            &prefix,
            &transcript,
            params.envelope,
            params.offset,
            params.prompt_budget,
        );
        announce_context_pressure(&assembled.elision, sink, &mut trim_announced);
        // A result the prompt dropped is a result the model never read: withdraw it as
        // evidence before the call goes out, so nothing downstream can cite its contents.
        if !assembled.elision.elided_call_ids.is_empty() {
            dispatcher.revoke_evidence(&assembled.elision.elided_call_ids);
        }

        let stream = match llm
            .respond(&assembled.system, &assembled.tools, &assembled.messages, cancel.clone())
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                // The call never opened, so it never reached `End`: nothing is
                // persisted (crash case b). Surface the typed error plus the
                // provider's own wording for display.
                let detail = error.detail().map(str::to_string);
                let kind = AgentErrorKind::from(error);
                emit(sink, AgentChatEvent::Failed { kind, detail });
                return TurnResult::Failed(kind);
            }
        };

        emit(sink, AgentChatEvent::AssistantStarted);
        let StreamOutcome {
            final_message,
            stop,
            usage,
            stream_error,
        } = consume_stream(stream, sink).await;

        let message = match final_message {
            Some(message) => message,
            None => {
                // The stream ended without an `End`: partial assistant text is discarded
                // (crash case a). A user cancel is a clean stop, not a failure.
                if cancel.is_cancelled() {
                    return TurnResult::Cancelled;
                }
                let detail = stream_error.as_ref().and_then(|e| e.detail().map(str::to_string));
                let kind = stream_error
                    .map(AgentErrorKind::from)
                    .unwrap_or(AgentErrorKind::UnfinishedReply);
                emit(sink, AgentChatEvent::Failed { kind, detail });
                return TurnResult::Failed(kind);
            }
        };

        // A completed `respond`: record a model transition (first `End` only, BEFORE the
        // user row so the event line sits between the turns), persist the user row (first
        // `End` only), then the assistant row (content written only now), then meter this
        // call's cost.
        if !model_recorded {
            model_recorded = true;
            record_model_transition(conn, params, sink);
        }
        if user_needs_persist && let Some(user) = params.user {
            match store::append_message(
                conn,
                params.conversation_id,
                AgentRole::User,
                &[user.part()],
                &user.search_text(),
                None,
                None,
                params.now_secs,
            ) {
                Ok((message_id, seq)) => {
                    user_needs_persist = false;
                    emit(sink, AgentChatEvent::UserPersisted { message_id, seq });
                }
                Err(e) => return persist_failed(sink, e),
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
            Err(e) => return persist_failed(sink, e),
        };
        transcript.push(message.clone());
        meter_cost(conn, params, usage);

        // Terminal vs. another tool turn.
        let has_tool_calls = message.parts.iter().any(|p| matches!(p, AgentPart::ToolCall(_)));
        if !has_tool_calls {
            report_context_usage(conn, params.conversation_id, &assembled.elision, sink);
            emit(
                sink,
                AgentChatEvent::Done {
                    message_id: assistant_id,
                    seq: assistant_seq,
                    stop,
                    usage,
                },
            );
            return TurnResult::Answered {
                assistant_message_id: assistant_id,
            };
        }

        // Dispatch each tool call, persisting its result on its own row, then loop.
        tally.tool_turns += 1;
        for part in &message.parts {
            let AgentPart::ToolCall(call) = part else { continue };
            let dispatch = dispatcher.dispatch(call).await;
            let result = dispatch.result;
            emit(
                sink,
                AgentChatEvent::ToolCallFinished {
                    call_id: call.call_id.clone(),
                    ok: dispatch_ok(&result),
                },
            );
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
                return persist_failed(sink, e);
            }
            if let Some(proposal) = dispatch.proposal {
                tally.proposals += 1;
                emit(sink, AgentChatEvent::ProposalReady { proposal });
            }
            transcript.push(tool_message);
        }
    }
}

struct StreamOutcome {
    final_message: Option<AgentMessage>,
    stop: AgentStopReason,
    usage: AgentUsage,
    stream_error: Option<AgentLlmError>,
}

/// Consume one `respond` stream, emitting display events and capturing the final
/// message plus its stop reason and usage (present only on a clean `End`). A stream
/// error or a drop leaves `final_message` `None` so the caller applies the
/// crash-case-a discard.
async fn consume_stream(mut stream: crate::agent::llm::AgentDeltaStream, sink: &ChatEventSink) -> StreamOutcome {
    let mut final_message = None;
    let mut stop = AgentStopReason::Completed;
    let mut usage = AgentUsage::default();
    let mut stream_error = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(AgentDelta::Text(text)) => emit(sink, AgentChatEvent::TextDelta { text }),
            Ok(AgentDelta::ReasoningTick) => emit(sink, AgentChatEvent::ReasoningTick),
            Ok(AgentDelta::ToolCallStarted { call_id, tool }) => {
                emit(sink, AgentChatEvent::ToolCallStarted { call_id, tool })
            }
            Ok(AgentDelta::End {
                message,
                stop: end_stop,
                usage: end_usage,
            }) => {
                stop = end_stop;
                usage = end_usage;
                final_message = Some(message);
            }
            Err(error) => {
                stream_error = Some(error);
                break;
            }
        }
    }
    StreamOutcome {
        final_message,
        stop,
        usage,
        stream_error,
    }
}

/// Tell the user, once per answered turn, how full the model's view got — and remember it with
/// the thread, so reopening the chat shows the last known figure instead of an empty gauge.
///
/// Called on the answered path with THAT call's assembly, which is the turn's last and largest:
/// within one turn each tool result joins the same prompt, and the biggest is what answers "is
/// this chat filling up?"
///
/// A failed or cancelled turn deliberately reports nothing: the user is looking at an error
/// line, and the previous turn's stored figure stays the last thing actually measured. A
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

fn persist_failed(sink: &ChatEventSink, error: AgentStoreError) -> TurnResult {
    log::warn!(target: LOG_TARGET, "persisting a chat message failed: {error}");
    emit(
        sink,
        AgentChatEvent::Failed {
            kind: AgentErrorKind::Provider,
            detail: Some(error.to_string()),
        },
    );
    TurnResult::Failed(AgentErrorKind::Provider)
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

/// Load a conversation's persisted messages as the working transcript. Event rows are
/// UI-facing timeline entries (a model change), NOT transcript content — they never
/// reach a provider, so they're filtered out here.
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
