# MCP server

## Purpose

Expose Cmdr functionality to AI agents via the Model Context Protocol (MCP). Agents can control the app using the same capabilities available to users, no more, no less.

## Architecture

### Server (`server.rs`)

- Runs in a background tokio task spawned at app startup
- Binds `127.0.0.1` only (localhost). Port defaults to ephemeral: when `developer.mcpPort` (the user setting) is 0 (the new default), the server binds `127.0.0.1:0` and asks the kernel for an unused port. When the setting (or `CMDR_MCP_PORT` env) is non-zero, the server binds that port and probes upward up to 100 ports on collision. The `resolve_bind_strategy` helper turns the resolved port into `BindStrategy::Ephemeral` or `BindStrategy::Pinned(port)` and is unit-tested in `server.rs`. The legacy fixed defaults (`19224` prod / `19225` dev) still live in `config.rs::DEFAULT_PORT` as the fallback for `CMDR_MCP_PORT` parse failures and are mirrored in the FE registry for users who want to pin a port.
- Writes the actual bound port to `<data_dir>/mcp.port` atomically (tempfile + fsync + rename) after `bind`. External readers (the `scripts/mcp-call.sh` CLI, E2E fixtures, agent helpers) discover the port from that file; the FE still uses the `get_mcp_port` IPC to read the same in-process atomic. On clean shutdown the file is removed; on crash it stays and readers discover staleness via `ECONNREFUSED` on the contained port. See `port_file.rs` for the read/write/remove API and typed `PortDiscoveryError`.
- Streamable HTTP transport (MCP spec 2025-11-25)
- Endpoints: `POST /mcp` (JSON-RPC), `GET /mcp` (optional SSE), `GET /mcp/health`

### Protocol (`protocol.rs`)

- JSON-RPC 2.0 message parsing
- Routes to `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- Session management (though most clients don't use sessions)

### Tools (`tools.rs`)

31 semantic tools grouped by category:
- Navigation (6): `select_volume` (also accepts MTP volume names), `nav_to_path` (supports `mtp://` paths, skips filesystem existence check), `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`
- Cursor/Selection (3): `move_cursor`, `open_under_cursor`, `select`
- File operations (6): `copy`, `move`, `delete`, `mkdir`, `mkfile`, `refresh`. `copy`/`move` accept optional `autoConfirm` (bool) and `onConflict` (`skip_all`|`overwrite_all`|`rename_all`). `delete` accepts optional `autoConfirm`. When `autoConfirm` is true, the dialog opens and immediately confirms.
- View (3): `sort`, `toggle_hidden`, `set_view_mode`
- Tabs (1): `tab` (unified: `action` = `new` | `close` | `close_others` | `activate` | `set_pinned`; `tab_id` defaults to active tab for close/close_others/set_pinned, required for activate; `pinned` boolean for set_pinned)
- Dialogs (2): `dialog` (unified open/focus/close/confirm). `action: "confirm"` programmatically confirms an open dialog. For `transfer-confirmation`: accepts optional `onConflict`. For `delete-confirmation`: just confirms. `type: "transfer-confirmation"` is the primary name (covers copy and move); `"copy-confirmation"` is accepted as an alias. `open_search_dialog` opens the whole-drive search overlay with optional pre-filled `query`, `mode` (`ai`/`filename`/`regex`), `sizeMin`/`sizeMax` (bytes), `modifiedAfter`/`modifiedBefore` (ISO date), `scope` (chip syntax), `caseSensitive`, `excludeSystemDirs`, and `autoRun` (default true: runs the search after open). Acks on `SoftDialogAppeared("search")` within the 1500 ms budget. **Race-with-close caveat**: if the dialog is mid-close when the event lands, the new mount may race; the ack times out and the tool surfaces a clean failure rather than a false-positive OK (per plan §5.7 risk register).
- App (3): `switch_pane`, `swap_panes`, `quit`
- Search (2): `search` (structured file search across the drive index, optional `scope` for path/exclude filtering), `ai_search` (natural language search using configured LLM, optional `scope` merged with AI-inferred scope)
- Settings (1): `set_setting` (change a setting value via round-trip to frontend)
- Network (3): `connect_to_server` (add a manual SMB server by address, checks TCP reachability), `remove_manual_server` (remove a manually-added server by host ID), `upgrade_smb_to_direct` (upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O; thin wrapper over the existing manual "Connect directly" Tauri command — tries Keychain creds, returns a typed result mirroring `UpgradeResult`)
- Async (1): `await` (poll PaneStateStore until a condition is met: `has_item`, `item_count_gte`, `path`, or `path_contains`. Supports `after_generation` to avoid matching stale state)

