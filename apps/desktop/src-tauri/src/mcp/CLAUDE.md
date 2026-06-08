# MCP server

## Purpose

Expose Cmdr functionality to AI agents via the Model Context Protocol (MCP). Agents can control the app using the same capabilities available to users, no more, no less.

## Architecture

### Server (`server.rs`)

- Runs in a background tokio task spawned at app startup
- Binds `127.0.0.1` only (localhost). Port defaults to ephemeral: when `developer.mcpPort` (the user setting) is 0 (the new default), the server binds `127.0.0.1:0` and asks the kernel for an unused port. A non-zero setting pins that port. `resolve_bind_strategy` turns the resolved port into `BindStrategy::Ephemeral` or `BindStrategy::Pinned(port)`; `bind_listener` then binds it under one of two `BindMode`s: **`ProbeOnCollision`** (startup auto-start — probe upward up to 100 ports so *a* server comes up when nobody's watching) or **`Exact`** (interactive settings changes — fail with `BindError::PortInUse` so the user is told their chosen port is busy instead of silently landing elsewhere). Both helpers are unit-tested in `server.rs`. The legacy fixed defaults (`19224` prod / `19225` dev) still live in `config.rs::DEFAULT_PORT` as the fallback for `CMDR_MCP_PORT` parse failures and are mirrored in the FE registry for users who want to pin a port.
- Writes the actual bound port to `<data_dir>/mcp.port` atomically (tempfile + fsync + rename, mode 0o600) after `bind`, plus a fresh per-instance bearer token to `<data_dir>/mcp.token` (same atomic write, 0o600). External readers (the `scripts/mcp-call.sh` CLI, E2E fixtures, agent helpers) discover the port and token from those files and send `Authorization: Bearer <token>` on every request; the FE still uses the `get_mcp_port` / `get_mcp_token` IPC to read the same in-process state. On clean shutdown both files are removed and the in-process token is cleared; on crash they stay and readers discover staleness via `ECONNREFUSED`. See `port_file.rs` for the `write_port_file` / `write_secret_file` / read / remove API and typed `PortDiscoveryError`.
- **Auth**: every `/mcp` request runs `validate_origin` (browser-CSRF / DNS-rebinding layer). The bearer token is then required only for the narrow set of calls that bypass the user's in-app confirmation dialog — see the Authentication section below. `validate_token` reads `Authorization: Bearer <token>`, compares against the in-process token in constant time, and returns `Err(())` on missing/empty/mismatched; the POST handler turns that into a friendly in-band JSON-RPC error via `auto_confirm_token_required_response` (HTTP 200, **not** 401 — see Authentication for why). It fails closed if no token is set. `GET /mcp/health` and `GET /mcp` (SSE) are unauthenticated.
- Streamable HTTP transport (MCP spec 2025-11-25)
- Endpoints: `POST /mcp` (JSON-RPC), `GET /mcp` (optional SSE), `GET /mcp/health`

### Protocol (`protocol.rs`)

- JSON-RPC 2.0 message parsing
- Routes to `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- Session management (though most clients don't use sessions)

### Tools (`tools.rs`)

**Param naming is camelCase** (`tabId`, `timeoutSeconds`, `sizeMin`, `autoConfirm`). Tool names stay snake_case. Don't add snake_case params; agents pattern-match across tools and every inconsistency is a guessed-wrong call.

32 semantic tools grouped by category:
- Navigation (6): `select_volume` (also accepts MTP volume names), `nav_to_path` (accepts absolute, `~`-relative, and virtual `mtp://` / `smb://` paths; virtual paths skip the local existence check, local paths get a timed one — see `executor/mod.rs::validate_path_exists`), `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`
- Cursor/Selection (3): `move_cursor` (honest errors: a missing filename or out-of-range index is a round-trip failure, never a false OK), `open_under_cursor`, `select` (index ranges, `all`, `count: 0` to clear, or `names: [...]`; every mode is a round-trip that replies after the new selection landed in `PaneStateStore`, and the names mode errors listing any names not in the listing. Focuses the target pane — both the backend store AND the FE focused pane, so a follow-up focused-pane operation (copy/delete) acts on the pane you just selected in)
- File operations (6): `copy`, `move`, `delete`, `mkdir`, `mkfile`, `refresh` (a round-trip that forces a backend re-read of the focused pane's listing — local volumes re-read from disk; watcher-backed MTP/SMB listings short-circuit). `copy`/`move`/`delete` fast-fail with the real cause when there's nothing to act on (no selection and the cursor is on `..`, or the pane shows no files) instead of a misleading ack timeout. `copy`/`move` accept optional `autoConfirm` (bool) and `onConflict` (`skip_all`|`overwrite_all`|`rename_all`). `onConflict` governs clashing **files only** — folders always merge (a source folder landing on a same-named dest folder merges into it; the policy then applies to the files inside). `delete` accepts optional `autoConfirm`. When `autoConfirm` is true, the dialog opens and immediately confirms.
- View (3): `sort`, `toggle_hidden`, `set_view_mode`
- Tabs (1): `tab` (unified: `action` = `new` | `close` | `close_others` | `activate` | `set_pinned`; `tabId` defaults to active tab for close/close_others/set_pinned, required for activate; `pinned` boolean for set_pinned)
- Dialogs (2): `dialog` (unified open/focus/close/confirm). `action: "confirm"` programmatically confirms an open dialog. For `transfer-confirmation`: accepts optional `onConflict`. For `delete-confirmation`: just confirms. `type: "transfer-confirmation"` is the primary name (covers copy and move); `"copy-confirmation"` is accepted as an alias. `open_search_dialog` opens the whole-drive search overlay with optional pre-filled `query`, `mode` (`ai`/`filename`/`regex`), `sizeMin`/`sizeMax` (bytes), `modifiedAfter`/`modifiedBefore` (ISO date), `scope` (chip syntax), `caseSensitive`, `excludeSystemDirs`, and `autoRun` (default true: runs the search after open). Acks on `SoftDialogAppeared("search")` within the 1500 ms budget. **Race-with-close caveat**: if the dialog is mid-close when the event lands, the new mount may race; the ack times out and the tool surfaces a clean failure rather than a false-positive OK (per plan §5.7 risk register).
- App (3): `switch_pane`, `swap_panes`, `quit`
- Search (2): `search` (structured file search across the drive index, optional `scope` for path/exclude filtering), `ai_search` (natural language search using configured LLM, optional `scope` merged with AI-inferred scope)
- Settings (1): `set_setting` (change a setting value via round-trip to frontend)
- Network (3): `connect_to_server` (add a manual SMB server by address, checks TCP reachability), `remove_manual_server` (remove a manually-added server by host ID), `upgrade_smb_to_direct` (upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O; thin wrapper over the existing manual "Connect directly" Tauri command — tries Keychain creds, returns a typed result mirroring `UpgradeResult`)
- Async (1): `await` (poll PaneStateStore until a condition is met: `has_item`, `not_has_item`, `item_count_gte`, `item_count_lte`, `path`, or `path_contains` — the absence conditions are for "wait until the delete finished" flows. `~` expands in path-condition values. Supports `afterGeneration` to avoid matching stale state, `timeoutSeconds` up to 60)
- Downloads (1): `go_to_latest_download` (no args; navigates the focused pane to `~/Downloads` and selects the most recently observed eligible file. Errors when no eligible file exists or FDA is missing. Reuses the same backend code path as the `⌘J` shortcut and the `go_to_latest_download` Tauri command, then drives `mcp-nav-to-path` + `mcp-move-cursor` round-trips for the navigation + cursor placement)

### Resources (`resources/`)

Directory module split by resource. `resources/mod.rs` is the shared spine: the registry (`get_all_resources`), URI/query parsing (`split_uri`, `parse_query`), the `read_resource` dispatch, the `resource_round_trip` helper, and the `cmdr://state` + `cmdr://dialogs/available` builders. The two independently-evolving plain-text builders live in their own files: `resources/logs.rs` (`cmdr://logs`: `LogOptions`, the `LOG_*` consts, `parse_log_options`, `read_log_tail`, `select_log_lines`, `line_timestamp_passes_since`) and `resources/indexing.rs` (`cmdr://indexing`: `build_indexing_status_text`, `format_duration_human`, `format_number`). Tests for each live in the `tests/` directory (see below).

- `cmdr://state`: Complete app state in YAML (both panes, volumes, dialogs, active `listings` cache, `recentErrors`). Includes MTP volumes with `name` and `id`, and per-pane `volumeId`. SMB volumes appear as structured entries with `name`, `id`, and `smbConnectionState` (`direct` | `os_mount` | `disconnected`) so agents can route the `upgrade_smb_to_direct` tool at the right volumes; non-SMB volumes stay as bare `- {name}` lines. The `listings` section reflects every entry in `LISTING_CACHE` (id, volumeId, path, entry count, ageMs); `recentErrors` is the last 20 directory-listing failures with `atUnixMs`, `listingId`, `volumeId`, `path`, `message` (see `listing_errors.rs` and the freshness contract below); the `path` and `message` fields are run through `crate::redact::redact_line` before serialization, since failed-listing errors can carry SMB URIs / home paths the user never saw rendered. Supports `?include=panes,volumes,dialogs,listings,recentErrors` projection (defaults to all) and `?compact=true` (drops the `files:` list inside each pane while keeping every summary field). Example: `cmdr://state?include=listings,recentErrors` is the minimal payload for "did the last listing succeed?".
- `cmdr://dialogs/available`: Static metadata about available dialogs
- `transfers:` (inside `cmdr://state`, also `?include=transfers`): in-flight copy/move/delete/trash operations with phase, bytes/files progress, current file (redacted), whole-run average speed, and ETA. Sourced from the write-operations status cache (`resources/transfers.rs`); entries vanish on completion. Without it a running 10 GB copy is invisible to agents.
- `cmdr://indexing`: Drive indexing status in plain text (current phase, timeline history, DB stats). Calls `indexing::get_debug_status()` and formats as human-readable text.
- `cmdr://settings`: All settings with current values, defaults, types, and constraints. Fetched via round-trip to the frontend (`mcp-get-all-settings` event).
- `cmdr://logs`: Tail of the live `cmdr.log` file. Query: `?since=<iso>&filter=<substring>&limit=<n>`. `limit` defaults to 100, capped at 1000; `filter` is a case-sensitive substring match (no regex dep); `since` drops lines whose ISO-8601 timestamp prefix is ≤ the given value (lines without a timestamp prefix, like a Rust panic, are kept). Reads the last ~5 MB of the file from the end so a 50 MB rotated log doesn't blow up MCP memory. Returns oldest-first. **Each returned line is run through `crate::redact::redact_line`** (in the pure, unit-tested `select_log_lines` helper) so the resource honors the same PII-redaction contract as the crash + error reporters — a loopback caller without filesystem read can't lift home paths, SMB URIs, emails, or device names out of the log. The `filter` substring matches against the RAW (pre-redaction) line, since redaction runs last.

### Executor (`executor/`)

Tool dispatch and the ack contract live in `executor/`. The category split (`app.rs`, `view.rs`, `nav.rs`, `file_ops.rs`, `dialogs.rs`, `async_tools.rs`, `search.rs`), the `AckSignal` variants and budgets, and the `mcp_round_trip` pattern for tools that need an explicit FE response are all documented in [`executor/CLAUDE.md`](executor/CLAUDE.md). Read that before adding or modifying a tool handler.

### Configuration (`config.rs`)

Constants and configuration for the MCP server (port, bind address, transport settings). The default port for users who pin (setting `developer.mcpPort` to non-zero) is build-mode-dependent: 19224 in prod, 19225 in dev. Different defaults so a dev session and an installed prod build don't collide when both pin. With the post-instance-isolation default of `developer.mcpPort = 0`, the server binds ephemeral and these constants only matter as the pinned-mode fallback. Mirrored in the FE registry; both are in 10000–29999 per AGENTS.md. See [`/docs/tooling/instance-isolation.md`](../../../../../docs/tooling/instance-isolation.md) for the cross-resource view.

### Dialog state (`dialog_state.rs`)

`SoftDialogTracker` implementation: tracks which dialogs MCP believes are open. Updated by MCP tool calls; not always in sync with actual Tauri window state (see gotchas).

### State stores

- `PaneStateStore`: Current state of left/right panes (path, files, cursor, selection, tabs, type-to-jump). Includes a monotonic `generation` counter (AtomicU64) bumped on every `set_left`/`set_right`. Exposed in `cmdr://state` as `generation:` and used by the `await` tool's `afterGeneration` param to avoid matching stale state. The optional `typeToJump` field (buffer, indicatorVisible, indicatorStale, lastMatchedName) mirrors the per-pane type-to-jump state when a buffer or indicator is live, so MCP-driven tests can assert the feature without DOM access.
- `SoftDialogTracker`: Which dialogs MCP thinks are open (in `dialog_state.rs`)
- `listing_errors`: Bounded ring buffer (capacity 20) of the most recent `listing-error` events. Populated from `file_system::listing::streaming` at both `emit_error` sites — see the call to `crate::mcp::listing_errors::record(...)` right before the FE event fires, so MCP-visible state matches what the FE saw. Surfaced as `recentErrors:` in `cmdr://state`. **Freshness contract**: the buffer holds the absolute-newest 20 errors process-wide; on a busy session older errors silently drop off, so test scenarios that need older context should snapshot earlier and compare. Cancellations are not recorded — only failures.

Frontend syncs state to these stores via Tauri commands (`update_left_pane_state`, `update_pane_tabs`, etc.). Settings are fetched on-demand via round-trip to the frontend rather than stored in a state store.

### Tests (`tests/`)

Directory module split by test category:
- `protocol_tests.rs`: tool name validation, schema checks, tool count
- `resource_tests.rs`: resource URI validation, count, mime types (the public `get_all_resources` surface)
- `resource_state_tests.rs`: `cmdr://state` builder — URI/query parsing, pane/tab/file formatting
- `resource_log_tests.rs`: `cmdr://logs` builder — option parsing, line selection, `since` filter, the PII-redaction contract
- `resource_indexing_tests.rs`: `cmdr://indexing` builder — duration/number formatting helpers
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

Binding to `0.0.0.0` would expose the server to the network, so we bind `127.0.0.1` only. But localhost binding alone is **not** a security boundary against the real threat: a local non-Cmdr process. macOS doesn't isolate loopback between local processes, and a non-browser process can set any HTTP header (or none), so `validate_origin` (a browser-CSRF / DNS-rebinding defense) is no barrier to it. That's why the bearer token exists — see Authentication.

## Authentication

### What's gated

The bearer token is required for **only the calls that bypass the user's in-app confirmation dialog**:

- `delete` / `move` / `copy` with `autoConfirm: true`,
- the `dialog` tool with `action: "confirm"` (programmatically confirming an open dialog), and
- `set_setting` (config mutation applies with no user confirmation, so the whole tool is gated — otherwise an unauthenticated local process could flip `updates.errorReports`, `network.*`, `developer.mcp*`, etc.).

**Everything else needs no token**: resource reads (`cmdr://state`, `cmdr://logs`, etc.), navigation, search, and the destructive ops that still pop the confirmation dialog (`autoConfirm` absent/false). The decision lives in one pure, unit-tested predicate, `tool_call_requires_token(method, params)` in `server.rs`: true iff `method == "tools/call"` and the tool+args match one of the cases above.

The POST handler runs `if tool_call_requires_token(..) && validate_token(..).is_err() { reject }`. `GET /mcp` (SSE) carries no tool call, so it's never gated. `GET /mcp/health` stays open for liveness probes.

### Why only those

The threat is a **local non-Cmdr process silently auto-confirming a destructive op** — POSTing `delete`/`move`/`copy` with `autoConfirm: true` (or a `dialog` confirm) to wipe files without the user ever seeing the dialog. `validate_origin` is browser-CSRF defense only; it's no barrier to a local process that sets its own headers. Gating reads/nav/search bought no security (those mirror what the user can already do in the UI) while making the server annoying to drive — so the gate is now exactly the auto-confirm bypass, nothing more.

### How a client obtains and sends the token

The token is a fresh CSPRNG value (`Uuid::new_v4`, 122 random bits), generated on every server start and written to `<data_dir>/mcp.token` at mode 0o600 (owner-only). The port file (`mcp.port`) is 0o600 too. A client gets the token by either:

- reading `<data_dir>/mcp.token` (the same filesystem access an attacker would need to do damage directly), or
- reading the `CMDR_MCP_TOKEN` env var (see override below).

It then sends `Authorization: Bearer <token>` on the gated calls. `scripts/mcp-call.sh` already does this (reads `<data_dir>/mcp.token`, or `CMDR_MCP_TOKEN`); the E2E harness fetches it via the `get_mcp_token` Tauri IPC. The app's own frontend does NOT talk to this HTTP server (it uses the separate Tauri MCP bridge), so it needs no token.

The rejection is returned as an **in-band JSON-RPC error at HTTP 200, not a 401**. The MCP Streamable-HTTP spec reserves HTTP 401 for an OAuth challenge, so a 401 makes clients (Claude Code, etc.) launch an OAuth discovery flow and surface a confusing "Invalid OAuth error response" instead of our message. A 200 + JSON-RPC `error` is the canonical application-error shape and renders client-side as `MCP error <code>: <message>` (the same path `nav_to_path`'s "path does not exist" takes). Our bearer gate isn't OAuth, so it must not look like one.

The message tells the caller exactly where the token lives (the `CMDR_MCP_TOKEN` env var and the resolved `<data_dir>/mcp.token` path). That's safe: the secret is the file's 0o600 contents and the env value, not the path, which is already discoverable. The message never echoes the token, and it's one uniform message for missing-vs-wrong token (no oracle).

### `CMDR_MCP_TOKEN` override

If `CMDR_MCP_TOKEN` is set and non-empty (after trim) when the server starts, that value is used as the token instead of a random one. The token file is still written 0o600 with the chosen value. Tradeoff: a fixed env token is **stable across restarts** (so a static `Authorization: Bearer ${CMDR_MCP_TOKEN}` client header keeps working) but **loses the per-launch replay protection** the random token gives. It's opt-in for the dev workflow, not the default.

### Letting an external MCP client auto-confirm

To let an external MCP client (for example Claude Code) issue auto-confirming destructive ops, export `CMDR_MCP_TOKEN` for the running Cmdr and add a matching header to the server entry in `.mcp.json`:

```json
{
  "mcpServers": {
    "cmdr": {
      "url": "http://127.0.0.1:<port>/mcp",
      "headers": { "Authorization": "Bearer ${CMDR_MCP_TOKEN}" }
    }
  }
}
```

Without that header the client still works for everything else (reads, nav, search, and destructive ops that prompt in the app); only auto-confirm and `dialog` confirm are rejected (as the in-band JSON-RPC error above).

### Why separate state stores?

`PaneStateStore` is always synced (file pane changes frequently). `SoftDialogTracker` is updated by MCP tools themselves. Separating concerns keeps each store simple. Settings are fetched on-demand via `resource_round_trip` rather than stored, since they rarely change and can be queried from the frontend when needed.

## Gotchas

### Server lifecycle is managed at runtime

There are two start paths. **Startup** (`start_mcp_server` → `start_mcp_server_background`, fire-and-forget) binds with `BindMode::ProbeOnCollision` and serves. **Interactive** settings changes go through `rebind_interactive` instead, which is the race-free core and is **bind-new-before-stop**: it binds the new listener (`BindMode::Exact`) *before* retiring the old server, so (a) a busy port leaves the existing server untouched and returns `McpServerOutcome::PortInUse`, (b) a successful change never drops an in-flight request, and (c) we never collide with our own still-open socket. The pure `decide_rebind_action(strategy, actual_port)` short-circuits a re-apply of the live pinned port to `NoOp` (the guard against Exact-binding a port we already hold). Both paths funnel into `serve_on`, which stores `MCP_ACTUAL_PORT`, mints + writes the bearer token, writes `<data_dir>/mcp.port` (0o600, atomic tempfile + fsync + rename — see `port_file.rs`), caches the data dir in `MCP_PORT_FILE_DIR`, and spawns the serve task into `MCP_HANDLE`. The token is a fresh CSPRNG value by default, or the `CMDR_MCP_TOKEN` env value when set non-empty (see Authentication § override).

**Stopping has two flavors, and the difference is load-bearing.** `stop_mcp_server()` (sync) only `abort()`s the task — `abort()` *requests* cancellation, so the listener socket may linger briefly after it returns. That's fine for app shutdown (process exiting) and for the retire-the-old-server step inside `rebind_interactive` (the new listener is already up on a *different* port, no contention). `stop_mcp_server_and_wait()` (async) aborts **and awaits the handle**, guaranteeing the socket is released before returning; the interactive disable path (`set_mcp_enabled(false)`) uses it so an immediate re-enable on the **same** port binds cleanly instead of racing a not-yet-closed socket. Both clear `MCP_TOKEN` to None (so `validate_token` fails closed) and remove the port + token files; the crash path resets `MCP_ACTUAL_PORT` to 0 the same way. `is_mcp_running()` reads `MCP_ACTUAL_PORT != 0`.

The live-control commands (`set_mcp_enabled`, `set_mcp_port`, no restart needed) return a typed `McpServerOutcome` (`Running { port }` / `Stopped` / `PortInUse { requested }`) so the frontend branches on `kind`, never a message string; the wire shape is pinned by `mcp_server_outcome_json_shape`. The frontend shows "(ephemeral)" when the setting was 0, "(port N was in use)" when a *startup* probe landed off the pinned port, and on `PortInUse` keeps the server on its current port while offering the suggested free port (`findAvailablePort`). If the server crashes mid-serve, `MCP_ACTUAL_PORT` resets to 0 but the on-disk file may linger; external readers retry on `ECONNREFUSED`. Check logs for "MCP server crashed" errors.

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
