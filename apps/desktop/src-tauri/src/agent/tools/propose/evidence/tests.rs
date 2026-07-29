//! Tests for the evidence guardrail: what may back a content-derived name, and how much of
//! the delivered text a quote actually covers.

use super::*;
use serde_json::json;

/// An `image_facts` result shaped exactly like `mcp::executor::image_facts` serializes
/// one, so the ledger is pinned against the real payload rather than a hand-fit stub.
fn image_facts_result(rows: Vec<Value>) -> Value {
    json!({ "status": "ok", "facts": rows, "coverage": [] })
}

fn indexed(path: &str, text: Option<&str>, tags: &[&str]) -> Value {
    let mut row = json!({ "path": path, "state": "indexed" });
    if let Some(text) = text {
        row["text"] = json!(text);
    }
    if !tags.is_empty() {
        row["tags"] = json!(
            tags.iter()
                .map(|t| json!({ "label": t, "score": 0.9 }))
                .collect::<Vec<_>>()
        );
    }
    row
}

/// The chat thread these tests deliver into, and one that never received anything.
const THREAD: EvidenceScope = EvidenceScope::Thread(7);
const OTHER_THREAD: EvidenceScope = EvidenceScope::Thread(8);

fn evidence(source: EvidenceSource, detail: &str) -> RenameEvidence {
    RenameEvidence {
        source,
        detail: detail.to_string(),
    }
}

/// The incident, in one assertion: the model never received facts for this path (the
/// result was dropped from the prompt), invented a content-shaped name, and claimed
/// image text for it. That MUST be refused.
#[test]
fn a_content_claim_with_no_delivered_facts_is_refused() {
    let ledger = ImageFactsLedger::default();

    let refusal = ledger.check(
        THREAD,
        "/Users/x/Screenshots/shot-2.png",
        &evidence(EvidenceSource::ImageText, "hello world output"),
    );

    assert_eq!(refusal, Err(EvidenceProblem::FactsNotDelivered));
}

/// The second half of the incident: facts DID arrive for the path, but the quote
/// behind the name is nowhere in them. A plausible-looking slug is still fabrication.
#[test]
fn a_quote_that_isnt_in_the_delivered_text_is_refused() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed(
            "/Users/x/Screenshots/shot-2.png",
            Some("LinkedIn\nMessaging  3 new messages"),
            &["screenshot", "text"],
        )]),
    );

    assert_eq!(
        ledger.check(
            THREAD,
            "/Users/x/Screenshots/shot-2.png",
            &evidence(EvidenceSource::ImageText, "hello world output")
        ),
        Err(EvidenceProblem::DetailNotInText)
    );
}

#[test]
fn a_quote_from_the_delivered_text_is_accepted_across_casing_and_line_breaks() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed(
            "/Users/x/Screenshots/shot-2.png",
            Some("LinkedIn\nMessaging  3 new messages"),
            &[],
        )]),
    );

    for detail in [
        "Messaging 3 new messages",
        "messaging   3 new messages",
        // A typographic-quoted phrase spanning the delivered text's hard line break.
        "\u{201c}LinkedIn Messaging\u{201d}",
    ] {
        assert!(
            ledger
                .check(
                    THREAD,
                    "/Users/x/Screenshots/shot-2.png",
                    &evidence(EvidenceSource::ImageText, detail)
                )
                .is_ok(),
            "{detail:?} is in the delivered text"
        );
    }
}

#[test]
fn a_tag_claim_must_name_a_delivered_tag() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", None, &["sunset", "beach"])]),
    );

    assert_eq!(
        ledger.check(THREAD, "/x/a.png", &evidence(EvidenceSource::ImageTags, "sunset")),
        Ok(None)
    );
    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageTags, "sunset, beach")
        ),
        Ok(None),
        "a list of delivered tags is a tag claim"
    );
    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageTags, "sunset over water")
        ),
        Err(EvidenceProblem::DetailNotInTags),
        "prose wrapped around a delivered tag is not a tag claim"
    );
    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageTags, "invoice document")
        ),
        Err(EvidenceProblem::DetailNotInTags)
    );
}

