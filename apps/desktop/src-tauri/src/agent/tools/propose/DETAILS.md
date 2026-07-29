# Rename proposal details

## Evidence: why a proposed name is believable

A model asked to name files by their contents will produce a confident, plausible name whether or not it ever saw the
contents. So the tool boundary, not the model's goodwill, decides whether a content-derived name may reach the user.

Each row's `evidence` is a required `{ source, detail }` pair. `EvidenceSource` splits into two classes:

- **Content claims** (`imageText`, `imageTags`): checked against `ImageFactsLedger`. The path needs a live delivery, and
  `detail` must be a real quote from the delivered text (normalized on both sides: NFD-lowercased, whitespace runs
  collapsed, surrounding quote characters stripped) or name one of the delivered tags. Every refusal is a typed
  `EvidenceProblem`, so nothing classifies on wording (`no-string-matching`).
- **No claim** (`filename`, `metadata`, `userInstruction`): always accepted, because there's nothing to verify. They
  still cross to the review dialog verbatim, where the UI labels them as reading nothing inside the file. That's the
  other half of the guardrail: a fabricated slug filed under `metadata` is visible as a name with no content behind it,
  rather than hidden. See `apps/desktop/src/lib/ask-cmdr/DETAILS.md` § The "Why this name" column.

`detail` is bounded at both ends (4 normalized characters minimum, 160 maximum): a one-character "quote" appears in any
text and proves nothing, and a review row can't honestly show a page of OCR output.

**Decision: a tag claim lists delivered tags and nothing else.** `check_tags` requires every comma- or
semicolon-separated part of `detail` to equal a delivered tag. The tempting direction (does the detail CONTAIN a
delivered tag?) is a hole: tags like `document`, `screenshot`, and `text` are near-universal in a screenshot corpus, so
160 characters of invented prose passes on one of them, and a fabricated name reads as tag-backed. `MIN_DETAIL_CHARS`
therefore applies only to `imageText`, where matching is by substring; membership needs no length floor and real tags
(`sky`) are short.

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

Evidence rides `RenameProposalRowSnapshot` to the frontend. Preflight and apply don't read it: the tool boundary already
checked it, and the store's rows are immutable afterwards.

## The proposal store

The store is feature-local because its opaque ids and immutable rows are the authority boundary for review and apply commands. Entries expire in memory and are deliberately not persisted in chat history. A successful preflight records both the exact allowed row-id set and server-only source fingerprints; Apply atomically consumes that pair, so a dialog cannot replay an already-started plan or substitute a different subset.

Proposal validation reads the `PaneStateStore` cache and index registration only. It does not call live filesystem APIs: a dead mount must not hang an agent turn, and symlinks remain links rather than targets.

Preflight owns row warnings as well as blockers. It compares the final filename extension case-insensitively and marks
extension additions, removals, and changes without blocking them. A renamed dotfile still has no extension; a trailing
dot is an empty extension and therefore differs from no extension. The same warning list carries dependency-cycle
metadata. Preflight peels acyclic dependencies from free destinations and marks only rows left in closed multi-file
cycles, so the frontend renders backend findings instead of re-deriving filename or graph semantics.
