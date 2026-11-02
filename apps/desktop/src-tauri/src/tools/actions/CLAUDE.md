# Tool actions

The handlers shared with outside clients. Dispatch (`execute_tool`) is generated from the `mcp_tools!` table in `../table.rs`,
which calls these by path. Up: `../CLAUDE.md`.

## Files

- **`mod.rs`**: shared types (`ToolResult`, `ToolError`), the `*_round_trip` helpers, and `user_path_param` /
  `expand_user_path` (tilde expansion). Dispatch is generated in the registry, not here.
- **`ack.rs`**: the ack contract (`AckSignal` variants, `snapshot_generation`, `wait_for_ack`, default budgets).
- Category handlers, one file per tool family (`nav.rs`, `file_ops.rs`, `memory.rs`, `cpu.rs`, `multi_rename.rs`, …).
  Which tools each one owns: DETAILS.md.

## Must-knows

- **Every fire-and-forget action tool waits for a backend ack before returning `OK`** (`wait_for_ack`, default 1500 ms;
  nav 5 s). A timeout is `ToolError::outcome_unknown`: the action went out and may still run. ❌ Never `OK` without
  waiting, ❌ never word a timeout as a failure. Variants and budgets: DETAILS.md § Ack contract.
- **A result that carries a list gets `fit_to_result_budget`** (`mod.rs`) and reports `total`/`returned`/`truncated`. A
  row cap doesn't bound a payload (`image_facts` at 200 paths was ~100k tokens), and an oversized result pushes the rest
  of the caller's turn out of context. ❌ Never cut silently. Depth: `../DETAILS.md` § The size contract.
- **`GenerationAdvanced` isn't a per-action proof** (an unrelated push is a rare false positive): switch such a tool to
  `mcp_round_trip`.
- **Use `mcp_round_trip` when the backend can't fully validate preconditions or must wait on the OS.** It waits for the
  FE `mcp-response` (`{ requestId, ok, error? }`) so FE knowledge isn't replicated in Rust. Its users, and
  `resource_round_trip`: DETAILS.md.
- **`move_cursor`, `select`, `nav_to_path`, `select_volume` flush the MCP state push before replying**; `tag` and the
  dialog openers call `flush_pane_state`. ❌ Don't drop it: a `copy` would read stale state and reject "Nothing to copy".
- **Read filesystem path params through `user_path_param` / `expand_user_path`, ❌ never raw `params.get(...)`.** Agents
  routinely send `~/Downloads`, and a literal `~` fails validation or silently never matches, burning the full timeout.
  Validate existence via `validate_path_exists`, ❌ never bare `Path::exists()` (blocks forever on a hung mount). The
  `search` / `ai_search` `scope` param is the exception: it handles `~` itself.
- **An answer to a name clash must NAME the clash, and report what the answer DID.** `resolve_conflict` requires the
  `conflictId` off `cmdr://state`'s `pendingConflict:` block and returns the backend's typed outcome; ❌ never collapse
  `stale_answer` / `no_pending_conflict` into an `OK`, and ❌ never default the id to "whatever is pending": the
  operation may have moved on.
- **Tools that START a file operation fast-fail while a dialog is up** (`refuse_while_dialog_blocks`), naming the
  blocker in the TYPED `data.blockingDialog`. ❌ Never widen it to `queue` or `dialog`: steering a RUNNING operation
  is what an agent needs then. `operations_rollback` refuses from the frontend (DETAILS.md § `operations_rollback`).
  `src/lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
- **Tools that start something require `pane`, and `copy`, `move`, and `tag` refuse without a selection**: focus moves
  silently. ❌ Never fall back to focus or the cursor. DETAILS.md § Ack contract.
- **Opening a dialog answers `{ awaitingPerson, dialog }`** (`open_dialog`), ❌ never an "OK" a model reads as done.
- **A window-based dialog can only be closed with FE opt-in**: `dialog close settings` needs that window listening for
  `mcp-settings-close`, else the backend polls for `WindowDisappeared` and times out. Same for any new one.
- **Tab mutations must go through `update_pane_tabs`** (the single place tab mutation + generation bump live); a bypass
  makes the `tab` tool's ack time out.

## Adding new tools

`../DETAILS.md` § Adding a tool.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
