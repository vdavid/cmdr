# Agent chat (`agent/chat/`)

The chat runtime and its pure context-assembly core: one message in, an answer out, crash-safe
and within budget. Depth (one call end to end, the constants, the crash cases, the numbers):
`DETAILS.md`.

## Module map

- `context.rs`: the PURE core (values in, prompt out) — the stable prefix, elide-only compaction,
  budget enforcement. `assemble_prompt` is the entry; `context/digest.rs` is how a dropped result
  describes itself.
- `budget.rs`: budget resolution (family windows, the user's size, the local window, the
  batch-size hint, a wake's digest share) + the ONE token estimator the agent shares. Pure.
- `system_prompt.rs`: the identity + rules string (part of the cached prefix).
- `runtime/`: the I/O-and-time half — `turn.rs` (`run_turn`, the driver), `mod.rs`
  (`ChatRuntime`: `send_message` and `wake`, both single-flight; re-exports it all), `events.rs`
  + `dispatch.rs` (the `AgentChatEvent` and `ToolDispatcher` seams), `cost.rs` (metering).
- `session.rs`: what a turn needs from live app state — the LLM slot, the prompt budget, the
  envelope. Shared by the rail's command and by a wake.

## Must-knows

- **The prefix stays byte-identical across a thread's calls** (it buys prompt caching): `system`
  (system prompt + `CMDR.md`) and the tool declarations never vary.
- **The envelope rides the latest user turn only, snapshot-at-send** (tests pin both): one
  `ContextEnvelope` per turn, reused on every `respond`, so the model's ground truth can't shift
  mid-turn.
- **Content is written only on `End`; the user row on the FIRST `End`.** The crash-safety
  contract (cases (a)–(d) in `DETAILS.md`, red-guarded). Don't pre-persist an assistant row or
  eagerly persist the user row.
- **The pure core is genuinely pure — keep it that way.** Offset, envelope, `CMDR.md`, and budget
  all arrive as values, and every context test runs with no tokio runtime. Don't reach for
  `Utc::now()` or the filesystem in `context.rs`.
- **Budget pressure NEVER touches the current turn's tool results** (`MIN_ELISION_TURNS_BACK`).
  Handed a stub instead of the content it was told to name files by, a model invents — that
  shipped, and renamed 12 real files to fiction. It overruns honestly instead.
- **A dropped result says what it held and how to re-read it**, structurally and within 80
  tokens (`context/digest.rs`; the fields are in `DETAILS.md`). Keep it shape-agnostic and keep
  result STRINGS out: a digest describes a delivery and is never one (invariant 6).
- **A context drop is never silent, and it revokes the dropped result's evidence.**
  `assemble_prompt` returns `ElisionFacts` as DATA (the core can't log), naming every dropped
  `call_id`; `runtime/turn.rs` warns, emits ONE `ContextTrimmed` per turn, and calls
  `revoke_evidence` — a result the model never read must not back a rename claim. New compaction
  path ⇒ all three.
- **The prompt budget is the user's setting, then the model's window**, resolved fresh per turn
  in `session.rs` (a wake reading a stale one would think with a different window than the rail,
  silently) and passed as `TurnParams::prompt_budget`; `context.rs` never learns the model.
  `budget.rs` holds the sources, the 16,384 local floor, and why the tool-result ceiling must NOT
  follow it.
- **A runaway loop is impossible by construction.** `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are checked
  at the TOP of the loop, so the next `respond` never fires once one is spent. Constants are "tune
  with use" — never silently bump one.
- **Never block the main thread.** `run_turn` is async; the real `ToolDispatcher` routes through
  `agent::tools::view::dispatch` (the read-only choke point), cache/SQLite only.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`**; `ask_cmdr_send_message` is a
  thin adapter onto a `Channel`, and a wake drains its own. No reasoning blob or provider state
  ever rides an event. `AssistantStarted` carries no id (no row until `End`); it arrives on `Done`.