/// An indexed image with nothing recognized in it is NOT evidence of content. The
/// distinction matters: the model was told "we looked and found no text".
#[test]
fn indexed_but_empty_facts_refuse_a_content_claim() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", None, &[])]),
    );

    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "some invoice total")
        ),
        Err(EvidenceProblem::NoTextInFacts)
    );
    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageTags, "a beach at sunset")
        ),
        Err(EvidenceProblem::NoTagsInFacts)
    );
}

/// A `notIndexed` row means the index has nothing for the path, so it must not become
/// evidence — otherwise "we don't know" would silently back a content name.
#[test]
fn a_not_indexed_row_never_enters_the_ledger() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![json!({ "path": "/x/a.png", "state": "notIndexed" })]),
    );

    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "anything at all")
        ),
        Err(EvidenceProblem::FactsNotDelivered)
    );
}

/// The elision seam: a result context assembly collapsed to a stub never reached the
/// model, so its facts stop being evidence.
#[test]
fn revoking_a_call_drops_exactly_that_calls_facts() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", Some("alpha invoice text"), &[])]),
    );
    ledger.record_delivered(
        THREAD,
        "call-2",
        &image_facts_result(vec![indexed("/x/b.png", Some("beta invoice text"), &[])]),
    );

    ledger.revoke_call("call-1");

    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "alpha invoice text")
        ),
        Err(EvidenceProblem::FactsNotDelivered)
    );
    assert!(
        ledger
            .check(
                THREAD,
                "/x/b.png",
                &evidence(EvidenceSource::ImageText, "beta invoice text")
            )
            .is_ok()
    );
}

/// These three claim nothing about content, so there's nothing to verify. They still
/// have to say something, and the review dialog labels them honestly.
#[test]
fn sources_that_claim_no_content_pass_with_an_empty_ledger() {
    let ledger = ImageFactsLedger::default();
    for source in [
        EvidenceSource::Filename,
        EvidenceSource::Metadata,
        EvidenceSource::UserInstruction,
    ] {
        assert!(!source.claims_image_content());
        assert_eq!(
            ledger.check(THREAD, "/x/a.png", &evidence(source, "Taken 2026-07-20")),
            Ok(None)
        );
    }
}

#[test]
fn a_blank_or_oversized_detail_is_refused_for_every_source() {
    let ledger = ImageFactsLedger::default();
    let long = "x".repeat(MAX_DETAIL_CHARS + 1);
    for source in [EvidenceSource::ImageText, EvidenceSource::Metadata] {
        assert_eq!(
            ledger.check(THREAD, "/x/a.png", &evidence(source, "  ")),
            Err(EvidenceProblem::DetailTooShort)
        );
        assert_eq!(
            ledger.check(THREAD, "/x/a.png", &evidence(source, &long)),
            Err(EvidenceProblem::DetailTooLong)
        );
    }
}

/// Normalization must fold identically on EVERY platform. Borrowing the path helper
/// (`normalize_for_comparison`) left this a no-op off macOS, so a correctly-quoted
/// `imageText` row was refused on Linux for nothing but its casing. Asserting the folded
/// output directly, rather than only round-tripping a check, is what pins that.
#[test]
fn detail_folding_is_platform_independent() {
    assert_eq!(
        normalize_detail("  LinkedIn\n Messaging   3 New "),
        "linkedin messaging 3 new"
    );
    assert_eq!(normalize_detail("'quoted'"), "quoted");
    // NFD is what makes a precomposed quote match decomposed OCR text (and vice versa),
    // so the two spellings of the same word must fold to one string.
    assert_eq!(
        normalize_detail("\u{201c}Årstaviken SUNSET\u{201d}"),
        normalize_detail("a\u{30a}rstaviken sunset")
    );
}

