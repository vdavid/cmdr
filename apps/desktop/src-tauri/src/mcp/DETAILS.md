# MCP server details

Pull-tier docs for `src-tauri/src/mcp/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in [CLAUDE.md](CLAUDE.md).

## Purpose

Expose Cmdr functionality to AI agents via the Model Context Protocol (MCP). Agents can control the app using the same capabilities available to users, no more, no less.

## Architecture

### Server (`server.rs`)

`server.rs` is server/dispatch only: HTTP/Axum setup, the bind/rebind lifecycle, request dispatch (`process_request`),
and response formatting (`format_sse_event` / `build_sse_response` / `build_json_response`). Per-request auth and
validation live in `auth.rs` (below); the server `use`s them. The split is one-directional — `server` depends on
`auth`, never the reverse.

- Runs in a background tokio task spawned at app startup
- Binds `127.0.0.1` only (localhost). Port defaults to ephemeral: when `developer.mcpPort` (the user setting) is 0 (the new default), the server binds `127.0.0.1:0` and asks the kernel for an unused port. A non-zero setting pins that port. `resolve_bind_strategy` turns the resolved port into `BindStrategy::Ephemeral` or `BindStrategy::Pinned(port)`; `bind_listener` then binds it under one of two `BindMode`s: **`ProbeOnCollision`** (startup auto-start — probe upward up to 100 ports so *a* server comes up when nobody's watching) or **`Exact`** (interactive settings changes — fail with `BindError::PortInUse` so the user is told their chosen port is busy instead of silently landing elsewhere). Both helpers are unit-tested in `server.rs`. The legacy fixed defaults (`19224` prod / `19225` dev) still live in `config.rs::DEFAULT_PORT` as the fallback for `CMDR_MCP_PORT` parse failures and are mirrored in the FE registry for users who want to pin a port.
- Writes the actual bound port to `<data_dir>/mcp.port` atomically (tempfile + fsync + rename, mode 0o600) after `bind`, plus a fresh per-instance bearer token to `<data_dir>/mcp.token` (same atomic write, 0o600). External readers (the `scripts/mcp-call.sh` CLI, E2E fixtures, agent helpers) discover the port and token from those files and send `Authorization: Bearer <token>` on every request; the FE still uses the `get_mcp_port` / `get_mcp_token` IPC to read the same in-process state. On clean shutdown both files are removed and the in-process token is cleared; on crash they stay and readers discover staleness via `ECONNREFUSED`. See `port_file.rs` for the `write_port_file` / `write_secret_file` / read / remove API and typed `PortDiscoveryError`.
- **Auth**: every `/mcp` request runs `validate_origin` (browser-CSRF / DNS-rebinding layer). The bearer token is then required only for the narrow set of calls that bypass the user's in-app confirmation dialog — see the Authentication section below. `validate_token` reads `Authorization: Bearer <token>`, compares against the in-process token in constant time, and returns `Err(())` on missing/empty/mismatched; the POST handler turns that into a friendly in-band JSON-RPC error via `auto_confirm_token_required_response` (HTTP 200, **not** 401 — see Authentication for why). It fails closed if no token is set. `GET /mcp/health` and `GET /mcp` (SSE) are unauthenticated.
- Streamable HTTP transport (MCP spec 2025-11-25)
- Endpoints: `POST /mcp` (JSON-RPC), `GET /mcp` (optional SSE), `GET /mcp/health`

### Auth (`auth.rs`)

Owns the per-instance bearer-token lifecycle and all per-request validation the POST handler runs. The token statics
(`MCP_TOKEN` + `mcp_token_slot`) and accessors `current_mcp_token` (re-exported from `mod.rs` for the `get_mcp_token`
IPC) / `set_mcp_token` (server-lifecycle only) live here, alongside `generate_token` and the constant-time compare.
Validation: `validate_origin` (+ `is_localhost_origin`), `validate_accept_header`, `prefers_sse`, `get_protocol_version`,
`tool_call_requires_token` (the pure gate predicate), `validate_token`, and `auto_confirm_token_required_response`. The
full model — what's gated, why only those calls, and why the rejection is HTTP 200 not 401 — is in the Authentication
section below.

### Protocol (`protocol.rs`)

- JSON-RPC 2.0 message parsing
- Routes to `initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`
- Session management (though most clients don't use sessions)

### Tools (`tool_registry.rs`)

All 39 tools are authored once in the `mcp_tools!` table in `tool_registry.rs` — name, description, JSON schema,
`TokenGate`, and handler per entry. That one table generates `get_all_tools()` (tools/list), `execute_tool()`
(dispatch), and `tool_gate()` (auth), so the facets can't drift and adding a tool is a single entry. `tools.rs` is a
thin shim: the `Tool` struct plus a re-export of `get_all_tools`. Read the entries for the exact schemas (don't
transcribe them here); the sections below summarize behavior. Wire output is byte-identical and pinned by
`tests/tool_snapshot_tests.rs`.

**Param naming is camelCase** (`tabId`, `timeoutSeconds`, `sizeMin`, `autoConfirm`). Tool names stay snake_case. Don't add snake_case params; agents pattern-match across tools and every inconsistency is a guessed-wrong call.

39 semantic tools grouped by category:
- Navigation (6): `select_volume` (also accepts MTP volume names), `nav_to_path` (accepts absolute, `~`-relative, and virtual `mtp://` / `smb://` paths; virtual paths skip the local existence check, local paths get a timed one — see `executor/mod.rs::validate_path_exists`), `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`
- Cursor/Selection (3): `move_cursor` (honest errors: a missing filename or out-of-range index is a round-trip failure, never a false OK), `open_under_cursor`, `select` (index ranges, `all`, `count: 0` to clear, or `names: [...]`; every mode is a round-trip that replies after the new selection landed in `PaneStateStore`, and the names mode errors listing any names not in the listing. Focuses the target pane — both the backend store AND the FE focused pane, so a follow-up focused-pane operation (copy/delete) acts on the pane you just selected in)
- File operations (7): `copy`, `move`, `delete`, `rename`, `mkdir`, `mkfile`, `refresh` (a round-trip that forces a backend re-read of the focused pane's listing — local volumes re-read from disk; watcher-backed MTP/SMB listings short-circuit). `copy`/`move`/`delete` fast-fail with the real cause when there's nothing to act on (no selection and the cursor is on `..`, or the pane shows no files) instead of a misleading ack timeout. `copy`/`move` accept optional `autoConfirm` (bool) and `onConflict` (`skip_all`|`overwrite_all`|`rename_all`). `onConflict` governs clashing **files only** — folders always merge (a source folder landing on a same-named dest folder merges into it; the policy then applies to the files inside). `delete` accepts optional `autoConfirm`. When `autoConfirm` is true, the dialog opens and immediately confirms, and the OK text carries the spawned `operationId` (via the `mcp_await_operation_start` round-trip — the FE replies with the id the manager minted) so the agent can drive `queue` / `await operation_complete` next. `compress` on an existing target keeps its dialog open and acks without an id. `rename` (`newName` required; optional `pane`, `name` (defaults to the cursor item), `autoConfirm`; gate `IfAutoConfirm`) targets the named item, else the cursor item, resolved off the pane state. Without `autoConfirm` it's a round-trip: the FE moves the cursor to the target row and starts the inline rename editor prefilled with `newName` for the user to review (`StartRenameOptions.initialName`, pinned to the row via `expectedName`) — the human-review affordance. With `autoConfirm` it calls the `rename_file` backend directly (`force: false`, so an existing target name is an honest error), and the managed op notifies the listing cache so the pane refreshes. `mkdir` / `mkfile` (gate `IfAutoConfirm`) take optional `name` + `autoConfirm`: no name opens the naming dialog; `name` alone opens it prefilled (`initialName`); `name` + `autoConfirm` creates directly — a round-trip where the FE calls `create_directory` / `create_file` with its LIVE focused-pane path (never a backend `PaneStateStore` read, which lags a nav by the debounced sync and could create in the pane's previous directory), returning OK or an honest conflict error. `delete` gains `mode` (`trash` | `delete`): without `autoConfirm` the dialog's trash/permanent toggle is preset to it; with `autoConfirm` the FE routes to `trash_files` vs `delete_files`. `mode` maps to the FE's `permanent` bool (a typed IPC flag, `no-string-matching`) and only rides the event when given — omitted, the FE applies its per-volume default (trash where supported, forced permanent on no-trash volumes and inside archives), so the volume clamp stays single-sourced in the FE.
- Tags (1): `tag` (`action` = `set` | `toggle` | `clear`, `colors` array of the seven Finder color names, optional `pane` (default focused) and `names` (default selection, else cursor); gate `Always`). A thin adapter over `file_system::tags` (`toggle_color` / `set_tags`) — the context-menu toggle's primitives. Resolves target paths off the pane state (`resolve_pane_target_paths`: names, else selection, else cursor — an unresolvable name/selection is an honest error), then patches the focused pane's cached listing via `apply_tags_to_listing` (the `enrich_tags` refresh path) so the dots re-render. Resolution reads the last-synced listing (nav doesn't force a flush — only `select`/`move_cursor` do), so tag by name after reading `cmdr://state` (or after a `select`), not immediately after a bare `nav`; otherwise a same-named file from the pane's previous directory could be the resolved target. `set` keeps colorless custom tags and preserves a custom-named tag of a requested color; `toggle` uses Finder's all-have→remove rule per color; `clear` removes all. macOS-only (Finder tags don't exist elsewhere), so off macOS it returns a clean not-supported error. No FE dispatch, no invented ack (the `indexing` precedent).
- View (3): `sort`, `toggle_hidden`, `set_view_mode`
- Tabs (1): `tab` (unified: `action` = `new` | `close` | `close_others` | `activate` | `set_pinned`; `tabId` defaults to active tab for close/close_others/set_pinned, required for activate; `pinned` boolean for set_pinned)
- Dialogs (2): `dialog` (unified open/focus/close/confirm). `action: "confirm"` programmatically confirms an open dialog. For `transfer-confirmation`: accepts optional `onConflict`. For `delete-confirmation`: just confirms. `type: "transfer-confirmation"` is the primary name (covers copy and move); `"copy-confirmation"` is accepted as an alias. `open_search_dialog` opens the whole-drive search overlay with optional pre-filled `query`, `mode` (`ai`/`filename`/`regex`), `sizeMin`/`sizeMax` (bytes), `modifiedAfter`/`modifiedBefore` (ISO date), `isDirectory` (true = folders only, false = files only, omit for both), `scope` (chip syntax), `caseSensitive`, `excludeSystemDirs`, and `autoRun` (default true: runs the search after open). Acks on `SoftDialogAppeared("search")` within the 1500 ms budget. **Race-with-close caveat**: if the dialog is mid-close when the event lands, the new mount may race; the ack times out and the tool surfaces a clean failure rather than a false-positive OK (per plan §5.7 risk register).
- App (3): `switch_pane`, `swap_panes`, `quit`
- Search (2): `search` (structured file search across the drive index, optional `scope` for path/exclude filtering), `ai_search` (natural language search using configured LLM, optional `scope` merged with AI-inferred scope)
- Settings (1): `set_setting` (change a setting value via round-trip to frontend)
- Indexing (1): `indexing` (`action` = `enable` | `disable` | `rescan` | `forget`, `volumeId`; gate `Always`). A thin
  adapter over `commands::indexing` (`enable_drive_index` / `disable_drive_index` / `rescan_drive_index` /
  `forget_drive_index`) — no FE dispatch, no invented ack (the `connect_to_server` precedent). `enable`/`rescan` map the
  typed `EnableIndexingOutcome` to honest text and carry the ordering contract (below); `disable`/`forget` map
  `Result<(), String>` directly. Because the generic executor can't supply the concrete `AppHandle` that
  `enable`/`rescan` need, they route through handle-free `*_via_handle` wrappers backed by a startup-cached handle
  (`set_app_handle` in `setup()`). Status is NOT an action — it lives in `cmdr://indexing`.
