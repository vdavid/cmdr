# Agent chat details

Pull-tier docs for `agent/chat/`. Must-knows live in `CLAUDE.md`.

## Anatomy of one call (spec §5, as built)

`run_turn` assembles and sends, top to bottom:

1. **System** (`system` arg), in three parts and this order: the agent's own memory, fenced
   (§ The memory block); `system_prompt::system_prompt()` (the text is `../prompts/system.md`, read once per turn);
   then the user's `CMDR.md` under a
   header if present (§ Which `CMDR.md`, and how much of it). Stable, cached. The prompt's
   labelled sections (identity, what you can do, coverage, memory, renaming, suggesting
   operations, evidence, style) let a rule be found and edited without re-reading the block;
   every load-bearing rule is pinned by a prompt-asset test in `system_prompt.rs`.
   **Each rule that forbids something also names the action to take instead** — a prohibition
   alone leaves the next token to chance, which is how a batch of screenshots got 12
   fabricated names.
2. **Tool declarations** (`tools` arg): `agent::tools::agent_tool_declarations()`. Stable,
   cached.
3. **History**: every persisted turn, each user turn opening with the envelope it was sent with
   (an `AgentPart::Envelope`, replayed verbatim; a row from before envelopes were stored gets a
   local timestamp marker, `[Fri 2026-07-11 09:15]`, instead). Sent exactly as last call while the budget has
   room (§ Budget enforcement); under pressure, the oldest turns' tool results collapse to a
   self-describing stub (§ The elision stub). Assistant prose survives verbatim. Eliding prose is never done — that's the soft-cap's job. The
   current turn's results never elide either (see § Budget enforcement).
4. **The envelope** opens each user turn, as a tagged block (§9 field set), rendered ONCE at send
   (`runtime/turn.rs::user_parts`) and persisted with the user row as `AgentPart::Envelope`. Every later call
   replays that string, so snapshot-at-send holds across the turn's tool loop, a crash-resume, and every later turn,
   and the previous call's prompt is a byte-identical prefix of the next
   (`the_whole_previous_prompt_is_a_prefix_of_the_next_turns`). Only a legacy latest row without one is rendered from
   the caller's envelope. The rail never shows the part and search never indexes it. Its shape:
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

## The memory block (why it leads, and why it is fenced)

`build_system` puts three things in the system string, and the ORDER is a security decision, not a layout one:

```
<the introducing line>

----- BEGIN SAVED NOTES (data) -----
<what the agent wrote about the user, cut to this turn's share>
----- END SAVED NOTES -----

<the system prompt>

The user's CMDR.md (their notes for you; read-only):
<CMDR.md>
```

⚠️ **Memory leads, and the rules follow it.** `CMDR.md` is appended AFTER the whole prompt, which is the strongest
override position a prompt has — fine for a file the user hand-writes, wrong for one the agent writes, because the
agent's write path is reachable from text it read (`image_facts` OCR, file names off disk). Whatever gets in there
rides the cached prefix of every later turn, including ones that call `propose_suggestions`, and survives restarts and
thread deletion. `../memory/DETAILS.md` § The injection surface.

Three properties make the block safe to feed at all, and all three have tests in `context/tests.rs`:

- **It is announced.** `MEMORY_HEADER` says the block is data, not instructions; that it never overrides the rules that
  follow; and that anything inside reading as an order got there from something the agent read, so it should be
  reported rather than obeyed.
- **The content cannot close the fence.** `fenced_memory` defangs any `MEMORY_END` / `MEMORY_BEGIN` in the content, so
  exactly one line in the finished string can close it. A fence whose text can forge its own terminator is not a fence.
- **Two fields, ❌ never one concatenation.** `PrefixInputs.cmdr_md` and `PrefixInputs.memory` stay separate all the way
  through, so each is labelled in its own voice. Merging them would launder agent-written text into the user's.

How much memory a turn carries is `budget::memory_slice_bytes` — see § What the budgets buy.

## Budget enforcement (elide-only, floored, and reported)

`assemble_prompt` takes the resolved model's `budget`. While the prompt fits, nothing elides. Over budget, it
elides the OLDEST turn's tool results first and tightens one turn at a time until the estimate fits, never touching
prose.

**Decision: only budget pressure elides, never age.** Why: elision used to start at three turns back whatever the
budget, which threw away listings the agent had already paid for and, worse, rewrote an earlier message on every turn.
A caching provider (Anthropic, OpenAI, Gemini, DeepSeek) bills the prompt from the first changed byte at full price, so
that rewrite made long chats MORE expensive, not less. With the budget now the whole window
(§ Resolving one budget), a thread with room sends every earlier message byte-identical to the last call
(`history_is_byte_stable_from_turn_to_turn_while_the_budget_has_room`). Cost control is compaction's job (planned).

The envelope is the other half: it used to open only the LATEST user turn and re-render as a bare timestamp once a
newer turn arrived, which broke the cache at the previous user message every turn. Each turn now persists the envelope
it was sent with and replays it (item 4 of § Anatomy of one call), at the cost of one envelope's tokens per historical turn,
cached.

**What still moves earlier bytes between calls** (known):

- **Memory writes.** Memory leads the system string, so a `memory_write` / `memory_edit` changes the prefix and the
  next call re-reads the whole thread at full price. Inherent to memory living in the prefix.
- **Budget-pressure elision itself** rewrites the turn it stubs, by design.

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

- `MAX_TOOL_TURNS = 25` — rounds of tool calls per message; past it ONE wrap-up call answers (§ Budgets and
  cancellation). Why 25: at 8, a real "where should these Downloads go" turn (verified on a dev build, `main.db`
  conversation 689, 2026-10-28) spent all eight rounds orienting (pane, importance, home, Documents, two cloud
  folders, volumes, the NAS, then a memory note) and never answered, at a tenth of its window. A cleanup that also
  reads its destinations and proposes in `files_per_batch` batches needs 15–20 rounds; 25 leaves headroom, and the
  wrap-up call plus "Keep going" carry a longer job across answers. Spend: a round re-sends the prompt, which grows
  by what the round read (each result is capped at `MAX_TOOL_RESULT_TOKENS`), so 25 rounds of a turn like 689's
  (~11k prompt, ~3k growth per round) re-send roughly 1.2M prompt tokens, mostly prompt-cache reads. The cap and the
  repeat breaker are what bound that; a dollar ceiling per answer is not built.
