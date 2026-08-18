//! Pure context-assembly tests: the prefix, the envelope, elision, and the budget. Every
//! one runs with no tokio runtime, no DB, and no app state — the whole point of keeping
//! this core pure (values in, prompt out).
//!
//! Siblings: `stub_tests` (what an elided result says), `cost_tests` (what the shapes
//! cost). Fixtures they all share live in `test_support`.

use chrono::TimeZone;
use serde_json::json;

use super::test_support::*;
use super::*;
use crate::agent::chat::budget::CHARS_PER_TOKEN_ESTIMATE;
use crate::agent::llm::types::{AgentPart, ToolId};

// ── Prefix stability ──────────────────────────────────────────────────────────

#[test]
fn prefix_is_byte_identical_across_calls() {
    let tools = [declaration(ToolId::AppState), declaration(ToolId::ListDir)];
    let transcript = [user("what is big?", 1_000)];
    let env = envelope_at(1_000);

    let first = assemble_prompt(&prefix(None, &tools), &transcript, &env, offset(), BUDGET);
    let second = assemble_prompt(&prefix(None, &tools), &transcript, &env, offset(), BUDGET);

    assert_eq!(first.system, second.system, "system prefix must be byte-identical");
    assert_eq!(first.tools, second.tools, "tool declarations must be byte-identical");
}

#[test]
fn a_changed_envelope_does_not_touch_the_prefix() {
    let tools = [declaration(ToolId::AppState)];
    let transcript = [user("what is big?", 1_000)];

    let one = assemble_prompt(
        &prefix(None, &tools),
        &transcript,
        &envelope_at(1_000),
        offset(),
        BUDGET,
    );
    let mut other_env = envelope_at(9_999);
    other_env.selection_count = 7;
    other_env.focused_pane_path = Some("~/Movies".to_string());
    let two = assemble_prompt(&prefix(None, &tools), &transcript, &other_env, offset(), BUDGET);

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
        "[{expected_ts} · focused: ~/Documents/taxes · cursor: 2024/ · 2 selected · volumes: Macintosh HD (fresh), \
         NAS-home (stale, direct) · rename batch: up to 101 files]"
    );
    assert_eq!(render_envelope(&env, off), expected);
}

/// The system prompt tells the model to propose "the batch size this turn's envelope names",
/// so the envelope has to actually name it. A prompt pointing at an absent field is worse than
/// a hardcoded number: the model fills the gap with a guess.
#[test]
fn envelope_names_the_batch_size_the_prompt_points_at() {
    let mut env = envelope_at(1_780_000_000);
    env.rename_batch_files = crate::agent::chat::budget::files_per_batch(16_000);

    let rendered = render_envelope(&env, offset());

    assert!(
        rendered.contains("rename batch: up to 27 files"),
        "the envelope must carry the turn's batch size, got: {rendered}"
    );
}

/// A style the user rejected must not come back next batch, so the names they turned down ride
/// the envelope. Names only, never a reason: a model-authored "why" would be a rationalization
/// the next batch inherits.
#[test]
fn envelope_lists_the_names_the_user_turned_down() {
    let mut env = envelope_at(1_780_000_000);
    env.denied_names = vec!["klarna-invoice.png".to_string(), "receipt-2.png".to_string()];

    let rendered = render_envelope(&env, offset());

    assert!(
        rendered.contains("turned down: klarna-invoice.png, receipt-2.png"),
        "the envelope must name what was rejected, got: {rendered}"
    );
}

/// Fifty denied rows would spend the user's window on our own bookkeeping (intention 8), and a
/// silent cut would misreport what the user rejected (invariant 9). So it caps AND says so.
#[test]
fn a_long_denial_list_is_capped_and_says_how_many_it_left_out() {
    let mut env = envelope_at(1_780_000_000);
    env.denied_names = (0..9).map(|index| format!("shot-{index}.png")).collect();

    let rendered = render_envelope(&env, offset());

    assert!(rendered.contains("shot-0.png"), "the first examples are shown");
    assert!(
        !rendered.contains("shot-5.png"),
        "past the cap the names stop: {rendered}"
    );
    assert!(
        rendered.contains("and 4 more"),
        "the cut has to be visible, got: {rendered}"
    );
}