- Queue (1): `queue` (`action` = `pause` | `resume` | `cancel` | `pause_all` | `resume_all`; `operationId` for the
  per-op actions, `operationIds` array for multi-cancel, `rollback` bool on cancel). A thin adapter over the
  `write_operations` manager (`pause_operation` / `resume_operation` / `pause_all` / `resume_all` / `cancel_operation` /
  `cancel_operations` / `cancel_write_operation(id, rollback)`) — direct backend calls, no FE ack (the `indexing`
  precedent). Per-op actions validate the id against `list_operations` for an honest "unknown operationId" instead of a
  silent backend no-op. Gate `IfRollback`: pause/resume/plain-cancel are `Open` (transient runtime actions on a
  crash-safe pipeline), but `rollback: true` deletes already-copied files, so it needs the token. Discover ids + status
  in `cmdr://state` `operations:`. `connect_to_server` (add a manual SMB server by address, checks TCP reachability), `remove_manual_server` (remove a manually-added server by host ID), `upgrade_smb_to_direct` (upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O; thin wrapper over the existing manual "Connect directly" Tauri command — tries Keychain creds, returns a typed result mirroring `UpgradeResult`)
- Favorites (1): `favorites` (`action` = `add` | `rename` | `remove` | `reorder`; `path` (+ optional `name`) for add, `id` for rename/remove, `name` for rename, `orderedIds` (the COMPLETE new ordering) for reorder; gate `Always`). A thin adapter over `commands::favorites` (`add_favorite` / `rename_favorite` / `remove_favorite` / `reorder_favorites`), which persist `favorites.json` and re-emit `volumes-changed` themselves, so the switcher refreshes live — no FE dispatch, no invented ack (the `indexing` precedent). `reorder` is a pass-through of the full ordering (a `(id, position)` shape would force the MCP layer to re-implement splicing). Discover ids in `cmdr://state` `favorites:`.
- Eject (1): `eject` (`volumeId`; gate `Open`). A thin adapter over `file_system::volume::eject::eject` — parity with the one-click Eject button. The backend refuses honestly (surfaced as errors, not false OKs) while a write op reads from or writes to the volume (`EjectError::Busy`) and for non-ejectable volumes; `Open` because it's a reversible, one-click runtime action touching no persistent state.
- Async (1): `await` (poll until a condition is met. Pane conditions (`pane` required): `has_item`, `not_has_item`, `item_count_gte`, `item_count_lte`, `path`, or `path_contains` — the absence conditions are for "wait until the delete finished" flows; `~` expands in path-condition values; `afterGeneration` avoids matching stale state. Volume condition (`volumeId` + `value`, no pane): `index_status` waits until a volume's indexing freshness equals `fresh` / `scanning` / `stale`, reading the single freshness store each tick (never re-deriving — the transition table lives in `indexing/freshness.rs`). Deliberately two fields, NOT a packed `<volumeId>:<status>` string: MTP volume ids embed colons. Operation conditions (no pane): `operation_complete` (`value` = operationId) resolves when the op settles and reports the terminal status (completed / cancelled / failed) — an id in neither the live registry (`list_operations`) nor the terminal-ops ring is an honest "unknown operationId" error, never a hang; `operations_idle` (no `value`) resolves when no op is running or queued (paused ops excluded, so it can't hang on a parked op). `timeoutSeconds` up to 60.)
- Downloads (1): `go_to_latest_download` (no args; navigates the focused pane to `~/Downloads` and selects the most recently observed eligible file. Errors when no eligible file exists or FDA is missing. Reuses the same backend code path as the `⌘J` shortcut and the `go_to_latest_download` Tauri command, then drives `mcp-nav-to-path` + `mcp-move-cursor` round-trips for the navigation + cursor placement)

