//! The pure context-assembly core: values in, prompt out, no I/O and no clock.
//!
//! Everything here is a pure function of its arguments, so every test in this file
//! runs with no tokio runtime, no DB, and no app state. The runtime (`runtime.rs`)
//! captures the live values (the envelope, the clock offset, `CMDR.md`) and calls in.
//!
//! Two properties this module exists to guarantee:
//! - **The prefix is byte-identical across calls.** `system` (the system prompt plus
//!   `CMDR.md`) and the tool declarations never change within or across a thread's
//!   calls, so provider prompt caching holds. A changed envelope must NOT touch the
//!   prefix — the envelope lives on the latest user turn only.
//! - **The envelope is snapshot-at-send.** The caller captures one [`ContextEnvelope`]
//!   at message-send and passes the SAME value on every `respond` call of that turn's
//!   tool loop, so the model's ground truth can't shift mid-turn.
//!
//! History compaction is **elide-only** (spec §2, §5): assistant prose always survives
//! verbatim; tool results from older turns collapse to a typed stub carrying an
//! approximate token-size hint. Summarize-on-overflow is deferred; when even full
//! elision can't fit the budget, the runtime shows the soft-cap nudge.

use chrono::{DateTime, FixedOffset, Utc};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

use crate::agent::llm::types::{AgentMessage, AgentPart, AgentRole, AgentToolResult, ToolDeclaration};

// ── Named constants (§10). Initial values; tune with use. ─────────────────────

/// Per user message: a loop that wants a tool turn past this stops (a final
/// tool-less answer is forced, then budget-exhausted if even that can't finish).
/// Makes a runaway loop impossible by construction. Initial value; tune with use.
pub const MAX_TOOL_TURNS: usize = 8;

/// Per user message wall-clock ceiling across the whole tool loop. Initial value;
/// tune with use.
pub const MAX_WALL_TIME: Duration = Duration::from_secs(120);

/// Target assembled-prompt size per call, in estimated tokens (spec's 6-10k band).
/// Assembly elides older tool results until it fits, never touching assistant prose.
/// Initial value; tune with use.
pub const CONTEXT_TOKEN_BUDGET: usize = 8_000;

/// Tool results this many turns back (or more) collapse to a typed stub; assistant
/// prose always survives verbatim. Initial value; tune with use.
pub const ELIDE_TOOL_RESULTS_AFTER_TURNS: usize = 3;

/// Past this many messages a thread shows the honest "this chat is getting long -
/// start a fresh one?" nudge, no hard cut. Initial value; tune with use.
pub const THREAD_SOFT_CAP_MESSAGES: usize = 40;

/// Rough characters-per-token divisor for the size estimates that drive elision and
/// the stub's token hint. A heuristic, not a real tokenizer. Initial value; tune with
/// use.
pub const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

/// Header that introduces the user's `CMDR.md` inside the system prompt when present.
const CMDR_MD_HEADER: &str = "The user's CMDR.md (their notes for you; read-only):";

// ── The context envelope (§9) ─────────────────────────────────────────────────

/// Index-freshness of a volume, as the envelope voices it. A pure mirror of the
/// live freshness the runtime reads from the volume snapshot, decoupled so this core
/// stays free of app-state types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeFreshness {
    Fresh,
    Scanning,
    Stale,
    Off,
}

impl EnvelopeFreshness {
    fn token(self) -> &'static str {
        match self {
            EnvelopeFreshness::Fresh => "fresh",
            EnvelopeFreshness::Scanning => "scanning",
            EnvelopeFreshness::Stale => "stale",
            EnvelopeFreshness::Off => "off",
        }
    }
}

/// SMB connectivity of a volume, as the envelope voices it (only SMB volumes carry
/// one). A pure mirror of the live `SmbConnectionState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeConnectivity {
    Direct,
    OsMount,
    Disconnected,
}

