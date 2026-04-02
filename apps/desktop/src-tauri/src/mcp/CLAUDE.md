# MCP server

## Purpose

Expose Cmdr functionality to AI agents via the Model Context Protocol (MCP). Agents can control the app using the same capabilities available to users—no more, no less.

## Architecture

### Server (`server.rs`)

- Runs in a background tokio task spawned at app startup
- Binds to `127.0.0.1:9224` by default (localhost only for security). If the port is taken, auto-probes upward (up to 100 ports) to find an available one
- Streamable HTTP transport (MCP spec 2025-11-25)
- Endpoints: `POST /mcp` (JSON-RPC), `GET /mcp` (optional SSE), `GET /mcp/health`

### Protocol (`protocol.rs`)

- JSON-RPC 2.0 message parsing
- Routes to `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- Session management (though most clients don't use sessions)

### Tools (`tools.rs`)

27 semantic tools grouped by category:
- Navigation (6): `select_volume` (also accepts MTP volume names), `nav_to_path` (supports `mtp://` paths, skips filesystem existence check), `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`
- Cursor/Selection (3): `move_cursor`, `open_under_cursor`, `select`
- File operations (6): `copy`, `move`, `delete`, `mkdir`, `mkfile`, `refresh`. `copy`/`move` accept optional `autoConfirm` (bool) and `onConflict` (`skip_all`|`overwrite_all`|`rename_all`). `delete` accepts optional `autoConfirm`. When `autoConfirm` is true, the dialog opens and immediately confirms.
- View (3): `sort`, `toggle_hidden`, `set_view_mode`
- Tabs (1): `tab` (unified: `action` = `new` | `close` | `close_others` | `activate` | `set_pinned`; `tab_id` defaults to active tab for close/close_others/set_pinned, required for activate; `pinned` boolean for set_pinned)
- Dialogs (1): `dialog` (unified open/focus/close/confirm). `action: "confirm"` programmatically confirms an open dialog. For `transfer-confirmation`: accepts optional `onConflict`. For `delete-confirmation`: just confirms. `type: "transfer-confirmation"` is the primary name (covers copy and move); `"copy-confirmation"` is accepted as an alias.
- App (3): `switch_pane`, `swap_panes`, `quit`
- Search (2): `search` (structured file search across the drive index, optional `scope` for path/exclude filtering), `ai_search` (natural language search using configured LLM, optional `scope` merged with AI-inferred scope)
- Settings (1): `set_setting` (change a setting value via round-trip to frontend)
- Async (1): `await` (poll PaneStateStore until a condition is met — `has_item`, `item_count_gte`, `path`, or `path_contains`. Supports `after_generation` to avoid matching stale state)

### Resources (`resources.rs`)

- `cmdr://state`: Complete app state in YAML (both panes, volumes, dialogs). Includes MTP volumes with `name` and `id`, and per-pane `volumeId`.
- `cmdr://dialogs/available`: Static metadata about available dialogs
- `cmdr://indexing`: Drive indexing status in plain text (current phase, timeline history, DB stats). Calls `indexing::get_debug_status()` and formats as human-readable text.
- `cmdr://settings`: All settings with current values, defaults, types, and constraints. Fetched via round-trip to the frontend (`mcp-get-all-settings` event).

### Executor (`executor.rs`)

 Routes tool calls to implementations. Most tools are fire-and-forget: emit a Tauri event and return "OK" immediately.

Tools where the backend can't fully validate preconditions use `mcp_round_trip`: emit an event with a `requestId`, wait for the frontend to respond via `mcp-response` with `{ requestId, ok, error? }`, and return the actual outcome. Used by `set_setting` (5s timeout). `nav_to_path` uses `mcp_round_trip_with_timeout` with a 30s timeout because it waits for the directory listing to complete (the frontend delays the response until `handleListingComplete` fires in FilePane). Resources that need frontend data use `resource_round_trip` (same pattern but returns `data` from the response). Used by `cmdr://settings`. Use these patterns for any new tool/resource where the backend would otherwise need to replicate frontend knowledge.

### Configuration (`config.rs`)

Constants and configuration for the MCP server (port, bind address, transport settings). Default port is 9224 for all build types (dev and prod use separate data dirs, so no collision risk).

### Dialog state (`dialog_state.rs`)

`SoftDialogTracker` implementation — tracks which dialogs MCP believes are open. Updated by MCP tool calls; not always in sync with actual Tauri window state (see gotchas).

### State stores

