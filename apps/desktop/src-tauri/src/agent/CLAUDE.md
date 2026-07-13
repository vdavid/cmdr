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
- `pricing.rs` (M8): the provisional per-model price table (USD per million tokens, Tier-1 defaults). Local ⇒ free +
  priced; a known cloud model ⇒ estimated + priced; an unknown cloud model ⇒ `priced = false` (cost shown "unknown",
  never a silent $0). **Prices drift** — re-verify at release. The runtime's `meter_cost` calls `price_call`.

## Must-knows

- **Read-only by construction.** The chat agent has NO write tool and no content-read tool — only names, paths, and
  metadata ever reach the provider (spec §2.1). This is a structural privacy line, not a runtime check; don't add a tool
  that breaks it without revisiting the whole consent + gating story.
- **The runtime drives the seams, and the IPC is wired.** M5's `chat::runtime` consumes the M1 seam, the M2 store
  queries, and the M4 tool dispatch, and `agent::start` registers `ChatRuntime` in state.
  [`commands/agent.rs`](../commands/agent.rs) is the thin frontend surface: `ask_cmdr_send_message` (streaming over a
  Tauri `Channel`, driven on a worker thread because `run_turn` holds a non-`Send` connection across awaits; takes
  `attachments: Vec<AttachmentRef>` folded into the envelope as path + kind only), `ask_cmdr_cancel`,
  `ask_cmdr_get_conversation`, `ask_cmdr_list_conversations`, plus M7's `ask_cmdr_search_conversations` (FTS hits with a
  snippet), `ask_cmdr_rename_conversation`, `ask_cmdr_archive_conversation`, and the attachment resolvers
  `ask_cmdr_selection_attachments` / `ask_cmdr_resolve_attachments` (kinds from `PaneStateStore`, no filesystem stat),
  plus M8's consent + cost commands: `ask_cmdr_consent_status` / `ask_cmdr_accept_consent` / `ask_cmdr_revoke_consent`
  and `ask_cmdr_conversation_cost` / `ask_cmdr_cost_summary`. Register a new command in BOTH `ipc.rs` and
  `ipc_collectors.rs`. Frontend: [`src/lib/ask-cmdr/`](../../../src/lib/ask-cmdr/CLAUDE.md).
- **The interactive slot (M8) layers a dedicated model over the shared `ai/` config.** `resolve_agent_llm` reads
  `askCmdr.interactiveModel` fresh (via `crate::settings::load_ask_cmdr_interactive_model`) and passes it to
  `ai::manager::resolve_backend_with_model`: provider on/off, keys, and base URLs stay single-sourced in `ai/` (D49); only
  the model is slot-specific, so the bulk slot slots in later with no migration (D43). An empty override uses the `ai/`
  model.
- **Consent is the opt-in gate, stored in `main.db` (not a preference).** `CONSENT_COPY_VERSION` in `commands/agent.rs`
  is the machine-checkable version of the `askCmdr.consent.*` copy; the rail sends NOTHING to a provider until the user
  accepts the current version. **Bump `CONSENT_COPY_VERSION` whenever the consent copy changes materially**, so users
  re-accept. The record (version + timestamp) lives in the `meta` table via `store::{get,set,clear}_consent`.

Depth (milestone layout, the read-only rationale, how the slice relates to the full agent): [DETAILS.md](DETAILS.md).
