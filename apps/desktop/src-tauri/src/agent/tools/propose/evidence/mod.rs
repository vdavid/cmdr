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
/// variant rather than sniffing wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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
    /// The user typed this name in the review dialog. No content claim, and no evidence at
    /// all: the name IS the decision (invariant 10).
    ///
    /// The review's revise path is the only thing that may set it. A plan that sends it is
    /// refused ([`EvidenceProblem::SourceReservedForUser`]), because "You typed this name"
    /// beside a model-invented name is the exact misattribution this module exists to stop.
    UserEdited,
}

impl EvidenceSource {
    /// Whether this source claims the model read what's INSIDE the file. Only these are
    /// checked against the ledger; the rest make no claim to verify.
    pub fn claims_image_content(self) -> bool {
        matches!(self, EvidenceSource::ImageText | EvidenceSource::ImageTags)
    }
}

impl RenameEvidence {
    /// What a row carries once the user typed its name: the honest source and nothing else.
    /// Never the model's quote — that described the model's name, not this one.
    pub fn user_edited() -> Self {
        RenameEvidence {
            source: EvidenceSource::UserEdited,
            detail: String::new(),
        }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
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
    /// The row claims the USER typed this name. Only the review dialog's revise path can say
    /// that, so a plan claiming it is refused rather than believed.
    SourceReservedForUser,
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

impl EvidenceScope {
    /// The chat thread this call belongs to, for anything that records WHICH conversation
    /// asked (a proposal sweep's provenance). Distinct from the ledger's use of the scope,
    /// which is about what a claim may cite; this is only the thread's id.
    pub fn conversation_id(self) -> Option<i64> {
        match self {
            EvidenceScope::Thread(id) => Some(id),
            EvidenceScope::NoThread => None,
        }
    }
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
        // A claim that the user typed the name can only come from the review dialog, so it is
        // refused here before anything else is weighed.
        if evidence.source == EvidenceSource::UserEdited {
            return Err(EvidenceProblem::SourceReservedForUser);
        }
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
            EvidenceSource::Filename
            | EvidenceSource::Metadata
            | EvidenceSource::UserInstruction
            | EvidenceSource::UserEdited => MIN_DETAIL_CHARS,
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
            EvidenceSource::Filename
            | EvidenceSource::Metadata
            | EvidenceSource::UserInstruction
            | EvidenceSource::UserEdited => Ok(None),
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
    cmdr_index::store::normalize_for_comparison(path)
}

#[cfg(test)]
mod tests;