### Resources (`resources/`)

Directory module split by resource. `resources/mod.rs` is the shared spine: the registry (`get_all_resources`), URI/query parsing (`split_uri`, `parse_query`), the `read_resource` dispatch, the `resource_round_trip` helper, and the `cmdr://state` + `cmdr://dialogs/available` builders. The independently-evolving builders live in their own files: `resources/logs.rs` (`cmdr://logs`), `resources/indexing.rs` (`cmdr://indexing`: `build_indexing_text`, `format_duration_human`, `format_number`), `resources/importance.rs` (`cmdr://importance`; tests colocated in `resources/importance/tests.rs`), `resources/operations.rs` (the `cmdr://state` `operations:` section), and `resources/volumes.rs` (the `cmdr://state` `volumes:` section: `build_volumes_yaml` over a `snapshot_volumes` seam, tests colocated). `importance.rs`, `operations.rs`, and `volumes.rs` colocate their tests; the rest live in the `tests/` directory (see below).

- `cmdr://state`: Complete app state in YAML (both panes, volumes, dialogs, active `listings` cache, `recentErrors`). Every `volumes:` entry is uniformly structured (`resources/volumes.rs`, pure `build_volumes_yaml` over a snapshot so it's unit-tested off fixtures): `name`, `id`, and `kind` (`local` | `smb` | `mtp` | `virtual`) always, plus present-when-known `filesystem` (the statfs fs type), `readOnly`, `ejectable`, `indexStatus` (`fresh` | `scanning` | `stale` | `off`, sharing the one `status_token` mapping with `cmdr://indexing` so the two can't disagree), and `smbConnectionState` (`direct` | `os_mount` | `disconnected`, so agents route `upgrade_smb_to_direct` at the right shares). No more bare `- {name}` lines — agents stop guessing which entries carry ids. `indexStatus` for local/favorite/SMB entries resolves via `get_volume_index_status_for_path` (favorites and local paths → `root`, SMB paths → the share's id); MTP entries look it up by their `{device_id}:{storage_id}` id. Per-pane `volumeId` still rides each pane block. The `listings` section reflects every entry in `LISTING_CACHE` (id, volumeId, path, entry count, ageMs); `recentErrors` is the last 20 directory-listing failures with `atUnixMs`, `listingId`, `volumeId`, `path`, `message` (see `listing_errors.rs` and the freshness contract below); the `path` and `message` fields are run through `crate::redact::redact_line` before serialization, since failed-listing errors can carry SMB URIs / home paths the user never saw rendered. The `favorites:` section lists the user's favorites (`id`, `name`, `path`) so agents can discover the ids the `favorites` tool's rename / remove / reorder actions take; paths render unredacted like `listings:` (user-chosen navigation targets, not error/log leakage). File entries carry a `[tags:red,blue]` marker (colored tags as their color name, colorless custom tags by name) when they have Finder tags — mirrored from the FE listing (`PaneFileEntry.tags`), filled visible-range-first by `enrich_tags`, zero cost when absent. Supports `?include=panes,volumes,dialogs,listings,recentErrors,operations,favorites` projection (defaults to all) and `?compact=true` (drops the `files:` list inside each pane while keeping every summary field). Example: `cmdr://state?include=listings,recentErrors` is the minimal payload for "did the last listing succeed?".
- `cmdr://dialogs/available`: Static metadata about available dialogs
- `operations:` (inside `cmdr://state`, also `?include=operations`): every queued, running, or paused write operation (copy/move/delete/trash/compress/archive-edit) with `operationId`, `type`, `status` (`running`/`paused`/`queued`), source/dest summary (redacted), and — for running/paused ops — progress, current file (redacted), whole-run average speed, and ETA. **Two-source join** (`resources/operations.rs`): membership + lifecycle status come from the operation manager's registry (`list_operations`, whose `OperationSnapshot` carries no progress by design); progress/speed/ETA come from the separate write-operations status cache (`get_operation_status`), joined by id. A queued op has no status-cache entry, so it renders status-only. This is the discovery surface for the `queue` tool. Settled ops leave both sources immediately (removal-on-terminal), so their outcome lives only in the terminal-ops ring (below). (Renamed from `transfers:`; a deliberate wire break.)
- `cmdr://indexing`: Per-volume drive indexing status in plain text (`resources/indexing.rs`). Default: one summary block per known (registered) volume — freshness (`fresh`/`scanning`/`stale`/`off`), current phase, live scan progress (counts + percent + ETA while scanning), a step checklist while scanning, DB entry/dir counts + file size, and the last completed scan. `?volume=<id>` adds a deep debug view for one volume (watcher / live-event stats, DB internals, and the phase timeline with triggers); an unknown id returns an honest "no index found". The builders (`build_indexing_text` / `build_volume_debug_text`) are pure over an injected `VolumeIndexingSnapshot` (the `transfers.rs` snapshot-then-format precedent, `now_unix_s` injected), so formatting is unit-tested without a live index; `snapshot_indexing` / `snapshot_volume_indexing` do the reads via `get_volume_index_status` + `get_debug_status` (freshness is never re-derived). Volumes come from `all_registered_volume_ids` (root first, then sorted).
- `cmdr://importance`: Folder-importance scores in plain text (`resources/importance.rs`), **offline-capable** — the per-volume `importance.db` stores outlive their volume's mount, so this answers about unmounted drives (the importance subsystem's headline). Four query modes: `?path=<abs-path>` (one folder's `WeightLookup` — Scored with score + the `explain` signal breakdown, Floored with the typed reason, or Unscored; `~` expands to `$HOME`), `?top=<n>&volume=<id>` (top-N by score, volume optional ⇒ merged across all scored volumes, `n` capped at 500), `?threshold=<f>` (folders scoring ≥ `f`, capped at 100 rows with a truncation note — a low threshold can match every scored folder), and no query (a usage summary plus a per-volume overview: id, kind, generation, folder count, so a blind first read teaches the syntax). Every read goes through `ImportanceIndex` (never raw SQLite — the subsystem's consumer-entry-point invariant); scored volumes are enumerated from the `importance-{id}.db` files on disk via `read::scored_volume_ids` (MTP is never background-scored, so it never appears). The `build_*` builders are pure over the snapshot + an injected `now_secs` (the `indexing.rs` snapshot-then-format precedent), so formatting is unit-tested without a live app; the explain breakdown opens the index with the volume kind's `signal_availability` mask so it sums to the stored score (SMB drops Spotlight `last_used`). A missing DB reads as empty / unscored, never an error.
- `cmdr://settings`: All settings with current values, defaults, types, and constraints. Fetched via round-trip to the frontend (`mcp-get-all-settings` event).
- `cmdr://logs`: Tail of the live `cmdr.log` file. Query: `?since=<iso>&filter=<substring>&limit=<n>`. `limit` defaults to 100, capped at 1000; `filter` is a case-sensitive substring match (no regex dep); `since` drops lines whose ISO-8601 timestamp prefix is ≤ the given value (lines without a timestamp prefix, like a Rust panic, are kept). Reads the last ~5 MB of the file from the end so a 50 MB rotated log doesn't blow up MCP memory. Returns oldest-first. **Each returned line is run through `crate::redact::redact_line`** (in the pure, unit-tested `select_log_lines` helper) so the resource honors the same PII-redaction contract as the crash + error reporters — a loopback caller without filesystem read can't lift home paths, SMB URIs, emails, or device names out of the log. The `filter` substring matches against the RAW (pre-redaction) line, since redaction runs last.

