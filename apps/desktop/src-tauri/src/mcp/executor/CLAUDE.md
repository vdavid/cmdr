# Executor

Tool execution layer for the MCP server: each `tools/call` dispatches into this directory's `execute_tool()`, which fans out to category-specific handlers and (for action tools) waits on a typed ack signal before returning `OK`.

Up: [`../CLAUDE.md`](../CLAUDE.md) (mcp).

## Files

| File             | Responsibility                                                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `mod.rs`         | `execute_tool()` dispatcher, shared types (`ToolResult`, `ToolError`), the `mcp_round_trip` / `resource_round_trip` helpers, and the `user_path_param` / `expand_user_path` tilde-expansion helpers.|
| `ack.rs`         | The ack contract: `AckSignal` variants, `snapshot_generation`, `wait_for_ack`, default budgets.                                |
| `app.rs`         | `quit`, `switch_pane`, `swap_panes`, `tab` (unified action verb).                                                              |
| `view.rs`        | `toggle_hidden`, `set_view_mode`, `sort`.                                                                                      |
| `nav.rs`         | `nav_to_path`, `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`, `select_volume`, `move_cursor`, `open_under_cursor`.   |
| `file_ops.rs`    | `copy`, `move`, `delete`, `mkdir`, `mkfile`, `refresh`, `select`.                                                              |
| `dialogs.rs`     | Unified `dialog` tool: open / focus / close / confirm for settings, file-viewer, about, and confirmation dialogs.              |
| `async_tools.rs` | `await`, `connect_to_server`, `remove_manual_server`, `upgrade_smb_to_direct`, `set_setting`.                                  |
| `search.rs`      | `search` (drive index), `ai_search` (LLM-driven), and the lazy-load of the search index via `spawn_blocking`.                  |
| `downloads.rs`   | `go_to_latest_download`: resolves via `downloads::commands::go_to_latest_download`, then `mcp-nav-to-path` + `mcp-move-cursor`.|
| `tests.rs`       | Unit tests for the dispatcher and shared helpers; per-category tests live alongside their handlers.                            |

## Conventions

### Action-tool ack contract

Every fire-and-forget action tool waits for a backend ack before returning `OK`. The mechanism lives in `ack.rs`. Each tool:

1. Captures a precondition snapshot (typically `snapshot_generation(app)`).
2. Emits its event / runs its command.
3. Calls `wait_for_ack(app, signal, DEFAULT_ACK_TIMEOUT)` — default 1500 ms; the nav family uses `NAV_ACK_TIMEOUT` (5 s) because parent/back can land on a slow SMB or MTP path.
4. Returns `OK` on signal, or `ToolError::internal` naming the missing signal and elapsed budget on timeout.

`AckSignal` variants and when they fire:

| Variant                                | Fires when                                                       | Used by                                                                                                                              |
| -------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `GenerationAdvanced`                   | `PaneStateStore.generation` strictly greater than captured value | Pane mutators: `set_view_mode`, `sort`, `toggle_hidden`, `tab`, `nav_*`, auto-confirmed `copy`/`move`/`delete`, `dialog confirm`. NOT `select`/`refresh` (both are round-trips). |
| `SoftDialogAppeared(id)`               | Soft dialog with that id is in `SoftDialogTracker`               | Confirmation dialogs from `copy`/`move`/`delete` (`autoConfirm: false`), `mkdir`, `mkfile`, `dialog open about`.                     |
| `SoftDialogDisappeared(id)`            | Soft dialog with that id is no longer tracked                    | `dialog close <confirmation>` — the FE `ModalDialog` fires `notifyDialogClosed` on unmount.                                          |
| `WindowAppeared(label)`                | A `webview_windows()` entry matches (exact, or `viewer-*`)       | `dialog open settings|file-viewer`, `dialog focus`.                                                                                  |
| `WindowDisappeared(label)`             | The matching `webview_windows()` entry is gone                   | `dialog close settings` (single-window family).                                                                                      |
| `WindowCountBelow {prefix, threshold}` | Matching window count is `< threshold`                           | `dialog close file-viewer` — snapshot count, ack when one closes (don't wait for all viewers to vanish).                             |
| `Any([...])`                           | Logical OR over inner signals                                    | Reserved for multi-mode tools.                                                                                                       |

Polling cadence: 250 ms for state-driven signals (matches the `await` tool); 100 ms for window/soft-dialog signals (both react faster than a full pane state push). The 1500 ms budget is a backend-side latency budget, not a client-facing knob — don't expose it as a tool parameter; bump per-call via the `Duration` arg to `wait_for_ack` if a specific op has a known higher floor.

### `mcp_round_trip` for explicit FE responses

When the backend can't fully validate preconditions (or has to wait on the OS), the tool emits an event with a `requestId` and waits for the FE to reply via `mcp-response` carrying `{ requestId, ok, error? }`. Used by:

- `move_cursor`, `set_setting` (5 s). The FE verifies the cursor actually landed (filename found, index in range) before replying `ok: true` — a silent no-op was the original false-positive-OK bug.
- `select` (5 s, all modes) — the FE applies the selection (names mode maps names → indices via the `findFileIndices` batch IPC first), then **flushes the MCP state push (`syncStateToMcpNow`) before replying**, so a follow-up `copy` reads fresh selection state. Missing names come back as the round-trip error.
- `refresh` (5 s) — the FE forces a backend re-read via the `refreshListing` IPC (local volumes always re-read; watcher-backed MTP/SMB listings short-circuit) and replies once it completes. `OK` means the directory was actually re-read.
- `nav_to_path` — 30 s via `mcp_round_trip_with_timeout`; FE delays the response until `handleListingComplete` fires.
- `open_under_cursor` — 5 s via `mcp_round_trip_with_timeout`; opening a file delegates to the OS default app, so neither `GenerationAdvanced` nor `WindowAppeared` would fire.
- Resources that need FE data use `resource_round_trip` (same pattern, returns the `data` field). Used by `cmdr://settings`.

### Agent-supplied paths go through `user_path_param` / `expand_user_path`

Agents routinely send `~/Downloads`; the frontend and the existence checks only understand absolute paths. Both helpers live in `mod.rs`: `user_path_param(params, key)` for a required path param (extract + missing-param error + tilde expansion), `expand_user_path(s)` for optional or conditional sites (the `dialog` tool's optional `path`, the `await` tool's `value` when the condition is path-shaped). Never read a path param with raw `params.get(...)` — a literal `~` either fails validation (`nav_to_path`, `dialog`) or silently never matches and burns the full timeout (`await`). Virtual paths (`mtp://…`) don't start with `~`, so expansion is a no-op for them; the `search`/`ai_search` `scope` param is the one exception that handles `~` itself (in `search::query::parse_scope`).

### Empty-operation fast-fail (`file_ops.rs`)

`copy`/`move`/`delete` run `check_operation_has_target` before dispatching. The pure core (`empty_operation_error`, unit-tested) mirrors the FE fallback semantics: a selection wins; no selection falls back to the cursor file; cursor on `..` (or an empty pane, where the FE renders no rows at all — `files` empty with `total_files <= 1`) means the FE would silently drop the dialog, so the tool rejects fast with the real cause instead of the generic 1500 ms ack timeout. Unsynced state (`path` empty) passes through — the FE is the authority. This is why `select`'s names mode flushes the state push before replying: without it, select → copy would read stale empty selection here and wrongly reject.

### Adding new tools

- Pick the right category file; if it doesn't exist yet, create one and register it in `mod.rs::execute_tool()`.
- If the tool takes a filesystem path param, extract it via `user_path_param` (see above), and validate existence (when appropriate) via `validate_path_exists` — never bare `Path::exists()`, which blocks forever on a hung mount.
- Param names are camelCase on the wire (`tabId`, `timeoutSeconds`); see `../CLAUDE.md` § Tools.
- If the tool mutates pane state, prefer `AckSignal::GenerationAdvanced` and route the mutation through `PaneStateStore` (or `update_pane_tabs` for tab work — the single place that bumps generation for tabs).
- If the tool needs an explicit outcome from the FE, use `mcp_round_trip`. Don't replicate FE knowledge in Rust.

## Gotchas

- **`GenerationAdvanced` isn't a per-action proof.** The snapshot-dispatch-wait sequence proves the FE pushed pane state *recently after* dispatch — not that the FE handled our specific event. An unrelated push between snapshot and dispatch (other pane's watcher, a tab refresh) satisfies the signal as a false positive. The window is microseconds wide and the FE was clearly running, so it's much weaker than the original "always OK" bug. If a real false positive surfaces, switch the affected tool to `mcp_round_trip` with a `requestId`. Tagged `TODO(mcp-ack):` in `ack.rs`.
- **`dialog close settings` requires FE opt-in.** The settings window must listen for `mcp-settings-close` and close itself (`apps/desktop/src/routes/settings/+page.svelte`). Without that listener, the backend keeps polling for `WindowDisappeared("settings")` and times out at 1500 ms while the window sits there. Same shape applies to any new window-based dialog: the FE side has to opt in.
- **Tab mutations must go through `update_pane_tabs`.** That command delegates to `PaneStateStore::set_tabs`, the single place tab mutation + generation bump live. Any new tab-mutating path that bypasses it makes the `tab` MCP tool's ack time out.