/// The common case: the user denied nothing, so the segment must vanish rather than render an
/// empty label on every single turn.
#[test]
fn envelope_omits_the_denial_segment_when_nothing_was_turned_down() {
    let rendered = render_envelope(&envelope_at(1_780_000_000), offset());
    assert!(!rendered.contains("turned down"), "no denials, no segment: {rendered}");
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
        denied_names: vec![],
        rename_batch_files: 101,
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
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &env, offset(), BUDGET);

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

    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(evening), off, BUDGET);
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
    let first = assemble_prompt(&prefix(None, &[]), &first_call, &env, offset(), BUDGET);

    let second_call = [
        user("what is big?", 2_000),
        assistant_tool_call("c1", ToolId::ListDir, json!({ "path": "/" }), 2_050),
        tool_result("c1", json!({ "entries": 3 }), 2_060),
    ];
    let second = assemble_prompt(&prefix(None, &[]), &second_call, &env, offset(), BUDGET);

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
        assistant_tool_call("old", ToolId::ListDir, json!({ "path": "/", "sortBy": "size" }), 1_010),
        tool_result("old", big_listing.clone(), 1_020),
        assistant_text("The big folders are Movies and Photos.", 1_030),
        user("turn 1", 2_000),
        assistant_text("answer 1", 2_010),
        user("turn 2", 3_000),
        assistant_text("answer 2", 3_010),
        user("turn 3 (latest)", 4_000),
        assistant_tool_call("new", ToolId::ListPaneFiles, json!({}), 4_010),
        tool_result("new", json!({ "entries": 5 }), 4_020),
    ];

    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(4_000), offset(), BUDGET);

    // The old tool result (index 2) is now a typed stub naming its tool + size hint.
    let AgentPart::ToolResult(old) = &assembled.messages[2].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(old.elided, "the old tool result must be elided");
    assert_eq!(old.content["elided_tool_result"], true);
    assert_eq!(old.content["tool"], "list_dir", "the stub names the tool it came from");
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

/// David's two-batch rename shape, the one that fabricated 12 filenames: batch 1's
/// `image_facts` result is history, batch 2's came back THIS turn and is far too big for
/// the budget. Budget pressure must never reach it — a model told to name files by their
/// content, handed a stub instead of the content, has invention as its only way to answer.
#[test]
fn the_current_turns_tool_result_survives_any_budget_pressure() {
    let batch_one = json!({ "facts": [{ "path": "/shots/a.png", "text": "x".repeat(11_000) }] });
    let batch_two =
        json!({ "facts": [{ "path": "/shots/l.png", "text": format!("LinkedIn inbox {}", "y".repeat(33_000)) }] });
    let transcript = [
        user("rename these 11 screenshots by their content", 1_000),
        assistant_tool_call("f1", ToolId::ImageFacts, json!({ "paths": ["/shots/a.png"] }), 1_010),
        tool_result("f1", batch_one, 1_020),
        assistant_text("Renamed the first 11.", 1_030),
        user("now the other 12", 2_000),
        assistant_tool_call("f2", ToolId::ImageFacts, json!({ "paths": ["/shots/l.png"] }), 2_010),
        tool_result("f2", batch_two, 2_020),
    ];

    let assembled = assemble_prompt(
        &prefix(None, &[]),
        &transcript,
        &envelope_at(2_000),
        offset(),
        TIGHT_BUDGET,
    );

    let AgentPart::ToolResult(latest) = &assembled.messages[6].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(
        !latest.elided,
        "the current turn's tool result must survive, whatever the budget says"
    );
    assert!(
        latest.content.to_string().contains("LinkedIn inbox"),
        "the payload the model was asked to name files from must still be there"
    );
    // The older batch DID elide (proving the test has teeth and the budget still bites).
    let AgentPart::ToolResult(older) = &assembled.messages[2].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(older.elided, "an earlier turn's oversized result is what elides");
    // And the drop is reported, so the runtime can say it out loud.
    assert!(
        assembled.elision.budget_forced(),
        "the budget, not age, forced this elision: {:?}",
        assembled.elision
    );
    assert_eq!(assembled.elision.elided_results, 1);
    assert!(assembled.elision.elided_tokens > 0, "the report sizes what it dropped");
    // And it NAMES the call whose result went, so the runtime can revoke that result's
    // standing as evidence: nothing may vouch for content the model never read.
    assert_eq!(assembled.elision.elided_call_ids, vec!["f1".to_string()]);
}

