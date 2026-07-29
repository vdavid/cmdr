# Agent tools (`agent/tools/`)

The Ask Cmdr agent's in-process tool layer: the five v1 read families, authored as
`consumers: [Agent], access: Read` entries in the shared `mcp_tools!` registry (agent-spec D49, one authored source),
handlers and typed result shapes colocated here. Depth: `DETAILS.md`.

## Module map

- `read/`: one file per family — `state` (`app_state`), `pane_listing` (`list_pane_files`), `listing` (`list_dir` +
  `largest_dirs`), `importance`
  (`important_folders` + `folder_importance`), `volumes` (`list_volumes`). The `operations_list` / `operations_get` and
  `search_photos` / `image_facts` (photo search and image lookup) tools are shared with the ai-client view, so their
  handlers live in `mcp/executor/` (`operation_log.rs`, `photos.rs`, `image_facts.rs`), not here.
- `view.rs`: the gated dispatch — `dispatch` + `refuse_unavailable` (the read-only choke point).
- `propose/`: server-owned, immutable proposal staging. Its tools validate cached state only and never apply a change.
- `mod.rs`: `agent_tool_declarations()` (registry view → `ToolDeclaration`s).

## Must-knows

- **Reuse the shipped core; never re-derive.** Each handler calls a deterministic core (the `indexing::read::queries`, the
  `importance` read API / the `cmdr://importance` `snapshot_*` functions, `snapshot_volumes`) and only SHAPES the result.
  Don't reimplement listing, scoring, or volume enumeration — a second copy rots against the first.
- **A result that carries a list must fit ONE tool result.** Page it with
  `mcp::executor::fit_to_result_budget` (ceiling: `agent::chat::budget::MAX_TOOL_RESULT_TOKENS`) and report
  `total`/`returned`/`truncated`. A row cap alone doesn't bound a payload, and an oversized result pushes the rest of the
  turn out of the prompt — that's how a rename turn lost its own evidence. Depth: `DETAILS.md` § The size contract.
- **Every result voices its coverage honestly (spec §2.4 — load-bearing).** A read that's a lower bound or stale says so
  in its typed result (index `Coverage`, `DirStats` size flags, importance `stale`), an unindexed volume returns a typed
  "no index" and NEVER a wrong zero, and the system prompt makes the model relay all of it. Field-by-field: `DETAILS.md`.
- **`Unrecognized` is out of the view AND out of dispatch.** `ToolId::from_wire_name` turns any non-view name (a
  hallucinated `delete`, a typo) into `Unrecognized`, and `refuse_unavailable` answers "not available" BEFORE
  `execute_tool` — as it does for anything the registry calls `Write` or doesn't classify. Keep `ToolId::KNOWN` 1:1 with
  `agent_tool_view()` (a structural test pins it).
- **The agent can propose; only the user can approve.** Dispatch admits `Access::Read` and `Access::Propose`, never
  `Access::Write`. A `Propose` tool stages a proposal and opens a review surface: it mutates nothing, it can't
  self-approve (no tool approves a proposal, ever), and it must cap its payload the way `image_facts` caps at 200 paths.
  Adding one also means adding its name to `EXPECTED_PROPOSE_TOOL_NAMES` by hand. Depth: `DETAILS.md`.
- **A proposal claiming file CONTENTS must prove it.** `dispatch` feeds every non-elided `image_facts` result into
  `propose::evidence::ImageFactsLedger`, scoped to the chat thread (`EvidenceScope`); `propose_rename_plan` refuses a
  whole plan citing content the ledger has no delivery for. Whatever elides a result owes the ledger a
  `revoke_call(call_id)`, or it vouches for content the model never read: how 12 real files got fabricated names.
  `propose/DETAILS.md`.
- **Handlers read Rust-side stores, pane caches, and SQLite only — never a live `statfs`/`readdir`**, so a dead NAS
  can't hang a tool.
- **The registry couples `mcp` ↔ `agent`.** The `mcp_tools!` entries reference `crate::agent::tools::read::*` handler +
  schema paths, and `agent::tools` calls back into `crate::mcp::{execute_tool, agent_tool_view, tool_access, Consumer,
  Access, ToolError, ToolResult}` (re-exported from `mcp` for exactly this). Same-crate cycle, intended (D49: one
  registry, two consumers). New agent tool = one registry entry + a handler/schema/result here + a `ToolId` variant +
  its name in `EXPECTED_AGENT_TOOL_NAMES` and `ToolId::KNOWN` + a rail label in `ask-cmdr-labels.ts` (miss it and the
  tool line shows the generic "Working" fallback, costing transparency silently; a structural test pins it).

Depth: `DETAILS.md`.
