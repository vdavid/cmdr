# Agent harness: let the human verify, keep the agent grounded

Two problems, in priority order. **The human can't check the agent's work**: the rename review shows text against text,
so a plausible wrong name gets approved, which is exactly how 12 real files got fabricated names. **The agent loses its
grounding on a job that doesn't fit one prompt**: a 500-file rename, a long thread, a folder whose facts outgrow the
window.

The engine-side fix for the original incident already shipped (`agent/chat/CLAUDE.md`,
`agent/tools/propose/DETAILS.md`): the current turn's results can never be dropped, results page themselves, budgets are
per-model, a drop is logged and announced, and a rename claiming file contents must quote facts the model provably
received in that thread. This plan is about everything that fix didn't touch.

**Read first**: `docs/design-principles.md`, `docs/style-guide.md`, `apps/desktop/src-tauri/src/agent/CLAUDE.md`,
`agent/chat/CLAUDE.md` + `DETAILS.md`, `agent/tools/CLAUDE.md`, `agent/tools/propose/DETAILS.md`,
`apps/desktop/src/lib/ask-cmdr/CLAUDE.md`.

**Refer to code by seam, not by path.** A concurrent effort (`.claude/worktrees/file-splits-2`) is splitting two files
this plan touches: `agent/tools/propose/rename.rs` is already `propose/rename/{mod,plan,store,preflight,tests}.rs`
there, and `agent/chat/runtime.rs` is becoming `runtime/{mod,turn,dispatch,events,cost}.rs`. **Rebase onto that work
before starting**, and find "the turn driver", "the dispatch seam", "the proposal store", "the plan schema" by name
(`codegraph_search`), never by the pre-split filename.

## The failure this is really about

The shipped guardrails stop a name with _no_ content behind it. They do nothing about a name with the _wrong reading_ of
real content: a Klarna payment confirmation that gets named `klarna-invoice`, backed by a perfectly genuine quote. The
model had the facts, quoted them verbatim, and still got it wrong.

The only thing that catches that is a human looking at the picture. Today the review dialog shows
`old name → new name → a quote`, and **you cannot see the file**. So Phase A comes first, and it's worth more than the
entire engine half of this plan. That's principle 6 (humans to humans) done properly: the human checks with their eyes,
not by auditing a quote chain.

## Measured ground truth

Estimated tokens (`chars/4`, the one ruler), re-measured 2026-07-30 against the shipped assets:

- **Fixed overhead, every call**: **3,124** (system prompt 740 + 12 tool declarations 2,384)
- **`image_facts`, per file at 900 chars of OCR**: 269
- **Plan row, per file** (path + name + evidence): 59
- **Pane listing, per file**: 21
- **A 100-file rename turn, all in**: **39,699**

