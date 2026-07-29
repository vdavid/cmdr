//! The backing every proposed rename name must carry, and the ledger that makes a
//! content claim checkable.
//!
//! A rename plan is the one place the agent's words turn into changed user data, so a
//! name that claims to come from what's INSIDE a file has to be provable, not trusted.
//! Each plan item declares a typed [`EvidenceSource`] plus a short `detail`; a source
//! that claims image content ([`EvidenceSource::ImageText`] / [`EvidenceSource::ImageTags`])
//! is checked against [`ImageFactsLedger`], which records what `image_facts` actually
//! handed the model. A claim with nothing behind it is refused at the tool boundary, so
//! the user never sees a plan item whose evidence didn't check out.
//!
//! The other three sources make no content claim and are always accepted, but they still
//! reach the review dialog verbatim, where the UI names them honestly ("file details, not
//! contents"). So a name invented out of thin air can't hide behind `metadata` either: it
//! shows up as one, and the reviewer sees no content behind it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ignore_poison::IgnorePoison;

/// How long a delivered set of image facts stays valid evidence. Long enough for a
/// multi-batch naming session (look at 23 files, propose in two plans, refine once), short
/// enough that yesterday's facts can't back today's name.
const FACTS_TTL: Duration = Duration::from_secs(30 * 60);

/// The most paths the ledger holds. `image_facts` caps one call at 200 paths, so this is
/// several full batches; past it the oldest entries go, which only ever costs a refusal
/// the model can fix by asking again.
const MAX_LEDGER_ENTRIES: usize = 1_000;

/// The shortest `detail` that can back an image-TEXT claim, in normalized characters. Matching
/// is by substring against up to a page of OCR, so a short fragment ("Card", "Total") appears
/// in almost any receipt or screenshot: the model can satisfy the check with text it would have
/// guessed anyway, and the review row shows a sliver that reads exactly as strong as a decisive
/// quote. Twelve characters is a phrase.
const MIN_IMAGE_TEXT_CHARS: usize = 12;

/// The shortest `detail` any source can carry. The three no-claim sources describe something
/// the user can check for themselves ("old name", "IMG_4021"), so they only have to say
/// something. A tag claim has no floor at all: membership in the delivered tag set is the
/// proof, and real tags like "sky" are short.
const MIN_DETAIL_CHARS: usize = 4;

/// The longest `detail` the tool accepts, in characters. It's a short quote or note for a
/// human reviewing a table row, not a place to paste a page of OCR text.
const MAX_DETAIL_CHARS: usize = 160;

/// How much delivered text to report on each side of a matched quote, in characters. Enough
/// to see the line the quote came from, short enough for a table cell.
const CONTEXT_CHARS: usize = 60;

// ── The typed evidence an item carries ────────────────────────────────────────

/// Where a proposed name came from. Typed, so the UI and the validator branch on a
/// variant rather than sniffing wording (`no-string-matching`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceSource {
    /// Text recognized inside the image, as `image_facts` delivered it.
    ImageText,
    /// Vision tags for the image, as `image_facts` delivered them.
    ImageTags,
    /// The file's existing name (no content claim).
    Filename,
    /// Dates, size, or other metadata the agent already had (no content claim).
    Metadata,
    /// A naming rule the user stated in the conversation (no content claim).
    UserInstruction,
}

impl EvidenceSource {
    /// Whether this source claims the model read what's INSIDE the file. Only these are
    /// checked against the ledger; the rest make no claim to verify.
    pub fn claims_image_content(self) -> bool {
        matches!(self, EvidenceSource::ImageText | EvidenceSource::ImageTags)
    }
}

/// One item's evidence: the typed source plus the short quote or note behind it.
///
/// `detail` is MODEL-AUTHORED TEXT that reaches the review dialog. The frontend renders it
/// as plain text (never `{@html}`), and its length is bounded here.
///
/// `deny_unknown_fields` keeps the plan schema closed: the row's coverage is a fact this
/// module derives from the ledger's own delivery, so a plan that tries to send one is refused
/// rather than believed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameEvidence {
    pub source: EvidenceSource,
    pub detail: String,
}

