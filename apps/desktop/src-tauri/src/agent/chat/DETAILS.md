# Agent chat details

Pull-tier docs for `agent/chat/`. Must-knows live in `CLAUDE.md`.

## Anatomy of one call (spec §5, as built)

`run_turn` assembles and sends, top to bottom:

1. **System** (`system` arg): `system_prompt::SYSTEM_PROMPT` + `~/.cmdr/CMDR.md` under a
   header if present. Stable, cached.
2. **Tool declarations** (`tools` arg): `agent::tools::agent_tool_declarations()`. Stable,
   cached.
3. **History**: every persisted turn, each user turn prefixed with its own local
   timestamp marker (`[Fri 2026-07-11 09:15]`). Assistant prose survives verbatim; tool
   results `ELIDE_TOOL_RESULTS_AFTER_TURNS` or more turns back collapse to a typed stub
   (`{ elided_tool_result: true, tool, approx_tokens }`). Eliding prose is never done —
   that's the soft-cap's job. The current turn's results never elide either (see
   § Budget enforcement).
4. **The envelope** opens the LATEST user turn only, as a tagged block (§9 field set):
   `[Sat 2026-07-12 21:30 · focused: <path> · cursor: <name|—> · <n> selected · volumes: <name> (<freshness>[, <connectivity>]), …]`.
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
`estimated_tokens`, `budget`) crosses back as data, keeping the core pure. `runtime.rs`'s
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
- `LARGE_CONTEXT_PROMPT_BUDGET = 60_000` — a known ≥128k-window cloud family (`claude-`, `gpt-4o`/`gpt-4.1`/`gpt-5`,
  `o3`/`o4-mini`, `gemini-2`). Far below the window on purpose: a prompt this size costs real money per call and dilutes
  attention, while still holding a 200-row listing plus a full `image_facts` batch.
- Local: `LOCAL_PROMPT_BUDGET_PERCENT = 60` of the server's configured window (`ai.localContextSize`, default 4096),
  floored at `MIN_LOCAL_PROMPT_BUDGET = 2_000` — the reply comes out of the same window.
- `MAX_TOOL_RESULT_TOKENS = DEFAULT_PROMPT_TOKEN_BUDGET / 2` — the most ONE tool result may spend. Derived from the
  conservative default, not the resolved model, because a tool handler doesn't know the model (and may be answering an
  external MCP client). Enforced via `mcp::executor::fit_to_result_budget`.

Windows and prices both drift: re-verify the families at release time, like `agent::pricing`.
Bumping any constant is a conscious change (never a silent side effect).

## The runtime (`runtime.rs`)

`run_turn` is the driver and holds all the testable logic (no Tauri app needed): it takes
the `AgentLlm`, a `ToolDispatcher`, a write `Connection`, the tools, the `TurnParams`, an
event sink, and a cancel token. `ChatRuntime::send_message` is the thin Tauri-bound wrapper
that `ask_cmdr_send_message` calls: it opens a write connection, lazily creates the conversation, acquires the
per-thread single-flight lock (emitting `Queued` if contended), reads `CMDR.md`, builds the
`AppHandleDispatcher`, and calls `run_turn`. It is registered in managed state by
`agent::start`, so the IPC command is a pass-through.

### Crash / persistence model (plan §M5 (a)–(d))

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

`ProposalReady` is a display-only stream event. The runtime emits it only after the proposal dispatcher staged
immutable rows in `RenameProposalStore`; chat history persists the concise tool result, not proposal authority.

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
the frontend branches on `kind`, never on `detail` (`no-string-matching`), and the string
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

Every `context.rs` test runs with no tokio runtime (the core is pure). The runtime tests
use a local `ProgrammableLlm` (per-turn text / tool calls / usage / a mid-stream drop with
no `End`) and scripted `ToolDispatcher` doubles — there is no in-tree full-Tauri harness for
the agent toolset at unit-test scope, so tool dispatch is exercised at the seam level.
Wall-time uses `tokio::time` under `start_paused`; a `SleepingDispatcher` advances virtual
time past the ceiling. The read→green evidence for the load-bearing invariants
(prefix stability, envelope-only-on-latest, elision, budget halt, crash-a persistence) was
captured by mutation before the code was completed.
