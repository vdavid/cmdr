# Agent subsystem

The in-app AI agent. First user-facing slice is **Ask Cmdr**, a read-only chat rail (spec:
[`ask-cmdr-spec.md`](../../../../../docs/specs/ask-cmdr-spec.md), plan:
[`ask-cmdr-plan.md`](../../../../../docs/specs/ask-cmdr-plan.md)). Named after the persistent entity, not the surface,
so later proactive slices (proposals, notifications) grow here too.

## Module map

- `llm/`: the `AgentLlm` seam (provider-agnostic trait, genai impl, deterministic fake, typed message-part model). See
  [`llm/CLAUDE.md`](llm/CLAUDE.md).
- `store/`: the `main.db` durable store (migration ladder, conversations, messages, FTS5 search, cost meter).
  `start(app)` opens the DB + registers `AgentDb`. See [`store/CLAUDE.md`](store/CLAUDE.md).
- `types.rs`: store-only token enums (`ConversationOrigin`) + `token_enum!` macro.
- `tools/`: the in-process read-only toolset, five read families as `consumers: [Agent]` registry entries, plus gated
  dispatch (the read-only choke point). See [`tools/CLAUDE.md`](tools/CLAUDE.md).
- `chat/`: the chat runtime (`run_turn` + `ChatRuntime`: single-flight, budgets, cancellation, crash-safe persistence,
  the `AgentChatEvent` seam) + pure context assembly. See [`chat/CLAUDE.md`](chat/CLAUDE.md).
- `consent.rs`: the consent gate (`CONSENT_COPY_VERSION` + `has_current_consent`, fails closed).
- `pricing.rs`: provisional per-model price table (USD/M tokens). Local ⇒ free+priced; known cloud ⇒ estimated+priced;
  unknown cloud ⇒ `priced = false` (shown "unknown", never a silent $0). **Prices drift**: re-verify at release.

## Must-knows

- **Read-only by construction.** No write tool, no arbitrary file-content-read tool. Names, paths, and metadata reach
  the provider (spec §2.1); the ONLY derived-content egress is the photo pair, `search_photos` (in-image OCR snippets +
  Vision tags) and `image_facts` (the FULL stored OCR text + tags for paths the caller names). Image-derived TEXT,
  never image bytes; see `mcp/executor/photos.rs` and `image_facts.rs`. That egress is named in the consent copy
  (`askCmdr.consent.*`), so bump `CONSENT_COPY_VERSION` if what it can send changes. This is a structural privacy line;
  don't add a tool that widens it without revisiting the whole consent + gating story.
- **The runtime drives the seams; the IPC is wired.** `chat::runtime` consumes the `AgentLlm` seam, store queries, and
  tool dispatch; `agent::start` registers `ChatRuntime`. [`commands/agent.rs`](../commands/agent.rs) is the thin
  frontend surface (send/cancel, conversation CRUD + FTS, attachment resolvers, consent + cost commands; full list in
  DETAILS.md). `ask_cmdr_send_message` streams over a Tauri `Channel` on a worker thread (`run_turn` holds a non-`Send`
  connection across awaits). Register a new command in BOTH `ipc.rs` and `ipc_collectors.rs`. Frontend:
  [`src/lib/ask-cmdr/`](../../../src/lib/ask-cmdr/CLAUDE.md).
- **The interactive slot layers a dedicated model over shared `ai/` config.** `resolve_agent_llm` reads
  `askCmdr.interactiveModel` fresh and passes it to `ai::manager::resolve_backend_with_model`: provider on/off, keys,
  and base URLs stay single-sourced in `ai/` (D49); only the model is slot-specific, so the bulk slot slots in later
  with no migration (D43). An empty override uses the `ai/` model.
- **Consent is enforced in the BACKEND send path, not just the rail UI.** `ask_cmdr_send_message` calls
  `consent::has_current_consent` BEFORE creating a thread or resolving the LLM and refuses with a typed `NoConsent` event
  otherwise (fails closed), so nothing reaches a provider unconsented even if the UI is bypassed. **Bump
  `CONSENT_COPY_VERSION` whenever the consent copy changes materially** so users re-accept. The record (version +
  timestamp) lives in `main.db`'s `meta` table via `store::{get,set,clear}_consent`.

Module layout, read-only rationale, how the slice relates to the full agent: [DETAILS.md](DETAILS.md). Read it before
any non-trivial work here: editing, planning, reorganizing, or advising.
