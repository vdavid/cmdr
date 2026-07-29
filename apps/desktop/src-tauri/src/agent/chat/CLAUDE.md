# Agent chat (`agent/chat/`)

The chat runtime and its pure context-assembly core: one user message in, an answer out,
crash-safe and within budget. Depth (anatomy-of-one-call, the constants, the crash cases,
rationale): `DETAILS.md`.

## Module map

- `context.rs`: the PURE core — values in, prompt out, no I/O and no clock. The stable
  prefix, elide-only compaction, the envelope, budget enforcement. `assemble_prompt` is
  the entry; `context/digest.rs` is how a dropped result describes itself.
- `budget.rs`: the per-model prompt-budget table + the ONE token-size estimator the agent
  shares (elision, the stub hint, every tool's self-cap). Pure data + arithmetic.
- `system_prompt.rs`: the stable identity + rules string (part of the cached prefix).
- `runtime/`: the I/O-and-time half — `turn.rs` (`run_turn`, the driver), `mod.rs`
  (`ChatRuntime`, the single-flight wrapper), `events.rs` (the `AgentChatEvent` seam),
  `dispatch.rs` (the `ToolDispatcher` seam), `cost.rs` (per-`respond` metering). `mod.rs`
  re-exports it all.

## Must-knows

- **The prefix must stay byte-identical across a thread's calls** (that's what buys provider
  prompt caching): `system` (system prompt + `CMDR.md`) and the tool declarations never vary.
  The **envelope lives on the latest user turn only**, never in the prefix (a test pins it).
- **The envelope is snapshot-at-send.** ONE `ContextEnvelope` captured at send, reused on
  every `respond` of that turn, so the model's ground truth can't shift mid-turn.
- **Content is written only on `End`; the user row on the FIRST `End`.** The crash-safety
  contract (cases (a)–(d) in `DETAILS.md`, each red-guarded). Don't pre-persist an
  assistant row or eagerly persist the user row.
- **The pure core is genuinely pure — keep it that way.** `context.rs` reads no clock and no
  files (offset, envelope, `CMDR.md`, budget all arrive as values), and every context test runs
  with no tokio runtime. Don't reach for `Utc::now()` or the filesystem there.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`
  floors every threshold). Handed a stub instead of the content it was told to name files by, a
  model invents — that shipped, and renamed 12 real files to fiction. The prompt overruns
  honestly instead. Don't drop the floor to "make it fit".
- **A dropped result says what it held and how to re-read it**: `tool` / `approx_tokens` /
  `call` / `held` / `refetch`, derived structurally in `context/digest.rs` within an 80-token
  budget. Keep it shape-agnostic (a per-tool `digest()` arrives as a value, never a match arm in
  the core), and keep result STRINGS out (lengths and counts only): a digest describes a
  delivery and is never one, so no plan may cite it (invariant 6).
- **A context drop is never silent, and it revokes the dropped result's evidence.**
  `assemble_prompt` returns `ElisionFacts` as DATA (the core can't log), naming every dropped
  `call_id`; `runtime/turn.rs` warns, emits ONE `ContextTrimmed` event per turn, and calls
  `ToolDispatcher::revoke_evidence` for those ids — a result the model never read must not
  back a rename claim. New compaction path ⇒ do all three.
- **The prompt budget is per-model, resolved in the command layer** and passed via
  `TurnParams::prompt_budget`. `context.rs` has no business knowing the model.
- **A runaway loop is impossible by construction.** `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are
  checked at the TOP of the loop, so the next `respond` never fires once a budget is spent;
  outcome `BudgetExhausted`. Constants are "tune with use" — never silently bump one.
- **Never block the main thread.** `run_turn` is async; the real `ToolDispatcher` routes
  through `agent::tools::view::dispatch` (the read-only choke point), cache/SQLite only.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**; `ask_cmdr_send_message` is a
  thin adapter onto a `Channel`. No reasoning blob or provider state ever rides an event.
  `AssistantStarted` carries no id (no row until `End`); it arrives on `Done`.
