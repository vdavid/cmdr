# Rename proposal details

## Evidence: why a proposed name is believable

A model asked to name files by their contents will produce a confident, plausible name whether or not it ever saw the
contents. So the tool boundary, not the model's goodwill, decides whether a content-derived name may reach the user.

Each row's `evidence` is a required `{ source, detail }` pair. `EvidenceSource` splits into two classes:

- **Content claims** (`imageText`, `imageTags`): checked against `ImageFactsLedger`. The path needs a live delivery, and
  `detail` must be a real quote from the delivered text (normalized on both sides: NFD-lowercased, whitespace runs
  collapsed, surrounding quote characters stripped) or name one of the delivered tags. Every refusal is a typed
  `EvidenceProblem`, so nothing classifies on wording.
- **No claim** (`filename`, `metadata`, `userInstruction`): always accepted, because there's nothing to verify. They
  still cross to the review dialog verbatim, where the UI labels them as reading nothing inside the file. That's the
  other half of the guardrail: a fabricated slug filed under `metadata` is visible as a name with no content behind it,
  rather than hidden. See `apps/desktop/src/lib/ask-cmdr/DETAILS.md` § The "Why this name" column.

`detail` is bounded at both ends (160 normalized characters maximum, because a review row can't honestly show a page of
OCR output), and its minimum is per source:

- **`imageText`: 12 characters** (`MIN_IMAGE_TEXT_CHARS`). Matching is by substring against up to a page of OCR, so a
  short fragment ("Card", "Total") appears in almost any receipt: the model could satisfy the check with text it would
  have guessed anyway, and the row would show a sliver that reads as strong as a decisive quote. Twelve is a phrase.
- **`filename` / `metadata` / `userInstruction`: 4 characters** (`MIN_DETAIL_CHARS`). They describe something the user
  can check for themselves ("old name", "IMG_4021"), so they only have to say something.
- **`imageTags`: no floor.** Membership in the delivered tag set is the proof, and real tags (`sky`) are short.

**Decision: a tag claim lists delivered tags and nothing else.** `check_tags` requires every comma- or
semicolon-separated part of `detail` to equal a delivered tag. The tempting direction (does the detail CONTAIN a
delivered tag?) is a hole: tags like `document`, `screenshot`, and `text` are near-universal in a screenshot corpus, so
160 characters of invented prose passes on one of them, and a fabricated name reads as tag-backed. The length floors
therefore skip `imageTags` entirely: membership needs no floor, and real tags (`sky`) are short.

**Decision: one unbacked row refuses the whole plan.** Staging the survivors would hand the user a partial plan they'd
read as complete, and the model has to resend the plan either way. The refusal names every offending row
(`evidenceRejected` + `guidance`), so no rejected row is dropped silently. The model can fix the rows or say honestly
that it doesn't have the content.

**Decision: the ledger is keyed by chat thread, and holds each delivery for 30 minutes; it is not per-turn.** Two
independent bounds, each answering a different question:

- **Per thread** (`EvidenceScope::Thread(conversation_id)`, part of the lookup key): "was this handed to the model that's
  making the claim?" A different thread's model never read those facts, whatever its own context holds, so a delivery
  there must not vouch here. `EvidenceScope::NoThread` covers the shared registry path an external MCP client uses:
  nothing is ever recorded against it, so it can back no content claim.
- **Not per turn** (the 30-minute TTL): the real flow is multi-batch (look at 23 files, propose in two plans, refine
  after feedback), and a user's "now do the rest" starts a fresh turn. Per-turn scoping would refuse the honest second
  half of exactly the workflow this exists to protect, while the TTL still stops yesterday's facts backing today's name.

`MAX_LEDGER_ENTRIES` bounds memory by dropping the oldest deliveries across all threads.

### The ledger's seams

- **Write**: `view.rs`'s dispatch calls `note_image_facts_delivered` for every `image_facts` result whose
  `AgentToolResult.elided` is false. That's the only write point.
- **Revoke**: `ImageFactsLedger::revoke_call(call_id)` exists because "the tool returned it" is not "the model read it".
  The incident this guardrail comes from was context assembly collapsing a fresh `image_facts` result to a stub: the
  facts were fetched and then dropped from the prompt, and the model named 12 files it had never seen anything about.
  Whatever code elides a tool result from an assembled prompt must call `revoke_call` for that `call_id`, or the ledger
  will vouch for content the model never received.