### Executor (`executor/`)

The tool handlers and the ack contract live in `executor/`. Dispatch itself (`execute_tool`) is generated by the
`mcp_tools!` table in `tool_registry.rs`, which calls these handlers by path; that's why the category submodules are
`pub(crate)` (a sibling module reaching their `pub` handler fns). The category split (`app.rs`, `view.rs`, `nav.rs`,
`file_ops.rs`, `dialogs.rs`, `async_tools.rs`, `search.rs`, `downloads.rs`), the `AckSignal` variants and budgets, and
the `mcp_round_trip` pattern for tools that need an explicit FE response are all documented in
[`executor/CLAUDE.md`](executor/CLAUDE.md). Read that before adding or modifying a tool handler.

### Configuration (`config.rs`)

Constants and configuration for the MCP server (port, bind address, transport settings). The default port for users who pin (setting `developer.mcpPort` to non-zero) is build-mode-dependent: 19224 in prod, 19225 in dev. Different defaults so a dev session and an installed prod build don't collide when both pin. With the post-instance-isolation default of `developer.mcpPort = 0`, the server binds ephemeral and these constants only matter as the pinned-mode fallback. Mirrored in the FE registry; both are in 10000–29999 per AGENTS.md. See [`/docs/tooling/instance-isolation.md`](../../../../../docs/tooling/instance-isolation.md) for the cross-resource view.