### Resources (`resources.rs`)

- `cmdr://state`: Complete app state in YAML (both panes, volumes, dialogs, active `listings` cache, `recentErrors`). Includes MTP volumes with `name` and `id`, and per-pane `volumeId`. SMB volumes appear as structured entries with `name`, `id`, and `smbConnectionState` (`direct` | `os_mount` | `disconnected`) so agents can route the `upgrade_smb_to_direct` tool at the right volumes; non-SMB volumes stay as bare `- {name}` lines. The `listings` section reflects every entry in `LISTING_CACHE` (id, volumeId, path, entry count, ageMs); `recentErrors` is the last 20 directory-listing failures with `atUnixMs`, `listingId`, `volumeId`, `path`, `message` (see `listing_errors.rs` and the freshness contract below). Supports `?include=panes,volumes,dialogs,listings,recentErrors` projection (defaults to all) and `?compact=true` (drops the `files:` list inside each pane while keeping every summary field). Example: `cmdr://state?include=listings,recentErrors` is the minimal payload for "did the last listing succeed?".
- `cmdr://dialogs/available`: Static metadata about available dialogs
- `cmdr://indexing`: Drive indexing status in plain text (current phase, timeline history, DB stats). Calls `indexing::get_debug_status()` and formats as human-readable text.
- `cmdr://settings`: All settings with current values, defaults, types, and constraints. Fetched via round-trip to the frontend (`mcp-get-all-settings` event).
- `cmdr://logs`: Tail of the live `cmdr.log` file. Query: `?since=<iso>&filter=<substring>&limit=<n>`. `limit` defaults to 100, capped at 1000; `filter` is a case-sensitive substring match (no regex dep); `since` drops lines whose ISO-8601 timestamp prefix is ≤ the given value (lines without a timestamp prefix, like a Rust panic, are kept). Reads the last ~5 MB of the file from the end so a 50 MB rotated log doesn't blow up MCP memory. Returns oldest-first.

### Executor (`executor/`)

Directory module split by tool category. `mod.rs` contains the main `execute_tool()` dispatcher, shared types (`ToolResult`, `ToolError`), and the `mcp_round_trip` helpers. Category files:
- `app.rs`: quit, switch_pane, swap_panes, tab
- `view.rs`: toggle_hidden, set_view_mode, sort
- `nav.rs`: navigation commands (with and without params)
- `file_ops.rs`: copy, move, delete, mkdir, mkfile, refresh, select
- `dialogs.rs`: unified dialog open/focus/close/confirm
- `async_tools.rs`: await, connect_to_server, remove_manual_server, upgrade_smb_to_direct, set_setting
- `search.rs`: search index loading, search, ai_search

**Action-tool ack contract.** Every fire-and-forget action tool now waits for a backend ack signal before returning `OK`. Previously the tool returned `OK` the instant the event was dispatched; if the FE was stalled (modal blocking input, error pane up, race during startup), the action was silently dropped and MCP reported success anyway. The ack contract makes `OK` a meaningful promise: the FE has actually processed the dispatched action.

The mechanism lives in `executor/ack.rs`. Each tool:

1. Captures a precondition snapshot (typically `snapshot_generation(app)`).
2. Emits its existing event / runs its existing command.
3. Calls `wait_for_ack(app, signal, DEFAULT_ACK_TIMEOUT)` — default 1500 ms.
4. Returns the original `OK` string on success, or a typed `ToolError::internal` whose message names the missing signal and the elapsed budget on timeout.

`AckSignal` variants:

