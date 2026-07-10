# Executor

The MCP tool handlers. Dispatch (`execute_tool`) is generated from the `mcp_tools!` table in
[`../tool_registry.rs`](../tool_registry.rs), which calls these handlers by path; action-tool handlers wait on a typed
ack signal before returning `OK`. Up: [`../CLAUDE.md`](../CLAUDE.md).

## Files

- **`mod.rs`**: shared types (`ToolResult`, `ToolError`), the `mcp_round_trip` / `resource_round_trip` helpers, and
  `user_path_param` / `expand_user_path` (tilde expansion). (Dispatch is generated in the registry, not here.)
- **`ack.rs`**: the ack contract (`AckSignal` variants, `snapshot_generation`, `wait_for_ack`, default budgets).
- Category handlers: `app.rs`, `view.rs`, `nav.rs`, `file_ops.rs`, `dialogs.rs`, `async_tools.rs`, `search.rs`,
  `downloads.rs`, `operation_log.rs` (the `operations_*` journal tools). (Per-file tool lists in DETAILS.md.)

## Must-knows

- **Every fire-and-forget action tool waits for a backend ack before returning `OK`** (`wait_for_ack`, default 1500 ms;
  nav family uses `NAV_ACK_TIMEOUT` = 5 s for slow SMB/MTP paths): snapshot precondition, emit/run, then wait; on
  timeout return `ToolError::internal` naming the missing signal and budget. Don't return `OK` without waiting. The
  budget is a backend floor, not a tool param (bump per-call via the `Duration` arg). Variants and mapping: DETAILS.md §
  Ack contract.
- **`GenerationAdvanced` isn't a per-action proof.** It shows the FE pushed pane state after dispatch, not that it
  handled our event; an unrelated push in that window is a rare false positive. If a real one surfaces, switch the tool
  to `mcp_round_trip` with a `requestId` (`parse_mcp_response` in `mod.rs`).
- **Use `mcp_round_trip` when the backend can't fully validate preconditions or must wait on the OS.** It emits an event
  with a `requestId` and waits for the FE `mcp-response` (`{ requestId, ok, error? }`); don't replicate FE knowledge in
  Rust. Used by `move_cursor`, `set_setting`, `select`, `refresh`, `nav_to_path`, `open_under_cursor`, and resources via
  `resource_round_trip`. Per-tool timeouts in DETAILS.md.
- **`move_cursor` and `select` flush the MCP state push (`syncStateToMcpNow`) before replying.** Without the flush, the
  new cursor/selection lives only in FE state until the debounced sync, so a follow-up `copy`/`move`/`delete` reads
  stale state and `check_operation_has_target` wrongly rejects with "Nothing to copy". Don't drop the flush. A read-only
  name-resolving tool (`tag`) has no such action, so it calls `flush_pane_state(app, pane)` first for the same freshness.
- **Read filesystem path params through `user_path_param` / `expand_user_path` (in `mod.rs`), never raw
  `params.get(...)`.** Agents routinely send `~/Downloads`; the FE and existence checks need absolute paths, and a
  literal `~` either fails validation or silently never matches and burns the full timeout. Validate existence via
  `validate_path_exists`, never bare `Path::exists()` (blocks forever on a hung mount). Exception: the `search` /
  `ai_search` `scope` param handles `~` itself in `search::query::parse_scope`.
- **`copy`/`move`/`delete` fast-fail on empty operations** via `check_operation_has_target` (pure unit-tested
  `empty_operation_error`) before dispatching: cursor on `..` or an empty pane means the FE would silently drop the
  dialog, so the tool rejects fast with the real cause instead of a generic timeout. Unsynced state (`path` empty)
  passes through (the FE is the authority).
- **`dialog close settings` requires FE opt-in.** The settings window must listen for `mcp-settings-close` and close
  itself; without that listener the backend polls for `WindowDisappeared("settings")` and times out at 1500 ms. Same
  shape for any new window-based dialog.
- **Tab mutations must go through `update_pane_tabs`** (delegates to `PaneStateStore::set_tabs`, the single place tab
  mutation + generation bump live). Any bypass makes the `tab` MCP tool's ack time out.

## Adding new tools

Add the handler here (pick or create a category file, declaring it `pub(crate) mod …` in `mod.rs`), then author the
tool's one `mcp_tools!` entry in [`../tool_registry.rs`](../tool_registry.rs) (schema, `TokenGate`, `run:` shape tag +
handler path). Follow the must-knows above for path params, ack choice, and round-trips (a pane-state mutator prefers
`AckSignal::GenerationAdvanced` via `PaneStateStore`). Full workflow:
[`mcp-development.md`](../../../../../../docs/guides/mcp-development.md).

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
