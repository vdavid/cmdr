//! Pure context-assembly tests. Every one runs with no tokio runtime, no DB, and no
//! app state — the whole point of keeping this core pure (values in, prompt out).

use chrono::{FixedOffset, TimeZone};
use serde_json::json;

use super::*;
use crate::agent::llm::types::{AgentMessage, AgentPart, AgentRole, AgentToolCall, AgentToolResult, ToolId};

const SYSTEM: &str = "SYSTEM PROMPT BODY";

fn offset() -> FixedOffset {
    FixedOffset::east_opt(2 * 3600).expect("valid offset")
}

fn user(text: &str, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text(text.to_string())],
        at,
    }
}

fn assistant_text(text: &str, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::Text(text.to_string())],
        at,
    }
}

fn assistant_tool_call(call_id: &str, tool: ToolId, args: Value, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::ToolCall(AgentToolCall {
            call_id: call_id.to_string(),
            tool,
            arguments: args,
            reasoning: None,
        })],
        at,
    }
}

fn tool_result(call_id: &str, content: Value, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::Tool,
        parts: vec![AgentPart::ToolResult(AgentToolResult {
            call_id: call_id.to_string(),
            content,
            elided: false,
        })],
        at,
    }
}

fn declaration(tool: ToolId) -> ToolDeclaration {
    ToolDeclaration {
        name: tool,
        description: "a read tool".to_string(),
        schema: json!({ "type": "object" }),
    }
}

fn prefix<'a>(cmdr_md: Option<&'a str>, tools: &'a [ToolDeclaration]) -> PrefixInputs<'a> {
    PrefixInputs {
        system_prompt: SYSTEM,
        cmdr_md,
        tools,
    }
}

fn envelope_at(at: i64) -> ContextEnvelope {
    ContextEnvelope {
        captured_at: at,
        focused_pane_path: Some("~/Documents/taxes".to_string()),
        cursor_item: Some("2024/".to_string()),
        selection_count: 2,
        volumes: vec![
            EnvelopeVolume {
                name: "Macintosh HD".to_string(),
                freshness: EnvelopeFreshness::Fresh,
                connectivity: None,
            },
            EnvelopeVolume {
                name: "NAS-home".to_string(),
                freshness: EnvelopeFreshness::Stale,
                connectivity: Some(EnvelopeConnectivity::Direct),
            },
        ],
        attachments: vec![],
    }
}

/// Pull the leading text part of a message (the envelope or timestamp marker the
/// assembly prepends).
fn leading_text(message: &AgentMessage) -> &str {
    match &message.parts[0] {
        AgentPart::Text(text) => text,
        _ => panic!("expected a leading text part"),
    }
}

// ── Prefix stability ──────────────────────────────────────────────────────────

#[test]
fn prefix_is_byte_identical_across_calls() {
    let tools = [declaration(ToolId::AppState), declaration(ToolId::ListDir)];
    let transcript = [user("what is big?", 1_000)];
    let env = envelope_at(1_000);

    let first = assemble_prompt(&prefix(None, &tools), &transcript, &env, offset());
    let second = assemble_prompt(&prefix(None, &tools), &transcript, &env, offset());

    assert_eq!(first.system, second.system, "system prefix must be byte-identical");
    assert_eq!(first.tools, second.tools, "tool declarations must be byte-identical");
}

#[test]
fn a_changed_envelope_does_not_touch_the_prefix() {
    let tools = [declaration(ToolId::AppState)];
    let transcript = [user("what is big?", 1_000)];

    let one = assemble_prompt(&prefix(None, &tools), &transcript, &envelope_at(1_000), offset());
    let mut other_env = envelope_at(9_999);
    other_env.selection_count = 7;
    other_env.focused_pane_path = Some("~/Movies".to_string());
    let two = assemble_prompt(&prefix(None, &tools), &transcript, &other_env, offset());

    // The prefix is untouched by the envelope change...
    assert_eq!(one.system, two.system, "envelope must not touch the system prefix");
    assert_eq!(one.tools, two.tools, "envelope must not touch the tool declarations");
    // ...but the latest user turn's envelope block DID change (proving the test has teeth).
    assert_ne!(
        leading_text(&one.messages[0]),
        leading_text(&two.messages[0]),
        "the envelope block on the latest user turn must reflect the change"
    );
}

#[test]
fn cmdr_md_appears_in_system_only_when_present() {
    let without = build_system(SYSTEM, None);
    assert_eq!(without, SYSTEM, "no CMDR.md means the system is just the prompt");

    let with = build_system(SYSTEM, Some("Prefer terse answers."));
    assert!(with.starts_with(SYSTEM), "the prompt still leads");
    assert!(with.contains("Prefer terse answers."), "CMDR.md content is appended");
    assert_ne!(with, without, "CMDR.md changes the system string");

    // Whitespace-only CMDR.md is treated as absent (no empty header block).
    assert_eq!(build_system(SYSTEM, Some("   \n ")), SYSTEM);
}

// ── Envelope ──────────────────────────────────────────────────────────────────

