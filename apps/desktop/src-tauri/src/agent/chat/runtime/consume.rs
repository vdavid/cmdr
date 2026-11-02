//! Reading one `respond` stream to its end: the display events it emits on the way, and the two
//! things that end it early, Stop and silence (`context::STREAM_STALL_TIMEOUT`).

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use super::events::{AgentChatEvent, ChatEventSink, emit};
use crate::agent::chat::context::STREAM_STALL_TIMEOUT;
use crate::agent::llm::types::{AgentDelta, AgentLlmError, AgentMessage, AgentStopReason, AgentUsage, ToolId};

pub(super) struct StreamOutcome {
    pub(super) final_message: Option<AgentMessage>,
    pub(super) stop: AgentStopReason,
    pub(super) usage: AgentUsage,
    pub(super) stream_error: Option<AgentLlmError>,
    /// No stream item came for [`STREAM_STALL_TIMEOUT`], so the stream was dropped.
    pub(super) stalled: bool,
}

/// Consume one `respond` stream, emitting display events and capturing the final
/// message plus its stop reason and usage (present only on a clean `End`). A stream
/// error or a drop leaves `final_message` `None` so the caller applies the
/// crash-case-a discard.
///
/// Stop ends the stream here, at once, whatever the `AgentLlm` does with its own copy of the
/// token: a model thinking before its next chunk would otherwise keep the turn (and the bill)
/// running until it spoke. Dropping the stream drops the response body, which ends the spend.
///
/// So does silence: no item at all for [`STREAM_STALL_TIMEOUT`] ends the stream as stalled. Any
/// item counts, the provider's keep-alive included, so a model thinking quietly between its
/// provider's pings is never mistaken for a dead connection.
pub(super) async fn consume_stream(
    mut stream: crate::agent::llm::AgentDeltaStream,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> StreamOutcome {
    let mut final_message = None;
    let mut stop = AgentStopReason::Completed;
    let mut usage = AgentUsage::default();
    let mut stream_error = None;
    let mut stalled = false;
    loop {
        let next = tokio::select! {
            biased;
            () = cancel.cancelled() => None,
            next = tokio::time::timeout(STREAM_STALL_TIMEOUT, stream.next()) => next.unwrap_or_else(|_| {
                stalled = true;
                None
            }),
        };
        let Some(item) = next else { break };
        match item {
            Ok(AgentDelta::Text(text)) => emit(sink, AgentChatEvent::TextDelta { text }),
            Ok(AgentDelta::ReasoningTick) => emit(sink, AgentChatEvent::ReasoningTick),
            Ok(AgentDelta::KeepAlive) => {}
            // `use_tool`'s arguments, which say what it runs, are still streaming. Its line arrives
            // with the round's `ToolCallsResolved`, naming the tool: the rail never shows the wrapper.
            Ok(AgentDelta::ToolCallStarted {
                tool: ToolId::UseTool, ..
            }) => {}
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
        stalled,
    }
}
