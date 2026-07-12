# Agent subsystem details

Pull-tier docs for `src-tauri/src/agent/`. Must-knows live in [CLAUDE.md](CLAUDE.md).

The agent is the app's AI agent (agent-spec: `docs/specs/later/agent-spec.md`). Its first shipped slice is **Ask Cmdr**:
a read-only chat rail where the user talks to a BYO-key LLM that can see what Cmdr already knows (the drive index,
importance, the operation log, live app state) and answers questions about their files. It deliberately ships ahead of
the agent's proactive machinery (wake loop, proposals, notifications) — the wow reaches beta users cheaply while the
risky proactivity bakes.

## Why "agent", not "ask-cmdr"

The persistent entity is "the agent" (agent-spec D44); "Ask Cmdr" is the user-facing name of this one read-only slice.
Naming the subsystem after the entity means the later proactive surfaces (proposals, notifications) grow inside `agent/`
rather than forcing a rename. `name-internals-after-the-UI` still applies to the surfaces (`ask-cmdr/` on the frontend).

## Milestone layout

Construction plan: [`docs/specs/ask-cmdr-plan.md`](../../../../../docs/specs/ask-cmdr-plan.md). The backend modules, in
build order:

- `llm/` (M1, present): the `AgentLlm` trait, its genai-backed impl over `crate::ai::AiBackend`, the deterministic fake,
  and the typed message-part model. This is the seam the whole runtime and UI test against. Depth:
  [`llm/DETAILS.md`](llm/DETAILS.md).
- `store/` (M2): the `main.db` durable store — a forward-migration ladder (mirroring `operation_log/store/`), FTS5 over
  message text, and a per-day cost meter. `agent::start(app)` (open the DB, register runtime state) lands here, modeled
  on `operation_log::start`.
- `tools/` (M4): the in-process read-only toolset — the agent's view of the consolidated tool registry (agent-spec D49,
  extend-don't-fork) plus the concrete tool handlers that call the shipped cores (drive index, importance, operation
  log, volumes, app state).
- `chat/` (M5): the chat runtime (single-flight per thread, per-message budgets, cancellation, typed errors) and the
  pure, TDD-heavy context-assembly core (stable prefix, elide-only compaction, the fresh context envelope on the latest
  user turn only).

## Read-only by construction

The v1 agent can only look and speak (spec §2.1): no write tool exists in its dispatch view, and there is no
content-read tool, so only names, paths, and metadata ever reach the provider — never file contents. This is the
privacy line and it is structural, not a runtime guard. The registry gains a `consumers` + `access` dimension in M3 so
the agent's view is pinned to exactly its `access: Read` entries; the runtime's `ToolId` parse step is the runtime choke
point (an unrecognized name resolves to `ToolId::Unrecognized`, which is never in the agent view, so dispatch refuses
it). Revisit the whole consent + gating story before adding the first write or content-read tool.

## Staged construction

M1's seam is built before its consumers (the runtime, M5; the IPC, M6). Until a non-test path wires the subsystem in,
its items are unreferenced from a release build, so `agent/mod.rs` carries a justified `#![allow(dead_code, reason=…)]`.
Remove it when M5 lands — leaving it would mask genuinely dead code in later milestones.