Evidence rides `RenameProposalRowSnapshot` to the frontend. Preflight and apply don't read it (the tool boundary already
checked it), with one exception: apply reads the SOURCE to pick the operation log's initiator, because a batch carrying a
user-edited row is `Initiator::AgentEdited` rather than `Agent`.

**`EvidenceSource::UserEdited` is the review dialog's word, never the model's.** It means the user typed the name
themselves, so the row carries an empty detail and no coverage. Only [the revise path](#revising-one-row) sets it; a plan
that sends it is refused with `EvidenceProblem::SourceReservedForUser`, because "You typed this name" beside an invented
name is the misattribution this whole module exists to stop. The plan schema's enum lists the five model-usable sources,
so a compliant model never trips that refusal.

## Revising one row

The review can replace one staged row's destination name with the user's own (`revise.rs`, reached over
`revise_bulk_rename_row`). It is deliberately NOT a re-staged plan: re-staging would re-run two gates that must not fire
for an edit. The whole-plan evidence rule refuses all 50 rows when one `call_id` was revoked, so fixing row seven could
destroy the review; and the pane's effective scope has moved on by review time, so every row would refuse. Revise
consults no ledger and no pane state.

What it does instead:

- **Validates the name server-side.** This is the first destination name to cross IPC, so it passes the same
  `validate_destination_name` the model's names pass. Apply still resolves every name from the stored row by opaque row
  id, so a client-supplied name is never trusted.
- **Replaces the evidence rather than keeping it** (invariant 10): `UserEdited`, empty detail, no coverage. The model's
  quote described the model's name.
- **Invalidates the accepted preflight.** This is the data-safety half. Apply skips its own re-check when the allowed row
  ids match the acceptance, and duplicate-destination, cycle, case-only, and target-exists detection all live in
  preflight — so edit → preflight → edit again → apply would put a name on disk that none of those checks ever saw.
  Two independent guards: `revise_row` clears the acceptance, AND `AcceptedPreflight` records
  `allowed_destination_names`, so a lookup whose names have moved on refuses even if some future path forgets to clear
  it. A refusal is not a failure: apply falls back to running a fresh authoritative preflight.

The names are stored, not hashed, on purpose: an exact comparison has no collision window, and the list is 200 short
strings that never leave the process.

### Coverage: how thin the match is

Evidence validation proves the model READ something; it can never prove the name is right. A genuine, verbatim quote can
still support a badly wrong name (a payment confirmation named `klarna-invoice`), and the only thing that catches that is
a human looking at the file. So an accepted `imageText` claim also carries an `EvidenceCoverage`, and the review row
renders the quote inside its surrounding line plus "matched 20 of 3,140 characters": a sliver of a page of OCR has to
LOOK like one.

- **Derived here, from the delivery the check just matched against.** `ImageFactsLedger::check` returns it, so a row can
  only have coverage after the ledger already accepted the row. It is Serialize-only and `RenameEvidence` is
  `deny_unknown_fields`, so a plan that tries to send its own coverage is refused rather than believed. Coverage is
  never a second way to pass validation, and never a delivery in its own right.
- **Counted in characters of the DELIVERED text** (`image_facts` caps that at 2,000), because that's what the model was
  actually handed. `matched_chars` can exceed the quote's own length: folding collapses whitespace runs, so one quoted
  space may cover a line break plus indentation.
- **The excerpt shows the delivered spelling**, not the folded form the matcher compares. `normalize_with_origins` folds
  and records the source character index per folded character, so a match found in lowercased, whitespace-collapsed text
  is reported back as a span of the text the user is looking at. Both the matcher and the display share that one folding
  implementation on purpose: a second folding path could refuse a correct quote.
- **The window is capped** at `CONTEXT_CHARS` each side and never crosses a line break, with `trimmed_before` /
  `trimmed_after` telling the UI to show the cut.

The frontend classifies a coverage figure as thin or solid for display (`lib/ask-cmdr/rename-evidence-coverage.ts`); the
backend supplies only the honest counts.

## Where a staged proposal lives

One group on the durable proposal spine, in `main.db`. **The spine's mechanism — the three levels, `GroupIntent`, the
claim transaction, the acceptance record, the statuses — is documented once, in `agent/store/proposals/DETAILS.md`.**
Only what is rename-specific lives here.

Rename is the spine's documented exception: `GroupIntent::Rename` binds one shared PARENT and each op carries the name
its source becomes, because `start_bulk_rename` refuses a row whose source and destination parents differ. So the plan
boundary refuses a plan spanning two folders, and rows take the group's single `source_volume_id` rather than each
carrying their own claim about which volume they're on.

The opaque ids the frontend holds are the group id and the op ids, as strings. Nothing above the store parses them: an
id that isn't ours is simply unknown, and `load` answers `None` for it exactly as it does for a group that already left
`pending`. Every later step (preflight, apply) resolves paths and names from the stored rows by id, so a client-supplied
value is never trusted, and the source fingerprints never leave the process.

### Evidence rides a sidecar table, and a group without it is unreviewable

`proposal_rename_evidence` (migration v5) holds one row per rename op: the `EvidenceSource` token, the model's `detail`,
and the eight columns of an accepted `imageText` match's coverage. It is a sidecar rather than columns on `proposal_ops`
because evidence is a rename-producer concern and the spine stays verb-agnostic; the coverage is columns rather than a
JSON blob because `EvidenceCoverage` is deliberately Serialize-only (a plan may never send one) and a column list makes
adding a field a compile error rather than a silently-dropped value.

**Decision: staging is two commits, and a load with missing evidence refuses the whole proposal.** The spine owns group
creation, so evidence can only be written once its ops have ids. A crash in between leaves rename ops with nothing
saying where their names came from — and a review row whose evidence is missing is exactly the thing this module exists
to prevent, so `load` answers `None` rather than showing a name with invented backing.

### The accepted preflight is deliberately NOT durable

A successful preflight records two things, and only one of them survives a restart:

- **The values, in `main.db`**: the spine's server-owned acceptance record, whose digest covers every live op's id,
  source path, and destination NAME. This is what makes an edited name refuse at claim time.
- **The row ids and fingerprints, in memory** (`AcceptedRenamePreflights`): the exact subset the user allowed, and the
  server-only `RenameSourceFingerprint` per row that apply rechecks the source against before renaming it.

The proposal has no expiry, so a user can answer it two weeks later. An APPROVAL can't work that way: a fingerprint
describes a file as it was at review time, and an app that died mid-review knows nothing about the disk it came back to.
Dropping the fingerprints on restart means apply falls back to a fresh authoritative preflight, which is the honest
answer. A refusal is not a failure here.

Closing a review REJECTS its group rather than deleting it, so what the user was asked and what they answered stay in
the decision record. Proposing, approving, and rejecting all go through `agent/suggested_ops/`, which is where the
acceptance-rate metric is reported from.

Proposal validation reads the `PaneStateStore` cache and index registration only. It does not call live filesystem APIs:
a dead mount must not hang an agent turn, and symlinks remain links rather than targets.

Preflight owns row warnings as well as blockers. It compares the final filename extension case-insensitively and marks
extension additions, removals, and changes without blocking them. A renamed dotfile still has no extension; a trailing
dot is an empty extension and therefore differs from no extension. The same warning list carries dependency-cycle
metadata. Preflight peels acyclic dependencies from free destinations and marks only rows left in closed multi-file
cycles, so the frontend renders backend findings instead of re-deriving filename or graph semantics.

## The name-quality eval (`name_quality_eval.rs`)

Every guardrail here answers "did the model read something?". This module answers the question
none of them can: **the model read the file, quoted it verbatim, and named it wrong anyway** — a
Klarna payment confirmation named `klarna-invoice`, backed by a real quote. That passes every
check we have, and should: refusing it would need us to understand the image.

So the eval measures **the review surface**, not the model's taste. Each fixture (a screenshot's
delivered OCR plus the evidence a model claimed for it) runs through the shipped ledger and
`check`, and is scored on whether the row that reaches the dialog carries what a human needs to
disagree: the matched text AS DELIVERED, a locatable position, and a real delivered-length to
weigh it against. It asserts nothing about name quality — that is what the human is for.

Two tiers, following `importance::evals`: hard constraints as ordinary tests, plus one scalar
(the share of accepted rows that are judgeable) against a **fixed floor**, never a self-updating
ratchet.

Load-bearing details a future reader should not "simplify":

- **Surrounding context is not required for a row to be judgeable.** A quote can be an entire
  line of its own ("Payment confirmation" is, in the incident text), so demanding context would
  fail the exact case this eval exists for.
- **An `imageText` claim accepted with NO coverage scores as not judgeable.** That is the M1
  regression this catches: suppress coverage and four of these tests fail while every unit test
  in `evidence/tests.rs` still passes. Verified by mutation.
- **A fair name and an unfair one must score identically.** If they ever diverge, something
  started judging name quality, which is the human's job and a thing we would get wrong.
- Offline and deterministic. No provider, no network, no clock.