#[test]
fn envelope_renders_the_exact_field_set() {
    // The §9 field set, order, and separators, verbatim. The timestamp is derived
    // through the same offset so the assertion pins structure, not a wall clock.
    let off = offset();
    let dt = off
        .with_ymd_and_hms(2026, 7, 12, 21, 30, 0)
        .single()
        .expect("valid datetime");
    let env = envelope_at(dt.timestamp());

    let expected_ts = dt.format("%a %Y-%m-%d %H:%M").to_string();
    let expected = format!(
        "[{expected_ts} · focused: ~/Documents/taxes · cursor: 2024/ · 2 selected · volumes: Macintosh HD (fresh), NAS-home (stale, direct)]"
    );
    assert_eq!(render_envelope(&env, off), expected);
}

#[test]
fn envelope_uses_em_dashes_and_none_when_fields_are_absent() {
    let env = ContextEnvelope {
        captured_at: 0,
        focused_pane_path: None,
        cursor_item: None,
        selection_count: 0,
        volumes: vec![],
        attachments: vec![],
    };
    let rendered = render_envelope(&env, offset());
    assert!(rendered.contains("focused: —"), "absent focus renders an em dash");
    assert!(rendered.contains("cursor: —"), "absent cursor renders an em dash");
    assert!(rendered.contains("0 selected"));
    assert!(rendered.contains("volumes: none"), "no volumes renders 'none'");
}

#[test]
fn envelope_opens_only_the_latest_user_turn() {
    let transcript = [
        user("first question", 1_000),
        assistant_text("first answer", 1_100),
        user("second question", 2_000),
    ];
    let env = envelope_at(2_000);
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &env, offset());

    let full_block = render_envelope(&env, offset());
    // The latest user turn (index 2) opens with the full envelope block.
    assert_eq!(leading_text(&assembled.messages[2]), full_block);
    // The earlier user turn (index 0) carries a timestamp marker, NOT the envelope.
    let earlier = leading_text(&assembled.messages[0]);
    assert_ne!(earlier, full_block, "an earlier turn must not carry the envelope");
    assert!(
        earlier.starts_with('[') && earlier.contains(':'),
        "it carries a timestamp"
    );
    assert!(
        !earlier.contains("selected"),
        "the timestamp marker has no envelope fields"
    );
}

#[test]
fn historical_turns_carry_their_own_timestamps() {
    let off = offset();
    let morning = off
        .with_ymd_and_hms(2026, 7, 12, 9, 15, 0)
        .single()
        .expect("valid")
        .timestamp();
    let evening = off
        .with_ymd_and_hms(2026, 7, 12, 21, 30, 0)
        .single()
        .expect("valid")
        .timestamp();
    let transcript = [
        user("this morning question", morning),
        user("evening question", evening),
    ];

    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(evening), off);
    // Earlier turn carries ITS timestamp (09:15), not the send time.
    assert!(leading_text(&assembled.messages[0]).contains("09:15"));
}

#[test]
fn two_assemblies_within_one_turn_see_a_byte_identical_envelope() {
    // Snapshot-at-send: the SAME envelope value is passed on both respond calls of a
    // turn's loop, and the transcript grows between them (an assistant tool call + its
    // result). The envelope block on the latest user turn must be byte-identical.
    let env = envelope_at(2_000);
    let first_call = [user("what is big?", 2_000)];
    let first = assemble_prompt(&prefix(None, &[]), &first_call, &env, offset());

    let second_call = [
        user("what is big?", 2_000),
        assistant_tool_call("c1", ToolId::ListDir, json!({ "path": "/" }), 2_050),
        tool_result("c1", json!({ "entries": 3 }), 2_060),
    ];
    let second = assemble_prompt(&prefix(None, &[]), &second_call, &env, offset());

    // The latest user turn is at index 0 in both; its envelope block is identical.
    assert_eq!(
        leading_text(&first.messages[0]),
        leading_text(&second.messages[0]),
        "the envelope must not shift across a turn's respond calls"
    );
}

// ── Elision ───────────────────────────────────────────────────────────────────

