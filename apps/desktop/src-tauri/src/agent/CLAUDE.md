# Agent subsystem

The in-app AI agent: **Ask Cmdr** (the chat rail) plus the proactive wake loop. Named after the persistent entity, not
the surface, so every later slice grows here too.

## Module map

- `llm/`: the `AgentLlm` seam (trait, genai impl, deterministic fake). `llm/CLAUDE.md`.
- `store/`: `main.db` (migrations, conversations, FTS5, cost meter, the proposal spine); `start(app)` opens it.
  `store/CLAUDE.md`.
- `suggested_ops/`: selector resolution over the spine plus the acceptance-rate metric. `suggested_ops/CLAUDE.md`.
- `tools/`: the toolset and its gated dispatch. `tools/CLAUDE.md`.
- `memory/`: the jailed, capped folder the agent writes about the user. `memory/CLAUDE.md`.
- `wake/`: the proactive pipeline and the thread driving it. `wake/CLAUDE.md`.
- `chat/`: `run_turn` + `ChatRuntime` plus pure context assembly. `chat/CLAUDE.md`.
- `types.rs`, `outcomes.rs`, `pricing.rs`
  (an unknown cloud model is `priced = false`, never a silent $0; prices drift, re-verify at release).

## Must-knows

- **The agent can propose; only the user can approve** (invariant 7). Dispatch admits `Read`, `Propose`, and `Memory`,
  never `Write`; a `Propose` tool mutates nothing, approval is a frontend user action, and no tool approves. `Propose`
  doesn't touch consent (proposals flow agent → user), so don't re-litigate that.
- **`Access::Memory` is the one write.** The promise is "the agent writes only into its memory folder", held by
  `memory/`'s jail plus a hand-authored allowlist: ❌ never tag a tool `Memory` without adding its name there. Memory
  rides every prefix, so it is an injection surface: `memory/DETAILS.md`.
- **The egress line is structural, and the cloud AI disclosure names every item on it.** Names, paths, and metadata
  reach the provider on every turn; contents only through `search_photos` / `image_facts` (image-derived text) and
  `inspect_file` (text windows, `find` lines, PDF pages plus title and author, archive entry names, EXIF incl. GPS;
  never bytes). Widening the line is a copy change AND a `CLOUD_AI_CONSENT_VERSION` bump (invariant 8).
- **Ask Cmdr has a plain on/off (`askCmdr.enabled`); cloud consent is `ai/`'s.** `ask_cmdr_send_message` refuses with
  a typed `AskCmdrOff` before a thread exists, and the slot's resolution refuses `NoCloudConsent` on Cloud without
  "Allow cloud AI", or `Managed` when the organization's policy refuses it (`session::admit_send`). Consent itself: `../ai/DETAILS.md` § Cloud AI consent.
- **The interactive slot layers a model over shared `ai/` config.** `resolve_agent_llm` reads `askCmdr.interactiveModel`
  fresh; provider on/off, keys, and base URLs stay single-sourced in `ai/` (D49). Empty override ⇒ the `ai/` model.
- **IPC is wired.** `agent::start` registers `ChatRuntime`; `../commands/agent/` is the thin surface. `run_turn` runs
  on a worker thread (it holds a non-`Send` connection across awaits) and streams over `chat::stream`, one
  conversation-keyed event a wake shares. Register a new command in the `ipc.rs` manifest. Frontend:
  `apps/desktop/src/lib/ask-cmdr/CLAUDE.md`.

Layout rationale, the proposal tier, the **invariants register** (where a bare `(invariant 6)` citation resolves), the
principles, and the **decision log** (where `agent decision D49` resolves): `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