/// How much of the delivered text a quote actually covers, and the line it came from.
///
/// Computed HERE, from the delivery the check just matched against, for one purpose: the
/// review row must show that a 7-character hit inside 3,140 characters of OCR is thin, where
/// a bare quote made it look exactly as strong as a decisive one (invariant 12).
///
/// **Serialize only, on purpose.** This is a display fact about a delivery that already
/// validated, never an input: if a plan could send it, "how thin is this match" would become
/// a field the model writes, and evidence has to stay a fact about what the ledger recorded
/// (invariant 6). It is also not a second way to pass validation — a row only ever gets
/// coverage after [`ImageFactsLedger::check`] has already accepted it.
///
/// Every count is in characters of the DELIVERED text (`image_facts` caps that at 2,000), so
/// the UI's "matched 7 of 3,140 characters" describes what the model was actually handed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCoverage {
    /// Where the match starts in the delivered text.
    pub match_offset: usize,
    /// How long the matched span is. Can exceed the quote's own length: folding collapses
    /// whitespace runs, so one quoted space may cover a line break plus indentation.
    pub matched_chars: usize,
    /// How much recognized text `image_facts` delivered for this file.
    pub delivered_chars: usize,
    /// The delivered text just before the match, within its line and capped at
    /// [`CONTEXT_CHARS`].
    pub context_before: String,
    /// The matched span as DELIVERED. The model's `detail` may differ in casing and spacing,
    /// and what the user is asked to trust is what the image says.
    pub matched_text: String,
    /// The delivered text just after the match, within its line and capped.
    pub context_after: String,
    /// Whether the line ran on past the window, so the UI shows the cut.
    pub trimmed_before: bool,
    pub trimmed_after: bool,
}

// ── Why a claim didn't check out ───────────────────────────────────────────────

/// Why one item's evidence was refused. A typed variant the model can act on, and never a
/// message substring anything branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceProblem {
    /// No image facts for this path ever reached the model, so it can't have read it.
    /// This is the fabrication case: a content-shaped name with no content behind it.
    FactsNotDelivered,
    /// Facts reached the model, but the image had no recognized text.
    NoTextInFacts,
    /// Facts reached the model, but the image had no tags.
    NoTagsInFacts,
    /// The quote isn't in the text that was delivered for this path.
    DetailNotInText,
    /// The detail names no tag that was delivered for this path.
    DetailNotInTags,
    /// Blank, or too short to prove anything.
    DetailTooShort,
    /// Past the length a review row can honestly show.
    DetailTooLong,
}

/// One refused plan item, as the tool result reports it back to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRejection {
    pub source_path: String,
    pub proposed_name: String,
    pub evidence_source: EvidenceSource,
    pub problem: EvidenceProblem,
}

// ── Which thread evidence belongs to ─────────────────────────────────────────

/// Whose delivery this is, and whose claim may cite it. Evidence NEVER crosses threads: a
/// model in another chat thread never read those facts, whatever its own context holds, so
/// vouching across threads would weaken the ledger's one claim to "what this model was
/// handed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceScope {
    /// An Ask Cmdr chat thread, by its conversation id.
    Thread(i64),
    /// A caller with no chat thread: the shared registry path an external MCP client uses.
    /// Nothing is ever recorded against it, so it can't back a content claim (fails closed).
    NoThread,
}

// ── The ledger ────────────────────────────────────────────────────────────────

/// A ledger lookup: one path, in one thread. Two threads asking about the same file are
/// two different questions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LedgerKey {
    scope: EvidenceScope,
    path: String,
}

/// One path's image facts, exactly as the model received them.
struct DeliveredFacts {
    /// The tool call that delivered them, so the elision seam can revoke a whole call.
    call_id: String,
    /// The recognized text as delivered (already capped by `image_facts`), so a quote is
    /// checked against what the model could actually read, not against the full stored row.
    text: String,
    tags: Vec<String>,
    at: Instant,
}

/// What `image_facts` delivered to the model, per path. Registered in managed state by
/// `agent::start` and written by the agent's tool dispatcher.
///
/// **Fails closed**: a path with no live entry can't back a content claim. So a delivery
/// that never happened, expired, or was revoked refuses the claim rather than trusting it.
#[derive(Default)]
pub struct ImageFactsLedger {
    entries: Mutex<HashMap<LedgerKey, DeliveredFacts>>,
}