#[test]
fn old_tool_result_elides_to_a_typed_stub_and_prose_survives() {
    // A four-turn thread. The oldest tool result (turn 0) is 3+ turns back, so it
    // elides; the newest (the latest turn) survives. Assistant prose is untouched.
    let big_listing = json!({ "big_folders": ["Movies 210 GB", "Photos 88 GB"] });
    let transcript = [
        user("turn 0", 1_000),
        assistant_tool_call("old", ToolId::LargestDirs, json!({ "path": "/" }), 1_010),
        tool_result("old", big_listing.clone(), 1_020),
        assistant_text("The big folders are Movies and Photos.", 1_030),
        user("turn 1", 2_000),
        assistant_text("answer 1", 2_010),
        user("turn 2", 3_000),
        assistant_text("answer 2", 3_010),
        user("turn 3 (latest)", 4_000),
        assistant_tool_call("new", ToolId::ListDir, json!({ "path": "/x" }), 4_010),
        tool_result("new", json!({ "entries": 5 }), 4_020),
    ];

    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(4_000), offset());

    // The old tool result (index 2) is now a typed stub naming its tool + size hint.
    let AgentPart::ToolResult(old) = &assembled.messages[2].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(old.elided, "the old tool result must be elided");
    assert_eq!(old.content["elided_tool_result"], true);
    assert_eq!(
        old.content["tool"], "largest_dirs",
        "the stub names the tool it came from"
    );
    assert!(
        old.content["approx_tokens"].as_u64().is_some_and(|n| n > 0),
        "the stub carries a token-size hint"
    );

    // Assistant prose from that old turn survives verbatim (the "remind me what the big
    // folders were" answerability).
    assert_eq!(
        assembled.messages[3].parts,
        vec![AgentPart::Text("The big folders are Movies and Photos.".to_string())]
    );

    // The newest tool result (index 10) is NOT elided.
    let AgentPart::ToolResult(new) = &assembled.messages[10].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(!new.elided, "the latest turn's tool result must survive");
    assert_eq!(new.content, json!({ "entries": 5 }));
}

// ── Budget ────────────────────────────────────────────────────────────────────

#[test]
fn assembly_elides_down_to_the_token_budget() {
    // A recent tool result too large to fit the budget forces elision below the normal
    // threshold. After assembly the estimate must be within CONTEXT_TOKEN_BUDGET.
    let huge = json!({ "blob": "x".repeat(CONTEXT_TOKEN_BUDGET * CHARS_PER_TOKEN_ESTIMATE * 2) });
    let transcript = [
        user("recent question", 1_000),
        assistant_tool_call("c1", ToolId::ListDir, json!({ "path": "/" }), 1_010),
        tool_result("c1", huge, 1_020),
    ];

    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(1_000), offset());
    let tokens = estimate_prompt_tokens(&assembled.system, &assembled.tools, &assembled.messages);
    assert!(
        tokens <= CONTEXT_TOKEN_BUDGET,
        "assembly must stay within the budget (got {tokens})"
    );
    // It fit by eliding the oversized tool result, not by dropping prose.
    let AgentPart::ToolResult(result) = &assembled.messages[2].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(result.elided, "the oversized result was elided to fit the budget");
}

// ── Attachments in the envelope (path + kind only; the privacy line) ────────────

fn envelope_with_attachments(attachments: Vec<EnvelopeAttachment>) -> ContextEnvelope {
    ContextEnvelope {
        captured_at: 1_780_000_000,
        focused_pane_path: Some("~/Documents".to_string()),
        cursor_item: None,
        selection_count: 0,
        volumes: vec![],
        attachments,
    }
}

#[test]
fn envelope_renders_attachment_paths_and_kinds() {
    let env = envelope_with_attachments(vec![
        EnvelopeAttachment {
            path: "/Users/d/photos".to_string(),
            kind: AttachmentKind::Folder,
        },
        EnvelopeAttachment {
            path: "/Users/d/taxes.pdf".to_string(),
            kind: AttachmentKind::File,
        },
    ]);
    let rendered = render_envelope(&env, offset());
    assert!(
        rendered.contains("attached: /Users/d/photos (folder), /Users/d/taxes.pdf (file)"),
        "attachments render as path + kind: {rendered}"
    );
}

#[test]
fn envelope_omits_the_attached_segment_when_empty() {
    let rendered = render_envelope(&envelope_with_attachments(vec![]), offset());
    assert!(
        !rendered.contains("attached:"),
        "no attachments ⇒ no segment: {rendered}"
    );
}

#[test]
fn attachments_ride_only_the_latest_user_turn_and_carry_nothing_but_path_and_kind() {
    // Two user turns; only the latest gets the envelope (with its attachments). The turn
    // text is unchanged and NOTHING beyond path + kind reaches the prompt (no size, no
    // contents) — the read-only privacy line asserted at the assembly boundary.
    let transcript = vec![
        user("first question", 100),
        assistant_text("an answer", 110),
        user("what's in this folder?", 200),
    ];
    let env = envelope_with_attachments(vec![EnvelopeAttachment {
        path: "/Users/d/secret".to_string(),
        kind: AttachmentKind::Folder,
    }]);
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &env, offset());

    // The earlier user turn carries only its timestamp marker, no attachment.
    let AgentPart::Text(first) = &assembled.messages[0].parts[0] else {
        panic!("expected the first user turn's text");
    };
    assert!(!first.contains("attached:"), "the older turn has no envelope: {first}");

    // The latest user turn opens with the envelope, naming the attachment path + kind.
    let AgentPart::Text(latest) = &assembled.messages[2].parts[0] else {
        panic!("expected the latest user turn's envelope text");
    };
    assert!(
        latest.contains("attached: /Users/d/secret (folder)"),
        "envelope names it: {latest}"
    );
    // The original question survives as its own part, unchanged.
    assert!(
        assembled.messages[2]
            .parts
            .iter()
            .any(|p| matches!(p, AgentPart::Text(t) if t == "what's in this folder?")),
        "the user's text is untouched"
    );
}
