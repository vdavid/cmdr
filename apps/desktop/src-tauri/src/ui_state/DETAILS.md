# UI state mirror details

Must-knows: `CLAUDE.md`.

## Why this is its own module

- **Decision**: these stores live in `crate::ui_state`, outside both `tools/` and `mcp/`.
- **Why**: 22 files outside the AI surface read or write them (IPC commands, the listing host, the error reporter, and
  others). They describe the UI, not a transport or a tool, so naming them after either one would mislead every reader.

## State stores

- `PaneStateStore`: Current state of left/right panes (path, files, cursor, selection, tabs, type-to-jump). `hasParentRow` says whether `totalFiles` counts a `..` row, which the empty-operation gate subtracts before deciding a pane has nothing to act on. A search-results snapshot pane pushes here like any other, with its rows coming off the frontend snapshot rather than a backend listing (`src/lib/file-explorer/pane/DETAILS.md` § "Volume capabilities"). Includes a monotonic `generation` counter (AtomicU64) bumped on every `set_left`/`set_right`. Exposed in `cmdr://state` as `generation:` and used by the `await` tool's `afterGeneration` param to avoid matching stale state. The optional `typeToJump` field (buffer, indicatorVisible, indicatorStale, lastMatchedName) mirrors the per-pane type-to-jump state when a buffer or indicator is live, so MCP-driven tests can assert the feature without DOM access. The optional `quickFilter` field is the pattern while a quick filter narrows the pane (printed as `quickFilter:` only then): the pane's files, counts, and indices are then the filtered rows, so an agent can't mistake a hidden file for a gone one. The optional `mountError` field (share, reason, message: the typed `MountError` tag beside the pane's sentence, which is in the UI's language) mirrors a mount that didn't go through the same way — the "Couldn't mount share" pane and the login form an auth-class failure routes to alike — and is what makes a failed mount visible here at all: either replaces the share list, but the pane's `path` and `files` still describe that list, so without it a failed mount reads as a pane that simply didn't move (`NetworkMountView.svelte`; any other view's push clears it). The `listing` field (`settled` | `loading` | `stalled` | `error`, rendered as a `listing:` line only when not settled) says where the pane's listing stands, since an empty folder, a load in flight, a folder whose server stopped answering, and an error screen otherwise all push `totalFiles: 0` with no rows (`crates/cmdr-listing/DETAILS.md` § "Stalled listings").
- `SoftDialogTracker`: Which dialogs MCP thinks are open (in `dialog_state.rs`)
- `listing_errors`: Bounded ring buffer (capacity 20) of the most recent `listing-error` events, each with its raw message plus the typed `ListingErrorReason` / `ErrorCategory` the FE rendered from (error reports read only the typed pair, see `error_reporter/DETAILS.md` § "State snapshot at error time"). Populated from `listing_host::events::TauriListingEvents` at both `listing-error` sites (a failed read and the restricted-empty root) — see `AppListingEndEffects::record_recent_error`, called right before the FE event fires, so MCP-visible state matches what the FE saw. Surfaced as `recentErrors:` in `cmdr://state`. **Freshness contract**: the buffer holds the absolute-newest 20 errors process-wide; on a busy session older errors silently drop off, so test scenarios that need older context should snapshot earlier and compare. Cancellations are not recorded — only failures.
- `terminal_ops`: Bounded ring buffer (capacity 20) of the last write operations to SETTLE (completed / cancelled / failed, with `operationType` and `settledAtUnixMs`). Populated at the `TauriEventSink` terminal-emit sites (`crate::ui_state::terminal_ops::record(...)` via `apps/desktop/src-tauri/src/operations_host/terminal_effects.rs`, before each emit of `write-complete` / `write-cancelled` / `write-error`) — the `listing_errors` emit-site pattern. Backs `await operation_complete`. **Why not `operations-changed`**: the manager removes a settled op from its registry BEFORE `operations-changed` fires, so that snapshot never carries a terminal status (`LifecycleStatus` never reaches `Done`/`Cancelled`/`Failed` on a live record); the terminal outcome lives only in the dedicated terminal events. Same freshness caveat as `listing_errors`: a busy batch can push a settle off before a slow agent awaits it (then `await operation_complete` returns an honest "unknown operationId").

Frontend syncs state to these stores via Tauri commands (`update_left_pane_state`, `update_pane_tabs`, etc.). Settings are fetched on-demand via round-trip to the frontend rather than stored in a state store.

`PaneStateStore` has one reader outside MCP: the Dock tile menu lists the open tabs, through `tabs_focused_first()`. That accessor is `try_read`-only and clones just the tab lists (❌ never `files`), because it runs inside AppKit's `applicationDockMenu:` while the Dock waits on the answer — a plain `read()` queued behind a pane push would stall the Dock itself. Contract and rationale: `../dock/menu/DETAILS.md`. Nothing else about the store changes for it; it stays the mirror of what the UI knows.

## Dialog state (`dialog_state.rs`)

`SoftDialogTracker` implementation: tracks which dialogs MCP believes are open. Updated by MCP tool calls; not always in sync with actual Tauri window state (see gotchas).

## Archive-password prompt (`archive_password.rs`)

`ArchivePasswordPromptStore`: one slot holding what the password prompt is ASKING (the container kind, its name, the
path an answer must name, the parent volume id, `browse` / `transfer`, `wrongAttempt`, and the settled operation id).
Pushed by the frontend flow as it raises and dismisses the prompt, and cleared by `register_known_dialogs` alongside the
open-dialog list, since a webview reload never fires the dismiss half. ❌ The password itself never reaches this store —
it goes straight to the container. `../tools/actions/DETAILS.md` § "Answering the password prompt" has the whole contract.

## Why separate state stores?

`PaneStateStore` is always synced (file pane changes frequently). `SoftDialogTracker` is updated by MCP tools themselves. Separating concerns keeps each store simple. Settings are fetched on-demand via `resource_round_trip` rather than stored, since they rarely change and can be queried from the frontend when needed.

## State sync is best-effort

Frontend calls `update_left_pane_state()` after loading files, but there's no guarantee it completes before an MCP resource read. In practice, updates are fast and this isn't an issue. If stale data is a concern, add explicit sync waits.

## `PaneFileEntry` mirrors ANSWERS, not raw index fields

`recursiveSizeUpdating` is the frontend's own per-row "can this size still move" answer (walk + aggregation + pending), not the raw `recursiveSizePending` field it superficially resembles. `pane_state.rs`'s field doc says what feeds it; `pane-mcp-sync.svelte.ts` composes it; `../tools/resources/DETAILS.md` § "Directory sizes say how much they're worth" renders it. The pattern generalizes: this store is a mirror of what the UI KNOWS, so a field here may fold several backend facts, and copying the nearest-named `FileEntry` field into one is how a marker ends up quietly narrower than the hourglass it claims to mirror.

## Dialog state is "soft"

`SoftDialogTracker` stores which dialogs MCP thinks are open, but if a dialog is closed manually (not via MCP), the tracker isn't updated. The `cmdr://state` resource double-checks reality by querying Tauri windows.

## Tab state is synced separately from pane state

Tab info (id, path, pinned, active) is synced to `PaneState.tabs` via a separate `update_pane_tabs` command, debounced at ~100ms in the frontend. The `cmdr://state` resource shows a `tabs:` section per pane only when tabs are synced (non-empty). The `tab` tool emits an `mcp-tab` Tauri event that the frontend handles for all tab actions (new, close, close_others, activate, set_pinned, reopen, move). `move` is the one round-trip among them: `../tools/actions/DETAILS.md` § `mcp_round_trip` for explicit FE responses.

## Schema version doesn't apply to MCP state

MCP state stores don't have `_schemaVersion` fields. They're runtime-only, not persisted. If the state format changes, just restart the app.
