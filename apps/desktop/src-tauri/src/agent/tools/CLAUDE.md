# Agent tools (`agent/tools/`)

The Ask Cmdr agent's in-process tool layer: read families, propose families, and the two memory tools, authored as
`consumers: [Agent]` entries in the shared `mcp_tools!` registry (agent-spec D49, one authored source), with handlers
and typed result shapes colocated here. Depth: `DETAILS.md`.

## Module map

- `read/`: one file per family — `state`, `pane_listing`, `listing`, `importance`, `volumes` — plus `inspect/` (DTOs +
  pipeline, `text.rs` the window, `find.rs` the search, `runner.rs` the timeouts). The `operations_*`,
  `search_photos`, and `image_facts` handlers are shared with the ai-client view, in `mcp/executor/`.
- `propose/`: server-owned rename proposals (`propose_rename_plan`) and the image-facts evidence ledger.
  `propose/CLAUDE.md`.
- `suggestions/`: the suggested-ops trio over the proposal spine. `suggestions/CLAUDE.md`.
- `memory.rs`: `memory_write` + `memory_edit`, the only tools that write. Every rule is in `../memory/`; this is root
  resolution plus result shaping. `DETAILS.md` § The two tools that write.
- `quiet.rs`: `nothing_to_suggest`, the one pure SIGNAL tool. Its handler changes nothing; the wake path acts on the
  call, so a rail turn calling it is inert. `DETAILS.md` § A tool that is only a signal.
- `view.rs`: the gated dispatch — `dispatch` + `refuse_unavailable`, the no-write choke point.
- `mod.rs`: `agent_tool_declarations()` (registry view → `ToolDeclaration`s).

## Must-knows

- **Reuse the shipped core; never re-derive.** Each handler calls a deterministic core (`indexing::read::queries`, the
  importance `snapshot_*` functions, `snapshot_volumes`, the proposal store) and only SHAPES the result. A second copy
  rots against the first.
- **A result that carries a list must fit ONE tool result.** Page it with `mcp::fit_to_result_budget` and report
  `total` / `returned` / `truncated`. A row cap alone doesn't bound a payload, and an oversized result pushes the rest
  of the turn out of the prompt — that's how a rename turn lost its evidence. `DETAILS.md` § The size contract.
- **A schema is PREFIX.** Every declaration rides in the cached prefix of every turn, so a verbose schema is paid for
  on calls that never touch the tool. Keep descriptions terse. `agent/chat/DETAILS.md` § What the budgets buy.
- **Every result voices its coverage honestly** (spec §2.4, load-bearing): a read that's a lower bound, stale, or
  unindexed says so in its typed result, never a wrong zero. Field by field: `DETAILS.md`.
- **`Unrecognized` is out of the view AND out of dispatch.** `ToolId::from_wire_name` turns any non-view name into
  `Unrecognized`, and `refuse_unavailable` answers "not available" BEFORE `execute_tool`, as it does for anything the
  registry calls `Write`. Keep `ToolId::KNOWN` 1:1 with `agent_tool_view()` (a test pins it).
- **The agent can propose; only the user can approve.** Dispatch admits `Read`, `Propose`, and `Memory`, never
  `Write`. A `Propose` tool stages a proposal, mutates nothing else, can't self-approve, and caps its payload.
- **Two hand-authored allowlists guard the two widenings**: `EXPECTED_PROPOSE_TOOL_NAMES` and
  `EXPECTED_MEMORY_TOOL_NAMES`. No structural check can prove a handler doesn't mutate, or stays in `../memory/`'s
  jail. ❌ Never tag an entry `Propose` or `Memory` without adding it there, and never expose a `Memory` tool to the
  ai-client view.
- **A proposal claiming file CONTENTS must prove it.** `dispatch` feeds every non-elided `image_facts` result into the
  `ImageFactsLedger`, scoped to the thread; a plan citing content the ledger has no delivery for is refused whole, and
  whatever elides a result owes the ledger a `revoke_call`. `propose/DETAILS.md`.
- **Handlers read Rust-side stores, pane caches, and SQLite only — never a live `statfs`/`readdir`**: a dead NAS
  can't hang a tool. The one exception is `inspect_file`: up to 200 files per call on blocking threads (5 s per path,
  20 s per call); past a deadline the row is `unreachable`, the thread is ABANDONED, and unanswered paths are named. It
  rides `file_viewer`'s seams (backends, search, UTF-16 clamp), never its own (`DETAILS.md` § Reading a file the way
  the viewer does), and is the one tool that egresses file CONTENTS (bounded text windows and `find` snippets, never
  bytes). ❌ The consent copy must name it before release.
- **The registry couples `mcp` ↔ `agent`** (D49, intended). New agent tool = one registry entry + handler/schema/result
  here + a `ToolId` variant + its name in `EXPECTED_AGENT_TOOL_NAMES` and `ToolId::KNOWN` + a rail label in
  `ask-cmdr-labels.ts` (miss it and the tool line shows "Working"; a test pins it). It also moves
  `budget::FIXED_PROMPT_OVERHEAD_TOKENS`, pinned by `context/cost_tests.rs`.
