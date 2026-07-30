//! What an elided tool result tells the model: which tool ran, what it was called with,
//! what it held, and how to get it back.
//!
//! A bare tombstone said only "something was dropped", so a model that met one knew it was
//! missing evidence but not what evidence or how to re-read it. These tests pin the stub's
//! four facts, its cost, and the two things it must NEVER be: a carrier of the content it
//! replaced, or something a rename can cite as evidence.

use serde_json::{Value, json};

use super::test_support::*;
use super::*;
use crate::agent::llm::types::ToolId;
use crate::agent::tools::propose::evidence::{
    EvidenceProblem, EvidenceScope, EvidenceSource, ImageFactsLedger, RenameEvidence,
};

/// The chat thread the evidence pin delivers into.
const THREAD: EvidenceScope = EvidenceScope::Thread(7);

const SHOTS_DIR: &str = "/Users/me/Downloads/shots";

fn shot_path(index: usize) -> String {
    format!("{SHOTS_DIR}/shot-{index}.png")
}

/// One `image_facts` row as the tool serializes it. `text: None` is a file with no
/// recognized text, `tags: &[]` one with no vision tags — the gaps the digest counts.
fn facts_row(index: usize, text: Option<&str>, tags: &[&str]) -> Value {
    let mut row = json!({ "path": shot_path(index), "state": "indexed" });
    row["text"] = json!(text.unwrap_or_default());
    row["tags"] = json!(
        tags.iter()
            .map(|label| json!({ "label": label, "score": 0.9 }))
            .collect::<Vec<_>>()
    );
    row
}

/// The 12-file `image_facts` batch from the fabrication incident: every file indexed, one
/// with no recognized text, three with no tags.
fn twelve_file_batch(ocr: &str) -> (Value, Value) {
    let arguments = json!({
        "volumeId": "root",
        "paths": (0..12).map(shot_path).collect::<Vec<_>>(),
    });
    let facts = (0..12)
        .map(|index| {
            facts_row(
                index,
                (index > 0).then_some(ocr),
                if index < 9 { &["screenshot", "document"] } else { &[] },
            )
        })
        .collect::<Vec<_>>();
    (arguments, json!({ "status": "ok", "coverage": [], "facts": facts }))
}

/// Assemble a thread whose FIRST turn's tool result is old enough to elide, and hand back
/// that stub. Four turns, so age alone (not budget pressure) does the eliding.
fn stub_of_an_aged_out_call(tool: ToolId, arguments: Value, content: Value) -> Value {
    let transcript = [
        user("turn 0", 1_000),
        assistant_tool_call("aged-out", tool, arguments, 1_010),
        tool_result("aged-out", content, 1_020),
        assistant_text("answer 0", 1_030),
        user("turn 1", 2_000),
        assistant_text("answer 1", 2_010),
        user("turn 2", 3_000),
        assistant_text("answer 2", 3_010),
        user("turn 3 (latest)", 4_000),
    ];
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(4_000), offset(), BUDGET);
    let stub = tool_result_part(&assembled.messages[2]);
    assert!(stub.elided, "the fixture's whole point is that this result elided");
    stub.content.clone()
}

/// The stub the spec asks for, field by field: which tool, how big the result was, what it
/// was called with, what it held, and the way back to it. Everything derived structurally
/// from the call and the result — no model call, no per-tool special casing.
#[test]
fn an_elided_result_names_the_tool_the_call_what_it_held_and_the_way_back() {
    let (arguments, content) = twelve_file_batch("Klarna payment confirmation");
    let approx = estimate_tokens_of_value(&content);

    let stub = stub_of_an_aged_out_call(ToolId::ImageFacts, arguments, content);

    assert_eq!(
        stub,
        json!({
            "elided_tool_result": true,
            "tool": "image_facts",
            "approx_tokens": approx,
            "call": "12 paths under /Users/me/Downloads/shots, volumeId: root",
            "held": "0 coverage, 12 facts (path, state, tags in 9, text in 11), status (2 chars)",
            "refetch": "call image_facts again for the paths you still need",
        }),
        "the stub must say what ran, what it held, and how to get it back"
    );
}

/// A stub that costs what the result cost defeats the purpose. The budget holds even for a
/// result built to blow it: many keys, long names, huge arrays.
#[test]
fn a_stub_costs_a_small_fraction_of_the_result_it_replaces() {
    let (arguments, content) = twelve_file_batch(&"x".repeat(900));
    let full = estimate_tokens_of_value(&content);
    let stub = stub_of_an_aged_out_call(ToolId::ImageFacts, arguments, content);
    let stub_tokens = estimate_tokens_of_value(&stub);
    assert!(
        stub_tokens <= STUB_TOKEN_BUDGET,
        "the stub must stay within its {STUB_TOKEN_BUDGET}-token budget (got {stub_tokens}): {stub}"
    );
    assert!(
        stub_tokens * 20 < full,
        "the stub ({stub_tokens}) must cost a small fraction of the {full} it replaces"
    );

    // A pathological result: 40 keys, each an array of wide objects under a long name.
    let sprawling = Value::Object(
        (0..40)
            .map(|i| {
                (
                    format!("a_very_long_result_key_number_{i:03}"),
                    json!(
                        (0..50)
                            .map(|j| json!({ "path": shot_path(j), "text": "y".repeat(2_000) }))
                            .collect::<Vec<_>>()
                    ),
                )
            })
            .collect(),
    );
    let stub = stub_of_an_aged_out_call(
        ToolId::ListDir,
        json!({ "path": SHOTS_DIR, "recursive": true }),
        sprawling,
    );
    let stub_tokens = estimate_tokens_of_value(&stub);
    assert!(
        stub_tokens <= STUB_TOKEN_BUDGET,
        "even a sprawling result gets a budgeted stub (got {stub_tokens}): {stub}"
    );
}

