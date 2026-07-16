# Agent tools (`agent/tools/`)

The Ask Cmdr agent's in-process read-only tool layer: the five v1 read families, authored as
`consumers: [Agent], access: Read` entries in the shared `mcp_tools!` registry (agent-spec D49, one authored source), with
their handlers + typed result shapes colocated here. Depth (tool catalog, the top-N-by-size gap, the shim/visibility
list): [DETAILS.md](DETAILS.md).

## Module map

- `read/`: one file per family — `state` (`app_state`), `listing` (`list_dir` + `largest_dirs`), `importance`
  (`important_folders` + `folder_importance`), `volumes` (`list_volumes`). The `operations_list` / `operations_get` and
  `search_photos` (photo search) tools are shared with the ai-client view, so their handlers live in `mcp/executor/`
  (`operation_log.rs`, `photos.rs`), not here.
- `view.rs`: the gated dispatch — `dispatch` + `refuse_unavailable` (the read-only choke point).
- `mod.rs`: `agent_tool_declarations()` (registry view → `ToolDeclaration`s).

## Must-knows

- **Reuse the shipped core; never re-derive.** Each handler calls a deterministic core (the `indexing::queries`, the
  `importance` read API / the `cmdr://importance` `snapshot_*` functions, `snapshot_volumes`) and only SHAPES the result.
  Don't reimplement listing, scoring, or volume enumeration — a second copy rots against the first.
- **Every result voices its coverage honestly (spec §2.4 — load-bearing).** A read that's a lower bound or stale MUST
  say so in its typed result: index `Coverage` (`fresh`/`scanning`/`stale`/`off`, only `fresh` authoritative), `DirStats`
  size flags (`sizeIsLowerBound`/`sizeIsStale`/`sizeIsUpdating`), importance `stale` (asOf vs recompute generation). An
  unindexed volume returns a typed "no index", NEVER a wrong zero; an unmounted-but-scored volume still answers
  importance (offline is a headline). The system prompt requires the model to relay these.
- **`Unrecognized` is out of the view AND out of dispatch.** A raw provider tool name resolves through
  `ToolId::from_wire_name`; any non-view name (a hallucinated `delete`/`copy`, a typo) becomes `ToolId::Unrecognized`,
  which `refuse_unavailable` turns into a typed "not available" result BEFORE `execute_tool` is reached. That parse step
  is the runtime read-only gate; `refuse_unavailable` also refuses any known name the registry doesn't classify
  `Access::Read` (a backstop). Keep `ToolId::KNOWN` 1:1 with `agent_tool_view()` (the structural test pins it).
- **Handlers read Rust-side stores + SQLite only — never a live `statfs`/`readdir` on a mount.** The index and
  importance DBs answer everything, so a dead NAS can't hang a tool (the whole point of reading the cache).
- **The registry couples `mcp` ↔ `agent`.** The `mcp_tools!` entries reference `crate::agent::tools::read::*` handler +
  schema paths, and `agent::tools` calls back into `crate::mcp::{execute_tool, agent_tool_view, tool_access, Consumer,
  Access, ToolError, ToolResult}` (re-exported from `mcp` for exactly this). Same-crate cycle, intended (D49: one
  registry, two consumers). New agent tool = one registry entry + a handler/schema/result here + a `ToolId` variant +
  its name in `EXPECTED_AGENT_TOOL_NAMES` and `ToolId::KNOWN`.

Depth: [DETAILS.md](DETAILS.md).