impl EnvelopeConnectivity {
    fn token(self) -> &'static str {
        match self {
            EnvelopeConnectivity::Direct => "direct",
            EnvelopeConnectivity::OsMount => "os_mount",
            EnvelopeConnectivity::Disconnected => "disconnected",
        }
    }
}

/// One volume as the envelope lists it: a name, its index freshness, and (SMB only)
/// its connectivity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeVolume {
    pub name: String,
    pub freshness: EnvelopeFreshness,
    pub connectivity: Option<EnvelopeConnectivity>,
}

/// Whether an attached reference points at a file or a folder. The only "metadata"
/// an attachment carries into the envelope beyond its path — never file contents
/// (the read-only privacy line, spec §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    File,
    Folder,
}

impl AttachmentKind {
    fn token(self) -> &'static str {
        match self {
            AttachmentKind::File => "file",
            AttachmentKind::Folder => "folder",
        }
    }
}

/// One file or folder the user referenced (dragged onto the composer, or "ask about
/// selection") for this turn. A pure reference — path plus kind — resolved into the
/// envelope, structurally never the file's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeAttachment {
    pub path: String,
    pub kind: AttachmentKind,
}

/// The live app-state snapshot the runtime captures ONCE at message-send and holds
/// constant across the whole turn (snapshot-at-send). Rendered as the tagged block
/// that opens the latest user turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEnvelope {
    /// Unix secs when the send happened; rendered as the block's local timestamp.
    pub captured_at: i64,
    /// The focused pane's current directory, or `None` if unknown.
    pub focused_pane_path: Option<String>,
    /// The cursor item's name, or `None` (rendered as an em dash).
    pub cursor_item: Option<String>,
    pub selection_count: u32,
    pub volumes: Vec<EnvelopeVolume>,
    /// Files/folders the user attached by reference for this turn (drag-onto-composer
    /// or "ask about selection"). Empty in the common case; rendered as a trailing
    /// `attached: …` segment. Paths + kinds only, never contents.
    pub attachments: Vec<EnvelopeAttachment>,
}

// ── Prefix + assembled output ─────────────────────────────────────────────────

/// The stable-prefix inputs: the system prompt, the user's `CMDR.md` if present, and
/// the tool declarations. These produce the byte-identical prefix.
pub struct PrefixInputs<'a> {
    pub system_prompt: &'a str,
    pub cmdr_md: Option<&'a str>,
    pub tools: &'a [ToolDeclaration],
}

/// The fully-assembled prompt for one `respond` call: the cached prefix (`system` +
/// `tools`) and the compacted message history with the envelope on the latest user
/// turn.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledPrompt {
    pub system: String,
    pub tools: Vec<ToolDeclaration>,
    pub messages: Vec<AgentMessage>,
}

/// Build the `system` string: the system prompt, plus the user's `CMDR.md` appended
/// under a header when it carries content. Pure and deterministic, so it is
/// byte-identical for the same inputs (the prefix-stability guarantee).
pub fn build_system(system_prompt: &str, cmdr_md: Option<&str>) -> String {
    match cmdr_md {
        Some(md) if !md.trim().is_empty() => format!("{system_prompt}\n\n{CMDR_MD_HEADER}\n{}", md.trim_end()),
        _ => system_prompt.to_string(),
    }
}

