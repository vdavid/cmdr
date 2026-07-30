//! The eval that outlives the fix: can a human still catch a wrong name?
//!
//! Every other guardrail in this module answers "did the model read something?". This answers
//! the question no guardrail can: **the model read the file, quoted it verbatim, and named it
//! wrong anyway.** A Klarna payment confirmation named `klarna-invoice`, backed by a real
//! quote, passes every check we have — and should, because refusing it would need us to
//! understand the image.
//!
//! So this measures OUR REVIEW SURFACE, not the model's taste: for each fixture it asks whether
//! the row that reaches the dialog carries what a human needs in order to disagree — the typed
//! provenance, the matched text as delivered, where it sat, and how much of the file it covers.
//! It asserts nothing about name quality, because name quality is exactly what the human is for.
//!
//! **Why it isn't just more unit tests.** "An invented quote is refused" is already pinned in
//! `evidence/tests.rs`; asserting it here too would be a tautology that passes forever while the
//! real gap widens. The fixtures below are chosen so that a regression in the REVIEW surface
//! (coverage dropped, provenance flattened, a claim accepted with nothing to show for it) fails
//! this module even though every existing test still passes.
//!
//! Offline and deterministic: fixtures in, ledger and evidence check as they ship, verdicts out.
//! No provider, no network, no clock.

use serde_json::{Value, json};

use super::evidence::{EvidenceProblem, EvidenceScope, EvidenceSource, ImageFactsLedger, RenameEvidence};

/// One screenshot with known contents, the name a model proposed for it, and whether that name
/// is a fair reading of what the file actually says.
struct Fixture {
    /// What this case exists to catch, in one line.
    what: &'static str,
    path: &'static str,
    /// The recognized text `image_facts` delivers for this file.
    delivered_text: &'static str,
    /// Vision tags delivered for this file.
    delivered_tags: &'static [&'static str],
    /// The evidence the model attaches to its proposed name.
    claimed: (EvidenceSource, &'static str),
    /// Whether the proposed name is a fair reading of the file. **Never asserted on directly**:
    /// it's here so a reader can see that fair and unfair names travel the same path, and that
    /// only the human can tell them apart.
    name_is_fair: bool,
}

/// What actually happened to a row, as the eval scores it.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// The claim was refused at the tool boundary; the user never sees the row.
    Refused(EvidenceProblem),
    /// The claim was accepted and the row reaches the review dialog. `reviewable` is true when
    /// it carries the facts a human needs to disagree: the matched text as DELIVERED (not the
    /// model's retyping), a locatable position, and a real delivered-length to weigh it against.
    Accepted { reviewable: bool, coverage_percent: f64 },
}

/// A payment confirmation named as an invoice: the failure this whole plan exists for. The quote
/// is real, verbatim, and decisive — and the name is still wrong.
const KLARNA_TEXT: &str = "Klarna\nPayment confirmation\nYour payment of 1,299 SEK to Elgiganten went through.\nOrder 4471-8823. Nothing further to pay.";