- `PaneStateStore`: Current state of left/right panes (path, files, cursor, selection, tabs). Includes a monotonic `generation` counter (AtomicU64) bumped on every `set_left`/`set_right`. Exposed in `cmdr://state` as `generation:` and used by the `await` tool's `after_generation` param to avoid matching stale state.
- `SoftDialogTracker`: Which dialogs MCP thinks are open (in `dialog_state.rs`)

Frontend syncs state to these stores via Tauri commands (`update_left_pane_state`, `update_pane_tabs`, etc.). Settings are fetched on-demand via round-trip to the frontend rather than stored in a state store.

## Key decisions

### Why agent-centric API?

The original design mirrored keyboard shortcuts (43 tools like `nav_up`, `nav_down`). This forced agents to make dozens of calls to find a file. The agent-centric redesign (Jan 2026) consolidated to 24 semantic tools (`move_cursor(index=42)`, `nav_to_path("/Users")`). This reduced round-trips from 6+ reads to 1 (`cmdr://state` resource).

### Why YAML over JSON for resources?

LLMs consume resources, not machines. YAML is 30-40% smaller and more readable. The `cmdr://state` resource is optimized for LLM token usage, not parsing speed.

### Why plain text responses?

Tool results and resource content are consumed by LLMs, not parsed by code. Output doesn't need to be JSON, YAML, or any structured format — anything that reads well to a human and is concise works. Tool results are plain text (`"OK: Navigated to /Users"`, aligned columns for search results), resources use YAML or plain text. Errors are still JSON-RPC error objects, but the `content` field is plain text. Optimize for readability and token efficiency, not parseability.

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

`start_mcp_server()` binds the port and spawns a tokio task, storing the `JoinHandle` in a static `MCP_HANDLE`. If the configured port is taken, it auto-probes upward (up to 100 ports) and stores the actual bound port in `MCP_ACTUAL_PORT`. The frontend queries this via `get_mcp_port()` and shows a notice when it differs from the configured port. The server can be started/stopped live via `set_mcp_enabled` and `set_mcp_port` Tauri commands — no app restart needed. `stop_mcp_server()` aborts the task and resets `MCP_ACTUAL_PORT` to 0. `is_mcp_running()` checks whether the handle exists. At startup, `start_mcp_server_background()` wraps the async start in a fire-and-forget spawn. If the server crashes, the app continues but MCP stops working. Check logs for "MCP server crashed" errors.

### Live MCP control only works from the settings window

`McpServerSection.svelte` subscribes to `developer.mcpEnabled` and `developer.mcpPort` changes and calls the Tauri commands directly. The main window's `settings-applier.ts` intentionally does NOT handle these settings to avoid double-firing (both windows receive setting change events). This means if an MCP tool changes `developer.mcpEnabled` via the settings bridge while the settings window is closed, the setting is saved but the server state doesn't change until the next app restart. This is acceptable — an MCP tool toggling its own server is circular.

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

### `select_volume` times out when re-selecting the same volume

`select_volume` polls the target pane's path in `PaneStateStore` until it changes. If the pane is already on the requested volume (same path), no change is detected and the tool times out after 30s. This is harmless — re-selecting the same volume is a no-op.

### Tool execution is async but mostly synchronous

`execute_tool()` is an async function. Most tools are fire-and-forget — they emit a Tauri event and return immediately (for example, "OK: Copy dialog opened" not "OK: Files copied"). This applies even with `autoConfirm: true` — the tool returns when the operation starts, not when it completes. Three categories of async tools exist: (1) `mcp_round_trip` tools (`nav_to_path`) that wait up to 5s for the frontend to confirm success/failure, (2) search tools (`search`, `ai_search`) that load the search index via `spawn_blocking` and (for `ai_search`) call the LLM API.

### Error codes are JSON-RPC standard

`INVALID_PARAMS = -32602`, `INTERNAL_ERROR = -32603`, etc. These are defined by the JSON-RPC spec, not MCP. Don't change them.

### Tab state is synced separately from pane state

Tab info (id, path, pinned, active) is synced to `PaneState.tabs` via a separate `update_pane_tabs` command, debounced at ~100ms in the frontend. The `cmdr://state` resource shows a `tabs:` section per pane only when tabs are synced (non-empty). The `tab` tool emits an `mcp-tab` Tauri event that the frontend handles for all tab actions (new, close, close_others, activate, set_pinned).

### Schema version doesn't apply to MCP state

MCP state stores don't have `_schemaVersion` fields. They're runtime-only, not persisted. If the state format changes, just restart the app.
