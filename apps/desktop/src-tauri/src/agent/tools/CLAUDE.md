# Agent tools (`agent/tools/`)

The Ask Cmdr agent's in-process tool layer: read families and propose families, authored as `consumers: [Agent]`
entries in the shared `mcp_tools!` registry (agent-spec D49, one authored source), with handlers and typed result
shapes colocated here. Depth: `DETAILS.md`.

## Module map

- `read/`: one file per family — `state`, `pane_listing`, `listing`, `importance`, `volumes`. The `operations_*`,
  `search_photos`, and `image_facts` handlers are shared with the ai-client view and live in `mcp/executor/`.
- `propose/`: server-owned rename proposals (`propose_rename_plan`) and the image-facts evidence ledger.
  `propose/CLAUDE.md`.
- `suggestions/`: the suggested-ops trio over the proposal spine. `suggestions/CLAUDE.md`.
- `view.rs`: the gated dispatch — `dispatch` + `refuse_unavailable`, the read-only choke point.
- `mod.rs`: `agent_tool_declarations()` (registry view → `ToolDeclaration`s).

## Must-knows

- **Reuse the shipped core; never re-derive.** Each handler calls a deterministic core (`indexing::read::queries`, the
  importance `snapshot_*` functions, `snapshot_volumes`, the proposal store) and only SHAPES the result. A second copy
  of listing, scoring, or enumeration rots against the first.
- **A result that carries a list must fit ONE tool result.** Page it with `mcp::fit_to_result_budget` and report
  `total` / `returned` / `truncated`. A row cap alone doesn't bound a payload, and an oversized result pushes the rest
  of the turn out of the prompt — that's how a rename turn lost its own evidence. `DETAILS.md` § The size contract.
- **A schema is PREFIX.** Every declaration rides in the cached prefix of every turn, so a verbose schema is paid for
  on calls that never touch the tool. Keep descriptions terse; say the rest once, in the registry line or the system
  prompt. `agent/chat/DETAILS.md` § What the budgets buy.
- **Every result voices its coverage honestly** (spec §2.4, load-bearing): a read that's a lower bound, stale, or
  unindexed says so in its typed result, and never answers a wrong zero. Field by field: `DETAILS.md`.
- **`Unrecognized` is out of the view AND out of dispatch.** `ToolId::from_wire_name` turns any non-view name into
  `Unrecognized`, and `refuse_unavailable` answers "not available" BEFORE `execute_tool`, as it does for anything the
  registry calls `Write` or doesn't classify. Keep `ToolId::KNOWN` 1:1 with `agent_tool_view()` (a test pins it).
- **The agent can propose; only the user can approve.** Dispatch admits `Access::Read` and `Access::Propose`, never
  `Access::Write`. A `Propose` tool stages a proposal, mutates nothing else, can't self-approve, and caps its payload.
  Adding one also means adding its name to `EXPECTED_PROPOSE_TOOL_NAMES` by hand.
- **A proposal claiming file CONTENTS must prove it.** `dispatch` feeds every non-elided `image_facts` result into the
  `ImageFactsLedger`, scoped to the thread; a plan citing content the ledger has no delivery for is refused whole.
  Whatever elides a result owes the ledger a `revoke_call`. `propose/DETAILS.md`.
- **Handlers read Rust-side stores, pane caches, and SQLite only — never a live `statfs`/`readdir`**, so a dead NAS
  can't hang a tool.
- **The registry couples `mcp` ↔ `agent`** (D49, intended). New agent tool = one registry entry + handler/schema/result
  here + a `ToolId` variant + its name in `EXPECTED_AGENT_TOOL_NAMES` and `ToolId::KNOWN` + a rail label in
  `ask-cmdr-labels.ts` (miss it and the tool line silently shows "Working"; a test pins it).

Depth: `DETAILS.md`.
