# Agent chat details

Pull-tier docs for `agent/chat/`. Must-knows live in `CLAUDE.md`.

## Anatomy of one call (spec §5, as built)

`run_turn` assembles and sends, top to bottom:

1. **System** (`system` arg): `system_prompt::SYSTEM_PROMPT` + the user's `CMDR.md` under a
   header if present (§ Which `CMDR.md`, and how much of it). Stable, cached. Five labelled sections (identity, what you can do,
   coverage, renaming, evidence, style) so a rule can be found and edited without re-reading
   the block; every load-bearing rule is pinned by a prompt-asset test in `system_prompt.rs`.
   **Each rule that forbids something also names the action to take instead** — a prohibition
   alone leaves the next token to chance, which is how a batch of screenshots got 12
   fabricated names.
2. **Tool declarations** (`tools` arg): `agent::tools::agent_tool_declarations()`. Stable,
   cached.
3. **History**: every persisted turn, each user turn prefixed with its own local
   timestamp marker (`[Fri 2026-07-11 09:15]`). Assistant prose survives verbatim; tool
   results `ELIDE_TOOL_RESULTS_AFTER_TURNS` or more turns back collapse to a self-describing
   stub (§ The elision stub). Eliding prose is never done — that's the soft-cap's job. The
   current turn's results never elide either (see § Budget enforcement).
4. **The envelope** opens the LATEST user turn only, as a tagged block (§9 field set):
   `[Sat 2026-07-12 21:30 · focused: <path> · cursor: <name|—> · <n> selected · volumes: <name> (<freshness>[, <connectivity>]), … · rename batch: up to <n> files]`.
   The batch size is `budget::files_per_batch` for this turn's resolved budget; it rides here
   rather than in the prompt because it moves with the model and the user's setting, and the
   prompt points at it (§ Sizing a batch from the budget).
5. **The user's text**, following the envelope block in the same message.

An answer that needs no tools comes straight back (`Done`). One that needs tools runs the
loop within budget, each call surfaced through `AgentChatEvent`s.

### Why the split (prefix vs. envelope)

Prompt caching keys on a byte-identical leading span. The system + tools are that span, so
they must not vary. The envelope changes every send, so it lives on the latest user turn —
after the cached prefix — and, being snapshot-at-send, it stays byte-identical across the
respond calls of one turn's tool loop. `assemble_prompt` is a pure function of its inputs,
so "same inputs → same bytes" is structural, and the two invariants are each red-guarded
in `context/tests.rs`.

## The pure core (`context.rs`)

`assemble_prompt(prefix, transcript, envelope, offset, budget) -> AssembledPrompt`. No
clock, no I/O: the local UTC `offset`, the `ContextEnvelope`, `CMDR.md`, and the model's
`budget` are values passed in. The runtime captures them; the core only formats. Timestamps render through `offset` (a
single offset for the whole assembly — a DST boundary mid-thread is a hint-level
imprecision, acceptable for v1). Token sizes are a `chars / CHARS_PER_TOKEN_ESTIMATE`
heuristic, not a real tokenizer — enough to keep assembly in the budget band and to size
the elision stub's hint.

## The elision stub (what a dropped result says about itself)

A stub is six fields, and a model that meets one can reconstruct the call it lost:

```json
{
  "elided_tool_result": true,
  "tool": "image_facts",
  "approx_tokens": 5406,
  "call": "12 paths under /Users/me/Downloads/shots, volumeId: root",
  "held": "0 coverage, 12 facts (path, state, tags in 9, text in 11), status (2 chars)",
  "refetch": "call image_facts again for the paths you still need"
}
```

`context/digest.rs` derives the last three, structurally: array lengths, key names, per-field
filled-in counts, and the folder a call's paths share. No model call is involved, so a digest
costs nothing and can't hallucinate. The stub above is 71 estimated tokens in place of 5,406.

