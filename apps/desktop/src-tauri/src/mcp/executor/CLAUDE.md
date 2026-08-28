# Executor

The MCP tool handlers. Dispatch (`execute_tool`) is generated from the `mcp_tools!` table in `../tool_registry/mod.rs`,
which calls these by path. Up: `../CLAUDE.md`.

## Files

- **`mod.rs`**: shared types (`ToolResult`, `ToolError`), the `*_round_trip` helpers, and `user_path_param` /
  `expand_user_path` (tilde expansion). Dispatch is generated in the registry, not here.
- **`ack.rs`**: the ack contract (`AckSignal` variants, `snapshot_generation`, `wait_for_ack`, default budgets).
- Category handlers, one per tool family: `app.rs`, `view.rs`, `nav.rs`, `file_ops.rs`, `dialogs.rs`, `queue.rs`,
  `conflicts.rs`, `quit.rs`, `async_tools.rs`, `search.rs`, `downloads.rs`, `operation_log.rs`, `photos.rs`,
  `image_facts.rs`. Which tools each one owns: DETAILS.md.

## Must-knows

- **Every fire-and-forget action tool waits for a backend ack before returning `OK`** (`wait_for_ack`, default 1500 ms;
  nav 5 s): snapshot precondition, emit/run, wait; on timeout return `ToolError::internal` naming the missing signal and
  budget. ❌ Never return `OK` without waiting. Variants and budgets: DETAILS.md § Ack contract.
- **A result that carries a list gets `fit_to_result_budget`** (`mod.rs`) and reports `total`/`returned`/`truncated`. A
  row cap doesn't bound a payload (`image_facts` at 200 paths was ~100k tokens), and an oversized result pushes the rest
  of the caller's turn out of context. ❌ Never cut silently. Depth: `../../agent/tools/DETAILS.md` § The size contract.
- **`GenerationAdvanced` isn't a per-action proof**: it shows the FE pushed pane state after dispatch, not that it
  handled our event, so an unrelated push is a rare false positive. Switch such a tool to `mcp_round_trip`.
- **Use `mcp_round_trip` when the backend can't fully validate preconditions or must wait on the OS.** It waits for the
  FE `mcp-response` (`{ requestId, ok, error? }`) so FE knowledge isn't replicated in Rust. Its users, and
  `resource_round_trip`: DETAILS.md.
- **`move_cursor` and `select` flush the MCP state push (`syncStateToMcpNow`) before replying**, and the read-only `tag`
  calls `flush_pane_state` for the same freshness. ❌ Don't drop it: a follow-up `copy`/`move`/`delete` would read stale
  state and `check_operation_has_target` would wrongly reject "Nothing to copy".
- **Read filesystem path params through `user_path_param` / `expand_user_path`, ❌ never raw `params.get(...)`.** Agents
  routinely send `~/Downloads`, and a literal `~` fails validation or silently never matches, burning the full timeout.
  Validate existence via `validate_path_exists`, ❌ never bare `Path::exists()` (blocks forever on a hung mount). The
  `search` / `ai_search` `scope` param is the exception: it handles `~` itself.
- **An answer to a name clash must NAME the clash, and report what the answer DID.** `resolve_conflict` requires the
  `conflictId` off `cmdr://state`'s `pendingConflict:` block and returns the backend's typed outcome; ❌ never collapse
  `stale_answer` / `no_pending_conflict` into an `OK`, and ❌ never default the id to "whatever is pending": the
  operation may have moved on.
- **Tools that START a file operation fast-fail while a dialog is up** (`refuse_while_dialog_blocks`), naming the
  blocker in the TYPED `data.blockingDialog`. ❌ Never widen it to `queue`, `operations_rollback`, or `dialog`:
  steering a RUNNING operation is what an agent needs while a dialog is up.
  `src/lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
- **`copy`/`move`/`delete` fast-fail on empty operations** via `check_operation_has_target`, so the tool rejects with
  the real cause rather than a timeout. Unsynced state (`path` empty) passes through: the FE is the authority.
- **A window-based dialog can only be closed with FE opt-in**: `dialog close settings` needs that window listening for
  `mcp-settings-close`, else the backend polls for `WindowDisappeared` and times out. Same for any new one.
- **Tab mutations must go through `update_pane_tabs`** (the single place tab mutation + generation bump live); a bypass
  makes the `tab` tool's ack time out.

## Adding new tools

The handler goes in a category file here; the tool is authored ONCE as an `mcp_tools!` entry in
`../tool_registry/mod.rs`. Full workflow: `docs/guides/mcp-development.md`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