| Variant                  | Fires when                                                              | Used by                                                                                                                          |
| ------------------------ | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `GenerationAdvanced`     | `PaneStateStore.generation` is strictly greater than the captured value | Anything that mutates pane state (`select`, `set_view_mode`, `sort`, `toggle_hidden`, `tab`, `nav_*`, auto-confirmed `copy`/`move`/`delete`, `dialog confirm`). NOT `refresh` — see TODO note below.                                  |
| `SoftDialogAppeared(id)` | A soft dialog with that ID is in `SoftDialogTracker`                    | Confirmation dialogs from `copy`/`move`/`delete` (autoConfirm: false), `mkdir`, `mkfile`; `dialog open about`                    |
| `SoftDialogDisappeared(id)` | A soft dialog with that ID is no longer in `SoftDialogTracker`        | `dialog close <confirmation>` — the FE `ModalDialog` fires `notifyDialogClosed` on unmount, so the tracker reflects the close even when cancel doesn't bump pane generation |
| `WindowAppeared(label)`  | A `webview_windows()` entry matches the label (exact, or `viewer-*`)    | `dialog open settings|file-viewer`, `dialog focus`                                                                               |
| `WindowDisappeared(label)` | The matching `webview_windows()` entry is gone                        | `dialog close settings` (single window family)                                                                                   |
| `WindowCountBelow {prefix, threshold}` | Count of matching windows is `< threshold`                | `dialog close file-viewer` — snapshot the count, ack when one closes (don't wait for *all* viewers to vanish)                    |
| `Any([...])`             | Logical OR — any inner signal fires                                     | Reserved for future multi-mode tools                                                                                              |

The polling cadence is 250 ms for state-driven signals (matching the existing `await` tool) and 100 ms for window/soft-dialog signals (both react faster than a full pane state push).

**Gotcha/Why**: `dialog close <settings>` requires the settings window to listen for the `mcp-settings-close` event and close itself (`apps/desktop/src/routes/settings/+page.svelte`). Without that listener the backend keeps polling for `WindowDisappeared("settings")` and times out at 1500 ms while the window stays put. Same shape applies if you add new window-based dialogs: the FE side has to opt in.

The 1500 ms budget is a backend-side latency budget, not a client-facing knob: MCP clients shouldn't have to tune ack timeouts. Bump it per-call via the `Duration` argument to `wait_for_ack` if a specific operation has a known higher latency floor; don't expose it as a tool parameter. The navigation family (`nav_to_parent`, `nav_back`, `nav_forward`, `open_under_cursor`) uses `NAV_ACK_TIMEOUT` (5 s) because a parent/back navigation can land on an SMB or MTP path whose directory listing takes a few seconds even on success. `nav_to_path` and `select_volume` use higher round-trip budgets via `mcp_round_trip_with_timeout` and aren't part of the ack-contract family.

**Caveat: `GenerationAdvanced` isn't a per-action proof.** The snapshot-dispatch-wait sequence proves the FE pushed pane state *recently after* dispatch — not that the FE specifically handled our event. An unrelated push between snapshot and dispatch (the other pane's watcher, a tab refresh) will satisfy the signal as a false positive. The window is microseconds wide and the FE was clearly running (something pushed), so this is a much weaker false-positive class than the original "always OK" bug. If a real false positive ever surfaces, the fix is to switch the affected tool to `mcp_round_trip` with a request id. Tagged `TODO(mcp-ack):` in `ack.rs`.

**Note on `update_pane_tabs`.** Tab changes flow through this command (not `set_left`/`set_right`). It delegates to `PaneStateStore::set_tabs`, which is the single place tab mutation + generation bump live. Add any new tab-mutating path through that helper, or the `tab` MCP tool's ack will time out.

**Known TODO: `refresh` is still fire-and-forget.** The FE skips the `update_*_pane_state` push when the new listing is byte-identical to the cached state (correct behavior for state sync but no signal for the ack helper). Switching `refresh` to `mcp_round_trip` is the right follow-up, but requires a FE change to emit `mcp-response` after every re-list. The original "FE silently dropping the action" bug is less acute for refresh than for destructive tools, so this stays open. Search the codebase for `TODO(mcp-ack):` to find this and any future similar cases.