const FIXTURES: &[Fixture] = &[
    Fixture {
        what: "a genuine, verbatim quote supporting a materially wrong name (the incident case)",
        path: "/shots/CleanShot 2026-07-24 at 19.36.00.png",
        delivered_text: KLARNA_TEXT,
        delivered_tags: &["screenshot", "document", "text"],
        // "Payment confirmation" is really in the text. The proposed name says invoice.
        claimed: (EvidenceSource::ImageText, "Payment confirmation"),
        name_is_fair: false,
    },
    Fixture {
        what: "the same file, named fairly, travels the identical path",
        path: "/shots/CleanShot 2026-07-24 at 19.36.01.png",
        delivered_text: KLARNA_TEXT,
        delivered_tags: &["screenshot", "document"],
        claimed: (EvidenceSource::ImageText, "Payment confirmation"),
        name_is_fair: true,
    },
    Fixture {
        what: "a twelve-character quote inside a page of OCR: accepted, and visibly thin",
        path: "/shots/CleanShot 2026-07-24 at 19.36.02.png",
        delivered_text: "Invoice 8821\nDue 2026-08-01\nSubtotal 1,040.00 SEK\nVAT 260.00 SEK\nTotal 1,300.00 SEK\nPay by bank transfer to 5368-1129. Late payment carries interest at 8% per year, per our terms of sale, which you accepted at checkout.",
        delivered_tags: &["document", "text"],
        claimed: (EvidenceSource::ImageText, "Invoice 8821"),
        name_is_fair: true,
    },
    Fixture {
        what: "a fabricated quote: refused, so it never reaches the human at all",
        path: "/shots/CleanShot 2026-07-24 at 19.36.03.png",
        delivered_text: KLARNA_TEXT,
        delivered_tags: &["screenshot"],
        claimed: (EvidenceSource::ImageText, "Invoice from Elgiganten"),
        name_is_fair: false,
    },
    Fixture {
        what: "a tag claim: accepted with no coverage, because a tag is not a quote in a text",
        path: "/shots/CleanShot 2026-07-24 at 19.36.04.png",
        delivered_text: KLARNA_TEXT,
        delivered_tags: &["receipt", "document"],
        claimed: (EvidenceSource::ImageTags, "receipt"),
        name_is_fair: true,
    },
    Fixture {
        what: "a metadata name: no content claim, and the row must say so rather than imply one",
        path: "/shots/CleanShot 2026-07-24 at 19.36.05.png",
        delivered_text: KLARNA_TEXT,
        delivered_tags: &["screenshot"],
        claimed: (EvidenceSource::Metadata, "taken 2026-07-24"),
        name_is_fair: true,
    },
];

/// The `image_facts` result shape, exactly as the tool serializes it, so the eval exercises the
/// real recording path rather than a convenient shortcut.
fn facts_result(fixture: &Fixture) -> Value {
    json!({
        "facts": [{
            "path": fixture.path,
            "state": "indexed",
            "text": fixture.delivered_text,
            "tags": fixture.delivered_tags
                .iter()
                .map(|label| json!({ "label": label, "score": 0.9 }))
                .collect::<Vec<_>>(),
        }]
    })
}

