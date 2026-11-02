# AI tools (`tools/`)

Every AI-callable tool, for both callers: one authored table, the handlers, and the `cmdr://` resources. `../mcp/` is the
transport for outside clients; `../agent/` is the in-process runtime, with the gated dispatch in `../agent/tool_gate.rs`.

## Module map

- `registry.rs` + `table.rs`: the `mcp_tools!` mechanism and the one authored table, which generates `tools/list`,
  `ToolId`, and the dispatch. `params.rs`: the schema gate. `schemas/`: per-tool `json!` blocks. `error_codes.rs`: JSON-RPC codes.
- `actions/`: the handlers shared with outside clients, with the ack contract. `actions/CLAUDE.md`.
- `resources/`: the read-only `cmdr://` views. `resources/CLAUDE.md`.
- `read/`: the agent's reads (`state`, `pane_listing`, `listing`, `importance`, `volumes`, `inspect/`).
- `propose/`, `suggestions/`: staged plans. `propose/CLAUDE.md`, `suggestions/CLAUDE.md`.
- `memory.rs`: the agent's notes; every rule lives in `../agent/memory/`. `quiet.rs`: `nothing_to_suggest`, a
  pure signal. `connect.rs`: `connect_volume`. `meta.rs`: the tool-search pair. `sensitive.rs`: the content gate. `policy.rs`: `admit`, `Caller`, `ToolCtx`, and
  direct mode. `ask.rs` + `ask_plans.rs`: the one way a tool waits for a person. `mod.rs`:
  `agent_tool_declarations()`.

## Must-knows

- **One table, one axis.** A tool declares only what it does (`access`), and `policy::admit` alone answers Allow, Ask, or
  Refuse. **The agent can start anything. A person finishes anything sensitive.** ❌ No per-caller list: every tool is
  reachable by both callers, or its entry says why not (`DevOnly`, `Load::Meta`). `DETAILS.md` § The rule.
- **Every class but `Read` and `AppState` is hand-allowlisted** (`EXPECTED_*_TOOL_NAMES`): nothing proves what a handler
  does.
- **`dialog_confirm` is the only way past a person** (`DevOnly`), and `dialog` has no `confirm`.
- **A handler needing the caller takes `ctx_params`**: `ToolCtx { caller, cancel, conversation_id, asked }`. A dialog opener's
  payload goes through `ctx.stamp`, so what a person confirms records who started it and a guarded caller's dialog must
  arm (`DETAILS.md` § Who's calling).
- **A `Sensitive` tool names an `ask:` plan**, which reads and runs twice, ❌ never a question in a handler. `DETAILS.md`
  § The human gate.
- **`DevOnly`, `ReadContents`, and `Memory` refuse an outside client off direct mode** (`admit`), and ❌ the JSON setting
  alone never turns it on. `DETAILS.md` § Direct mode.
- **A call is checked against its declared schema before a handler sees it** (`params.rs`). Every schema closes itself
  with `additionalProperties: false`, nested objects too. Params are camelCase, tool
  names snake_case.
- **Whatever a user can reach, an agent must reach, answer, and OBSERVE.** ❌ No modal state with no way to see or answer
  it.
- **A handler calls a deterministic core** and only SHAPES the result.
- **A place is `path` + optional `volumeId` in, `location` out, via `crate::location`'s one resolver.** A miss is a typed
  `Ok`, ❌ never an empty listing.
- **A list must fit ONE tool result**: `fit_to_result_budget`, reporting `total` / `returned` / `truncated`. ⚠️ `search`'s
  `matchCount` counts PAST the cap, so it's never a `total`.
- **A core schema and a deferred `brief` are PREFIX**, so a new tool is `Load::Deferred` unless a flow needs it. ⚠️ Every
  declared property needs a `type`, or llama-server refuses the request. `DETAILS.md` § Lazy loading.
- **Every result voices its coverage**: a typed field, never a wrong zero.
- **A proposal claiming file CONTENTS must prove it** through `ImageFactsLedger`. `propose/DETAILS.md`.
- **Read handlers never touch a live filesystem, except `inspect_file` and `list_dir`'s live read**, bounded. Anything
  yielding contents passes `sensitive.rs`. A new content KIND is a copy change plus a `CLOUD_AI_CONSENT_VERSION` bump.
- **New tool** = table entry + handler and schema + its class's `EXPECTED_*_TOOL_NAMES` + a rail label + the wire fixture.
  `DETAILS.md` § Adding a tool.

Depth: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
