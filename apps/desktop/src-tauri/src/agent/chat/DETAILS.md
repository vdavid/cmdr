# Agent chat details

Pull-tier docs for `agent/chat/`. Must-knows live in [CLAUDE.md](CLAUDE.md). Plan:
[`docs/specs/ask-cmdr-plan.md`](../../../../../../docs/specs/ask-cmdr-plan.md) § M5, §9
(envelope), §10 (constants). Spec: [`ask-cmdr-spec.md`](../../../../../../docs/specs/ask-cmdr-spec.md)
§5 (anatomy of one call), §2 (principles).

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
   that's the soft-cap's job.
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

`assemble_prompt(prefix, transcript, envelope, offset) -> AssembledPrompt`. No clock, no
I/O: the local UTC `offset`, the `ContextEnvelope`, and `CMDR.md` are values passed in.
The runtime captures them; the core only formats. Timestamps render through `offset` (a
single offset for the whole assembly — a DST boundary mid-thread is a hint-level
imprecision, acceptable for v1). Token sizes are a `chars / CHARS_PER_TOKEN_ESTIMATE`
heuristic, not a real tokenizer — enough to keep assembly in the budget band and to size
the elision stub's hint.

Budget enforcement is elide-only: `assemble_prompt` tightens the elision threshold turn by
turn until the estimate fits `CONTEXT_TOKEN_BUDGET`, never touching prose. When even full
elision can't fit (prose alone is over budget), it returns the best it can and the runtime
shows the soft-cap nudge — summarize-on-overflow is deferred (spec §3).

## Constants table (§10; initial values, tune with use)

- `MAX_TOOL_TURNS = 8` — per message; the loop stops before the 9th tool respond fires.
- `MAX_WALL_TIME = 60s` — per message wall-clock ceiling across the whole loop.
- `CONTEXT_TOKEN_BUDGET = 8_000` — target assembled-prompt size (spec's 6–10k band).
- `ELIDE_TOOL_RESULTS_AFTER_TURNS = 3` — tool results this many turns back (or more) elide.
- `THREAD_SOFT_CAP_MESSAGES = 40` — past this the UI nudges "start a fresh one?".
- `CHARS_PER_TOKEN_ESTIMATE = 4` — the size-estimate divisor (also sizes the stub hint).

They live at their definition sites in `context.rs`, each commented "initial value; tune
with use". Bumping any is a conscious change (never a silent side effect).

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
