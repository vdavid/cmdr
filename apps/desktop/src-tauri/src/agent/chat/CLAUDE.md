# Agent chat (`agent/chat/`)

The chat runtime and its pure context-assembly core: one user message in, an answer out,
crash-safe and within budget. Depth (anatomy-of-one-call, the constants table, the crash
cases, decision rationale): `DETAILS.md`.

## Module map

- `context.rs`: the PURE core — values in, prompt out, no I/O and no clock. The stable
  prefix, elide-only compaction, the envelope, budget enforcement. `assemble_prompt` is
  the entry.
- `budget.rs`: the per-model prompt-budget table + the ONE token-size estimator the agent
  shares (elision, the stub hint, every tool's self-cap). Pure data + arithmetic.
- `system_prompt.rs`: the stable identity + rules string (part of the cached prefix).
- `runtime.rs`: the I/O-and-time half — `run_turn` (the driver), `ChatRuntime`
  (single-flight wrapper, registered in state by `agent::start`), the `AgentChatEvent`
  seam, and the `ToolDispatcher` seam.

## Must-knows

- **The prefix must stay byte-identical across a thread's calls** (that's what buys provider
  prompt caching): `system` (system prompt + `CMDR.md`) and the tool declarations never vary.
  The **envelope lives on the latest user turn only**, never in the prefix (a test pins it).
- **The envelope is snapshot-at-send.** ONE `ContextEnvelope` captured at send, reused on
  every `respond` of that turn, so the model's ground truth can't shift mid-turn.
- **Content is written only on `End`; the user row on the FIRST `End`.** The crash-safety
  contract (spec §2.3, cases (a)–(d) in `DETAILS.md`, each red-guarded). Don't pre-persist an
  assistant row or eagerly persist the user row.
- **The pure core is genuinely pure — keep it that way.** `context.rs` reads no clock and no
  files (offset, envelope, `CMDR.md`, budget all come in as values), and every context test
  runs with no tokio runtime. Don't reach for `Utc::now()` or the filesystem there.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`
  floors every elision threshold). A model told to name files by their content, handed a stub
  instead of the content, invents — that shipped and renamed 12 real files to fiction. The
  prompt overruns its budget instead, honestly. Don't drop the floor to "make it fit".
- **A context drop is never silent.** `assemble_prompt` returns `ElisionFacts` as DATA (the
  core can't log); `runtime.rs`'s `announce_context_pressure` warns and emits ONE
  `ContextTrimmed` event per turn for the rail. New compaction path ⇒ report it the same way.
- **The prompt budget is per-model, resolved in the command layer** (`budget::prompt_budget`,
  or `prompt_budget_for_local_context` for a local server's window) and passed in through
  `TurnParams::prompt_budget`. `context.rs` has no business knowing the model.
- **A runaway loop is impossible by construction.** `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are
  checked at the TOP of the loop, so the next `respond` never fires once a budget is spent;
  the typed outcome is `BudgetExhausted`. Every constant is "tune with use" — never silently
  bump one.
- **Never block the main thread.** `run_turn` is async; the real `ToolDispatcher` routes
  through `agent::tools::view::dispatch` (the read-only choke point) and reads cache/SQLite
  only.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**; `ask_cmdr_send_message` is
  a thin adapter onto a `Channel`. No reasoning blob or provider state ever rides an event.
  `AssistantStarted` carries no id by design (no row until `End`); the id arrives on `Done`.

Depth: `DETAILS.md`.
