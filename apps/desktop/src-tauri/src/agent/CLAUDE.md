# Agent subsystem

The in-app AI agent. Its first user-facing slice is **Ask Cmdr**, a read-only chat rail
([`docs/specs/ask-cmdr-spec.md`](../../../../../docs/specs/ask-cmdr-spec.md); plan:
[`docs/specs/ask-cmdr-plan.md`](../../../../../docs/specs/ask-cmdr-plan.md)). Named after the persistent entity, not the
surface, so later proactive slices (proposals, notifications) grow here too. Full map + milestone layout:
[DETAILS.md](DETAILS.md).

## Module map

- `llm/` (M1, here now): the `AgentLlm` seam — the provider-agnostic trait, its genai-backed impl, the deterministic
  fake, and the typed message-part model. See [`llm/CLAUDE.md`](llm/CLAUDE.md).
- `store/` (M2, here now): the `main.db` durable store — the migration ladder, conversations, messages, FTS5 search, and
  the cost meter. `start(app)` (open the DB, register `AgentDb`) lands here. See [`store/CLAUDE.md`](store/CLAUDE.md).
- `types.rs` (M2): store-only token enums (`ConversationOrigin`) + the `token_enum!` macro.
- `tools/` (M4, here now): the in-process read-only toolset — the five read families as `consumers: [Agent]` registry
  entries, their handlers/result shapes, and the gated dispatch (the read-only choke point). See
  [`tools/CLAUDE.md`](tools/CLAUDE.md).
- `chat/` (M5, here now): the chat runtime (`run_turn` + `ChatRuntime`, single-flight, budgets, cancellation, crash-safe
  persistence, the `AgentChatEvent` seam) and the pure context-assembly core. See [`chat/CLAUDE.md`](chat/CLAUDE.md).

## Must-knows

- **Read-only by construction.** The chat agent has NO write tool and no content-read tool — only names, paths, and
  metadata ever reach the provider (spec §2.1). This is a structural privacy line, not a runtime check; don't add a tool
  that breaks it without revisiting the whole consent + gating story.
- **The runtime drives the seams; IPC is still pending.** M5's `chat::runtime` now consumes the M1 seam, the M2 store
  queries, and the M4 tool dispatch, and `agent::start` registers `ChatRuntime` in state — so the module-level
  `#![allow(dead_code)]` is gone. The IPC commands that reach the runtime from the frontend are M6; `ChatRuntime` is
  reachable public API in the meantime.

Depth (milestone layout, the read-only rationale, how the slice relates to the full agent): [DETAILS.md](DETAILS.md).