/// Run one fixture through the shipped ledger and evidence check, and score what the review
/// dialog would receive.
fn evaluate(fixture: &Fixture) -> Verdict {
    let ledger = ImageFactsLedger::default();
    let scope = EvidenceScope::Thread(1);
    ledger.record_delivered(scope, "call-1", &facts_result(fixture));

    let evidence = RenameEvidence {
        source: fixture.claimed.0,
        detail: fixture.claimed.1.to_string(),
    };
    match ledger.check(scope, fixture.path, &evidence) {
        Err(problem) => Verdict::Refused(problem),
        Ok(None) => Verdict::Accepted {
            // No coverage is correct for a claim that isn't a quote in a text (a tag, a filename,
            // metadata): the typed source already tells the human this name rests on something
            // other than the file's contents.
            //
            // For an ImageText claim it is NOT correct, and that is the M1 regression this eval
            // exists to catch: a quote accepted with nothing to locate it leaves the human the
            // model's own retyping and no way to weigh it.
            reviewable: fixture.claimed.0 != EvidenceSource::ImageText,
            coverage_percent: 0.0,
        },
        Ok(Some(coverage)) => {
            // What the human actually needs: the matched span AS DELIVERED (so they compare the
            // file's own words, not the model's retyping), the line it came from, and a real
            // total to weigh it against.
            //
            // Surrounding context is deliberately NOT required: a quote can be an entire line of
            // its own ("Payment confirmation" is, in the incident text), and then there is no
            // surrounding text to show. Demanding it would fail the very case this eval exists
            // for. What must always hold is that the span is real and locatable in what was
            // delivered.
            let reviewable = !coverage.matched_text.is_empty()
                && coverage.delivered_chars > 0
                && coverage.matched_chars > 0
                && coverage.match_offset + coverage.matched_chars <= coverage.delivered_chars;
            let coverage_percent = coverage.matched_chars as f64 / coverage.delivered_chars as f64 * 100.0;
            Verdict::Accepted {
                reviewable,
                coverage_percent,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hard constraint. **This is the milestone's whole point.** A real quote behind a wrong name
    /// must reach the human, carrying what they need to disagree — not be refused (impossible
    /// without understanding the image), and not arrive as a bare assertion either.
    ///
    /// It fails if M1's coverage regresses: drop `EvidenceCoverage`, stop reporting the delivered
    /// spelling, or return a match the offsets don't support, and `reviewable` goes false here
    /// while every existing unit test still passes.
    #[test]
    fn a_real_quote_behind_a_wrong_name_reaches_the_human_with_something_to_judge() {
        let incident = &FIXTURES[0];
        assert!(!incident.name_is_fair, "fixture 0 is the wrong-name case");

        match evaluate(incident) {
            Verdict::Accepted { reviewable, .. } => assert!(
                reviewable,
                "{}: the row reached review with nothing for the human to weigh",
                incident.what
            ),
            Verdict::Refused(problem) => panic!(
                "{}: refused as {problem:?}. A verbatim quote CANNOT be refused — we'd have to \
                 understand the image to know the name is wrong, so this row must reach the human",
                incident.what
            ),
        }
    }

    /// Hard constraint. A fair name and an unfair one, on identical delivered text, must be
    /// indistinguishable to the backend. If they ever diverge, something started judging name
    /// quality, which is the human's job and a thing we would get wrong.
    #[test]
    fn a_fair_name_and_an_unfair_one_are_indistinguishable_to_the_guardrails() {
        let unfair = evaluate(&FIXTURES[0]);
        let fair = evaluate(&FIXTURES[1]);
        assert_eq!(
            unfair, fair,
            "the same delivered text and the same quote must score identically, whatever the name"
        );
    }

    /// Hard constraint. Every accepted content claim arrives reviewable. A row the human can't
    /// weigh is a row they will approve on faith, which is how 12 files got fabricated names.
    #[test]
    fn every_accepted_row_carries_what_a_human_needs_to_disagree() {
        for fixture in FIXTURES {
            if let Verdict::Accepted { reviewable, .. } = evaluate(fixture) {
                assert!(reviewable, "{}: accepted but not reviewable", fixture.what);
            }
        }
    }

    /// Hard constraint, and the one case that IS refusable: nothing behind the words at all.
    #[test]
    fn a_fabricated_quote_never_reaches_the_human() {
        let invented = &FIXTURES[3];
        assert_eq!(
            evaluate(invented),
            Verdict::Refused(EvidenceProblem::DetailNotInText),
            "{}",
            invented.what
        );
    }

    /// Hard constraint. A thin match must be VISIBLY thin: the percentage the review row shows
    /// has to separate a decisive quote from a bare hit. Both are accepted; only the human
    /// decides.
    #[test]
    fn a_thin_match_and_a_decisive_one_report_different_coverage() {
        let Verdict::Accepted {
            coverage_percent: thin, ..
        } = evaluate(&FIXTURES[2])
        else {
            panic!("a real 12-character quote is accepted, and left for the human to weigh")
        };
        let Verdict::Accepted {
            coverage_percent: decisive,
            ..
        } = evaluate(&FIXTURES[0])
        else {
            panic!("the incident quote is accepted")
        };

        assert!(
            thin < decisive / 2.0,
            "a 12-character hit in a page of OCR ({thin:.1}%) must read as far weaker than a \
             decisive quote ({decisive:.1}%), or the review row makes them look alike"
        );
    }

    /// The soft tier, in the shape `importance::evals` established: one scalar with a FIXED
    /// floor, never a self-updating ratchet. It answers "what share of the rows a user reviews
    /// arrive judgeable?", and 1.0 is the only honest answer today — the floor is here so that a
    /// change which quietly drops evidence from some rows fails, rather than passing because the
    /// remaining rows still look fine.
    const REVIEWABLE_SHARE_FLOOR: f64 = 1.0;

    #[test]
    fn the_reviewable_share_holds_its_floor() {
        let accepted: Vec<Verdict> = FIXTURES
            .iter()
            .map(evaluate)
            .filter(|verdict| matches!(verdict, Verdict::Accepted { .. }))
            .collect();
        assert!(!accepted.is_empty(), "the corpus must contain accepted rows to score");

        let reviewable = accepted
            .iter()
            .filter(|verdict| matches!(verdict, Verdict::Accepted { reviewable: true, .. }))
            .count();
        let share = reviewable as f64 / accepted.len() as f64;

        assert!(
            share >= REVIEWABLE_SHARE_FLOOR,
            "{reviewable} of {} accepted rows are judgeable ({share:.2}), below the {REVIEWABLE_SHARE_FLOOR:.2} floor",
            accepted.len()
        );
    }
}