Tools where the backend can't fully validate preconditions use `mcp_round_trip`: emit an event with a `requestId`, wait for the frontend to respond via `mcp-response` with `{ requestId, ok, error? }`, and return the actual outcome. Used by `move_cursor` and `set_setting` (5 s timeout). `nav_to_path` uses `mcp_round_trip_with_timeout` with a 30 s timeout because it waits for the directory listing to complete (the frontend delays the response until `handleListingComplete` fires in FilePane). `open_under_cursor` also uses `mcp_round_trip_with_timeout` (5 s) because Enter on a non-directory file delegates to the OS default app (no pane state push, no viewer window), so neither `GenerationAdvanced` nor `WindowAppeared` would fire — the FE explicitly awaits `handleNavigate(entry)` and signals completion. Resources that need frontend data use `resource_round_trip` (same pattern but returns `data` from the response). Used by `cmdr://settings`. Use these patterns for any new tool/resource where the backend would otherwise need to replicate frontend knowledge.

### Configuration (`config.rs`)

Constants and configuration for the MCP server (port, bind address, transport settings). The default port for users who pin (setting `developer.mcpPort` to non-zero) is build-mode-dependent: 19224 in prod, 19225 in dev. Different defaults so a dev session and an installed prod build don't collide when both pin. With the post-instance-isolation default of `developer.mcpPort = 0`, the server binds ephemeral and these constants only matter as the pinned-mode fallback. Mirrored in the FE registry; both are in 10000–29999 per AGENTS.md. See [`/docs/tooling/instance-isolation.md`](../../../../../docs/tooling/instance-isolation.md) for the cross-resource view.

### Dialog state (`dialog_state.rs`)

`SoftDialogTracker` implementation: tracks which dialogs MCP believes are open. Updated by MCP tool calls; not always in sync with actual Tauri window state (see gotchas).

### State stores

- `PaneStateStore`: Current state of left/right panes (path, files, cursor, selection, tabs, type-to-jump). Includes a monotonic `generation` counter (AtomicU64) bumped on every `set_left`/`set_right`. Exposed in `cmdr://state` as `generation:` and used by the `await` tool's `after_generation` param to avoid matching stale state. The optional `typeToJump` field (buffer, indicatorVisible, indicatorStale, lastMatchedName) mirrors the per-pane type-to-jump state when a buffer or indicator is live, so MCP-driven tests can assert the feature without DOM access.
- `SoftDialogTracker`: Which dialogs MCP thinks are open (in `dialog_state.rs`)
- `listing_errors`: Bounded ring buffer (capacity 20) of the most recent `listing-error` events. Populated from `file_system::listing::streaming` at both `emit_error` sites — see the call to `crate::mcp::listing_errors::record(...)` right before the FE event fires, so MCP-visible state matches what the FE saw. Surfaced as `recentErrors:` in `cmdr://state`. **Freshness contract**: the buffer holds the absolute-newest 20 errors process-wide; on a busy session older errors silently drop off, so test scenarios that need older context should snapshot earlier and compare. Cancellations are not recorded — only failures.

Frontend syncs state to these stores via Tauri commands (`update_left_pane_state`, `update_pane_tabs`, etc.). Settings are fetched on-demand via round-trip to the frontend rather than stored in a state store.

### Tests (`tests/`)

Directory module split by test category:
- `protocol_tests.rs`: tool name validation, schema checks, tool count
- `resource_tests.rs`: resource URI validation, count, mime types
- `tool_category_tests.rs`: tool existence by category, schema checks
- `security_tests.rs`: shell injection, forbidden tool patterns, input injection
- `request_response_tests.rs`: McpRequest parsing, McpResponse serialization
- `pane_state_tests.rs`: PaneStateStore CRUD, edge cases, concurrency, PaneFileEntry serialization
- `spec_compliance_tests.rs`: MCP spec 2025-11-25 compliance, origin validation, SSE events

## Key decisions

### MCP action tools wait for backend ack before returning success

**Decision (May 2026):** Every fire-and-forget action tool waits for a typed ack signal (`AckSignal::GenerationAdvanced`, `SoftDialogAppeared`/`Disappeared`, `WindowAppeared`/`Disappeared`, `WindowCountBelow`, or `Any`) within a 1500 ms budget (5 s for the nav family) before returning `OK`. On timeout, the tool returns a `ToolError::internal` whose message names the missing signal and elapsed budget.

**Why.** Real QA hit a paper-cut: MCP tools were returning `OK` while the FE was stalled (modal blocking input, error pane up, race during startup), so the dispatched action was silently dropped. That made MCP unreliable as an automation surface. The ack contract makes `OK` a real promise: the FE actually processed the dispatched action.

