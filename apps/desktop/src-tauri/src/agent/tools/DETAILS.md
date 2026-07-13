# Agent tools — details

The read-only toolset the Ask Cmdr chat agent dispatches in-process. Must-knows: [CLAUDE.md](CLAUDE.md). Contract:
[`docs/specs/ask-cmdr-plan.md`](../../../../../../docs/specs/ask-cmdr-plan.md) § M4 and
[`ask-cmdr-spec.md`](../../../../../../docs/specs/ask-cmdr-spec.md) §2.4.

## The two-view registry model

There is ONE authored tool table (`mcp/tool_registry/mod.rs`, `mcp_tools!`). Each entry declares `consumers`
(`AiClient` / `Agent`) and `access` (`Read` / `Write`). The agent's tools are `consumers: [Agent], access: Read`
entries; `operations_list` / `operations_get` are shared `[AiClient, Agent]`. `agent_tool_view()` is the agent's slice;
`get_all_tools()` is the ai-client slice (agent-only entries filtered out, so the ai-client wire snapshot is unchanged).
`execute_tool(app, Consumer::Agent, name, params)` dispatches only the agent view. See
[`mcp/tool_registry` + `mcp/DETAILS.md`](../../mcp/DETAILS.md) § Consumer and access views for the mechanism.

Why the handlers live under `agent/` but the entries under `mcp/`: the registry is one authored source (D49), but a
tool's handler, schema, and typed result belong with the feature that owns them (organized by feature, not layer). So
the `mcp_tools!` entry names the handler/schema by path into `crate::agent::tools::read::*`, and `agent::tools` reaches
back for the dispatch surface (`execute_tool`, `agent_tool_view`, `tool_access`, `Consumer`, `Access`, `ToolError`,
`ToolResult`), all re-exported from `mcp` for this. It's a same-crate module cycle, which Rust allows and which D49
makes intentional.

## The tool catalog

Each handler is `async fn(&AppHandle<R>, &Value) -> ToolResult` (the `app_params` macro shape), reuses a shipped core,
and returns a typed serde shape as the tool-result JSON the model reads. Every tool maps 1:1 to a `ToolId` variant.

- **`app_state`** (`read/state.rs`) — both panes (path, cursor item, selection count, view/sort) plus the volume list.
  Built from `PaneStateStore` (`get_focused_pane` returns the SIDE; the path comes from that side's state) +
  `snapshot_volumes`. Not the private `build_state_yaml` — typed data, not parsed YAML.
- **`list_dir`** (`read/listing.rs`) — a directory's immediate children (`indexing::list_dir_children`, a new
  path-based helper added beside `get_dir_stats`) plus its recursive size stats (`get_dir_stats`) and a `Coverage`
  block. `Ok(None)` children ⇒ typed "not in index" / "no index", distinguished by whether the volume is indexed.
- **`largest_dirs`** (`read/listing.rs`) — the subdirectories under a path, ranked by recursive size. **No index query
  does this**: the handler lists the child dirs, batches `get_dir_stats` over them (`get_dir_stats_batch`), and sorts
  here. Files and symlinks are skipped (only real dirs are size-rankable).
- **`important_folders`** (`read/importance.rs`) — top-N or above-threshold across scored volumes, reusing
  `mcp::resources::importance::{snapshot_top, snapshot_threshold, snapshot_overview}` (which read every scored volume,
  including offline ones). The overview carries each volume's current generation for staleness.
- **`folder_importance`** (`read/importance.rs`) — one folder's `PathImportance` (`snapshot_path`): Scored (score +
  `Explanation` breakdown + `stale` from asOf vs the volume's current `recompute_generation`), Floored (with reason), or
  Unscored. Offline-capable.
- **`list_volumes`** (`read/volumes.rs`) — every volume with `indexStatus` (`fresh`/`scanning`/`stale`/`off`) and, for
  SMB, `smbConnectionState` (`direct`/`os_mount`/`disconnected`), straight from `snapshot_volumes` so tokens can't drift.
- **`operations_list` / `operations_get`** — the shipped executors (`mcp/executor/operation_log.rs`), shared into the
  agent view unchanged (their schemas + coverage flags already fit an agent reader).

## The honesty (coverage) contract

`read/listing.rs::coverage` is the single builder for index freshness honesty: it reuses `status_token` +
`Freshness::is_authoritative` (never re-derives the tokens) and attaches a plain-language note when a read isn't
authoritative or the path isn't indexed. `SizeStats::from_dir_stats` carries the exact-vs-lower-bound / stale / updating
/ has-symlinks flags verbatim from `DirStats`. Importance staleness is `asOfGeneration < recomputeGeneration`. These are
the flags spec §2.4 makes load-bearing; the system prompt requires the model to voice them.

## The read-only dispatch gate

`view.rs::refuse_unavailable(call_id, tool)` is the runtime enforcement point:

- `ToolId::Unrecognized(_)` (any non-view name — a hallucinated `delete`, a typo) ⇒ a typed `{ available: false, … }`
  result, returned BEFORE `execute_tool`. The parse (`ToolId::from_wire_name`) is the choke point.
- A known name the registry doesn't classify `Access::Read` ⇒ also refused (a runtime backstop against a mis-tagged
  entry; belt to the structural all-`Read` test's suspenders).
- Otherwise `None` ⇒ `dispatch` calls `execute_tool(app, Consumer::Agent, …)`, which itself refuses any name outside the
  agent view (a second, structural backstop).

The negative test (`view.rs`) drives the fake `AgentLlm`'s `CallRawTool("delete", …)` and asserts the refusal end to
end; it was proven red (gate disabled ⇒ "delete" not refused) before green.

## Cross-module symbols the toolset reuses

- `indexing::queries::list_dir_children` — a path-based helper (re-exported from `crate::indexing`); the child-listing
  analog of `get_dir_stats`, wrapping the read-pool + `index_read_path` + `resolve_path` + `IndexStore::list_children_on`
  wiring so the tool stays path-based (it lives in `indexing`, its elegant home).
- `mcp::resources::volumes::VolumeKind::token` — `pub(crate)` so the volume mapper reuses the one kind→token mapping.
- `mcp` re-exports `Access`, `Consumer`, `agent_tool_view`, `execute_tool`, `tool_access`, `ToolError`, `ToolResult` as
  `pub(crate)` for the agent runtime. `snapshot_volumes` and the `importance::snapshot_*` functions are `pub(crate)` too.

## Not covered here (the runtime harness)

A full fake-driven dispatch of a REAL agent tool (success path through `execute_tool`) needs a Tauri app with managed
state (`PaneStateStore`, the index registry, a data dir). That app harness is the chat runtime's concern, so this layer
covers the success path with per-tool pure-shaper tests (fixtures for the coverage flags) and the refusal path in full
(no app needed). The dispatch entry point the runtime calls is `agent::tools::view::dispatch`; the declaration API is
`agent::tools::agent_tool_declarations`.
