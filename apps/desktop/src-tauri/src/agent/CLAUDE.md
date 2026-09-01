# Agent subsystem

The in-app AI agent. Its first user-facing slice is **Ask Cmdr**, a chat rail. Named after the persistent entity, not
the surface, so later proactive slices grow here too.

## Module map

- `llm/`: the `AgentLlm` seam (provider-agnostic trait, genai impl, deterministic fake, typed message-part model). See
  `llm/CLAUDE.md`.
- `store/`: the `main.db` durable store (migrations, conversations, messages, FTS5 search, cost meter, the proposal
  spine). `start(app)` opens the DB + registers `AgentDb`. See `store/CLAUDE.md`.
- `suggested_ops/`: the service over that spine — selector resolution against the drive index, plus the
  acceptance-rate metric. See `suggested_ops/CLAUDE.md`.
- `types.rs`: store-only token enums + `token_enum!`, and `ProposalDecision`.
- `outcomes.rs`: what the user did with a proposal, on the two channels that need it. See
  `suggested_ops/DETAILS.md`.
- `tools/`: the in-process toolset plus gated dispatch (the choke point). See `tools/CLAUDE.md`.
- `memory/`: the jailed, capped Markdown folder the agent writes about the user, fed back into every prefix. See
  `memory/CLAUDE.md`.
- `wake/`: the proactive half — the pure noticing pipeline, its gates, and the thread driving them (started by
  `start(app)`). See `wake/CLAUDE.md`.
- `chat/`: the chat runtime (`run_turn` + `ChatRuntime`: single-flight, budgets, cancellation, crash-safe
  persistence, the turn stream) + pure context assembly. See `chat/CLAUDE.md`.
- `consent.rs`: the consent gate (`CONSENT_COPY_VERSION` + `has_current_consent`, fails closed).
- `pricing.rs`: provisional per-model price table (USD/M tokens). An unknown cloud model is `priced = false`, shown
  "unknown", never a silent $0. **Prices drift**: re-verify at release.

## Must-knows

- **The agent can propose; only the user can approve** (invariant 7). No write tool for the user's files; the one
  file-content read is `inspect_file` (bounded, one path, typed): the agent view admits `Read`, `Propose`, and `Memory` entries, never `Write`. A
  `Propose` tool mutates nothing, approval originates in the frontend as a user action, and no tool approves one.
  **`Propose` doesn't touch consent** (proposals flow agent → user, never to the provider), so don't re-litigate that.
- **`Access::Memory` is the one write, and it was a deliberate widening.** The promise is now "the agent writes only
  into its memory folder", held by `memory/`'s jail plus a hand-authored allowlist. ❌ Never tag a new tool `Memory`
  without adding its name there. Memory rides the prefix of every turn, so it is also an injection surface:
  `memory/DETAILS.md`.
- **The egress line is structural.** Names, paths, and metadata reach the provider; content egress is the photo pair,
  `search_photos` and `image_facts` (image-derived TEXT, never image bytes; `mcp/executor/photos.rs`,
  `image_facts.rs`) plus `inspect_file`'s bounded text window (`tools/read/inspect.rs`). The consent copy
  (`askCmdr.consent.*`) names the photo pair; ❌ it must name `inspect_file` too before that ships. Don't widen the
  line further without revisiting the whole consent story.
- **The runtime drives the seams; the IPC is wired.** `agent::start` registers `ChatRuntime`; `../commands/agent/` is
  the thin frontend surface. `ask_cmdr_send_message` runs its turn on a worker thread (`run_turn` holds a non-`Send`
  connection across awaits) and streams over `chat::stream`, one conversation-keyed event a wake shares. Register a new
  command in the `ipc.rs` manifest. Frontend: `apps/desktop/src/lib/ask-cmdr/CLAUDE.md`.
- **The interactive slot layers a dedicated model over shared `ai/` config.** `resolve_agent_llm` reads
  `askCmdr.interactiveModel` fresh and hands it to `ai::manager::resolve_backend_with_model`: provider on/off, keys,
  and base URLs stay single-sourced in `ai/` (D49). An empty override means the `ai/` model.
- **Consent is enforced in the BACKEND send path, not just the rail UI.** `ask_cmdr_send_message` checks
  `consent::has_current_consent` before creating a thread or resolving the LLM, and answers a typed `NoConsent`
  refusal, so a bypassed UI still reaches no provider. **Bump `CONSENT_COPY_VERSION` whenever the copy changes
  materially**; the record lives in `main.db`'s `meta` table.

Module layout, read-only rationale, how the slice relates to the full agent, and the **invariants register** (where
code citing a bare `(invariant 6)` resolves): `DETAILS.md`. Read it before any non-trivial work here.