### Dialog state (`dialog_state.rs`)

`SoftDialogTracker` implementation: tracks which dialogs MCP believes are open. Updated by MCP tool calls; not always in sync with actual Tauri window state (see gotchas).

### State stores

- `PaneStateStore`: Current state of left/right panes (path, files, cursor, selection, tabs, type-to-jump). Includes a monotonic `generation` counter (AtomicU64) bumped on every `set_left`/`set_right`. Exposed in `cmdr://state` as `generation:` and used by the `await` tool's `afterGeneration` param to avoid matching stale state. The optional `typeToJump` field (buffer, indicatorVisible, indicatorStale, lastMatchedName) mirrors the per-pane type-to-jump state when a buffer or indicator is live, so MCP-driven tests can assert the feature without DOM access.
- `SoftDialogTracker`: Which dialogs MCP thinks are open (in `dialog_state.rs`)
- `listing_errors`: Bounded ring buffer (capacity 20) of the most recent `listing-error` events. Populated from `file_system::listing::streaming` at both `emit_error` sites — see the call to `crate::mcp::listing_errors::record(...)` right before the FE event fires, so MCP-visible state matches what the FE saw. Surfaced as `recentErrors:` in `cmdr://state`. **Freshness contract**: the buffer holds the absolute-newest 20 errors process-wide; on a busy session older errors silently drop off, so test scenarios that need older context should snapshot earlier and compare. Cancellations are not recorded — only failures.
- `terminal_ops`: Bounded ring buffer (capacity 20) of the last write operations to SETTLE (completed / cancelled / failed, with `operationType` and `settledAtUnixMs`). Populated at the `TauriEventSink` terminal-emit sites (`crate::mcp::terminal_ops::record(...)` inside `emit_complete` / `emit_cancelled` / `emit_error`) — the `listing_errors` emit-site pattern. Backs `await operation_complete`. **Why not `operations-changed`**: the manager removes a settled op from its registry BEFORE `operations-changed` fires, so that snapshot never carries a terminal status (`LifecycleStatus` never reaches `Done`/`Cancelled`/`Failed` on a live record); the terminal outcome lives only in the dedicated terminal events. Same freshness caveat as `listing_errors`: a busy batch can push a settle off before a slow agent awaits it (then `await operation_complete` returns an honest "unknown operationId").

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

