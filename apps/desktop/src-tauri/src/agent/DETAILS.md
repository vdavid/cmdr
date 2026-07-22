# Agent subsystem details

Pull-tier docs for `src-tauri/src/agent/`. Must-knows live in `CLAUDE.md`.

The agent is the app's AI agent (agent-spec: `docs/specs/later/agent-spec.md`). Its first shipped slice is **Ask Cmdr**:
a read-only chat rail where the user talks to a BYO-key LLM that can see what Cmdr already knows (the drive index,
importance, the operation log, live app state) and answers questions about their files. It deliberately ships ahead of
the agent's proactive machinery (wake loop, proposals, notifications) — the wow reaches beta users cheaply while the
risky proactivity bakes.

## Why "agent", not "ask-cmdr"

The persistent entity is "the agent" (agent-spec D44); "Ask Cmdr" is the user-facing name of this one read-only slice.
Naming the subsystem after the entity means the later proactive surfaces (proposals, notifications) grow inside `agent/`
rather than forcing a rename. `name-internals-after-the-UI` still applies to the surfaces (`ask-cmdr/` on the frontend).

## Module layout

Construction plan: `docs/specs/ask-cmdr-plan.md`. The backend modules:

- `llm/`: the `AgentLlm` trait, its genai-backed impl over `crate::ai::AiBackend`, the deterministic fake,
  and the typed message-part model. This is the seam the whole runtime and UI test against. Depth:
  `llm/DETAILS.md`.
- `store/`: the `main.db` durable store — a forward-migration ladder (mirroring `operation_log/store/`),
  FTS5 over message text, and a per-day cost meter. `agent::start(app)` (open the DB, register the `AgentDb` handle)
  lands here, modeled on `operation_log::start`. Depth: `store/DETAILS.md`.
- `tools/`: the in-process toolset — the five read families authored as `consumers: [Agent]`
  entries in the consolidated registry (agent-spec D49, extend-don't-fork), their handlers/result shapes that reuse the
  shipped cores (drive index, importance, operation log, volumes, app state), and the gated dispatch that refuses any
  non-view name before `execute_tool`. Depth: `tools/DETAILS.md`.
- `chat/`: the chat runtime (single-flight per thread, per-message budgets, cancellation, typed errors,
  crash-safe persistence, the `AgentChatEvent` seam) and the pure, TDD-heavy context-assembly core (stable prefix,
  elide-only compaction, the fresh context envelope on the latest user turn only). Depth: `chat/DETAILS.md`.

## The agent can propose; only the user can approve

`RenameProposalStore` is managed with the agent runtime. It holds short-lived immutable rename proposals by opaque id;
the tool can stage one, but no agent path can approve or apply it.

**The invariant.** The agent can propose. Only the user can approve. Approval originates in the frontend as a user
action. There is no tool, and never will be a tool, that approves a proposal. Without that, `Propose` is `Write` with
extra steps.

The agent can look, speak, and ask (spec §2.1): no write tool exists in its dispatch view, and there is no content-read
tool, so only names, paths, and metadata ever reach the provider — never file contents. This is the privacy line and it
is structural, not a runtime guard. The registry's `consumers` + `access` dimensions pin the agent's view to exactly its
authored `[agent]` entries, every one `Access::Read` or `Access::Propose`, never `Access::Write`; the runtime's `ToolId`
parse step is the runtime choke point (an unrecognized name resolves to `ToolId::Unrecognized`, which is never in the
agent view, so dispatch refuses it). Revisit the whole consent + gating story before adding the first write or
content-read tool.

**What a `Propose` tool may do.** Stage a proposal and open a review surface. That is its entire power: no filesystem
write, no silent config mutation, no self-approval. Because no structural check can prove a handler doesn't mutate,
`Propose` tools are an explicit hand-authored allowlist (`EXPECTED_PROPOSE_TOOL_NAMES` in
`mcp/tests/tool_registry_tests.rs`) rather than something inferred — adding one is a deliberate act a human signs off,
having read the handler. It is empty today; that's the correct state until the first proposing feature ships.

**Consent is unaffected.** Proposals flow agent → user, never to the provider. `Propose` adds no egress, so the
provider-egress question and `CONSENT_COPY_VERSION` are unchanged by this tier. Don't re-litigate it: only a change to
what reaches the provider touches consent.

**Bounding is the tool's contract.** A `Propose` payload must be capped the way `image_facts` caps at 200 paths. A
proposal the user can't actually review is a proposal they can only rubber-stamp, which quietly dissolves the invariant
above. The cap can't be enforced generically (each tool's payload shape differs), so the first `Propose` tool has to
honour it explicitly and pin it with a test.
