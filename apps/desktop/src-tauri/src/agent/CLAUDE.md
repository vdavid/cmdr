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
- **The runtime drives the seams, and M6 wired the IPC.** M5's `chat::runtime` consumes the M1 seam, the M2 store
  queries, and the M4 tool dispatch, and `agent::start` registers `ChatRuntime` in state. M6's
  [`commands/agent.rs`](../commands/agent.rs) is the thin frontend surface: `ask_cmdr_send_message` (streaming over a
  Tauri `Channel`, driven on a worker thread because `run_turn` holds a non-`Send` connection across awaits),
  `ask_cmdr_cancel`, `ask_cmdr_get_conversation`, `ask_cmdr_list_conversations`. The interactive LLM is resolved from the
  existing `ai/` config as an interim (M8 adds the dedicated slot). Frontend: [`src/lib/ask-cmdr/`](../../../src/lib/ask-cmdr/CLAUDE.md).

Depth (milestone layout, the read-only rationale, how the slice relates to the full agent): [DETAILS.md](DETAILS.md).
