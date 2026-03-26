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

24 semantic tools grouped by category:
- Navigation (6): `select_volume`, `nav_to_path`, `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`
- Cursor/Selection (3): `move_cursor`, `open_under_cursor`, `select`
- File operations (5): `copy`, `delete`, `mkdir`, `mkfile`, `refresh`
- View (3): `sort`, `toggle_hidden`, `set_view_mode`
- Tabs (1): `tab` (unified: `action` = `new` | `close` | `close_others` | `activate` | `set_pinned`; `tab_id` defaults to active tab for close/close_others/set_pinned, required for activate; `pinned` boolean for set_pinned)
- Dialogs (1): `dialog` (unified open/focus/close)
- App (3): `switch_pane`, `swap_panes`, `quit`
- Search (2): `search` (structured file search across the drive index, optional `scope` for path/exclude filtering), `ai_search` (natural language search using configured LLM, optional `scope` merged with AI-inferred scope)

### Resources (`resources.rs`)

- `cmdr://state`: Complete app state in YAML (both panes, volumes, dialogs)
- `cmdr://dialogs/available`: Static metadata about available dialogs
- `cmdr://indexing`: Drive indexing status in plain text (current phase, timeline history, DB stats). Calls `indexing::get_debug_status()` and formats as human-readable text.

### Executor (`executor.rs`)

 Routes tool calls to implementations. Most tools are fire-and-forget: emit a Tauri event and return "OK" immediately.

Tools where the backend can't fully validate preconditions use `mcp_round_trip`: emit an event with a `requestId`, wait for the frontend to respond via `mcp-response` with `{ requestId, ok, error? }`, and return the actual outcome. Currently used by `nav_to_path` (the frontend knows whether the pane's volume supports local path navigation). Use this pattern for any new tool where the backend would otherwise need to replicate frontend knowledge.

### Configuration (`config.rs`)

Constants and configuration for the MCP server (port, bind address, transport settings). Default port is 9224 for all build types (dev and prod use separate data dirs, so no collision risk).

### Dialog state (`dialog_state.rs`)

`SoftDialogTracker` implementation — tracks which dialogs MCP believes are open. Updated by MCP tool calls; not always in sync with actual Tauri window state (see gotchas).

### State stores

- `PaneStateStore`: Current state of left/right panes (path, files, cursor, selection, tabs)
- `SoftDialogTracker`: Which dialogs MCP thinks are open (in `dialog_state.rs`)
- `SettingsStateStore`: Current settings window state (section, settings, shortcuts)

Frontend syncs state to these stores via Tauri commands (`update_left_pane_state`, `update_pane_tabs`, `mcp_update_settings_sections`, etc.).

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

`PaneStateStore` is always synced (file pane changes frequently). `SettingsStateStore` is only synced when settings window is open (rare). `SoftDialogTracker` is updated by MCP tools themselves. Separating concerns keeps each store simple.

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

### Settings state sync is window-specific

The settings window calls `syncSettingsState()` on mount and section changes. The main window doesn't sync settings state (it doesn't need to). This means `cmdr://state` only includes settings when the settings window is open.

### MCP-settings bridge vs MCP-shortcuts listener

Settings window: full bridge (`mcp-settings-bridge.ts`) syncs all state and handles all MCP events.
Main window: lightweight listener (`mcp-shortcuts-listener.ts`) only handles shortcut changes.
This separation keeps main window overhead minimal.

### Tool execution is async but mostly synchronous

`execute_tool()` is an async function. Most tools are fire-and-forget — they emit a Tauri event and return immediately (for example, "OK: Copy dialog opened" not "OK: Files copied"). Three categories of async tools exist: (1) `mcp_round_trip` tools (`nav_to_path`) that wait up to 5s for the frontend to confirm success/failure, (2) search tools (`search`, `ai_search`) that load the search index via `spawn_blocking` and (for `ai_search`) call the LLM API.

### Error codes are JSON-RPC standard

`INVALID_PARAMS = -32602`, `INTERNAL_ERROR = -32603`, etc. These are defined by the JSON-RPC spec, not MCP. Don't change them.

### Tab state is synced separately from pane state

Tab info (id, path, pinned, active) is synced to `PaneState.tabs` via a separate `update_pane_tabs` command, debounced at ~100ms in the frontend. The `cmdr://state` resource shows a `tabs:` section per pane only when tabs are synced (non-empty). The `tab` tool emits an `mcp-tab` Tauri event that the frontend handles for all tab actions (new, close, close_others, activate, set_pinned).

### Schema version doesn't apply to MCP state

MCP state stores don't have `_schemaVersion` fields. They're runtime-only, not persisted. If the state format changes, just restart the app.