impl ImageFactsLedger {
    /// Record what one `image_facts` result delivered. Only `indexed` rows land: a
    /// `notIndexed` row tells the model "nothing is stored", which is the opposite of
    /// evidence. Called by the agent dispatcher for a result that reached the model.
    pub fn record_delivered(&self, scope: EvidenceScope, call_id: &str, content: &Value) {
        let Some(facts) = content.get("facts").and_then(Value::as_array) else {
            return;
        };
        let mut entries = self.entries.lock_ignore_poison();
        entries.retain(|_, delivered| delivered.at.elapsed() < FACTS_TTL);
        for row in facts {
            if row.get("state").and_then(Value::as_str) != Some("indexed") {
                continue;
            }
            let Some(path) = row.get("path").and_then(Value::as_str) else {
                continue;
            };
            entries.insert(
                LedgerKey {
                    scope,
                    path: path_key(path),
                },
                DeliveredFacts {
                    call_id: call_id.to_string(),
                    text: row.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
                    tags: row
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|tags| {
                            tags.iter()
                                .filter_map(|tag| tag.get("label").and_then(Value::as_str))
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    at: Instant::now(),
                },
            );
        }
        // Keep the map bounded by dropping the oldest deliveries first.
        while entries.len() > MAX_LEDGER_ENTRIES {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, delivered)| delivered.at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
    }

    /// Drop everything one tool call delivered, because it did NOT reach the model after
    /// all (context assembly collapsed the result to a stub). The seam that keeps the
    /// ledger honest about delivery rather than about dispatch.
    pub fn revoke_call(&self, call_id: &str) {
        self.entries
            .lock_ignore_poison()
            .retain(|_, delivered| delivered.call_id != call_id);
    }

    /// Check one item's evidence. Content claims are checked against what was delivered;
    /// the other sources only have to be a usable note.
    ///
    /// An accepted image-TEXT claim also reports its [`EvidenceCoverage`], so the review row
    /// can show how much of the delivered text the quote actually covers. Every other source
    /// reports `None`: there's no span to measure.
    pub fn check(
        &self,
        scope: EvidenceScope,
        source_path: &str,
        evidence: &RenameEvidence,
    ) -> Result<Option<EvidenceCoverage>, EvidenceProblem> {
        // Length first, so an oversized detail is rejected before it's worth normalizing.
        if evidence.detail.chars().count() > MAX_DETAIL_CHARS {
            return Err(EvidenceProblem::DetailTooLong);
        }
        let detail = normalize_detail(&evidence.detail);
        // The tall floor guards SUBSTRING matching (a short fragment hits any text). Tag claims
        // are checked by membership, so a short real tag must not trip any floor; the no-claim
        // sources only have to say something.
        let floor = match evidence.source {
            EvidenceSource::ImageText => MIN_IMAGE_TEXT_CHARS,
            EvidenceSource::ImageTags => 1,
            EvidenceSource::Filename | EvidenceSource::Metadata | EvidenceSource::UserInstruction => MIN_DETAIL_CHARS,
        };
        if detail.chars().count() < floor {
            return Err(EvidenceProblem::DetailTooShort);
        }
        if !evidence.source.claims_image_content() {
            return Ok(None);
        }
        let entries = self.entries.lock_ignore_poison();
        let delivered = entries
            .get(&LedgerKey {
                scope,
                path: path_key(source_path),
            })
            .filter(|delivered| delivered.at.elapsed() < FACTS_TTL)
            .ok_or(EvidenceProblem::FactsNotDelivered)?;
        match evidence.source {
            EvidenceSource::ImageText => check_quote(&delivered.text, &detail).map(Some),
            EvidenceSource::ImageTags => check_tags(&delivered.tags, &detail).map(|()| None),
            EvidenceSource::Filename | EvidenceSource::Metadata | EvidenceSource::UserInstruction => Ok(None),
        }
    }
}

/// The quote has to appear in the text the model was handed, and where it appears is what the
/// review row reports. Normalized on both sides, so re-wrapped OCR whitespace and casing don't
/// cause a false refusal, but an invented phrase can't pass.
fn check_quote(text: &str, detail: &str) -> Result<EvidenceCoverage, EvidenceProblem> {
    if text.trim().is_empty() {
        return Err(EvidenceProblem::NoTextInFacts);
    }
    locate_quote(text, detail).ok_or(EvidenceProblem::DetailNotInText)
}

/// Find the folded `detail` in `text` and describe the match as a span of the ORIGINAL text:
/// the acceptance decision and the display facts come from one search, so the row can never
/// show coverage for a match the check didn't make.
fn locate_quote(text: &str, detail: &str) -> Option<EvidenceCoverage> {
    let detail_chars = detail.chars().count();
    if detail_chars == 0 {
        return None;
    }
    let (folded, origins) = normalize_with_origins(text);
    let folded_start = folded[..folded.find(detail)?].chars().count();
    // Fold-to-source is per character, so the span's ends come from its first and last
    // characters; everything between them belongs to the match by construction.
    let match_start = *origins.get(folded_start)?;
    let match_end = *origins.get(folded_start + detail_chars - 1)? + 1;
    let chars: Vec<char> = text.chars().collect();
    let line_start = chars[..match_start]
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |i| i + 1);
    let line_end = chars[match_end..]
        .iter()
        .position(|c| *c == '\n')
        .map_or(chars.len(), |i| match_end + i);
    let window_start = line_start.max(match_start.saturating_sub(CONTEXT_CHARS));
    let window_end = line_end.min(match_end + CONTEXT_CHARS);
    Some(EvidenceCoverage {
        match_offset: match_start,
        matched_chars: match_end - match_start,
        delivered_chars: chars.len(),
        context_before: chars[window_start..match_start].iter().collect(),
        matched_text: chars[match_start..match_end].iter().collect(),
        context_after: chars[match_end..window_end].iter().collect(),
        trimmed_before: window_start > line_start,
        trimmed_after: window_end < line_end,
    })
}