**Decision:** Every fire-and-forget action tool waits for a typed ack signal (`AckSignal::GenerationAdvanced`, `GenerationAdvancedOrSoftDialog` — the one honest two-arm OR, used by auto-confirmed `compress` where the op either starts OR the confirm dialog stays open on an existing target, see `executor/ack.rs`, `SoftDialogAppeared`/`Disappeared`, `WindowAppeared`/`Disappeared`, `WindowCountBelow`, or `Any`) within a 1500 ms budget (5 s for the nav family) before returning `OK`. On timeout, the tool returns a `ToolError::internal` whose message names the missing signal and elapsed budget.

**Why.** Real QA hit a paper-cut: MCP tools were returning `OK` while the FE was stalled (modal blocking input, error pane up, race during startup), so the dispatched action was silently dropped. That made MCP unreliable as an automation surface. The ack contract makes `OK` a real promise: the FE actually processed the dispatched action.

**Why 1500 ms.** Most state pushes complete within ~100–300 ms in practice (FE debouncing, IPC round-trip). 1500 ms gives a generous margin for the slow cases (cold start, large directory listings) while still failing fast when the FE genuinely isn't responding. Latency-sensitive tools (`nav_to_path`) keep their existing higher budgets via `mcp_round_trip_with_timeout`.