#[test]
fn the_report_names_every_call_whose_result_was_dropped() {
    // Two older results elide, the current turn's survives: the report must name exactly the
    // two that left, in transcript order, and never the one still in the prompt.
    let bulky = json!({ "text": "z".repeat(20_000) });
    let transcript = [
        user("turn 0", 1_000),
        assistant_tool_call("old-a", ToolId::ImageFacts, json!({ "paths": ["/a.png"] }), 1_010),
        tool_result("old-a", bulky.clone(), 1_020),
        user("turn 1", 2_000),
        assistant_tool_call("old-b", ToolId::ImageFacts, json!({ "paths": ["/b.png"] }), 2_010),
        tool_result("old-b", bulky.clone(), 2_020),
        user("turn 2 (latest)", 3_000),
        assistant_tool_call("fresh", ToolId::ImageFacts, json!({ "paths": ["/c.png"] }), 3_010),
        tool_result("fresh", bulky, 3_020),
    ];

    let assembled = assemble_prompt(
        &prefix(None, &[]),
        &transcript,
        &envelope_at(3_000),
        offset(),
        TIGHT_BUDGET,
    );

    assert_eq!(
        assembled.elision.elided_call_ids,
        vec!["old-a".to_string(), "old-b".to_string()]
    );
    assert_eq!(assembled.elision.elided_results, 2);
}

#[test]
fn an_assembly_that_drops_nothing_names_no_calls() {
    let transcript = [
        user("what is in these?", 1_000),
        assistant_tool_call("c1", ToolId::ImageFacts, json!({ "paths": ["/a.png"] }), 1_010),
        tool_result("c1", json!({ "facts": [] }), 1_020),
    ];
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(1_000), offset(), BUDGET);
    assert!(
        assembled.elision.elided_call_ids.is_empty(),
        "nothing dropped ⇒ nothing to revoke"
    );
}

// ── Budget ────────────────────────────────────────────────────────────────────

#[test]
fn assembly_elides_history_down_to_the_token_budget() {
    // An older tool result too large to fit forces elision below the normal threshold.
    // History is what elides, and after assembly the estimate is back inside the budget.
    let huge = json!({ "blob": "x".repeat(BUDGET * CHARS_PER_TOKEN_ESTIMATE * 2) });
    let transcript = [
        user("older question", 1_000),
        assistant_tool_call("c1", ToolId::ListDir, json!({ "path": "/" }), 1_010),
        tool_result("c1", huge, 1_020),
        assistant_text("answer", 1_030),
        user("recent question", 2_000),
    ];

    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(2_000), offset(), BUDGET);
    let tokens = estimate_prompt_tokens(&assembled.system, &assembled.tools, &assembled.messages);
    assert!(tokens <= BUDGET, "assembly must stay within the budget (got {tokens})");
    // It fit by eliding the oversized tool result, not by dropping prose.
    let AgentPart::ToolResult(result) = &assembled.messages[2].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(result.elided, "the oversized result was elided to fit the budget");
    assert_eq!(
        assembled.elision.estimated_tokens, tokens,
        "the report's estimate is the assembled prompt's own"
    );
    assert!(!assembled.elision.over_budget(), "it fit, so nothing overran");
}

#[test]
fn an_unfittable_current_turn_overruns_the_budget_and_says_so() {
    // The turn in flight alone is over budget: nothing can be dropped without blinding the
    // model to what it just looked at, so assembly overruns ON PURPOSE and reports it. The
    // runtime turns `over_budget` into a warn; silence here is what made a fabricated
    // answer read like a normal one.
    let huge = json!({ "blob": "x".repeat(TIGHT_BUDGET * CHARS_PER_TOKEN_ESTIMATE * 2) });
    let transcript = [
        user("name these by content", 1_000),
        assistant_tool_call("c1", ToolId::ImageFacts, json!({ "paths": ["/a.png"] }), 1_010),
        tool_result("c1", huge, 1_020),
    ];

    let assembled = assemble_prompt(
        &prefix(None, &[]),
        &transcript,
        &envelope_at(1_000),
        offset(),
        TIGHT_BUDGET,
    );

    let AgentPart::ToolResult(result) = &assembled.messages[2].parts[0] else {
        panic!("expected a tool-result part");
    };
    assert!(!result.elided, "the current turn's evidence is never traded for budget");
    assert!(assembled.elision.over_budget(), "the overrun must be reported");
    assert_eq!(assembled.elision.elided_results, 0, "there was nothing older to drop");
    assert_eq!(assembled.elision.budget, TIGHT_BUDGET);
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
        denied_names: vec![],
        rename_batch_files: 101,
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
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &env, offset(), BUDGET);

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
