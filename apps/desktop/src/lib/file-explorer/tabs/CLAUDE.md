# Tabs

Per-pane tab system for the dual-pane file explorer. Each pane side (left/right) has an independent tab bar.

## Architecture

- `tab-types.ts` — Type definitions: `TabId`, `TabState`, `PersistedTab`, `PersistedPaneTabs`, `UnreachableState`
- `tab-state-manager.svelte.ts` — Reactive state manager using `$state()`. All tab operations (add, close, switch,
  cycle, pin). Max 10 tabs per pane.
- `TabBar.svelte` — Tab bar UI component. Always visible, Chrome-style shrinking tabs, pin icons, close buttons, context
  menu.
- `tab-state-manager.test.ts` — Unit tests for state manager

## Key decisions

**Decision**: Cold load on tab switch via `{#key activeTabId}` — destroys and recreates FilePane, no warm cache **Why**:
Keeping inactive tabs alive would mean multiple FilePanes with active file watchers, listing caches, and scroll state in
memory. For 10 tabs per pane (20 total), that is untenable. Cold load with cursor-by-filename restoration is fast enough
that the simplicity wins.

**Decision**: Clone trick for new tab — `addTab` inserts to the LEFT without changing `activeTabId` **Why**: Since
`{#key activeTabId}` drives FilePane recreation, not changing the active tab ID means no remount happens. The user sees
the new tab appear in the tab bar instantly while staying on their current tab. Switching to the new tab is a separate
explicit action.

**Decision**: Cursor restored by filename, not by index **Why**: The listing may have changed while the tab was inactive
(file watcher events still apply). Index-based restoration would point to the wrong file. Filename-based restoration via
`findFileIndex` is resilient to insertions/deletions.

**Decision**: Selection cleared on tab switch **Why**: v1 simplification. Preserving selection would require storing a
`Set<number>` per tab and remapping indices after re-sort on switch-to. The complexity is not worth it until there is a
concrete user need.

**Decision**: Sort is per-tab with no global per-column sort memory **Why**: Users browse different directories with
different sort needs (e.g. Downloads sorted by date, projects sorted by name). Per-tab sort avoids surprising column
changes when switching tabs.

**Decision**: Leading-edge debounce on Ctrl+Tab cycling (50ms) **Why**: Each tab switch triggers a full FilePane remount
(cold load). Rapid cycling through 5 tabs without debounce would mount/destroy 5 FilePanes in quick succession, causing
flicker and wasted IPC. The debounce fires the first press immediately (responsive feel), then batches subsequent
presses, committing only the final target.

**Decision**: Pinned tab navigation auto-creates a new tab instead of navigating in-place **Why**: The purpose of
pinning is to preserve a location. If navigating away from a pinned tab changed its path, pinning would be meaningless.
The auto-created tab inherits the target path and appears after the pinned tab. Falls back to in-place navigation only
at the tab cap (10) to avoid blocking the user.

## Unreachable tabs

When a tab's `findContainingVolume` call times out during startup restoration, the tab enters an "unreachable" state
(`TabState.unreachable: UnreachableState`). Instead of silently falling back to the default volume, the tab shows an
inline banner (`VolumeUnreachableBanner.svelte`) with the original path, a "Retry" button, and an "Open home folder"
button. The tab bar shows a small warning icon on affected tabs. This is runtime-only state (not persisted) — on next
startup, volume resolution is re-attempted.

## Context menu

Tab context menu (pin/unpin, close, close others) uses a native Tauri popup menu via `show_tab_context_menu` IPC.

**Gotcha**: Tauri 2's `Menu::popup()` returns before `on_menu_event` fires **Why**: muda queues the `MenuEvent` through
an event loop proxy. The popup's NSEvent tracking loop on macOS consumes the wakeup signal, so a synchronous
`mpsc::channel` with timeout always races and loses. Instead, `on_menu_event` emits a `tab-context-action` Tauri event,
and the frontend uses a one-shot listener (`onTabContextAction`) registered before showing the popup. Do NOT try a
synchronous channel approach -- it will always time out.

**Gotcha**: `getActiveTab` silently fixes stale `activeTabId` by falling back to the first tab **Why**: After closing
tabs or restoring persisted state, `activeTabId` may reference a tab that no longer exists. Throwing would crash the UI.
Auto-correcting to the first tab keeps the pane usable.

**Gotcha**: Tab bar close button hidden via CSS container query at `max-width: 80px` **Why**: Chrome-style shrinking
tabs can get very narrow. Showing a close button on a 40px-wide tab would leave no room for the label. The container
query hides it gracefully without JavaScript measurement. Middle-click close still works regardless of tab width.

## Persistence

Tab state persisted via `loadPaneTabs`/`savePaneTabs` in `app-status-store.ts`. Migrates from old scalar keys on first
load.

## MCP

- `tab` tool with `action`: `new` (create tab), `close` (close tab), `close_others` (close all but target + pinned), `activate` (switch to tab), `set_pinned` (pin/unpin)
- `tab_id` defaults to active tab for close/close_others/set_pinned; required for activate
- `close` on the last tab returns an error instead of closing the window
- `close` skips the pinned-tab confirmation dialog (agents know what they're doing)
- `set_pinned` is idempotent
- Tab list shown in `cmdr://state` YAML resource
- Frontend syncs tab state to backend via debounced `updatePaneTabs` IPC