/// Assemble the full prompt for one call: the stable prefix plus the compacted
/// `transcript` (history + the latest user turn + any in-flight turn messages), with
/// the `envelope` rendered onto the latest user turn only and historical user turns
/// carrying just their timestamp. `offset` is the local UTC offset captured at send,
/// applied to every rendered timestamp.
///
/// Deterministic: same inputs → identical output (byte-identical prefix; identical
/// messages). Changing only the envelope changes only the latest user turn, never the
/// prefix.
pub fn assemble_prompt(
    prefix: &PrefixInputs<'_>,
    transcript: &[AgentMessage],
    envelope: &ContextEnvelope,
    offset: FixedOffset,
) -> AssembledPrompt {
    let system = build_system(prefix.system_prompt, prefix.cmdr_md);
    let tools = prefix.tools.to_vec();

    // Elide older tool results, tightening the threshold until the estimate fits the
    // budget (assistant prose is never touched — that's the soft-cap's job).
    let mut threshold = ELIDE_TOOL_RESULTS_AFTER_TURNS;
    let mut messages = build_messages(transcript, envelope, offset, threshold);
    while threshold > 0 && estimate_prompt_tokens(&system, &tools, &messages) > CONTEXT_TOKEN_BUDGET {
        threshold -= 1;
        messages = build_messages(transcript, envelope, offset, threshold);
    }

    AssembledPrompt {
        system,
        tools,
        messages,
    }
}

/// Render the envelope as its tagged block (the exact §9 field set). Public so the
/// runtime and tests can assert the rendered form directly.
pub fn render_envelope(envelope: &ContextEnvelope, offset: FixedOffset) -> String {
    let timestamp = format_timestamp(envelope.captured_at, offset);
    let focused = envelope.focused_pane_path.as_deref().unwrap_or(EM_DASH);
    let cursor = envelope.cursor_item.as_deref().unwrap_or(EM_DASH);
    let volumes = if envelope.volumes.is_empty() {
        "none".to_string()
    } else {
        envelope
            .volumes
            .iter()
            .map(render_volume)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let attachments = if envelope.attachments.is_empty() {
        String::new()
    } else {
        let refs = envelope
            .attachments
            .iter()
            .map(|a| format!("{} ({})", a.path, a.kind.token()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" · attached: {refs}")
    };
    format!(
        "[{timestamp} · focused: {focused} · cursor: {cursor} · {} selected · volumes: {volumes}{attachments}]",
        envelope.selection_count
    )
}

const EM_DASH: &str = "—";

fn render_volume(volume: &EnvelopeVolume) -> String {
    match volume.connectivity {
        Some(connectivity) => format!(
            "{} ({}, {})",
            volume.name,
            volume.freshness.token(),
            connectivity.token()
        ),
        None => format!("{} ({})", volume.name, volume.freshness.token()),
    }
}

/// Render a historical user turn's lighter timestamp marker (no full envelope; the
/// envelope opens the latest turn only).
fn render_history_timestamp(at: i64, offset: FixedOffset) -> String {
    format!("[{}]", format_timestamp(at, offset))
}

/// `Sat 2026-07-12 21:30`: local weekday, ISO date, and time. Pure given `offset`
/// (no ambient clock or timezone read), so tests are deterministic.
fn format_timestamp(unix_secs: i64, offset: FixedOffset) -> String {
    let utc = DateTime::<Utc>::from_timestamp(unix_secs, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    utc.with_timezone(&offset).format("%a %Y-%m-%d %H:%M").to_string()
}

/// Transform the transcript into the messages to send at a given elision threshold:
/// the envelope onto the latest user turn, a timestamp marker onto every earlier user
/// turn, and tool results `threshold`-or-more turns back collapsed to a typed stub.
fn build_messages(
    transcript: &[AgentMessage],
    envelope: &ContextEnvelope,
    offset: FixedOffset,
    threshold: usize,
) -> Vec<AgentMessage> {
    let user_positions: Vec<usize> = transcript
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == AgentRole::User)
        .map(|(i, _)| i)
        .collect();
    let latest_user = user_positions.last().copied();
    let tool_names = tool_names_by_call_id(transcript);

    transcript
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let turns_back = user_positions.iter().filter(|&&pos| pos > index).count();
            match message.role {
                AgentRole::User if Some(index) == latest_user => {
                    prepend_text(message, render_envelope(envelope, offset))
                }
                AgentRole::User => prepend_text(message, render_history_timestamp(message.at, offset)),
                AgentRole::Tool if turns_back >= threshold => elide_tool_results(message, &tool_names),
                _ => message.clone(),
            }
        })
        .collect()
}

