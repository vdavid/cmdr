# Agent chat (`agent/chat/`)

The chat runtime and its pure context-assembly core: one user message in, an answer out,
crash-safe and within budget. Depth (one call end to end, the constants, the crash cases,
rationale, the numbers): `DETAILS.md`.

## Module map

- `context.rs`: the PURE core (values in, prompt out) — the stable prefix, elide-only
  compaction, budget enforcement. `assemble_prompt` is the entry; `context/digest.rs` is how a
  dropped result describes itself.
- `budget.rs`: budget resolution (family windows, the user's size, the local window, the
  batch-size hint) + the ONE token-size estimator the agent shares. Pure data + arithmetic.
- `system_prompt.rs`: the stable identity + rules string (part of the cached prefix).
- `runtime/`: the I/O-and-time half — `turn.rs` (`run_turn`, the driver), `mod.rs`
  (`ChatRuntime`, single-flight, re-exports it all), `events.rs` + `dispatch.rs` (the
  `AgentChatEvent` and `ToolDispatcher` seams), `cost.rs` (metering).

## Must-knows

- **The prefix must stay byte-identical across a thread's calls** (it buys prompt caching):
  `system` (system prompt + `CMDR.md`) and the tool declarations never vary.
- **The envelope rides the latest user turn only, snapshot-at-send** (tests pin both): one
  `ContextEnvelope` per send, reused on every `respond`, so the model's ground truth can't
  shift mid-turn.
- **Content is written only on `End`; the user row on the FIRST `End`.** The crash-safety
  contract (cases (a)–(d) in `DETAILS.md`, red-guarded). Don't pre-persist an assistant
  row or eagerly persist the user row.
- **The pure core is genuinely pure — keep it that way.** Offset, envelope, `CMDR.md`, and
  budget all arrive as values, and every context test runs with no tokio runtime. Don't reach
  for `Utc::now()` or the filesystem in `context.rs`.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`
  floors every threshold). Handed a stub instead of the content it was told to name files by, a
  model invents — that shipped, and renamed 12 real files to fiction. It overruns honestly
  instead; don't drop the floor to "make it fit".
- **A dropped result says what it held and how to re-read it**: `tool` / `approx_tokens` /
  `call` / `held` / `refetch`, derived structurally in `context/digest.rs` within 80 tokens.
  Keep it shape-agnostic (a per-tool `digest()` arrives as a value, never a match arm in the
  core) and keep result STRINGS out (lengths and counts only): a digest describes a delivery
  and is never one, so no plan may cite it (invariant 6).
- **A context drop is never silent, and it revokes the dropped result's evidence.**
  `assemble_prompt` returns `ElisionFacts` as DATA (the core can't log), naming every dropped
  `call_id`; `runtime/turn.rs` warns, emits ONE `ContextTrimmed` event per turn, and calls
  `ToolDispatcher::revoke_evidence` for those ids — a result the model never read must not back
  a rename claim. New compaction path ⇒ all three.
- **The prompt budget is the user's setting, then the model's window**, resolved once per send
  in the command layer and passed as `TurnParams::prompt_budget`; `context.rs` never learns the
  model. `budget.rs` holds the sources, the 16,384 local floor, and why the tool-result ceiling
  must NOT follow this number.
- **A runaway loop is impossible by construction.** `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are
  checked at the TOP of the loop, so the next `respond` never fires once one is spent; outcome
  `BudgetExhausted`. Constants are "tune with use" — never silently bump one.
- **Never block the main thread.** `run_turn` is async; the real `ToolDispatcher` routes
  through `agent::tools::view::dispatch` (the read-only choke point), cache/SQLite only.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**; `ask_cmdr_send_message` is a
  thin adapter onto a `Channel`. No reasoning blob or provider state ever rides an event.
  `AssistantStarted` carries no id (no row until `End`); it arrives on `Done`.