- `MAX_WALL_TIME = 600s` — per message wall-clock ceiling across the whole loop: a hang guard, not a spend guard
  (Stop is the user's). Ten minutes is 25 rounds of a reasoning-heavy model at ~20 s each with room left; it ends
  the turn with no further call.
- Neither is a setting: the rail's limit notice says the limit is fixed and links Send feedback. ❌ No token total
  gates a turn: the per-turn and per-thread token figures are display (the stats line, the cost footer); only the
  context window bounds one call (`tokens_summed_across_rounds_never_end_a_turn`).
- `MIN_ELISION_TURNS_BACK = 1` — the floor above; the current turn's results never elide.
- `THREAD_SOFT_CAP_MESSAGES = 40` — past this the UI nudges "start a fresh one?".

In `budget.rs` (how many tokens a prompt and a tool result may spend):

- `CHARS_PER_TOKEN_ESTIMATE = 4` — the ONE size-estimate divisor (elision, the stub hint, every tool's self-cap).
- `DEFAULT_PROMPT_TOKEN_BUDGET = 16_000` — an unrecognized model's budget. Conservative because guessing high is a hard
  provider rejection mid-turn; still double the 8k that overflowed on a 12-file batch.
- `ESTIMATE_SAFETY_PERCENT = 15` — what Automatic leaves unclaimed of a window after the reply's room
  (`AGENT_MAX_OUTPUT_TOKENS`): the margin for `chars/4`'s error on JSON- and path-heavy prompts, which real tokenizers
  split finer. Cloud and local alike, so 1,000,000 resolves to 839,800, 200,000 to 159,800, and 32,768 to 17,652.
- `WAKE_DIGEST_MAX_TOKENS = 10_000` — the cap on a wake's digest, whatever the window. See § Sizing a wake's digest.
- `MIN_LOCAL_CONTEXT_TOKENS = 32_768` — the smallest local window one turn can run in, mirrored by
  `ai.localContextSize`'s default and its smallest option. It tracks the prefix, so it moves when the tool view grows.
  See § A local window too small to use.
- `FIXED_PROMPT_OVERHEAD_TOKENS = 7_107` / `RENAME_TOKENS_PER_FILE = 349` / `BATCH_HINT_HEADROOM_PERCENT = 10` — what
  `files_per_batch` divides. See § Sizing a batch from the budget.
- `MAX_TOOL_RESULT_TOKENS = DEFAULT_PROMPT_TOKEN_BUDGET / 2` — the most ONE tool result may spend. Derived from the
  conservative default, not the resolved budget, because a tool handler doesn't know the model (and may be answering an
  external MCP client). Enforced via `mcp::executor::fit_to_result_budget`. **The asymmetry is load-bearing**, not an
  oversight to tidy up: at a user-chosen 200,000 a proportional ceiling would let one result claim 100,000, and the same
  handler would hand that result to an MCP client whose window is a tenth of it. The `budget.rs` header says so too,
  since that's where a reader meets the constant.

Windows and prices both drift: re-verify the families at release time, like `agent::pricing`.
Bumping any constant is a conscious change (never a silent side effect).

### Resolving one budget: four sources, and the answer says which

`budget::resolve_prompt_budget` takes a `BudgetInputs` (provider, model, the user's `askCmdr.chatMemorySize` choice, the
local server's window, the provider-reported window) and answers a `ResolvedBudget` carrying `prompt_tokens`, the
`BudgetSource` that decided, the window it believes the model has, and whether the user's size exceeds that window.
`session.rs` gathers those values once per turn and logs the source; the core reads no settings and no app state
(invariant 2).

**Automatic spends the whole window** (`budget_for_window`): the window, less the reply's room
(`AGENT_MAX_OUTPUT_TOKENS`, which comes out of the same window), less `ESTIMATE_SAFETY_PERCENT` of the rest. There is no
cap below the window. Decision/Why: a 1M-window user saw "23,648 of 60,000" and most of their model hidden behind a
60K cap set for cost; David chose the whole window. **Cost is compaction's job, planned and not built**: `/compact`,
auto-compacting old tool results once the one-hour prompt-cache TTL has lapsed (a re-read then costs full price anyway),
and a "consider compacting" nudge past roughly 200,000 tokens. Elision (`context.rs`) is the only shrinking today, and
only under budget pressure. An explicit "Chat memory size" is the user's lever to spend less meanwhile.

- **`UserSetting`** — an explicit size, used exactly as chosen. It is never clamped down: our knowledge will be wrong
  sometimes and the user may be right about their own model. `over_known_window` (the size plus the reply's room past the window: a provider refuses
  prompt plus `max_tokens` over it) rides along, the settings row warns ("Your model may refuse a message this long"),
  and the provider gets the final say.
- **`LocalServerWindow`** — the whole of `ai.localContextSize`, as above.
- **`ProviderReported`** — the whole window the provider's own model list reports (`ai::model_capabilities`, via
  `session::reported_window_tokens`). Wins over the table in both directions: the report is today's answer.
- **`FamilyTable`** — the whole of the family's window. A family row carries its WINDOW, not a budget, so no row can
  claim more than its window holds.
- **`Default`** — nothing recognized the model, so `DEFAULT_PROMPT_TOKEN_BUDGET` and no claim about the window (an
  unknown model can't be "over" a window nobody knows).

**The report never blocks a send.** `ai::model_capabilities` answers from an in-memory cache and refreshes in the
background (at `configure_ai`, on a connection check, and when a lookup finds the catalog over a day old), so the first
send after launch sizes from the table. OpenAI, DeepSeek, xAI, and Qwen report no window at all, so for them the table
is all there is, and it will age. Hence the label: a budget that came from a stale table is visible in the log rather
than silently authoritative. Since the budget is the whole window now, a table row that overclaims is a rejected prompt:
every row is a floor (`sonar` is 127,072, not a rounded 128,000).
`every_shipped_cloud_preset_is_in_the_family_table` walks the default model of every provider preset
`lib/settings/cloud-providers.ts` ships, so a new preset without a row fails a test rather than quietly costing a user
most of their window; gateway-prefixed ids (`openai/gpt-4.1-mini`, `accounts/fireworks/models/llama-v3p3-…`)
normalize to the family they name. Ollama, LM Studio, and an unconfigured Custom endpoint stay OUT on purpose: their
window is whatever the user's own server was started with, and the conservative default is the honest answer.

**A settings change never moves a turn in flight.** The choice is read fresh per send
(`settings::load_ask_cmdr_chat_memory_size`) and the resolved number rides `TurnParams::prompt_budget` as a value, the
same shape as the interactive model override.

### Reasoning effort and the reply's room (`effort.rs`)

The Effort picker (Settings › AI, `ai.reasoningEfforts`, keyed `"<provider id>/<model>"`) decides how hard the
interactive slot's model thinks. `session::resolve_call_settings` gathers the pick and what the provider reports
(`ai::model_capabilities`), and the pure `effort.rs` turns them into `CallSettings { effort, max_output_tokens }`,
which rides `TurnParams.call` into every `respond` of the turn.

- **`effort_options`**: what the model can take. The provider's report first (Anthropic `capabilities.effort` and
  `thinking.types`, OpenRouter `reasoning`), then `REASONING_FAMILIES` for providers that report nothing (OpenAI) or
  say "reasons" without levels (Groq `gpt-oss`, Gemini). Recommended's level: the reported default, else the
  documented one (Anthropic high, OpenAI Responses medium), else none (Gemini thinks dynamically). A local model, and
  a model nothing knows, offer Recommended only; a model known not to reason gets no picker.
- **`resolve_effort`**: Recommended sends `ProviderDefault`, the one value safe everywhere. A pick the model can't take
  maps to the nearest level it can (ties to the cheaper; Off without an off switch to the cheapest; no known levels to
  Recommended), and `session.rs` logs the mapping. ❌ Never send an explicit level the options don't list: it's a 400.
- **`reply_room`**: thinking spends the reply's `max_tokens`, so the ceiling grows with the level: 12,000
  (`AGENT_MAX_OUTPUT_TOKENS`, Off / Low / unknown), 16,000 Medium, 32,000 High, 64,000 Max, capped at the model's
  reported `max_output_tokens`. Recommended sizes for what the provider will do (Claude's default is high, so 32,000).
  Decision/Why: at a flat 12,000, a High or Max think could hit `MaxTokens` before the tool call and end the turn
  unfinished; a ceiling is not a charge, the provider bills what the model writes.
- **`fit_reply_to_window`**: the ceiling then fits the model's known window: at most a quarter of it, never under the
  base slot while the window can spare it, and less (down to `MIN_REPLY_TOKENS`, 4,096) where the prompt would
  otherwise fall under `MIN_CLOUD_PROMPT_TOKENS` (the prefix plus one paged result). Decision/Why: High on a 32K window
  reserved 32,000 and left the prompt 652, under its own prefix, while prompt plus `max_tokens` overran the window.
- **The budget leaves that room free**: `BudgetInputs::reply_tokens` is the same `max_output_tokens`, and the send path
  resolves the call settings once and hands both to `resolve_prompt_budget_for`, so prompt and reply can't overlap.
- **`files_per_batch` stays on the base slot.** A bigger ceiling at a higher effort is room the model spends thinking,
  not room for more plan rows. The agent path has no exhaustion retry (`ai::client`'s empty-response retry serves the
  one-shot translate calls, with its own small ceiling).
- **Wakes run on Recommended** (`session::resolve_wake_call_settings`), resolved like the rail's, so Claude's
  default-high thinking gets High's room; a background sweep never inherits the user's pick. The wake's budget is sized
  with that same call (`resolve_prompt_budget_for`).
- **Metering** already counts thinking as output: `completion_tokens` includes reasoning on every provider, and
  `genai_impl::map_usage` covers Gemini's thoughts-only gap (`../llm/DETAILS.md` § Reasoning effort).

### A local window too small to use

A window that gives the reply its 12,000 tokens has to hold `FIXED_PROMPT_OVERHEAD_TOKENS` (7,107) plus a paged tool
result before it holds a single word of the conversation, so the floor is `MIN_LOCAL_CONTEXT_TOKENS = 32_768`, which
resolves to 17,652. `ai.localContextSize` offers nothing smaller, and a smaller stored size no longer validates (it
resolves to the 32,768 default on load, migrating an early tester instead of leaving them broken).

**The floor is derived, not chosen**, and `budget.rs`'s `the_floor_leaves_room_for_a_prefix_and_a_paged_result` is what
holds it: one window down, 16,384 resolves to 3,726 and cannot even fit the prefix. Every tool that
joins the agent view grows the prefix, so that assertion is answered by raising the floor or shrinking the prefix, never
by shrinking the reply's room or the estimator's margin.

A window UNDER the floor is still reachable — a local server Cmdr didn't launch at the current setting — and it is
refused, not assembled: `BudgetRefusal::LocalWindowBelowFloor` reaches the rail as
`AgentErrorKindView::LocalWindowTooSmall`, whose copy names the number to pick and where. The refusal happens in
`ask_cmdr_send_message` before a conversation exists, like the send gate (`session::admit_send`).

### Reporting what a turn cost

Three events carry context pressure and spend to the user:

- `ContextTrimmed` — the budget pushed history out, once per turn. Only the budget elides, so it fires whenever an
  assembly elided anything (`ElisionFacts::budget_forced`), at most once per turn.
- `ContextUsage` — what the prompt cost against its budget, plus the set-aside count, for the rail's gauge. Emitted after
  EVERY completed call (its `End`) from that call's own assembly, so the gauge moves round by round through a
  tool-running turn; the turn's last report is its largest assembly (each tool result joins the same prompt). A call
  that never reached `End` reports nothing: the user is looking at an error line, and the previous call's stored figure
  stays the last thing actually measured.
- `CostMetered` — right after `meter_cost` folds a completed call in, no payload. The rail re-reads
  `ask_cmdr_conversation_cost` on it, so the footer's total moves mid-turn while the meter stays its one source.
  ❌ Don't put the call's usage on it for the rail to add up: two totals drift.

Before any call, the rail's gauge shows the BASELINE (`slot_status.rs`, `ask_cmdr_slot_status`): `context::prefix_tokens`
over the same prefix parts a send reads (`runtime::PrefixParts`, so the two can't drift) against the budget the next
send would resolve. It's the honest floor, since every call carries the whole prefix. The same command answers whether
a provider is set up at all (`ProviderReadiness`: setup only; consent is the rail gate's question and a managed refusal
the send's) and how the next call is priced, so a new chat's footer says "free", "about $0", or "cost unknown" from the
start. The E2E fake reads as ready there, from the same `test_mode::ask_cmdr_fake_active` accessor `resolve_agent_llm`
gates on, so "send allowed" and "send answered" can't disagree.

Each `ContextUsage` stamps `conversations.last_prompt_tokens` / `last_prompt_budget` (store migration v3, nullable, no
backfill), so reopening a thread shows its last real reading instead of the baseline. The pair is read as a pair: a
size without the budget it was measured against can't become a percentage, so a half-recorded row is no measurement at
all. A persist problem is logged and dropped — a gauge is worth no turn.

### Sizing a batch from the budget

`files_per_batch(prompt_tokens)` answers how many files one content-based rename batch fits, as the **smaller of two
limits**:

- what the PROMPT holds: `(budget − 10% headroom − 7,107 of prefix) / 349 per file`. The headroom exists because the
  measured 100-file turn came in ~4% above what the per-file costs account for (the paths the calls name, the envelope,
  the user's sentence, JSON scaffolding).
- what one REPLY can emit: `AGENT_MAX_OUTPUT_TOKENS` (12,000), less a half-slot reasoning reserve, divided by the plan
  row's 59 tokens, so **101**.

20 files at 16,000, 62 at 32,000, then **101 from roughly 50,000 upward** — including at 200,000, because past that
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
let one digest push a small-local-window user's tool results out of the prompt. It is capped at `WAKE_DIGEST_MAX_TOKENS` (10,000, what a 60,000-token budget gave it), since a fifth of a 1M window is a digest
nobody needs and every wake would pay for it.

`0` is a meaningful answer — a budget that cannot hold the prefix cannot hold a digest either, and `prepare_wake` then
stays quiet rather than opening a thread it can say nothing in.

### What the budgets buy, measured

Estimated tokens from the shipped assets and `estimate_prompt_tokens`. Every figure below is pinned within a tenth by
`context/cost_tests.rs`, whose constants block is the single copy; a failure there names both numbers and says to update
the test and this section together.

- **Fixed overhead: 7,107 tokens** on every single call — 2,378 for the system prompt and 4,729 for the 21 tool
  declarations. The old flat 8k budget, against a prefix half this size, still left only ~4.9k for the actual work, so
  an 11-file `image_facts` batch fit and a 12-file one did not. **It grows with the tool view**: the four suggested-ops
  tools are ~1,300 tokens of schema (`propose_suggestions` alone 759, the most expensive declaration), which every call pays whether or not it suggests anything, and which costs a 16k budget
  about four files of rename batch. Even
  `nothing_to_suggest`, one string argument and a two-sentence description, is 97 of them, paid by every rail turn that
  will never call it. `memory_write` + `memory_edit` cost 252 between them, and the prompt's memory section another 265.
  A new tool's schema is prefix, so keep its descriptions terse and say the rest once, in the registry line or the
  prompt. `search` is the next most expensive at 602 (483 of schema, 119 of description), and the
  § Coverage paragraph that teaches a model to read its answer costs 182 more.
- **Per file: 269 for an `image_facts` row** (at 900 chars of OCR, the corpus average, against the 2,000-char cap — a
  text-dense corpus costs up to ~2.2× more), **59 for a plan row**, **21 for a pane-listing entry**. The facts dominate
  by more than 3×, so a window has to be sized for them, not for the plan.
- **A 100-file content-based rename: 43,689 tokens** for the whole turn. The parts above account for over 90% of it; the
  rest is the paths the calls name, the envelope, the user's sentence, and JSON scaffolding. The facts arrive over
  several `MAX_TOOL_RESULT_TOKENS` pages that all stay in the turn.
- So **60k does 100 files, 16k does roughly 20** (`files_per_batch` says 101 and 20). A model's window must exceed the
  whole turn, not one page of it: every page of facts is evidence the plan cites, so none of it may elide.
- **Memory is priced as a SHARE, not a block**: `memory_slice_bytes` takes a tenth of what the budget has left after the
  overhead, so a 16k budget carries ~4 KB of notes and a 60k one ~21 KB. From a budget of about 170,000 up, the share
  passes the 64 KB disk cap, so on a large window the disk cap binds and the whole folder rides along. It has to be a share because the system string
  is never elided (memory is a permanent tax on every turn) and because the agent writes the file itself — a flat cap
  would let it degrade its own chat with no way back. ❌ Not to be confused with `memory::MEMORY_DIR_MAX_BYTES`, the
  64 KB DISK cap.

## The runtime (`runtime/`)

Eight files plus `ChatRuntime` in `mod.rs`: `types.rs` (what a turn IS as data), `events.rs`
(the `AgentChatEvent` seam and the typed `AgentErrorKind`), `dispatch.rs` (the
`ToolDispatcher` seam and `AppHandleDispatcher`; every call carries a `CallContext`, the turn's origin and its Stop,
and runs under `dispatch_until_stopped`, § Stop reaches every tool call), `turn.rs` (`run_turn` and everything it
drives), `repeats.rs` (the turn's memory of calls that already failed), `cost.rs` (metering one
completed `respond`), `cmdr_md.rs` (which `CMDR.md`, and how much of it), `analytics.rs` (the
anonymous `ask_cmdr_turn` event). `mod.rs` re-exports all of it, so callers keep saying
`chat::runtime::X`.

**`types.rs` is a leaf, and that is load-bearing.** `UserTurn`, `TurnParams`, `TurnResult`,
and `TurnTally` live there rather than in `turn.rs` because THREE modules need that
vocabulary (`turn` drives a turn, `cost` and `analytics` each report on one afterwards) and
only one implements the driver. With the types in `turn.rs`, both reporters imported the
driver's module and the driver imported theirs, which `module-cycles` correctly read as a
circle among `analytics`, `cost`, and `turn` for no design reason: nothing in the vocabulary
calls anything. It takes no `super` import beyond `events::AgentErrorKind`, which is what
keeps the arrows pointing one way; the module's own doc carries the rule that holds it.

`ChatRuntime` has two entry points and they differ only in what they already know.
`send_message` may create the thread and derives its title from the user's text; `wake` is
handed a thread `agent::wake::prepare_wake` already opened and a `TurnParams` already composed
(`wake_turn_params`). Both open their own write connection and take the per-conversation
single-flight guard. ⚠️ **A wake must not bypass this**: a wake thread is a real conversation the
user can reply to, so calling `run_turn` directly would let the reply and the wake's own turn
run concurrently in one thread. It also means TWO write connections to `main.db` during a wake,
this one and the wake loop's; WAL makes that fine (`wake/DETAILS.md` says why).

### The repeat breaker (`repeats.rs`)

A refused `propose_rename_plan` came back to the driver eight times running, byte-identical each
time, until `MAX_TOOL_TURNS` ended the turn: about 90 seconds and eight provider round trips
spent on one broken payload, and from the user's side the agent simply stopped answering. The
model had nothing new to read, so it had nothing new to try.

`FailedCalls` is a per-turn map from the tool's wire name plus its whole serialized arguments
object, keys sorted at every depth (`AgentToolCall::identity_key`; `serde_json` keeps insertion order in this
build, so it sorts explicitly and key order can't split one call into two records) to what that call came back with. The driver judges each call before dispatching:

1. **Nothing identical failed yet** ⇒ dispatch, and record the content if `dispatch_ok` says it
   came back with a problem.
2. **Identical to a failed call** ⇒ don't dispatch. Hand back that original content plus
   `repeatedCall: true` and a sentence saying this answers the same way every time, so the model
   still reads what to fix and learns that re-sending is not the move.
3. **Identical to one it was already told that about** ⇒ hand back the same again (the transcript
   still needs a result row against every call, or the next turn loads a dangling tool call), and
   end the turn with `AgentErrorKind::RepeatedToolCall` once the message's calls are all
   answered. Three round trips instead of eight, and the user reads "it kept retrying the same
   lookup" rather than "it hit its limit".

**Why it can't fire on a legitimate repeat.** Only a call that came back with a PROBLEM is
remembered, judged by `dispatch_ok`'s typed result keys and never by wording. A re-fetch of an
elided result (which the system prompt explicitly asks for, and which every idempotent local
read supports) repeats a call that SUCCEEDED, so it was never recorded. Paging varies `offset`,
which is part of the key. **What it does cost**: a call that failed for a passing reason gets no
second execution inside the same turn. That is the trade, taken deliberately: the model is
handed the original problem rather than a silence, varying the arguments at all dispatches
normally, and a turn a user is waiting on stops burning its whole budget on one payload.

The other half of the same fix is that a refusal has to be worth reading. The rename boundary
now names WHICH rows and WHICH field (`../tools/propose/rename/plan.rs`, `rows_problem`), so a
model that reads it can act on it and never reaches step 2.

### The turn event (`analytics.rs`), and why it is the funnel's denominator

`run_turn` is a thin wrapper: it makes a `TurnTally`, calls `drive` (the loop, with every
early return it always had), and on the way out writes the `TurnFinished` row, emits the ONE
terminal event, and reports one `ask_cmdr_turn` (§ The turn's end, below). The wrapper
exists because `drive` returns from a dozen places and this is the only point all of them
meet; scattering a capture across those returns is how one gets missed.

The tally is a `&mut` the loop increments as it goes (`tool_turns`, `tool_calls`, the summed
`prompt_tokens` / `completion_tokens`, and `proposals` for tool calls that staged something to
review), so a turn that fails on its fourth tool round still reports four, not zero.

**Why the event has to exist at all.** The proposal events downstream
(`../suggested_ops/analytics.rs`) carry the agent's north-star acceptance rate, and a zero on
a north-star is only actionable if you can tell WHICH zero it is. Without a turn event,
"nobody configured a provider", "people chat but the model never proposes", and "the capture
path is broken" all read as the same empty chart. With it they are three different shapes:
no `ask_cmdr_turn` at all, `ask_cmdr_turn` with `proposals: "0"`, or `proposals` above zero
with no `suggestion_group_proposed` behind it (that last one is the only one that is a bug).

**The funnel's top is a second reporter.** The gates in `../../commands/agent/chat.rs` refuse a
send BEFORE `run_turn` exists: no agent store, Ask Cmdr off (`askCmdrOff`), cloud AI not allowed
(`noCloudConsent`, from the slot's `SlotRefusal`), the organization's managed policy refusing the provider
(`managedByOrganization`, `SlotRefusal::Managed`), no resolvable provider, and
a local window under the one-turn floor. None of them reach `drive`, so `AskCmdrSendRefusal`'s
two constructors report the same event with `outcome: "refused"` and the gate as `failure`.
Without that half, "AI is off" and "nobody opened the rail" are both no events at all, which is
the exact ambiguity this event exists to remove. ⚠️ Build a refusal through `of` or `detailed`,
never as a struct literal, or that gate goes unreported.

Both enums that can land in `failure` (`AgentErrorKind` here, `AgentErrorKindView` in
`stream.rs`) tokenize the shared variants identically, pinned by a test. One gate reporting
under two names would make the funnel's top and middle unaddable. `ManagedByOrganization` is in both: the slot
refuses with it before a turn, and the LLM client's per-request policy backstop ends a running turn with it
(`AgentLlmError::Managed`; `../../ai/DETAILS.md` § Managed policy). A wake logs that end at info.

`origin` splits the surfaces: `text` (the rail), `wake` (the proactive loop), `outcomes` (the
follow-up turn a rejection opens), and `resume`, RESERVED: no production caller sets it. A user's
retry in the rail is a fresh send, so it runs and counts as `text`. It is `TurnParams::origin`
(`agent::types::TurnOrigin`), stated by each caller rather than read off the opener, because the
dispatch gate reads the same value per call: `Access::Session` tools run only on a user-started
origin (`agent/tools/DETAILS.md` § The dispatch gate). `resume` is kept for a replay the USER asks
for from a stored transcript (no opener, so it would otherwise inflate whichever origin it was
folded into), which is why it counts as user-started. ⚠️ A replay the APP starts on its own (say,
after a crash, or of a wake thread) is not that, and needs an origin of its own. A refusal is always `text`; a wake never
reaches that path. `provider` reads `unresolved` on a refusal, since every gate but the local
window fires before the slot resolves and naming the settings value would claim a provider was chosen
for a send that never chose one. ⚠️ **`origin: "wake"` is not the same
population as `agent_wake`** (`../wake/runner.rs`): that event counts every wake OUTCOME,
including the ones that never reached a turn. Comparing the two is the point; merging them
would lose it.

`AgentErrorKind::as_token` is deliberately separate from the enum's serde wire form. The
frontend's form follows the copy and the bindings; a metric a saved analytics query groups by
must not move when either changes.

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
  discarded — no assistant row — and the UI gets `AgentErrorKind::UnfinishedReply`. An `End`
  whose message carries NO parts takes this same path: it's what a degenerate provider turn
  reduces to once `llm/genai_impl.rs` drops a nameless tool call, and persisting it would
  show a blank bubble and call it an answer.
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

### Slot-change events

`ProposalReady` is a display-only stream event. The runtime emits it only after the proposal dispatcher staged the rows
in `main.db`; chat history persists the concise tool result, not proposal authority.

A settings change can move a thread's interactive slot mid-conversation, and the thread
logs it honestly as UI-facing event rows so the user sees which replies ran under what.
Two facets, each with its own persisted comparison and its own event:

- the **effective model** (`conversations.last_model`, `ConversationEvent::ModelChanged`);
- the **chat memory size**, meaning `budget::chat_memory_figure` (`TurnParams.chat_memory_tokens`,
  `conversations.last_chat_memory`, `ConversationEvent::ChatMemoryChanged`): the prompt budget with the base reply slot,
  sized from the family table where it knows the model. It moves only when the user moved the setting or the model;
  an Effort pick (it reserves more reply) or a provider report arriving mid-thread moves the actual budget, never this
  line.

They are separate because a model switch usually moves the budget too, and "switched to X"
doesn't say what it costs the thread — the budget is the part the user can't see anywhere
else. A thread that changes both gets both lines, model first.

⚠️ **`last_chat_memory` is deliberately NOT `last_prompt_budget`**, which holds the same
number: that one is half of the gauge's pair (`conversation_context_usage`) and is written
only beside the `last_prompt_tokens` it must be read with, so stamping it from a live
settings change would leave the gauge reporting an old size against a new budget.

Two cooperating paths run both comparisons:

- **Send-time** (`record_model_transition` + `record_chat_memory_transition`, at the turn's
  FIRST `End`, before the user row): covers threads that weren't active when the setting
  changed (a resumed thread). Running at first `End` keeps crash case b intact — a failed
  first attempt records nothing, and the next successful turn re-runs both comparisons, so
  an event is deferred, never lost. A thread's first turn only stamps (nothing to change
  from).
- **Change-time** (`ChatRuntime::record_slot_change`, called by the
  `ask_cmdr_record_slot_change` command when a slot-affecting setting changes): awaits the
  thread's single-flight lock, so with a turn in flight the rows land right AFTER that reply
  (the turn keeps what it already resolved — a change never yanks a running request). It
  answers one `RecordedSlotEvent` per facet that moved, and `None` for a chat memory size
  that couldn't be resolved (a local window under the floor) leaves that stamp alone rather
  than clearing it. The two paths can't double-log: whichever runs first updates the stamp,
  and the other sees "unchanged" and no-ops.

An event's identity reaches the live rail via `AgentChatEvent::ModelChanged` /
`ChatMemoryChanged` (send-time) or the command's returned `MessageView`s (change-time);
history shows it via the `Event` role projection. Event rows never enter the LLM transcript
(`load_transcript` filters them) or the prompt prefix, and they carry numbers and names
rather than sentences, so the rail says them in the user's own language.

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

`MAX_TOOL_TURNS` and `MAX_WALL_TIME` are checked at the TOP of the loop, so a runaway is
impossible by construction. Wall time ends the turn there (`BudgetExhausted`). The round cap
instead makes ONE wrap-up call: the same assembly plus a trailing user message,
`context::tool_round_wrap_up_note()`, telling the model no further tool will run and to answer
from what it found, naming what it didn't get to. The note is appended after assembly, so the
cached prefix holds, and it is never persisted, so the thread's history stays what was said.
Its own bounding: the reply's tool calls are stripped before the row persists (no call without
a result row) and never run; prose alone answers the turn, and prose plus a tool call, or no
prose, ends it `BudgetExhausted`. Either way `TurnTally::limit` records a `TurnLimit` (kind,
the value that applied, `answered`), which rides `TurnStats` into the terminal event and the
`TurnFinished` row, so the rail names the limit live and after a reload
(`apps/desktop/src/lib/ask-cmdr/DETAILS.md` § Limit notice).

**Decision: the round cap asks for an answer instead of failing.** Why: every round's results
were already paid for, and failing at the cap threw them away unread, leaving the user a
"hit its limit" line over 14 lookups and no answer.

**Stop ends the turn wherever it is, so it always stops the spend**: at the top of the loop
(a clean stop between tool boundaries), while `respond` is still opening the call (raced against the token in
`turn.rs`: a provider slow to answer, a long queue, used to hold the turn until it did), while
the stream waits for its next chunk (`consume_stream` races each `next()`, whatever the
`AgentLlm` does with its own copy of the token: a model thinking before its first token used to
keep billing), and when a stream ends without `End` while the token is set (a user stop,
distinguished from a crash). All return `TurnResult::Cancelled`, whose one terminal event is
`Stopped` (never `Failed`); dropping the call or the stream drops the reqwest body (genai's
impl also ends its stream on the token). Regression anchor:
`stop_ends_a_model_call_that_never_answers`. A tool call meets Stop in `dispatch_until_stopped`
(§ Stop reaches every tool call); a `Propose` or `Memory` call finishes first. Turning Ask Cmdr
off trips every turn (`cancel::stop_all_unless_on`, from `ask_cmdr_enabled_changed`): the rail
then shows its gate where the composer was, so a running turn would have no Stop left to press.
Revoking cloud consent does the same (`ai::cloud_consent`).

The tokens live in `cancel.rs`, one process-global map keyed by conversation id, holding a LIST
of registrations per thread. ⚠️ **Not one token per thread**: a send registers BEFORE it waits
for the single-flight lock (`commands/agent/chat.rs`, so an immediate Stop finds it; a wake
registers before its lock too), so a send queued behind a running turn is registered while that
turn still is. One slot per thread let the queued send overwrite the running turn's token, which
made the running turn unstoppable and let its exit unregister the queued one. Each
`CancelRegistration` is its own entry (tagged by a generation), `cancel_turn` trips every entry
of the thread (a Stop during `Queued` stops both), and dropping a registration removes only its
own. `cancel_turn` answers a `CancelOutcome` (`tripped`: entries no earlier Stop had tripped;
`live`: entries still registered), which `ask_cmdr_cancel` hands the rail so it waits for
exactly the terminals still coming (`apps/desktop/src/lib/ask-cmdr/DETAILS.md` § Stop is local
first). The rail turn's registration lives on its worker thread under a `TerminalOnPanic` guard
(`commands/agent/chat.rs`), which emits `Failed { unfinishedReply }` if the turn unwinds, BEFORE
the registration drops, so a Stop that reached a panicking turn still hears its terminal. ❌ Don't fix it by registering under the lock: the Stop-before-the-thread-runs race comes
back. ⚠️ **It sits here rather
than in `commands/agent/chat.rs` because a WAKE registers in it too**: `agent/wake/runner.rs`
is below `commands/` and may not import upward, and a wake is a multi-second background turn
spending the user's money, which `docs/design-principles.md` requires be cancelable. One
registry means `ask_cmdr_cancel` stops either kind of turn without knowing which it is, and the
status corner's stop button is that same command with the id its event carries. The
registration is RAII: it clears its entry on every exit, so a turn that returned early can't
leave a token for the next turn on the same thread to be stopped by.

**The in-flight mark.** The same module records, per thread, the running turn's seq high-water
mark: the thread's last `seq` when the turn took the single-flight lock (`store::last_seq`, `-1`
for an empty thread). `ChatRuntime::send_message` and `wake` set it right after acquiring the lock
and drop it before releasing it, so at most one mark per thread exists and it is always the
running turn's. `turn_in_flight_since_seq(id)` reads it: rows after the mark belong to the
running turn, which is how a reload tells that turn's result-less calls (still running) from a
crashed turn's (settled, no answer recorded).

### Stop reaches every tool call

`dispatch.rs::dispatch_until_stopped` is the ONE place Stop meets a tool call, for every tool and
every dispatcher, test doubles included. A call that hasn't started when Stop lands is never
dispatched; one in flight is raced against the token and abandoned (its future dropped), answering
`{ miss: "cancelled", detail }`. Either way the call keeps its result row, so the next turn loads
a whole call/result pair (crash case c). The race polls the call FIRST, so a handler holding the
token (`list_dir`'s live read, `agent/tools/DETAILS.md` § The live source) answers with its own,
more specific `cancelled` in the same poll.

⚠️ **A `Propose` or `Memory` call is never abandoned mid-flight** (`stop_may_abandon`, exhaustive
over `Access`). Both commit on a blocking thread that dropping the future doesn't stop, so
abandoning would record `cancelled` beside a proposal the user can see, or a note the agent reads
next turn. They run to their own bounded end with their real answer; the loop-top check then ends
the turn.

## The event seam (`AgentChatEvent`)

The runtime emits typed progress through `ChatEventSink` (a
`tokio::sync::mpsc::UnboundedSender<AgentChatEvent>`). The `ask_cmdr_send_message` command
is a thin adapter:

1. `let (tx, mut rx) = unbounded_channel();` and `stream::forward_to_windows(conversation_id,
   &mut rx)`, which drains until the runtime drops its sender.
2. Capture the `ContextEnvelope` from live state (`PaneStateStore` + `snapshot_volumes`),
   resolve the interactive-slot `GenaiAgentLlm`, then call
   `ChatRuntime::send_message(app, &llm, provider, model, conversation_id, text, envelope,
   offset, tx, cancel)`.
3. `to_wire_event` maps `AgentChatEvent` → the wire `AskCmdrStreamEvent`: `AssistantStarted`
   carries no id (map to a bubble-start); a round's assistant id arrives on its
   `ToolCallsResolved`, the answer round's on `Done`. NEVER forward a reasoning blob or
   provider state — the events already exclude them.

### What the stream says about each tool line

A line's facts are typed (`agent/tools/line/`: `ToolSubject`, `ToolOutcome`, repeat marks) and
computed by the same pure functions the reload projection runs, so a live line and a reloaded
one agree. Per round, in order:

1. `ToolCallStarted { call_id, tool }` per call WHILE its arguments stream (so no subject yet).
   A nameless call gets one and is then dropped from the final message (`llm/DETAILS.md`).
2. The assistant row persists, then ONE `ToolCallsResolved { message_id, calls }`: every call
   of the final message, in order, each with its tool's wire name, its argument-only subject,
   and `repeat_of` (the turn's first identical call, by `AgentToolCall::identity_key`, the
   repeat breaker's key). It stamps the row id onto the round, adds a call whose start never
   came (the tool name is what gives that line its label), and lets the rail
   drop a started call the batch doesn't list.
3. Per call: dispatch, persist its result row, THEN `ToolCallFinished { message_id, call_id,
   outcome, subject }` (the subject refined by the result). ⚠️ **Persist, then emit**: the
   event is what tells the rail the result can be fetched, so it must never race the row;
   `line_events_tests.rs` pins it by failing the persist.

⚠️ **A line is `(message_id, call_id)`, never the call id alone.** Ids collide across rounds as
the norm: the fake mints `fake-call-{index}` per round (keep it that way; it is the regression
fixture) and Gemini resets them per response.

`memory_write` paths become a place to go only against the memory folder, which is app state,
so `ToolDispatcher::memory_root` hands it over (default `None`: a plain-text path). A wrapping
dispatcher (`wake/watch.rs`) must forward it.

`UserPersisted { opener }` carries the persisted opener as the `MessageView` history shows
(`message_view.rs`), so a rail that reloaded before the row landed can insert it.

### The turn's end

`drive` returns how it ended (`TurnEnd`: answered with id/seq/stop/usage, failed with kind +
detail, or cancelled) and emits no terminal event itself. `run_turn` then, once per turn:

1. builds `TurnStats` from the tally plus its own wall clock and whether the turn was cancelled
   (`stopped`), `None` when no tool ran, so a tool-less turn looks the same live and reloaded;
2. appends `ConversationEvent::TurnFinished` when there are stats, after the turn's last row
   (a persist problem is logged and dropped; `store/DETAILS.md` § What a turn spent);
3. emits exactly one of `Done`, `Failed`, or `Stopped`, each carrying `turn: Option<TurnStats>`.

⚠️ Stats ride the TERMINAL event, never a trailing one: the rail reads any non-terminal event
after a turn as a live turn. A `Failed` from outside a turn (the command layer's refusals and
spawn failures) carries `turn: None`. `Done.usage` stays the LAST call's usage.

### What a reloaded page says (`message_view/thread.rs`)

`ask_cmdr_get_conversation` projects a page through `project_page`, so a reloaded tool line says
what the live one did: the same `tools::line` functions over the stored arguments and results.

- **Turn-scoped window.** A line's result sits on a later row, a repeat points at an earlier
  round, and a turn's stats ride its last assistant row, while the history pages by rows. So the
  command also reads `store::turn_context`: from the opener of the page's first turn to the end
  of its last turn, ❌ never the whole thread. Only the page's own rows come back.
- **Pairing**: the k-th call with an id in an assistant row takes the k-th result with that id
  after it, before the next assistant or user row. Results persist in call order, and the bound
  keeps a later round's colliding id out.
- **Turn edges**: every view carries `turn_id` (the opener's row id; `None` before the first user
  row), every event view `during_turn` (before its turn's last assistant row). A slot event's
  `turn_id` comes from `store::turn_opener_at`, since it is projected alone.
- **One stats rule**: the turn's stats ride its LAST assistant view, from its `TurnFinished` row,
  else rebuilt from its rows (every result row a call, tokens summed per row, `wall_ms: None`).
  The turn holding rows after `turn_in_flight_since_seq` gets no rebuilt stats: it is still
  running, and the live terminal event brings them. The mark is read BEFORE the rows, so a turn
  ending in between reads as running for one load, never as settled with its last results
  missing.
- **Tool rows project to no blocks**: a call's outcome rides its own block. A lone row
  (`to_message_view`, the stream's opener) knows no turn, so its calls carry argument-only
  subjects and no outcome.
- **The raw result's one door** is `ask_cmdr_tool_result(conversation_id, message_id, call_id,
  occurrence)`, called on the user's expand of one line: `store::row_and_rest_of_turn` (the
  ownership check), then `tool_result_detail` pairs the way the page does and takes the
  `occurrence`-th call with that id in the row (a provider can name one id twice in a round), and
  answers `preview_of` plus the pretty-printed content for Copy. ❌ Never in the stream or a
  page, never logged (only a store failure is), and its `Err` is the fieldless
  `AskCmdrToolResultError { NotFound, StoreUnavailable }`.

### One transport, keyed by conversation (`stream.rs`)

Each wire event leaves as an `AskCmdrTurn { conversation_id, run, event }`, a `tauri_specta::Event`.
`run` is the turn's `cancel::TurnRun` (its registration's generation, distinct per turn for the
process's life), stamped by `forward_to_windows` and every direct emit of a rail send or wake;
`None` only for a `ProposalDecided`, which no turn owns. The send command answers it beside the
conversation id (`AskCmdrSendAccepted`). Both register BEFORE `Started`, so every event of a turn
carries it. Why: the rail correlates a terminal with the turn it belongs to, so a late terminal of
a turn it already ended can't end one sent since (`apps/desktop/src/lib/ask-cmdr/DETAILS.md` §
Stop is local first).

**Why not the per-invoke `Channel<T>` this used to be.** Three things it couldn't do, each now a
test:

- **Survive a reload.** A channel dies with the webview that handed it in, so the forwarder broke
  and the turn finished into `main.db` while the user watched a dead panel. A subscriber keyed on
  the thread just picks the stream back up, which is why the forwarder deliberately has no "the
  listener went away" exit.
- **Serve a wake.** A wake has no invoke to reply into. It forwards through the same function, so
  the two can't drift into streaming different things.
- **Stay typed.** `Channel<T>` isn't specta-friendly, so the command rode raw `invoke` and the
  wire enum never reached `bindings.ts`, leaving it hand-mirrored on the frontend with nothing
  catching drift.

Two variants map from no runtime event, because the runtime neither creates nor deletes threads:
`Started` (a thread exists and is being worked on — how the session list hears about a wake's) and
`Discarded` (a quiet wake deleted its thread; the one thing a subscriber can't recover from by
re-reading, since there is nothing to read).

A third, `ProposalDecided`, is not part of a turn at all. `agent/outcomes.rs` emits it as it writes
a decision's timeline row (and again, under the same row id, when an approval's "running" line is
rewritten with its outcome), usually with no turn running, and it rides here because this is the
transport keyed by conversation (`../suggested_ops/DETAILS.md` § What the user's answer teaches the
agent). ⚠️ A subscriber must not read it as proof a turn is live.

Unit tests have no app to emit into, so `emit_turn_event` also records each event in a test-only
thread-local, and `take_emitted_turns` is how a test asks what would have reached the windows.

A refusal decided BEFORE the turn exists is the command's `Err` (`AskCmdrSendRefusal`), not an
event: half of them happen before there is a conversation to key one on.

⚠️ The event reaches every window, and only the main window hosts a rail; the frontend scopes the
subscription there (`lib/ask-cmdr/DETAILS.md` § The streaming model).

The envelope's live sources (plan §9): focused pane path from `PaneStateStore` (it returns
the pane SIDE, so resolve that side's directory from the snapshot), cursor + selection from
pane state, per-volume freshness + SMB connectivity from `snapshot_volumes()`. Map those
live types into `context`'s pure `EnvelopeFreshness` / `EnvelopeConnectivity` mirrors.

## Testing notes

The context tests are four modules under `context/`, split by concern: `tests.rs` (the prefix,
the envelope, elision, the budget), `stub_tests.rs` (what a dropped result says, and that a plan
can't cite it), `cost_tests.rs` (what the real shapes cost), and `test_support.rs` (the
transcript builders and budgets they share). The runtime tests split the same way, over
`runtime/test_support.rs`: `tests.rs` (single-flight, the per-message budgets, cancellation,
the crash cases, cost, the typed error surface, and the attachment + send gates),
`repeat_tests.rs` (the repeat breaker), `line_events_tests.rs` (the tool-line events and the
turn's stats), `context_budget_tests.rs`, `slot_change_tests.rs`, and `wake_tests.rs`. Put a new test in the module whose concern it matches rather than growing
`tests.rs`.

Every `context.rs` test runs with no tokio runtime (the core is pure). The runtime tests
use a local `ProgrammableLlm` (per-turn text / tool calls / usage / a mid-stream drop with
no `End`) and scripted `ToolDispatcher` doubles — there is no in-tree full-Tauri harness for
the agent toolset at unit-test scope, so tool dispatch is exercised at the seam level.
Wall-time uses `tokio::time` under `start_paused`; a `SleepingDispatcher` advances virtual
time past the ceiling. The read→green evidence for the load-bearing invariants
(prefix stability, envelope-only-on-latest, elision, budget halt, crash-a persistence) was
captured by mutation before the code was completed.
