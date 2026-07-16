# Executor details

Depth for the MCP tool-execution layer. `CLAUDE.md` holds the must-knows.

## Tools by category file

- **`app.rs`**: `quit`, `switch_pane`, `swap_panes`, `tab` (unified action verb).
- **`view.rs`**: `toggle_hidden`, `set_view_mode`, `sort`.
- **`nav.rs`**: `nav_to_path`, `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`, `select_volume`, `move_cursor`,
  `open_under_cursor`.
- **`file_ops.rs`**: `copy`, `move`, `delete`, `mkdir`, `mkfile`, `refresh`, `select`.
- **`dialogs.rs`**: unified `dialog` tool: open / focus / close / confirm for settings, file-viewer, about, and
  confirmation dialogs.
- **`async_tools.rs`**: `await`, `connect_to_server`, `remove_manual_server`, `upgrade_smb_to_direct`, `set_setting`.
- **`search.rs`**: `search` (drive index), `ai_search` (LLM-driven), and the lazy-load of the search index via
  `spawn_blocking`.
- **`downloads.rs`**: `go_to_latest_download` (resolves via `downloads::commands::go_to_latest_download`, then
  `mcp-nav-to-path` + `mcp-move-cursor`).
- **`operation_log.rs`**: `operations_list`, `operations_get` (short-lived read-only connection over the query API,
  the `commands/operation_log.rs` pattern), `operations_rollback` (dispatches the rollback engine via
  `write_operations::rollback::dispatch_rollback`; returns after dispatch — see `mcp/DETAILS.md` § dispatch-then-poll).
  The pure filter/param parsers and the typed-refusal shape are unit-tested in `operation_log/tests.rs`.
- **`photos.rs`**: `search_photos` (shared `[AiClient, Agent]` read). Shapes the `media_index` read API
  (`search_semantic` / `search_ocr` / `images_with_tag`) into a TEXT-ONLY DTO (no image bytes), resolves the mode like
  the search UI (Auto composes semantic + OCR, degrades to OCR with no CLIP model), reuses `media_index::commands::volume_state`
  for coverage honesty, and returns a typed status (`imageIndexingOff` / `semanticModelNotInstalled` / `ok`). Pure mode
  resolution, hit merging, coverage derivation, and the no-bytes property are unit-tested in-file.
- **`tests.rs`**: unit tests for the dispatcher and shared helpers; per-category tests live alongside their handlers.

## Ack contract

Each action tool: (1) captures a precondition snapshot (typically `snapshot_generation(app)`); (2) emits its event /
runs its command; (3) calls `wait_for_ack(app, signal, DEFAULT_ACK_TIMEOUT)` (default 1500 ms; nav family uses
`NAV_ACK_TIMEOUT` = 5 s); (4) returns `OK` on signal, or `ToolError::internal` naming the missing signal and elapsed
budget on timeout.

`AckSignal` variants, when they fire, and who uses them:

- **`GenerationAdvanced`**: fires when `PaneStateStore.generation` is strictly greater than the captured value. Used by
  pane mutators: `set_view_mode`, `sort`, `toggle_hidden`, `tab`, `nav_*`, auto-confirmed `copy`/`move`/`delete`, and
  `dialog confirm`. NOT `select`/`refresh` (both round-trips).
- **`SoftDialogAppeared(id)`**: fires when a soft dialog with that id is in `SoftDialogTracker`. Used by confirmation
  dialogs from `copy`/`move`/`delete` (`autoConfirm: false`), `mkdir`, `mkfile`, and `dialog open about`.
- **`SoftDialogDisappeared(id)`**: fires when a soft dialog with that id is no longer tracked. Used by
  `dialog close <confirmation>` (the FE `ModalDialog` fires `notifyDialogClosed` on unmount).
- **`WindowAppeared(label)`**: fires when a `webview_windows()` entry matches (exact, or `viewer-*`). Used by
  `dialog open settings|file-viewer` and `dialog focus`.
- **`WindowDisappeared(label)`**: fires when the matching `webview_windows()` entry is gone. Used by
  `dialog close settings` (single-window family).
- **`WindowCountBelow {prefix, threshold}`**: fires when the matching window count is `< threshold`. Used by
  `dialog close file-viewer` (snapshot count, ack when one closes; don't wait for all viewers to vanish).
- **`Any([...])`**: fires on a logical OR over inner signals. Reserved for multi-mode tools.

Polling cadence: 250 ms for state-driven signals (matches the `await` tool); 100 ms for window/soft-dialog signals (both
react faster than a full pane state push).

## `mcp_round_trip` for explicit FE responses

When the backend can't fully validate preconditions (or has to wait on the OS), the tool emits an event with a
`requestId` and waits for the FE to reply via `mcp-response` carrying `{ requestId, ok, error? }`. Response correlation
lives in the pure, unit-tested `parse_mcp_response` in `mod.rs`. Per-tool:

- `move_cursor`, `set_setting` (5 s). The FE verifies the cursor actually landed (filename found, index in range), then
  (move_cursor) flushes the MCP state push (`syncStateToMcpNow`) before replying, so a follow-up `copy`/`move`/`delete`
  reads the new cursor instead of the stale pre-move one. A silent no-op (cursor never moved) was the original
  false-positive-OK bug.
- `select` (5 s, all modes): the FE applies the selection (names mode maps names → indices via the `findFileIndices`
  batch IPC first), then flushes the state push before replying, so a follow-up `copy` reads fresh selection state.
  Missing names come back as the round-trip error.
- `refresh` (5 s): the FE forces a backend re-read via `refreshListing` (local volumes always re-read; watcher-backed
  MTP/SMB short-circuit) and replies once it completes. `OK` means the directory was actually re-read.
- `nav_to_path`: 30 s via `mcp_round_trip_with_timeout`; the FE delays the response until `handleListingComplete` fires.
- `open_under_cursor`: 5 s via `mcp_round_trip_with_timeout`; opening a file delegates to the OS default app, so neither
  `GenerationAdvanced` nor `WindowAppeared` would fire.
- Resources that need FE data use `resource_round_trip` (same pattern, returns the `data` field). Used by
  `cmdr://settings`.

## Agent-supplied paths

`user_path_param(params, key)` for a required path param (extract, missing-param error, tilde expansion);
`expand_user_path(s)` for optional or conditional sites (the `dialog` tool's optional `path`, the `await` tool's `value`
when path-shaped). Both in `mod.rs`. Virtual paths (`mtp://…`) don't start with `~`, so expansion is a no-op for them.

## Empty-operation fast-fail (`file_ops.rs`)

`empty_operation_error` (pure, unit-tested) mirrors the FE fallback semantics: a selection wins; no selection falls back
to the cursor file; cursor on `..` (or an empty pane, where `files` is empty with `total_files <= 1`) means the FE would
silently drop the dialog, so the tool rejects fast. Unsynced state (`path` empty) passes through. Without the `select` /
`move_cursor` pre-reply flush, select → copy reads a stale empty selection and move_cursor → copy reads a stale cursor
(still on `..`), and either wrongly rejects here.