/// The bypass this check exists to stop: 160 characters of invented prose that merely
/// CONTAINS a near-universal delivered tag ("document", "screenshot", "text"). Under a
/// substring rule that passes, so a fabricated name reads as tag-backed. A tag claim must
/// name delivered tags and nothing else.
#[test]
fn invented_prose_around_a_real_tag_is_refused() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", None, &["document", "screenshot"])]),
    );

    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageTags, "Klarna invoice, 1,299 SEK, receipt document")
        ),
        Err(EvidenceProblem::DetailNotInTags),
        "prose that merely contains a delivered tag is not a tag claim"
    );
    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageTags, "document, screenshot")
        ),
        Ok(None),
        "a list of delivered tags is"
    );
}

/// What the review row needs to show how thin a match is: where the quote sits in the
/// delivered text, how much of it the quote covers, and the line it came from. Invariant
/// 12 — evidence proves the model READ something, never that the name is right, so the
/// surface has to let the user see the difference between a sliver and a decisive quote.
#[test]
fn a_matched_quote_reports_its_offset_length_and_surrounding_line() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed(
            "/x/a.png",
            Some("Order summary\nKlarna payment confirmation 1,299 SEK\nThank you"),
            &[],
        )]),
    );

    let coverage = ledger
        .check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "payment confirmation"),
        )
        .expect("the quote is in the delivered text")
        .expect("an image-text match reports its coverage");

    assert_eq!(coverage.matched_text, "payment confirmation");
    assert_eq!(coverage.matched_chars, 20);
    assert_eq!(coverage.match_offset, 21, "past 'Order summary\\n' and 'Klarna '");
    assert_eq!(coverage.delivered_chars, 61, "the whole delivered text, not the line");
    assert_eq!(coverage.context_before, "Klarna ");
    assert_eq!(coverage.context_after, " 1,299 SEK");
    assert!(!coverage.trimmed_before, "the line's start is inside the window");
    assert!(!coverage.trimmed_after);
}

/// The thin case this exists for: a decisive-looking quote buried in a page of OCR. The
/// window around it is capped, and the flags say it was cut, so the UI can show the cut
/// rather than implying the quote is the whole text.
#[test]
fn a_quote_inside_a_long_line_reports_a_capped_and_flagged_window() {
    let ledger = ImageFactsLedger::default();
    let filler = "abcdefghij ".repeat(20);
    let text = format!("{filler}total 1,299 SEK{filler}");
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", Some(&text), &[])]),
    );

    let coverage = ledger
        .check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "total 1,299 SEK"),
        )
        .expect("the quote is in the delivered text")
        .expect("coverage");

    assert_eq!(coverage.matched_chars, 15);
    assert_eq!(coverage.delivered_chars, text.chars().count());
    assert_eq!(coverage.context_before.chars().count(), CONTEXT_CHARS);
    assert_eq!(coverage.context_after.chars().count(), CONTEXT_CHARS);
    assert!(coverage.trimmed_before, "the line ran on before the window");
    assert!(coverage.trimmed_after);
}

/// The excerpt shows the DELIVERED spelling, not the folded one the matcher compares. The
/// model's own casing and spacing may differ from the image's, and what the user is being
/// asked to trust is what the image says.
#[test]
fn coverage_reports_the_delivered_spelling_not_the_normalized_one() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", Some("LinkedIn\nMessaging  3 new"), &[])]),
    );

    let coverage = ledger
        .check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "linkedin   MESSAGING"),
        )
        .expect("folding matches it")
        .expect("coverage");

    assert_eq!(coverage.matched_text, "LinkedIn\nMessaging");
    assert_eq!(coverage.match_offset, 0);
    assert_eq!(coverage.matched_chars, 18, "the span in the delivered text");
}

/// Coverage describes a span of delivered TEXT, so the sources that don't match a span
/// report none. A tag claim is membership, and the other three make no content claim.
#[test]
fn only_an_image_text_claim_reports_coverage() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", Some("Invoice 4021 total"), &["document"])]),
    );

    assert_eq!(
        ledger.check(THREAD, "/x/a.png", &evidence(EvidenceSource::ImageTags, "document")),
        Ok(None)
    );
    assert_eq!(
        ledger.check(
            THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::Metadata, "Taken 2026-07-20")
        ),
        Ok(None)
    );
}