/// The detail must be delivered tags and NOTHING else: a comma- or semicolon-separated list
/// where every part names a tag this path actually got.
///
/// Deliberately NOT "the detail contains a delivered tag". That direction lets 160 characters
/// of invented prose pass on one near-universal tag ("document", "screenshot", "text"), which
/// is fabrication wearing a badge — the exact thing this ledger exists to refuse.
fn check_tags(tags: &[String], detail: &str) -> Result<(), EvidenceProblem> {
    if tags.is_empty() {
        return Err(EvidenceProblem::NoTagsInFacts);
    }
    let delivered: Vec<String> = tags.iter().map(|tag| normalize_detail(tag)).collect();
    let mut named = 0usize;
    for part in detail.split([',', ';']) {
        let part = normalize_detail(part);
        if part.is_empty() {
            continue;
        }
        if !delivered.iter().any(|tag| !tag.is_empty() && *tag == part) {
            return Err(EvidenceProblem::DetailNotInTags);
        }
        named += 1;
    }
    if named > 0 {
        Ok(())
    } else {
        Err(EvidenceProblem::DetailNotInTags)
    }
}

/// Fold text to its comparable form: unwrap a surrounding quote pair, NFD-decompose,
/// lowercase, and collapse every whitespace run to one space. OCR text arrives with hard
/// line breaks and odd spacing, and models like to wrap a quote in typographic quotes.
///
/// Deliberately NOT `indexing::store::normalize_for_comparison`: that one carries PATH
/// comparison semantics, which are platform-dependent (a no-op off macOS), so borrowing it
/// here made a quote case-sensitive on Linux. Evidence text must compare identically
/// everywhere; `path_key` keeps the path helper, where platform semantics are correct.
fn normalize_detail(text: &str) -> String {
    let trimmed = text
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’'));
    normalize_with_origins(trimmed).0
}

/// The folding above, plus the source character index each folded character came from. That
/// map is what lets a match found in folded text be reported back as a span of the delivered
/// text the user is looking at, rather than as lowercased, whitespace-collapsed output.
///
/// Both callers share this one implementation deliberately: a quote is matched against text
/// folded here, so a second folding path could refuse a correct quote for nothing.
fn normalize_with_origins(text: &str) -> (String, Vec<usize>) {
    use unicode_normalization::UnicodeNormalization;
    let mut folded = String::with_capacity(text.len());
    let mut origins = Vec::with_capacity(text.len());
    // A whitespace run becomes one space, credited to where the run started. Held back until
    // a non-space follows, so leading and trailing runs vanish (as `split_whitespace` does).
    let mut pending_space: Option<usize> = None;
    for (index, source) in text.chars().enumerate() {
        if source.is_whitespace() {
            if !folded.is_empty() && pending_space.is_none() {
                pending_space = Some(index);
            }
            continue;
        }
        if let Some(space_index) = pending_space.take() {
            folded.push(' ');
            origins.push(space_index);
        }
        for lowered in source.nfd().flat_map(char::to_lowercase) {
            folded.push(lowered);
            origins.push(index);
        }
    }
    (folded, origins)
}

/// The ledger's key for a path: the same normalization the rest of the rename flow uses,
/// so a case- or Unicode-different spelling of the same path still matches.
fn path_key(path: &str) -> String {
    crate::indexing::store::normalize_for_comparison(path)
}

#[cfg(test)]
mod tests {
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
}