**Bounded by construction.** `STUB_TOKEN_BUDGET = 80`. `stub_for` serializes the fixed fields
first (marker, tool, size, re-fetch sentence), then splits what's left of `80 *
CHARS_PER_TOKEN_ESTIMATE` bytes evenly between the two digests, so no tool name or key name can
push a stub past its budget.

**Decision: shape-agnostic rules in the core, not a per-tool `digest()` passed in.** Both were
open (the core must stay pure, invariant 2, so per-tool knowledge may not live here). The
generic rules — lengths, key names, filled-in counts, common path prefix — reproduce what the
per-tool wording would have said for every shipped tool, so the seam would have had no second
implementation to justify it. **If one tool ever needs wording these rules can't produce, add a
`digest()` parameter and pass it in from the runtime; do NOT add a match arm per tool here.**
`digest.rs`'s module doc says the same, at the place someone would break it.

**A result's strings never survive, at any depth.** A string field reports its LENGTH (`text in
11`, `note (17 chars)`), never its text. Call ARGUMENTS are quoted (capped): the model wrote
them itself, and they are what makes a call re-issuable. Two reasons, the second load-bearing:
2,000 characters of OCR has no re-fetch value, and text lifted out of a result reads as content
the model was handed. A digest is a description of a delivery, never a delivery — `stub_tests.rs`
pins that a plan citing one is refused, with the same quote checking out before the drop.

## Budget enforcement (elide-only, floored, and reported)

`assemble_prompt` takes the resolved model's `budget` and tightens the elision threshold
turn by turn until the estimate fits, never touching prose.

**The floor: `MIN_ELISION_TURNS_BACK = 1`.** The loop stops there, and `build_messages`
enforces it independently (`turns_back >= threshold.max(MIN_ELISION_TURNS_BACK)`), so no
caller and no future threshold value can reach the turn in flight.

**Decision: an honest budget overrun beats a blinded model.** Why: the loop used to run the
threshold to 0, which elided the result that came back THIS turn. A user asked Ask Cmdr to
rename 23 screenshots by their content in two batches; the second batch's `image_facts`
result (32 KB, ~8.2k estimated tokens against an 8k budget) collapsed to
`{ elided_tool_result: true }` while the instruction "name these by their content" still
stood, so the model invented 12 filenames, which were approved and applied to real files.
The stored tool result was correct; only the assembled prompt had lost it. So budget
pressure now stops at history, and a turn whose own results don't fit goes over budget and
says so.

**Every cut is reported, never logged from here.** `AssembledPrompt::elision`
(`ElisionFacts`: `elided_results`, `elided_tokens`, `elided_call_ids`, `threshold`,
`estimated_tokens`, `budget`) crosses back as data, keeping the core pure. `runtime/turn.rs`'s
`announce_context_pressure` splits it in two: `budget_forced()` (history was dropped) warns
AND emits one `AgentChatEvent::ContextTrimmed` per turn for the rail;
`over_budget()` (nothing safe left to drop) warns only, because on a small local window it
would otherwise fire every turn and the soft-cap nudge already covers "this chat is long".
Summarize-on-overflow is still deferred (spec §3).

### A dropped result loses its standing as evidence

`propose_rename_plan` items carry typed evidence, and a content claim (`imageText` /
`imageTags`) is verified against what `image_facts` actually delivered (the `ImageFactsLedger`
under `agent/tools/propose/`). Delivery is recorded at DISPATCH, but only assembly knows what
the model ends up reading, so `run_turn` calls
`ToolDispatcher::revoke_evidence(&elision.elided_call_ids)` right after each assembly, before
the `respond` goes out.

**Why the dispatch seam and not a direct ledger call:** `context.rs` must stay pure, and
`run_turn` holds no `AppHandle` — the dispatcher is the one seam with app state, and it is
already the half that recorded delivery. Deliver and revoke through one seam, and the two
halves can't drift. The trait's default is a no-op so test doubles ignore it;
`AppHandleDispatcher::revoke_evidence` is the single production site. After
`MIN_ELISION_TURNS_BACK` this fires only for genuinely aged-out results, which is defence in
depth, not the main line.

## Constants table (initial values, tune with use)

In `context.rs` (turn shape and compaction):

- `MAX_TOOL_TURNS = 8` — per message; the loop stops before the 9th tool respond fires.
- `MAX_WALL_TIME = 120s` — per message wall-clock ceiling across the whole loop; leaves room for reasoning-heavy
  OpenAI-compatible models while the tool-turn cap still prevents a runaway loop.
- `ELIDE_TOOL_RESULTS_AFTER_TURNS = 3` — tool results this many turns back (or more) elide.
- `MIN_ELISION_TURNS_BACK = 1` — the floor above; the current turn's results never elide.
- `THREAD_SOFT_CAP_MESSAGES = 40` — past this the UI nudges "start a fresh one?".

In `budget.rs` (how many tokens a prompt and a tool result may spend):

- `CHARS_PER_TOKEN_ESTIMATE = 4` — the ONE size-estimate divisor (elision, the stub hint, every tool's self-cap).
- `DEFAULT_PROMPT_TOKEN_BUDGET = 16_000` — an unrecognized model's budget. Conservative because guessing high is a hard
  provider rejection mid-turn; still double the 8k that overflowed on a 12-file batch.
- `PROMPT_BUDGET_60K = 60_000` — the cap on any family's budget, which every ≥100k-window family therefore gets. Far
  below those windows on purpose: a prompt this size costs real money per call and dilutes attention, while still
  holding a 200-row listing plus a full `image_facts` batch.
- `PROMPT_BUDGET_WINDOW_PERCENT = 60` — the share of a window one prompt may claim, cloud and local alike. The rest is
  the reply's, which comes out of the same window.
- `MIN_LOCAL_CONTEXT_TOKENS = 16_384` — the smallest local window one turn can run in, mirrored by
  `ai.localContextSize`'s default and its smallest option. See § A local window too small to use.
- `FIXED_PROMPT_OVERHEAD_TOKENS = 3_124` / `RENAME_TOKENS_PER_FILE = 349` / `BATCH_HINT_HEADROOM_PERCENT = 10` — what
  `files_per_batch` divides. See § Sizing a batch from the budget.
- `MAX_TOOL_RESULT_TOKENS = DEFAULT_PROMPT_TOKEN_BUDGET / 2` — the most ONE tool result may spend. Derived from the
  conservative default, not the resolved budget, because a tool handler doesn't know the model (and may be answering an
  external MCP client). Enforced via `mcp::executor::fit_to_result_budget`. **The asymmetry is load-bearing**, not an
  oversight to tidy up: at a user-chosen 200,000 a proportional ceiling would let one result claim 100,000, and the same
  handler would hand that result to an MCP client whose window is a tenth of it. The `budget.rs` header says so too,
  since that's where a reader meets the constant.

Windows and prices both drift: re-verify the families at release time, like `agent::pricing`.
Bumping any constant is a conscious change (never a silent side effect).

### Resolving one budget: three sources, and the answer says which

`budget::resolve_prompt_budget` takes a `BudgetInputs` (provider, model, the user's `askCmdr.chatMemorySize` choice, the
local server's window) and answers a `ResolvedBudget` carrying `prompt_tokens`, the `BudgetSource` that decided, the
window it believes the model has, and whether the user's size exceeds that window. `session.rs` gathers those values
once per turn and logs the source; the core reads no settings and no app state (invariant 2).

- **`UserSetting`** — an explicit size, used exactly as chosen. It is never clamped down: our table will be wrong
  sometimes and the user may be right about their own model. `over_known_window` rides along, the settings row warns
  ("Your model may refuse a message this long"), and the provider gets the final say.
- **`LocalServerWindow`** — 60% of `ai.localContextSize`.
- **`FamilyTable`** — 60% of the family's window, capped at 60,000. A family row carries its WINDOW, not a budget, so no
  row can claim more than its window holds.
- **`Default`** — nothing recognized the model, so `DEFAULT_PROMPT_TOKEN_BUDGET` and no claim about the window (an
  unknown model can't be "over" a window nobody knows).

**There is no provider-reported window.** No API this app talks to reports one, so the table is the only knowledge we
have, and it will age. Hence the label: a budget that came from a stale table is visible in the log rather than silently
authoritative. `every_shipped_cloud_preset_is_in_the_family_table` walks the default model of every provider preset
`lib/settings/cloud-providers.ts` ships, so a new preset without a row fails a test rather than quietly costing a user
four fifths of their window; gateway-prefixed ids (`openai/gpt-4.1-mini`, `accounts/fireworks/models/llama-v3p3-…`)
normalize to the family they name. Ollama, LM Studio, and an unconfigured Custom endpoint stay OUT on purpose: their
window is whatever the user's own server was started with, and the conservative default is the honest answer.

**A settings change never moves a turn in flight.** The choice is read fresh per send
(`settings::load_ask_cmdr_chat_memory_size`) and the resolved number rides `TurnParams::prompt_budget` as a value, the
same shape as the interactive model override.

### A local window too small to use

`prompt_budget_for_local_context(4096)` was 2,457 tokens against 5,077 of fixed overhead: the shipped default could not
complete one turn. So `MIN_LOCAL_CONTEXT_TOKENS = 16_384` is the floor, `ai.localContextSize` offers nothing smaller,
and a stored 2,048 / 4,096 / 8,192 no longer validates (it resolves to the 16,384 default on load, migrating an early
tester instead of leaving them broken).

A window UNDER the floor is still reachable — a local server Cmdr didn't launch at the current setting — and it is
refused, not assembled: `BudgetRefusal::LocalWindowBelowFloor` reaches the rail as
`AgentErrorKindView::LocalWindowTooSmall`, whose copy names the number to pick and where. The refusal happens in
`ask_cmdr_send_message` before a conversation exists, like the consent gate.

### Reporting what a turn cost

Two events carry context pressure to the user, and both are once per turn:

- `ContextTrimmed` — the budget pushed history out. Fired only when the BUDGET forced it (`ElisionFacts::budget_forced`),
  not on ordinary age-based elision, or it would cry wolf every turn on a small window.
- `ContextUsage` — what the prompt cost against its budget, plus the set-aside count, for the rail's gauge. Emitted on
  the ANSWERED path from that call's own assembly, which is the turn's last and largest (each tool result joins the same
  prompt). A failed or cancelled turn reports nothing: the user is looking at an error line, and the previous turn's
  stored figure stays the last thing actually measured.

The same call stamps `conversations.last_prompt_tokens` / `last_prompt_budget` (store migration v3, nullable, no
backfill), so reopening a thread shows its last real reading instead of an empty gauge. The pair is read as a pair: a
size without the budget it was measured against can't become a percentage, so a half-recorded row is no measurement at
all. A persist problem is logged and dropped — a gauge is worth no turn.

### Sizing a batch from the budget

`files_per_batch(prompt_tokens)` answers how many files one content-based rename batch fits, as the **smaller of two
limits**:

- what the PROMPT holds: `(budget − 10% headroom − 5,077 of prefix) / 349 per file`. The headroom exists because the
  measured 100-file turn came in ~4% above what the per-file costs account for (the paths the calls name, the envelope,
  the user's sentence, JSON scaffolding).
- what one REPLY can emit: `AGENT_MAX_OUTPUT_TOKENS` (12,000), less a half-slot reasoning reserve, divided by the plan
  row's 59 tokens, so **101**.

27 files at 16,000, 68 at 32,000, then **101 from roughly 50,000 upward** — including at 200,000, because past that
crossover the reply's ceiling binds and a bigger window buys no bigger batch.

**Both limits are load-bearing.** The number is advertised to the model as "propose this many files" and the model
answers by EMITTING that many plan rows, so a hint past the completion slot doesn't degrade gracefully: the reply is cut
off mid-JSON and the whole plan is lost. The reserve is half the slot because reasoning tokens share it and their size
isn't knowable in advance; `ai::client` already retries with a raised ceiling when reasoning consumes everything, so an
exhausted slot is observed behaviour, not a hypothesis.

The arithmetic lives in `budget.rs`, next to the budget it derives from, and `cost_tests.rs` proves the promise against
the real shapes: a batch of exactly that size, assembled against that budget, fits with nothing elided. Renderers may
show the number; they don't own it. The **per-turn envelope** carries it to the model (`rename batch: up to N files`),
because it moves with the model and the setting and so can't live in the cached prefix.

### Sizing a wake's digest from the budget

`wake_digest_budget(prompt_tokens)` gives a wake's digest a fifth of what is left after the fixed prefix. Derived rather
than a constant, for the same reason `files_per_batch` is: the digest OPENS the turn, and everything after it (the
envelope, the tool results the agent pulls once awake, its own reasoning) shares the same window, so a flat number would
let one digest push a small-local-window user's tool results out of the prompt.

`0` is a meaningful answer — a budget that cannot hold the prefix cannot hold a digest either, and `prepare_wake` then
stays quiet rather than opening a thread it can say nothing in.

### What the budgets buy, measured

Estimated tokens from the shipped assets and `estimate_prompt_tokens`. Every figure below is pinned within a tenth by
`context/cost_tests.rs`, whose constants block is the single copy; a failure there names both numbers and says to update
the test and this section together.

- **Fixed overhead: 5,077 tokens** on every single call — 1,371 for `SYSTEM_PROMPT` and 3,706 for the 15 tool
  declarations. It's why the old flat 8k left only ~4.9k for the actual work, so an 11-file `image_facts` batch fit and a
  12-file one did not. **It grows with the tool view**: the suggested-ops trio added ~1,100 tokens of schema, which every
  call pays whether or not it suggests anything, and which costs a 16k budget about four files of rename batch. Even
  `nothing_to_suggest`, one string argument and a two-sentence description, is 105 of them, paid by every rail turn that
  will never call it. A new tool's schema is prefix, so keep its descriptions terse and say the rest once, in the
  registry line or the prompt.
- **Per file: 269 for an `image_facts` row** (at 900 chars of OCR, the corpus average, against the 2,000-char cap — a
  text-dense corpus costs up to ~2.2× more), **59 for a plan row**, **21 for a pane-listing entry**. The facts dominate
  by more than 3×, so a window has to be sized for them, not for the plan.
- **A 100-file content-based rename: 41,554 tokens** for the whole turn. The parts above account for over 90% of it; the
  rest is the paths the calls name, the envelope, the user's sentence, and JSON scaffolding. The facts arrive over
  several `MAX_TOOL_RESULT_TOKENS` pages that all stay in the turn.
- So **60k does 100 files, 16k does roughly 27** (`files_per_batch` says 101 and 27). A model's window must exceed the
  whole turn, not one page of it: every page of facts is evidence the plan cites, so none of it may elide.

## The runtime (`runtime/`)

Five files plus `ChatRuntime` in `mod.rs`: `events.rs` (the `AgentChatEvent` seam and the
typed `AgentErrorKind`), `dispatch.rs` (the `ToolDispatcher` seam and `AppHandleDispatcher`),
`turn.rs` (`run_turn` and everything it drives), `cost.rs` (metering one completed
`respond`), `cmdr_md.rs` (which `CMDR.md`, and how much of it). `mod.rs` re-exports all of
it, so callers keep saying `chat::runtime::X`.

`ChatRuntime` has two entry points and they differ only in what they already know.
`send_message` may create the thread and derives its title from the user's text; `wake` is
handed a thread `agent::wake::prepare_wake` already opened and a `TurnParams` already composed
(`wake_turn_params`). Both open their own write connection and take the per-conversation
single-flight guard. ⚠️ **A wake must not bypass this**: a wake thread is a real conversation the
user can reply to, so calling `run_turn` directly would let the reply and the wake's own turn
run concurrently in one thread. It also means TWO write connections to `main.db` during a wake,
this one and the wake loop's; WAL makes that fine (`wake/DETAILS.md` says why).

### What a turn resolves from live app state (`session.rs`)

The LLM slot, the prompt budget, and the context envelope, all read FRESH per turn. It sits in
`agent/` rather than in `commands/agent/`, which is ABOVE `agent/`: a wake resolves the same
three and may not import upward. One copy is the point — a wake reading a stale budget would
think with a different window than the rail, silently, and nothing about the resulting thread
would say why.

`capture_envelope` takes already-mapped `EnvelopeAttachment`s, so the command layer's view type
stays in the command layer. With no main window (a routine-launched app on macOS)
`PaneStateStore` is absent and the pane fields come back empty — the honest answer, and not a
reason to skip the capture.

### Which `CMDR.md`, and how much of it

`cmdr_md.rs` resolves `<CMDR_DATA_DIR>/CMDR.md` when that variable is set and non-empty, else
`~/.cmdr/CMDR.md`. Production sets no `CMDR_DATA_DIR`, so it is unaffected; only isolated
instances (an E2E run, a `pnpm dev --worktree` instance) move. Sharing the home dotfile meant one
developer's standing instructions rode along in every automated run's prompt, which is a
non-deterministic prefix in exactly the tests that exist to be deterministic. The file stays a
dotfile in home for real use: it is user-authored config, not app-managed state.

The read is capped at `MAX_CMDR_MD_BYTES` (64 KB). The system string is never elided, so a big
`CMDR.md` is a permanent, non-elidable tax on every turn, and the file is hand-written with
nothing else bounding it. Over the cap, the head is fed and a one-line note says so, because a
prompt that stops mid-sentence otherwise reads as the whole of what the user asked for. The cut is
a byte cut, so it can land inside a multi-byte character; the read backs up to the last character
boundary rather than discarding a large non-ASCII file entirely. A file that is not UTF-8 at all
reads as absent, with a warning: silently believing the user wrote nothing is the failure worth
being loud about.

`run_turn` is the driver and holds all the testable logic (no Tauri app needed): it takes
the `AgentLlm`, a `ToolDispatcher`, a write `Connection`, the tools, the `TurnParams`, an
event sink, and a cancel token. `ChatRuntime::send_message` is the thin Tauri-bound wrapper
that `ask_cmdr_send_message` calls: it opens a write connection, lazily creates the conversation, acquires the
per-thread single-flight lock (emitting `Queued` if contended), reads `CMDR.md`, builds the
`AppHandleDispatcher`, and calls `run_turn`. It is registered in managed state by
`agent::start`, so the IPC command is a pass-through.

### Crash / persistence model (cases (a)–(d))

A message's `content_blocks` are written only on that `respond` call's `End`, so partial
state is unambiguous:

- **(a)** assistant text before a non-`End` termination (a provider drop, a crash) is
  discarded — no assistant row — and the UI gets `AgentErrorKind::UnfinishedReply`.
- **(b)** the user row is written on the FIRST `End`, not at send. A first `respond` that
  never reached `End` records nothing, so a re-send re-assembles byte-identically.
- **(c)** completed turns (each written on its own `End`, tool results on their own rows)
  stay persisted. A retry calls `run_turn` with `user_text: None`, which loads the
  persisted transcript and issues a FRESH `respond` from it — not a re-send of the original
  message.
- **(d)** cost is metered per completed `End` via `store::record_cost`, so completed turns
  count once, never double, never lost. Pricing is a per-model table (`pricing.rs`): a local
  model is free + priced, a known cloud model is estimated + priced, and an unknown cloud model
  records tokens with `priced = false` (cost "unknown", never a silent $0 — spec §2.4).

`TurnResult` (`Answered` / `Failed(kind)` / `Cancelled`) is the caller's bookkeeping; the
`AgentChatEvent`s already told the frontend everything.

### Model-change events

`ProposalReady` is a display-only stream event. The runtime emits it only after the proposal dispatcher staged the rows
in `main.db`; chat history persists the concise tool result, not proposal authority.

A settings change can switch a thread's effective model mid-conversation; the thread logs
it honestly as a UI-facing event row (`store::ConversationEvent::ModelChanged`) so the
user sees which replies used which model. Two cooperating paths, one comparison
(`conversations.last_model` vs the effective model):

- **Send-time** (`record_model_transition`, at the turn's FIRST `End`, before the user
  row): covers threads that weren't active when the setting changed (a resumed thread).
  Running at first `End` keeps crash case b intact — a failed first attempt records
  nothing, and the next successful turn re-runs the comparison, so the event is deferred,
  never lost. The first turn of a thread only stamps `last_model` (nothing to switch from).
- **Change-time** (`ChatRuntime::record_model_change`, called by the
  `ask_cmdr_record_model_change` command when a model-affecting setting changes): awaits
  the thread's single-flight lock, so with a turn in flight the event lands right AFTER
  that reply (the turn keeps its already-resolved model — a change never yanks a running
  request). The two paths can't double-log: whichever runs first updates `last_model`, and
  the other sees "unchanged" and no-ops.

The event's identity reaches the live rail via `AgentChatEvent::ModelChanged` (send-time)
or the command's returned `MessageView` (change-time); history shows it via the `Event`
role projection. Event rows never enter the LLM transcript (`load_transcript` filters
them) or the prompt prefix.

**Decision: `Failed` carries `detail: Option<String>` — the source error's own wording —
alongside the typed `kind`.** Why: the typed kinds alone left the user blind on the
catch-all `Provider` case (a retired model slug's "use this slug instead" hint died in the
logs while the UI said only "something went wrong"), so the provider-authored sentence
rides the event and the rail shows it under the friendly headline. It is display only:
the frontend branches on `kind`, never on `detail`, and the string
is rendered as escaped plain text, never `{@html}`. `AgentLlmError::detail()` says which
variants carry wording; `crate::ai`'s `provider_error_detail` extracts the JSON body's
`error.message` (capped) so the UI gets the sentence, not a JSON blob.

### Budgets and cancellation

`MAX_TOOL_TURNS` and `MAX_WALL_TIME` are checked at the TOP of the loop, so the next
`respond` never fires once a budget is spent — a runaway is impossible by construction, and
the typed outcome is `BudgetExhausted`. "Answers with what it has" is realized as the text
already streamed plus that notice; a forced tool-less final answer is a documented
refinement, deliberately not built in v1 (it would need its own bounding). Cancellation is
checked at the top of the loop (a clean stop between tool boundaries) and when a stream ends
without `End` while the token is set (a user stop, distinguished from a crash) — both return
`TurnResult::Cancelled` with no `Failed` event; stream-cancel itself drops the reqwest body
via the token threaded into `AgentLlm::respond`.

## The event seam (`AgentChatEvent`)

The runtime emits typed progress through `ChatEventSink` (a
`tokio::sync::mpsc::UnboundedSender<AgentChatEvent>`). The `ask_cmdr_send_message` command
is a thin adapter:

1. Make the Tauri `Channel<AskCmdrStreamEvent>` from the command args.
2. `let (tx, mut rx) = unbounded_channel();` and spawn a task: `while let Some(ev) =
   rx.recv().await { channel.send(map_to_wire(ev))?; }`.
3. Capture the `ContextEnvelope` from live state (`PaneStateStore` + `snapshot_volumes`),
   resolve the interactive-slot `GenaiAgentLlm`, then call
   `ChatRuntime::send_message(app, &llm, provider, model, conversation_id, text, envelope,
   offset, tx, cancel)`.
4. Map `AgentChatEvent` → the wire `AskCmdrStreamEvent` (§7): `AssistantStarted` carries no
   id (map to a bubble-start); the persisted assistant id arrives on `Done`. A refusal or
   handler problem surfaces as `ToolCallFinished { ok: false }`. NEVER forward a reasoning
   blob or provider state — the events already exclude them.

The envelope's live sources (plan §9): focused pane path from `PaneStateStore` (it returns
the pane SIDE, so resolve that side's directory from the snapshot), cursor + selection from
pane state, per-volume freshness + SMB connectivity from `snapshot_volumes()`. Map those
live types into `context`'s pure `EnvelopeFreshness` / `EnvelopeConnectivity` mirrors.

## Testing notes

The context tests are four modules under `context/`, split by concern: `tests.rs` (the prefix,
the envelope, elision, the budget), `stub_tests.rs` (what a dropped result says, and that a plan
can't cite it), `cost_tests.rs` (what the real shapes cost), and `test_support.rs` (the
transcript builders and budgets they share). Put a new context test in the module whose concern
it matches rather than growing `tests.rs`.

Every `context.rs` test runs with no tokio runtime (the core is pure). The runtime tests
use a local `ProgrammableLlm` (per-turn text / tool calls / usage / a mid-stream drop with
no `End`) and scripted `ToolDispatcher` doubles — there is no in-tree full-Tauri harness for
the agent toolset at unit-test scope, so tool dispatch is exercised at the seam level.
Wall-time uses `tokio::time` under `start_paused`; a `SleepingDispatcher` advances virtual
time past the ceiling. The read→green evidence for the load-bearing invariants
(prefix stability, envelope-only-on-latest, elision, budget halt, crash-a persistence) was
captured by mutation before the code was completed.