**Why 1500 ms.** Most state pushes complete within ~100–300 ms in practice (FE debouncing, IPC round-trip). 1500 ms gives a generous margin for the slow cases (cold start, large directory listings) while still failing fast when the FE genuinely isn't responding. Latency-sensitive tools (`nav_to_path`) keep their existing higher budgets via `mcp_round_trip_with_timeout`.

**Why not a per-tool client-facing timeout knob.** The timeout is a backend-side latency budget, not a client concern. MCP clients shouldn't have to tune it per call — they expect tools to either succeed or report a clear failure.

### Why agent-centric API?

The original design mirrored keyboard shortcuts (43 tools like `nav_up`, `nav_down`). This forced agents to make dozens of calls to find a file. The agent-centric redesign (Jan 2026) consolidated to 24 semantic tools (`move_cursor(index=42)`, `nav_to_path("/Users")`). This reduced round-trips from 6+ reads to 1 (`cmdr://state` resource).

### Why YAML over JSON for resources?

LLMs consume resources, not machines. YAML is 30-40% smaller and more readable. The `cmdr://state` resource is optimized for LLM token usage, not parsing speed.

### Why plain text responses?

Tool results and resource content are consumed by LLMs, not parsed by code. Output doesn't need to be JSON, YAML, or any structured format. Anything that reads well to a human and is concise works. Tool results are plain text (`"OK: Navigated to /Users"`, aligned columns for search results), resources use YAML or plain text. Errors are still JSON-RPC error objects, but the `content` field is plain text. Optimize for readability and token efficiency, not parseability.

### Why stateful architecture?

Without state, resources would need to query the frontend on every read (slow, async). Storing state in Rust allows synchronous reads. The frontend syncs state after meaningful changes (file load, cursor move, selection).

### Why no file system access?

Security via parity: agents can only do what users can do. Giving agents `fs.read`/`fs.write` would violate this. Agents navigate the UI just like users, using `move_cursor`, `open_under_cursor`, etc.

### Why localhost only?

Binding to `0.0.0.0` would expose the server to the network. An attacker could quit the app, change settings, or navigate to sensitive directories. Localhost binding ensures only local processes can connect.

### Why separate state stores?

`PaneStateStore` is always synced (file pane changes frequently). `SoftDialogTracker` is updated by MCP tools themselves. Separating concerns keeps each store simple. Settings are fetched on-demand via `resource_round_trip` rather than stored, since they rarely change and can be queried from the frontend when needed.

## Gotchas

### Server lifecycle is managed at runtime

`start_mcp_server()` binds the port and spawns a tokio task, storing the `JoinHandle` in a static `MCP_HANDLE`. Port-binding strategy comes from `resolve_bind_strategy(port)`: a 0 setting (the new default) means `BindStrategy::Ephemeral`, which binds `127.0.0.1:0` and reads the assigned port via `local_addr()`. A non-zero setting means `BindStrategy::Pinned(port)`, which uses `bind_with_probe()` to try tokio `TcpListener::bind` directly and retry up to 100 ports on collision (avoids the TOCTOU race of checking with a sync listener then re-binding async). The actual bound port is stored in `MCP_ACTUAL_PORT`. After `bind`, `write_port_file` lands `<data_dir>/mcp.port` atomically (tempfile + fsync + rename, see `port_file.rs`) so external readers can discover the port without IPC; the data dir is cached in `MCP_PORT_FILE_DIR` for the shutdown path. The frontend queries the in-process atomic via `get_mcp_port()` and shows "(ephemeral)" when the setting was 0 or "(port N was in use)" when the pinned port differs from the bound one. The server can be started/stopped live via `set_mcp_enabled` and `set_mcp_port` Tauri commands, no app restart needed. `stop_mcp_server()` aborts the task, resets `MCP_ACTUAL_PORT` to 0, and removes `<data_dir>/mcp.port` (best-effort: stale file is not a correctness bug). `is_mcp_running()` checks whether the handle exists. At startup, `start_mcp_server_background()` wraps the async start in a fire-and-forget spawn. If the server crashes mid-serve, `MCP_ACTUAL_PORT` resets to 0 but the on-disk file may linger; external readers retry on `ECONNREFUSED`. Check logs for "MCP server crashed" errors.

### Live MCP control only works from the settings window