/// Return a clone of `message` with a leading text part carrying `text`.
fn prepend_text(message: &AgentMessage, text: String) -> AgentMessage {
    let mut parts = Vec::with_capacity(message.parts.len() + 1);
    parts.push(AgentPart::Text(text));
    parts.extend(message.parts.iter().cloned());
    AgentMessage {
        role: message.role,
        parts,
        at: message.at,
    }
}

/// Collapse every tool-result part in `message` to a typed stub carrying the tool name
/// (from the matching call) and the elided result's approximate token size.
fn elide_tool_results(message: &AgentMessage, tool_names: &HashMap<String, String>) -> AgentMessage {
    let parts = message
        .parts
        .iter()
        .map(|part| match part {
            AgentPart::ToolResult(result) if !result.elided => {
                AgentPart::ToolResult(stub_for(result, tool_names.get(&result.call_id).map(String::as_str)))
            }
            other => other.clone(),
        })
        .collect();
    AgentMessage {
        role: message.role,
        parts,
        at: message.at,
    }
}

/// The typed elision stub for one tool result: a small object the model can read,
/// naming the tool and the approximate token size the full result would have cost.
fn stub_for(result: &AgentToolResult, tool_name: Option<&str>) -> AgentToolResult {
    AgentToolResult {
        call_id: result.call_id.clone(),
        content: json!({
            "elided_tool_result": true,
            "tool": tool_name,
            "approx_tokens": estimate_tokens_of_value(&result.content),
        }),
        elided: true,
    }
}

/// Map each tool call's `call_id` to its wire tool name, so an elided result can name
/// the tool it came from ("[tool result elided: list_dir, ~3.1k tokens]").
fn tool_names_by_call_id(transcript: &[AgentMessage]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for message in transcript {
        for part in &message.parts {
            if let AgentPart::ToolCall(call) = part {
                map.insert(call.call_id.clone(), call.tool.as_wire_name().to_string());
            }
        }
    }
    map
}

// ── Token estimation (heuristic, drives elision + the stub hint) ──────────────

/// Estimate the assembled prompt's token size: the system string, the serialized tool
/// declarations, and every message part. A rough heuristic (chars / 4), not a real
/// tokenizer — enough to keep assembly inside the budget band.
pub fn estimate_prompt_tokens(system: &str, tools: &[ToolDeclaration], messages: &[AgentMessage]) -> usize {
    let system_tokens = estimate_tokens_str(system);
    let tool_tokens: usize = tools.iter().map(estimate_tokens_of_tool).sum();
    let message_tokens: usize = messages.iter().map(estimate_tokens_of_message).sum();
    system_tokens + tool_tokens + message_tokens
}

fn estimate_tokens_str(text: &str) -> usize {
    text.len().div_ceil(CHARS_PER_TOKEN_ESTIMATE)
}

fn estimate_tokens_of_value(value: &Value) -> usize {
    estimate_tokens_str(&value.to_string())
}

fn estimate_tokens_of_tool(tool: &ToolDeclaration) -> usize {
    estimate_tokens_str(tool.name.as_wire_name())
        + estimate_tokens_str(&tool.description)
        + estimate_tokens_of_value(&tool.schema)
}

fn estimate_tokens_of_message(message: &AgentMessage) -> usize {
    message
        .parts
        .iter()
        .map(|part| match part {
            AgentPart::Text(text) => estimate_tokens_str(text),
            AgentPart::ToolCall(call) => {
                estimate_tokens_str(call.tool.as_wire_name()) + estimate_tokens_of_value(&call.arguments)
            }
            AgentPart::ToolResult(result) => estimate_tokens_of_value(&result.content),
            AgentPart::Reasoning(state) => estimate_tokens_of_value(&state.blob),
        })
        .sum()
}

#[cfg(test)]
mod tests;
