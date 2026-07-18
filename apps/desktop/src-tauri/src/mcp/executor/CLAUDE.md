# Executor

The MCP tool handlers. Dispatch (`execute_tool`) is generated from the `mcp_tools!` table in
[`../tool_registry/mod.rs`](../tool_registry/mod.rs), which calls these handlers by path; action-tool handlers wait on a
typed ack before returning `OK`. Up: [`../CLAUDE.md`](../CLAUDE.md).

## Files

- **`mod.rs`**: shared types (`ToolResult`, `ToolError`), the `mcp_round_trip` / `resource_round_trip` helpers, and
  `user_path_param` / `expand_user_path` (tilde expansion). Dispatch is generated in the registry, not here.
- **`ack.rs`**: the ack contract (`AckSignal` variants, `snapshot_generation`, `wait_for_ack`, default budgets).
- Category handlers: `app.rs`, `view.rs`, `nav.rs`, `file_ops.rs`, `dialogs.rs`, `async_tools.rs`, `search.rs`,
  `downloads.rs`, `operation_log.rs` (`operations_*` journal), `photos.rs` (`search_photos`, text-only DTO over the
  `media_index` read API). Per-file tool lists in DETAILS.md.

## Must-knows

- **Every fire-and-forget action tool waits for a backend ack before returning `OK`** (`wait_for_ack`, default 1500 ms;
  nav uses `NAV_ACK_TIMEOUT` = 5 s): snapshot precondition, emit/run, wait; on timeout return `ToolError::internal`
  naming the missing signal + budget. Never return `OK` without waiting. The budget is a backend floor, bumped per-call
  via the `Duration` arg. Variants + mapping: DETAILS.md § Ack contract.
- **`GenerationAdvanced` isn't a per-action proof** (it shows the FE pushed pane state after dispatch, not that it
  handled our event; unrelated pushes are rare false positives). If a real one surfaces, switch the tool to
  `mcp_round_trip` with a `requestId`.
- **Use `mcp_round_trip` when the backend can't fully validate preconditions or must wait on the OS.** It waits for the
  FE `mcp-response` (`{ requestId, ok, error? }`) so FE knowledge isn't replicated in Rust. Used by `move_cursor`,
  `set_setting`, `select`, `refresh`, `nav_to_path`, `open_under_cursor` (+ resources via `resource_round_trip`).
- **`move_cursor` and `select` flush the MCP state push (`syncStateToMcpNow`) before replying.** Without it a follow-up
  `copy`/`move`/`delete` reads stale state and `check_operation_has_target` wrongly rejects "Nothing to copy". Don't
  drop the flush. The read-only `tag` tool calls `flush_pane_state(app, pane)` first for the same freshness.
- **Read filesystem path params through `user_path_param` / `expand_user_path`, never raw `params.get(...)`.** Agents
  routinely send `~/Downloads`; a literal `~` fails validation or silently never matches and burns the full timeout.
  Validate existence via `validate_path_exists`, never bare `Path::exists()` (blocks forever on a hung mount). Exception:
  the `search` / `ai_search` `scope` param handles `~` itself in `search::query::parse_scope`.
- **`copy`/`move`/`delete` fast-fail on empty operations** via `check_operation_has_target` before dispatching (cursor on
  `..` or an empty pane → the FE silently drops the dialog), so the tool rejects with the real cause, not a timeout.
  Unsynced state (`path` empty) passes through (the FE is the authority).
- **`dialog close settings` requires FE opt-in**: the settings window must listen for `mcp-settings-close` and close
  itself, else the backend polls for `WindowDisappeared("settings")` and times out at 1500 ms. Same for any new
  window-based dialog.
- **Tab mutations must go through `update_pane_tabs`** (the single place tab mutation + generation bump live); a bypass
  makes the `tab` tool's ack time out.

## Adding new tools

Add the handler here (pick/create a category file, declare it `pub(crate) mod …` in `mod.rs`), then author the tool's
one `mcp_tools!` entry in [`../tool_registry/mod.rs`](../tool_registry/mod.rs) (schema, `TokenGate`, `run:` shape tag +
handler path). Follow the must-knows for path params, ack choice, and round-trips (a pane-state mutator prefers
`AckSignal::GenerationAdvanced` via `PaneStateStore`). Full workflow:
[`mcp-development.md`](../../../../../../docs/guides/mcp-development.md).

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