/// Coverage is a fact about a delivery the ledger recorded, never something a plan can
/// assert. If the model could send one, "how thin is this match" would become a field it
/// writes, which is the whole failure this guardrail exists to refuse (invariant 6).
#[test]
fn a_model_supplied_coverage_does_not_parse() {
    let smuggled = json!({
        "source": "imageText",
        "detail": "Invoice 4021 total",
        "coverage": { "matchOffset": 0, "matchedChars": 3140, "deliveredChars": 3140 },
    });
    assert!(serde_json::from_value::<RenameEvidence>(smuggled).is_err());
}

/// The floor a quote has to clear. Eleven characters of a real receipt ("Klarna paym")
/// still isn't a phrase: short fragments appear in almost any screenshot, so the model can
/// satisfy the check with text it would have guessed anyway, and the review row shows a
/// sliver that reads as strong as a decisive quote. Twelve characters is the line.
///
/// A DATA-SAFETY assertion: this pair is what stops a thin match from backing a rename.
#[test]
fn an_image_text_quote_shorter_than_twelve_characters_is_refused() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed(
            "/x/a.png",
            Some("Klarna payment confirmation 1,299 SEK"),
            &[],
        )]),
    );

    assert_eq!(
        ledger.check(THREAD, "/x/a.png", &evidence(EvidenceSource::ImageText, "Klarna paym")),
        Err(EvidenceProblem::DetailTooShort),
        "11 characters of real delivered text is still too thin to back a name"
    );
    assert!(
        ledger
            .check(THREAD, "/x/a.png", &evidence(EvidenceSource::ImageText, "Klarna payme"))
            .is_ok(),
        "12 characters is the shortest quote that passes"
    );
}

/// The raised floor is for image TEXT only. The other sources describe things the user can
/// check for themselves ("old name", "IMG_4021"), so a short note stays usable there.
#[test]
fn the_twelve_character_floor_applies_to_image_text_only() {
    let ledger = ImageFactsLedger::default();
    for source in [
        EvidenceSource::Filename,
        EvidenceSource::Metadata,
        EvidenceSource::UserInstruction,
    ] {
        assert!(
            ledger.check(THREAD, "/x/a.png", &evidence(source, "IMG_4021")).is_ok(),
            "{source:?} takes a short note"
        );
    }
}

/// A short tag is still a tag. The minimum-length rule guards SUBSTRING matching against
/// image text; membership needs no such floor.
#[test]
fn a_short_delivered_tag_backs_a_claim() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", None, &["sky"])]),
    );

    assert_eq!(
        ledger.check(THREAD, "/x/a.png", &evidence(EvidenceSource::ImageTags, "sky")),
        Ok(None)
    );
}

/// Evidence is scoped to the chat thread that received it. A model in another thread
/// never read those facts, whatever its own context holds, so the ledger must not vouch
/// for them there: "what this model was handed" is the whole claim it exists to make.
#[test]
fn facts_delivered_in_one_thread_do_not_back_a_claim_in_another() {
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &image_facts_result(vec![indexed("/x/a.png", Some("Invoice 4021 total"), &["document"])]),
    );

    assert!(
        ledger
            .check(THREAD, "/x/a.png", &evidence(EvidenceSource::ImageText, "Invoice 4021"))
            .is_ok(),
        "the thread that fetched the facts can cite them"
    );
    assert_eq!(
        ledger.check(
            OTHER_THREAD,
            "/x/a.png",
            &evidence(EvidenceSource::ImageText, "Invoice 4021")
        ),
        Err(EvidenceProblem::FactsNotDelivered),
        "another thread never saw them"
    );
}

#[test]
fn evidence_parses_from_the_camel_case_wire_shape() {
    let parsed: RenameEvidence =
        serde_json::from_value(json!({ "source": "imageText", "detail": "Invoice 4021" })).expect("valid evidence");
    assert_eq!(parsed.source, EvidenceSource::ImageText);
    assert_eq!(parsed.detail, "Invoice 4021");
    assert!(serde_json::from_value::<RenameEvidence>(json!({ "source": "guess", "detail": "x" })).is_err());
}
