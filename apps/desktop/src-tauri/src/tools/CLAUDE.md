# AI tools (`tools/`)

Every AI-callable tool, for both callers: one authored table, the handlers, and the `cmdr://` resources. `../mcp/` is the
transport for outside clients; `../agent/` is the in-process runtime, with the gated dispatch in `../agent/tool_gate.rs`.

## Module map

- `registry.rs` + `table.rs`: the `mcp_tools!` mechanism and the one authored table, which generates each consumer's
  view, `ToolId`, and the dispatch. `params.rs`: the schema gate. `schemas/`: per-tool `json!` blocks. `error_codes.rs`: JSON-RPC codes.
- `actions/`: the handlers shared with outside clients, with the ack contract. `actions/CLAUDE.md`.
- `resources/`: the read-only `cmdr://` views. `resources/CLAUDE.md`.
- `read/`: the agent's reads (`state`, `pane_listing`, `listing`, `importance`, `volumes`, `inspect/`).
- `propose/`, `suggestions/`: staged plans. `propose/CLAUDE.md`, `suggestions/CLAUDE.md`.
- `memory.rs`: the only tools that write; every rule lives in `../agent/memory/`. `quiet.rs`: `nothing_to_suggest`, a
  pure signal. `connect.rs`: `connect_volume`. `sensitive.rs`: the content gate. `policy.rs`: direct mode. `mod.rs`:
  `agent_tool_declarations()`.

## Must-knows

- **One table, two consumer views.** Each entry declares `consumers` + `access`. **The agent can propose; only the user
  can approve**: `[agent]` entries are `Read`, `ReadContents`, `Propose`, `Memory`, or `Session` (pinned structurally).
- **Every class but `Read` and `AppState` is hand-allowlisted** (`EXPECTED_*_TOOL_NAMES`): nothing proves a handler doesn't
  mutate, stays jailed, or dials only a saved place. ❌ Never give AI clients `Memory`.
- **`DevOnly` tools refuse an outside client off direct mode** (`policy.rs`), and ❌ the JSON setting alone never turns it
  on. `DETAILS.md` § Direct mode.
- **A call is checked against its declared schema before a handler sees it** (`params.rs`). An agent tool closes itself
  with `additionalProperties: false`. Params are camelCase, tool names snake_case.
- **Whatever a user can reach, an agent must reach, answer, and OBSERVE.** ❌ No modal state with no way to see or answer
  it: a conflict wedge lived in one for months.
- **Reuse the shipped core.** A handler calls a deterministic core and only SHAPES the result.
- **A place is `path` + optional `volumeId` in, `location` out, via `crate::location`'s one resolver.** A miss is a typed
  `Ok`, ❌ never an empty listing.
- **A list must fit ONE tool result**: `fit_to_result_budget`, reporting `total` / `returned` / `truncated`. ⚠️ `search`'s
  `matchCount` counts PAST the cap, so it's never a `total`.
- **A schema is PREFIX**: keep descriptions terse. A new agent tool moves `FIXED_PROMPT_OVERHEAD_TOKENS`.
- **Every result voices its coverage honestly**: a typed field, never a wrong zero.
- **A proposal claiming file CONTENTS must prove it** through `ImageFactsLedger`. `propose/DETAILS.md`.
- **Handlers never touch a live filesystem, except `inspect_file` and `list_dir`'s live read**: bounded, the thread
  abandoned past a deadline. A new content KIND is a copy change plus a `CLOUD_AI_CONSENT_VERSION` bump.
- **Anything yielding file contents passes `sensitive.rs`.**
- **New tool** = table entry (`ToolId` generates from it) + handler/schema/result + `EXPECTED_AGENT_TOOL_NAMES` when the agent
  gets it + a rail label in `ask-cmdr-tool-label-keys.ts`. `../agent/line/` arms only with `subject: SubjectSpec::Custom`.

Depth: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