**Why not a per-tool client-facing timeout knob.** The timeout is a backend-side latency budget, not a client concern. MCP clients shouldn't have to tune it per call — they expect tools to either succeed or report a clear failure.

### The `indexing` ordering contract (race-free `await index_status`)

**Decision.** `indexing` `enable` / `rescan` don't return until the volume's freshness has LEFT its pre-scan state, so a
follow-up `await index_status <volume> fresh` can't instantly match the pre-rescan Fresh state. The handler captures the
freshness before the backend call, then polls until it differs (bounded 5 s, timeout-tolerant); a pre-state that's
already `scanning` needs no wait.

**Why it's not a spurious ack.** `force_scan` (the active-index rescan path) fires `ScanStarted` → `Scanning`
synchronously before returning, so the first poll already differs and the handler returns immediately. Only the
enable-first-scan path (a not-yet-active volume, especially async SMB via `start_indexing_for_smb`) can lag the flip,
and that's exactly where the wait earns its keep. This replaces a generation counter for the new `await` condition (the
tool contract sequences it instead), which is cheaper and honest.

### Why the TS parse layer stays separate from the registry

The `mcp_tools!` registry single-sources the three **Rust** consumers (list, dispatch, auth gate). The frontend's
per-event validate-parse layer in `apps/desktop/src/routes/(main)/mcp-listeners.ts` is deliberately NOT folded in. It's
a transport adapter on the other side of the IPC boundary: it validates the *event payloads* the handlers emit, not the
tool schemas, and those payloads are unchanged. Folding it into the registry would require generating TypeScript from
the Rust table (a Rust→TS codegen pipeline) — a much larger, separate effort, out of scope here. The event-payload
contracts stay in sync across the boundary exactly as they did before the registry existed, so nothing was lost by
leaving this side hand-written.

### Why agent-centric API?

