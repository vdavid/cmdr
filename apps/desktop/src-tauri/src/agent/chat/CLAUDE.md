# Agent chat (`agent/chat/`)

The chat runtime and its pure context-assembly core: one user message in, an answer out,
crash-safe and within budget. Depth (anatomy-of-one-call, the constants table, the crash
cases, decision rationale): `DETAILS.md`.

## Module map

- `context.rs`: the PURE core — values in, prompt out, no I/O and no clock. The stable
  prefix, elide-only compaction, the envelope, budget enforcement. `assemble_prompt` is
  the entry.
- `system_prompt.rs`: the stable identity + rules string (part of the cached prefix).
- `runtime.rs`: the I/O-and-time half — `run_turn` (the driver), `ChatRuntime`
  (single-flight wrapper, registered in state by `agent::start`), the `AgentChatEvent`
  seam, and the `ToolDispatcher` seam.

## Must-knows

- **The prefix must stay byte-identical across a thread's calls** (that's what buys
  provider prompt caching). `system` (system prompt + `CMDR.md`) and the tool
  declarations never vary within or across calls. The **envelope lives on the latest user
  turn only** — never in the prefix. A test pins that changing the envelope does not touch
  `system`/`tools`; don't move the envelope, `CMDR.md`, or anything per-call into the
  prefix.
- **The envelope is snapshot-at-send.** `ChatRuntime` captures ONE `ContextEnvelope` at
  message-send and passes the same value on every `respond` call of that turn's loop, so
  the model's ground truth can't shift mid-turn. The next user message captures a fresh
  one.
- **Content is written only on `End`; persist the user row on the FIRST `End`.** This is
  the crash-safety contract (spec §2.3): (a) partial assistant text before a non-`End`
  termination is discarded — no assistant row — with a typed `UnfinishedReply` notice; (b)
  a first `respond` that never reached `End` records NOTHING, so a re-send re-assembles
  byte-identically; (c) completed turns stay persisted, and a retry resumes with
  `run_turn(..., user_text: None)` — a fresh `respond` from the persisted transcript, NOT a
  re-send; (d) cost is metered per completed `End`. Don't pre-persist an assistant row or
  eagerly persist the user row — both break these cases (each has a red-guarded test).
- **The pure core is genuinely pure — keep it that way.** `context.rs` reads no clock and
  no files; the timestamp offset, the envelope, and `CMDR.md` are passed in as values.
  Every context test runs with no tokio runtime. Don't reach for `Utc::now()` or the
  filesystem inside `context.rs`.
- **A runaway loop is impossible by construction.** `MAX_TOOL_TURNS` / `MAX_WALL_TIME` are
  checked at the top of the loop, so the next `respond` never fires once a budget is spent;
  the typed outcome is `BudgetExhausted`. The §10 constants are "initial value; tune with
  use" — never silently bump them.
- **Never block the main thread.** `run_turn` is async; the real `ToolDispatcher` routes
  through `agent::tools::view::dispatch` (the read-only choke point) and reads
  cache/SQLite only. `ChatRuntime::send_message` runs on the caller's tokio task.
- **The event seam is `AgentChatEvent` over an `UnboundedSender`.** The `ask_cmdr_send_message` Tauri command is a
  thin adapter (forward each event onto a `Channel`, map to the wire enum). No reasoning
  blob or provider state ever rides an event. `AssistantStarted` carries no id by design
  (no row exists until `End`); the persisted id arrives on `Done`.

Depth: `DETAILS.md`.