/// The digest describes the SHAPE of what was dropped, never the content. OCR text has no
/// re-fetch value and it reads as evidence, which is the whole failure this milestone must
/// not reintroduce — so no string the result carried may survive into the stub.
#[test]
fn a_stub_never_carries_the_text_it_dropped() {
    let ocr = "KLARNA betalningsbekräftelse Faktura Summa kronor ".repeat(41);
    assert!(ocr.chars().count() > 2_000, "the fixture is a full page of OCR");
    let (arguments, content) = twelve_file_batch(&ocr);

    let stub = stub_of_an_aged_out_call(ToolId::ImageFacts, arguments, content);

    let rendered = stub.to_string();
    for fragment in ["KLARNA", "betalningsbekräftelse", "Faktura", "Summa", "kronor"] {
        assert!(
            !rendered.contains(fragment),
            "the dropped OCR must not survive in the stub ({fragment} did): {rendered}"
        );
    }
    // Tags are delivered content too (an `imageTags` claim cites them), so labels stay out
    // as well. Key NAMES are structure and do appear ("tags in 9").
    assert!(
        !rendered.contains("screenshot") && !rendered.contains("document"),
        "delivered tag labels must not survive either: {rendered}"
    );
}

/// Invariant 6, pinned: a digest is never a delivery. The model that met this stub never
/// read the facts, so nothing it writes about their content may back a rename — not the OCR
/// it can no longer see, and not the digest it CAN see.
#[test]
fn a_plan_citing_an_elided_results_digest_is_refused() {
    let ocr = "Klarna betalningsbekräftelse 431 kr";
    let (arguments, content) = twelve_file_batch(ocr);
    let path = shot_path(1);

    // Dispatch recorded the delivery, as it does for every `image_facts` result...
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(THREAD, "aged-out", &content);
    let quote = RenameEvidence {
        source: EvidenceSource::ImageText,
        detail: ocr.to_string(),
    };
    assert!(
        ledger.check(THREAD, &path, &quote).is_ok(),
        "teeth: while the result was in the prompt, quoting it checked out"
    );

    // ...and then assembly dropped it, so the runtime revokes what the model never read.
    let transcript = [
        user("turn 0", 1_000),
        assistant_tool_call("aged-out", ToolId::ImageFacts, arguments, 1_010),
        tool_result("aged-out", content, 1_020),
        user("turn 1", 2_000),
        assistant_text("answer 1", 2_010),
        user("turn 2", 3_000),
        assistant_text("answer 2", 3_010),
        user("turn 3 (latest)", 4_000),
    ];
    let assembled = assemble_prompt(&prefix(None, &[]), &transcript, &envelope_at(4_000), offset(), BUDGET);
    assert_eq!(assembled.elision.elided_call_ids, vec!["aged-out".to_string()]);
    for call_id in &assembled.elision.elided_call_ids {
        ledger.revoke_call(call_id);
    }

    // The OCR it was handed is gone as evidence...
    assert_eq!(
        ledger.check(THREAD, &path, &quote),
        Err(EvidenceProblem::FactsNotDelivered),
        "a result the model never read cannot back a name"
    );
    // ...and the digest it CAN still see is not a substitute for it.
    let digest = tool_result_part(&assembled.messages[2]).content["held"]
        .as_str()
        .expect("the stub carries a digest of what was held")
        .to_string();
    assert_eq!(
        ledger.check(
            THREAD,
            &path,
            &RenameEvidence {
                source: EvidenceSource::ImageText,
                detail: digest,
            }
        ),
        Err(EvidenceProblem::FactsNotDelivered),
        "a digest is a description of a delivery, never a delivery"
    );
}

/// Shapes with little to say still say something, and a result whose call has scrolled out
/// of the transcript still names what it can.
#[test]
fn a_stub_stays_readable_for_shapes_with_nothing_to_count() {
    let empty = stub_of_an_aged_out_call(ToolId::ListDir, json!({}), json!({}));
    assert_eq!(empty["call"], "no arguments");
    assert_eq!(empty["held"], "an empty object");
    assert_eq!(
        empty["refetch"],
        "call list_dir again if you still need what it returned"
    );

    let scalars = stub_of_an_aged_out_call(
        ToolId::ListVolumes,
        json!({ "includeHidden": true, "limit": 4 }),
        json!({ "count": 2, "note": "nothing to report" }),
    );
    assert_eq!(scalars["call"], "includeHidden: true, limit: 4");
    assert_eq!(
        scalars["held"], "count: 2, note (17 chars)",
        "a string's length, never its text"
    );
}

/// The stub is part of the prompt, so it may not shift between the respond calls of one
/// turn (invariant 3's reason: a byte-different prefix is a cache miss, and a shifting
/// history is a model reading two different pasts).
#[test]
fn the_same_dropped_result_digests_byte_identically_every_time() {
    let (arguments, content) = twelve_file_batch("Klarna payment confirmation");
    let first = stub_of_an_aged_out_call(ToolId::ImageFacts, arguments.clone(), content.clone());
    let second = stub_of_an_aged_out_call(ToolId::ImageFacts, arguments, content);
    assert_eq!(first.to_string(), second.to_string());
}