**The whole breakdown is pinned, each figure within a tenth**, by `agent/chat/context/cost_tests.rs`: the fixed overhead
and its two halves, the three per-file costs, the total, and that the parts account for over 90% of the turn (the rest is
the paths the calls name, the envelope, the user's sentence, and JSON scaffolding). The numbers live in one constants
block there, and a failure says to update the test and this section together. The per-file figure assumes 900 chars of
OCR against a 2,000-char cap, so a text-dense corpus costs up to ~2.2× more.

Two consequences that change what milestones must do:

- **A 4,096-token local window can't run Ask Cmdr at all.** `prompt_budget_for_local_context(4096)` is 2,457, below the
  ~3,100 fixed overhead. That's today's default `ai.localContextSize`, so a local-model user on defaults gets no working
  turn at all. **M6 owns fixing it** (raise the default, or refuse honestly and name the setting).
- **The E2E fake path is in exactly that state.** The scripted fake resolves as `ProviderTag::Local` / `"fake"`, so
  every fake turn is already over budget today. **M7 owns settling it** (open question 5), or the suite blesses a
  pathological state.

## Design intentions

1. **A human verifies with their eyes.** Any surface where the agent's output becomes real data must show the user the
   thing itself, not a description of it.
2. **Reversible beats careful.** A safety net after the fact is worth more than another gate before it, because the user
   only discovers a wrong name once they see the result.
3. **Context is a cache; the durable stores are the truth.** `main.db` keeps every tool result, the oplog records every
   rename, the folder shows the current names. Anything needed twice is re-derivable, not preserved in the window.
4. **Honest degradation over silent degradation.** Every cut says it was cut: in the result, the log, and the UI.
5. **A guardrail must not gain an inconsistent twin.** New paths to guarded data route through the seam the guardrail
   watches.
6. **The model states intent as DATA, not prose we later re-read.** A pattern, a batch size, a budget: typed fields the
   app owns and echoes back.
7. **Per-turn facts ride the envelope, not the prompt.** The prefix stays byte-identical for prompt caching.
8. **Don't spend the user's window on our own bookkeeping.** Three examples beat fifty rows; a digest beats a payload.

## Non-goals

- **No rehydrate/un-elide tool.** Every agent-visible tool is an idempotent local read, so re-calling _is_ the
  rehydrate, and a second path to guarded data needs its own ledger wiring (intention 5). Revisit only for a tool whose
  results are expensive or unreproducible; the shape then is a result handle plus a slice call.
- **No new egress, no consent bump.** If a milestone seems to need `CONSENT_COPY_VERSION` bumped, stop and escalate.
- **No change to prefix stability.** M4 rewrites the prompt's text once, statically.
- **No real tokenizer.** `chars/4` stays; every number the UI shows is labelled an estimate.
- **The agent proposes; only the user approves.** M11 would move approval from per-item to per-rule for the tail of a
  large job. That's a policy change and it is David's call, not an implementation detail.

---

## Milestones and the order to do them in

**Milestone numbers are stable identifiers, not execution order.** They stay put through revisions so cross-references
survive.

**Order: M1 → M2 → M3 → M5 → M6 → M4 → M7 → M8 → M10**, then the deferred three (M11's decision, M12, M13).

Why that order, where it isn't obvious: M5 before M4 because M4 carries M5's re-fetch sentence; M6 before M4 because M4
points at M6's per-batch arithmetic; M6 before M7 because a gauge needs a settable budget. M10 sits last among the
actionable ones only because it's the least urgent, not because it belongs to long jobs: thematically it serves Phase A,
so pull it forward if there's appetite.

---

## Phase A: the human can verify (do this first)

### M1: See the file, and see how thin the evidence is

**Scope.** Two things, both in the review dialog:

1. **A preview.** The focused row shows a thumbnail; a key (Space, matching the app's quick-look idiom) opens the full
   viewer.
2. **The quote in context.** Today the column shows a bare quote, so a four-character hit inside 3,000 characters of OCR
   looks exactly as strong as a decisive one. Send the match offset and the delivered text's length with the quote, and
   render it in its surrounding line plus a coverage hint ("matched 7 of 3,140 characters"). A thin match must _look_
   thin. **Also raise the `imageText` floor from 4 to 12 characters** (decision 6).

**Why.** This is the milestone that catches the failure class we actually hit. Everything else in this plan makes the
agent better behaved; this makes the human able to disagree with it.

**Shape.** The pieces exist: media-index thumbnails ship as `cmdr-media://` tokens rendered straight into an `<img>`
(`lib/search/ImageSearchResults.svelte` is the precedent, including its token-drop lifecycle), and
`file-viewer/open-viewer.ts` opens the real thing.

**Landmines.**

- Thumbnails depend on media-index enrichment. An unenriched or non-image file must degrade to a neutral placeholder,
  never a broken image or a blank cell, and the row must still be reviewable.
- Honour the token lifecycle: drop tokens when the dialog closes, or they leak.
- The dialog is already 1,040 px wide with four columns, so layout space is the scarce resource here. Decision 5 settles
  it: a narrow leading thumbnail column, quote-in-context inside the existing evidence column, no fifth column.
- Keyboard-first: preview follows the focused row, no mouse required, and the a11y test must cover it.

**Tests.** Component tests: the focused row drives the preview; an unenriched file shows the placeholder; the token is
dropped on close; a quote renders with its surrounding line and its coverage figure; a thin match and a decisive one
render differently. Rust: the match offset and delivered-text length reach the snapshot. a11y test in the existing
dialog pattern.

**DONE when** David can look at each screenshot while reviewing its proposed name, AND a four-character match inside
3,000 characters is visibly weaker than a decisive one.

### M2: Fix the name yourself, and see what wasn't read

**Scope.** Two additions to the dialog: (a) the proposed name becomes editable inline; (b) a visible per-row state for
"nothing was read inside this file, so the name comes from metadata".

**Why.** Today a row is allow-or-deny, so the user's only options are the model's name or the old one. That's the
pressure that produces "approved because it looked plausible". And M4's "keep a neutral name" instruction is worthless
if the user can't see which rows took that path.

**Shape: a server-side per-row `revise_row`, not a re-stage.** Routing an edit through the normal proposal path would
re-run two gates that must not fire here: the evidence check (one revoked `call_id` refuses all 50 rows under the
shipped whole-plan rule, so editing row 7 could destroy the review) and pane-scope validation (the user scrolled, so
every row refuses). So revise is its own narrow operation on the staged row.

**Landmines (data safety).**

- **An edit must invalidate the accepted preflight.** It compares `allowed_row_ids` only, and apply skips the re-check
  when they match, so edit → preflight → edit again → apply would apply a name that duplicate-destination, cycle, and
  case-only detection never saw. Hash the destination-name set into the accepted preflight; any revise clears it.
- **Names cross IPC for the first time**, so `validate_destination_name` runs at revise time on the server, and apply
  never trusts a client-supplied name.
- **Replace the evidence, don't keep it.** Evidence rides the row snapshot and apply never reads it, so an edited row
  would otherwise keep displaying the model's quote beside the user's name. Revise swaps it for a `userEdited` marker,
  and the oplog's `Initiator::Agent` becomes a lie for that row: record the mixed provenance honestly (this is the one
  remaining item from the earlier bulk-rename hardening handoff, so the two efforts meet here).
- The "kept the name" state must keep saying nothing inside the file was read. That's what the column is for.

**Tests.** Rust, test-first: revise validates the name server-side; revise clears the accepted preflight (red-guarded,
it's the data-safety case); a revised row needs no evidence and reports `userEdited`; the whole-plan evidence rule is
NOT re-run by a revise. Frontend: edit, blur, apply round-trip; the "kept the name" state renders.

**DONE when** a wrong name can be corrected in place instead of abandoned.

### M3: Undo after it lands

**Scope.** Surface undo for a completed rename batch: a "Renamed 23 files. Undo" affordance after apply, and a
job-scoped undo across a multi-batch run.

**Why.** Intention 2. Every other safety net in this system fires before the user can possibly know whether the names
are right; the one that fires after is worth more than all of them.

**Shape.** The pieces exist: renames journal, the oplog reports `rollbackState: rollbackable` (confirmed on real ops),
the `operations_rollback` IPC exists, and the operation log dialog _displays_ rollback state. What's missing is a
user-reachable undo **trigger** next to the rename result.

**PREREQUISITE, and it's a real defect: undo currently verifies identity by size alone.** `record_bulk_rename_outcomes`
journals the fingerprint's `size` and passes `None` for mtime, so `verify_snapshot` compares size and nothing else.
Replace the renamed file with a same-size different file and undo renames _that_ back. The claim "the journal's
fingerprints are the authority" is false until this is fixed.

- The data is already held: `BulkRenameFingerprint::Local { modified_nanos }` and `Remote { modified }`.
- **The trap is precision.** The journal takes `Option<i64>` and `verify_snapshot` compares it against
  `FileEntry::modified_at`. Convert to whatever unit that field actually reports, or every undo reports drift and
  refuses, which is a worse failure than the one being fixed (it silently disables undo).
- Tests, red first: a matching target verifies; a same-size-different-mtime target is refused as drift; a remote row
  with no mtime available stays `Unverifiable` rather than becoming a false match.

**Landmines.**

- **Multi-batch undo must run newest-first.** If batch 3 reused a name batch 1 freed, oldest-first hits "restore target
  occupied" and skips silently. State the order and pin it.
- Partial success must be loud: N restored, M skipped and why, per the existing rollback vocabulary.
- Undo of a row the user later hand-renamed must refuse (drift), not force.

**Tests.** Reuse the existing rollback test bed; add the ordering case, the drift case, and the multi-batch case.
Test-first throughout (this is a data-safety path).

**DONE when** a bad batch is one action away from undone, a drifted target is refused rather than clobbered, and a
multi-batch undo restores in the reverse order it applied.

---

## Phase B: the model behaves

### M4: A prompt that instructs instead of forbidding

**Scope.** (1) Name the fallback action: keep the existing name, or `<date> <existing-name>`, and list the unseen files.
(2) Point at the per-turn batch size named in the envelope rather than hardcoding one. (3) Require the quote to be
verbatim: a paraphrase is refused, refusal is per-plan. (4) Restructure into labelled sections (identity, coverage
honesty, renaming, evidence, style). (5) Fold in M5's re-fetch sentence.

**Why.** A prohibition leaves the next token to chance; a named action gets followed. Items 2 and 3 turn classes of
backend refusal into non-events.

**Landmines.** Static text only (prefix stability). Existing prompt-asset tests stay green; a moved phrase moves its
test and the commit says so. **Verify the completion-token ceiling separately**: a 100-row plan call is ~5,400 tokens of
_output_, a different limit from the prompt budget, so check what the slot allows before writing a number. Don't add a
fourth honesty exhortation.

**Tests.** Prompt-asset tests, one per item 1–3 plus the re-fetch rule (authored text, so written alongside).

**DONE when** the prompt names an action for every failure it forbids.

### M5: A dropped result says what it was, and how to get it back

**Scope.** Replace the elision tombstone with tool name, a digest of the call's arguments, a digest of the dropped
result, and a re-fetch hint. Add the breakdown assertions to the measurement test.

```json
{
  "elided_tool_result": true,
  "tool": "image_facts",
  "approx_tokens": 5406,
  "call": "12 paths under /Users/me/Downloads/shots",
  "held": "12 indexed files, OCR text for 11",
  "refetch": "call image_facts again for the paths you still need"
}
```

**Landmines.**

- **A digest must never be citable as evidence.** Revocation already fires for elided results so this should hold for
  free; pin it anyway, because it's the one way this milestone reintroduces the original bug.
- Digests are derived structurally (array lengths, common prefix, counts), never by a model call; ~120 chars each, whole
  stub under ~80 tokens.
- **Per-tool shape knowledge inside the pure core is new coupling.** Keep it shape-agnostic (lengths and key names) or
  pass a per-tool `digest()` in as a value. Prefer the latter the moment it grows a match arm per tool.
- No OCR text in the digest: no re-fetch value, and it reads as content.
- `context/tests.rs` is ~690 lines; put new tests in a sibling module rather than crossing the 800-line warn.

**Tests.** Pure, test-first: all four fields present and within budget; a 2,000-char OCR field never appears; **a plan
citing digest-only text is refused**; breakdown assertions.

**DONE when** a model handed a stub can reconstruct the call, and a plan citing a digest is refused.

---

## Phase C: the user is informed and in control

### M6: The chat's memory size is the user's to set

**Scope.** A new setting: "Automatic (recommended)" or an explicit size. Backend resolution honours and clamps the
override. Fix the stale-family class in the budget table (`qwen`, `deepseek`, `grok`, `mistral` are all stale today, not
just one).

**Shape: a preset list plus "Automatic (recommended)"** (16,000 / 32,000 / 60,000 / 128,000 / 200,000), following the
`ai.localContextSize` precedent. **Decided, not open**: presets make the bounds unmisstateable, so there is no
below-minimum case to clamp and no numeric validation copy. An over-window warning is still needed, because a user can
pick 200,000 for a model whose window is 32,000. Resolution stays pure in the budget module with the override passed
from the command layer; the read-fresh-per-send precedent means no settings-applier case.

**Also in scope: the shipped 4,096 default breaks the agent for local users.** `prompt_budget_for_local_context(4096)`
is 2,457 against ~3,100 of fixed overhead, so a user on defaults cannot get a single working turn. Nobody else owns
this. Either raise the shipped default to a window that works, or refuse the turn with an honest message naming the
setting to change. Silently assembling an over-budget prompt is the one option this plan rules out.

**Landmines.**

- **Minimum 16,000** (decision 2): ~3,100 of overhead plus one paged result leaves nothing below that.
- **Drop "provider-reported window"**: no source for it exists in this codebase. The ceiling is the local server's
  configured window, else the family table, else the default. Label the source so a stale table is visible.
- Above the real window warns, never blocks: our table will be wrong sometimes and the user may be right. Name the
  consequence (the model may refuse the message).
- **The tool-result ceiling stays derived from the DEFAULT budget**, not the effective one: a tool handler doesn't know
  the model and may serve an external MCP client, so a 200k budget must not let one result claim 100k. Document the
  asymmetry where the constant lives.
- A change must not affect an in-flight turn (the model-change path sets the precedent).
- The envelope's per-batch file hint (M4 item 2) derives from this budget: `(budget − overhead) / per-file cost`. Keep
  that arithmetic next to the budget, not in the prompt or the UI.

**Tests.** Test-first for resolution: auto follows the table; an override is honoured; a preset above the known window
warns and is still used; an unknown model gets the default; the per-batch hint derives correctly at 16,000 / 32,000 /
60,000; a stored below-floor `ai.localContextSize` is clamped up to the floor; a local server whose own configured
window is under the floor is refused honestly rather than assembled.

**DONE when** every resolution case is pinned, a local user on defaults gets a working turn, and David has reviewed the
copy.

### M7: Say when the chat is filling up

**Scope.** A usage indicator in the rail footer: a fill bar with a percentage, and a plain-language tooltip. Plus a
one-line thread notice when older material is set aside.

**The three states**, so a component test can name them: **calm** (under 80% of budget), **filling** (80% or more,
nothing set aside yet), **set aside** (something from history was dropped this turn; the tooltip carries the count).
Over 100% is not a fourth state: it renders as "set aside" with a full bar.

**Persistence, decided**: store the last turn's usage with the thread, so a reopened thread shows the last known figure
rather than an empty gauge. An event-only number would blank on reopen, which reads as "no usage" instead of "not
measured yet".

**Note on scope, for David.** You asked for the bar, the percentage, and the "N of M tokens" tooltip, so that's what
this specifies. The round-2 reviewer argued for cutting the gauge and keeping only the trim notice, on the grounds that
a percentage answers a question no file-manager user asked. Worth weighing: the notice alone would delete the E2E-fake
landmine below and two open questions. Your call.

**Landmines.**

- **The E2E fake path is over budget on today's shipped default** (2,457 vs ~3,100). Decision 4 settles it: give the
  fake path its own realistic budget rather than raising the harness's `ai.localContextSize`, so the harness keeps
  mirroring a real user. Otherwise every E2E run pins the gauge and the assertions bless a pathological state.
- **The stream event type is hand-mirrored in TypeScript** (Channel enums fall outside specta): a new event means a hand
  edit in both languages, kept in sync.
- **Every new string needs 10-locale parity** (error-level check) plus a `keys.gen.ts` regeneration.
- Show the assembled prompt, not the thread's cumulative cost; the footer already shows the latter, so label both.
- Full-flow, not "over budget" as a user-visible state: history was set aside, the turn worked.

**Copy** (drafts for David; plainer than the engine's vocabulary, and no "k" abbreviations per the style guide):

- Tooltip: "31,200 of 60,000 tokens used (estimated)".
- Trim notice: "Older parts of this chat were set aside to make room. Nothing from this answer was left out."
- Setting label: "Chat memory size". Description: "How much of the conversation Cmdr sends to the model. Automatic
  follows your model's known limit."
- Over-window warning: "Your model may refuse a message this long. Cmdr keeps the value you set."

**Tests.** Formatter unit tests (the 80% threshold, rounding, thousands separators); a component test per named state
above; an a11y test; a runtime test that usage fires once per turn with the assembly's numbers; a test that a reopened
thread renders the persisted figure.

**DONE when** a turn that sets material aside says so, the E2E path shows a sane gauge, and David has reviewed the copy.

---

## Phase D: long jobs (plus the eval that outlives them)

### M8: A batch job grounds itself in reality, not its own transcript

**Scope, cut down to what earns its keep.** (1) Prompt: before a follow-up batch, call `list_pane_files` and match the
convention the already-renamed files show. (2) Carry the user's denials into the next batch, so a rejected style isn't
proposed again.

**Why.** The transcript records what the model _proposed_; the folder records what actually happened, including the
user's denials and hand edits. It's free to read, always current, and can't drift.

**Cut from the earlier draft, deliberately:**

- **The "recent renames here" envelope line, and its new folder-scoped oplog query.** `list_pane_files` already shows
  the renamed files for free, in a call the model is making anyway, so the envelope version bought nothing but a
  permanent per-turn cost and a backend workstream. Worse, feeding content-shaped `old → new` examples into every turn
  invites the model to extend a convention to files it never read, under `filename` evidence, which always passes: names
  that _look_ content-derived while the column honestly says nothing inside was read. Invariant 6 holds, but only just,
  and the temptation isn't worth it.
- **Don't tell the model to use `operations_list` either.** Its rows carry no paths and there's no folder filter, so
  old→new names would need an `operations_get` per operation plus dir-prefix reconstruction: several calls against a
  `MAX_TOOL_TURNS` of 8, to learn what one `list_pane_files` call shows.

**Landmines.** The denial feedback must carry _what_ was rejected without re-proposing it: pass the denied names, not a
model-authored summary of why, or the next batch inherits a rationalization.

**Tests.** Prompt-asset test for the re-derive rule; a runtime test that denials reach the next turn.

**DONE when** batch two states the convention from tool output alone, and a style the user denied doesn't come back.

### M9: CUT (was: the naming pattern is data the app owns)

**Dropped on the round-3 argument, which I accept.** A model-authored pattern shown above the table would be a claim the
backend can never validate (the moment it tries to verify that a name "matches" a free-text pattern, it owns a pattern
language it can't win), displayed exactly where it raises the user's confidence in a batch they're about to approve.
Unverifiable reassurance at the approval boundary is anti-safety, which is the opposite of this plan's purpose.

**What survives the idea**: M11's per-rule flow, if David wants it, needs a rule the _app_ interprets rather than prose
the model authors. That's a different and much harder thing than a display field, and it belongs to that decision.

### M10: Keep it honest over time (serves Phase A's purpose; pull it forward if you can)

**Scope.** An offline eval over a fixture corpus of screenshots with known content, scripted through the fake path.

**The case that matters is NOT "invented quote is refused"** — unit tests already cover that, so asserting it here is a
tautology that would pass forever while the real gap widens. The eval's job is the case no guardrail catches: **a
genuine, verbatim quote supporting a materially wrong name** (the payment confirmation named `klarna-invoice`). Assert
that such a row is _flagged for the human_ (thin-coverage hint from M1, provenance visible), not that it's refused,
because refusing it is impossible without understanding the image.

**Why.** Every other milestone is a one-time fix. This is the only one that detects the next regression, and it runs
without a provider. There's existing eval infrastructure for importance ranking to model it on.

**Landmines.** Offline and deterministic (the scripted fake, never a live model). It measures our guardrails and our
review surface, not the model's taste, so assert on provenance, coverage, and refusals, never on name quality.

**DONE when** a fixture whose quote is real but whose name is wrong comes back flagged rather than silently plausible.

### M11 (decision needed, then a write-engine change): trial batch, then background remainder

**The shape.** Instead of ten review dialogs for 500 files: the model applies one rule to a trial batch of five to 10
the user reviews carefully, and the remainder runs as **one background operation** with progress, cancel, and undo (what
the design principles ask for: background, quantified progress, cancelable).

**Why it's a decision, not a task.** It moves approval from per-item to per-rule for the tail. That's a real shift in
the "only the user approves" line, safe only because M3's undo exists, and it's David's call.

**And it is NOT a spike: it's a write-engine change.** Round 3 established this concretely, so nobody should start it
believing otherwise:

- **A background remainder has no proposal, therefore no safety.** Every guardrail on this path lives in the reviewed
  proposal: the evidence check, pane-scoped source validation, the fingerprinted preflight, and the per-row fingerprint
  recheck at write time. A tail with no proposal has no `expected_fingerprint` to compare, no ledger check, and no human
  look, while a content-shaped rule means the tail _must_ derive names from content. That is the original incident at
  490 files instead of 12.
- **`MAX_RENAMES` is 200**, `start_bulk_rename` refuses rows whose parent differs, and a proposal only covers the loaded
  pane window. So "the rest of the folder" is several folder-scoped operations, not one call.
- **Progress is emitted after the whole run finishes**, so "progress and cancel" doesn't exist yet for bulk rename.
- **The 15-minute proposal TTL** breaks a paced job: step away mid-review and batch six is gone.

**So if David says yes**, the honest version is: per-rule approval still enumerates and preflights _every_ row up front,
in ≤200-row folder-scoped operations, with streaming progress added to the bulk-rename driver. That's a substantial
piece of work, and it's the only version that doesn't open a second unguarded path to the user's files.

**If David says yes, M12 evaporates**: the tail stops being a series of plan calls in a transcript, so M12 is only worth
building if this shape is declined.

### M12 (deferred, and gated by M11 being DECLINED): a consumed plan stops costing what it cost

**Scope.** Extend compaction to tool _call_ arguments for rename plans no longer live: N rows collapse to
`{ renames: N, examples: [3 rows] }`. The plan under review is never touched.

**Honest arithmetic.** Ten batches of 50 at 60k: ~27k of plan calls + ~13k current-turn facts + ~3,100 overhead + ~2,100
listing + ~1k per batch of unelidable prose ≈ **55k, which still fits**. The wall is nearer **500–600 files** at 60k,
and at the 16k default it's **batch two**. So the DONE test pins the 16k case (a 60k test would pass either way and
prove nothing), plus a 500-file case at 60k.

**Shape.** The pure core can't query the store, so: the store records the **originating `call_id`** with each staged
proposal, exposes `live_call_ids()`, and the runtime passes that set into assembly through a dispatch-seam method
defaulting to empty (the shape evidence revocation already uses). Assembly classifies on the `ToolCall`'s own `call_id`,
re-reading the set per loop iteration so a plan staged mid-turn counts as live.

**Landmines.**

- **"Liveness, not age" is the wrong rationale**: proposals carry a 15-minute TTL, so the live set _is_ age-gated. Say
  "the store's live set, which expires with the proposal", or the next reader deletes the TTL check.
- **Collapsed calls need their own `ElisionFacts` counters.** Reusing `elided_call_ids` would revoke evidence
  spuriously; counting them in `elided_results` would fire a false trim notice.
- Narrows the "assistant messages are never modified" invariant: state the narrowed form out loud (prose never; a call
  only when it's a non-live rename plan).
- Row ids are opaque and single-use, so examples are names, never ids.

### M13 (deferred, needs measurement): prose compaction

Fold assistant prose older than N turns into a running summary. Deferred because prose is ~1k per batch and may not be
the binding constraint once M12 or M11 lands. **Trigger**: a thread that hits the 40-message soft cap and still reports
over budget with prose dominant. Don't start on principle.

---

## Invariants register

No milestone may break these. **Guarded by a red-guarded test today: 1–9 and 11.** **Established by this plan: 10 (M2's
revise path) and 12 (M1's coverage hint plus M10's eval).** Don't hunt for tests that don't exist yet; write them as
those milestones land.

1. The current turn's tool results are never elided.
2. The pure context core stays pure: no clock, no I/O, no app state. New inputs arrive as values.
3. The prefix is byte-identical across a thread's calls.
4. The envelope rides the latest user turn only, snapshot-at-send.
5. Assistant prose is never modified. (M12 narrows this to prose specifically and says so.)
6. A content claim needs a delivery the ledger recorded, in that thread. Digests, envelopes, patterns, and summaries are
   never deliveries.
7. The agent proposes; only the user approves. (M11 would amend this for the tail of a large job, by David's decision.)
8. No new egress category; no consent bump.
9. Every cut, cap, or trim is visible in the result, the log, and (where the user could be misled) the UI.
10. A user-edited name needs no evidence, never claims any, and never inherits the model's. It also invalidates the
    accepted preflight, so no name reaches the filesystem unchecked.
11. Every row that reaches the filesystem was preflighted with a fingerprint the writer rechecks. No path around the
    proposal (M11's tail included) may skip that.
12. Evidence validation proves the model READ something, never that the name is right. The review surface must therefore
    show how thin a match is, and the eval (M10) targets exactly the genuine-quote-wrong-name case.

**Two shipped weaknesses this plan found, for the record.** One is fixed: a tag claim used to pass if the model's prose
merely _contained_ a delivered tag, so invented text rode a near-universal tag like `document` (fixed 2026-07-29). One
is open and is M3's prerequisite: bulk-rename undo verifies identity by size alone, because the journal records no
mtime.

## Parallelization

**M1 and M2 are one coherent UI effort** and should run sequentially, by one agent, on the review dialog. **M3 is
backend** (the journal fingerprint defect, the oplog trigger) and can run in parallel with them.

**M4 is NOT independent**, despite what an earlier draft said: it carries M5's re-fetch sentence and points at M6's
per-batch arithmetic, so it comes after both. M6 before M7 (a gauge needs a settable budget). M8 and M10 are independent
of everything else.

Not in a hurry: prefer sequential.

## Decisions (all questions closed; execution can proceed unblocked)

1. **The undo verification defect** stays where it is: M3's prerequisite, fixed in M3, in the order above. Not a
   separate pre-effort.
2. **The local window floor is 16,384 tokens** (David: "below that, it's unusable"). So M6's fix to the shipped 4,096
   default is: raise the default to `16384`, drop the `2048` / `4096` / `8192` options from `ai.localContextSize`, and
   **clamp a stored below-floor value up on read** so an existing user who picked 4,096 is migrated rather than left
   broken. No honest-refusal path is needed once the floor is unreachable, but keep the refusal for a local server whose
   own configured window is smaller than the floor.
3. **M7 keeps the gauge** (bar, percentage, tooltip) plus the trim notice, as originally asked. The round-2 argument to
   ship only the notice is noted and declined.
4. **The E2E fake path gets its own budget**, not a raised harness setting, so the harness keeps mirroring a real user.
5. **M1 layout, decided so nobody is blocked** (David reviews the result; it's a visual call he QAs himself):
   - A **narrow leading thumbnail column** (~44 px, fixed), because scanning 50 rows for the odd wrong one is the actual
     review task, and a detail pane only shows the row you already suspect.
   - **Quote-in-context stays inside the existing "Why this name" column**, gaining the surrounding line plus the
     coverage hint underneath. No fifth column: the filenames are already tight at 90 vw.
   - **Space opens the full viewer** for the focused row.
   - If David wants the detail-pane shape instead, it's a layout swap over the same data; nothing below depends on which
     one ships.
6. **The `imageText` floor rises from 4 to 12 characters**, done as part of M1 (it's the same "a thin match must look
   thin" concern, and the coverage hint makes the remaining thin matches visible rather than refused).
7. **M11 stays undecided, so M11 / M12 / M13 are out of this execution pass.** M11 is a policy shift (approval moves
   from per-item to per-rule for a job's tail) and a write-engine change; M12 is gated on M11 being declined; M13 needs
   measurement that doesn't exist yet. **Executing now: M1 → M2 → M3 → M5 → M6 → M4 → M7 → M8 → M10.** M9 is cut.
8. ~~Split `rename.rs`~~ — obsolete; the concurrent file-splits effort already did it, and it has landed on `main`
   (`propose/rename/` and `chat/runtime/` are directories now).
9. ~~Budget setting shape~~ — presets plus "Automatic", per M6. With decision 2 the preset list starts at 16,000.