The original design mirrored keyboard shortcuts (43 tools like `nav_up`, `nav_down`). This forced agents to make dozens of calls to find a file. The agent-centric redesign consolidated to semantic tools (`move_cursor(index=42)`, `nav_to_path("/Users")`). This reduced round-trips from 6+ reads to 1 (`cmdr://state` resource).

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
- the `dialog` tool with `action: "confirm"` (programmatically confirming an open dialog),
- `set_setting` (config mutation applies with no user confirmation, so the whole tool is gated — otherwise an unauthenticated local process could flip `updates.errorReports`, `network.*`, `developer.mcp*`, etc.), and
- `indexing` (all four actions mutate per-drive config or throw away / start heavy background work with no confirmation dialog — the `set_setting` rationale applied per drive),
- `tag` (silent metadata mutation on user files, with no in-app confirmation dialog to piggyback on),
- `favorites` (persistent app-config mutation with no confirmation dialog — the `set_setting` rationale), and
- `queue` with `rollback: true` (a rollback cancel deletes already-copied files with no confirmation dialog — the auto-confirm-a-destructive-thing shape; the `IfRollback` gate). Plain pause/resume/cancel stay open (transient runtime actions, crash-safe pipeline, no persistent state touched), as does `eject` (a reversible one-click runtime action; the backend refuses honestly while the volume is busy).

**Everything else needs no token**: resource reads (`cmdr://state`, `cmdr://logs`, etc.), navigation, search, and the destructive ops that still pop the confirmation dialog (`autoConfirm` absent/false).

The classification is **sourced from the tool registry, not a separate string list** — this is the by-construction win. Each tool declares a `TokenGate` (`Open` / `Always` / `IfAutoConfirm` / `IfConfirmAction` / `IfRollback`) on its `mcp_tools!` entry in `tool_registry.rs`. `auth::tool_call_requires_token(method, params)` (a pure, unit-tested predicate) returns true iff `method == "tools/call"` and `tool_gate(name)` reports a gate whose `requires_token(arguments)` is true. Because the gate is a required field on every entry, a new destructive tool can't ship with the gate forgotten — and two structural tests (`test_autoconfirm_tools_are_gated`, `test_rollback_tools_are_gated`) fail if a tool exposing `autoConfirm` / `rollback` is left `Open`, while a full-table set-equality test forces a conscious gate for any newly-added tool. `TokenGate::IfConfirmAction` / `IfRollback` read the tool's own typed `action` / `rollback` field, not a message substring, so they're not a `no-string-matching` violation.

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

`execute_tool()` is an async function. Action tools follow the ack contract (see "Action-tool ack contract" above): dispatch the event, then `wait_for_ack` for a small backend-side signal before returning. The tool's reported "OK" thus means "the FE accepted the dispatched action," not "the underlying operation completed." For long-running operations (a copy of 10 GB), the agent still has to poll via the `await` tool to observe completion. The ack-contract change made the FE-accepted line meaningful — before it, the tool returned `OK` even when the FE wasn't listening.

Three categories of latency-sensitive tools exist beyond the ack contract: (1) `mcp_round_trip` tools (`nav_to_path`, `move_cursor`, `select`, `refresh`, `set_setting`, `open_under_cursor`) that wait up to 5–30 s for an explicit `mcp-response` event with success/failure — the per-request correlation means the ack works even when the action produced no pane-state push (a `refresh` whose re-listing is byte-identical to the cached state still acks), (2) search tools (`search`, `ai_search`) that load the search index via `spawn_blocking` and (for `ai_search`) call the LLM API, (3) `select_volume` which polls until the target pane's `volume_name` matches.

### Error codes are JSON-RPC standard

`INVALID_PARAMS = -32602`, `INTERNAL_ERROR = -32603`, etc. These are defined by the JSON-RPC spec, not MCP. Don't change them.

### Tab state is synced separately from pane state

Tab info (id, path, pinned, active) is synced to `PaneState.tabs` via a separate `update_pane_tabs` command, debounced at ~100ms in the frontend. The `cmdr://state` resource shows a `tabs:` section per pane only when tabs are synced (non-empty). The `tab` tool emits an `mcp-tab` Tauri event that the frontend handles for all tab actions (new, close, close_others, activate, set_pinned).

### Schema version doesn't apply to MCP state

MCP state stores don't have `_schemaVersion` fields. They're runtime-only, not persisted. If the state format changes, just restart the app.