`McpServerSection.svelte` subscribes to `developer.mcpEnabled` and `developer.mcpPort` changes and calls the Tauri commands directly. The main window's `settings-applier.ts` intentionally does NOT handle these settings to avoid double-firing (both windows receive setting change events). This means if an MCP tool changes `developer.mcpEnabled` via the settings bridge while the settings window is closed, the setting is saved but the server state doesn't change until the next app restart. This is acceptable, since an MCP tool toggling its own server is circular.

### State sync is best-effort

Frontend calls `update_left_pane_state()` after loading files, but there's no guarantee it completes before an MCP resource read. In practice, updates are fast and this isn't an issue. If stale data is a concern, add explicit sync waits.

### Dialog state is "soft"

`SoftDialogTracker` stores which dialogs MCP thinks are open, but if a dialog is closed manually (not via MCP), the tracker isn't updated. The `cmdr://state` resource double-checks reality by querying Tauri windows.

### View mode affects resource detail

`cmdr://state` shows file details differently based on view mode:
- Full mode: all file info inline (`i:42 f package.json 1.2K 2025-01-10`)
- Brief mode: only cursor file gets details, rest are just names (`i:42 f package.json`)

This prevents overwhelming agents with data they can't see in the UI.

### Pane state includes pagination

Large directories (50k+ files) are paginated. The `totalFiles`, `loadedStart`, `loadedEnd` fields indicate what's currently loaded. Agents must use `scroll_to(index)` to load different regions.

### Resources don't require initialization

Unlike tools (which need a session via `initialize`), resources can be read immediately after server start. This is by design for debugging with curl.

### Settings are fetched on-demand, not synced

The `cmdr://settings` resource and `set_setting` tool both use round-trips to the main window frontend. This means settings are always fetched fresh from the source of truth, rather than being synced to a Rust-side store. The tradeoff is a ~5s timeout if the frontend is unresponsive, but this avoids stale state issues.

### `select_volume` polls for `volume_name` match, not path change

`select_volume` polls the target pane's `volume_name` in `PaneStateStore` until it equals the requested name. Two consequences worth knowing:

- **Re-selecting the same volume is an instant no-op** (the first poll matches). The previous "wait for path to change" formulation timed out for ~30s in this case.
- **Virtual volumes like `Network`** work correctly even though the pane path doesn't necessarily change. The volume_name does change, which is what we check.

`volume_name` flows through `PaneState` from the FE via `update_left_pane_state` / `update_right_pane_state` on every state push (`FilePane.svelte`).

### Tool execution is async; action tools wait for ack

`execute_tool()` is an async function. Action tools follow the ack contract (see "Action-tool ack contract" above): dispatch the event, then `wait_for_ack` for a small backend-side signal before returning. The tool's reported "OK" thus means "the FE accepted the dispatched action," not "the underlying operation completed." For long-running operations (a copy of 10 GB), the agent still has to poll via the `await` tool to observe completion. The ack-contract change made the FE-accepted line meaningful — pre-May 2026, the tool returned `OK` even when the FE wasn't listening.

Three categories of latency-sensitive tools exist beyond the ack contract: (1) `mcp_round_trip` tools (`nav_to_path`, `move_cursor`, `set_setting`, `open_under_cursor`) that wait up to 5–30 s for an explicit `mcp-response` event with success/failure, (2) search tools (`search`, `ai_search`) that load the search index via `spawn_blocking` and (for `ai_search`) call the LLM API, (3) `select_volume` which polls until the target pane's `volume_name` matches.

### Error codes are JSON-RPC standard

`INVALID_PARAMS = -32602`, `INTERNAL_ERROR = -32603`, etc. These are defined by the JSON-RPC spec, not MCP. Don't change them.

### Tab state is synced separately from pane state

Tab info (id, path, pinned, active) is synced to `PaneState.tabs` via a separate `update_pane_tabs` command, debounced at ~100ms in the frontend. The `cmdr://state` resource shows a `tabs:` section per pane only when tabs are synced (non-empty). The `tab` tool emits an `mcp-tab` Tauri event that the frontend handles for all tab actions (new, close, close_others, activate, set_pinned).

### Schema version doesn't apply to MCP state

MCP state stores don't have `_schemaVersion` fields. They're runtime-only, not persisted. If the state format changes, just restart the app.
